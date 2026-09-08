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
