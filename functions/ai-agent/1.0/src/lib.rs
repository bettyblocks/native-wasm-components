use crate::exports::betty_blocks::ai_agent::ai_agent::{self, Input, Output};

wit_bindgen::generate!({ with: {
    "wasi:io/streams@0.2.6": ::wasi::io::streams,
    "wasi:io/error@0.2.6": ::wasi::io::error,
    "wasi:clocks/monotonic-clock@0.2.6": ::wasi::clocks::monotonic_clock,
    "wasi:io/poll@0.2.6": ::wasi::io::poll,
    "wasi:http/types@0.2.6": ::wasi::http::types,
    "wasi:http/outgoing-handler@0.2.6": ::wasi::http::outgoing_handler,
    }
});

struct Component;

impl ai_agent::Guest for Component {
    fn ai_agent(input: Input) -> Result<Output, String> {
        Err(format!(
            "Unsupported provider: {}",
            input.provider.provider
        ))
    }
}

export! {Component}
