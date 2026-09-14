use crate::exports::betty_blocks::ai_agent::ai_agent::AiProvider;
use crate::http::HttpClient;

pub(crate) struct Prompt {
    pub(crate) instructions: String,
    pub(crate) message: String,
    pub(crate) max_tokens: Option<u32>,
}

pub(crate) trait Provider {
    async fn complete(
        &self,
        client: &impl HttpClient,
        provider: &AiProvider,
        prompt: &Prompt,
    ) -> Result<String, String>;
}
