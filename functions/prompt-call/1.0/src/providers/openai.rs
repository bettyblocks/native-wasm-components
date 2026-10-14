use serde::Deserialize;

use crate::betty_blocks_types::types::types::{BettyAiAgent, McpOptions};
use crate::http::HttpClient;
use crate::providers::{
    ApiError, Prompt, Provider, ProviderRequest, ProviderResponse, ensure_configured,
    mcp_auth_token,
};

const DEFAULT_MAX_TOKENS: u32 = 4096;
const RESPONSES_PATH: &str = "responses";
const WEB_SEARCH_TOOL_TYPE: &str = "web_search";

pub(crate) struct OpenAi;

#[derive(Debug, Deserialize)]
struct ResponsesResponse {
    status: Option<String>,
    #[serde(default)]
    output: Vec<OutputItem>,
    error: Option<ApiError>,
    incomplete_details: Option<IncompleteDetails>,
}

#[derive(Debug, Deserialize)]
struct IncompleteDetails {
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OutputItem {
    #[serde(rename = "type")]
    item_type: String,
    #[serde(default)]
    content: Vec<ContentPart>,
}

#[derive(Debug, Deserialize)]
struct ContentPart {
    #[serde(rename = "type")]
    part_type: String,
    text: Option<String>,
    refusal: Option<String>,
}

impl Provider for OpenAi {
    async fn complete(
        &self,
        client: &impl HttpClient,
        agent: &BettyAiAgent,
        prompt: &Prompt,
    ) -> Result<String, String> {
        ensure_configured(agent)?;

        let headers = vec![
            ("authorization", format!("Bearer {}", agent.api_key)),
            ("content-type", "application/json".to_string()),
        ];

        let (status, response) = client
            .post_json(
                &responses_url(&agent.url),
                headers,
                Self::request_body(agent, prompt),
            )
            .await?;

        ResponsesResponse::handle(status, &response, &agent.api_key)
    }
}

fn responses_url(base_url: &str) -> String {
    format!("{}/{RESPONSES_PATH}", base_url.trim_end_matches('/'))
}

impl ProviderRequest for OpenAi {
    fn base_body(agent: &BettyAiAgent, prompt: &Prompt) -> serde_json::Value {
        serde_json::json!({
            "model": agent.ai_model_name,
            "max_output_tokens": prompt.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            "instructions": prompt.instructions,
            "input": prompt.message,
            "store": false,
        })
    }

    fn web_search_tool() -> serde_json::Value {
        serde_json::json!({ "type": WEB_SEARCH_TOOL_TYPE })
    }

    fn mcp_tool(options: &McpOptions, label: &str) -> serde_json::Value {
        let mut tool = serde_json::json!({
            "type": "mcp",
            "server_label": label,
            "server_url": options.url,
            "require_approval": "never",
        });

        if let Some(token) = mcp_auth_token(options) {
            tool["authorization"] = token.into();
        }

        if let Some(selected) = options.enabled_tools.as_deref() {
            tool["allowed_tools"] = selected.into();
        }

        tool
    }
}

impl ResponsesResponse {
    fn message_parts(&self) -> impl Iterator<Item = &ContentPart> {
        self.output
            .iter()
            .filter(|item| item.item_type == "message")
            .flat_map(|item| &item.content)
    }
}

impl ProviderResponse for ResponsesResponse {
    const PROVIDER: &'static str = "OpenAI";

    fn text_parts(&self) -> Vec<&str> {
        self.message_parts()
            .filter(|part| part.part_type == "output_text")
            .filter_map(|part| part.text.as_deref())
            .collect()
    }

    fn ensure_succeeded(&self) -> Result<(), String> {
        if self.status.as_deref() == Some("failed") {
            let details = self
                .error
                .as_ref()
                .map(ApiError::describe)
                .unwrap_or_default();
            return Err(format!("OpenAI response failed{details}"));
        }

        Ok(())
    }

