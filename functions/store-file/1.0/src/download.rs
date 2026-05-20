use anyhow::{Context, Result};
use tracing::debug;
use wstd::http::{Client, IntoBody, Request, body::BoundedBody};
use wstd::io::AsyncRead;

const NETWORK_BUF_SIZE: usize = 64 * 1024; // 64kb

pub async fn download_to_memory(
    url: &str,
) -> Result<Vec<u8>> {
    debug!("Downloading from: {}", url);

    let client = Client::new();
    let request = build_request(url)?;

    let response = client
        .send(request)
        .await
        .context("Failed to send HTTP request")?;

    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(anyhow::anyhow!(
            "HTTP request failed with status code: {}",
            status
        ));
    }

    let mut body = response.into_body();
    let mut file_bytes = Vec::new();
    let mut buf = [0u8; NETWORK_BUF_SIZE];

    loop {
        let n = body
            .read(&mut buf)
            .await
            .context("Failed to read response chunk")?;
        if n == 0 {
            break;
        }
        file_bytes.extend_from_slice(&buf[..n]);
    }

    debug!("Downloaded {} bytes", file_bytes.len());
    Ok(file_bytes)
}

fn build_request(
    url: &str,
) -> Result<Request<BoundedBody<Vec<u8>>>> {
    let builder = Request::get(url);
    builder
        .body(Vec::new().into_body())
        .context("Failed to build HTTP request")
}

pub fn extract_file_info_from_url(raw_url: &str) -> Result<String> {
    let parsed = url::Url::parse(raw_url).context("Invalid URL")?;

    let filename = parsed
        .path_segments()
        .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
        .ok_or_else(|| anyhow::anyhow!("Could not extract filename from URL"))?;

    let filename = percent_encoding::percent_decode_str(filename)
        .decode_utf8_lossy()
        .to_string();

    Ok(filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_file_info_from_url_simple() {
        let filename =
            extract_file_info_from_url("https://example.com/files/document.pdf").unwrap();
        assert_eq!(filename, "document.pdf");
    }

    #[test]
    fn test_extract_file_info_from_url_with_query() {
        let filename =
            extract_file_info_from_url("https://example.com/files/image.png?token=abc123").unwrap();
        assert_eq!(filename, "image.png");
    }

    #[test]
    fn test_extract_file_info_from_url_encoded() {
        let filename =
            extract_file_info_from_url("https://example.com/files/my%20file.txt").unwrap();
        assert_eq!(filename, "my file.txt");
    }

    #[test]
    fn test_extract_file_info_from_url_unknown_extension() {
        let filename =
            extract_file_info_from_url("https://example.com/files/data.unknownext123").unwrap();
        assert_eq!(filename, "data.unknownext123");
    }

    #[test]
    fn test_extract_filename_from_trailing_slash_link() {
        let filename =
            extract_file_info_from_url("https://example.com/files/data/somedir/test.pdf/").unwrap();
        assert_eq!(filename, "test.pdf");
    }
}
