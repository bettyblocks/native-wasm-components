pub(crate) mod anthropic;
pub(crate) mod openai;

use crate::betty_blocks_types::types::types::BettyAiAgent;
use crate::http::HttpClient;

#[derive(Debug)]
pub(crate) struct Prompt {
    pub(crate) instructions: String,
    pub(crate) message: String,
    pub(crate) max_tokens: Option<u32>,
}

pub(crate) trait Provider {
    async fn complete(
        &self,
        client: &impl HttpClient,
        provider: &BettyAiAgent,
        prompt: &Prompt,
    ) -> Result<String, String>;
}

const MCP_SERVER_NAME_MAX_LEN: usize = 64;

pub(crate) fn unique_mcp_server_name(url: &str, taken: &[&str]) -> String {
    let base = mcp_server_name(url);
    let is_taken = |name: &str| taken.contains(&name);

    if !is_taken(&base) {
        return base;
    }

    (2..)
        .map(|n| {
            let suffix = format!("-{n}");
            let stem: String = base
                .chars()
                .take(MCP_SERVER_NAME_MAX_LEN - suffix.len())
                .collect();
            format!("{stem}{suffix}")
        })
        .find(|name| !is_taken(name))
        .expect("an unused suffix always exists")
}

fn mcp_server_name(url: &str) -> String {
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let host = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(without_scheme);
    let sanitized: String = host
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();

    let name: String = sanitized
        .trim_matches('-')
        .chars()
        .take(MCP_SERVER_NAME_MAX_LEN)
        .collect();

    if name.is_empty() {
        "mcp-server".to_string()
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_server_name_sanitizes_the_url_host() {
        assert_eq!(
            mcp_server_name("https://MCP.Example.com:8080/sse"),
            "MCP-Example-com-8080"
        );
        assert_eq!(mcp_server_name("http://localhost:3000"), "localhost-3000");
        assert_eq!(mcp_server_name("not a url"), "not-a-url");
        assert_eq!(mcp_server_name("///"), "mcp-server");
    }

    #[test]
    fn unique_mcp_server_name_suffixes_a_taken_name() {
        assert_eq!(
            unique_mcp_server_name("https://mcp.example.com/a", &[]),
            "mcp-example-com"
        );
        assert_eq!(
            unique_mcp_server_name("https://mcp.example.com/b", &["mcp-example-com"]),
            "mcp-example-com-2"
        );
    }
}
