//! Floating suggestion-window manager. M0 stub. M4 fills in non-activating
//! panel style mask + caret-anchored positioning.

use anyhow::Result;
use tauri::{AppHandle, WebviewWindow, WebviewWindowBuilder, WebviewUrl, Manager};

pub const SUGGESTIONS_LABEL: &str = "suggestions";

pub fn ensure_suggestions_window(app: &AppHandle) -> Result<WebviewWindow> {
    if let Some(w) = app.get_webview_window(SUGGESTIONS_LABEL) {
        return Ok(w);
    }
    let w = WebviewWindowBuilder::new(
        app,
        SUGGESTIONS_LABEL,
        WebviewUrl::App("suggestions.html".into()),
    )
    .inner_size(320.0, 56.0)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .visible(false)
    .build()?;
    Ok(w)
}
