pub mod upload;

pub mod bindings {
    wit_bindgen::generate!({
        generate_all,
    });
}

use bindings::{
    betty_blocks::data_api::data_api_utilities::Property,
    exports::betty_blocks::file::upload_file::{Guest as UploadFileGuest, Model, UploadResult},
};

use crate::upload::upload_bytes_internal;

struct Component;

impl UploadFileGuest for Component {
    fn upload(
        model: Model,
        property: Property,
        file_bytes: Vec<u8>,
        filename: String,
        content_type: String,
    ) -> Result<UploadResult, String> {
        wstd::runtime::block_on(upload_bytes_internal(
            model,
            property,
            file_bytes,
            filename,
            content_type,
        ))
        .map_err(|e| e.to_string())
    }
}

bindings::export!(Component with_types_in bindings);
