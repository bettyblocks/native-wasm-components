mod http;
#[cfg(test)]
mod test_helpers;

use url::Url;
use wstd::http::{Client, Method};

use crate::betty_blocks_types::types::types::{BettyHttpMethod, BettySoftwareApi};
use crate::exports::betty_blocks::call_api::call_api;
use crate::http::HttpClient;

wit_bindgen::generate!({ generate_all });

type JsonObject = serde_json::Map<String, serde_json::Value>;

struct Component;

impl call_api::Guest for Component {
    fn call_api(api: BettySoftwareApi) -> Result<String, String> {
        wstd::runtime::block_on(call(&Client::new(), &api))
    }
}

async fn call(client: &impl HttpClient, api: &BettySoftwareApi) -> Result<String, String> {
    validate_api(api)?;
    let query_parameters = parse_json_object(api.query_parameters.as_deref(), "query parameters")?;
    let url = add_query_parameters(&join_url(&api.base_url, &api.path), &query_parameters)?;
    let mut headers = to_headers(&parse_json_object(api.headers.as_deref(), "headers")?);
    let body_parameters = parse_json_object(api.body_parameters.as_deref(), "body parameters")?;
    if !body_parameters.is_empty() {
        add_json_content_type(&mut headers);
    }

    let (status, body) = client
        .send_request(
            to_http_method(api.method),
            &url,
            headers,
            to_body(&body_parameters),
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

fn join_url(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

fn parse_json_object(json: Option<&str>, name: &str) -> Result<JsonObject, String> {
    match json.map(str::trim) {
        None | Some("") => Ok(JsonObject::new()),
        Some(json) => {
            serde_json::from_str(json).map_err(|_| format!("The {name} are not a JSON object"))
        }
    }
}

fn add_query_parameters(url: &str, query_parameters: &JsonObject) -> Result<String, String> {
    if query_parameters.is_empty() {
        return Ok(url.to_string());
    }

    let mut url = Url::parse(url).map_err(|_| "The API url is not valid".to_string())?;
    url.query_pairs_mut().extend_pairs(
        query_parameters
            .iter()
            .map(|(key, value)| (key, to_parameter_value(value))),
    );

    Ok(url.into())
}

fn to_headers(headers: &JsonObject) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(name, value)| (name.clone(), to_parameter_value(value)))
        .collect()
}

fn add_json_content_type(headers: &mut Vec<(String, String)>) {
    let has_content_type = headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("content-type"));

    if !has_content_type {
        headers.push(("Content-Type".to_string(), "application/json".to_string()));
    }
}

fn to_body(body_parameters: &JsonObject) -> Vec<u8> {
    if body_parameters.is_empty() {
        return Vec::new();
    }

    serde_json::Value::Object(body_parameters.clone())
        .to_string()
        .into_bytes()
}

