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
    exports::betty_blocks::file::store::{Guest as StoreGuest, Model},
};

use crate::download::{download_to_memory, extract_file_info_from_url, make_unique_filename};

struct Component;

impl StoreGuest for Component {
    fn store_file(
        helper_context: HelperContext,
        model: Model,
        property: Vec<Property>,
        url: String,
    ) -> Result<String, String> {
        wstd::runtime::block_on(store_file_internal(helper_context, model, property, url))
            .map_err(|e| e.to_string())
    }
}

async fn store_file_internal(
    helper_context: HelperContext,
    model: Model,
    property: Vec<Property>,
    url: String,
) -> anyhow::Result<String> {
    let property = property
        .first()
        .ok_or(anyhow::anyhow!("Failed to fetch file property"))?;

    let (base_name, content_type) = extract_file_info_from_url(&url)
        .map_err(|e| anyhow::anyhow!("Failed to extract file info from URL: {e}"))?;
    let filename = make_unique_filename(&base_name);

    let file_bytes = download_to_memory(&url).await?;

    let upload_result = upload_file::upload(
        &helper_context,
        &model,
        property,
        &file_bytes,
        &filename,
        &content_type,
    )
    .map_err(|e| anyhow::anyhow!("Upload failed: {e}"))?;

    Ok(upload_result.reference)
}

bindings::export!(Component with_types_in bindings);
