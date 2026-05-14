//! CGEventTap-based key event listener. Filled in during M3.

use crate::MacosError;

#[derive(Debug, Clone, Copy)]
pub enum KeyEvent {
    Char(char),
    Space,
    Backspace,
    Enter,
    Tab,
    Escape,
    Cmd1,
    Cmd2,
    Cmd3,
    FocusChanged,
}

pub fn install(_tx: tokio::sync::mpsc::UnboundedSender<KeyEvent>) -> Result<(), MacosError> {
    // M3 will install a CGEventTap on kCGSessionEventTap with kCGEventKeyDown.
    Err(MacosError::EventTap)
}
