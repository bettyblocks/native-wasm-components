pub(crate) mod anthropic;

use crate::betty_blocks_types::types::types::BettyAiProvider;
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
        provider: &BettyAiProvider,
        prompt: &Prompt,
    ) -> Result<String, String>;
}
