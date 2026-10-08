use std::cell::RefCell;
use std::collections::VecDeque;

use crate::betty_blocks_types::types::types::{
    AuthenticationConfig, BettyAiAgent, BettyAiTool, HttpSearchOptions, McpOptions,
};
use crate::http::HttpClient;
use crate::providers::Prompt;

#[derive(Clone)]
pub(crate) struct RecordedRequest {
    pub(crate) url: String,
    pub(crate) headers: Vec<(&'static str, String)>,
    pub(crate) body: Vec<u8>,
}

impl RecordedRequest {
    pub(crate) fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).expect("request body was not valid JSON")
    }

    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.as_str())
    }
}

pub(crate) struct MockHttpClient {
    responses: RefCell<VecDeque<(u16, String)>>,
    requests: RefCell<Vec<RecordedRequest>>,
}

impl MockHttpClient {
    pub(crate) fn new(responses: Vec<(u16, String)>) -> Self {
        Self {
            responses: RefCell::new(responses.into()),
            requests: RefCell::new(Vec::new()),
        }
    }

    pub(crate) fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.borrow().clone()
    }
}

impl HttpClient for MockHttpClient {
    async fn post_json(
        &self,
        url: &str,
        headers: Vec<(&'static str, String)>,
        body: Vec<u8>,
    ) -> Result<(u16, Vec<u8>), String> {
        self.requests.borrow_mut().push(RecordedRequest {
            url: url.to_string(),
            headers,
            body,
        });

        let (status, response) = self
            .responses
            .borrow_mut()
            .pop_front()
            .ok_or_else(|| "MockHttpClient has no queued response".to_string())?;

        Ok((status, response.into_bytes()))
    }
}

pub(crate) fn anthropic_text_response(text: &str) -> String {
    serde_json::json!({ "content": [{ "type": "text", "text": text }] }).to_string()
}

pub(crate) fn test_provider() -> BettyAiAgent {
    BettyAiAgent {
        name: "anthropic".to_string(),
        ai_model_name: "claude-sonnet-5".to_string(),
        url: "https://api.anthropic.com/v1".to_string(),
        tools: None,
        api_key: "test-key".to_string(),
    }
}

pub(crate) fn test_agent_with_tools(tools: Vec<BettyAiTool>) -> BettyAiAgent {
    let mut agent = test_provider();
    agent.tools = Some(tools);
    agent
}

pub(crate) fn http_search_tool() -> BettyAiTool {
    BettyAiTool::HttpSearch(HttpSearchOptions { description: None })
}

pub(crate) fn mcp_tool(url: &str, token: Option<&str>) -> BettyAiTool {
    BettyAiTool::Mcp(McpOptions {
        description: None,
        url: url.to_string(),
        authentication: AuthenticationConfig {
            kind: "bearer".to_string(),
            value: token.map(|token| token.to_string()),
        },
    })
}

pub(crate) fn test_prompt(message: &str) -> Prompt {
    Prompt {
        instructions: "You are helpful.".to_string(),
        message: message.to_string(),
        max_tokens: None,
    }
}
