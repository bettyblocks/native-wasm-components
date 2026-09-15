pub(crate) fn api_version() -> String {
    std::env::var("ANTHROPIC_API_VERSION").unwrap_or_else(|_| "2023-06-01".to_string())
}

fn api_key_name(provider_name: &str) -> String {
    format!("ai_provider:{provider_name}")
}

#[cfg(not(test))]
pub(crate) fn api_key(provider_name: &str) -> Result<String, String> {
    let key = api_key_name(provider_name);

    crate::wasi::config::store::get(&key)
        .map_err(|error| format!("Failed to read runtime configuration: {error:?}"))?
        .ok_or_else(|| format!("No '{key}' found in runtime configuration"))
}

// The host import is stubbed as `unreachable!()` off wasm32, so tests get a fixture.
#[cfg(test)]
pub(crate) fn api_key(_provider_name: &str) -> Result<String, String> {
    Ok("test-key".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_name_is_scoped_to_the_provider() {
        assert_eq!(api_key_name("anthropic"), "ai_provider:anthropic");
        assert_eq!(api_key_name("open_ai"), "ai_provider:open_ai");
    }
}
