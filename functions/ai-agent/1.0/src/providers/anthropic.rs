use serde::Deserialize;

use crate::betty_blocks_types::types::types::{BettyAiAgent, BettyAiTool, McpOptions};
use crate::http::HttpClient;
use crate::providers::{Prompt, Provider};

const DEFAULT_MAX_TOKENS: u32 = 4096;
const API_VERSION: &str = "2023-06-01";
const MESSAGES_PATH: &str = "messages";
const WEB_SEARCH_TOOL_TYPE: &str = "web_search_20260318";
const MCP_SERVER_NAME_MAX_LEN: usize = 64;
const MCP_CLIENT_BETA: &str = "mcp-client-2025-11-20";
const API_KEY_AUTH: &str = "api_key";

pub(crate) struct Anthropic;

#[derive(Debug, Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
}

impl Provider for Anthropic {
    async fn complete(
        &self,
        client: &impl HttpClient,
        agent: &BettyAiAgent,
        prompt: &Prompt,
    ) -> Result<String, String> {
        if agent.api_key.is_empty() {
            return Err("No API key configured for agent: anthropic".to_string());
        }

        if agent.url.is_empty() {
            return Err("No URL configured for agent: anthropic".to_string());
        }

        let mut headers = vec![
            ("x-api-key", agent.api_key.clone()),
            ("anthropic-version", API_VERSION.to_string()),
            ("content-type", "application/json".to_string()),
        ];
        if uses_mcp(agent) {
            headers.push(("anthropic-beta", MCP_CLIENT_BETA.to_string()));
        }

        let (status, response) = client
            .post_json(
                &messages_url(&agent.url),
                headers,
                request_body(agent, prompt),
            )
            .await?;

        if status != 200 {
            return Err(format!(
                "Anthropic returned status {status}: {}",
                String::from_utf8_lossy(&response)
            ));
        }

        let pretty = serde_json::from_slice::<serde_json::Value>(&response)
            .map(|value| serde_json::to_string_pretty(&value).unwrap_or_default())
            .unwrap_or_else(|_| String::from_utf8_lossy(&response).into_owned());
        eprintln!("Anthropic response:\n{pretty}");

        extract_text(&response)
    }
}

fn messages_url(base_url: &str) -> String {
    format!("{}/{MESSAGES_PATH}", base_url.trim_end_matches('/'))
}

fn uses_mcp(agent: &BettyAiAgent) -> bool {
    agent
        .tools
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .any(|tool| matches!(tool, BettyAiTool::Mcp(_)))
}

fn request_body(agent: &BettyAiAgent, prompt: &Prompt) -> Vec<u8> {
    let mut body = serde_json::json!({
        "model": agent.ai_model_name,
        "max_tokens": prompt.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        "system": prompt.instructions,
        "messages": [{ "role": "user", "content": prompt.message }],
    });

    let tools = agent.tools.as_deref().unwrap_or(&[]);
    if tools.is_empty() {
        return body.to_string().into_bytes();
    }

    let mut tools_body = Vec::new();
    let mut mcp_servers = Vec::new();

    for tool in tools {
        match tool {
            BettyAiTool::HttpSearch(_) => tools_body.push(serde_json::json!({
                "type": WEB_SEARCH_TOOL_TYPE,
                "name": "web_search",
                "allowed_callers": ["direct"],
            })),
            BettyAiTool::Mcp(options) => {
                let name = unique_mcp_server_name(&options.url, &mcp_servers);
                tools_body.push(serde_json::json!({
                    "type": "mcp_toolset",
                    "mcp_server_name": name,
                }));
                mcp_servers.push(mcp_server(options, name))
            }
        }
    }

    if !tools_body.is_empty() {
        body["tools"] = tools_body.into();
    }
    if !mcp_servers.is_empty() {
        body["mcp_servers"] = mcp_servers.into();
    }
    body.to_string().into_bytes()
}

fn mcp_server(options: &McpOptions, name: String) -> serde_json::Value {
    let mut server = serde_json::json!({
        "type": "url",
        "url": options.url,
        "name": name,
    });

    if let Some(value) = options
        .authentication
        .value
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        if options.authentication.kind == API_KEY_AUTH {
            server["custom_headers"] = serde_json::json!({ "X-API-Key": value });
        } else {
            server["authorization_token"] = value.into();
        }
    }

    server
}

