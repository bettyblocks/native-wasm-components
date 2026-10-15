pub(crate) mod anthropic;
pub(crate) mod openai;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::betty_blocks_types::types::types::{BettyAiAgent, BettyAiTool, McpOptions};
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

pub(crate) fn active_tools(agent: &BettyAiAgent) -> impl Iterator<Item = &BettyAiTool> {
    agent.tools.iter().flatten().filter(|tool| match tool {
        BettyAiTool::Mcp(options) => options
            .enabled_tools
            .as_ref()
            .is_none_or(|selected| !selected.is_empty()),
        BettyAiTool::HttpSearch(_) => true,
    })
}

pub(crate) fn ensure_configured(agent: &BettyAiAgent) -> Result<(), String> {
    if agent.api_key.is_empty() {
        return Err(format!("No API key configured for agent: {}", agent.name));
    }

    if agent.url.is_empty() {
        return Err(format!("No URL configured for agent: {}", agent.name));
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
pub(crate) struct ApiError {
    message: Option<String>,
    code: Option<String>,
    #[serde(rename = "type")]
    error_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    error: ApiError,
}

impl ApiError {
    pub(crate) fn describe(&self) -> String {
        let kind = self
            .code
            .as_deref()
            .or(self.error_type.as_deref())
            .map(|kind| format!(" ({kind})"))
            .unwrap_or_default();
        let message = self
            .message
            .as_deref()
            .filter(|message| !message.is_empty())
            .map(|message| format!(": {message}"))
            .unwrap_or_default();

        format!("{kind}{message}")
    }
}

const REDACTED: &str = "[redacted]";

pub(crate) fn redact(message: &str, agent: &BettyAiAgent) -> String {
    let mut secrets: Vec<&str> = agent
        .tools
        .iter()
        .flatten()
        .filter_map(|tool| match tool {
            BettyAiTool::Mcp(options) => mcp_auth_token(options),
            BettyAiTool::HttpSearch(_) => None,
        })
        .chain([agent.api_key.as_str()])
        .filter(|secret| !secret.is_empty())
        .collect();

    secrets.sort_by_key(|secret| std::cmp::Reverse(secret.len()));

    let message = secrets
        .into_iter()
        .fold(message.to_string(), |message, secret| {
            message.replace(secret, REDACTED)
        });

    redact_key_like_tokens(&message)
}

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '*')
}

fn redact_key_like_tokens(message: &str) -> String {
    let mut redacted = String::with_capacity(message.len());
    let mut rest = message;
    let mut after_bearer = false;

    while let Some(start) = rest.find(is_token_char) {
        redacted.push_str(&rest[..start]);
        rest = &rest[start..];

        let run = &rest[..rest.find(|c| !is_token_char(c)).unwrap_or(rest.len())];

        let token = match run.trim_end_matches('.') {
            "" => run,
            token => token,
        };

        if after_bearer || token.starts_with("sk-") {
            redacted.push_str(REDACTED);
        } else {
            redacted.push_str(token);
        }
        after_bearer = token.eq_ignore_ascii_case("bearer");
        rest = &rest[token.len()..];
    }

    redacted.push_str(rest);
    redacted
}

pub(crate) trait ProviderResponse: DeserializeOwned {
    const PROVIDER: &'static str;

    fn text_parts(&self) -> Vec<&str>;

    fn ensure_succeeded(&self) -> Result<(), String> {
        Ok(())
    }

    fn no_text_error(&self) -> String {
        format!("{} response contained no text content", Self::PROVIDER)
    }

    fn parse(response: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(response)
            .map_err(|e| format!("Failed to parse {} response: {e}", Self::PROVIDER))
    }

    fn text(&self) -> Result<String, String> {
        self.ensure_succeeded()?;

        let text = self
            .text_parts()
            .into_iter()
            .map(str::trim)
            .collect::<Vec<_>>()
            .join("\n\n");

        if text.is_empty() {
            return Err(self.no_text_error());
        }

        Ok(text)
    }

    fn extract_text(response: &[u8]) -> Result<String, String> {
        Self::parse(response)?.text()
    }

