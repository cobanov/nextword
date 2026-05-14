//! Tauri commands invoked from the webview.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tokio_util::sync::CancellationToken;

use crate::AppState;

#[derive(Serialize)]
pub struct StatusReply {
    pub ready: bool,
    pub sidecar_url: Option<String>,
}

#[tauri::command]
pub fn cmd_status(state: State<'_, AppState>) -> StatusReply {
    let supervisor = state.supervisor.lock().clone();
    let predictor_ready = state.predictor.lock().is_some();
    let sidecar_url = supervisor.and_then(|s| s.current().map(|h| h.base_url()));
    StatusReply {
        ready: predictor_ready && sidecar_url.is_some(),
        sidecar_url,
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
    predictor
        .predict(&args.context, cancel)
        .await
        .map_err(|e| e.to_string())
}

/// Re-spawn the sidecar after a fatal crash. Returns the new base URL on
/// success.
#[tauri::command]
pub async fn cmd_retry_sidecar(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let supervisor = state.supervisor.lock().clone();
    let supervisor = match supervisor {
        Some(s) => s,
        None => return Err("no supervisor registered".into()),
    };
    let handle = supervisor.start().await.map_err(|e| e.to_string())?;
    let url = handle.base_url();
    state.install_predictor(url.clone());
    let _ = tauri::Emitter::emit(
        &app,
        "status:update",
        serde_json::json!({"state": "ready"}),
    );
    Ok(url)
}
