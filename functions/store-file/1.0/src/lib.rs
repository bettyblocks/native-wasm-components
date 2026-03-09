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
    exports::betty_blocks::file::store::{FileSource, Guest as StoreGuest, Model, StoreFileResult},
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

use crate::download::{download_to_memory, extract_file_info_from_url, make_unique_filename};

struct Component;

impl StoreGuest for Component {
    fn store_file(
        _helper_context: HelperContext,
        model: Model,
        property: Property,
        source: FileSource,
    ) -> Result<StoreFileResult, String> {
        wstd::runtime::block_on(store_file_internal(model, property, source))
            .map_err(|e| e.to_string())
    }
}

async fn store_file_internal(
    model: Model,
    property: Property,
    source: FileSource,
) -> anyhow::Result<StoreFileResult> {
    let (file_bytes, filename, content_type) = match source {
        FileSource::Base64(base64_src) => {
            let file_bytes = BASE64
                .decode(&base64_src.data)
                .map_err(|e| anyhow::anyhow!("Failed to decode base64 source: {e}"))?;

            let content_type = mime_guess::from_path(&base64_src.filename)
                .first_or_octet_stream()
                .to_string();
            let filename = make_unique_filename(&base64_src.filename);
            (file_bytes, filename, content_type)
        }
        FileSource::Url(url_src) => {
            let (base_name, content_type) = extract_file_info_from_url(&url_src.url)
                .map_err(|e| anyhow::anyhow!("Failed to extract file info from URL: {e}"))?;
            let filename = make_unique_filename(&base_name);
            let file_bytes = download_to_memory(&url_src.url, url_src.headers.as_deref()).await?;
            (file_bytes, filename, content_type)
        }
    };

    let upload_result =
        upload_file::upload(&model, &property, &file_bytes, &filename, &content_type)
            .map_err(|e| anyhow::anyhow!("Upload failed: {e}"))?;

    Ok(StoreFileResult {
        reference: upload_result.reference,
        file_size: upload_result.file_size,
        message: upload_result.message,
    })
}

bindings::export!(Component with_types_in bindings);
