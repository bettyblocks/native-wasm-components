use serde::Deserialize;

use crate::betty_blocks_types::types::types::{BettyAiAgent, BettyAiTool, McpOptions};
use crate::http::HttpClient;
use crate::providers::{Prompt, Provider, unique_mcp_server_name};

const DEFAULT_MAX_TOKENS: u32 = 4096;
const RESPONSES_PATH: &str = "responses";
const WEB_SEARCH_TOOL_TYPE: &str = "web_search";

pub(crate) struct OpenAi;

#[derive(Debug, Deserialize)]
struct ResponsesResponse {
    output: Vec<OutputItem>,
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
}

impl Provider for OpenAi {
    async fn complete(
        &self,
        client: &impl HttpClient,
        agent: &BettyAiAgent,
        prompt: &Prompt,
    ) -> Result<String, String> {
        if agent.api_key.is_empty() {
            return Err("No API key configured for agent: openai".to_string());
        }

        if agent.url.is_empty() {
            return Err("No URL configured for agent: openai".to_string());
        }

        let headers = vec![
            ("authorization", format!("Bearer {}", agent.api_key)),
            ("content-type", "application/json".to_string()),
        ];

        let (status, response) = client
            .post_json(
                &responses_url(&agent.url),
                headers,
                request_body(agent, prompt),
            )
            .await?;

        if status != 200 {
            return Err(format!("OpenAI returned status {status}"));
        }

        extract_text(&response)
    }
}

fn responses_url(base_url: &str) -> String {
    format!("{}/{RESPONSES_PATH}", base_url.trim_end_matches('/'))
}

fn request_body(agent: &BettyAiAgent, prompt: &Prompt) -> Vec<u8> {
    let mut body = serde_json::json!({
        "model": agent.ai_model_name,
        "max_output_tokens": prompt.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        "instructions": prompt.instructions,
        "input": prompt.message,
        "store": false,
    });

    let tools = agent.tools.as_deref().unwrap_or(&[]);
    if tools.is_empty() {
        return body.to_string().into_bytes();
    }

    let mut labels: Vec<String> = Vec::new();
    let tools_body: Vec<serde_json::Value> = tools
        .iter()
        .map(|tool| match tool {
            BettyAiTool::HttpSearch(_) => serde_json::json!({ "type": WEB_SEARCH_TOOL_TYPE }),
            BettyAiTool::Mcp(options) => {
                let taken: Vec<&str> = labels.iter().map(String::as_str).collect();
                let label = unique_mcp_server_name(&options.url, &taken);
                let tool = mcp_tool(options, &label);
                labels.push(label);
                tool
            }
        })
        .collect();

    body["tools"] = tools_body.into();
    body.to_string().into_bytes()
}

fn mcp_tool(options: &McpOptions, label: &str) -> serde_json::Value {
    let mut tool = serde_json::json!({
        "type": "mcp",
        "server_label": label,
        "server_url": options.url,
        "require_approval": "never",
    });

    if let Some(value) = options
        .authentication
        .value
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        tool["authorization"] = value.into();
    }

    tool
}

fn extract_text(response: &[u8]) -> Result<String, String> {
    let parsed: ResponsesResponse = serde_json::from_slice(response)
        .map_err(|e| format!("Failed to parse OpenAI response: {e}"))?;

    let text: String = parsed
        .output
        .into_iter()
        .filter(|item| item.item_type == "message")
        .flat_map(|item| item.content)
        .filter(|part| part.part_type == "output_text")
        .filter_map(|part| part.text)
        .map(|text| text.trim().to_string())
        .collect::<Vec<_>>()
        .join("\n\n");

    if text.is_empty() {
        return Err("OpenAI response contained no text content".to_string());
    }

    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{
        MockHttpClient, http_search_tool, mcp_tool as betty_mcp_tool, openai_agent,
        openai_text_response, test_prompt,
    };

    fn agent_with_tools(tools: Vec<BettyAiTool>) -> BettyAiAgent {
        let mut agent = openai_agent();
        agent.tools = Some(tools);
        agent
    }

    fn body_of(agent: &BettyAiAgent, prompt: &Prompt) -> serde_json::Value {
        serde_json::from_slice(&request_body(agent, prompt)).unwrap()
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

        assert_eq!(extract_text(response.as_bytes()).unwrap(), "Hello\n\nworld");
    }

    #[test]
    fn extract_text_rejects_a_response_carrying_no_text() {
        let response = serde_json::json!({ "output": [] }).to_string();

        assert!(extract_text(response.as_bytes()).is_err());
    }

    #[test]
    fn extract_text_rejects_malformed_json() {
        assert!(extract_text(b"not json").is_err());
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
    async fn complete_reports_a_non_success_status_without_the_body() {
        let client = MockHttpClient::new(vec![(
            401,
            r#"{"error":{"message":"Incorrect API key provided: sk-...abcd"}}"#.to_string(),
        )]);

        let error = OpenAi
            .complete(&client, &openai_agent(), &test_prompt("hi"))
            .await
            .unwrap_err();

        assert_eq!(error, "OpenAI returned status 401");
    }
}
