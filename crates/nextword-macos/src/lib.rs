//! macOS-specific bits: Accessibility text reads, CGEventTap keylog,
//! caret position resolution, AX permission check.
//!
//! Everything in here is gated by cfg(target_os = "macos"). On other
//! platforms the crate compiles to a no-op shell so the workspace builds.

#[cfg(target_os = "macos")]
pub mod ax;
#[cfg(target_os = "macos")]
pub mod caret;
#[cfg(target_os = "macos")]
pub mod keytap;
#[cfg(target_os = "macos")]
pub mod permissions;

#[cfg(not(target_os = "macos"))]
pub mod stub {
    //! Placeholder so non-mac builds in CI don't break. Remove when
    //! Windows lands in its own crate.
}

#[derive(Debug, thiserror::Error)]
pub enum MacosError {
    #[error("accessibility permission not granted")]
    AxNotTrusted,

    #[error("no focused ui element")]
    NoFocusedElement,

    #[error("AX call failed: {0}")]
    Ax(String),

    #[error("event tap creation failed")]
    EventTap,
}
