mod provider;

use provider::SmtpProvider;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    SmtpProvider::run().await?;
    eprintln!("SMTP provider exiting");
    Ok(())
}
