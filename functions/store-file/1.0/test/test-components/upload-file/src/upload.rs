use anyhow::Result;
use multipart::client::lazy::Multipart;
use std::io::Read;
use tracing::{debug, error};
use wstd::http::{Body, Client, Request};

use crate::bindings::{
    betty_blocks::data_api::data_api_utilities::{self, Model, PresignedPost, Property},
    exports::betty_blocks::file::upload_file::UploadResult,
};

pub async fn upload_bytes_internal(
    model: Model,
    property: Property,
    file_bytes: Vec<u8>,
    filename: String,
    content_type: String,
) -> Result<UploadResult> {
    let file_size = file_bytes.len() as u64;

    let presigned_post =
        data_api_utilities::fetch_presigned_post(&model, &property, &content_type, &filename)
            .map_err(|e| {
                error!(
                    "upload_bytes_internal: Failed to fetch presigned URL: {}",
                    e
                );
                anyhow::anyhow!("Failed to fetch presigned URL: {}", e)
            })?;

    upload_to_presigned_post(&presigned_post, file_bytes, &filename, &content_type).await?;

    Ok(UploadResult {
        reference: presigned_post.reference,
        file_size,
        message: Some("Upload successful".into()),
    })
}

async fn upload_to_presigned_post(
    presigned_post: &PresignedPost,
    file_bytes: Vec<u8>,
    filename: &str,
    content_type: &str,
) -> Result<()> {
    let client = Client::new();
    let mut form = Multipart::new();

    for field in &presigned_post.fields {
        form.add_text(field.key.clone(), field.value.clone());
    }

    let mime: mime::Mime = content_type
        .parse()
        .unwrap_or(mime::APPLICATION_OCTET_STREAM);

    form.add_stream("file", file_bytes.as_slice(), Some(filename), Some(mime));

    let mut prepared = form
        .prepare()
        .map_err(|e| anyhow::anyhow!("Failed to prepare multipart form: {e}"))?;

    let content_type_header = format!("multipart/form-data; boundary={}", prepared.boundary());

    let mut body_bytes = Vec::new();
    prepared
        .read_to_end(&mut body_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to read multipart body: {e}"))?;

    let request = Request::post(&presigned_post.url)
        .header("content-type", &*content_type_header)
        .body(Body::from(body_bytes))
        .map_err(|e| anyhow::anyhow!("Failed to build upload request: {e}"))?;

    let response = client
        .send(request)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to send upload request: {e}"))?;

    let status = response.status().as_u16();
    debug!("Status: {}", status);

    if status >= 300 {
        let mut err_body = response.into_body();
        let err = match err_body.contents().await {
            Ok(b) => String::from_utf8_lossy(b).to_string(),
            Err(_) => String::new(),
        };
        debug!("Error body: {}", err);
        return Err(anyhow::anyhow!(
            "upload failed with status {}: {}",
            status,
            err
        ));
    }

    debug!("Presigned POST upload succeeded");
    Ok(())
}
