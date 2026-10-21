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
use crate::providers::mistral::Mistral;
use crate::providers::openai::OpenAi;
use crate::providers::{Output, Prompt, Provider, active_tools, redact};

wit_bindgen::generate!({ generate_all });

struct Component;

impl prompt_call::Guest for Component {
    fn prompt_call(
        system_prompt: String,
        prompt: Option<String>,
        agent: BettyAiAgent,
        max_tokens: Option<u32>,
        // Types the `result` Output in the IDE; at run time it carries the schema model as JSON
        // Schema, which asks the provider for structured output.
        schema_model: Option<String>,
    ) -> Result<prompt_call::BettyFlatOutput, String> {
      let prompt = build_prompt(system_prompt, prompt, max_tokens, schema_model)?;

        let output = wstd::runtime::block_on(run(&Client::new(), &agent, &prompt))?;

        Ok(prompt_call::BettyFlatOutput {
            result: json_result(output.text, &prompt),
            usage: output.usage.to_string(),
        })
    }
}

fn build_prompt(
  system_prompt: String,
  prompt: Option<String>,
  max_tokens: Option<u32>,
  schema_model: Option<String>,
) -> Result<Prompt, String> {
  let (instructions, message) = match prompt.filter(|prompt| !prompt.trim().is_empty()) {
    Some(message) => (system_prompt, message),
    None if !system_prompt.trim().is_empty() => (String::new(), system_prompt),
    None => return Err("Either a system prompt or a prompt is required".to_string()),
  };

  Ok(Prompt {
    instructions,
    message,
    max_tokens,
    output_schema: parse_schema_model(schema_model)?,
  })
}

// `result` is always valid JSON. With a schema model it is the structured output, already
// checked by `ensure_json`. Without one it is text, even when the text happens to parse as
// JSON, so the IDE only ever sees an Object or a Text.
fn json_result(text: String, prompt: &Prompt) -> String {
    match prompt.output_schema {
        Some(_) => text,
        None => serde_json::Value::String(text).to_string(),
    }
}

fn parse_schema_model(schema_model: Option<String>) -> Result<Option<serde_json::Value>, String> {
    let Some(schema) = schema_model.filter(|schema| !schema.trim().is_empty()) else {
        return Ok(None);
    };

    match serde_json::from_str::<serde_json::Value>(&schema) {
        Ok(schema) if schema.is_object() => Ok(Some(schema)),
        Ok(_) => Err("Schema model must be a JSON Schema object".to_string()),
        Err(e) => Err(format!("Schema model is not valid JSON Schema: {e}")),
    }
}

// A response cut off by max tokens still carries text, but not a complete JSON document.
fn ensure_json(output: Output, prompt: &Prompt) -> Result<Output, String> {
    if prompt.output_schema.is_none()
        || serde_json::from_str::<serde_json::Value>(&output.text).is_ok()
    {
        return Ok(output);
    }

    Err("Response is not valid JSON; it may have been cut off by max tokens".to_string())
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
        "mistral" => Mistral.complete(client, agent, &prompt).await,
        other => Err(format!("Unsupported provider: {other}")),
    };

    result
        .and_then(|output| ensure_json(output, &prompt))
        .map_err(|error| redact(&error, agent))
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
        output_schema: prompt.output_schema.clone(),
    }
}

