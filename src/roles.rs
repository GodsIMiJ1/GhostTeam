use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::PathBuf;

const WORKSPACE_DIR_ENV: &str = "GHOSTTEAM_WORKSPACE_DIR";

pub fn load_roles() -> Result<()> {
    Ok(())
}

pub fn load_role(role_name: &str) -> Result<String> {
    let workspace = env::var_os(WORKSPACE_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".ghostteam"));
    let path = workspace
        .join("roles")
        .join(format!("{role_name}.md"));
    let prompt = fs::read_to_string(&path)
        .with_context(|| format!("failed to read role prompt at {}", path.display()))?;
    Ok(prompt)
}

pub fn load_role_prompt(role: &str) -> Result<String> {
    load_role(role)
}
