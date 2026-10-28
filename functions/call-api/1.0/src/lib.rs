use serde::Serialize;

use crate::betty_blocks_types::types::types::BettyApi;
use crate::exports::betty_blocks::call_api::call_api;

wit_bindgen::generate!({ generate_all });

struct Component;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiOutput<'a> {
    id: &'a str,
    software_name: &'a str,
}

impl call_api::Guest for Component {
    fn call_api(api: BettyApi) -> Result<String, String> {
        describe(&api)
    }
}

/// Stub: echoes the selected API back as JSON. No HTTP call is made yet.
fn describe(api: &BettyApi) -> Result<String, String> {
    if api.software_name.trim().is_empty() {
        return Err("No API selected".to_string());
    }

    serde_json::to_string(&ApiOutput {
        id: &api.id,
        software_name: &api.software_name,
    })
    .map_err(|error| format!("Failed to serialize API: {error}"))
}

export!(Component);

#[cfg(test)]
mod tests {
    use super::*;

    fn api(id: &str, software_name: &str) -> BettyApi {
        BettyApi {
            id: id.to_string(),
            software_name: software_name.to_string(),
        }
    }

    #[test]
    fn returns_the_api_as_json() {
        let json = describe(&api("1", "Exact")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value, serde_json::json!({"id": "1", "softwareName": "Exact"}));
    }

    #[test]
    fn rejects_an_empty_software_name() {
        assert_eq!(describe(&api("1", "")), Err("No API selected".to_string()));
    }

    #[test]
    fn rejects_a_blank_software_name() {
        assert_eq!(describe(&api("1", "   ")), Err("No API selected".to_string()));
    }
}
