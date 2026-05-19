//! Wire macOS input capture into the prediction pipeline + floating panel.
//!
//! - Owns a ContextBuffer (keylog fallback) and the latest live suggestions.
//! - On Space: resolve context (AX-first, keylog fallback), run trigger
//!   rules, debounce 50ms, ask the predictor, then show the floating panel.
//! - On Accept(n): synthesise insertion of the matching suggestion plus a
//!   trailing space, hide the panel.
//! - On DismissRequest, FocusChanged, or any text change while visible:
//!   hide the panel.

use std::sync::Arc;

use parking_lot::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use nextword_core::context::{trim_to_sentence_start, ContextBuffer};
use nextword_core::trigger::{should_trigger, TriggerInput};

use crate::{window, AppState};

#[cfg(target_os = "macos")]
use nextword_macos::{ax, caret, insert, keytap, keytap::KeyEvent, keytap::TapState};

#[cfg(target_os = "macos")]
pub fn install(app: AppHandle) -> anyhow::Result<()> {
    let (tx, rx) = mpsc::unbounded_channel::<KeyEvent>();
    let tap_state = Arc::new(TapState::default());
    keytap::install(tx, tap_state.clone())
        .map_err(|e| anyhow::anyhow!("keytap install failed: {e}"))?;
    // Make sure the suggestions window is built once up-front so we don't
    // pay creation latency on the first space press.
    let _ = window::ensure_suggestions_window(&app);
    tauri::async_runtime::spawn(consume_events(app, rx, tap_state));
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn install(_app: AppHandle) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(target_os = "macos")]
async fn consume_events(
    app: AppHandle,
    mut rx: mpsc::UnboundedReceiver<KeyEvent>,
    tap_state: Arc<TapState>,
) {
    let buffer = Arc::new(ContextBuffer::new());
    let suggestions: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let in_flight: Arc<Mutex<Option<CancellationToken>>> = Arc::new(Mutex::new(None));

    while let Some(event) = rx.recv().await {
        match event {
            KeyEvent::Char(c) => {
                buffer.push_char(c);
                // Panel stays open while user keeps typing; the next space
                // press will refresh the suggestions.
            }
            KeyEvent::Backspace => {
                let mut s = buffer.snapshot();
                s.pop();
                buffer.replace(&s);
            }
            KeyEvent::Enter => {
                buffer.push_char('\n');
                // Sticky-mode: panel stays open across line breaks too.
            }
            KeyEvent::Space => {
                buffer.push_char(' ');
                let app = app.clone();
                let buffer = buffer.clone();
                let in_flight = in_flight.clone();
                let suggestions = suggestions.clone();
                let tap_state = tap_state.clone();
                tauri::async_runtime::spawn(async move {
                    handle_space(app, buffer, in_flight, suggestions, tap_state).await;
                });
            }
            KeyEvent::Accept(n) => {
                let word = suggestions.lock().get(n as usize).cloned();
                if let Some(word) = word {
                    let to_insert = format!("{word} ");
                    if let Err(e) = insert::insert_text(&to_insert) {
                        tracing::warn!(error = %e, "failed to insert suggestion");
                    } else {
                        buffer.push_str(&to_insert);
                    }
                    // Immediately refresh suggestions for the post-insert
                    // context so the user can chain Tab presses.
                    let app = app.clone();
                    let buffer = buffer.clone();
                    let in_flight = in_flight.clone();
                    let suggestions = suggestions.clone();
                    let tap_state = tap_state.clone();
                    tauri::async_runtime::spawn(async move {
                        handle_space(app, buffer, in_flight, suggestions, tap_state).await;
                    });
                }
            }
            KeyEvent::DismissRequest => {
                // User pressed Esc; user wants the panel sticky so we honour
                // sticky-mode and ignore. Flip to hide_panel later if we
                // ever add an explicit "close" affordance.
            }
            KeyEvent::FocusChanged => {
                buffer.clear();
                // Panel stays where it is; new app's context will fill in
                // on the next space.
            }
            KeyEvent::Tab | KeyEvent::Escape | KeyEvent::Cmd1 | KeyEvent::Cmd2 | KeyEvent::Cmd3
            | KeyEvent::ModifierOnly => {
                // Panel-not-visible paths. Nothing for us to do here.
            }
        }
    }
}

#[cfg(target_os = "macos")]
async fn handle_space(
    app: AppHandle,
    buffer: Arc<ContextBuffer>,
    in_flight: Arc<Mutex<Option<CancellationToken>>>,
    suggestions: Arc<Mutex<Vec<String>>>,
    tap_state: Arc<TapState>,
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
    let Some(predictor) = predictor else {
        tracing::warn!("space pressed but predictor not ready");
        return;
    };

    let start = std::time::Instant::now();
    let words = match predictor.predict(&context, cancel.clone()).await {
        Ok(w) => w,
        Err(nextword_core::CoreError::Cancelled) => return,
        Err(e) => {
            tracing::warn!(error = %e, "prediction failed");
            return;
        }
    };
    let elapsed_ms = start.elapsed().as_millis();
    tracing::info!(elapsed_ms, suggestions = ?words, "prediction");

    if words.is_empty() {
        return;
    }

    *suggestions.lock() = words.clone();
    let caret_pos = caret::resolve();
    let (x, y, source) = match caret_pos {
        Ok(p) => (p.x.max(0.0), p.y.max(0.0), "ax"),
        Err(_) => {
            // Top-left of screen so we can see the window in dev mode.
            (100.0, 100.0, "fallback")
        }
    };
    tracing::info!(x, y, source, "showing suggestion panel");
    show_panel(&app, &tap_state, &words, x, y).await;
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

#[cfg(target_os = "macos")]
async fn show_panel(
    app: &AppHandle,
    tap_state: &TapState,
    words: &[String],
    x: f64,
    y: f64,
) {
    let already_visible = tap_state.is_visible();
    if let Ok(w) = window::ensure_suggestions_window(app) {
        if !already_visible {
            if let Err(e) = window::show_at(&w, x, y) {
                tracing::warn!(error = %e, "show_at failed");
            }
        }
        let _ = app.emit(
            "suggestions:show",
            serde_json::json!({"words": words, "x": x, "y": y}),
        );
        tap_state.set_visible(true);
    }
}

#[cfg(target_os = "macos")]
async fn hide_panel(app: &AppHandle, tap_state: &TapState) {
    tap_state.set_visible(false);
    let _ = app.emit("suggestions:hide", serde_json::Value::Null);
    if let Some(w) = app.get_webview_window(window::SUGGESTIONS_LABEL) {
        let _ = window::hide(&w);
    }
}
