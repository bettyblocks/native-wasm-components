use std::cell::RefCell;

use wstd::http::Method;

use crate::http::HttpClient;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RecordedRequest {
    pub(crate) method: Method,
    pub(crate) url: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
}

pub(crate) struct MockHttpClient {
    response: Result<(u16, Vec<u8>), String>,
    requests: RefCell<Vec<RecordedRequest>>,
}

impl MockHttpClient {
    pub(crate) fn returning(status: u16, body: &str) -> Self {
        Self {
            response: Ok((status, body.as_bytes().to_vec())),
            requests: RefCell::new(Vec::new()),
        }
    }

    pub(crate) fn failing(message: &str) -> Self {
        Self {
            response: Err(message.to_string()),
            requests: RefCell::new(Vec::new()),
        }
    }

    pub(crate) fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.borrow().clone()
    }
}

impl HttpClient for MockHttpClient {
    async fn send_request(
        &self,
        method: Method,
        url: &str,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> Result<(u16, Vec<u8>), String> {
        self.requests.borrow_mut().push(RecordedRequest {
            method,
            url: url.to_string(),
            headers,
            body,
        });

        self.response.clone()
    }
}
