use anyhow::Result;
use std::io::Read;
use tracing::{debug, error};
use wstd::{http::body::IncomingBody, io::AsyncRead};

use crate::bindings::wasi::{
    filesystem::{
        preopens::get_directories,
        types::{Descriptor, DescriptorFlags, OpenFlags, PathFlags},
    },
    io::streams::{InputStream, StreamError},
};

const WASI_WRITE_CHUNK: usize = 4 * 1024; // 4kb - max per WASI blocking_write_and_flush call (that's a wasi limitation)
pub const NETWORK_BUF_SIZE: usize = 64 * 1024; // 64kb - buffer for network I/O

/// A reader that streams from a WASI filesystem file.
///
/// Implements std::io::Read so it can be used with the multipart crate's streaming API.
/// This will allow streaming file uploads without loading the entire file into memory.
pub struct WasiFileReader {
    stream: InputStream,
    #[allow(dead_code)]
    file: Descriptor,
}

impl WasiFileReader {
    /// Open a file for streaming read.
    /// Returns a reader that can be passed to multipart's add_stream()
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

pub async fn stream_incoming_to_file(body: &mut IncomingBody, filename: &str) -> Result<u64> {
    let preopens = get_directories();

    if preopens.is_empty() {
        error!("stream_incoming_to_file: No preopened directories available!");
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
            error!("stream_incoming_to_file: Failed to open file for writing: {e:?}");
            anyhow::anyhow!("Failed to open file for writing: {e:?}")
        })?;

    let write_stream = file.write_via_stream(0).map_err(|e| {
        error!("stream_incoming_to_file: Failed to get write stream: {e:?}");
        anyhow::anyhow!("Failed to get write stream: {e:?}")
    })?;

    let mut total_bytes: u64 = 0;
    let mut buf = [0u8; NETWORK_BUF_SIZE];

    loop {
        let n = body.read(&mut buf).await.map_err(|e| {
            error!("stream_incoming_to_file: Failed to read response chunk: {e}");
            let _ = dir.unlink_file_at(filename);
            anyhow::anyhow!("Failed to read response chunk: {e}")
        })?;

        if n == 0 {
            break;
        }

        if let Err(e) = write_stream_in_chunks(&write_stream, &buf[..n]) {
            error!("stream_incoming_to_file: Failed to write chunk: {}", e);
            let _ = dir.unlink_file_at(filename);
            return Err(anyhow::anyhow!("Failed to write chunk to file"));
        }

        total_bytes += n as u64;
    }

    write_stream.flush().map_err(|e| {
        error!("stream_incoming_to_file: Failed to flush write stream: {e:?}");
        anyhow::anyhow!("Failed to flush write stream: {e:?}")
    })?;

    Ok(total_bytes)
}

pub fn write_stream_in_chunks(
    stream: &crate::bindings::wasi::io::streams::OutputStream,
    data: &[u8],
) -> Result<()> {
    for chunk in data.chunks(WASI_WRITE_CHUNK) {
        stream
            .blocking_write_and_flush(chunk)
            .map_err(|e| anyhow::anyhow!("Stream write error: {e:?}"))?;
    }
    Ok(())
}
