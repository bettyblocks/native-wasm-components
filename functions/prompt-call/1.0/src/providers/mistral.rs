use serde::Deserialize;

use crate::betty_blocks_types::types::types::BettyAiAgent;
use crate::http::HttpClient;
use crate::providers::{
    McpServer, Output, Prompt, Provider, ProviderRequest, ProviderResponse, ensure_configured,
    null_as_empty,
};

pub(crate) struct Mistral;

#[derive(Debug, Deserialize)]
struct ConversationsResponse {
    #[serde(default, deserialize_with = "null_as_empty")]
    outputs: Vec<OutputItem>,
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
        ensure_configured(agent)?;

        let headers = vec![
            ("authorization", format!("Bearer {}", agent.api_key)),
            ("content-type", "application/json".to_string()),
        ];

        let (status, response) = client
            .post_json(
                &Self::endpoint_url(agent),
                headers,
                Self::request_body(agent, prompt),
            )
            .await?;

        ConversationsResponse::handle(status, &response)
    }
}

impl ProviderRequest for Mistral {
    const PATH: &'static str = "conversations";

    fn base_body(agent: &BettyAiAgent, prompt: &Prompt) -> serde_json::Value {
        serde_json::json!({
            "model": agent.ai_model_name,
            "inputs": [{ "role": "user", "content": user_content(prompt) }],
            "completion_args": { "max_tokens": prompt.max_tokens() },
        })
    }

    fn web_search_tool() -> serde_json::Value {
        serde_json::json!({ "type": "web_search" })
    }

    // The Conversations API has no MCP support, so MCP servers are left out.
    fn mcp_tool(_server: &McpServer) -> Option<serde_json::Value> {
        None
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
        None
    }
}

// The Conversations API only accepts user and assistant input roles, so the
// system prompt travels inside the user message.
fn user_content(prompt: &Prompt) -> String {
    if prompt.instructions.is_empty() {
        return prompt.message.clone();
    }

    format!("{}\n\n{}", prompt.instructions, prompt.message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{
        MockHttpClient, http_search_tool, mcp_tool, mistral_text_response,
        test_mistral_agent_with_tools, test_mistral_provider, test_prompt,
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
        assert_eq!(
            body["inputs"][0]["content"],
            "You are helpful.\n\nSummarise this"
        );
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
    fn user_content_leaves_the_prompt_alone_without_instructions() {
        let mut prompt = test_prompt("hi");
        prompt.instructions = String::new();

        assert_eq!(user_content(&prompt), "hi");
    }

    #[test]
    fn request_body_offers_web_search_but_no_mcp_servers() {
        let agent = test_mistral_agent_with_tools(vec![
            http_search_tool(),
            mcp_tool("https://mcp.example.com/sse", None),
        ]);

        let body = body_of(&agent, &test_prompt("hi"));

        assert_eq!(body["tools"], serde_json::json!([{ "type": "web_search" }]));
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
    fn extract_rejects_malformed_json() {
        assert!(ConversationsResponse::extract(b"not json").is_err());
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
}
