use serde::Deserialize;

use crate::betty_blocks_types::types::types::{BettyAiAgent, BettyAiTool};
use crate::http::HttpClient;
use crate::providers::{
    McpServer, Output, Prompt, Provider, ProviderRequest, ProviderResponse, active_tools,
    complete_request, mcp_auth_token, null_as_empty,
};

const API_VERSION: &str = "2023-06-01";
const WEB_SEARCH_TOOL_TYPE: &str = "web_search_20260318";
const MCP_CLIENT_BETA: &str = "mcp-client-2025-11-20";

pub(crate) struct Anthropic;

#[derive(Debug, Deserialize)]
pub(crate) struct MessagesResponse {
    #[serde(default, deserialize_with = "null_as_empty")]
    content: Vec<ContentBlock>,
    stop_reason: Option<String>,
    usage: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
    name: Option<String>,
}

impl MessagesResponse {
    fn requested_tools(&self) -> String {
        let names: Vec<&str> = self
            .content
            .iter()
            .filter(|block| block.block_type == "tool_use")
            .filter_map(|block| block.name.as_deref())
            .collect();

        if names.is_empty() {
            return String::new();
        }

        format!(". Requested tools: {}", names.join(", "))
    }
}

impl Provider for Anthropic {
    async fn complete(
        &self,
        client: &impl HttpClient,
        agent: &BettyAiAgent,
        prompt: &Prompt,
    ) -> Result<Output, String> {
        complete_request::<Anthropic>(client, agent, prompt).await
    }
}

fn uses_mcp(agent: &BettyAiAgent) -> bool {
    active_tools(agent).any(|tool| matches!(tool, BettyAiTool::Mcp(_)))
}

impl ProviderRequest for Anthropic {
    type Response = MessagesResponse;

    const PATH: &'static str = "messages";

    fn headers(agent: &BettyAiAgent) -> Vec<(&'static str, String)> {
        let mut headers = vec![
            ("x-api-key", agent.api_key.clone()),
            ("anthropic-version", API_VERSION.to_string()),
            ("content-type", "application/json".to_string()),
        ];
        if uses_mcp(agent) {
            headers.push(("anthropic-beta", MCP_CLIENT_BETA.to_string()));
        }
        headers
    }

    fn base_body(agent: &BettyAiAgent, prompt: &Prompt) -> serde_json::Value {
        serde_json::json!({
            "model": agent.ai_model_name,
            "max_tokens": prompt.max_tokens(),
            "system": prompt.instructions,
            "messages": [{ "role": "user", "content": prompt.message }],
        })
    }

    fn web_search_tool() -> serde_json::Value {
        serde_json::json!({
            "type": WEB_SEARCH_TOOL_TYPE,
            "name": "web_search",
            "allowed_callers": ["direct"],
        })
    }

    fn mcp_tool(server: &McpServer) -> serde_json::Value {
        let mut toolset = serde_json::json!({
            "type": "mcp_toolset",
            "mcp_server_name": server.name,
        });

        let Some(selected) = server.options.enabled_tools.as_deref() else {
            return toolset;
        };

        toolset["default_config"] = serde_json::json!({ "enabled": false });
        toolset["configs"] = selected
            .iter()
            .map(|tool| (tool.clone(), serde_json::json!({ "enabled": true })))
            .collect::<serde_json::Map<String, serde_json::Value>>()
            .into();

        toolset
    }

    fn finish_body(body: &mut serde_json::Value, servers: &[McpServer]) {
        if servers.is_empty() {
            return;
        }

        body["mcp_servers"] = servers
            .iter()
            .map(|server| {
                let mut entry = serde_json::json!({
                    "type": "url",
                    "url": server.options.url,
                    "name": server.name,
                });
                if let Some(token) = mcp_auth_token(server.options) {
                    entry["authorization_token"] = token.into();
                }
                entry
            })
            .collect::<Vec<_>>()
            .into();
    }
}

impl ProviderResponse for MessagesResponse {
    const PROVIDER: &'static str = "Anthropic";

    fn text_parts(&self) -> Vec<&str> {
        self.content
            .iter()
            .filter(|block| block.block_type == "text")
            .filter_map(|block| block.text.as_deref())
            .collect()
    }

    fn usage(&self) -> Option<&serde_json::Value> {
        self.usage.as_ref()
    }

