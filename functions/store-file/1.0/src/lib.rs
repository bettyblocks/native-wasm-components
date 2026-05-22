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

use crate::download::{download_to_memory, extract_file_info_from_url};

struct Component;

impl StoreGuest for Component {
    fn store_file(
        helper_context: HelperContext,
        model: Model,
        property: Vec<Property>,
        url: String,
        file_extension: Option<String>,
    ) -> Result<String, String> {
        wstd::runtime::block_on(store_file_internal(helper_context, model, property, url, file_extension))
            .map_err(|error| error.to_string())
    }
}

async fn store_file_internal(
    helper_context: HelperContext,
    model: Model,
    property: Vec<Property>,
    url: String,
    file_extension: Option<String>,
) -> anyhow::Result<String> {
    let property = property
        .first()
        .ok_or(anyhow::anyhow!("Failed to fetch file property"))?;

    let filename = extract_file_info_from_url(&url)
        .map_err(|error| anyhow::anyhow!("Failed to extract file info from URL: {error}"))?;

    let full_filename = if filename.contains('.') {
        Ok(filename)
    } else {
        match file_extension {
            Some(file_extension) if file_extension.starts_with('.') => Ok(format!("{filename}{file_extension}")),
            Some(file_extension) => {
                let file_extension = file_extension.to_lowercase();
                Ok(format!("{filename}.{file_extension}"))
            },
            None => Err(anyhow::anyhow!(format!("No file extension found and no file extension set for {}", url)))
        }
    }?;

    let file_bytes = download_to_memory(&url).await?;

    let upload_result = upload_file::upload(
        &helper_context,
        &model,
        property,
        &file_bytes,
        &full_filename,
    )
    .map_err(|error| anyhow::anyhow!("Upload failed: {error}"))?;

    Ok(upload_result.reference)
}

bindings::export!(Component with_types_in bindings);
