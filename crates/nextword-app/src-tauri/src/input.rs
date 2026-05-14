//! Wire macOS input capture into the prediction pipeline.
//!
//! Listens to KeyEvent over a channel from the CGEventTap, maintains a
//! keylog context buffer (used when AX doesn't return text), applies the
//! trigger rules, debounces, and finally fires the predictor. For M3 we
//! log the result — M4 will pipe it into the floating window instead.

use std::sync::Arc;

use parking_lot::Mutex;
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use nextword_core::context::{trim_to_sentence_start, ContextBuffer};
use nextword_core::trigger::{should_trigger, TriggerInput};

use crate::AppState;

#[cfg(target_os = "macos")]
use nextword_macos::{ax, keytap, keytap::KeyEvent};

#[cfg(target_os = "macos")]
pub fn install(app: AppHandle) -> anyhow::Result<()> {
    let (tx, rx) = mpsc::unbounded_channel::<KeyEvent>();
    keytap::install(tx).map_err(|e| anyhow::anyhow!("keytap install failed: {e}"))?;
    tauri::async_runtime::spawn(consume_events(app, rx));
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn install(_app: AppHandle) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(target_os = "macos")]
async fn consume_events(app: AppHandle, mut rx: mpsc::UnboundedReceiver<KeyEvent>) {
    let buffer = Arc::new(ContextBuffer::new());
    let in_flight: Arc<Mutex<Option<CancellationToken>>> = Arc::new(Mutex::new(None));

    while let Some(event) = rx.recv().await {
        match event {
            KeyEvent::Char(c) => {
                buffer.push_char(c);
            }
            KeyEvent::Backspace => {
                // Cheap: pop one char off the keylog. AX reads will reconcile next trigger.
                let mut s = buffer.snapshot();
                s.pop();
                buffer.replace(&s);
            }
            KeyEvent::Enter => {
                buffer.push_char('\n');
            }
            KeyEvent::Space => {
                buffer.push_char(' ');
                let app = app.clone();
                let buffer = buffer.clone();
                let in_flight = in_flight.clone();
                tauri::async_runtime::spawn(async move {
                    handle_space(app, buffer, in_flight).await;
                });
            }
            KeyEvent::FocusChanged => {
                buffer.clear();
            }
            // M4 will react to these; in M3 we just ignore them.
            KeyEvent::Tab | KeyEvent::Escape | KeyEvent::Cmd1 | KeyEvent::Cmd2 | KeyEvent::Cmd3
            | KeyEvent::ModifierOnly => {}
        }
    }
}

#[cfg(target_os = "macos")]
async fn handle_space(
    app: AppHandle,
    buffer: Arc<ContextBuffer>,
    in_flight: Arc<Mutex<Option<CancellationToken>>>,
) {
    let (context, is_secure) = resolve_context(&buffer);

    let input = TriggerInput {
        context: &context,
        key_is_space: true,
        modifier_pressed: false,
        is_secure_field: is_secure,
    };
    if !should_trigger(input) {
        tracing::trace!(context = %context, "trigger rejected");
        return;
    }

    // Cancel any prior request, install our own token.
    let cancel = CancellationToken::new();
    {
        let mut slot = in_flight.lock();
        if let Some(old) = slot.take() {
            old.cancel();
        }
        *slot = Some(cancel.clone());
    }

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    if cancel.is_cancelled() {
        return;
    }

    let predictor = app.state::<AppState>().predictor.lock().clone();
    let predictor = match predictor {
        Some(p) => p,
        None => {
            tracing::warn!("space pressed but predictor not ready");
            return;
        }
    };

    let start = std::time::Instant::now();
    match predictor.predict(&context, cancel.clone()).await {
        Ok(words) => {
            let elapsed_ms = start.elapsed().as_millis();
            tracing::info!(
                elapsed_ms,
                context = %context.chars().rev().take(40).collect::<String>().chars().rev().collect::<String>(),
                suggestions = ?words,
                "prediction"
            );
        }
        Err(nextword_core::CoreError::Cancelled) => {
            tracing::trace!("prediction cancelled");
        }
        Err(e) => {
            tracing::warn!(error = %e, "prediction failed");
        }
    }
}

#[cfg(target_os = "macos")]
fn resolve_context(keylog: &ContextBuffer) -> (String, bool) {
    match ax::get_focused_text_context() {
        Ok(ctx) if !ctx.text_before_caret.is_empty() => {
            (trim_to_sentence_start(&ctx.text_before_caret, 256), ctx.is_secure)
        }
        Ok(ctx) if ctx.is_secure => (String::new(), true),
        _ => (trim_to_sentence_start(&keylog.snapshot(), 256), false),
    }
}
