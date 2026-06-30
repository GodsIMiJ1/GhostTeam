#[path = "../telegram_bridge.rs"]
mod telegram_bridge;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    telegram_bridge::run().await
}
