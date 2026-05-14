//! Accessibility API: read the focused element's text + caret position.
//!
//! All FFI is gated behind `unsafe`. On the happy path:
//!
//! 1. AXUIElementCreateSystemWide() -> system element
//! 2. Copy kAXFocusedUIElementAttribute -> focused element
//! 3. Inspect kAXSubrole -> bail out for AXSecureTextField (passwords)
//! 4. Copy kAXValueAttribute -> the full text
//! 5. Copy kAXSelectedTextRangeAttribute -> CFRange (caret + selection)
//! 6. Slice the last 256 chars before the caret.
//!
//! Anything missing or unsupported throws us back to the keylog fallback
//! in the caller.

use std::ffi::c_void;

use core_foundation::base::{CFRange, CFType, CFTypeRef, TCFType};
use core_foundation::number::CFNumberRef;
use core_foundation::string::{CFString, CFStringRef};

use crate::MacosError;

pub const CONTEXT_CHARS: usize = 256;

/// Opaque handle for an AXUIElement.
#[repr(C)]
struct __AXUIElement(c_void);
type AXUIElementRef = *const __AXUIElement;

const K_AX_ERROR_SUCCESS: i32 = 0;
const K_CF_NUMBER_SINT32_TYPE: i32 = 3;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXValueGetType(value: CFTypeRef) -> u32;
    fn AXValueGetValue(value: CFTypeRef, the_type: u32, ptr: *mut c_void) -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: CFTypeRef);
    fn CFGetTypeID(cf: CFTypeRef) -> u64;
    fn CFStringGetTypeID() -> u64;
    fn CFNumberGetTypeID() -> u64;
    fn CFNumberGetValue(num: CFNumberRef, the_type: i32, ptr: *mut c_void) -> bool;
}

const K_AXVALUE_CFRANGE_TYPE: u32 = 4;

#[derive(Debug, Clone)]
pub struct FocusedContext {
    pub text_before_caret: String,
    pub is_secure: bool,
}

/// Reads the focused UI element's text up to the caret, capped at 256 chars.
pub fn get_focused_text_context() -> Result<FocusedContext, MacosError> {
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return Err(MacosError::Ax("AXUIElementCreateSystemWide returned null".into()));
        }

        let focused = match copy_attr(system, "AXFocusedUIElement")? {
            Some(v) => v,
            None => {
                CFRelease(system as CFTypeRef);
                return Err(MacosError::NoFocusedElement);
            }
        };

        // We're done with `system` once we have the focused element.
        CFRelease(system as CFTypeRef);

        let is_secure = match copy_attr(focused as AXUIElementRef, "AXSubrole") {
            Ok(Some(subrole_cf)) => {
                let role = cfstring_to_rust(subrole_cf);
                CFRelease(subrole_cf);
                role.as_deref() == Some("AXSecureTextField")
            }
            _ => false,
        };

        if is_secure {
            CFRelease(focused);
            return Ok(FocusedContext {
                text_before_caret: String::new(),
                is_secure: true,
            });
        }

        let value_cf = match copy_attr(focused as AXUIElementRef, "AXValue")? {
            Some(v) => v,
            None => {
                CFRelease(focused);
                return Err(MacosError::Ax("focused element has no AXValue".into()));
            }
        };
        let text = cfstring_to_rust(value_cf).unwrap_or_default();
        CFRelease(value_cf);

        let caret_offset = match copy_attr(focused as AXUIElementRef, "AXSelectedTextRange")? {
            Some(range_cf) => {
                let offset = cfrange_offset(range_cf).unwrap_or(text.chars().count() as i64);
                CFRelease(range_cf);
                offset as usize
            }
            None => text.chars().count(),
        };

        CFRelease(focused);

        // Slice: take chars up to caret_offset, keep at most CONTEXT_CHARS at the tail.
        let chars: Vec<char> = text.chars().collect();
        let end = caret_offset.min(chars.len());
        let start = end.saturating_sub(CONTEXT_CHARS);
        let slice: String = chars[start..end].iter().collect();

        Ok(FocusedContext {
            text_before_caret: slice,
            is_secure: false,
        })
    }
}

unsafe fn copy_attr(elem: AXUIElementRef, name: &str) -> Result<Option<CFTypeRef>, MacosError> {
    let key = CFString::new(name);
    let mut out: CFTypeRef = std::ptr::null();
    let err = AXUIElementCopyAttributeValue(elem, key.as_concrete_TypeRef(), &mut out);
    if err == K_AX_ERROR_SUCCESS {
        Ok(if out.is_null() { None } else { Some(out) })
    } else if err == -25212 || err == -25204 {
        // kAXErrorNoValue / kAXErrorAttributeUnsupported - not an error,
        // just means the focused element doesn't carry this attribute.
        Ok(None)
    } else {
        Err(MacosError::Ax(format!("AX{} failed: {err}", name)))
    }
}

unsafe fn cfstring_to_rust(cf: CFTypeRef) -> Option<String> {
    if cf.is_null() {
        return None;
    }
    if CFGetTypeID(cf) != CFStringGetTypeID() {
        return None;
    }
    let s: CFString = CFString::wrap_under_get_rule(cf as CFStringRef);
    Some(s.to_string())
}

unsafe fn cfrange_offset(cf: CFTypeRef) -> Option<i64> {
    if cf.is_null() {
        return None;
    }
    if AXValueGetType(cf) != K_AXVALUE_CFRANGE_TYPE {
        // Not an AXValue/CFRange - probably a CFNumber for a single caret index.
        if CFGetTypeID(cf) == CFNumberGetTypeID() {
            let mut n: i32 = 0;
            if CFNumberGetValue(cf as CFNumberRef, K_CF_NUMBER_SINT32_TYPE, &mut n as *mut _ as *mut c_void) {
                return Some(n as i64);
            }
        }
        return None;
    }
    let mut range = CFRange { location: 0, length: 0 };
    if !AXValueGetValue(cf, K_AXVALUE_CFRANGE_TYPE, &mut range as *mut _ as *mut c_void) {
        return None;
    }
    Some(range.location as i64)
}

// Ensure CFType is linked so importing the type doesn't show as unused on
// platforms where we end up not using it. Cheap no-op.
#[allow(dead_code)]
fn _force_link(_: CFType) {}
