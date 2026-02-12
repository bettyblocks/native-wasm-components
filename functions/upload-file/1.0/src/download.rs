use anyhow::{Context, Result};
use tracing::debug;
use waki::{header::HeaderName, Client, RequestBuilder};

pub fn download_and_stream_to_disk(
    client: &Client,
    url: &str,
    headers: Option<&[(String, String)]>,
    file_name: &str,
) -> Result<u64> {
    debug!("Downloading from: {}", url);

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

    let file_size = crate::fs::stream_response_to_file(&response, file_name)?;

    Ok(file_size)
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
