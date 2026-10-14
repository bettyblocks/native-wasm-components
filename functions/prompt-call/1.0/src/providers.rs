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

fn redact(message: &str, api_key: &str) -> String {
    let message = if api_key.is_empty() {
        message.to_string()
    } else {
        message.replace(api_key, "[redacted]")
    };

    message
        .split(' ')
        .map(|word| {
            if word.starts_with("sk-") {
                "[redacted]"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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

    fn handle(status: u16, response: &[u8], api_key: &str) -> Result<String, String> {
        let result = if status == 200 {
            Self::extract_text(response)
        } else {
            Err(Self::status_error(status, response))
        };

        result.map_err(|error| redact(&error, api_key))
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

        for tool in agent.tools.as_deref().unwrap_or(&[]) {
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
