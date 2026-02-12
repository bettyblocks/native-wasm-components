use anyhow::Result;
use std::io::Read;
use tracing::{debug, error};
use waki::Response;

use crate::bindings::wasi::{
    filesystem::{
        preopens::get_directories,
        types::{Descriptor, DescriptorFlags, OpenFlags, PathFlags},
    },
    io::streams::{InputStream, StreamError},
};

const CHUNK_SIZE: usize = 4 * 1024; // 4kb
const RES_CHUNK_SIZE: u64 = 65536; // 64kb

/// A reader that streams from a WASI filesystem file.
///
/// implements std::io::Read so it can be used with our waki fork's streaming body API.
/// This allows streaming file uploads without loading the entire file into memory.
pub struct WasiFileReader {
    stream: InputStream,
    #[allow(dead_code)]
    file: Descriptor,
}

impl WasiFileReader {
    /// Open a file for streaming read.
    /// returns a reader that can be passed to waki fork's streaming_body() or StreamingPart::from_reader()
    pub fn open(filename: &str) -> Result<Self> {
        let preopens = get_directories();
        if preopens.is_empty() {
            return Err(anyhow::anyhow!("No preopened directories available"));
        }

        let (dir, _) = &preopens.first().unwrap();

        let file = dir
            .open_at(
                PathFlags::empty(),
                filename,
                OpenFlags::empty(),
                DescriptorFlags::READ,
            )
            .map_err(|e| anyhow::anyhow!("Failed to open file for reading: {e:?}"))?;

        let stream = file
            .read_via_stream(0)
            .map_err(|e| anyhow::anyhow!("Failed to get read stream: {e:?}"))?;

        Ok(Self { stream, file })
    }
}

impl Read for WasiFileReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        match self.stream.blocking_read(buf.len() as u64) {
            Ok(chunk) if chunk.is_empty() => Ok(0),
            Ok(chunk) => {
                let len = chunk.len();
                buf[..len].copy_from_slice(&chunk);
                Ok(len)
            }
            Err(StreamError::Closed) => Ok(0),
            Err(e) => Err(std::io::Error::other(format!(
                "WASI stream read error: {e:?}"
            ))),
        }
    }
}

pub fn delete_from_filesystem(filename: &str) -> Result<()> {
    let preopens = get_directories();
    if preopens.is_empty() {
        return Err(anyhow::anyhow!("No preopened directories available"));
    }

    let (dir, _) = &preopens.first().unwrap();

    dir.unlink_file_at(filename)
        .map_err(|e| anyhow::anyhow!("Failed to delete file: {e:?}"))?;

    debug!("Deleted temporary file: {}", filename);

    Ok(())
}

pub fn stream_response_to_file(response: &Response, filename: &str) -> Result<u64> {
    let preopens = get_directories();

    if preopens.is_empty() {
        error!("stream_response_to_file: No preopened directories available!");
        return Err(anyhow::anyhow!("No preopened directories available"));
    }

    let (dir, _) = &preopens.first().unwrap();

    let file = dir
        .open_at(
            PathFlags::empty(),
            filename,
            OpenFlags::CREATE | OpenFlags::TRUNCATE,
            DescriptorFlags::WRITE,
        )
        .map_err(|e| {
            error!("stream_response_to_file: Failed to open file for writing: {e:?}");
            anyhow::anyhow!("Failed to open file for writing: {e:?}")
        })?;

    let write_stream = file.write_via_stream(0).map_err(|e| {
        error!("stream_response_to_file: Failed to get write stream: {e:?}");
        anyhow::anyhow!("Failed to get write stream: {e:?}")
    })?;

    let mut total_bytes: u64 = 0;
    let mut chunk_count: u64 = 0;

    loop {
        let chunk_result = response.chunk(RES_CHUNK_SIZE);
        match chunk_result {
            Ok(Some(chunk)) if !chunk.is_empty() => {
                chunk_count += 1;

                if let Err(e) = crate::fs::write_stream_in_chunks(&write_stream, &chunk) {
                    error!(
                        "stream_response_to_file: Failed to write chunk {}: {}",
                        chunk_count, e
                    );
                    let _ = dir.unlink_file_at(filename);
                    return Err(anyhow::anyhow!("Failed to write chunk {}", chunk_count));
                }
                total_bytes += chunk.len() as u64;
            }
            Ok(Some(_)) | Ok(None) => {
                break;
            }
            Err(e) => {
                error!("stream_response_to_file: Failed to read response chunk: {e:?}");
                let _ = dir.unlink_file_at(filename);
                return Err(anyhow::anyhow!("Failed to read response chunk: {e:?}"));
            }
        }
    }

    write_stream.flush().map_err(|e| {
        error!("stream_response_to_file: Failed to flush write stream: {e:?}");
        anyhow::anyhow!("Failed to flush write stream: {e:?}")
    })?;

    Ok(total_bytes)
}

pub fn write_stream_in_chunks(
    stream: &crate::bindings::wasi::io::streams::OutputStream,
    data: &[u8],
) -> Result<()> {
    for chunk in data.chunks(CHUNK_SIZE) {
        stream
            .blocking_write_and_flush(chunk)
            .map_err(|e| anyhow::anyhow!("Stream write error: {e:?}"))?;
    }
    Ok(())
}
