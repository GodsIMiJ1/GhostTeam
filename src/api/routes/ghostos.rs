use axum::{
    extract::Json,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Router,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use tokio::time::sleep;

use crate::model::ghostos::GhostOsConfig;

#[derive(Debug, Deserialize)]
pub struct InferRequest {
    pub prompt: String,
}

#[derive(Debug, Serialize)]
pub struct InferResponse {
    pub output: String,
}

pub fn router() -> Router {
    Router::new().route("/infer", post(infer))
}

pub async fn infer(Json(request): Json<InferRequest>) -> impl IntoResponse {
    match infer_with_config(&request.prompt).await {
        Ok(output) => (
            StatusCode::OK,
            Json(InferResponse { output }),
        )
            .into_response(),
        Err(error) => {
            log::error!("ghostos infer failed: {error}");
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": error.to_string()
                })),
            )
                .into_response()
        }
    }
}

async fn infer_with_config(prompt: &str) -> anyhow::Result<String> {
    let config = GhostOsConfig::load()?;
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(anyhow::Error::from)?;

    let mut last_error = None;

    for attempt in 1..=3 {
        log::debug!(
            "ghostos api request attempt={} endpoint={} model={} prompt_bytes={}",
            attempt,
            config.ghostos_endpoint,
            config.ghostos_model,
            prompt.len()
        );

        match client
            .post(&config.ghostos_endpoint)
            .json(&serde_json::json!({
                "model": config.ghostos_model,
                "prompt": prompt
            }))
            .send()
            .await
        {
            Ok(response) => match response.error_for_status() {
                Ok(ok_response) => match ok_response.json::<Value>().await {
                    Ok(value) => {
                        if let Some(output) = value.get("output").and_then(|entry| entry.as_str()) {
                            log::debug!(
                                "ghostos api response endpoint={} output_bytes={}",
                                config.ghostos_endpoint,
                                output.len()
                            );
                            return Ok(output.to_string());
                        }

                        let error = anyhow::anyhow!(
                            "ghostos response missing output field from {}",
                            config.ghostos_endpoint
                        );
                        log::error!("{error}");
                        last_error = Some(error);
                    }
                    Err(error) => {
                        log::error!(
                            "failed to decode ghostos api response from {}: {error}",
                            config.ghostos_endpoint
                        );
                        last_error = Some(error.into());
                    }
                },
                Err(error) => {
                    log::error!(
                        "ghostos api returned error status from {}: {error}",
                        config.ghostos_endpoint
                    );
                    last_error = Some(error.into());
                }
            },
            Err(error) => {
                log::error!(
                    "ghostos api request failed attempt={} endpoint={} error={error}",
                    attempt,
                    config.ghostos_endpoint
                );
                last_error = Some(error.into());
            }
        }

        if attempt < 3 {
            let backoff_ms = 100_u64 * (1_u64 << (attempt - 1));
            log::debug!(
                "ghostos api retrying endpoint={} backoff_ms={}",
                config.ghostos_endpoint,
                backoff_ms
            );
            sleep(Duration::from_millis(backoff_ms)).await;
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("ghostos inference failed")))
}
