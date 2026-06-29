use anyhow::Result;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;
use std::env;
use std::time::Duration;

use super::ModelBackend;

#[derive(Debug, Clone)]
pub struct OllamaBackend {
    client: Client,
    endpoint: String,
    model: String,
}

impl Default for OllamaBackend {
    fn default() -> Self {
        let endpoint = env::var("GHOSTTEAM_OLLAMA_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:11434/api/generate".to_string());
        let model = env::var("GHOSTTEAM_OLLAMA_MODEL").unwrap_or_else(|_| "llama3".to_string());
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("failed to build reqwest client"),
            endpoint,
            model,
        }
    }
}

#[derive(Debug, Deserialize)]
struct OllamaGenerateResponse {
    response: Option<String>,
}

impl ModelBackend for OllamaBackend {
    fn generate(&self, prompt: &str) -> Result<String> {
        log::debug!(
            "ollama generate endpoint={} model={} prompt_bytes={}",
            self.endpoint,
            self.model,
            prompt.len()
        );
        let response = self
            .client
            .post(&self.endpoint)
            .json(&json!({
                "model": self.model,
                "prompt": prompt,
                "stream": false
            }))
            .send()
            .map_err(|error| {
                log::error!("ollama request failed endpoint={} error={error}", self.endpoint);
                error
            })?
            .error_for_status()
            .map_err(|error| {
                log::error!("ollama returned error endpoint={} error={error}", self.endpoint);
                error
            })?;

        let payload: OllamaGenerateResponse = response
            .json()
            .map_err(|error| {
                log::error!("failed to decode ollama response endpoint={}: {error}", self.endpoint);
                error
            })?;

        log::debug!(
            "ollama response endpoint={} bytes={}",
            self.endpoint,
            payload.response.as_ref().map(|value| value.len()).unwrap_or(0)
        );
        Ok(payload.response.unwrap_or_default())
    }
}

pub fn connect() -> Result<()> {
    Ok(())
}
