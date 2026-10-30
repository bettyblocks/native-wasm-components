use serde::Serialize;

use crate::betty_blocks_types::types::types::{BettySoftwareApi, BettyHttpMethod};
use crate::exports::betty_blocks::call_api::call_api;

wit_bindgen::generate!({ generate_all });

struct Component;

/// What the stub returns.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiOutput<'a> {
    software_name: &'a str,
    api_name: &'a str,
    method: &'static str,
    url: String,
}

impl call_api::Guest for Component {
    fn call_api(api: BettySoftwareApi) -> Result<String, String> {
        describe(&api)
    }
}

/// Stub: describes the resolved API call as JSON. No HTTP call is made yet.
fn describe(api: &BettySoftwareApi) -> Result<String, String> {
    if api.base_url.trim().is_empty() {
        return Err("The API has no base url".to_string());
    }
    if api.path.trim().is_empty() {
        return Err("The API has no path".to_string());
    }

    serde_json::to_string(&ApiOutput {
        software_name: &api.software_name,
        api_name: &api.api_name,
        method: method_name(api.method),
        url: join_url(&api.base_url, &api.path),
    })
    .map_err(|error| format!("Failed to serialize API: {error}"))
}

/// Joins a base url and a path with exactly one slash between them.
fn join_url(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

fn method_name(method: BettyHttpMethod) -> &'static str {
    match method {
        BettyHttpMethod::Get => "get",
        BettyHttpMethod::Post => "post",
        BettyHttpMethod::Put => "put",
        BettyHttpMethod::Patch => "patch",
        BettyHttpMethod::Delete => "delete",
        BettyHttpMethod::Head => "head",
        BettyHttpMethod::Options => "options",
    }
}

export!(Component);

#[cfg(test)]
mod tests {
    use super::*;

    fn api(base_url: &str, path: &str) -> BettySoftwareApi {
        BettySoftwareApi {
            software_name: "Exact".to_string(),
            api_name: "Get a user".to_string(),
            base_url: base_url.to_string(),
            path: path.to_string(),
            method: BettyHttpMethod::Get,
        }
    }

    fn parse(json: &str) -> serde_json::Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn returns_the_api_as_json() {
        let json = describe(&api("https://api.example.com", "/users/{id}")).unwrap();

        assert_eq!(
            parse(&json),
            serde_json::json!({
                "softwareName": "Exact",
                "apiName": "Get a user",
                "method": "get",
                "url": "https://api.example.com/users/{id}"
            })
        );
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
    fn names_every_method_in_lowercase() {
        let cases = [
            (BettyHttpMethod::Get, "get"),
            (BettyHttpMethod::Post, "post"),
            (BettyHttpMethod::Put, "put"),
            (BettyHttpMethod::Patch, "patch"),
            (BettyHttpMethod::Delete, "delete"),
            (BettyHttpMethod::Head, "head"),
            (BettyHttpMethod::Options, "options"),
        ];
        for (method, name) in cases {
            assert_eq!(method_name(method), name);
        }
    }

    #[test]
    fn rejects_an_empty_base_url() {
        assert_eq!(
            describe(&api(" ", "/x")),
            Err("The API has no base url".to_string())
        );
    }

    #[test]
    fn rejects_an_empty_path() {
        assert_eq!(
            describe(&api("https://a.com", "")),
            Err("The API has no path".to_string())
        );
    }
}
