mod anthropic;
mod config;
mod http;
mod provider;
#[cfg(test)]
mod test_helpers;

use wstd::http::Client;

use crate::exports::betty_blocks::ai_agent::ai_agent::{self, Input, Output};
use crate::http::HttpClient;
use crate::provider::Provider;

wit_bindgen::generate!({ generate_all });

struct Component;

impl ai_agent::Guest for Component {
    fn ai_agent(input: Input) -> Result<Output, String> {
        wstd::runtime::block_on(run(&Client::new(), input))
    }
}

async fn run(client: &impl HttpClient, input: Input) -> Result<Output, String> {
    let text = match input.provider.provider.as_str() {
        "anthropic" => anthropic::Anthropic.complete(client, &input).await?,
        other => return Err(format!("Unsupported provider: {other}")),
    };

    Ok(Output { as_: text })
}

export! {Component}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{MockHttpClient, anthropic_text_response, test_input};

    #[tokio::test]
    async fn dispatches_anthropic_to_the_anthropic_provider() {
        let client = MockHttpClient::new(vec![(200, anthropic_text_response("hello"))]);

        let output = run(&client, test_input("hi")).await.unwrap();

        assert_eq!(output.as_, "hello");
        assert_eq!(client.requests().len(), 1);
    }

    #[tokio::test]
    async fn rejects_an_unsupported_provider_without_calling_out() {
        let client = MockHttpClient::new(vec![(200, anthropic_text_response("unused"))]);
        let mut input = test_input("hi");
        input.provider.provider = "openai".to_string();

        let error = run(&client, input).await.unwrap_err();

        assert!(error.contains("openai"));
        assert!(client.requests().is_empty());
    }
}