    fn no_text_error(&self) -> String {
        match self.stop_reason.as_deref() {
            Some("refusal") => "Anthropic refused the request".to_string(),
            Some(reason) if !matches!(reason, "end_turn" | "stop_sequence") => format!(
                "Anthropic response was incomplete ({reason}) before producing any text{}",
                self.requested_tools()
            ),
            _ => "Anthropic response contained no text content".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::betty_blocks_types::types::types::AuthenticationConfig;
    use crate::betty_blocks_types::types::types::McpOptions;
    use crate::test_helpers::{
        MockHttpClient, anthropic_text_response, http_search_tool, mcp_tool, mcp_tool_limited_to,
        test_agent_with_tools, test_prompt, test_provider,
    };

    #[test]
    fn request_body_carries_model_prompts_and_default_max_tokens() {
        let body: serde_json::Value = serde_json::from_slice(&Anthropic::request_body(
            &test_provider(),
            &test_prompt("Summarise this"),
        ))
        .unwrap();

        assert_eq!(body["model"], "claude-sonnet-5");
        assert_eq!(body["max_tokens"], crate::providers::DEFAULT_MAX_TOKENS);
        assert_eq!(body["system"], "You are helpful.");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "Summarise this");
    }

    #[test]
    fn request_body_honours_an_explicit_max_tokens() {
        let mut prompt = test_prompt("hi");
        prompt.max_tokens = Some(256);

        let body: serde_json::Value =
            serde_json::from_slice(&Anthropic::request_body(&test_provider(), &prompt)).unwrap();

        assert_eq!(body["max_tokens"], 256);
    }

    #[test]
    fn request_body_omits_the_tool_fields_without_tools() {
        let body: serde_json::Value = serde_json::from_slice(&Anthropic::request_body(
            &test_provider(),
            &test_prompt("hi"),
        ))
        .unwrap();

        assert!(body.get("tools").is_none());
        assert!(body.get("mcp_servers").is_none());
    }

    #[test]
    fn request_body_enables_web_search_for_an_http_search_tool() {
        let agent = test_agent_with_tools(vec![http_search_tool()]);

        let body: serde_json::Value = serde_json::from_slice(&Anthropic::request_body(
            &agent,
            &test_prompt("latest news"),
        ))
        .unwrap();

        assert_eq!(body["tools"].as_array().unwrap().len(), 1);
        assert_eq!(body["tools"][0]["type"], WEB_SEARCH_TOOL_TYPE);
        assert_eq!(body["tools"][0]["name"], "web_search");
        assert_eq!(body["tools"][0]["allowed_callers"][0], "direct");
        assert!(body["tools"][0].get("description").is_none());
        assert!(body.get("mcp_servers").is_none());
    }

    #[test]
    fn request_body_registers_an_mcp_server_with_its_auth_token() {
        let agent = test_agent_with_tools(vec![mcp_tool(
            "https://mcp.example.com/sse",
            Some("secret-token"),
        )]);

        let body: serde_json::Value =
            serde_json::from_slice(&Anthropic::request_body(&agent, &test_prompt("hi"))).unwrap();

        assert_eq!(body["mcp_servers"].as_array().unwrap().len(), 1);
        assert_eq!(body["mcp_servers"][0]["type"], "url");
        assert_eq!(body["mcp_servers"][0]["url"], "https://mcp.example.com/sse");
        assert_eq!(body["mcp_servers"][0]["name"], "mcp-example-com");
        assert_eq!(
            body["mcp_servers"][0]["authorization_token"],
            "secret-token"
        );
    }

    #[test]
    fn request_body_sends_an_api_key_as_the_authorization_token() {
        let agent = test_agent_with_tools(vec![BettyAiTool::Mcp(McpOptions {
            description: None,
            url: "https://mcp.example.com/sse".to_string(),
            authentication: AuthenticationConfig {
                kind: "api_key".to_string(),
                value: Some("my-api-key".to_string()),
            },
            enabled_tools: None,
        })]);

        let body: serde_json::Value =
            serde_json::from_slice(&Anthropic::request_body(&agent, &test_prompt("hi"))).unwrap();

        assert_eq!(body["mcp_servers"][0]["authorization_token"], "my-api-key");
        assert!(body["mcp_servers"][0].get("custom_headers").is_none());
    }

    #[test]
    fn request_body_references_each_mcp_server_from_an_mcp_toolset() {
        let agent = test_agent_with_tools(vec![mcp_tool("https://mcp.example.com/sse", None)]);

        let body: serde_json::Value =
            serde_json::from_slice(&Anthropic::request_body(&agent, &test_prompt("hi"))).unwrap();

        assert_eq!(body["tools"].as_array().unwrap().len(), 1);
        assert_eq!(body["tools"][0]["type"], "mcp_toolset");
        assert_eq!(body["tools"][0]["mcp_server_name"], "mcp-example-com");
        assert!(body["tools"][0].get("name").is_none());
        assert!(body["tools"][0].get("description").is_none());
    }

    #[test]
    fn request_body_gives_mcp_servers_on_the_same_host_distinct_names() {
        let agent = test_agent_with_tools(vec![
            mcp_tool("https://mcp.example.com/finance", None),
            mcp_tool("https://mcp.example.com/weather", None),
        ]);

        let body: serde_json::Value =
            serde_json::from_slice(&Anthropic::request_body(&agent, &test_prompt("hi"))).unwrap();

        assert_eq!(body["mcp_servers"][0]["name"], "mcp-example-com");
        assert_eq!(body["mcp_servers"][1]["name"], "mcp-example-com-2");
        assert_eq!(body["tools"][0]["mcp_server_name"], "mcp-example-com");
        assert_eq!(body["tools"][1]["mcp_server_name"], "mcp-example-com-2");
    }

    #[test]
    fn request_body_omits_the_token_when_the_mcp_server_has_no_authentication_value() {
        let agent = test_agent_with_tools(vec![mcp_tool("https://mcp.example.com/sse", None)]);

        let body: serde_json::Value =
            serde_json::from_slice(&Anthropic::request_body(&agent, &test_prompt("hi"))).unwrap();

        assert!(body["mcp_servers"][0].get("authorization_token").is_none());
    }

    #[test]
    fn request_body_separates_http_search_and_mcp_tools() {
        let agent = test_agent_with_tools(vec![
            http_search_tool(),
            mcp_tool("https://mcp.example.com/sse", Some("secret-token")),
        ]);

        let body: serde_json::Value =
            serde_json::from_slice(&Anthropic::request_body(&agent, &test_prompt("hi"))).unwrap();

        assert_eq!(body["tools"].as_array().unwrap().len(), 2);
        assert_eq!(body["tools"][0]["type"], WEB_SEARCH_TOOL_TYPE);
        assert_eq!(body["tools"][1]["type"], "mcp_toolset");
        assert_eq!(body["mcp_servers"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn request_body_leaves_every_mcp_tool_available_without_a_selection() {
        let agent = test_agent_with_tools(vec![mcp_tool("https://mcp.example.com/sse", None)]);

        let body: serde_json::Value =
            serde_json::from_slice(&Anthropic::request_body(&agent, &test_prompt("hi"))).unwrap();

        assert!(body["tools"][0].get("default_config").is_none());
        assert!(body["tools"][0].get("configs").is_none());
    }

    #[test]
    fn request_body_enables_only_the_mcp_tools_the_builder_picked() {
        let agent = test_agent_with_tools(vec![mcp_tool_limited_to(
            "https://mcp.example.com/sse",
            &["get_customer", "list_orders"],
        )]);

        let body: serde_json::Value =
            serde_json::from_slice(&Anthropic::request_body(&agent, &test_prompt("hi"))).unwrap();

        assert_eq!(body["tools"][0]["default_config"]["enabled"], false);
        assert_eq!(body["tools"][0]["configs"]["get_customer"]["enabled"], true);
        assert_eq!(body["tools"][0]["configs"]["list_orders"]["enabled"], true);
        assert_eq!(body["tools"][0]["configs"].as_object().unwrap().len(), 2);
    }

    #[test]
    fn request_body_leaves_out_an_mcp_server_with_an_empty_selection() {
        let agent = test_agent_with_tools(vec![
            mcp_tool_limited_to("https://mcp.example.com/sse", &[]),
            http_search_tool(),
        ]);

        let body: serde_json::Value =
            serde_json::from_slice(&Anthropic::request_body(&agent, &test_prompt("hi"))).unwrap();

        assert_eq!(body["tools"].as_array().unwrap().len(), 1);
        assert_eq!(body["tools"][0]["type"], WEB_SEARCH_TOOL_TYPE);
        assert!(body.get("mcp_servers").is_none());
        assert!(!uses_mcp(&agent));
    }

    #[test]
    fn request_body_keeps_the_selections_of_two_mcp_servers_apart() {
        let agent = test_agent_with_tools(vec![
            mcp_tool_limited_to("https://mcp.example.com/finance", &["get_quote"]),
            mcp_tool_limited_to("https://mcp.example.com/weather", &["get_forecast"]),
        ]);

        let body: serde_json::Value =
            serde_json::from_slice(&Anthropic::request_body(&agent, &test_prompt("hi"))).unwrap();

        assert_eq!(body["tools"][0]["configs"]["get_quote"]["enabled"], true);
        assert!(body["tools"][0]["configs"].get("get_forecast").is_none());
        assert_eq!(body["tools"][1]["configs"]["get_forecast"]["enabled"], true);
        assert!(body["tools"][1]["configs"].get("get_quote").is_none());
    }

    #[test]
    fn mcp_server_name_sanitizes_the_url_host() {
        assert_eq!(
            crate::providers::mcp_server_name("https://MCP.Example.com:8080/sse"),
            "MCP-Example-com-8080"
        );
        assert_eq!(
            crate::providers::mcp_server_name("http://localhost:3000"),
            "localhost-3000"
        );
        assert_eq!(crate::providers::mcp_server_name("not a url"), "not-a-url");
        assert_eq!(crate::providers::mcp_server_name("///"), "mcp-server");
    }

    #[test]
    fn extract_joins_consecutive_text_blocks() {
        let response = serde_json::json!({
            "content": [
                { "type": "text", "text": "Hello " },
                { "type": "text", "text": "world" }
            ]
        })
        .to_string();

        assert_eq!(
            MessagesResponse::extract(response.as_bytes()).unwrap().text,
            "Hello\n\nworld"
        );
    }

    #[test]
    fn extract_skips_blocks_that_are_not_text() {
        let response = serde_json::json!({
            "content": [
                { "type": "thinking", "thinking": "hmm" },
                { "type": "text", "text": "answer" }
            ]
        })
        .to_string();

        assert_eq!(
            MessagesResponse::extract(response.as_bytes()).unwrap().text,
            "answer"
        );
    }

    #[test]
    fn extract_rejects_a_response_carrying_no_text() {
        let response = serde_json::json!({ "content": [] }).to_string();

        assert!(MessagesResponse::extract(response.as_bytes()).is_err());
    }

    #[test]
    fn extract_passes_the_usage_through() {
        let usage = serde_json::json!({
            "input_tokens": 12,
            "output_tokens": 3,
            "cache_read_input_tokens": 0,
            "server_tool_use": { "web_search_requests": 1 }
        });
        let response = serde_json::json!({
            "content": [{ "type": "text", "text": "answer" }],
            "usage": usage
        })
        .to_string();

        assert_eq!(
            MessagesResponse::extract(response.as_bytes())
                .unwrap()
                .usage,
            usage
        );
    }

    #[test]
    fn extract_reports_an_empty_usage_when_the_response_has_none() {
        assert_eq!(
            MessagesResponse::extract(anthropic_text_response("answer").as_bytes())
                .unwrap()
                .usage,
            serde_json::json!({})
        );
    }

    #[test]
    fn extract_rejects_malformed_json() {
        assert!(MessagesResponse::extract(b"not json").is_err());
    }

    #[tokio::test]
    async fn complete_returns_the_assistant_text() {
        let client = MockHttpClient::new(vec![(200, anthropic_text_response("42"))]);

        let text = Anthropic
            .complete(&client, &test_provider(), &test_prompt("meaning?"))
            .await
            .unwrap();

        assert_eq!(text.text, "42");
    }

    #[tokio::test]
    async fn complete_posts_to_the_messages_endpoint_of_the_provider_url() {
        let client = MockHttpClient::new(vec![(200, anthropic_text_response("ok"))]);
        let mut agent = test_provider();
        agent.url = "http://localhost:4010/v1".to_string();

        Anthropic
            .complete(&client, &agent, &test_prompt("hi"))
            .await
            .unwrap();

        assert_eq!(
            client.requests()[0].url,
            "http://localhost:4010/v1/messages"
        );
    }

    #[tokio::test]
    async fn complete_sends_the_anthropic_headers() {
        let client = MockHttpClient::new(vec![(200, anthropic_text_response("ok"))]);

        Anthropic
            .complete(&client, &test_provider(), &test_prompt("hi"))
            .await
            .unwrap();

        let request = &client.requests()[0];
        assert_eq!(request.header("x-api-key"), Some("test-key"));
        assert_eq!(request.header("anthropic-version"), Some(API_VERSION));
        assert_eq!(request.header("content-type"), Some("application/json"));
        assert_eq!(request.json()["model"], "claude-sonnet-5");
        assert_eq!(request.header("anthropic-beta"), None);
    }

    #[tokio::test]
    async fn complete_sends_the_mcp_beta_header_when_an_mcp_tool_is_configured() {
        let client = MockHttpClient::new(vec![(200, anthropic_text_response("ok"))]);
        let agent = test_agent_with_tools(vec![mcp_tool("https://mcp.example.com/sse", None)]);

        Anthropic
            .complete(&client, &agent, &test_prompt("hi"))
            .await
            .unwrap();

        assert_eq!(
            client.requests()[0].header("anthropic-beta"),
            Some(MCP_CLIENT_BETA)
        );
    }

    #[tokio::test]
    async fn complete_reports_a_non_success_status_with_a_redacted_error() {
        let client = MockHttpClient::new(vec![(
            401,
            r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key test-key"}}"#
                .to_string(),
        )]);

        let error = crate::run(&client, &test_provider(), &test_prompt("hi"))
            .await
            .unwrap_err();

        assert_eq!(
            error,
            "Anthropic returned status 401 (authentication_error): invalid x-api-key [redacted]"
        );
    }

    #[test]
    fn status_error_includes_the_error_type_and_message() {
        let body = r#"{"type":"error","error":{"type":"rate_limit_error","message":"Number of requests has exceeded your rate limit."}}"#;

        assert_eq!(
            MessagesResponse::handle(429, body.as_bytes()).unwrap_err(),
            "Anthropic returned status 429 (rate_limit_error): Number of requests has exceeded your rate limit."
        );
    }

    #[test]
    fn status_error_does_not_echo_a_body_that_is_not_an_anthropic_error() {
        assert_eq!(
            MessagesResponse::handle(502, b"<html>Bad gateway</html>").unwrap_err(),
            "Anthropic returned status 502"
        );
    }

    #[test]
    fn extract_reports_a_refusal() {
        let response = serde_json::json!({ "content": [], "stop_reason": "refusal" }).to_string();

        assert_eq!(
            MessagesResponse::extract(response.as_bytes()).unwrap_err(),
            "Anthropic refused the request"
        );
    }

    #[test]
    fn extract_names_the_tools_requested_before_any_text() {
        let response = serde_json::json!({
            "content": [
                { "type": "thinking", "thinking": "", "signature": "sig" },
                {
                    "type": "mcp_tool_use",
                    "id": "mcptoolu_code",
                    "name": "get_public_code",
                    "server_name": "mcp-example-com",
                    "input": {}
                },
                { "type": "tool_use", "id": "toolu_echo", "name": "mcp-example-com_get_echo", "input": {} },
                { "type": "tool_use", "id": "toolu_secret", "name": "mcp-example-com_get_secret", "input": {} }
            ],
            "stop_reason": "tool_use"
        })
        .to_string();

        assert_eq!(
            MessagesResponse::extract(response.as_bytes()).unwrap_err(),
            "Anthropic response was incomplete (tool_use) before producing any text. \
             Requested tools: mcp-example-com_get_echo, mcp-example-com_get_secret"
        );
    }

    #[test]
    fn extract_reports_a_response_that_ran_out_of_tokens() {
        let response = serde_json::json!({
            "content": [{ "type": "thinking", "thinking": "hmm" }],
            "stop_reason": "max_tokens"
        })
        .to_string();

        assert_eq!(
            MessagesResponse::extract(response.as_bytes()).unwrap_err(),
            "Anthropic response was incomplete (max_tokens) before producing any text"
        );
    }

    #[test]
    fn extract_keeps_partial_text_from_a_response_that_ran_out_of_tokens() {
        let response = serde_json::json!({
            "content": [{ "type": "text", "text": "partial" }],
            "stop_reason": "max_tokens"
        })
        .to_string();

        assert_eq!(
            MessagesResponse::extract(response.as_bytes()).unwrap().text,
            "partial"
        );
    }
}
