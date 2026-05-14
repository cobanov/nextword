//! Accessibility permission check + prompt. Wraps `AXIsProcessTrustedWithOptions`.

use std::ffi::c_void;
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;

use crate::MacosError;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    static kAXTrustedCheckOptionPrompt: core_foundation::string::CFStringRef;
}

/// Returns true iff our process holds Accessibility trust.
pub fn is_trusted() -> bool {
    unsafe { AXIsProcessTrustedWithOptions(std::ptr::null()) }
}

/// Same check but pops the system-modal "This app wants Accessibility
/// access" dialog if we don't have it yet. Safe to call repeatedly.
pub fn prompt_if_needed() -> bool {
    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let value = CFBoolean::true_value();
        let dict = CFDictionary::from_CFType_pairs(&[(key, value)]);
        AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef() as *const c_void)
    }
}

pub fn require_trusted() -> Result<(), MacosError> {
    if is_trusted() {
        Ok(())
    } else {
        Err(MacosError::AxNotTrusted)
    }
}

/// Opens System Settings to the Privacy → Accessibility pane.
pub fn open_settings_pane() {
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn();
}
