//! Resolve the on-screen position of the caret. Filled in during M4.

use crate::MacosError;

#[derive(Debug, Clone, Copy)]
pub struct CaretPosition {
    pub x: f64,
    pub y: f64,
}

pub fn resolve() -> Result<CaretPosition, MacosError> {
    // M4: AXUIElementCopyParameterizedAttributeValue + kAXBoundsForRangeParameterizedAttribute.
    Err(MacosError::NoFocusedElement)
}

pub fn bottom_right_fallback() -> CaretPosition {
    // M4 will replace this with real-screen-aware logic using NSScreen.
    CaretPosition { x: 1200.0, y: 700.0 }
}
