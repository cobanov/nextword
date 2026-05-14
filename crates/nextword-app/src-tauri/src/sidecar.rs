//! Sidecar lifecycle: spawn `llama-server`, poll /health, surface crashes,
//! quit cleanly. M6 will add the 3-strikes auto-restart policy; for now a
//! crashed sidecar just notifies the UI and waits for a manual Retry.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};

const HEALTH_TIMEOUT_SECS: u64 = 30;
const HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(250);

pub struct SidecarHandle {
    child: Mutex<Option<CommandChild>>,
    port: u16,
}

impl SidecarHandle {
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn kill(&self) {
        if let Some(child) = self.child.lock().take() {
            let _ = child.kill();
        }
    }
}

impl Drop for SidecarHandle {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Owns the most recent SidecarHandle and the model path. The supervisor is
/// what `Retry` clicks act on.
pub struct Supervisor {
    app: AppHandle,
    model_path: PathBuf,
    handle: Mutex<Option<Arc<SidecarHandle>>>,
}

impl Supervisor {
    pub fn new(app: AppHandle, model_path: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            app,
            model_path,
            handle: Mutex::new(None),
        })
    }

    pub fn current(&self) -> Option<Arc<SidecarHandle>> {
        self.handle.lock().clone()
    }

    pub fn kill(&self) {
        if let Some(h) = self.handle.lock().take() {
            h.kill();
        }
    }

    /// Spawn the sidecar (killing any previous one) and wait for /health.
    /// On termination, the UI is notified via the `sidecar:state` event.
    pub async fn start(self: &Arc<Self>) -> Result<Arc<SidecarHandle>> {
        self.kill();
        let new_handle = self.spawn_and_wait().await?;
        *self.handle.lock() = Some(new_handle.clone());
        Ok(new_handle)
    }

    async fn spawn_and_wait(self: &Arc<Self>) -> Result<Arc<SidecarHandle>> {
        let port = pick_free_port()?;
        tracing::info!(port, model = %self.model_path.display(), "starting llama-server sidecar");

        let _ = self.app.emit(
            "sidecar:state",
            serde_json::json!({"state": "starting", "port": port}),
        );

        let (mut rx, child) = self
            .app
            .shell()
            .sidecar("llama-server")
            .context("llama-server sidecar not configured")?
            .args([
                "--host", "127.0.0.1",
                "--port", &port.to_string(),
                "-m", &self.model_path.to_string_lossy(),
                "-c", "2048",
                "-ngl", "999",
                "--no-warmup",
            ])
            .spawn()
            .context("spawn llama-server")?;

        let handle = Arc::new(SidecarHandle {
            child: Mutex::new(Some(child)),
            port,
        });

        // Watch the child for stdout/stderr and termination. On termination
        // notify the UI; the M6 supervisor will handle auto-restart. For now,
        // the user clicks Retry which triggers Supervisor::start again.
        let app_for_task = self.app.clone();
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    CommandEvent::Stdout(line) | CommandEvent::Stderr(line) => {
                        if let Ok(s) = std::str::from_utf8(&line) {
                            tracing::debug!(target: "llama-server", "{}", s.trim_end());
                        }
                    }
                    CommandEvent::Error(e) => {
                        tracing::error!("sidecar event error: {e}");
                    }
                    CommandEvent::Terminated(t) => {
                        tracing::warn!(code = ?t.code, signal = ?t.signal, "sidecar terminated");
                        let _ = app_for_task.emit(
                            "sidecar:state",
                            serde_json::json!({
                                "state": "crashed",
                                "fatal": true,
                                "code": t.code,
                                "message": "llama-server exited. Click Retry to start it again."
                            }),
                        );
                        break;
                    }
                    _ => {}
                }
            }
        });

        wait_for_health(&handle.base_url()).await?;
        let _ = self.app.emit(
            "sidecar:state",
            serde_json::json!({"state": "ready", "base_url": handle.base_url()}),
        );
        Ok(handle)
    }
}

fn pick_free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

async fn wait_for_health(base_url: &str) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(HEALTH_TIMEOUT_SECS);
    let url = format!("{base_url}/health");

    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow!("sidecar health check timed out"));
        }
        match client.get(&url).send().await {
            Ok(r) if r.status().is_success() => {
                tracing::info!(base_url, "sidecar healthy");
                return Ok(());
            }
            Ok(r) => tracing::trace!(status = %r.status(), "health not ready"),
            Err(e) => tracing::trace!(error = %e, "health poll failed"),
        }
        tokio::time::sleep(HEALTH_POLL_INTERVAL).await;
    }
}
