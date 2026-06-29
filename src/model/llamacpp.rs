use anyhow::{Context, Result};
use std::env;
use std::io::Write;
use std::process::{Command, Stdio};

use super::ModelBackend;

#[derive(Debug, Clone)]
pub struct LlamaCppBackend {
    binary: String,
    args: Vec<String>,
}

impl Default for LlamaCppBackend {
    fn default() -> Self {
        let binary = env::var("GHOSTTEAM_LLAMA_CPP_BIN").unwrap_or_else(|_| "llama-cli".to_string());
        let args = env::var("GHOSTTEAM_LLAMA_CPP_ARGS")
            .ok()
            .and_then(|value| serde_json::from_str::<Vec<String>>(&value).ok())
            .unwrap_or_default();
        Self { binary, args }
    }
}

impl ModelBackend for LlamaCppBackend {
    fn generate(&self, prompt: &str) -> Result<String> {
        log::debug!(
            "llama.cpp generate binary={} args={:?} prompt_bytes={}",
            self.binary,
            self.args,
            prompt.len()
        );
        let mut child = Command::new(&self.binary)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                log::error!("failed to spawn llama.cpp binary {}: {error}", self.binary);
                error
            })
            .with_context(|| format!("failed to spawn llama.cpp binary: {}", self.binary))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(prompt.as_bytes())
                .map_err(|error| {
                    log::error!("failed to write prompt to llama.cpp stdin for {}: {error}", self.binary);
                    error
                })
                .context("failed to write prompt to llama.cpp stdin")?;
        }

        let output = child
            .wait_with_output()
            .map_err(|error| {
                log::error!("failed to read llama.cpp output from {}: {error}", self.binary);
                error
            })
            .context("failed to read llama.cpp output")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::error!(
                "llama.cpp exited unsuccessfully binary={} status={} stderr={}",
                self.binary,
                output.status,
                stderr.trim()
            );
            return Err(anyhow::anyhow!(
                "llama.cpp exited with status {}: {}",
                output.status,
                stderr.trim()
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        log::debug!(
            "llama.cpp response binary={} bytes={}",
            self.binary,
            stdout.len()
        );
        Ok(stdout)
    }
}

pub fn connect() -> Result<()> {
    Ok(())
}
