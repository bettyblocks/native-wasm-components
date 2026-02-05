use anyhow::{Context, Result};
use serde::Deserialize;

pub mod download;
pub mod fs;
pub mod tests;
pub mod upload;

pub mod bindings {
    wit_bindgen::generate!({
        generate_all,
    });
}

use bindings::{
    betty_blocks::data_api::{data_api::HelperContext, data_api_utilities::Property},
    exports::betty_blocks::file::uploader::{Guest as UploaderGuest, Model, UploadResult},
    exports::wasi::http::incoming_handler::Guest,
    wasi::{
        http::types::{Fields, IncomingRequest, OutgoingBody, OutgoingResponse, ResponseOutparam},
        io::streams::StreamError,
    },
};

use crate::bindings::exports::betty_blocks::file::uploader::DownloadHeaders;
use crate::upload::upload_file_internal;

// Intermediate structs for JSON deserialization
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadRequestPayload {
    #[serde(alias = "application_id")]
    application_id: String,
    #[serde(alias = "action_id")]
    action_id: String,
    #[serde(alias = "log_id")]
    log_id: String,
    #[serde(alias = "encrypted_configurations")]
    encrypted_configurations: Option<Vec<String>>,
    jwt: Option<String>,
    model_name: String,
    property_name: String,
    url: String,
    headers: Option<Vec<HeaderPair>>,
}

#[derive(Debug, Deserialize)]
struct HeaderPair {
    key: String,
    value: String,
}

struct Component;

impl Guest for Component {
    fn handle(request: IncomingRequest, response_out: ResponseOutparam) {
        match handle_request(request) {
            Ok(message) => {
                send_response(response_out, 200, message.as_bytes());
            }
            Err(e) => {
                let error_msg = format!("Failed to upload file: {e}");
                send_response(response_out, 500, error_msg.as_bytes());
            }
        }
    }
}

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

fn handle_request(request: IncomingRequest) -> Result<String> {
    let body_content = read_request_body(request)?;

    let payload = parse_upload_request(&body_content)?;

    let _helper_context = HelperContext {
        application_id: payload.application_id,
        action_id: payload.action_id,
        log_id: payload.log_id,
        encrypted_configurations: payload.encrypted_configurations,
        jwt: payload.jwt,
    };
    let model = Model {
        name: payload.model_name,
    };
    let property = Property {
        name: payload.property_name,
    };

    let headers: Option<Vec<(String, String)>> = payload
        .headers
        .map(|vec| vec.into_iter().map(|h| (h.key, h.value)).collect());

    let result = upload::upload_file_internal(model, property, payload.url, headers)?;

    Ok(format!(
        "File uploaded successfully! Reference: {}, Size: {} bytes",
        result.reference, result.file_size
    ))
}

fn parse_upload_request(body_content: &str) -> Result<UploadRequestPayload> {
    let payload: UploadRequestPayload =
        serde_json::from_str(body_content).context("Request body must be valid JSON")?;
    Ok(payload)
}

fn read_request_body(request: IncomingRequest) -> Result<String> {
    let body_stream = request
        .consume()
        .map_err(|_| anyhow::anyhow!("Failed to consume request"))?;

    let input_stream = body_stream
        .stream()
        .map_err(|_| anyhow::anyhow!("Failed to get stream"))?;

    let mut body_data = Vec::new();
    loop {
        match input_stream.blocking_read(8192) {
            Ok(chunk) if chunk.is_empty() => break,
            Ok(chunk) => body_data.extend_from_slice(&chunk),
            Err(StreamError::Closed) => break,
            Err(e) => return Err(anyhow::anyhow!("Stream error: {e:?}")),
        }
    }

    String::from_utf8(body_data).context("Invalid UTF-8 in request body")
}

fn send_response(response_out: ResponseOutparam, status: u16, body: &[u8]) {
    let response = OutgoingResponse::new(Fields::new());
    response.set_status_code(status).unwrap();
    let response_body = response.body().unwrap();
    ResponseOutparam::set(response_out, Ok(response));
    let stream = response_body.write().unwrap();
    stream.blocking_write_and_flush(body).unwrap();
    drop(stream);
    OutgoingBody::finish(response_body, None).unwrap();
}

bindings::export!(Component with_types_in bindings);
