use serde::Deserialize;

use crate::betty_blocks_types::types::types::BettyAiAgent;
use crate::http::HttpClient;
use crate::providers::{
    McpServer, Output, Prompt, Provider, ProviderRequest, ProviderResponse, complete_request,
    ensure_no_mcp_tools, null_as_empty,
};

pub(crate) struct Mistral;

#[derive(Debug, Deserialize)]
pub(crate) struct ConversationsResponse {
    #[serde(default, deserialize_with = "null_as_empty")]
    outputs: Vec<OutputItem>,
    usage: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct OutputItem {
    #[serde(rename = "type")]
    output_type: String,
    #[serde(default, deserialize_with = "joined_content")]
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Chunk {
    text: Option<String>,
}

// The Conversations API returns the assistant text either as a plain string
// or as a list of text chunks; joining here keeps both shapes in one field.
fn joined_content<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Content {
        Text(String),
        Chunks(Vec<Chunk>),
    }

    Ok(match Option::<Content>::deserialize(deserializer)? {
        Some(Content::Text(text)) => Some(text),
        Some(Content::Chunks(chunks)) => Some(
            chunks
                .into_iter()
                .filter_map(|chunk| chunk.text)
                .collect::<Vec<_>>()
                .join(""),
        ),
        None => None,
    })
}

impl Provider for Mistral {
    async fn complete(
        &self,
        client: &impl HttpClient,
        agent: &BettyAiAgent,
        prompt: &Prompt,
    ) -> Result<Output, String> {
        complete_request::<Mistral>(client, agent, prompt).await
    }
}

impl ProviderRequest for Mistral {
    type Response = ConversationsResponse;

    const PATH: &'static str = "conversations";

    // The Conversations API has no MCP support, so agents relying on MCP tools
    // are rejected instead of silently losing them in the request.
    fn ensure_tools_supported(agent: &BettyAiAgent) -> Result<(), String> {
        ensure_no_mcp_tools("Mistral", agent)
    }

    fn base_body(agent: &BettyAiAgent, prompt: &Prompt) -> serde_json::Value {
        serde_json::json!({
            "model": agent.ai_model_name,
            "instructions": prompt.instructions,
            "inputs": [{ "role": "user", "content": prompt.message }],
            "completion_args": { "max_tokens": prompt.max_tokens() },
            "store": false,
        })
    }

    fn web_search_tool() -> serde_json::Value {
        serde_json::json!({ "type": "web_search" })
    }

    fn mcp_tool(_server: &McpServer) -> serde_json::Value {
        unreachable!("ensure_tools_supported rejects MCP tools before the request body is built")
    }
}

impl ProviderResponse for ConversationsResponse {
    const PROVIDER: &'static str = "Mistral";

    fn text_parts(&self) -> Vec<&str> {
        self.outputs
            .iter()
            .filter(|output| output.output_type == "message.output")
            .filter_map(|output| output.content.as_deref())
            .collect()
    }

