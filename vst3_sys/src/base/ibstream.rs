//! IBStream interface for state persistence

use crate::base::types::*;
use std::ffi::c_void;

// =============================================================================
// Stream Seek Mode
// =============================================================================

pub mod StreamSeekMode {
    use crate::base::types::int32;
    pub const kIBSeekSet: int32 = 0;
    pub const kIBSeekCur: int32 = 1;
    pub const kIBSeekEnd: int32 = 2;
}

// =============================================================================
// IBStream VTable
// =============================================================================

/// IBStream vtable - binary stream for state read/write
#[repr(C)]
pub struct IBStreamVtbl {
    pub unknown: IUnknownVtbl,
    pub read: unsafe extern "system" fn(
        this: *mut c_void,
        buffer: *mut c_void,
        num_bytes: int32,
        num_bytes_read: *mut int32,
    ) -> tresult,
    pub write: unsafe extern "system" fn(
        this: *mut c_void,
        buffer: *mut c_void,
        num_bytes: int32,
        num_bytes_written: *mut int32,
    ) -> tresult,
    pub seek: unsafe extern "system" fn(
        this: *mut c_void,
        pos: int64,
        mode: int32,
        result: *mut int64,
    ) -> tresult,
    pub tell: unsafe extern "system" fn(this: *mut c_void, pos: *mut int64) -> tresult,
}