    fn status_error(status: u16, response: &[u8]) -> String {
        match serde_json::from_slice::<ErrorBody>(response) {
            Ok(body) => format!(
                "{} returned status {status}{}",
                Self::PROVIDER,
                body.error.describe()
            ),
            Err(_) => format!("{} returned status {status}", Self::PROVIDER),
        }
    }

    fn handle(status: u16, response: &[u8]) -> Result<String, String> {
        if status == 200 {
            Self::extract_text(response)
        } else {
            Err(Self::status_error(status, response))
        }
    }
}

pub(crate) trait ProviderRequest {
    fn base_body(agent: &BettyAiAgent, prompt: &Prompt) -> serde_json::Value;

    fn web_search_tool() -> serde_json::Value;

    fn mcp_tool(options: &McpOptions, name: &str) -> serde_json::Value;

    fn mcp_server(_options: &McpOptions, _name: &str) -> Option<serde_json::Value> {
        None
    }

    fn request_body(agent: &BettyAiAgent, prompt: &Prompt) -> Vec<u8> {
        let mut body = Self::base_body(agent, prompt);
        let mut tools = Vec::new();
        let mut mcp_servers = Vec::new();
        let mut mcp_names: Vec<String> = Vec::new();

        for tool in active_tools(agent) {
            match tool {
                BettyAiTool::HttpSearch(_) => tools.push(Self::web_search_tool()),
                BettyAiTool::Mcp(options) => {
                    let taken: Vec<&str> = mcp_names.iter().map(String::as_str).collect();
                    let name = unique_mcp_server_name(&options.url, &taken);
                    tools.push(Self::mcp_tool(options, &name));
                    mcp_servers.extend(Self::mcp_server(options, &name));
                    mcp_names.push(name);
                }
            }
        }

        if !tools.is_empty() {
            body["tools"] = tools.into();
        }
        if !mcp_servers.is_empty() {
            body["mcp_servers"] = mcp_servers.into();
        }
        body.to_string().into_bytes()
    }
}

pub(crate) fn mcp_auth_token(options: &McpOptions) -> Option<&str> {
    options
        .authentication
        .value
        .as_deref()
        .filter(|value| !value.is_empty())
}

const MCP_SERVER_NAME_MAX_LEN: usize = 64;

fn unique_mcp_server_name(url: &str, taken: &[&str]) -> String {
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
    use crate::test_helpers::{mcp_tool, test_agent_with_tools};

    fn agent_with_secrets() -> BettyAiAgent {
        let mut agent = test_agent_with_tools(vec![mcp_tool(
            "https://mcp.example.com",
            Some("mcp-secret-token"),
        )]);
        agent.api_key = "mykey123".to_string();
        agent
    }

    #[test]
    fn redact_removes_the_configured_api_key_and_mcp_tokens() {
        assert_eq!(
            redact(
                "key mykey123 and token mcp-secret-token are not valid",
                &agent_with_secrets()
            ),
            "key [redacted] and token [redacted] are not valid"
        );
    }

    #[test]
    fn redact_removes_key_like_tokens_whatever_surrounds_them() {
        let agent = agent_with_secrets();

        assert_eq!(
            redact(r#"bad key "sk-proj-abc" given"#, &agent),
            r#"bad key "[redacted]" given"#
        );
        assert_eq!(redact("key=sk-abc,next", &agent), "key=[redacted],next");
        assert_eq!(redact("line\nsk-ant-xyz\n", &agent), "line\n[redacted]\n");
        assert_eq!(
            redact(
                "Incorrect API key provided: sk-proj-...abcd. See docs.",
                &agent
            ),
            "Incorrect API key provided: [redacted]. See docs."
        );
        assert_eq!(
            redact("Incorrect API key provided: sk-proj-****abcd", &agent),
            "Incorrect API key provided: [redacted]"
        );
    }

    #[test]
    fn redact_removes_a_bearer_token() {
        assert_eq!(
            redact(
                "header was Authorization: Bearer abc.def-ghi",
                &agent_with_secrets()
            ),
            "header was Authorization: Bearer [redacted]"
        );
    }

    #[test]
    fn redact_keeps_ordinary_text() {
        let message = "task-runner hit a rate limit... retry in 2.5s (risk-free).";

        assert_eq!(redact(message, &agent_with_secrets()), message);
    }

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
