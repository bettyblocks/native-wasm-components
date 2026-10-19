use serde::Deserialize;

use crate::betty_blocks_types::types::types::{BettyAiAgent, BettyAiTool};
use crate::http::HttpClient;
use crate::providers::{Output, Prompt, Provider};

const CONVERSATIONS_PATH: &str = "conversations";

pub(crate) struct Mistral;

#[derive(Debug, Deserialize)]
struct ConversationsResponse {
    outputs: Vec<OutputItem>,
}

#[derive(Debug, Deserialize)]
struct OutputItem {
    #[serde(rename = "type")]
    output_type: String,
    content: Option<Content>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Content {
    Text(String),
    Chunks(Vec<Chunk>),
}

#[derive(Debug, Deserialize)]
struct Chunk {
    text: Option<String>,
}

impl Provider for Mistral {
    async fn complete(
        &self,
        client: &impl HttpClient,
        agent: &BettyAiAgent,
        prompt: &Prompt,
    ) -> Result<Output, String> {
        if agent.api_key.is_empty() {
            return Err("No API key configured for agent: mistral".to_string());
        }

        if agent.url.is_empty() {
            return Err("No URL configured for agent: mistral".to_string());
        }

        let mut tools = Vec::new();

        for tool in agent.tools.as_deref().unwrap_or(&[]) {
            if let BettyAiTool::HttpSearch(_) = tool {
                tools.push(serde_json::json!({ "type": "web_search" }));
            }
        }

        let (status, response) = client
            .post_json(
                &conversations_url(&agent.url),
                auth_headers(agent),
                request_body(agent, prompt, tools),
            )
            .await?;

        if status != 200 {
            Err(format!(
                "Mistral returned status {status}: {}",
                String::from_utf8_lossy(&response)
            ))
        } else {
            extract_text(&response).map(|text| Output {
                text,
                usage: serde_json::json!({}),
            })
        }
    }
}

fn auth_headers(agent: &BettyAiAgent) -> Vec<(&'static str, String)> {
    vec![
        ("authorization", format!("Bearer {}", agent.api_key)),
        ("content-type", "application/json".to_string()),
    ]
}

fn conversations_url(base_url: &str) -> String {
    format!(
        "{base}/{CONVERSATIONS_PATH}",
        base = base_url.trim_end_matches('/')
    )
}

fn request_body(agent: &BettyAiAgent, prompt: &Prompt, tools: Vec<serde_json::Value>) -> Vec<u8> {
    let mut body = serde_json::json!({
        "model": agent.ai_model_name,
        "inputs": [{ "role": "user", "content": user_content(prompt) }],
        "completion_args": {
            "max_tokens": prompt.max_tokens(),
        },
    });

    if !tools.is_empty() {
        body["tools"] = tools.into();
    }

    body.to_string().into_bytes()
}

// The Conversations API only accepts user and assistant input roles, so the
// system prompt travels inside the user message.
fn user_content(prompt: &Prompt) -> String {
    if prompt.instructions.is_empty() {
        return prompt.message.clone();
    }

    format!("{}\n\n{}", prompt.instructions, prompt.message)
}

fn extract_text(response: &[u8]) -> Result<String, String> {
    let parsed: ConversationsResponse = serde_json::from_slice(response)
        .map_err(|e| format!("Failed to parse Mistral response: {e}"))?;

    let text: String = parsed
        .outputs
        .into_iter()
        .filter(|output| output.output_type == "message.output")
        .filter_map(|output| output.content)
        .map(|content| match content {
            Content::Text(text) => text.trim().to_string(),
            Content::Chunks(chunks) => chunks
                .into_iter()
                .filter_map(|chunk| chunk.text)
                .collect::<Vec<_>>()
                .join(""),
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    if text.is_empty() {
        return Err("Mistral response contained no text content".to_string());
    }

    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{
        MockHttpClient, http_search_tool, mistral_text_response, test_mistral_agent_with_tools,
        test_mistral_provider, test_prompt,
    };

    #[test]
    fn conversations_url_appends_the_endpoint_to_the_base_url() {
        assert_eq!(
            conversations_url("https://api.mistral.ai/v1"),
            "https://api.mistral.ai/v1/conversations"
        );
    }

    #[test]
    fn conversations_url_does_not_double_the_separator() {
        assert_eq!(
            conversations_url("https://api.mistral.ai/v1/"),
            "https://api.mistral.ai/v1/conversations"
        );
    }

    #[test]
    fn request_body_carries_model_prompts_and_default_max_tokens() {
        let body: serde_json::Value = serde_json::from_slice(&request_body(
            &test_mistral_provider(),
            &test_prompt("Summarise this"),
            Vec::new(),
        ))
        .unwrap();

        assert_eq!(body["model"], "mistral-large-latest");
        assert_eq!(body["completion_args"]["max_tokens"], DEFAULT_MAX_TOKENS);
        assert_eq!(body["inputs"].as_array().unwrap().len(), 1);
        assert_eq!(body["inputs"][0]["role"], "user");
        assert_eq!(
            body["inputs"][0]["content"],
            "You are helpful.\n\nSummarise this"
        );
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn user_content_leaves_the_prompt_alone_without_instructions() {
        let mut prompt = test_prompt("hi");
        prompt.instructions = String::new();

        assert_eq!(user_content(&prompt), "hi");
    }

    #[test]
    fn request_body_honours_an_explicit_max_tokens() {
        let mut prompt = test_prompt("hi");
        prompt.max_tokens = Some(256);

        let body: serde_json::Value =
            serde_json::from_slice(&request_body(&test_mistral_provider(), &prompt, Vec::new()))
                .unwrap();

        assert_eq!(body["completion_args"]["max_tokens"], 256);
    }

    #[test]
    fn extract_text_returns_the_assistant_text() {
        let response = serde_json::json!({
            "outputs": [
                { "type": "message.output", "content": "Hello there" }
            ]
        })
        .to_string();

        assert_eq!(extract_text(response.as_bytes()).unwrap(), "Hello there");
    }

    #[test]
    fn extract_text_joins_chunked_content() {
        let response = serde_json::json!({
            "outputs": [
                {
                    "type": "message.output",
                    "content": [
                        { "type": "text", "text": "Hello " },
                        { "type": "text", "text": "world" }
                    ]
                }
            ]
        })
        .to_string();

        assert_eq!(extract_text(response.as_bytes()).unwrap(), "Hello world");
    }

    #[test]
    fn extract_text_skips_outputs_that_are_not_messages() {
        let response = serde_json::json!({
            "outputs": [
                { "type": "tool.execution", "name": "web_search" },
                { "type": "message.output", "content": "answer" }
            ]
        })
        .to_string();

        assert_eq!(extract_text(response.as_bytes()).unwrap(), "answer");
    }

    #[test]
    fn extract_text_rejects_a_response_carrying_no_text() {
        let response = serde_json::json!({ "outputs": [] }).to_string();

        assert!(extract_text(response.as_bytes()).is_err());
    }

    #[test]
    fn extract_text_rejects_malformed_json() {
        assert!(extract_text(b"not json").is_err());
    }

    #[tokio::test]
    async fn complete_returns_the_assistant_text() {
        let client = MockHttpClient::new(vec![(200, mistral_text_response("42"))]);

        let text = Mistral
            .complete(&client, &test_mistral_provider(), &test_prompt("meaning?"))
            .await
            .unwrap();

        assert_eq!(text.text, "42");
    }

    #[tokio::test]
    async fn complete_posts_to_the_conversations_endpoint_of_the_provider_url() {
        let client = MockHttpClient::new(vec![(200, mistral_text_response("ok"))]);
        let mut agent = test_mistral_provider();
        agent.url = "http://localhost:4010/v1".to_string();

        Mistral
            .complete(&client, &agent, &test_prompt("hi"))
            .await
            .unwrap();

        assert_eq!(
            client.requests()[0].url,
            "http://localhost:4010/v1/conversations"
        );
    }

    #[tokio::test]
    async fn complete_sends_the_bearer_token_and_json_body() {
        let client = MockHttpClient::new(vec![(200, mistral_text_response("ok"))]);

        Mistral
            .complete(&client, &test_mistral_provider(), &test_prompt("hi"))
            .await
            .unwrap();

        let request = &client.requests()[0];
        assert_eq!(request.header("authorization"), Some("Bearer test-key"));
        assert_eq!(request.header("content-type"), Some("application/json"));
        assert_eq!(request.json()["model"], "mistral-large-latest");
    }

    #[tokio::test]
    async fn complete_sends_the_web_search_tool() {
        let client = MockHttpClient::new(vec![(200, mistral_text_response("ok"))]);
        let agent = test_mistral_agent_with_tools(vec![http_search_tool()]);

        Mistral
            .complete(&client, &agent, &test_prompt("latest news"))
            .await
            .unwrap();

        assert_eq!(client.requests().len(), 1);
        let request = client.requests()[0].json();
        assert_eq!(request["tools"][0]["type"], "web_search");
    }

    #[tokio::test]
    async fn complete_rejects_an_empty_api_key_without_calling_out() {
        let client = MockHttpClient::new(vec![(200, mistral_text_response("unused"))]);
        let mut agent = test_mistral_provider();
        agent.api_key = String::new();

        assert!(
            Mistral
                .complete(&client, &agent, &test_prompt("hi"))
                .await
                .is_err()
        );
        assert!(client.requests().is_empty());
    }

    #[tokio::test]
    async fn complete_rejects_an_empty_url_without_calling_out() {
        let client = MockHttpClient::new(vec![(200, mistral_text_response("unused"))]);
        let mut agent = test_mistral_provider();
        agent.url = String::new();

        assert!(
            Mistral
                .complete(&client, &agent, &test_prompt("hi"))
                .await
                .is_err()
        );
        assert!(client.requests().is_empty());
    }

    #[tokio::test]
    async fn complete_reports_a_non_success_status() {
        let client = MockHttpClient::new(vec![(429, "rate limited".to_string())]);

        let error = Mistral
            .complete(&client, &test_mistral_provider(), &test_prompt("hi"))
            .await
            .unwrap_err();

        assert!(error.contains("429"));
    }
}
