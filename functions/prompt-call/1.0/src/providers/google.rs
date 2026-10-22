use serde::Deserialize;

use crate::betty_blocks_types::types::types::{BettyAiAgent, BettyAiTool};
use crate::http::HttpClient;
use crate::providers::{
    McpServer, Output, Prompt, Provider, ProviderRequest, ProviderResponse, active_tools,
    ensure_configured, null_as_empty,
};

const API_KEY_HEADER: &str = "x-goog-api-key";
const SUCCESSFUL_FINISH_REASON: &str = "STOP";

pub(crate) struct Google;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GenerateContentResponse {
    #[serde(default, deserialize_with = "null_as_empty")]
    candidates: Vec<Candidate>,
    prompt_feedback: Option<PromptFeedback>,
    usage_metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    content: Option<Content>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Content {
    #[serde(default, deserialize_with = "null_as_empty")]
    parts: Vec<Part>,
}

#[derive(Debug, Deserialize)]
struct Part {
    text: Option<String>,
    thought: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptFeedback {
    block_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    error: ErrorStatus,
}

#[derive(Debug, Deserialize)]
struct ErrorStatus {
    message: Option<String>,
    status: Option<String>,
}

impl Provider for Google {
    async fn complete(
        &self,
        client: &impl HttpClient,
        agent: &BettyAiAgent,
        prompt: &Prompt,
    ) -> Result<Output, String> {
        ensure_configured(agent)?;
        ensure_no_mcp_tools(agent)?;

        let headers = vec![
            (API_KEY_HEADER, agent.api_key.clone()),
            ("content-type", "application/json".to_string()),
        ];

        let (status, response) = client
            .post_json(
                &Self::endpoint_url(agent),
                headers,
                Self::request_body(agent, prompt),
            )
            .await?;

        GenerateContentResponse::handle(status, &response)
    }
}

fn ensure_no_mcp_tools(agent: &BettyAiAgent) -> Result<(), String> {
    if active_tools(agent).any(|tool| matches!(tool, BettyAiTool::Mcp(_))) {
        return Err(format!(
            "Gemini does not support server-side MCP tools, configured on agent: {}",
            agent.name
        ));
    }

    Ok(())
}

impl ProviderRequest for Google {
    type Response = GenerateContentResponse;

    const PATH: &'static str = "generateContent";

    fn endpoint_url(agent: &BettyAiAgent) -> String {
        format!(
            "{}/models/{}:{}",
            agent.url.trim_end_matches('/'),
            agent.ai_model_name,
            Self::PATH
        )
    }

    fn base_body(_agent: &BettyAiAgent, prompt: &Prompt) -> serde_json::Value {
        serde_json::json!({
            "contents": [{ "role": "user", "parts": [{ "text": prompt.message }] }],
            "systemInstruction": { "parts": [{ "text": prompt.instructions }] },
            "generationConfig": { "maxOutputTokens": prompt.max_tokens() },
        })
    }

    fn web_search_tool() -> serde_json::Value {
        serde_json::json!({ "googleSearch": {} })
    }

    fn mcp_tool(_server: &McpServer) -> Option<serde_json::Value> {
        unreachable!("ensure_no_mcp_tools rejects MCP tools before the request body is built")
    }
}

impl GenerateContentResponse {
    // Candidates are alternative answers, not segments of one, so only the first is used.
    fn answer(&self) -> Option<&Candidate> {
        self.candidates.first()
    }

    fn blocked_reason(&self) -> Option<&str> {
        self.prompt_feedback.as_ref()?.block_reason.as_deref()
    }

    fn unsuccessful_finish_reason(&self) -> Option<&str> {
        self.answer()?
            .finish_reason
            .as_deref()
            .filter(|reason| *reason != SUCCESSFUL_FINISH_REASON)
    }
}

impl ErrorStatus {
    fn describe(&self) -> String {
        let kind = self
            .status
            .as_deref()
            .map(|status| format!(" ({status})"))
            .unwrap_or_default();
        let message = self
            .message
            .as_deref()
            .filter(|message| !message.is_empty())
            .map(|message| format!(": {message}"))
            .unwrap_or_default();

        format!("{kind}{message}")
    }
}

impl ProviderResponse for GenerateContentResponse {
    const PROVIDER: &'static str = "Google";

