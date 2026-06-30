#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ghostteam::telegram_bridge::run().await
}
