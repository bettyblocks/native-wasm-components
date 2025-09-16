use wasmcloud_component::http;

pub mod bindings {
    wit_bindgen::generate!({ generate_all });
}

use crate::bindings::betty_blocks::types::actions::{Input, Payload, call};

struct Component;

#[derive(serde::Deserialize, Debug)]
struct PayloadWrapper {
    input: String,
}
#[derive(serde::Deserialize, Debug)]
struct InputWrapper {
    action_id: String,
    payload: PayloadWrapper,
}

fn inner_handle(
    request: http::IncomingRequest,
) -> Result<http::Response<impl http::OutgoingBody>, String> {
    let body = request.body();

    body.subscribe().block();
    let body_bytes = body.read(u64::MAX).expect("Failed to read body");

    let input_wrapper = serde_json::from_slice::<InputWrapper>(&body_bytes)
        .map_err(|e| return format!("Debug: {:?}", e))?;

    let input = Input {
        action_id: input_wrapper.action_id,
        payload: Payload {
            input: input_wrapper.payload.input,
        },
    };

    let result = call(&input).map_err(|e| return format!("call_error: {:?}", e))?;

    Ok(http::Response::new(result.result))
}

impl http::Server for Component {
    fn handle(
        request: http::IncomingRequest,
    ) -> http::Result<http::Response<impl http::OutgoingBody>> {
        inner_handle(request).map_err(|e| http::ErrorCode::InternalError(Some(e)))
    }
}

http::export!(Component);
