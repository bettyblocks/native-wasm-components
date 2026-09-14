pub(crate) fn api_version() -> String {
    std::env::var("ANTHROPIC_API_VERSION").unwrap_or_else(|_| "2023-06-01".to_string())
}
