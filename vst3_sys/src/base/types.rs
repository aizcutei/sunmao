//! Base types, result codes, and constants

use std::ffi::c_void;
use std::os::raw::c_char;

// =============================================================================
// Basic Types
// =============================================================================

pub type int8 = i8;
pub type int16 = i16;
pub type int32 = i32;
pub type int64 = i64;
pub type uint8 = u8;
pub type uint16 = u16;
pub type uint32 = u32;
pub type uint64 = u64;

pub type char8 = c_char;
pub type char16 = u16;
pub type tchar = char16;

pub type tresult = int32;
pub type TBool = u8;
pub type FIDString = *const char8;

/// 16-byte Globally Unique Identifier
pub type TUID = [int8; 16];

/// 128 character UTF-16 string
pub type String128 = [char16; 128];

// =============================================================================
// Result Codes
// =============================================================================

#[cfg(not(target_os = "windows"))]
pub const kResultOk: tresult = 0;
#[cfg(not(target_os = "windows"))]
pub const kResultTrue: tresult = kResultOk;
#[cfg(not(target_os = "windows"))]
pub const kResultFalse: tresult = 1;
#[cfg(not(target_os = "windows"))]
pub const kNoInterface: tresult = -1;
#[cfg(not(target_os = "windows"))]
pub const kInvalidArgument: tresult = 2;
#[cfg(not(target_os = "windows"))]
pub const kNotImplemented: tresult = 3;
#[cfg(not(target_os = "windows"))]
pub const kInternalError: tresult = 4;
#[cfg(not(target_os = "windows"))]
pub const kNotInitialized: tresult = 5;
#[cfg(not(target_os = "windows"))]
pub const kOutOfMemory: tresult = 6;

#[cfg(target_os = "windows")]
pub const kResultOk: tresult = 0;
#[cfg(target_os = "windows")]
pub const kResultTrue: tresult = kResultOk;
#[cfg(target_os = "windows")]
pub const kResultFalse: tresult = 1;
#[cfg(target_os = "windows")]
pub const kNoInterface: tresult = -2_147_467_262;
#[cfg(target_os = "windows")]
pub const kInvalidArgument: tresult = -2_147_024_809;
#[cfg(target_os = "windows")]
pub const kNotImplemented: tresult = -2_147_467_263;
#[cfg(target_os = "windows")]
pub const kInternalError: tresult = -2_147_467_259;
#[cfg(target_os = "windows")]
pub const kNotInitialized: tresult = -2_147_418_113;
#[cfg(target_os = "windows")]
pub const kOutOfMemory: tresult = -2_147_024_882;

// =============================================================================
// Helper Functions
// =============================================================================

/// Compare two TUIDs for equality
pub fn iid_equal(a: &TUID, b: &TUID) -> bool {
    a == b
}

/// Copy a C string into a fixed-size buffer
pub fn strcpy_safe<const N: usize>(dst: &mut [char8; N], src: &[u8]) {
    let len = src.len().min(N - 1);
    for (i, &byte) in src.iter().take(len).enumerate() {
        dst[i] = byte as char8;
    }
    if len < N {
        dst[len] = 0;
    }
}

/// Copy a UTF-16 string into a char16 array of any size
pub fn str16cpy<const N: usize>(dst: &mut [char16; N], src: &str) {
    let mut i = 0;
    for c in src.encode_utf16() {
        if i >= N - 1 {
            break;
        }
        dst[i] = c;
        i += 1;
    }
    if i < N {
        dst[i] = 0;
    }
}

/// Copy a UTF-16 string into a String128 - alias for str16cpy
pub fn str16cpy_safe(dst: &mut String128, src: &str) {
    str16cpy(dst, src);
}

// =============================================================================
// TUID Macro
// =============================================================================

/// Creates a TUID from 4 u32 values (non-COM byte order for macOS/Linux)
#[macro_export]
macro_rules! uid {
    ($l1:expr, $l2:expr, $l3:expr, $l4:expr) => {{
        const fn make_uid(l1: u32, l2: u32, l3: u32, l4: u32) -> [i8; 16] {
            [
                ((l1 >> 24) & 0xFF) as i8,
                ((l1 >> 16) & 0xFF) as i8,
                ((l1 >> 8) & 0xFF) as i8,
                (l1 & 0xFF) as i8,
                ((l2 >> 24) & 0xFF) as i8,
                ((l2 >> 16) & 0xFF) as i8,
                ((l2 >> 8) & 0xFF) as i8,
                (l2 & 0xFF) as i8,
                ((l3 >> 24) & 0xFF) as i8,
                ((l3 >> 16) & 0xFF) as i8,
                ((l3 >> 8) & 0xFF) as i8,
                (l3 & 0xFF) as i8,
                ((l4 >> 24) & 0xFF) as i8,
                ((l4 >> 16) & 0xFF) as i8,
                ((l4 >> 8) & 0xFF) as i8,
                (l4 & 0xFF) as i8,
            ]
        }
        make_uid($l1 as u32, $l2 as u32, $l3 as u32, $l4 as u32)
    }};
}

// =============================================================================
// FUnknown VTable
// =============================================================================

/// FUnknown vtable - the base interface for all COM objects
#[repr(C)]
pub struct IUnknownVtbl {
    pub query_interface:
        unsafe extern "system" fn(this: *mut c_void, iid: *const TUID, obj: *mut *mut c_void) -> tresult,
    pub add_ref: unsafe extern "system" fn(this: *mut c_void) -> uint32,
    pub release: unsafe extern "system" fn(this: *mut c_void) -> uint32,
}