fn unique_mcp_server_name(url: &str, servers: &[serde_json::Value]) -> String {
    let base = mcp_server_name(url);
    let taken = |name: &str| servers.iter().any(|server| server["name"] == name);

    if !taken(&base) {
        return base;
    }

    (2..)
        .map(|n| {
            let suffix = format!("-{n}");
            let stem: String = base
                .chars()
                .take(MCP_SERVER_NAME_MAX_LEN - suffix.len())
                .collect();
            format!("{stem}{suffix}")
        })
        .find(|name| !taken(name))
        .expect("an unused suffix always exists")
}

fn mcp_server_name(url: &str) -> String {
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let host = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(without_scheme);
    let sanitized: String = host
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();

    let name: String = sanitized
        .trim_matches('-')
        .chars()
        .take(MCP_SERVER_NAME_MAX_LEN)
        .collect();

    if name.is_empty() {
        "mcp-server".to_string()
    } else {
        name
    }
}

fn extract_text(response: &[u8]) -> Result<String, String> {
    let parsed: MessagesResponse = serde_json::from_slice(response)
        .map_err(|e| format!("Failed to parse Anthropic response: {e}"))?;

    let text: String = parsed
        .content
        .into_iter()
        .filter(|block| block.block_type == "text")
        .filter_map(|block| block.text)
        .map(|text| text.trim().to_string())
        .collect::<Vec<_>>()
        .join("\n\n");

    if text.is_empty() {
        return Err("Anthropic response contained no text content".to_string());
    }

    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::betty_blocks_types::types::types::AuthenticationConfig;
    use crate::test_helpers::{
        MockHttpClient, anthropic_text_response, http_search_tool, mcp_tool, test_agent_with_tools,
        test_prompt, test_provider,
    };

    #[test]
    fn messages_url_appends_the_endpoint_to_the_base_url() {
        assert_eq!(
            messages_url("https://api.anthropic.com/v1"),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn messages_url_does_not_double_the_separator() {
        assert_eq!(
            messages_url("https://api.anthropic.com/v1/"),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn request_body_carries_model_prompts_and_default_max_tokens() {
        let body: serde_json::Value = serde_json::from_slice(&request_body(
            &test_provider(),
            &test_prompt("Summarise this"),
        ))
        .unwrap();

        assert_eq!(body["model"], "claude-sonnet-5");
        assert_eq!(body["max_tokens"], DEFAULT_MAX_TOKENS);
        assert_eq!(body["system"], "You are helpful.");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "Summarise this");
    }

    #[test]
    fn request_body_honours_an_explicit_max_tokens() {
        let mut prompt = test_prompt("hi");
        prompt.max_tokens = Some(256);

        let body: serde_json::Value =
            serde_json::from_slice(&request_body(&test_provider(), &prompt)).unwrap();

        assert_eq!(body["max_tokens"], 256);
    }

    #[test]
    fn request_body_omits_the_tool_fields_without_tools() {
        let body: serde_json::Value =
            serde_json::from_slice(&request_body(&test_provider(), &test_prompt("hi"))).unwrap();

        assert!(body.get("tools").is_none());
        assert!(body.get("mcp_servers").is_none());
    }

    #[test]
    fn request_body_enables_web_search_for_an_http_search_tool() {
        let agent = test_agent_with_tools(vec![http_search_tool()]);

        let body: serde_json::Value =
            serde_json::from_slice(&request_body(&agent, &test_prompt("latest news"))).unwrap();

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
            serde_json::from_slice(&request_body(&agent, &test_prompt("hi"))).unwrap();

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
    fn request_body_sends_an_api_key_as_a_custom_header() {
        let agent = test_agent_with_tools(vec![BettyAiTool::Mcp(McpOptions {
            description: None,
            url: "https://mcp.example.com/sse".to_string(),
            authentication: AuthenticationConfig {
                kind: "api_key".to_string(),
                value: Some("my-api-key".to_string()),
            },
        })]);

        let body: serde_json::Value =
            serde_json::from_slice(&request_body(&agent, &test_prompt("hi"))).unwrap();

        assert_eq!(body["mcp_servers"][0]["custom_headers"]["X-API-Key"], "my-api-key");
        assert!(body["mcp_servers"][0].get("authorization_token").is_none());
    }

    #[test]
    fn request_body_references_each_mcp_server_from_an_mcp_toolset() {
        let agent = test_agent_with_tools(vec![mcp_tool("https://mcp.example.com/sse", None)]);

        let body: serde_json::Value =
            serde_json::from_slice(&request_body(&agent, &test_prompt("hi"))).unwrap();

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
            serde_json::from_slice(&request_body(&agent, &test_prompt("hi"))).unwrap();

        assert_eq!(body["mcp_servers"][0]["name"], "mcp-example-com");
        assert_eq!(body["mcp_servers"][1]["name"], "mcp-example-com-2");
        assert_eq!(body["tools"][0]["mcp_server_name"], "mcp-example-com");
        assert_eq!(body["tools"][1]["mcp_server_name"], "mcp-example-com-2");
    }

    #[test]
    fn request_body_omits_the_token_when_the_mcp_server_has_no_authentication_value() {
        let agent = test_agent_with_tools(vec![mcp_tool("https://mcp.example.com/sse", None)]);

        let body: serde_json::Value =
            serde_json::from_slice(&request_body(&agent, &test_prompt("hi"))).unwrap();

        assert!(body["mcp_servers"][0].get("authorization_token").is_none());
    }

    #[test]
    fn request_body_separates_http_search_and_mcp_tools() {
        let agent = test_agent_with_tools(vec![
            http_search_tool(),
            mcp_tool("https://mcp.example.com/sse", Some("secret-token")),
        ]);

        let body: serde_json::Value =
            serde_json::from_slice(&request_body(&agent, &test_prompt("hi"))).unwrap();

        assert_eq!(body["tools"].as_array().unwrap().len(), 2);
        assert_eq!(body["tools"][0]["type"], WEB_SEARCH_TOOL_TYPE);
        assert_eq!(body["tools"][1]["type"], "mcp_toolset");
        assert_eq!(body["mcp_servers"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn mcp_server_name_sanitizes_the_url_host() {
        assert_eq!(
            mcp_server_name("https://MCP.Example.com:8080/sse"),
            "MCP-Example-com-8080"
        );
        assert_eq!(mcp_server_name("http://localhost:3000"), "localhost-3000");
        assert_eq!(mcp_server_name("not a url"), "not-a-url");
        assert_eq!(mcp_server_name("///"), "mcp-server");
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

        assert_eq!(extract_text(response.as_bytes()).unwrap(), "Hello\n\nworld");
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
        let client = MockHttpClient::new(vec![(200, anthropic_text_response("42"))]);

        let text = Anthropic
            .complete(&client, &test_provider(), &test_prompt("meaning?"))
            .await
            .unwrap();

        assert_eq!(text, "42");
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
    async fn complete_rejects_an_empty_api_key_without_calling_out() {
        let client = MockHttpClient::new(vec![(200, anthropic_text_response("unused"))]);
        let mut agent = test_provider();
        agent.api_key = String::new();

        assert!(
            Anthropic
                .complete(&client, &agent, &test_prompt("hi"))
                .await
                .is_err()
        );
        assert!(client.requests().is_empty());
    }

    #[tokio::test]
    async fn complete_rejects_an_empty_url_without_calling_out() {
        let client = MockHttpClient::new(vec![(200, anthropic_text_response("unused"))]);
        let mut agent = test_provider();
        agent.url = String::new();

        assert!(
            Anthropic
                .complete(&client, &agent, &test_prompt("hi"))
                .await
                .is_err()
        );
        assert!(client.requests().is_empty());
    }

    #[tokio::test]
    async fn complete_reports_a_non_success_status() {
        let client = MockHttpClient::new(vec![(429, "rate limited".to_string())]);

        let error = Anthropic
            .complete(&client, &test_provider(), &test_prompt("hi"))
            .await
            .unwrap_err();

        assert!(error.contains("429"));
    }
}
