use anyhow::Result;
use tracing::{debug, error};
use waki::multipart::{StreamingForm, StreamingPart};
use waki::Client;

use crate::bindings::{
    betty_blocks::data_api::data_api_utilities::{self, Model, PresignedPost, Property},
    exports::betty_blocks::file::uploader::{DownloadHeaders, UploadResult},
};
use crate::fs::WasiFileReader;

pub fn upload_file_internal(
    model: Model,
    property: Property,
    download_url: String,
    download_headers: DownloadHeaders,
) -> Result<UploadResult> {
    let client = Client::new();

    let (file_size, file_name, content_type) = match crate::download::download_and_stream_to_disk(
        &client,
        &download_url,
        download_headers.as_deref(),
    ) {
        Ok(data) => data,
        Err(e) => {
            error!("upload_file_internal: Failed to download file: {:?}", e);
            return Err(e.context(format!("Failed to download file from {}", download_url)));
        }
    };

    // fetch presigned upload url
    let presigned_upload_url =
        data_api_utilities::fetch_presigned_post(&model, &property, &content_type, &file_name)
            .map_err(|e| {
                error!("upload_file_internal: Failed to fetch presigned URL: {}", e);
                anyhow::anyhow!("Failed to fetch presigned URL: {}", e)
            })?;

    // upload to s3
    if let Err(e) =
        upload_to_presigned_post(&client, &presigned_upload_url, &file_name, &content_type)
    {
        error!("upload_file_internal: Upload to S3 failed: {:?}", e);
        // Try to clean up the temporary file if upload failed
        if let Err(cleanup_err) = crate::fs::delete_from_filesystem(&file_name) {
            debug!(
                "Warning: Failed to cleanup temporary file after upload failure: {}",
                cleanup_err
            );
        }

        return Err(e.context("Failed to upload file to S3"));
    }

    // cleanup
    if let Err(e) = crate::fs::delete_from_filesystem(&file_name) {
        debug!("Warning: Failed to delete temporary file: {}", e);
    }

    Ok(UploadResult {
        reference: presigned_upload_url.reference,
        file_size,
        message: Some("Upload successful".into()),
    })
}

fn upload_to_presigned_post(
    client: &Client,
    presigned_post: &PresignedPost,
    filename: &str,
    content_type: &str,
) -> Result<()> {
    // Build streaming multipart form
    let mut form = StreamingForm::new();

    // Add policy form fields (these are small, in-memory is fine)
    for field in &presigned_post.fields {
        form = form.text(field.key.clone(), field.value.clone());
    }

    let file_reader = WasiFileReader::open(filename)?;

    let file_part = StreamingPart::from_reader("file", file_reader)
        .filename(filename)
        .mime_str(content_type)
        .map_err(|e| anyhow::anyhow!("Failed to set mime type: {e:?}"))?;

    form = form.part(file_part);

    let response = client
        .post(&presigned_post.url)
        .streaming_multipart(form)
        .send()
        .map_err(|e| anyhow::anyhow!("Failed to send upload request: {e:?}"))?;

    let status = response.status_code();
    debug!("Status: {}", status);

    if status >= 300 {
        let err = response
            .body()
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .unwrap_or_default();
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
