mod http;
mod providers;
#[cfg(test)]
mod test_helpers;

use wstd::http::Client;

use crate::betty_blocks_types::types::types::{BettyAiAgent, BettyAiTool};
use crate::exports::betty_blocks::prompt_call::prompt_call;
use crate::http::HttpClient;
use crate::providers::anthropic::Anthropic;
use crate::providers::google::Google;
use crate::providers::openai::OpenAi;
use crate::providers::{Output, Prompt, Provider, active_tools, redact};

wit_bindgen::generate!({ generate_all });

struct Component;

impl prompt_call::Guest for Component {
    fn prompt_call(
        agent: BettyAiAgent,
        system_prompt: String,
        prompt: String,
        max_tokens: Option<u32>,
    ) -> Result<prompt_call::BettyFlatOutput, String> {
        let prompt = Prompt {
            instructions: system_prompt,
            message: prompt,
            max_tokens,
        };

        let output = wstd::runtime::block_on(run(&Client::new(), &agent, &prompt))?;

        Ok(prompt_call::BettyFlatOutput {
            result: output.text,
            usage: output.usage.to_string(),
        })
    }
}

async fn run(
    client: &impl HttpClient,
    agent: &BettyAiAgent,
    prompt: &Prompt,
) -> Result<Output, String> {
    let prompt = with_tool_descriptions(agent, prompt);
    let result = match agent.name.as_str() {
        "anthropic" => Anthropic.complete(client, agent, &prompt).await,
        "openai" => OpenAi.complete(client, agent, &prompt).await,
        "google" => Google.complete(client, agent, &prompt).await,
        other => Err(format!("Unsupported provider: {other}")),
    };

    result.map_err(|error| redact(&error, agent))
}

fn with_tool_descriptions(agent: &BettyAiAgent, prompt: &Prompt) -> Prompt {
    let lines: Vec<String> = active_tools(agent)
        .map(|tool| match tool {
            BettyAiTool::HttpSearch(options) => match &options.description {
                Some(description) => format!("- http-search: {description}"),
                None => "- http-search: use for searching the web when the answer requires current information".to_string(),
            },
            BettyAiTool::Mcp(options) => match &options.description {
                Some(description) => {
                    format!("- mcp-server {}: {description}", options.url)
                }
                None => format!("- mcp-server {}: use when appropriate", options.url),
            },
        })
        .collect();

    let mut instructions = prompt.instructions.clone();
    if !lines.is_empty() {
        if !instructions.is_empty() {
            instructions.push_str("\n\n");
        }
        instructions.push_str("Tools:\n");
        instructions.push_str(&lines.join("\n"));
    }

    Prompt {
        instructions,
        message: prompt.message.clone(),
        max_tokens: prompt.max_tokens,
    }
}

export! {Component}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::betty_blocks_types::types::types::{AuthenticationConfig, McpOptions};
    use crate::test_helpers::{
        MockHttpClient, anthropic_text_response, google_agent, google_text_response, openai_agent,
        openai_text_response, test_prompt, test_provider,
    };

    #[tokio::test]
    async fn adds_tool_descriptions_to_the_system_prompt() {
        let client = MockHttpClient::new(vec![(200, anthropic_text_response("hello"))]);
        let mut agent = test_provider();
        agent.tools = Some(vec![BettyAiTool::Mcp(McpOptions {
            description: Some("call this tool if the prompt contain \"mcp call\"".to_string()),
            url: "https://gateway.mcpservers.org/yahoo-finance/mcp".to_string(),
            authentication: AuthenticationConfig {
                kind: "access_token".to_string(),
                value: None,
            },
            enabled_tools: None,
        })]);

        run(&client, &agent, &test_prompt("hi")).await.unwrap();

        let system = client.requests()[0].json()["system"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(system.starts_with("You are helpful.\n\nTools:"));
        assert!(system.contains(
            "- mcp-server https://gateway.mcpservers.org/yahoo-finance/mcp: call this tool if the prompt contain \"mcp call\""
        ));
    }

    #[tokio::test]
    async fn leaves_an_mcp_server_with_an_empty_selection_out_of_the_system_prompt() {
        let client = MockHttpClient::new(vec![(200, anthropic_text_response("hello"))]);
        let mut agent = test_provider();
        agent.tools = Some(vec![BettyAiTool::Mcp(McpOptions {
            description: Some("never offered".to_string()),
            url: "https://mcp.example.com/mcp".to_string(),
            authentication: AuthenticationConfig {
                kind: "access_token".to_string(),
                value: None,
            },
            enabled_tools: Some(vec![]),
        })]);

        run(&client, &agent, &test_prompt("hi")).await.unwrap();

        let request = &client.requests()[0];
        assert_eq!(request.json()["system"], "You are helpful.");
        assert!(request.json().get("tools").is_none());
        assert_eq!(request.header("anthropic-beta"), None);
    }

    #[tokio::test]
    async fn leaves_the_system_prompt_untouched_without_tools() {
        let client = MockHttpClient::new(vec![(200, anthropic_text_response("hello"))]);

        run(&client, &test_provider(), &test_prompt("hi"))
            .await
            .unwrap();

        let system = client.requests()[0].json()["system"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(system, "You are helpful.");
    }

    #[tokio::test]
    async fn dispatches_anthropic_to_the_anthropic_provider() {
        let client = MockHttpClient::new(vec![(200, anthropic_text_response("hello"))]);

        let text = run(&client, &test_provider(), &test_prompt("hi"))
            .await
            .unwrap();

        assert_eq!(text.text, "hello");
        assert_eq!(client.requests().len(), 1);
    }

    #[tokio::test]
    async fn dispatches_openai_to_the_openai_provider() {
        let client = MockHttpClient::new(vec![(200, openai_text_response("hello"))]);

        let text = run(&client, &openai_agent(), &test_prompt("hi"))
            .await
            .unwrap();

        assert_eq!(text.text, "hello");
        assert!(client.requests()[0].url.ends_with("/responses"));
    }

    #[tokio::test]
    async fn dispatches_google_to_the_google_provider() {
        let client = MockHttpClient::new(vec![(200, google_text_response("hello"))]);

        let text = run(&client, &google_agent(), &test_prompt("hi"))
            .await
            .unwrap();

        assert_eq!(text, "hello");
        assert!(client.requests()[0].url.ends_with(":generateContent"));
    }

    #[tokio::test]
    async fn rejects_an_unsupported_provider_without_calling_out() {
        let client = MockHttpClient::new(vec![(200, anthropic_text_response("unused"))]);
        let mut provider = test_provider();
        provider.name = "mistral".to_string();

        let error = run(&client, &provider, &test_prompt("hi"))
            .await
            .unwrap_err();

        assert!(error.contains("mistral"));
        assert!(client.requests().is_empty());
    }
}
