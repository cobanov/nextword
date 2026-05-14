//! Library entry-point for the Tauri app. Keeps main.rs thin and lets
//! integration tests pull symbols from here.

pub mod ipc;
pub mod model_download;
pub mod sidecar;
pub mod window;

use std::sync::Arc;
use parking_lot::Mutex;
use tauri::{Manager, AppHandle};

use nextword_core::Predictor;

/// Shared app state — wrapped in Mutex/Option so we can swap the predictor
/// when the sidecar restarts on a new port.
#[derive(Default)]
pub struct AppState {
    pub predictor: Mutex<Option<Predictor>>,
    pub sidecar: Mutex<Option<Arc<sidecar::SidecarHandle>>>,
}

pub fn run() {
    init_logging();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState::default())
        .setup(|app| {
            let handle: AppHandle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = bootstrap(handle).await {
                    tracing::error!(error = ?e, "bootstrap failed");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc::cmd_status,
            ipc::cmd_predict,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

async fn bootstrap(app: AppHandle) -> anyhow::Result<()> {
    use tauri::Emitter;

    let _ = app.emit("status:update", serde_json::json!({"state": "starting"}));

    // M5 will check + download the model here. For M0 we just go straight to
    // sidecar startup if a model exists.
    let model_path = model_download::default_model_path()?;
    if !model_path.exists() {
        let _ = app.emit("status:update", serde_json::json!({
            "state": "needs_model",
            "message": "Model not found. The M5 downloader will land soon. For now drop a GGUF at:",
            "path": model_path.display().to_string(),
        }));
        return Ok(());
    }

    let _ = app.emit("status:update", serde_json::json!({"state": "starting_sidecar"}));
    let handle = sidecar::start(&app, &model_path).await?;
    let base_url = handle.base_url();
    {
        let state = app.state::<AppState>();
        *state.sidecar.lock() = Some(Arc::new(handle));
        *state.predictor.lock() = Some(Predictor::new(nextword_core::PredictorConfig {
            base_url,
            ..Default::default()
        }));
    }

    let _ = app.emit("status:update", serde_json::json!({"state": "ready"}));
    Ok(())
}

fn init_logging() {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = EnvFilter::try_from_env("NEXTWORD_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info,nextword=debug"));

    let _ = fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}
