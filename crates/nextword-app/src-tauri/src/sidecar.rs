//! Sidecar lifecycle: spawn `llama-server`, read the port it bound, poll
//! /health, and arrange a clean shutdown on app quit.
//!
//! In M0 we only stub out a SidecarHandle. M1 fills this in.

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};

const HEALTH_TIMEOUT: Duration = Duration::from_secs(30);
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

pub async fn start(app: &AppHandle, model_path: &Path) -> Result<SidecarHandle> {
    let port = pick_free_port()?;
    tracing::info!(port, model = %model_path.display(), "starting llama-server sidecar");

    let shell = app.shell();
    let cmd = shell
        .sidecar("llama-server")
        .context("llama-server sidecar not configured")?
        .args([
            "--host", "127.0.0.1",
            "--port", &port.to_string(),
            "-m", &model_path.to_string_lossy(),
            "-c", "2048",
            "-ngl", "999",
            "--no-warmup",
        ]);

    let (mut rx, child) = cmd.spawn().context("spawn llama-server")?;

    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(line) => {
                    if let Ok(s) = std::str::from_utf8(&line) {
                        tracing::debug!(target: "llama-server", "{}", s.trim_end());
                    }
                }
                CommandEvent::Stderr(line) => {
                    if let Ok(s) = std::str::from_utf8(&line) {
                        tracing::debug!(target: "llama-server", "{}", s.trim_end());
                    }
                }
                CommandEvent::Error(e) => tracing::error!("sidecar error: {e}"),
                CommandEvent::Terminated(t) => {
                    tracing::warn!(code = ?t.code, signal = ?t.signal, "sidecar terminated");
                    break;
                }
                _ => {}
            }
        }
    });

    let handle = SidecarHandle {
        child: Mutex::new(Some(child)),
        port,
    };

    wait_for_health(&handle.base_url()).await?;
    Ok(handle)
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
    let deadline = tokio::time::Instant::now() + HEALTH_TIMEOUT;
    let url = format!("{base_url}/health");

    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow!("sidecar health check timed out"));
        }
        match client.get(&url).send().await {
            Ok(r) if r.status().is_success() => {
                tracing::info!("sidecar healthy");
                return Ok(());
            }
            Ok(r) => tracing::trace!(status = %r.status(), "health not ready"),
            Err(e) => tracing::trace!(error = %e, "health poll failed"),
        }
        tokio::time::sleep(HEALTH_POLL_INTERVAL).await;
    }
}
