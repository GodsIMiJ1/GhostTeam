use axum::{
    body::Body,
    http::{Request, StatusCode, header::HeaderName},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::{env, fs, path::PathBuf};

const API_KEYS_FILE: &str = "api_keys.yaml";
const WORKSPACE_DIR: &str = ".ghostteam";
const WORKSPACE_DIR_ENV: &str = "GHOSTTEAM_WORKSPACE_DIR";
const HEADER_NAME: &str = "x-ghostteam-key";

#[derive(Debug, Deserialize)]
struct ApiKeysConfig {
    keys: Vec<String>,
}

pub async fn require_api_key(request: Request<Body>, next: Next) -> Response {
    let provided_key = request
        .headers()
        .get(HeaderName::from_static(HEADER_NAME))
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .or_else(|| query_param(request.uri().query(), "api_key"))
        .or_else(|| query_param(request.uri().query(), "key"));

    let Some(provided_key) = provided_key else {
        log::warn!("api request rejected: missing X-GhostTeam-Key header");
        return unauthorized();
    };

    match load_api_keys() {
        Ok(config) if config.keys.iter().any(|key| key == &provided_key) => {
            log::debug!("api request authorized");
            next.run(request).await
        }
        Ok(_) => {
            log::warn!("api request rejected: invalid API key");
            unauthorized()
        }
        Err(error) => {
            log::error!("api request rejected: failed to load api keys: {error}");
            unauthorized()
        }
    }
}

fn load_api_keys() -> anyhow::Result<ApiKeysConfig> {
    let path = api_keys_path();
    let contents = fs::read_to_string(&path).map_err(|error| {
        anyhow::anyhow!("failed to read API keys file at {}: {error}", path.display())
    })?;
    let config = serde_yaml::from_str::<ApiKeysConfig>(&contents).map_err(|error| {
        anyhow::anyhow!("failed to parse API keys file at {}: {error}", path.display())
    })?;
    Ok(config)
}

fn api_keys_path() -> PathBuf {
    env::var_os(WORKSPACE_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(WORKSPACE_DIR))
        .join(API_KEYS_FILE)
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({
            "error": "unauthorized"
        })),
    )
        .into_response()
}

fn query_param(query: Option<&str>, key: &str) -> Option<String> {
    let query = query?;
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        if name == key { Some(value.to_string()) } else { None }
    })
}
