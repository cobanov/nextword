//! First-launch model downloader. M0 stub: just returns the expected path.
//! M5 fills in HTTP stream download + progress events + hash verification.

use std::path::PathBuf;
use anyhow::{Context, Result};

pub const DEFAULT_MODEL_FILE: &str = "Llama-3.2-1B-Instruct-Q4_K_M.gguf";
pub const DEFAULT_MODEL_URL: &str = "https://huggingface.co/bartowski/Llama-3.2-1B-Instruct-GGUF/resolve/main/Llama-3.2-1B-Instruct-Q4_K_M.gguf";
pub const DEFAULT_MODEL_SHA256: &str = ""; // M5: paste verified hash here

pub fn app_support_dir() -> Result<PathBuf> {
    let base = dirs_home()?;
    Ok(base.join("Library").join("Application Support").join("NextWord"))
}

pub fn models_dir() -> Result<PathBuf> {
    Ok(app_support_dir()?.join("models"))
}

pub fn default_model_path() -> Result<PathBuf> {
    Ok(models_dir()?.join(DEFAULT_MODEL_FILE))
}

fn dirs_home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME not set")
}
