mod provider;

use provider::GraphqlProvider;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    GraphqlProvider::run().await?;
    eprintln!("Graphql Provider exiting");
    Ok(())
}
