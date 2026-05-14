//! Global keystroke listener via CGEventTap. Runs a CFRunLoop on a
//! dedicated OS thread and forwards key events through a tokio channel.
//!
//! Listens to keyDown + flagsChanged at the session tap (post input
//! method, so we see the actual characters the user typed).

use std::ffi::c_void;
use std::os::raw::c_ulong;

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

/// Custom CGEvent integer-value-field we set on synthetic events so we can
/// ignore them on the way back in. Apple reserves the low field numbers;
/// this one is well outside the documented range.
pub const SYNTHETIC_MARK_FIELD: CGEventField = 0x0BADC0DE;

#[derive(Debug, Clone)]
pub enum KeyEvent {
    Char(char),
    Space,
    Backspace,
    Tab,
    Enter,
    Escape,
    Cmd1,
    Cmd2,
    Cmd3,
    ModifierOnly,
    /// Heuristic focus-change signal (currently Cmd+Tab / Cmd+`). M6 can
    /// upgrade to NSWorkspace activation notifications for richer tracking.
    FocusChanged,
}

/// Install the tap. Spawns a dedicated thread that owns the CFRunLoop.
/// The tap lives for the rest of the process; controlled shutdown is M6.
pub fn install(tx: tokio::sync::mpsc::UnboundedSender<KeyEvent>) -> Result<(), MacosError> {
    std::thread::Builder::new()
        .name("nextword-keytap".into())
        .spawn(move || run_loop(tx))
        .map_err(|_| MacosError::EventTap)?;
    Ok(())
}

fn run_loop(tx: tokio::sync::mpsc::UnboundedSender<KeyEvent>) {
    let callback = move |_proxy, _etype, event: &CGEvent| -> Option<CGEvent> {
        // Skip events we synthesised ourselves (set during M4 acceptance).
        if event.get_integer_value_field(SYNTHETIC_MARK_FIELD) != 0 {
            return None;
        }

        match event.get_type() {
            CGEventType::KeyDown => {
                if let Some(ev) = classify_key_down(event) {
                    let _ = tx.send(ev);
                }
            }
            CGEventType::FlagsChanged => {
                let flags = event.get_flags();
                if !flags.is_empty() {
                    let _ = tx.send(KeyEvent::ModifierOnly);
                }
            }
            _ => {}
        }
        None
    };

    let tap = match CGEventTap::new(
        CGEventTapLocation::Session,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::ListenOnly,
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
        CFRunLoop::run_current();
    }
}

fn classify_key_down(event: &CGEvent) -> Option<KeyEvent> {
    let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
    let flags = event.get_flags();
    let has_cmd = flags.contains(CGEventFlags::CGEventFlagCommand);
    let has_alt = flags.contains(CGEventFlags::CGEventFlagAlternate);
    let has_ctrl = flags.contains(CGEventFlags::CGEventFlagControl);

    // Cmd+Tab → focus-change heuristic.
    if has_cmd && keycode == KEYCODE_TAB {
        return Some(KeyEvent::FocusChanged);
    }

    // Cmd+1/2/3 → suggestion accept shortcuts. Plain digit goes via Char.
    if has_cmd && !has_alt && !has_ctrl {
        match keycode {
            KEYCODE_KEY_1 => return Some(KeyEvent::Cmd1),
            KEYCODE_KEY_2 => return Some(KeyEvent::Cmd2),
            KEYCODE_KEY_3 => return Some(KeyEvent::Cmd3),
            _ => return Some(KeyEvent::ModifierOnly),
        }
    }

    if has_ctrl || has_alt {
        return Some(KeyEvent::ModifierOnly);
    }

    match keycode {
        KEYCODE_SPACE => Some(KeyEvent::Space),
        KEYCODE_RETURN => Some(KeyEvent::Enter),
        KEYCODE_TAB => Some(KeyEvent::Tab),
        KEYCODE_DELETE => Some(KeyEvent::Backspace),
        KEYCODE_ESCAPE => Some(KeyEvent::Escape),
        _ => match unicode_from_event(event) {
            Some(c) if !c.is_control() => Some(KeyEvent::Char(c)),
            _ => None,
        },
    }
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
