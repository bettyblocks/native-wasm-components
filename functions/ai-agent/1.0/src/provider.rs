use crate::exports::betty_blocks::ai_agent::ai_agent::Input;
use crate::http::HttpClient;

pub(crate) trait Provider {
    async fn complete(&self, client: &impl HttpClient, input: &Input) -> Result<String, String>;
}
