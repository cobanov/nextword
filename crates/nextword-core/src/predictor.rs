//! llama-server `/completion` HTTP client. Fleshed out in M2.

use std::time::Duration;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::{parser, CoreError, Result};

#[derive(Debug, Clone)]
pub struct PredictorConfig {
    pub base_url: String,
    pub n_predict: u32,
    pub temperature: f32,
    pub top_k: u32,
    pub top_p: f32,
    pub timeout: Duration,
    pub stop_tokens: Vec<String>,
    pub variants: usize,
}

impl Default for PredictorConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:8080".into(),
            n_predict: 4,
            temperature: 0.7,
            top_k: 20,
            top_p: 0.9,
            timeout: Duration::from_millis(500),
            stop_tokens: vec![
                " ".into(), "\n".into(), ".".into(), ",".into(),
                "!".into(), "?".into(), ";".into(), ":".into(),
            ],
            variants: 3,
        }
    }
}

#[derive(Serialize)]
struct CompletionRequest<'a> {
    prompt: &'a str,
    n_predict: u32,
    temperature: f32,
    top_k: u32,
    top_p: f32,
    stop: &'a [String],
    cache_prompt: bool,
    n_keep: i32,
    seed: i64,
}

#[derive(Deserialize)]
struct CompletionResponse {
    content: String,
}

#[derive(Debug, Clone)]
pub struct Predictor {
    config: PredictorConfig,
    http: reqwest::Client,
}

impl Predictor {
    pub fn new(config: PredictorConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("reqwest client init");
        Self { config, http }
    }

    pub fn config(&self) -> &PredictorConfig {
        &self.config
    }

    /// Run N parallel sampling calls, parse, dedupe, return up to 3 words.
    pub async fn predict(&self, context: &str, cancel: CancellationToken) -> Result<Vec<String>> {
        let url = format!("{}/completion", self.config.base_url);
        let cfg = self.config.clone();

        let mut tasks = Vec::with_capacity(cfg.variants);
        for i in 0..cfg.variants {
            let req = CompletionRequest {
                prompt: context,
                n_predict: cfg.n_predict,
                temperature: cfg.temperature,
                top_k: cfg.top_k,
                top_p: cfg.top_p,
                stop: &cfg.stop_tokens,
                cache_prompt: true,
                n_keep: -1,
                seed: (i as i64) * 1009 + 1,
            };
            let body = serde_json::to_vec(&req)?;
            let url = url.clone();
            let http = self.http.clone();
            let cancel = cancel.clone();

            tasks.push(tokio::spawn(async move {
                tokio::select! {
                    _ = cancel.cancelled() => Err(CoreError::Cancelled),
                    res = http.post(&url).header("Content-Type", "application/json").body(body).send() => {
                        let resp = res.map_err(CoreError::Predictor)?;
                        let parsed: CompletionResponse = resp.json().await.map_err(CoreError::Predictor)?;
                        Ok::<String, CoreError>(parsed.content)
                    }
                }
            }));
        }

        let mut raw: Vec<String> = Vec::with_capacity(cfg.variants);
        for t in tasks {
            match t.await {
                Ok(Ok(s)) => raw.push(s),
                Ok(Err(CoreError::Cancelled)) => return Err(CoreError::Cancelled),
                Ok(Err(e)) => tracing::warn!(error = %e, "completion variant failed"),
                Err(e) => tracing::warn!(error = %e, "completion task join failed"),
            }
        }

        if raw.is_empty() {
            return Err(CoreError::Other("no completions returned".into()));
        }

        Ok(parser::parse_suggestions(&raw, context))
    }
}
