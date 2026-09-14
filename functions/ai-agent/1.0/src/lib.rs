mod anthropic;
mod config;
mod http;
mod provider;
#[cfg(test)]
mod test_helpers;

use wstd::http::Client;

use crate::exports::betty_blocks::ai_agent::ai_agent::{self, AiProvider};
use crate::http::HttpClient;
use crate::provider::{Prompt, Provider};

wit_bindgen::generate!({ generate_all });

struct Component;

impl ai_agent::Guest for Component {
    fn ai_agent(
        provider: AiProvider,
        instructions: String,
        message: String,
        max_tokens: Option<u32>,
    ) -> Result<String, String> {
        let prompt = Prompt {
            instructions,
            message,
            max_tokens,
        };

        wstd::runtime::block_on(run(&Client::new(), &provider, &prompt))
    }
}

async fn run(
    client: &impl HttpClient,
    provider: &AiProvider,
    prompt: &Prompt,
) -> Result<String, String> {
    match provider.name.as_str() {
        "anthropic" => anthropic::Anthropic.complete(client, provider, prompt).await,
        other => Err(format!("Unsupported provider: {other}")),
    }
}

export! {Component}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{
        MockHttpClient, anthropic_text_response, test_prompt, test_provider,
    };

    #[tokio::test]
    async fn dispatches_anthropic_to_the_anthropic_provider() {
        let client = MockHttpClient::new(vec![(200, anthropic_text_response("hello"))]);

        let text = run(&client, &test_provider(), &test_prompt("hi"))
            .await
            .unwrap();

        assert_eq!(text, "hello");
        assert_eq!(client.requests().len(), 1);
    }

    #[tokio::test]
    async fn rejects_an_unsupported_provider_without_calling_out() {
        let client = MockHttpClient::new(vec![(200, anthropic_text_response("unused"))]);
        let mut provider = test_provider();
        provider.name = "openai".to_string();

        let error = run(&client, &provider, &test_prompt("hi"))
            .await
            .unwrap_err();

        assert!(error.contains("openai"));
        assert!(client.requests().is_empty());
    }
}
