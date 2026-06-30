mod api;
mod agent;
mod cli;
mod db;
mod konnect;
mod logging;
mod model;
mod roles;
mod tasks;

use clap::Parser;
use std::env;

#[tokio::main]
async fn main() {
    logging::init_logging();

    let cli = cli::Cli::parse();

    let result = if cli.api {
        let port = env::var("GHOSTTEAM_API_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(3000);
        api::server::start_api_server(port).await.map_err(anyhow::Error::from)
    } else {
        cli::run(cli)
    };

    if let Err(error) = result {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
