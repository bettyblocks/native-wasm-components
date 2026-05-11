pub mod download;

pub mod bindings {
    wit_bindgen::generate!({
        generate_all,
    });
}

use bindings::{
    betty_blocks::data_api::data_api::HelperContext,
    betty_blocks::file::upload_file,
    betty_blocks::types::types::Property,
    exports::betty_blocks::file::store::{Base64Source, Guest as StoreGuest, Model},
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

use crate::download::{make_unique_filename};

struct Component;

impl StoreGuest for Component {
    fn store_file(
        helper_context: HelperContext,
        model: Model,
        property: Property,
        base64_source: Base64Source,
    ) -> Result<String, String> {
        wstd::runtime::block_on(store_file_internal(helper_context, model, property, base64_source))
            .map_err(|e| e.to_string())
    }
}

async fn store_file_internal(
    helper_context: HelperContext,
    model: Model,
    property: Property,
    base64_source: Base64Source,
) -> anyhow::Result<String> {
    let file_bytes = BASE64
        .decode(&base64_source.data)
        .map_err(|e| anyhow::anyhow!("Failed to decode base64 source: {e}"))?;

    let content_type = mime_guess::from_path(&base64_source.filename)
        .first_or_octet_stream()
        .to_string();
    let filename = make_unique_filename(&base64_source.filename);

    let upload_result =
        upload_file::upload(&helper_context, &model, &property, &file_bytes, &filename, &content_type)
            .map_err(|e| anyhow::anyhow!("Upload failed: {e}"))?;

    Ok(upload_result.reference)
}

bindings::export!(Component with_types_in bindings);
