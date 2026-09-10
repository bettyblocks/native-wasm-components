mod anthropic;
mod config;
mod http;
mod provider;
#[cfg(test)]
mod test_helpers;

use wstd::http::Client;

use crate::exports::betty_blocks::ai_agent::ai_agent::{self, Input, Output};
use crate::provider::Provider;

wit_bindgen::generate!({ generate_all });

struct Component;

impl ai_agent::Guest for Component {
    fn ai_agent(input: Input) -> Result<Output, String> {
        wstd::runtime::block_on(run(input))
    }
}

async fn run(input: Input) -> Result<Output, String> {
    let text = match input.provider.provider.as_str() {
        "anthropic" => anthropic::Anthropic.complete(&Client::new(), &input).await?,
        other => return Err(format!("Unsupported provider: {other}")),
    };

    Ok(Output { as_: text })
}

export! {Component}
