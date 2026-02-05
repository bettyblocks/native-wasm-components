#[cfg(test)]
mod tests {
    use crate::download::extract_file_info_from_url;

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
        let (filename, _content_type) =
            extract_file_info_from_url("https://example.com/files/my%20file.txt").unwrap();
        assert_eq!(filename, "my file.txt");
    }

    #[test]
    fn test_extract_file_info_from_url_unknown_extension() {
        let (filename, content_type) =
            extract_file_info_from_url("https://example.com/files/data.unknownext123").unwrap();
        assert_eq!(filename, "data.unknownext123");
        assert_eq!(content_type, "application/octet-stream");
    }
}