    fn text_parts(&self) -> Vec<&str> {
        self.answer()
            .and_then(|candidate| candidate.content.as_ref())
            .map(|content| {
                content
                    .parts
                    .iter()
                    .filter(|part| part.thought != Some(true))
                    .filter_map(|part| part.text.as_deref())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn usage(&self) -> Option<&serde_json::Value> {
        self.usage_metadata.as_ref()
    }

    fn ensure_succeeded(&self) -> Result<(), String> {
        match self.blocked_reason() {
            Some(reason) => Err(format!("Google blocked the prompt ({reason})")),
            None => Ok(()),
        }
    }

    fn no_text_error(&self) -> String {
        match self.unsuccessful_finish_reason() {
            Some(reason) => format!("Google stopped before producing any text ({reason})"),
            None => "Google response contained no text content".to_string(),
        }
    }

    fn status_error(status: u16, response: &[u8]) -> String {
        let reported = format!("{} returned status {status}", Self::PROVIDER);

        match serde_json::from_slice::<ErrorBody>(response) {
            Ok(body) => format!("{reported}{}", body.error.describe()),
            Err(_) => reported,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{
        MockHttpClient, google_agent, google_text_response, http_search_tool,
        mcp_tool as betty_mcp_tool, mcp_tool_limited_to, test_prompt,
    };

    fn agent_with_tools(tools: Vec<BettyAiTool>) -> BettyAiAgent {
        let mut agent = google_agent();
        agent.tools = Some(tools);
        agent
    }

    fn body_of(agent: &BettyAiAgent, prompt: &Prompt) -> serde_json::Value {
        serde_json::from_slice(&Google::request_body(agent, prompt)).unwrap()
    }

    #[test]
    fn request_body_carries_the_prompt_and_default_max_tokens() {
        let body = body_of(&google_agent(), &test_prompt("Summarise this"));

        assert_eq!(
            body["contents"],
            serde_json::json!([{ "role": "user", "parts": [{ "text": "Summarise this" }] }])
        );
        assert_eq!(
            body["systemInstruction"],
            serde_json::json!({ "parts": [{ "text": "You are helpful." }] })
        );
        assert_eq!(
            body["generationConfig"]["maxOutputTokens"],
            crate::providers::DEFAULT_MAX_TOKENS
        );
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn request_body_leaves_the_model_to_the_endpoint_url() {
        assert!(
            body_of(&google_agent(), &test_prompt("hi"))
                .get("model")
                .is_none()
        );
    }

    #[test]
    fn request_body_honours_an_explicit_max_tokens() {
        let mut prompt = test_prompt("hi");
        prompt.max_tokens = Some(256);

        assert_eq!(
            body_of(&google_agent(), &prompt)["generationConfig"]["maxOutputTokens"],
            256
        );
    }

    #[test]
    fn request_body_enables_google_search_for_an_http_search_tool() {
        let body = body_of(
            &agent_with_tools(vec![http_search_tool()]),
            &test_prompt("hi"),
        );

        assert_eq!(body["tools"], serde_json::json!([{ "googleSearch": {} }]));
    }

    #[test]
    fn extract_text_joins_the_parts_of_the_first_candidate_only() {
        let response = serde_json::json!({
            "candidates": [
                {
                    "content": { "role": "model", "parts": [{ "text": "Hello " }, { "text": "world" }] },
                    "finishReason": "STOP"
                },
                {
                    "content": { "role": "model", "parts": [{ "text": "a different answer" }] },
                    "finishReason": "STOP"
                }
            ]
        })
        .to_string();

        assert_eq!(
            GenerateContentResponse::extract(response.as_bytes())
                .unwrap()
                .text,
            "Hello\n\nworld"
        );
    }

    #[test]
    fn extract_text_leaves_out_the_models_thinking() {
        let response = serde_json::json!({
            "candidates": [{
                "content": { "parts": [
                    { "text": "First I should check the docs.", "thought": true },
                    { "text": "The answer is 42." }
                ]},
                "finishReason": "STOP"
            }]
        })
        .to_string();

        assert_eq!(
            GenerateContentResponse::extract(response.as_bytes())
                .unwrap()
                .text,
            "The answer is 42."
        );
    }

    #[test]
    fn extract_text_reports_a_blocked_prompt() {
        let response =
            serde_json::json!({ "promptFeedback": { "blockReason": "SAFETY" } }).to_string();

        assert_eq!(
            GenerateContentResponse::extract(response.as_bytes()).unwrap_err(),
            "Google blocked the prompt (SAFETY)"
        );
    }

    #[test]
    fn extract_text_reports_an_unsuccessful_finish_reason() {
        let response = serde_json::json!({
            "candidates": [{ "content": { "parts": [] }, "finishReason": "MAX_TOKENS" }]
        })
        .to_string();

        assert_eq!(
            GenerateContentResponse::extract(response.as_bytes()).unwrap_err(),
            "Google stopped before producing any text (MAX_TOKENS)"
        );
    }

    #[test]
    fn extract_text_keeps_partial_text_from_a_truncated_response() {
        let response = serde_json::json!({
            "candidates": [{
                "content": { "parts": [{ "text": "partial" }] },
                "finishReason": "MAX_TOKENS"
            }]
        })
        .to_string();

        assert_eq!(
            GenerateContentResponse::extract(response.as_bytes())
                .unwrap()
                .text,
            "partial"
        );
    }

    #[test]
    fn extract_text_reads_null_candidates_as_no_text() {
        let response = r#"{ "candidates": null }"#;

        assert_eq!(
            GenerateContentResponse::extract(response.as_bytes()).unwrap_err(),
            "Google response contained no text content"
        );
    }

    #[test]
    fn extract_text_rejects_malformed_json() {
        assert!(GenerateContentResponse::extract(b"not json").is_err());
    }

    #[test]
    fn extract_passes_the_usage_metadata_through() {
        let usage = serde_json::json!({
            "promptTokenCount": 12,
            "candidatesTokenCount": 3,
            "thoughtsTokenCount": 40,
            "totalTokenCount": 55,
            "promptTokensDetails": [{ "modality": "TEXT", "tokenCount": 12 }]
        });
        let response = serde_json::json!({
            "candidates": [{
                "content": { "parts": [{ "text": "answer" }] },
                "finishReason": "STOP"
            }],
            "usageMetadata": usage
        })
        .to_string();

        assert_eq!(
            GenerateContentResponse::extract(response.as_bytes())
                .unwrap()
                .usage,
            usage
        );
    }

    #[test]
    fn extract_reports_an_empty_usage_when_the_response_has_none() {
        assert_eq!(
            GenerateContentResponse::extract(google_text_response("answer").as_bytes())
                .unwrap()
                .usage,
            serde_json::json!({})
        );
    }

    #[test]
    fn status_error_includes_the_status_and_message() {
        let body = r#"{"error":{"code":400,"message":"API key not valid. Please pass a valid API key.","status":"INVALID_ARGUMENT","details":[{"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"API_KEY_INVALID"}]}}"#;

        assert_eq!(
            GenerateContentResponse::handle(400, body.as_bytes()).unwrap_err(),
            "Google returned status 400 (INVALID_ARGUMENT): API key not valid. Please pass a valid API key."
        );
    }

    #[test]
    fn status_error_keeps_the_message_of_an_error_body_without_a_status() {
        let body = r#"{"error":{"code":"rate_limit_exceeded","message":"Too many requests."}}"#;

        assert_eq!(
            GenerateContentResponse::handle(429, body.as_bytes()).unwrap_err(),
            "Google returned status 429: Too many requests."
        );
    }

    #[test]
    fn status_error_does_not_echo_a_body_that_is_not_a_google_error() {
        assert_eq!(
            GenerateContentResponse::handle(502, b"<html>Bad gateway</html>").unwrap_err(),
            "Google returned status 502"
        );
    }

    #[tokio::test]
    async fn complete_posts_to_the_model_endpoint_with_the_api_key_header() {
        let client = MockHttpClient::new(vec![(200, google_text_response("42"))]);

        let text = Google
            .complete(&client, &google_agent(), &test_prompt("meaning?"))
            .await
            .unwrap();

        let request = &client.requests()[0];
        assert_eq!(text.text, "42");
        assert_eq!(
            request.url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-pro-latest:generateContent"
        );
        assert_eq!(request.header("x-goog-api-key"), Some("test-key"));
        assert_eq!(request.header("content-type"), Some("application/json"));
    }

    #[tokio::test]
    async fn complete_rejects_an_mcp_tool_without_calling_out() {
        let client = MockHttpClient::new(vec![(200, google_text_response("unused"))]);
        let agent = agent_with_tools(vec![betty_mcp_tool("https://mcp.example.com/sse", None)]);

        assert_eq!(
            Google
                .complete(&client, &agent, &test_prompt("hi"))
                .await
                .unwrap_err(),
            "Gemini does not support server-side MCP tools, configured on agent: google"
        );
        assert!(client.requests().is_empty());
    }

    #[tokio::test]
    async fn complete_ignores_an_mcp_server_with_every_tool_disabled() {
        let client = MockHttpClient::new(vec![(200, google_text_response("42"))]);
        let agent = agent_with_tools(vec![
            mcp_tool_limited_to("https://mcp.example.com/sse", &[]),
            http_search_tool(),
        ]);

        let text = Google
            .complete(&client, &agent, &test_prompt("hi"))
            .await
            .unwrap();

        assert_eq!(text.text, "42");
        assert_eq!(
            client.requests()[0].json()["tools"],
            serde_json::json!([{ "googleSearch": {} }])
        );
    }
}
