use anyhow::{Context, Result};
use multipart::client::lazy::Multipart;
use std::io::Read;
use tracing::{debug, error};
use wstd::{
    http::{body::BodyForthcoming, Client, Request},
    io::AsyncWrite,
};

use crate::bindings::{
    betty_blocks::data_api::data_api_utilities::{self, Model, PresignedPost, Property},
    exports::betty_blocks::file::uploader::{DownloadHeaders, UploadResult},
};
use crate::fs::{WasiFileReader, NETWORK_BUF_SIZE};

pub async fn upload_file_internal(
    model: Model,
    property: Property,
    download_url: String,
    download_headers: DownloadHeaders,
) -> Result<UploadResult> {
    let client = Client::new();

    let (base_name, content_type) = crate::download::extract_file_info_from_url(&download_url)
        .context("Failed to extract file info from URL")?;
    let file_name = crate::download::make_unique_filename(&base_name);

    let file_size = match crate::download::download_and_stream_to_disk(
        &client,
        &download_url,
        download_headers.as_deref(),
        &file_name,
    )
    .await
    {
        Ok(size) => size,
        Err(e) => {
            error!("upload_file_internal: Failed to download file: {:?}", e);
            if let Err(cleanup_err) = crate::fs::delete_from_filesystem(&file_name) {
                debug!(
                    "Warning: Failed to cleanup temporary file after download failure: {}",
                    cleanup_err
                );
            }
            return Err(e.context(format!("Failed to download file from {}", download_url)));
        }
    };

    let presigned_upload_url =
        data_api_utilities::fetch_presigned_post(&model, &property, &content_type, &file_name)
            .map_err(|e| {
                error!("upload_file_internal: Failed to fetch presigned URL: {}", e);
                anyhow::anyhow!("Failed to fetch presigned URL: {}", e)
            })?;

    if let Err(e) =
        upload_to_presigned_post(&client, &presigned_upload_url, &file_name, &content_type).await
    {
        error!("upload_file_internal: Upload to Storage failed: {:?}", e);
        if let Err(cleanup_err) = crate::fs::delete_from_filesystem(&file_name) {
            debug!(
                "Warning: Failed to cleanup temporary file after upload failure: {}",
                cleanup_err
            );
        }

        return Err(e.context("Failed to upload file to Storage"));
    }

    if let Err(e) = crate::fs::delete_from_filesystem(&file_name) {
        debug!("Warning: Failed to delete temporary file: {}", e);
    }

    Ok(UploadResult {
        reference: presigned_upload_url.reference,
        file_size,
        message: Some("Upload successful".into()),
    })
}

async fn upload_to_presigned_post(
    client: &Client,
    presigned_post: &PresignedPost,
    filename: &str,
    content_type: &str,
) -> Result<()> {
    let mut form = Multipart::new();

    for field in &presigned_post.fields {
        form.add_text(field.key.clone(), field.value.clone());
    }

    let file_reader = WasiFileReader::open(filename)?;

    let mime: mime::Mime = content_type
        .parse()
        .unwrap_or(mime::APPLICATION_OCTET_STREAM);

    form.add_stream("file", file_reader, Some(filename), Some(mime));

    let mut prepared = form
        .prepare()
        .map_err(|e| anyhow::anyhow!("Failed to prepare multipart form: {e}"))?;

    let content_type_header = format!("multipart/form-data; boundary={}", prepared.boundary());

    let request = Request::post(&presigned_post.url)
        .header("content-type", &*content_type_header)
        .body(BodyForthcoming)
        .map_err(|e| anyhow::anyhow!("Failed to build upload request: {e}"))?;

    let (mut outgoing_body, response_future) = client
        .start_request(request)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to start upload request: {e}"))?;

    let mut buf = [0u8; NETWORK_BUF_SIZE];
    loop {
        let n = prepared
            .read(&mut buf)
            .map_err(|e| anyhow::anyhow!("Failed to read multipart body: {e}"))?;
        if n == 0 {
            break;
        }
        outgoing_body
            .write_all(&buf[..n])
            .await
            .map_err(|e| anyhow::anyhow!("Failed to write to outgoing body: {e}"))?;
    }

    Client::finish(outgoing_body, None)
        .map_err(|e| anyhow::anyhow!("Failed to finish outgoing body: {e}"))?;

    let response = response_future
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get upload response: {e}"))?;

    let status = response.status().as_u16();
    debug!("Status: {}", status);

    if status >= 300 {
        let mut err_body = response.into_body();
        let err = match err_body.bytes().await {
            Ok(b) => String::from_utf8_lossy(&b).to_string(),
            Err(_) => String::new(),
        };
        debug!("Error body: {}", err);
        return Err(anyhow::anyhow!(
            "upload failed with status {}: {}",
            status,
            err
        ));
    }

    debug!("Presigned POST upload succeeded (streamed from disk)");
    Ok(())
}
