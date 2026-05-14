//! Platform-agnostic core for NextWord.
//!
//! No Tauri, no macOS-only deps. Holds the predictor client, context buffer,
//! debouncer, trigger rules, and response parser.

pub mod context;
pub mod debounce;
pub mod parser;
pub mod predictor;
pub mod trigger;

pub use context::ContextBuffer;
pub use debounce::Debouncer;
pub use parser::parse_suggestions;
pub use predictor::{Predictor, PredictorConfig};
pub use trigger::{should_trigger, TriggerInput};

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("predictor request failed: {0}")]
    Predictor(#[from] reqwest::Error),

    #[error("predictor returned malformed json: {0}")]
    BadJson(#[from] serde_json::Error),

    #[error("request was cancelled")]
    Cancelled,

    #[error("{0}")]
    Other(String),
}

pub type Result<T, E = CoreError> = std::result::Result<T, E>;
