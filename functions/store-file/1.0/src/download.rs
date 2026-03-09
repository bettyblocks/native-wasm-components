use anyhow::{Context, Result};
use tracing::debug;
use wstd::http::{body::BoundedBody, Client, IntoBody, Request};
use wstd::io::AsyncRead;

const NETWORK_BUF_SIZE: usize = 64 * 1024; // 64kb

pub async fn download_to_memory(
    url: &str,
    headers: Option<&[(String, String)]>,
) -> Result<Vec<u8>> {
    debug!("Downloading from: {}", url);

    let client = Client::new();
    let request = build_request(url, headers)?;

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

pub fn make_unique_filename(filename: &str) -> String {
    let random_bytes = crate::bindings::wasi::random::random::get_random_bytes(8);
    let hex: String = random_bytes.iter().map(|b| format!("{b:02x}")).collect();

    match filename.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem}_{hex}.{ext}"),
        None => format!("{filename}_{hex}"),
    }
}

fn build_request(
    url: &str,
    headers: Option<&[(String, String)]>,
) -> Result<Request<BoundedBody<Vec<u8>>>> {
    let mut builder = Request::get(url);
    if let Some(custom_headers) = headers {
        for (key, value) in custom_headers {
            builder = builder.header(key.to_lowercase().as_str(), value.as_str());
        }
    }
    builder
        .body(Vec::<u8>::new().into_body())
        .context("Failed to build HTTP request")
}

pub fn extract_file_info_from_url(url: &str) -> Result<(String, String)> {
    let url_path = url
        .split('?')
        .next()
        .ok_or_else(|| anyhow::anyhow!("Invalid URL format"))?;

    let encoded_filename = url_path
        .split('/')
        .rfind(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Could not extract filename from URL"))?;

    let filename = urlencoding::decode(encoded_filename)
        .unwrap_or(std::borrow::Cow::Borrowed(encoded_filename))
        .to_string();

    let content_type = mime_guess::from_path(&filename)
        .first_or_octet_stream()
        .to_string();

    Ok((filename, content_type))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_file_info_from_url_simple() {
        let (filename, content_type) =
            extract_file_info_from_url("https://example.com/files/document.pdf").unwrap();
        assert_eq!(filename, "document.pdf");
        assert_eq!(content_type, "application/pdf");
    }

    #[test]
    fn test_extract_file_info_from_url_with_query() {
        let (filename, content_type) =
            extract_file_info_from_url("https://example.com/files/image.png?token=abc123").unwrap();
        assert_eq!(filename, "image.png");
        assert_eq!(content_type, "image/png");
    }

    #[test]
    fn test_extract_file_info_from_url_encoded() {
        let (filename, content_type) =
            extract_file_info_from_url("https://example.com/files/my%20file.txt").unwrap();
        assert_eq!(filename, "my file.txt");
        assert_eq!(content_type, "text/plain");
    }

    #[test]
    fn test_extract_file_info_from_url_unknown_extension() {
        let (filename, content_type) =
            extract_file_info_from_url("https://example.com/files/data.unknownext123").unwrap();
        assert_eq!(filename, "data.unknownext123");
        assert_eq!(content_type, "application/octet-stream");
    }

    #[test]
    fn test_extract_filename_from_trailing_slash_link() {
        let (filename, content_type) =
            extract_file_info_from_url("https://example.com/files/data/somedir/test.pdf/").unwrap();
        assert_eq!(filename, "test.pdf");
        assert_eq!(content_type, "application/pdf");
    }
}
