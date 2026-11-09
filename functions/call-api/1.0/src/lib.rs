mod http;
#[cfg(test)]
mod test_helpers;

use wstd::http::{Client, Method};

use crate::betty_blocks_types::types::types::{BettyHttpMethod, BettySoftwareApi};
use crate::exports::betty_blocks::call_api::call_api;
use crate::http::HttpClient;

wit_bindgen::generate!({ generate_all });

struct Component;

impl call_api::Guest for Component {
    fn call_api(api: BettySoftwareApi) -> Result<String, String> {
        wstd::runtime::block_on(call(&Client::new(), &api))
    }
}

async fn call(client: &impl HttpClient, api: &BettySoftwareApi) -> Result<String, String> {
    validate_api(api)?;

    let (status, body) = client
        .send_request(
            to_http_method(api.method),
            &join_url(&api.base_url, &api.path),
            Vec::new(),
            Vec::new(),
        )
        .await
        .map_err(|error| format!("The API request failed: {error}"))?;

    if !(200..300).contains(&status) {
        return Err(format!("The API responded with status {status}"));
    }

    Ok(to_json_string(&body))
}

fn validate_api(api: &BettySoftwareApi) -> Result<(), String> {
    if api.base_url.trim().is_empty() {
        return Err("The API has no base url".to_string());
    }
    if api.path.trim().is_empty() {
        return Err("The API has no path".to_string());
    }

    Ok(())
}

/// Joins a base url and a path with exactly one slash between them.
fn join_url(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

fn to_http_method(method: BettyHttpMethod) -> Method {
    match method {
        BettyHttpMethod::Get => Method::GET,
        BettyHttpMethod::Post => Method::POST,
        BettyHttpMethod::Put => Method::PUT,
        BettyHttpMethod::Patch => Method::PATCH,
        BettyHttpMethod::Delete => Method::DELETE,
        BettyHttpMethod::Head => Method::HEAD,
        BettyHttpMethod::Options => Method::OPTIONS,
    }
}

/// A body that is not JSON is returned as a JSON string, so the output is always valid JSON.
fn to_json_string(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body).into_owned();

    serde_json::from_str::<serde_json::Value>(&text)
        .unwrap_or(serde_json::Value::String(text))
        .to_string()
}

export!(Component);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{MockHttpClient, RecordedRequest};

    fn api(base_url: &str, path: &str) -> BettySoftwareApi {
        BettySoftwareApi {
            software_name: "Exact".to_string(),
            api_name: "Get users".to_string(),
            base_url: base_url.to_string(),
            path: path.to_string(),
            method: BettyHttpMethod::Get,
        }
    }

    #[tokio::test]
    async fn sends_the_request_to_the_joined_url() {
        let client = MockHttpClient::returning(200, "{}");

        call(&client, &api("https://api.example.com/", "/users"))
            .await
            .unwrap();

        assert_eq!(
            client.requests(),
            vec![RecordedRequest {
                method: Method::GET,
                url: "https://api.example.com/users".to_string(),
                headers: Vec::new(),
                body: Vec::new(),
            }]
        );
    }

    #[tokio::test]
    async fn sends_the_method_of_the_api() {
        let client = MockHttpClient::returning(200, "{}");
        let api = BettySoftwareApi {
            method: BettyHttpMethod::Delete,
            ..api("https://api.example.com", "/users/1")
        };

        call(&client, &api).await.unwrap();

        assert_eq!(client.requests()[0].method, Method::DELETE);
    }

    #[tokio::test]
    async fn returns_a_json_body() {
        let client = MockHttpClient::returning(200, r#"{"users": [{"id": 1}]}"#);

        let output = call(&client, &api("https://api.example.com", "/users")).await;

        assert_eq!(output, Ok(r#"{"users":[{"id":1}]}"#.to_string()));
    }

    #[tokio::test]
    async fn returns_a_body_that_is_not_json_as_a_json_string() {
        let client = MockHttpClient::returning(200, "OK");

        let output = call(&client, &api("https://api.example.com", "/ping")).await;

        assert_eq!(output, Ok(r#""OK""#.to_string()));
    }

    #[tokio::test]
    async fn returns_an_empty_body_as_an_empty_json_string() {
        let client = MockHttpClient::returning(204, "");

        let output = call(&client, &api("https://api.example.com", "/users/1")).await;

        assert_eq!(output, Ok(r#""""#.to_string()));
    }

    #[tokio::test]
    async fn rejects_a_status_outside_2xx_without_the_body() {
        let client = MockHttpClient::returning(401, r#"{"error": "invalid key abc123"}"#);

        let output = call(&client, &api("https://api.example.com", "/users")).await;

        assert_eq!(output, Err("The API responded with status 401".to_string()));
    }

    #[tokio::test]
    async fn reports_a_request_that_failed() {
        let client = MockHttpClient::failing("connection refused");

        let output = call(&client, &api("https://api.example.com", "/users")).await;

        assert_eq!(
            output,
            Err("The API request failed: connection refused".to_string())
        );
    }

    #[tokio::test]
    async fn rejects_an_empty_base_url_without_a_request() {
        let client = MockHttpClient::returning(200, "{}");

        let output = call(&client, &api(" ", "/users")).await;

        assert_eq!(output, Err("The API has no base url".to_string()));
        assert!(client.requests().is_empty());
    }

    #[tokio::test]
    async fn rejects_an_empty_path_without_a_request() {
        let client = MockHttpClient::returning(200, "{}");

        let output = call(&client, &api("https://api.example.com", "")).await;

        assert_eq!(output, Err("The API has no path".to_string()));
        assert!(client.requests().is_empty());
    }

    #[test]
    fn joins_the_url_with_exactly_one_slash() {
        assert_eq!(join_url("https://a.com", "users"), "https://a.com/users");
        assert_eq!(join_url("https://a.com/", "users"), "https://a.com/users");
        assert_eq!(join_url("https://a.com", "/users"), "https://a.com/users");
        assert_eq!(
            join_url("https://a.com//", "//users"),
            "https://a.com/users"
        );
    }

    #[test]
    fn maps_every_method() {
        let cases = [
            (BettyHttpMethod::Get, Method::GET),
            (BettyHttpMethod::Post, Method::POST),
            (BettyHttpMethod::Put, Method::PUT),
            (BettyHttpMethod::Patch, Method::PATCH),
            (BettyHttpMethod::Delete, Method::DELETE),
            (BettyHttpMethod::Head, Method::HEAD),
            (BettyHttpMethod::Options, Method::OPTIONS),
        ];
        for (method, expected) in cases {
            assert_eq!(to_http_method(method), expected);
        }
    }
}
