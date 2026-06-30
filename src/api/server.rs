use std::net::SocketAddr;

use axum::{middleware, Router};
use thiserror::Error;
use tokio::net::TcpListener;

use crate::api::{auth, routes};

#[derive(Debug, Error)]
pub enum ApiServerError {
    #[error("failed to bind API server on {addr}: {source}")]
    Bind {
        addr: SocketAddr,
        #[source]
        source: std::io::Error,
    },
    #[error("API server failed: {0}")]
    Serve(#[from] std::io::Error),
}

pub fn build_router() -> Router {
    let public_router = Router::new().merge(routes::dashboard::router());
    let api_router = Router::new()
        .nest("/agents", routes::agents::router())
        .nest("/tasks", routes::tasks::router())
        .nest("/messages", routes::messages::router())
        .nest("/logs", routes::logs::router())
        .nest("/ghostos", routes::ghostos::router())
        .layer(middleware::from_fn(auth::require_api_key));

    public_router.merge(api_router)
}

pub async fn start_api_server(port: u16) -> Result<(), ApiServerError> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|source| ApiServerError::Bind { addr, source })?;

    log::info!("starting GhostTeam API server on {addr}");
    axum::serve(listener, build_router())
        .await
        .map_err(ApiServerError::from)
        .map(|_| ())
}
