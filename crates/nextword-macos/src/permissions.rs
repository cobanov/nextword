//! Accessibility permission check + prompt. Real impl in M3.

use crate::MacosError;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrustedWithOptions(options: *const std::ffi::c_void) -> bool;
}

pub fn is_trusted() -> bool {
    unsafe { AXIsProcessTrustedWithOptions(std::ptr::null()) }
}

pub fn require_trusted() -> Result<(), MacosError> {
    if is_trusted() {
        Ok(())
    } else {
        Err(MacosError::AxNotTrusted)
    }
}

pub fn open_settings_pane() {
    // x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn();
}
