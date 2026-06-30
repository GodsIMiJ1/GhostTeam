use anyhow::{Result, anyhow};

pub mod ghostos;
pub mod llamacpp;
pub mod ollama;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Ollama,
    LlamaCpp,
    GhostOS,
}

impl BackendKind {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "ollama" => Ok(Self::Ollama),
            "llamacpp" | "llama.cpp" | "llama_cpp" => Ok(Self::LlamaCpp),
            "ghostos" => Ok(Self::GhostOS),
            other => Err(anyhow!("unknown backend kind: {other}")),
        }
    }
}

pub trait ModelBackend {
    fn generate(&self, prompt: &str) -> Result<String>;
}

pub fn backend_for(kind: BackendKind) -> Box<dyn ModelBackend + Send + Sync> {
    match kind {
        BackendKind::Ollama => Box::new(ollama::OllamaBackend::default()),
        BackendKind::LlamaCpp => Box::new(llamacpp::LlamaCppBackend::default()),
        BackendKind::GhostOS => Box::new(ghostos::GhostOsBackend::default()),
    }
}
