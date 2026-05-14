//! Accessibility API reads: focused element, text value, caret range.
//! Filled in during M3.

use crate::MacosError;

#[derive(Debug, Clone)]
pub struct FocusedContext {
    pub text_before_caret: String,
    pub is_secure: bool,
}

/// Returns the 256 chars before the caret in the focused UI element.
/// Stub for M0. Real implementation in M3.
pub fn get_focused_text_context() -> Result<FocusedContext, MacosError> {
    Err(MacosError::NoFocusedElement)
}