fn to_parameter_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Null => String::new(),
        serde_json::Value::Array(values) => values
            .iter()
            .map(to_parameter_value)
            .collect::<Vec<_>>()
            .join(","),
        other => other.to_string(),
    }
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
            headers: None,
            query_parameters: None,
            body_parameters: None,
        }
    }

    fn api_with_query(query_parameters: &str) -> BettySoftwareApi {
        BettySoftwareApi {
            query_parameters: Some(query_parameters.to_string()),
            ..api("https://api.example.com", "/users")
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

    #[tokio::test]
    async fn sends_the_query_parameters_in_the_url() {
        let client = MockHttpClient::returning(200, "{}");

        call(
            &client,
            &api_with_query(r#"{"page": 2, "name": "Jane Doe"}"#),
        )
        .await
        .unwrap();

        assert_eq!(
            client.requests()[0].url,
            "https://api.example.com/users?name=Jane+Doe&page=2"
        );
    }

    #[tokio::test]
    async fn keeps_a_query_that_is_already_in_the_path() {
        let client = MockHttpClient::returning(200, "{}");
        let api = BettySoftwareApi {
            query_parameters: Some(r#"{"page": 2}"#.to_string()),
            ..api("https://api.example.com", "/users?active=true")
        };

        call(&client, &api).await.unwrap();

        assert_eq!(
            client.requests()[0].url,
            "https://api.example.com/users?active=true&page=2"
        );
    }

    #[tokio::test]
    async fn sends_the_url_unchanged_with_empty_query_parameters() {
        let client = MockHttpClient::returning(200, "{}");

        call(&client, &api_with_query(" ")).await.unwrap();

        assert_eq!(client.requests()[0].url, "https://api.example.com/users");
    }

    #[tokio::test]
    async fn rejects_query_parameters_that_are_not_a_json_object_without_a_request() {
        let client = MockHttpClient::returning(200, "{}");

        let output = call(&client, &api_with_query("[1, 2]")).await;

        assert_eq!(
            output,
            Err("The query parameters are not a JSON object".to_string())
        );
        assert!(client.requests().is_empty());
    }

    #[tokio::test]
    async fn sends_the_headers() {
        let client = MockHttpClient::returning(200, "{}");
        let api = BettySoftwareApi {
            headers: Some(r#"{"Authorization": "Bearer abc", "X-Page": 2}"#.to_string()),
            ..api("https://api.example.com", "/users")
        };

        call(&client, &api).await.unwrap();

        assert_eq!(
            client.requests()[0].headers,
            vec![
                ("Authorization".to_string(), "Bearer abc".to_string()),
                ("X-Page".to_string(), "2".to_string()),
            ]
        );
    }

    #[tokio::test]
    async fn rejects_headers_that_are_not_a_json_object_without_a_request() {
        let client = MockHttpClient::returning(200, "{}");
        let api = BettySoftwareApi {
            headers: Some("Authorization: Bearer abc".to_string()),
            ..api("https://api.example.com", "/users")
        };

        let output = call(&client, &api).await;

        assert_eq!(output, Err("The headers are not a JSON object".to_string()));
        assert!(client.requests().is_empty());
    }

    #[tokio::test]
    async fn sends_the_body_parameters_as_json() {
        let client = MockHttpClient::returning(201, "{}");
        let api = BettySoftwareApi {
            method: BettyHttpMethod::Post,
            body_parameters: Some(r#"{"name": "Jane", "age": 30}"#.to_string()),
            ..api("https://api.example.com", "/users")
        };

        call(&client, &api).await.unwrap();

        let request = &client.requests()[0];
        assert_eq!(request.body, br#"{"age":30,"name":"Jane"}"#.to_vec());
        assert_eq!(
            request.headers,
            vec![("Content-Type".to_string(), "application/json".to_string())]
        );
    }

    #[tokio::test]
    async fn keeps_the_content_type_from_the_headers() {
        let client = MockHttpClient::returning(201, "{}");
        let api = BettySoftwareApi {
            method: BettyHttpMethod::Post,
            headers: Some(r#"{"content-type": "application/vnd.api+json"}"#.to_string()),
            body_parameters: Some(r#"{"name": "Jane"}"#.to_string()),
            ..api("https://api.example.com", "/users")
        };

        call(&client, &api).await.unwrap();

        assert_eq!(
            client.requests()[0].headers,
            vec![(
                "content-type".to_string(),
                "application/vnd.api+json".to_string()
            )]
        );
    }

    #[tokio::test]
    async fn rejects_body_parameters_that_are_not_a_json_object_without_a_request() {
        let client = MockHttpClient::returning(201, "{}");
        let api = BettySoftwareApi {
            method: BettyHttpMethod::Post,
            body_parameters: Some(r#"[{"name": "Jane"}]"#.to_string()),
            ..api("https://api.example.com", "/users")
        };

        let output = call(&client, &api).await;

        assert_eq!(
            output,
            Err("The body parameters are not a JSON object".to_string())
        );
        assert!(client.requests().is_empty());
    }

    #[test]
    fn converts_parameter_values_like_the_http_step() {
        let cases = [
            (serde_json::json!("text"), "text"),
            (serde_json::json!(1.5), "1.5"),
            (serde_json::json!(true), "true"),
            (serde_json::json!(null), ""),
            (serde_json::json!([1, "a", [2, 3]]), "1,a,2,3"),
            (serde_json::json!({"a": 1}), r#"{"a":1}"#),
        ];
        for (value, expected) in cases {
            assert_eq!(to_parameter_value(&value), expected);
        }
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
