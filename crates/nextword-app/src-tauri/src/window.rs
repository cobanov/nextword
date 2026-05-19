//! Floating suggestion-window manager.
//!
//! The window is created via Tauri (so it gets the webview pipeline for free)
//! then converted into a non-activating, floating panel by flipping the
//! underlying NSWindow's style mask and collection behaviour. Plain Tauri
//! transparent + always_on_top isn't enough: pressing keys in another app
//! must NOT steal focus from that app into NextWord.

use anyhow::Result;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub const SUGGESTIONS_LABEL: &str = "suggestions";
pub const SUGGESTIONS_WIDTH: f64 = 320.0;
pub const SUGGESTIONS_HEIGHT: f64 = 56.0;

pub fn ensure_suggestions_window(app: &AppHandle) -> Result<WebviewWindow> {
    if let Some(w) = app.get_webview_window(SUGGESTIONS_LABEL) {
        return Ok(w);
    }
    let w = WebviewWindowBuilder::new(
        app,
        SUGGESTIONS_LABEL,
        WebviewUrl::App("suggestions.html".into()),
    )
    .inner_size(SUGGESTIONS_WIDTH, SUGGESTIONS_HEIGHT)
    .resizable(false)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .visible(false)
    .build()?;

    Ok(w)
}

pub fn show_at(win: &WebviewWindow, x: f64, y: f64) -> Result<()> {
    win.set_position(tauri::PhysicalPosition::new(x as i32, y as i32))?;
    win.show()?;
    Ok(())
}

pub fn hide(win: &WebviewWindow) -> Result<()> {
    win.hide()?;
    Ok(())
}
