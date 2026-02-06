use anyhow::{Context, Result};
use tracing::{debug, error};
use waki::{header::HeaderName, Client, RequestBuilder, Response};

use crate::bindings::wasi::filesystem::{
    preopens::get_directories,
    types::{DescriptorFlags, OpenFlags, PathFlags},
};

const CHUNK_SIZE: u64 = 65536; // 64kb

pub fn download_and_stream_to_disk(
    client: &Client,
    url: &str,
    headers: Option<&[(String, String)]>,
) -> Result<(u64, String, String)> {
    debug!("Downloading from: {}", url);

    let (file_name, content_type) =
        extract_file_info_from_url(url).context("Failed to extract file info from URL")?;

    let response = build_request(client, url, headers)?
        .send()
        .context("Failed to send HTTP request")?;

    let status = response.status_code();
    if !(200..300).contains(&status) {
        return Err(anyhow::anyhow!(
            "HTTP request failed with status code: {}",
            status
        ));
    }

    let file_size = stream_response_to_file(&response, &file_name)?;

    Ok((file_size, file_name, content_type))
}

fn build_request(
    client: &Client,
    url: &str,
    headers: Option<&[(String, String)]>,
) -> Result<RequestBuilder> {
    let mut request = client.get(url);
    if let Some(custom_headers) = headers {
        for (key, value) in custom_headers {
            let header_name = HeaderName::try_from(key.to_lowercase())
                .with_context(|| format!("Invalid header name: '{}'", key))?;
            request = request.header(header_name, value.as_str());
        }
    }
    Ok(request)
}

fn stream_response_to_file(response: &Response, filename: &str) -> Result<u64> {
    let preopens = get_directories();

    if preopens.is_empty() {
        error!("stream_response_to_file: No preopened directories available!");
        return Err(anyhow::anyhow!("No preopened directories available"));
    }

    let (dir, _dir_name) = &preopens[0];

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
        // stream the chunk, Question(Aditya): what should be the chunk size?
        let chunk_result = response.chunk(CHUNK_SIZE);
        match chunk_result {
            Ok(Some(chunk)) if !chunk.is_empty() => {
                chunk_count += 1;

                if let Err(e) = write_chunk_to_stream(&write_stream, &chunk) {
                    error!(
                        "stream_response_to_file: Failed to write chunk {}: {}",
                        chunk_count, e
                    );
                    let _ = dir.unlink_file_at(filename);
                    return Err(e.context(format!("Failed to write chunk {}", chunk_count)));
                }
                total_bytes += chunk.len() as u64;
            }
            Ok(Some(_)) | Ok(None) => break,
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
    drop(write_stream);
    drop(file);

    Ok(total_bytes)
}

fn write_chunk_to_stream(
    stream: &crate::bindings::wasi::io::streams::OutputStream,
    chunk: &[u8],
) -> Result<()> {
    let mut offset = 0;
    while offset < chunk.len() {
        let to_write = &chunk[offset..];
        match stream.check_write() {
            Ok(0) => {
                stream.subscribe().block();
                continue;
            }
            Ok(available) => {
                let write_size = std::cmp::min(available as usize, to_write.len());
                stream.write(&to_write[..write_size]).map_err(|e| {
                    error!("write_chunk_to_stream: Write failed: {e:?}");
                    anyhow::anyhow!("Write failed: {e:?}")
                })?;
                offset += write_size;
            }
            Err(e) => {
                error!("write_chunk_to_stream: check_write failed: {e:?}");
                return Err(anyhow::anyhow!("check_write failed: {e:?}"));
            }
        }
    }
    Ok(())
}

pub fn extract_file_info_from_url(url: &str) -> Result<(String, String)> {
    let url_path = url
        .split('?')
        .next()
        .ok_or_else(|| anyhow::anyhow!("Invalid URL format"))?;

    let encoded_filename = url_path
        .split('/')
        .filter(|s| !s.is_empty())
        .next_back()
        .ok_or_else(|| anyhow::anyhow!("Could not extract filename from URL"))?;

    let filename = urlencoding::decode(encoded_filename)
        .unwrap_or_else(|_| std::borrow::Cow::Borrowed(encoded_filename))
        .to_string();

    let content_type = mime_guess::from_path(&filename)
        .first_or_octet_stream()
        .to_string();

    Ok((filename, content_type))
}
