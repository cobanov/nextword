//! Library entry-point for the Tauri app. Keeps main.rs thin and lets
//! integration tests pull symbols from here.

pub mod input;
pub mod ipc;
pub mod model_download;
pub mod sidecar;
pub mod window;

use std::sync::Arc;

use parking_lot::Mutex;
use tauri::{AppHandle, Emitter, Manager, RunEvent};

use nextword_core::{Predictor, PredictorConfig};

/// Shared app state. The supervisor is the source of truth for the sidecar
/// child; the predictor is rebuilt whenever the sidecar gets a new port.
#[derive(Default)]
pub struct AppState {
    pub predictor: Mutex<Option<Predictor>>,
    pub supervisor: Mutex<Option<Arc<sidecar::Supervisor>>>,
}

impl AppState {
    pub fn install_supervisor(&self, sup: Arc<sidecar::Supervisor>) {
        *self.supervisor.lock() = Some(sup);
    }

    pub fn install_predictor(&self, base_url: String) {
        *self.predictor.lock() = Some(Predictor::new(PredictorConfig {
            base_url,
            ..Default::default()
        }));
    }

    pub fn clear_predictor(&self) {
        *self.predictor.lock() = None;
    }
}

pub fn run() {
    init_logging();
    install_panic_hook();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState::default())
        .setup(|app| {
            let handle: AppHandle = app.handle().clone();
            forward_sidecar_state(&handle);
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
            ipc::cmd_retry_sidecar,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app, event| {
        if let RunEvent::ExitRequested { .. } | RunEvent::Exit = event {
            let sup = app.state::<AppState>().supervisor.lock().clone();
            if let Some(sup) = sup {
                sup.kill();
            }
        }
    });
}

pub(crate) async fn bootstrap(app: AppHandle) -> anyhow::Result<()> {
    let _ = app.emit("status:update", serde_json::json!({"state": "starting"}));

    let model_path = model_download::default_model_path()?;
    if !model_path.exists() {
        let _ = app.emit(
            "status:update",
            serde_json::json!({
                "state": "needs_model",
                "message": "Model not found. The M5 downloader will land soon. For now place a GGUF at:",
                "path": model_path.display().to_string(),
            }),
        );
        return Ok(());
    }

    let _ = app.emit("status:update", serde_json::json!({"state": "starting_sidecar"}));

    let supervisor = sidecar::Supervisor::new(app.clone(), model_path);
    let handle = match supervisor.start().await {
        Ok(h) => h,
        Err(e) => {
            tracing::error!(error = ?e, "supervisor failed to start sidecar");
            let _ = app.emit(
                "status:update",
                serde_json::json!({
                    "state": "error",
                    "message": format!("Could not start llama-server: {e}"),
                }),
            );
            // Still install the supervisor so Retry can reuse it.
            app.state::<AppState>().install_supervisor(supervisor);
            return Ok(());
        }
    };
    let base_url = handle.base_url();
    let state = app.state::<AppState>();
    state.install_supervisor(supervisor);
    state.install_predictor(base_url);

    let _ = app.emit("status:update", serde_json::json!({"state": "ready"}));

    // M3: install the keystroke listener. Requires Accessibility trust.
    #[cfg(target_os = "macos")]
    {
        if check_and_prompt_ax_permission(&app) {
            if let Err(e) = input::install(app.clone()) {
                tracing::error!(error = %e, "failed to install input listener");
                let _ = app.emit(
                    "status:update",
                    serde_json::json!({
                        "state": "error",
                        "message": format!("Input listener install failed: {e}"),
                    }),
                );
            }
        } else {
            let _ = app.emit(
                "status:update",
                serde_json::json!({
                    "state": "needs_ax_permission",
                    "message": "NextWord needs Accessibility access. Open System Settings → Privacy → Accessibility and toggle NextWord on, then quit and relaunch.",
                }),
            );
        }
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn check_and_prompt_ax_permission(_app: &AppHandle) -> bool {
    use nextword_macos::permissions;
    if permissions::is_trusted() {
        return true;
    }
    // Pop the system dialog the first time so the user can grant access without
    // hunting for it; subsequent launches just observe is_trusted().
    permissions::prompt_if_needed();
    permissions::is_trusted()
}

/// Re-emit sidecar:state events as status:update so the main window UI
/// only has to subscribe to one channel. Crash + ready get translated.
fn forward_sidecar_state(app: &AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        use tauri::Listener;
        handle.clone().listen("sidecar:state", move |event| {
            let payload: serde_json::Value =
                serde_json::from_str(event.payload()).unwrap_or(serde_json::Value::Null);
            let state = payload.get("state").and_then(|v| v.as_str()).unwrap_or("");
            let mapped = match state {
                "starting" => "starting_sidecar",
                "ready" => "ready",
                "crashed" => "crashed",
                _ => state,
            };
            let mut forwarded = payload.clone();
            forwarded["state"] = serde_json::Value::String(mapped.to_string());
            let _ = handle.emit("status:update", forwarded);

            // Drop the predictor as soon as the sidecar is gone so callers fail
            // fast instead of hanging on a dead port.
            if state == "crashed" {
                handle.state::<AppState>().clear_predictor();
            } else if state == "ready" {
                if let Some(base_url) = payload.get("base_url").and_then(|v| v.as_str()) {
                    handle.state::<AppState>().install_predictor(base_url.to_string());
                }
            }
        });
    });
}

fn init_logging() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_env("NEXTWORD_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info,nextword=debug"));
    let _ = fmt().with_env_filter(filter).with_target(false).try_init();
}

fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(%info, "panic; killing sidecar before unwinding");
        // We have no AppHandle at panic time; rely on the OS to reap the
        // Tauri-managed child. Drop() on SidecarHandle does the kill on
        // normal exits; the panic path just logs.
        prev(info);
    }));
}
