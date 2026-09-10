use serde::Deserialize;

use crate::config;
use crate::exports::betty_blocks::ai_agent::ai_agent::Input;
use crate::http::HttpClient;
use crate::provider::Provider;

const DEFAULT_MAX_TOKENS: u32 = 4096;

pub(crate) struct Anthropic;

#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
}

impl Provider for Anthropic {
    async fn complete(&self, client: &impl HttpClient, input: &Input) -> Result<String, String> {
        if input.provider.api_key.is_empty() {
            return Err("No API key configured for provider: anthropic".to_string());
        }

        let headers = vec![
            ("x-api-key", input.provider.api_key.clone()),
            ("anthropic-version", config::api_version()),
            ("content-type", "application/json".to_string()),
        ];

        let (status, response) = client
            .post_json(&config::api_url(), headers, request_body(input))
            .await?;

        if status != 200 {
            return Err(format!("Anthropic returned status {status}"));
        }

        extract_text(&response)
    }
}

fn request_body(input: &Input) -> Vec<u8> {
    serde_json::json!({
        "model": input.provider.model,
        "max_tokens": input.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        "system": input.instructions,
        "messages": [{ "role": "user", "content": input.message }],
    })
    .to_string()
    .into_bytes()
}

fn extract_text(response: &[u8]) -> Result<String, String> {
    let parsed: MessagesResponse = serde_json::from_slice(response)
        .map_err(|e| format!("Failed to parse Anthropic response: {e}"))?;

    let text: String = parsed
        .content
        .into_iter()
        .filter(|block| block.block_type == "text")
        .filter_map(|block| block.text)
        .collect();

    if text.is_empty() {
        return Err("Anthropic response contained no text content".to_string());
    }

    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{MockHttpClient, test_input};

    fn text_response(text: &str) -> String {
        serde_json::json!({ "content": [{ "type": "text", "text": text }] }).to_string()
    }

    #[test]
    fn request_body_carries_model_prompts_and_default_max_tokens() {
        let body: serde_json::Value =
            serde_json::from_slice(&request_body(&test_input("Summarise this"))).unwrap();

        assert_eq!(body["model"], "claude-sonnet-5");
        assert_eq!(body["max_tokens"], DEFAULT_MAX_TOKENS);
        assert_eq!(body["system"], "You are helpful.");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "Summarise this");
    }

    #[test]
    fn request_body_honours_an_explicit_max_tokens() {
        let mut input = test_input("hi");
        input.max_tokens = Some(256);

        let body: serde_json::Value = serde_json::from_slice(&request_body(&input)).unwrap();

        assert_eq!(body["max_tokens"], 256);
    }

    #[test]
    fn extract_text_joins_consecutive_text_blocks() {
        let response = serde_json::json!({
            "content": [
                { "type": "text", "text": "Hello " },
                { "type": "text", "text": "world" }
            ]
        })
        .to_string();

        assert_eq!(extract_text(response.as_bytes()).unwrap(), "Hello world");
    }

    #[test]
    fn extract_text_skips_blocks_that_are_not_text() {
        let response = serde_json::json!({
            "content": [
                { "type": "thinking", "thinking": "hmm" },
                { "type": "text", "text": "answer" }
            ]
        })
        .to_string();

        assert_eq!(extract_text(response.as_bytes()).unwrap(), "answer");
    }

    #[test]
    fn extract_text_rejects_a_response_carrying_no_text() {
        let response = serde_json::json!({ "content": [] }).to_string();

        assert!(extract_text(response.as_bytes()).is_err());
    }

    #[test]
    fn extract_text_rejects_malformed_json() {
        assert!(extract_text(b"not json").is_err());
    }

    #[tokio::test]
    async fn complete_returns_the_assistant_text() {
        let client = MockHttpClient::new(vec![(200, text_response("42"))]);

        let text = Anthropic
            .complete(&client, &test_input("meaning?"))
            .await
            .unwrap();

        assert_eq!(text, "42");
    }

    #[tokio::test]
    async fn complete_sends_the_anthropic_headers() {
        let client = MockHttpClient::new(vec![(200, text_response("ok"))]);

        Anthropic
            .complete(&client, &test_input("hi"))
            .await
            .unwrap();

        let request = &client.requests()[0];
        assert!(request.url.ends_with("/v1/messages"));
        assert_eq!(request.header("x-api-key"), Some("test-key"));
        assert_eq!(request.header("anthropic-version"), Some("2023-06-01"));
        assert_eq!(request.header("content-type"), Some("application/json"));
        assert_eq!(request.json()["model"], "claude-sonnet-5");
    }

    #[tokio::test]
    async fn complete_rejects_an_empty_api_key_without_calling_out() {
        let client = MockHttpClient::new(vec![(200, text_response("unused"))]);
        let mut input = test_input("hi");
        input.provider.api_key = String::new();

        assert!(Anthropic.complete(&client, &input).await.is_err());
        assert!(client.requests().is_empty());
    }

    #[tokio::test]
    async fn complete_reports_a_non_success_status() {
        let client = MockHttpClient::new(vec![(429, "rate limited".to_string())]);

        let error = Anthropic
            .complete(&client, &test_input("hi"))
            .await
            .unwrap_err();

        assert!(error.contains("429"));
    }
}