    fn usage(&self) -> Option<&serde_json::Value> {
        self.usage.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{
        MockHttpClient, mcp_tool, mistral_text_response, test_mistral_agent_with_tools,
        test_mistral_provider, test_prompt,
    };

    fn body_of(agent: &BettyAiAgent, prompt: &Prompt) -> serde_json::Value {
        serde_json::from_slice(&Mistral::request_body(agent, prompt)).unwrap()
    }

    #[test]
    fn request_body_carries_model_prompts_and_default_max_tokens() {
        let body = body_of(&test_mistral_provider(), &test_prompt("Summarise this"));

        assert_eq!(body["model"], "mistral-large-latest");
        assert_eq!(
            body["completion_args"]["max_tokens"],
            crate::providers::DEFAULT_MAX_TOKENS
        );
        assert_eq!(body["inputs"].as_array().unwrap().len(), 1);
        assert_eq!(body["inputs"][0]["role"], "user");
        assert_eq!(body["inputs"][0]["content"], "Summarise this");
        assert_eq!(body["instructions"], "You are helpful.");
        assert_eq!(body["store"], false);
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn request_body_honours_an_explicit_max_tokens() {
        let mut prompt = test_prompt("hi");
        prompt.max_tokens = Some(256);

        assert_eq!(
            body_of(&test_mistral_provider(), &prompt)["completion_args"]["max_tokens"],
            256
        );
    }

    #[test]
    fn request_body_carries_empty_instructions_without_a_placeholder() {
        let mut prompt = test_prompt("hi");
        prompt.instructions = String::new();

        let body = body_of(&test_mistral_provider(), &prompt);

        assert_eq!(body["instructions"], "");
        assert_eq!(body["inputs"][0]["content"], "hi");
    }

    #[test]
    fn extract_returns_the_assistant_text() {
        let response = serde_json::json!({
            "outputs": [
                { "type": "message.output", "content": "Hello there" }
            ]
        })
        .to_string();

        assert_eq!(
            ConversationsResponse::extract(response.as_bytes())
                .unwrap()
                .text,
            "Hello there"
        );
    }

    #[test]
    fn extract_joins_chunked_content() {
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

        assert_eq!(
            ConversationsResponse::extract(response.as_bytes())
                .unwrap()
                .text,
            "Hello world"
        );
    }

    #[test]
    fn extract_skips_outputs_that_are_not_messages() {
        let response = serde_json::json!({
            "outputs": [
                { "type": "tool.execution", "name": "web_search" },
                { "type": "message.output", "content": "answer" }
            ]
        })
        .to_string();

        assert_eq!(
            ConversationsResponse::extract(response.as_bytes())
                .unwrap()
                .text,
            "answer"
        );
    }

    #[test]
    fn extract_rejects_a_response_carrying_no_text() {
        let response = serde_json::json!({ "outputs": [] }).to_string();

        assert!(ConversationsResponse::extract(response.as_bytes()).is_err());
    }

    #[test]
    fn extract_passes_the_usage_through() {
        let usage = serde_json::json!({
            "prompt_tokens": 12,
            "completion_tokens": 3,
            "total_tokens": 15
        });
        let response = serde_json::json!({
            "outputs": [{ "type": "message.output", "content": "answer" }],
            "usage": usage
        })
        .to_string();

        assert_eq!(
            ConversationsResponse::extract(response.as_bytes())
                .unwrap()
                .usage,
            usage
        );
    }

    #[test]
    fn extract_reports_an_empty_usage_when_the_response_has_none() {
        assert_eq!(
            ConversationsResponse::extract(mistral_text_response("answer").as_bytes())
                .unwrap()
                .usage,
            serde_json::json!({})
        );
    }

    #[test]
    fn extract_rejects_malformed_json() {
        assert!(ConversationsResponse::extract(b"not json").is_err());
    }

    #[tokio::test]
    async fn complete_rejects_an_mcp_tool_without_calling_out() {
        let client = MockHttpClient::new(vec![(200, mistral_text_response("unused"))]);
        let agent =
            test_mistral_agent_with_tools(vec![mcp_tool("https://mcp.example.com/sse", None)]);

        let error = Mistral
            .complete(&client, &agent, &test_prompt("hi"))
            .await
            .unwrap_err();

        assert_eq!(
            error,
            "Mistral does not support server-side MCP tools, configured on agent: mistral"
        );
        assert!(client.requests().is_empty());
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

    #[test]
    fn status_error_includes_the_type_and_message() {
        let body = r#"{"error":{"type":"rate_limit_error","message":"Too many requests."}}"#;

        assert_eq!(
            ConversationsResponse::handle(429, body.as_bytes()).unwrap_err(),
            "Mistral returned status 429 (rate_limit_error): Too many requests."
        );
    }

    #[test]
    fn status_error_does_not_echo_a_body_that_is_not_a_mistral_error() {
        assert_eq!(
            ConversationsResponse::handle(502, b"bad gateway").unwrap_err(),
            "Mistral returned status 502"
        );
    }
}
