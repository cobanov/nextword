//! Global keystroke listener via CGEventTap. Runs a CFRunLoop on a
//! dedicated OS thread and forwards key events through a tokio channel.
//!
//! When `state.suggestions_visible` is true, the Tab / Cmd+1-3 / Esc keys
//! are intercepted (returned as None) so they don't reach the focused app;
//! every other keystroke hides the suggestions and passes through.

use std::ffi::c_void;
use std::os::raw::c_ulong;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::event::{
    CGEvent, CGEventField, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventType, EventField,
};
use foreign_types_shared::ForeignType;

use crate::MacosError;

const KEYCODE_SPACE: i64 = 49;
const KEYCODE_RETURN: i64 = 36;
const KEYCODE_TAB: i64 = 48;
const KEYCODE_DELETE: i64 = 51;
const KEYCODE_ESCAPE: i64 = 53;
const KEYCODE_KEY_1: i64 = 18;
const KEYCODE_KEY_2: i64 = 19;
const KEYCODE_KEY_3: i64 = 20;

/// Custom CGEvent integer-value-field number we set on synthetic events so
/// the tap can ignore them on the way back in. Apple reserves the low field
/// numbers; this one is well outside the documented range.
pub const SYNTHETIC_MARK_FIELD: CGEventField = 0x0BADC0DE;

#[derive(Debug, Clone)]
pub enum KeyEvent {
    Char(char),
    Space,
    Backspace,
    Tab,
    Enter,
    Escape,
    /// Suggestion accept (0-based index, 0..=2). Fired only when the panel
    /// is visible and the user pressed Tab or Cmd+1/2/3.
    Accept(u8),
    /// Suggestion dismiss. Fired on Esc when the panel is visible.
    DismissRequest,
    Cmd1,
    Cmd2,
    Cmd3,
    ModifierOnly,
    FocusChanged,
}

/// Shared flag the tap consults to decide whether to swallow Tab / Esc /
/// Cmd+1-3. Flip it in lockstep with show()/hide() of the suggestion panel.
#[derive(Default)]
pub struct TapState {
    pub suggestions_visible: AtomicBool,
}

impl TapState {
    pub fn set_visible(&self, v: bool) {
        self.suggestions_visible.store(v, Ordering::Release);
    }
    pub fn is_visible(&self) -> bool {
        self.suggestions_visible.load(Ordering::Acquire)
    }
}

pub fn install(
    tx: tokio::sync::mpsc::UnboundedSender<KeyEvent>,
    state: Arc<TapState>,
) -> Result<(), MacosError> {
    std::thread::Builder::new()
        .name("nextword-keytap".into())
        .spawn(move || run_loop(tx, state))
        .map_err(|_| MacosError::EventTap)?;
    Ok(())
}

fn run_loop(tx: tokio::sync::mpsc::UnboundedSender<KeyEvent>, state: Arc<TapState>) {
    let callback = move |_proxy, _etype, event: &CGEvent| -> Option<CGEvent> {
        // Events we synthesised ourselves: skip our processing, but pass
        // them through to the OS so they reach the user's target app.
        if event.get_integer_value_field(SYNTHETIC_MARK_FIELD) != 0 {
            return Some(event.clone());
        }

        match event.get_type() {
            CGEventType::KeyDown => handle_key_down(&tx, &state, event),
            CGEventType::FlagsChanged => {
                let flags = event.get_flags();
                if !flags.is_empty() {
                    let _ = tx.send(KeyEvent::ModifierOnly);
                }
                Some(event.clone())
            }
            CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
                tracing::warn!(?_etype, "event tap disabled; re-enabling");
                Some(event.clone())
            }
            _ => Some(event.clone()),
        }
    };

    let tap = match CGEventTap::new(
        CGEventTapLocation::Session,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        vec![CGEventType::KeyDown, CGEventType::FlagsChanged],
        callback,
    ) {
        Ok(t) => t,
        Err(_) => {
            tracing::error!("CGEventTap creation failed - missing Accessibility permission?");
            return;
        }
    };

    unsafe {
        let current = CFRunLoop::get_current();
        let source = match tap.mach_port.create_runloop_source(0) {
            Ok(s) => s,
            Err(_) => {
                tracing::error!("create_runloop_source failed");
                return;
            }
        };
        current.add_source(&source, kCFRunLoopCommonModes);
        tap.enable();
        tracing::info!("CGEventTap installed");
        CFRunLoop::run_current();
    }
}

