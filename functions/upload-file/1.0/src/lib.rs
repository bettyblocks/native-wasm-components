use anyhow::Result;
pub mod download;
pub mod fs;
pub mod upload;

pub mod bindings {
    wit_bindgen::generate!({
        generate_all,
    });
}

use bindings::{
    betty_blocks::data_api::{data_api::HelperContext, data_api_utilities::Property},
    exports::betty_blocks::file::uploader::{Guest as UploaderGuest, Model, UploadResult},
};

use crate::bindings::exports::betty_blocks::file::uploader::DownloadHeaders;
use crate::upload::upload_file_internal;

struct Component;

impl UploaderGuest for Component {
    fn upload(
        _helper_context: HelperContext,
        model: Model,
        property: Property,
        download_url: String,
        download_headers: DownloadHeaders,
    ) -> Result<UploadResult, String> {
        upload_file_internal(model, property, download_url, download_headers)
            .map_err(|e| e.to_string())
    }
}

bindings::export!(Component with_types_in bindings);

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
