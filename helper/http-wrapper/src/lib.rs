use wasmcloud_component::http;

pub mod bindings {
    wit_bindgen::generate!({ generate_all });
}

use crate::bindings::betty_blocks::types::actions::{Input, Payload, call};

// 2**24 = 16mb
const MAX_READ: u64 = 2u64.pow(24);

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

enum Error {
    InvalidInput(String),
    FailedToReadBody(String),
    ActionCallFailed(String),
}

impl Into<http::Response<String>> for Error {
    fn into(self) -> http::Response<String> {
        match self {
            Error::InvalidInput(message) => {
                http::Response::builder().status(400).body(message).unwrap()
            }
            Error::FailedToReadBody(message) => {
                http::Response::builder().status(500).body(message).unwrap()
            }
            Error::ActionCallFailed(message) => {
                http::Response::builder().status(400).body(message).unwrap()
            }
        }
    }
}

fn inner_handle(request: http::IncomingRequest) -> Result<http::Response<String>, Error> {
    let body = request.body();

    body.subscribe().block();
    let body_bytes = body
        .read(MAX_READ)
        .map_err(|e| Error::FailedToReadBody(e.to_string()))?;

    let input_wrapper = serde_json::from_slice::<InputWrapper>(&body_bytes)
        .map_err(|e| Error::InvalidInput(e.to_string()))?;

    let input = Input {
        action_id: input_wrapper.action_id,
        payload: Payload {
            input: input_wrapper.payload.input,
        },
    };

    let result = call(&input).map_err(|e| Error::ActionCallFailed(e))?;

    Ok(http::Response::new(result.result))
}

impl http::Server for Component {
    fn handle(
        request: http::IncomingRequest,
    ) -> http::Result<http::Response<impl http::OutgoingBody>> {
        match inner_handle(request) {
            Ok(response) => Ok(response),
            Err(e) => Ok(e.into()),
        }
    }
}

http::export!(Component);
