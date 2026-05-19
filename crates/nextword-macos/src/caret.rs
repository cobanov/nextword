//! Resolve the on-screen position of the caret in the focused UI element.
//!
//! Uses AXUIElementCopyParameterizedAttributeValue with
//! kAXBoundsForRangeParameterizedAttribute. The result is a CGRect in
//! screen coordinates (Cocoa coordinate space, origin bottom-left).
//!
//! We expose a Tauri-friendly variant that returns top-left physical
//! pixels so the floating window can be placed without conversion.

use std::ffi::c_void;

use core_foundation::base::{CFRange, CFType, CFTypeRef, TCFType};
use core_foundation::number::{CFNumber, CFNumberRef};
use core_foundation::string::{CFString, CFStringRef};

use crate::MacosError;

#[repr(C)]
struct __AXUIElement(c_void);
type AXUIElementRef = *const __AXUIElement;
type AXValueRef = CFTypeRef;

const K_AX_ERROR_SUCCESS: i32 = 0;
const K_AXVALUE_CGRECT_TYPE: u32 = 1;
const K_AXVALUE_CFRANGE_TYPE: u32 = 4;
const K_CF_NUMBER_SINT32_TYPE: i32 = 3;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct CGPoint {
    x: f64,
    y: f64,
}
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct CGSize {
    width: f64,
    height: f64,
}
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementCopyParameterizedAttributeValue(
        element: AXUIElementRef,
        parameter: CFStringRef,
        param_value: CFTypeRef,
        out: *mut CFTypeRef,
    ) -> i32;
    fn AXValueCreate(value_type: u32, value_ptr: *const c_void) -> AXValueRef;
    fn AXValueGetValue(value: CFTypeRef, the_type: u32, ptr: *mut c_void) -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: CFTypeRef);
    fn CFGetTypeID(cf: CFTypeRef) -> u64;
    fn CFNumberGetTypeID() -> u64;
    fn CFNumberGetValue(num: CFNumberRef, the_type: i32, ptr: *mut c_void) -> bool;
}

#[derive(Debug, Clone, Copy)]
pub struct CaretPosition {
    /// Screen-space top-left in physical pixels (Tauri coordinates).
    pub x: f64,
    pub y: f64,
    /// Height of the caret line in points.
    pub line_height: f64,
}

/// Best-effort caret resolution. Returns Err when AX doesn't support it
/// on the focused element (most non-Cocoa apps: Chrome, Slack, VSCode).
pub fn resolve() -> Result<CaretPosition, MacosError> {
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
        CFRelease(system as CFTypeRef);

        let range_cf = match copy_attr(focused as AXUIElementRef, "AXSelectedTextRange")? {
            Some(v) => v,
            None => {
                CFRelease(focused);
                return Err(MacosError::Ax("no AXSelectedTextRange".into()));
            }
        };

        // Bounds-for-range wants a CFRange. The selected range is already that.
        let mut bounds_cf: CFTypeRef = std::ptr::null();
        let key = CFString::new("AXBoundsForRange");
        let err = AXUIElementCopyParameterizedAttributeValue(
            focused as AXUIElementRef,
            key.as_concrete_TypeRef(),
            range_cf,
            &mut bounds_cf,
        );
        CFRelease(range_cf);
        CFRelease(focused);

        if err != K_AX_ERROR_SUCCESS || bounds_cf.is_null() {
            return Err(MacosError::Ax(format!("AXBoundsForRange failed: {err}")));
        }

        let mut rect = CGRect::default();
        let ok = AXValueGetValue(bounds_cf, K_AXVALUE_CGRECT_TYPE, &mut rect as *mut _ as *mut c_void);
        CFRelease(bounds_cf);
        if !ok {
            return Err(MacosError::Ax("AXBoundsForRange not a CGRect".into()));
        }

        // AX returns Cocoa coords (origin top-left for AX bounds, despite docs).
        // For our floating window we want the position just BELOW the caret
        // line, anchored to its left edge.
        Ok(CaretPosition {
            x: rect.origin.x,
            y: rect.origin.y + rect.size.height + 4.0,
            line_height: rect.size.height,
        })
    }
}

/// Fallback when AX can't tell us where the caret is: bottom-right of the
/// primary display, 40 px from the edges, 320x56 panel-sized.
pub fn bottom_right_fallback() -> CaretPosition {
    // Hard-coded for now; M6 will read NSScreen.mainScreen().frame and pick
    // the active screen. This is fine for v1 on a single-display setup.
    CaretPosition { x: 1200.0, y: 700.0, line_height: 18.0 }
}

unsafe fn copy_attr(elem: AXUIElementRef, name: &str) -> Result<Option<CFTypeRef>, MacosError> {
    let key = CFString::new(name);
    let mut out: CFTypeRef = std::ptr::null();
    let err = AXUIElementCopyAttributeValue(elem, key.as_concrete_TypeRef(), &mut out);
    if err == K_AX_ERROR_SUCCESS {
        Ok(if out.is_null() { None } else { Some(out) })
    } else if err == -25212 || err == -25204 {
        Ok(None)
    } else {
        Err(MacosError::Ax(format!("AX{} failed: {err}", name)))
    }
}

// Silence dead-code warnings while we keep CFRange/CFNumber types in scope
// for the M4 follow-up that decodes more attributes.
#[allow(dead_code)]
fn _force_link(_a: CFRange, _b: &CFNumber, _c: CFType) {}

#[allow(dead_code)]
unsafe fn _unused() {
    let _ = K_CF_NUMBER_SINT32_TYPE;
    let _ = K_AXVALUE_CFRANGE_TYPE;
    let _ = AXValueCreate as unsafe extern "C" fn(u32, *const c_void) -> AXValueRef;
    let _ = CFNumberGetTypeID as unsafe extern "C" fn() -> u64;
    let _ = CFNumberGetValue as unsafe extern "C" fn(CFNumberRef, i32, *mut c_void) -> bool;
    let _ = CFGetTypeID as unsafe extern "C" fn(CFTypeRef) -> u64;
}