export! {Component}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::betty_blocks_types::types::types::{AuthenticationConfig, McpOptions};
    use crate::test_helpers::{
        MockHttpClient, anthropic_text_response, google_agent, google_text_response,
        mistral_text_response, openai_agent, openai_text_response, test_mistral_provider,
        test_prompt, test_provider,
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

    #[test]
    fn build_prompt_keeps_both_prompts_when_given() {
        let prompt = build_prompt("Be brief.".into(), Some("hi".into()), Some(10)).unwrap();

        assert_eq!(prompt.instructions, "Be brief.");
        assert_eq!(prompt.message, "hi");
        assert_eq!(prompt.max_tokens, Some(10));
    }

    #[test]
    fn build_prompt_uses_the_system_prompt_as_message_without_a_prompt() {
        for missing in [None, Some(String::new()), Some("  ".to_string())] {
            let prompt = build_prompt("Write a haiku.".into(), missing, None).unwrap();

            assert_eq!(prompt.instructions, "");
            assert_eq!(prompt.message, "Write a haiku.");
        }
    }

    #[test]
    fn build_prompt_rejects_when_both_prompts_are_empty() {
        let error = build_prompt(" ".into(), None, None).unwrap_err();

        assert_eq!(error, "Either a system prompt or a prompt is required");
    }

    #[tokio::test]
    async fn dispatches_google_to_the_google_provider() {
        let client = MockHttpClient::new(vec![(200, google_text_response("hello"))]);

        let text = run(&client, &google_agent(), &test_prompt("hi"))
            .await
            .unwrap();

        assert_eq!(text.text, "hello");
        assert!(client.requests()[0].url.ends_with(":generateContent"));
    }

    #[tokio::test]
    async fn dispatches_mistral_to_the_mistral_provider() {
        let client = MockHttpClient::new(vec![(200, mistral_text_response("hello"))]);

        let text = run(&client, &test_mistral_provider(), &test_prompt("hi"))
            .await
            .unwrap();

        assert_eq!(text.text, "hello");
        assert_eq!(client.requests().len(), 1);
        assert_eq!(
            client.requests()[0].url,
            "https://api.mistral.ai/v1/conversations"
        );
    }

    #[tokio::test]
    async fn rejects_an_unsupported_provider_without_calling_out() {
        let client = MockHttpClient::new(vec![(200, anthropic_text_response("unused"))]);
        let mut provider = test_provider();
        provider.name = "unknown".to_string();

        let error = run(&client, &provider, &test_prompt("hi"))
            .await
            .unwrap_err();

        assert!(error.contains("unknown"));
        assert!(client.requests().is_empty());
    }

    #[test]
    fn json_result_wraps_any_text_as_a_json_string_without_a_schema_model() {
        let prompt = test_prompt("hi");

        assert_eq!(
            json_result("Hello \"world\"\n".into(), &prompt),
            r#""Hello \"world\"\n""#
        );
        assert_eq!(json_result("4".into(), &prompt), r#""4""#);
        assert_eq!(
            json_result(r#"{"a": 1}"#.into(), &prompt),
            r#""{\"a\": 1}""#
        );
    }

    #[test]
    fn json_result_passes_the_json_through_with_a_schema_model() {
        assert_eq!(
            json_result(r#"{"a": 1}"#.into(), &schema_prompt()),
            r#"{"a": 1}"#
        );
    }

    #[test]
    fn parse_schema_model_reads_the_json_schema() {
        let schema = r#"{"type":"object","properties":{"answer":{"type":"string"}}}"#;

        assert_eq!(
            parse_schema_model(Some(schema.into())).unwrap(),
            Some(serde_json::from_str(schema).unwrap())
        );
    }

    #[test]
    fn parse_schema_model_ignores_a_missing_or_blank_schema_model() {
        assert_eq!(parse_schema_model(None).unwrap(), None);
        assert_eq!(parse_schema_model(Some("  ".into())).unwrap(), None);
    }

    #[test]
    fn parse_schema_model_rejects_a_schema_model_that_is_not_a_json_schema_object() {
        let invalid = parse_schema_model(Some("{".into())).unwrap_err();
        let not_object = parse_schema_model(Some("[]".into())).unwrap_err();

        assert!(invalid.starts_with("Schema model is not valid JSON Schema"));
        assert_eq!(not_object, "Schema model must be a JSON Schema object");
    }

    fn schema_prompt() -> Prompt {
        let mut prompt = test_prompt("hi");
        prompt.output_schema = Some(serde_json::json!({ "type": "object" }));
        prompt
    }

    #[tokio::test]
    async fn returns_the_json_text_when_a_schema_model_is_given() {
        let client = MockHttpClient::new(vec![(200, anthropic_text_response(r#"{"a":1}"#))]);

        let output = run(&client, &test_provider(), &schema_prompt())
            .await
            .unwrap();

        assert_eq!(output.text, r#"{"a":1}"#);
    }

    #[tokio::test]
    async fn rejects_text_that_is_not_json_when_a_schema_model_is_given() {
        let client = MockHttpClient::new(vec![(200, anthropic_text_response(r#"{"a":"#))]);

        let error = run(&client, &test_provider(), &schema_prompt())
            .await
            .unwrap_err();

        assert_eq!(
            error,
            "Response is not valid JSON; it may have been cut off by max tokens"
        );
    }
}
