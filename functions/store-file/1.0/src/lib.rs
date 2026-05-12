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
    exports::betty_blocks::file::store::{UrlSource, Guest as StoreGuest, Model},
};

use crate::download::{download_to_memory, extract_file_info_from_url, make_unique_filename};

struct Component;

impl StoreGuest for Component {
    fn store_file(
        helper_context: HelperContext,
        model: Model,
        property: Property,
        source: UrlSource,
    ) -> Result<String, String> {
        wstd::runtime::block_on(store_file_internal(helper_context, model, property, source))
            .map_err(|e| e.to_string())
    }
}

async fn store_file_internal(
    helper_context: HelperContext,
    model: Model,
    property: Property,
    source: UrlSource,
) -> anyhow::Result<String> {
    let (base_name, content_type) = extract_file_info_from_url(&source.url)
        .map_err(|e| anyhow::anyhow!("Failed to extract file info from URL: {e}"))?;
    let filename = make_unique_filename(&base_name);

    let headers_as_tuple: Option<Vec<(String, String)>> = match source.headers {
        None => None,
        Some(headers) => Some(headers.into_iter().map(|header| (header.key, header.value)).collect())
    };

    let file_bytes = download_to_memory(&source.url, headers_as_tuple.as_deref()).await?;

    let upload_result =
        upload_file::upload(&helper_context, &model, &property, &file_bytes, &filename, &content_type)
            .map_err(|e| anyhow::anyhow!("Upload failed: {e}"))?;

    Ok(upload_result.reference)
}

bindings::export!(Component with_types_in bindings);