/// Return value is forwarded to the OS: None swallows the event, Some
/// lets it through. `event.clone()` would produce a fresh ref but tauri's
/// expectations are met by returning None for the swallow path.
fn handle_key_down(
    tx: &tokio::sync::mpsc::UnboundedSender<KeyEvent>,
    state: &TapState,
    event: &CGEvent,
) -> Option<CGEvent> {
    let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
    let flags = event.get_flags();
    let has_cmd = flags.contains(CGEventFlags::CGEventFlagCommand);
    let has_alt = flags.contains(CGEventFlags::CGEventFlagAlternate);
    let has_ctrl = flags.contains(CGEventFlags::CGEventFlagControl);

    let panel_visible = state.is_visible();

    // ----- Acceptance / dismiss while panel is up. Swallow only the
    // acceptance and dismiss keys; every other keystroke passes through
    // and the panel keeps showing live suggestions. -----
    if panel_visible {
        if !has_cmd && !has_alt && !has_ctrl {
            match keycode {
                KEYCODE_TAB => {
                    let _ = tx.send(KeyEvent::Accept(0));
                    return None;
                }
                KEYCODE_ESCAPE => {
                    let _ = tx.send(KeyEvent::DismissRequest);
                    return None;
                }
                _ => {}
            }
        }
        if has_cmd && !has_alt && !has_ctrl {
            let slot = match keycode {
                KEYCODE_KEY_1 => Some(0u8),
                KEYCODE_KEY_2 => Some(1u8),
                KEYCODE_KEY_3 => Some(2u8),
                _ => None,
            };
            if let Some(n) = slot {
                let _ = tx.send(KeyEvent::Accept(n));
                return None;
            }
        }
    }

    // ----- Regular classification (panel not visible, OR fallthrough above) -----
    if has_cmd && keycode == KEYCODE_TAB {
        let _ = tx.send(KeyEvent::FocusChanged);
        return Some(event.clone());
    }
    if has_cmd && !has_alt && !has_ctrl {
        match keycode {
            KEYCODE_KEY_1 => {
                let _ = tx.send(KeyEvent::Cmd1);
            }
            KEYCODE_KEY_2 => {
                let _ = tx.send(KeyEvent::Cmd2);
            }
            KEYCODE_KEY_3 => {
                let _ = tx.send(KeyEvent::Cmd3);
            }
            _ => {
                let _ = tx.send(KeyEvent::ModifierOnly);
            }
        }
        return Some(event.clone());
    }
    if has_ctrl || has_alt {
        let _ = tx.send(KeyEvent::ModifierOnly);
        return Some(event.clone());
    }

    let ev = match keycode {
        KEYCODE_SPACE => Some(KeyEvent::Space),
        KEYCODE_RETURN => Some(KeyEvent::Enter),
        KEYCODE_TAB => Some(KeyEvent::Tab),
        KEYCODE_DELETE => Some(KeyEvent::Backspace),
        KEYCODE_ESCAPE => Some(KeyEvent::Escape),
        _ => match unicode_from_event(event) {
            Some(c) if !c.is_control() => Some(KeyEvent::Char(c)),
            _ => None,
        },
    };
    if let Some(ev) = ev {
        let _ = tx.send(ev);
    }
    Some(event.clone())
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn CGEventKeyboardGetUnicodeString(
        event: *mut c_void,
        max_string_length: c_ulong,
        actual_string_length: *mut c_ulong,
        unicode_string: *mut u16,
    );
}

fn unicode_from_event(event: &CGEvent) -> Option<char> {
    let mut buf = [0u16; 4];
    let mut actual: c_ulong = 0;
    unsafe {
        CGEventKeyboardGetUnicodeString(
            event.as_ptr() as *mut c_void,
            buf.len() as c_ulong,
            &mut actual,
            buf.as_mut_ptr(),
        );
    }
    if actual == 0 {
        return None;
    }
    let slice = &buf[..actual as usize];
    let s = String::from_utf16_lossy(slice);
    s.chars().next()
}
