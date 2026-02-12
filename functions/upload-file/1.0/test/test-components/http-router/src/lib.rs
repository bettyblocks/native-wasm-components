pub mod bindings {
    wit_bindgen::generate!({
        generate_all,
    });
}

use bindings::betty_blocks::file::uploader;
use bindings::exports::wasi::http::incoming_handler::Guest;
use bindings::wasi::http::types::{
    Fields, IncomingBody, IncomingRequest, OutgoingBody, OutgoingResponse, ResponseOutparam,
};
use bindings::wasi::io::streams::StreamError;

struct Component;

impl Guest for Component {
    fn handle(request: IncomingRequest, response_out: ResponseOutparam) {
        let (status, body) = match handle_upload_request(request) {
            Ok(body) => (200u16, body),
            Err(e) => (
                500u16,
                format!(r#"{{"error":"{}"}}"#, e.replace('"', "\\\"")),
            ),
        };
        send_response(response_out, status, &body);
    }
}

fn read_body(incoming_body: IncomingBody) -> Result<Vec<u8>, String> {
    let stream = incoming_body
        .stream()
        .map_err(|_| "Failed to get body stream")?;
    let mut buf = Vec::new();
    loop {
        match stream.blocking_read(4096) {
            Ok(chunk) => {
                buf.extend_from_slice(&chunk);
            }
            Err(StreamError::Closed) => break,
            Err(StreamError::LastOperationFailed(e)) => {
                return Err(format!("Stream read error: {}", e.to_debug_string()));
            }
        }
    }
    drop(stream);
    let _trailers = IncomingBody::finish(incoming_body);
    Ok(buf)
}

fn handle_upload_request(request: IncomingRequest) -> Result<String, String> {
    let body = request
        .consume()
        .map_err(|_| "Failed to consume request body")?;
    let buf = read_body(body)?;
    let body_str = String::from_utf8(buf).map_err(|e| format!("Invalid UTF-8 body: {}", e))?;

    let json: serde_json::Value =
        serde_json::from_str(&body_str).map_err(|e| format!("Invalid JSON: {}", e))?;

    let application_id = json["applicationId"]
        .as_str()
        .ok_or("Missing applicationId")?
        .to_string();
    let action_id = json["actionId"]
        .as_str()
        .ok_or("Missing actionId")?
        .to_string();
    let log_id = json["logId"].as_str().ok_or("Missing logId")?.to_string();
    let model_name = json["modelName"]
        .as_str()
        .ok_or("Missing modelName")?
        .to_string();
    let property_name = json["propertyName"]
        .as_str()
        .ok_or("Missing propertyName")?
        .to_string();
    let download_url = json["url"].as_str().ok_or("Missing url")?.to_string();

    let helper_context = bindings::betty_blocks::data_api::data_api::HelperContext {
        application_id,
        action_id,
        log_id,
        encrypted_configurations: None,
        jwt: None,
    };
    let model = bindings::betty_blocks::types::types::Model { name: model_name };
    let property = bindings::betty_blocks::types::types::Property {
        name: property_name,
    };

    let download_headers: Option<Vec<(String, String)>> = None;
    let result = uploader::upload(
        &helper_context,
        &model,
        &property,
        &download_url,
        &download_headers,
    )?;

    let message = result.message.unwrap_or_default();
    let response_json = format!(
        r#"{{"Reference":"{}","file_size":{},"message":"uploaded successfully: {}"}}"#,
        result.reference, result.file_size, message
    );
    Ok(response_json)
}

fn send_response(response_out: ResponseOutparam, status: u16, body_str: &str) {
    let headers =
        Fields::from_list(&[("content-type".to_string(), b"application/json".to_vec())]).unwrap();
    let response = OutgoingResponse::new(headers);
    response.set_status_code(status).unwrap();
    let body = response.body().unwrap();
    ResponseOutparam::set(response_out, Ok(response));

    let stream = body.write().unwrap();
    stream
        .blocking_write_and_flush(body_str.as_bytes())
        .unwrap();
    drop(stream);
    OutgoingBody::finish(body, None).unwrap();
}

bindings::export!(Component with_types_in bindings);
