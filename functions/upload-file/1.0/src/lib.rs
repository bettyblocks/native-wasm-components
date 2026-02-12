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
