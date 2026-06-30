use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use super::ModelBackend;

const CONFIG_FILE: &str = "config.yaml";
const CONFIG_DIR: &str = ".ghostteam";
const WORKSPACE_DIR_ENV: &str = "GHOSTTEAM_WORKSPACE_DIR";

#[derive(Debug, Default, Clone)]
pub struct GhostOsBackend;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GhostOsConfig {
    pub ghostos_endpoint: String,
    pub ghostos_model: String,
}

impl Default for GhostOsConfig {
    fn default() -> Self {
        Self {
            ghostos_endpoint: "http://localhost:9000/infer".to_string(),
            ghostos_model: "ghost-1".to_string(),
        }
    }
}

impl GhostOsConfig {
    pub fn load() -> Result<Self> {
        let path = config_path();
        let mut config = if path.exists() {
            let contents = fs::read_to_string(&path).with_context(|| {
                format!("failed to read GhostOS config at {}", path.display())
            })?;
            serde_yaml::from_str(&contents).with_context(|| {
                format!("failed to parse GhostOS config at {}", path.display())
            })?
        } else {
            log::info!(
                "ghostos config not found at {}, using defaults",
                path.display()
            );
            Self::default()
        };

        if let Ok(endpoint) = env::var("GHOSTTEAM_GHOSTOS_ENDPOINT") {
            log::debug!("ghostos endpoint overridden from environment");
            config.ghostos_endpoint = endpoint;
        }

        if let Ok(model) = env::var("GHOSTTEAM_GHOSTOS_MODEL") {
            log::debug!("ghostos model overridden from environment");
            config.ghostos_model = model;
        }

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create config directory at {}", parent.display())
            })?;
        }

        let contents = serde_yaml::to_string(self).context("failed to serialize GhostOS config")?;
        fs::write(&path, contents)
            .with_context(|| format!("failed to write GhostOS config at {}", path.display()))?;
        Ok(())
    }
}

fn config_path() -> PathBuf {
    let workspace = env::var_os(WORKSPACE_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(CONFIG_DIR));
    workspace.join(CONFIG_FILE)
}

impl ModelBackend for GhostOsBackend {
    fn generate(&self, prompt: &str) -> Result<String> {
        let config = GhostOsConfig::load()?;
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .context("failed to build GhostOS HTTP client")?;

        let mut last_error = None;
        for attempt in 1..=3 {
            log::debug!(
                "ghostos request attempt={} endpoint={} model={} prompt_bytes={}",
                attempt,
                config.ghostos_endpoint,
                config.ghostos_model,
                prompt.len()
            );

            match client
                .post(&config.ghostos_endpoint)
                .json(&json!({
                    "model": config.ghostos_model,
                    "prompt": prompt
                }))
                .send()
            {
                Ok(response) => match response.error_for_status() {
                    Ok(ok_response) => match ok_response.json::<Value>() {
                        Ok(value) => {
                            if let Some(output) = value.get("output").and_then(|entry| entry.as_str()) {
                                log::debug!(
                                    "ghostos response endpoint={} output_bytes={}",
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
                                "failed to decode GhostOS response from {}: {error}",
                                config.ghostos_endpoint
                            );
                            last_error = Some(error.into());
                        }
                    },
                    Err(error) => {
                        log::error!(
                            "ghostos returned error status from {}: {error}",
                            config.ghostos_endpoint
                        );
                        last_error = Some(error.into());
                    }
                },
                Err(error) => {
                    log::error!(
                        "ghostos request failed attempt={} endpoint={} error={error}",
                        attempt,
                        config.ghostos_endpoint
                    );
                    last_error = Some(error.into());
                }
            }

            if attempt < 3 {
                let backoff_ms = 100_u64 * (1_u64 << (attempt - 1));
                log::debug!(
                    "ghostos retrying endpoint={} backoff_ms={}",
                    config.ghostos_endpoint,
                    backoff_ms
                );
                thread::sleep(Duration::from_millis(backoff_ms));
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("ghostos generation failed")))
    }
}

pub fn connect() -> Result<()> {
    Ok(())
}
