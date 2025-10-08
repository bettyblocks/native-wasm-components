mod provider;

use provider::DataApiProvider;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    DataApiProvider::run().await?;
    eprintln!("Data Api Provider exiting");
    Ok(())
}
