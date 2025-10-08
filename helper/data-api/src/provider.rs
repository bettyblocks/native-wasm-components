use anyhow::Context as _;
use wasmcloud_provider_sdk::initialize_observability;
use wasmcloud_provider_sdk::{
    run_provider, serve_provider_exports, Context, Provider, ProviderInitConfig,
};

pub(crate) mod bindings {
    wit_bindgen_wrpc::generate!();
}

use bindings::exports::betty_blocks::data_api::data_api::{Handler, HelperContext};
#[derive(Default, Clone)]
pub struct DataApiProvider {}

impl DataApiProvider {
    fn name() -> &'static str {
        "data-api-provider"
    }

    pub async fn run() -> anyhow::Result<()> {
        initialize_observability!(
            Self::name(),
            std::env::var_os("DATA_API_PROVIDER_FLAMEGRAPH_PATH")
        );
        let provider = Self::default();
        let shutdown = run_provider(provider.clone(), DataApiProvider::name())
            .await
            .context("failed to run provider")?;

        let connection = wasmcloud_provider_sdk::get_connection();
        serve_provider_exports(
            &connection
                .get_wrpc_client(connection.provider_key())
                .await
                .context("failed to get wrpc client")?,
            provider,
            shutdown,
            bindings::serve,
        )
        .await
    }
}
impl Handler<Option<Context>> for DataApiProvider {
    async fn request(
        &self,
        _ctx: Option<Context>,
        helper_context: HelperContext,
        query: String,
        variables: String,
    ) -> anyhow::Result<Result<String, String>> {
        if query.contains("create") {
            return Ok(Ok(serde_json::json!({
                "createtest": {
                    "id": "1"
                }
            })
            .to_string()));
        }

        if query.contains("update") {
            return Ok(Ok(serde_json::json!({
                "updatetest" : {
                    "id": "test"
                }
            })
            .to_string()));
        }

        Ok(Ok(serde_json::json!({
            "onetest" : {
                "id": "test"
            }
        })
        .to_string()))
    }
}

impl Provider for DataApiProvider {
    async fn init(&self, _config: impl ProviderInitConfig) -> anyhow::Result<()> {
        Ok(())
    }
}
