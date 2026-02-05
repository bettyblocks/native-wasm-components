use anyhow::Result;
use std::thread;
use std::time::Duration;
use tracing::debug;

use crate::bindings::wasi::{
    filesystem::{
        preopens::get_directories,
        types::{DescriptorFlags, OpenFlags, PathFlags},
    },
    io::streams::StreamError,
};

const MAX_READ_RETRIES: u32 = 3;
const RETRY_DELAY_MS: u64 = 100;

pub fn read_from_filesystem(filename: &str) -> Result<Vec<u8>> {
    let preopens = get_directories();
    if preopens.is_empty() {
        return Err(anyhow::anyhow!("No preopened directories available"));
    }

    let (dir, _) = &preopens[0];

    // Open file with READ permission
    let file = dir
        .open_at(
            PathFlags::empty(),
            filename,
            OpenFlags::empty(),
            DescriptorFlags::READ,
        )
        .map_err(|e| anyhow::anyhow!("Failed to open file for reading: {e:?}"))?;

    // Get file size
    let stat = file
        .stat()
        .map_err(|e| anyhow::anyhow!("Failed to get file stats: {e:?}"))?;

    let file_size = stat.size;
    debug!("Reading file of size: {} bytes", file_size);

    // Read from position 0
    let stream = file
        .read_via_stream(0)
        .map_err(|e| anyhow::anyhow!("Failed to get read stream: {e:?}"))?;

    let mut data = Vec::with_capacity(file_size as usize);
    loop {
        match stream.blocking_read(8192) {
            Ok(chunk) if chunk.is_empty() => break,
            Ok(chunk) => data.extend_from_slice(&chunk),
            Err(StreamError::Closed) => break,
            Err(e) => return Err(anyhow::anyhow!("Stream error while reading file: {e:?}")),
        }
    }

    drop(stream);
    drop(file);

    Ok(data)
}

// Code is referred mostly from wasmtime p2-fs test components.
pub fn save_to_filesystem(filename: &str, data: &[u8]) -> Result<()> {
    let preopens = get_directories();
    if preopens.is_empty() {
        return Err(anyhow::anyhow!("No preopened directories available"));
    }

    let (dir, _) = &preopens[0];

    // Open file with CREATE flag and READ|WRITE permissions
    let file = dir
        .open_at(
            PathFlags::empty(),
            filename,
            OpenFlags::CREATE,
            DescriptorFlags::READ | DescriptorFlags::WRITE,
        )
        .map_err(|e| anyhow::anyhow!("Failed to open file for writing: {e:?}"))?;

    // Write from position 0
    let stream = file
        .write_via_stream(0)
        .map_err(|e| anyhow::anyhow!("Failed to get write stream: {e:?}"))?;

    write_stream_in_chunks(&stream, data)?;

    drop(stream);
    drop(file);

    debug!("Saved {} bytes to {}", data.len(), filename);

    Ok(())
}

pub fn delete_from_filesystem(filename: &str) -> Result<()> {
    let preopens = get_directories();
    if preopens.is_empty() {
        return Err(anyhow::anyhow!("No preopened directories available"));
    }

    let (dir, _) = &preopens[0];

    dir.unlink_file_at(filename)
        .map_err(|e| anyhow::anyhow!("Failed to delete file: {e:?}"))?;

    debug!("Deleted temporary file: {}", filename);

    Ok(())
}

pub fn write_stream_in_chunks(
    stream: &crate::bindings::wasi::io::streams::OutputStream,
    data: &[u8],
) -> Result<()> {
    for chunk in data.chunks(4096) {
        stream
            .blocking_write_and_flush(chunk)
            .map_err(|e| anyhow::anyhow!("Stream write error: {e:?}"))?;
    }
    Ok(())
}

pub fn read_with_retry(file_name: &str) -> Result<Vec<u8>> {
    let mut last_error = None;

    for attempt in 1..=MAX_READ_RETRIES {
        match crate::fs::read_from_filesystem(file_name) {
            Ok(data) => {
                if attempt > 1 {
                    debug!("Successfully read file on attempt {}", attempt);
                }
                return Ok(data);
            }
            Err(e) => {
                debug!("Read attempt {} failed: {}", attempt, e);
                last_error = Some(e);

                if attempt < MAX_READ_RETRIES {
                    thread::sleep(Duration::from_millis(RETRY_DELAY_MS * attempt as u64));
                }
            }
        }
    }

    // if all retry are exhausted then cleanup and fail
    let _ = crate::fs::delete_from_filesystem(file_name);
    Err(last_error.unwrap().context(format!(
        "Failed to read file '{}' after {} attempts",
        file_name, MAX_READ_RETRIES
    )))
}