    fn no_text_error(&self) -> String {
        if let Some(refusal) = self
            .message_parts()
            .find_map(|part| part.refusal.as_deref())
        {
            return format!("OpenAI refused the request: {refusal}");
        }

        if self.status.as_deref() == Some("incomplete") {
            let reason = self
                .incomplete_details
                .as_ref()
                .and_then(|details| details.reason.as_deref())
                .unwrap_or("unknown reason");
            return format!("OpenAI response was incomplete ({reason}) before producing any text");
        }

        "OpenAI response contained no text content".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::betty_blocks_types::types::types::BettyAiTool;
    use crate::test_helpers::{
        MockHttpClient, http_search_tool, mcp_tool as betty_mcp_tool, mcp_tool_limited_to,
        openai_agent, openai_text_response, test_prompt,
    };

    fn agent_with_tools(tools: Vec<BettyAiTool>) -> BettyAiAgent {
        let mut agent = openai_agent();
        agent.tools = Some(tools);
        agent
    }

    fn body_of(agent: &BettyAiAgent, prompt: &Prompt) -> serde_json::Value {
        serde_json::from_slice(&OpenAi::request_body(agent, prompt)).unwrap()
    }

    #[test]
    fn responses_url_appends_the_endpoint_to_the_base_url() {
        assert_eq!(
            responses_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1/responses"
        );
        assert_eq!(
            responses_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/responses"
        );
    }

    #[test]
    fn request_body_carries_model_prompts_and_default_max_tokens() {
        let body = body_of(&openai_agent(), &test_prompt("Summarise this"));

        assert_eq!(body["model"], "gpt-5.4");
        assert_eq!(body["max_output_tokens"], DEFAULT_MAX_TOKENS);
        assert_eq!(body["instructions"], "You are helpful.");
        assert_eq!(body["input"], "Summarise this");
        assert_eq!(body["store"], false);
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn request_body_honours_an_explicit_max_tokens() {
        let mut prompt = test_prompt("hi");
        prompt.max_tokens = Some(256);

        assert_eq!(body_of(&openai_agent(), &prompt)["max_output_tokens"], 256);
    }

    #[test]
    fn request_body_enables_web_search_for_an_http_search_tool() {
        let body = body_of(
            &agent_with_tools(vec![http_search_tool()]),
            &test_prompt("hi"),
        );

        assert_eq!(body["tools"], serde_json::json!([{ "type": "web_search" }]));
    }

    #[test]
    fn request_body_registers_an_mcp_server_with_its_auth_token() {
        let agent = agent_with_tools(vec![betty_mcp_tool(
            "https://mcp.example.com/sse",
            Some("secret-token"),
        )]);

        assert_eq!(
            body_of(&agent, &test_prompt("hi"))["tools"],
            serde_json::json!([{
                "type": "mcp",
                "server_label": "mcp-example-com",
                "server_url": "https://mcp.example.com/sse",
                "require_approval": "never",
                "authorization": "secret-token",
            }])
        );
    }

    #[test]
    fn request_body_omits_the_token_when_the_mcp_server_has_no_authentication_value() {
        let agent = agent_with_tools(vec![betty_mcp_tool("https://mcp.example.com/sse", None)]);

        assert!(
            body_of(&agent, &test_prompt("hi"))["tools"][0]
                .get("authorization")
                .is_none()
        );
    }

    #[test]
    fn request_body_allows_every_mcp_tool_without_a_selection() {
        let agent = agent_with_tools(vec![betty_mcp_tool("https://mcp.example.com/sse", None)]);

        assert!(
            body_of(&agent, &test_prompt("hi"))["tools"][0]
                .get("allowed_tools")
                .is_none()
        );
    }

    #[test]
    fn request_body_allows_only_the_mcp_tools_the_builder_picked() {
        let agent = agent_with_tools(vec![mcp_tool_limited_to(
            "https://mcp.example.com/sse",
            &["get_customer", "list_orders"],
        )]);

        assert_eq!(
            body_of(&agent, &test_prompt("hi"))["tools"][0]["allowed_tools"],
            serde_json::json!(["get_customer", "list_orders"])
        );
    }

    #[test]
    fn request_body_allows_no_mcp_tools_for_an_empty_selection() {
        let agent = agent_with_tools(vec![mcp_tool_limited_to(
            "https://mcp.example.com/sse",
            &[],
        )]);

        assert_eq!(
            body_of(&agent, &test_prompt("hi"))["tools"][0]["allowed_tools"],
            serde_json::json!([])
        );
    }

    #[test]
    fn request_body_keeps_the_selections_of_two_mcp_servers_apart() {
        let agent = agent_with_tools(vec![
            mcp_tool_limited_to("https://mcp.example.com/finance", &["get_quote"]),
            mcp_tool_limited_to("https://mcp.example.com/weather", &["get_forecast"]),
        ]);

        let body = body_of(&agent, &test_prompt("hi"));

        assert_eq!(
            body["tools"][0]["allowed_tools"],
            serde_json::json!(["get_quote"])
        );
        assert_eq!(
            body["tools"][1]["allowed_tools"],
            serde_json::json!(["get_forecast"])
        );
    }

    #[test]
    fn request_body_gives_mcp_servers_on_the_same_host_distinct_labels() {
        let agent = agent_with_tools(vec![
            http_search_tool(),
            betty_mcp_tool("https://mcp.example.com/finance", None),
            betty_mcp_tool("https://mcp.example.com/weather", None),
        ]);

        let body = body_of(&agent, &test_prompt("hi"));

        assert_eq!(body["tools"][0]["type"], "web_search");
        assert_eq!(body["tools"][1]["server_label"], "mcp-example-com");
        assert_eq!(body["tools"][2]["server_label"], "mcp-example-com-2");
    }

    #[test]
    fn extract_text_joins_output_text_across_messages_and_skips_tool_items() {
        let response = serde_json::json!({
            "output": [
                { "type": "reasoning", "summary": [] },
                { "type": "web_search_call", "status": "completed" },
                { "type": "message", "role": "assistant", "content": [
                    { "type": "output_text", "text": "Hello ", "annotations": [] },
                    { "type": "refusal", "refusal": "no" }
                ]},
                { "type": "message", "role": "assistant", "content": [
                    { "type": "output_text", "text": "world", "annotations": [] }
                ]}
            ]
        })
        .to_string();

        assert_eq!(
            ResponsesResponse::extract_text(response.as_bytes()).unwrap(),
            "Hello\n\nworld"
        );
    }

    #[test]
    fn extract_text_rejects_a_response_carrying_no_text() {
        let response = serde_json::json!({ "output": [] }).to_string();

        assert!(ResponsesResponse::extract_text(response.as_bytes()).is_err());
    }

    #[test]
    fn extract_text_rejects_malformed_json() {
        assert!(ResponsesResponse::extract_text(b"not json").is_err());
    }

    #[test]
    fn extract_text_reports_a_failed_response_with_its_error() {
        let response = serde_json::json!({
            "status": "failed",
            "output": [],
            "error": { "code": "server_error", "message": "The model failed." }
        })
        .to_string();

        assert_eq!(
            ResponsesResponse::extract_text(response.as_bytes()).unwrap_err(),
            "OpenAI response failed (server_error): The model failed."
        );
    }

    #[test]
    fn extract_text_reports_an_incomplete_response_without_text() {
        let response = serde_json::json!({
            "status": "incomplete",
            "incomplete_details": { "reason": "max_output_tokens" },
            "output": [{ "type": "reasoning", "summary": [] }]
        })
        .to_string();

        assert_eq!(
            ResponsesResponse::extract_text(response.as_bytes()).unwrap_err(),
            "OpenAI response was incomplete (max_output_tokens) before producing any text"
        );
    }

    #[test]
    fn extract_text_keeps_partial_text_from_an_incomplete_response() {
        let response = serde_json::json!({
            "status": "incomplete",
            "incomplete_details": { "reason": "max_output_tokens" },
            "output": [{ "type": "message", "content": [
                { "type": "output_text", "text": "partial" }
            ]}]
        })
        .to_string();

        assert_eq!(
            ResponsesResponse::extract_text(response.as_bytes()).unwrap(),
            "partial"
        );
    }

    #[test]
    fn extract_text_reports_a_refusal() {
        let response = serde_json::json!({
            "status": "completed",
            "output": [{ "type": "message", "content": [
                { "type": "refusal", "refusal": "I can't help with that." }
            ]}]
        })
        .to_string();

        assert_eq!(
            ResponsesResponse::extract_text(response.as_bytes()).unwrap_err(),
            "OpenAI refused the request: I can't help with that."
        );
    }

    #[test]
    fn status_error_includes_the_code_and_message() {
        let body = r#"{"error":{"message":"Rate limit reached for gpt-5.4.","type":"requests","code":"rate_limit_exceeded"}}"#;

        assert_eq!(
            ResponsesResponse::handle(429, body.as_bytes(), "test-key").unwrap_err(),
            "OpenAI returned status 429 (rate_limit_exceeded): Rate limit reached for gpt-5.4."
        );
    }

    #[test]
    fn status_error_falls_back_to_the_type_without_a_code() {
        let body =
            r#"{"error":{"message":"Bad input.","type":"invalid_request_error","code":null}}"#;

        assert_eq!(
            ResponsesResponse::handle(400, body.as_bytes(), "test-key").unwrap_err(),
            "OpenAI returned status 400 (invalid_request_error): Bad input."
        );
    }

    #[test]
    fn status_error_redacts_the_masked_api_key() {
        let body = r#"{"error":{"message":"Incorrect API key provided: sk-proj-...abcd. You can find your API key at https://platform.openai.com/account/api-keys.","code":"invalid_api_key"}}"#;

        let error = ResponsesResponse::handle(401, body.as_bytes(), "sk-proj-secret").unwrap_err();

        assert!(!error.contains("abcd"));
        assert!(error.starts_with(
            "OpenAI returned status 401 (invalid_api_key): Incorrect API key provided: [redacted]"
        ));
    }

    #[test]
    fn status_error_redacts_the_configured_api_key() {
        let body = r#"{"error":{"message":"key mykey123 is not valid","code":"invalid_api_key"}}"#;

        assert!(
            !ResponsesResponse::handle(401, body.as_bytes(), "mykey123")
                .unwrap_err()
                .contains("mykey123")
        );
    }

    #[test]
    fn status_error_does_not_echo_a_body_that_is_not_an_openai_error() {
        assert_eq!(
            ResponsesResponse::handle(502, b"<html>Bad gateway</html>", "test-key").unwrap_err(),
            "OpenAI returned status 502"
        );
    }

    #[tokio::test]
    async fn complete_posts_to_the_responses_endpoint_with_a_bearer_token() {
        let client = MockHttpClient::new(vec![(200, openai_text_response("42"))]);

        let text = OpenAi
            .complete(&client, &openai_agent(), &test_prompt("meaning?"))
            .await
            .unwrap();

        let request = &client.requests()[0];
        assert_eq!(text, "42");
        assert_eq!(request.url, "https://api.openai.com/v1/responses");
        assert_eq!(request.header("authorization"), Some("Bearer test-key"));
        assert_eq!(request.header("content-type"), Some("application/json"));
    }

    #[tokio::test]
    async fn complete_rejects_an_empty_api_key_without_calling_out() {
        let client = MockHttpClient::new(vec![(200, openai_text_response("unused"))]);
        let mut agent = openai_agent();
        agent.api_key = String::new();

        assert!(
            OpenAi
                .complete(&client, &agent, &test_prompt("hi"))
                .await
                .is_err()
        );
        assert!(client.requests().is_empty());
    }

    #[tokio::test]
    async fn complete_rejects_an_empty_url_without_calling_out() {
        let client = MockHttpClient::new(vec![(200, openai_text_response("unused"))]);
        let mut agent = openai_agent();
        agent.url = String::new();

        assert!(
            OpenAi
                .complete(&client, &agent, &test_prompt("hi"))
                .await
                .is_err()
        );
        assert!(client.requests().is_empty());
    }

    #[tokio::test]
    async fn complete_reports_a_non_success_status_with_a_redacted_error() {
        let client = MockHttpClient::new(vec![(
            401,
            r#"{"error":{"message":"Incorrect API key provided: sk-...abcd"}}"#.to_string(),
        )]);

        let error = OpenAi
            .complete(&client, &openai_agent(), &test_prompt("hi"))
            .await
            .unwrap_err();

        assert_eq!(
            error,
            "OpenAI returned status 401: Incorrect API key provided: [redacted]"
        );
    }
}
