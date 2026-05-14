//! Tauri commands invoked from the webview.

use serde::{Deserialize, Serialize};
use tauri::State;
use tokio_util::sync::CancellationToken;

use crate::AppState;

#[derive(Serialize)]
pub struct StatusReply {
    pub ready: bool,
    pub sidecar_url: Option<String>,
}

#[tauri::command]
pub fn cmd_status(state: State<'_, AppState>) -> StatusReply {
    let sidecar = state.sidecar.lock();
    StatusReply {
        ready: sidecar.is_some() && state.predictor.lock().is_some(),
        sidecar_url: sidecar.as_ref().map(|h| h.base_url()),
    }
}

#[derive(Deserialize)]
pub struct PredictArgs {
    pub context: String,
}

#[tauri::command]
pub async fn cmd_predict(
    state: State<'_, AppState>,
    args: PredictArgs,
) -> Result<Vec<String>, String> {
    let predictor = state.predictor.lock().clone();
    let predictor = match predictor {
        Some(p) => p,
        None => return Err("predictor not ready".into()),
    };
    let cancel = CancellationToken::new();
    predictor.predict(&args.context, cancel)
        .await
        .map_err(|e| e.to_string())
}
