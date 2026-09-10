use std::cell::RefCell;
use std::collections::VecDeque;

use crate::exports::betty_blocks::ai_agent::ai_agent::{AiProvider, Input};
use crate::http::HttpClient;

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

pub(crate) fn test_input(message: &str) -> Input {
    Input {
        provider: AiProvider {
            provider: "anthropic".to_string(),
            model: "claude-sonnet-5".to_string(),
            api_key: "test-key".to_string(),
        },
        instructions: "You are helpful.".to_string(),
        message: message.to_string(),
        max_tokens: None,
    }
}
