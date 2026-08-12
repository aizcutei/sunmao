//! VST messaging and processor/controller connection interfaces.

use crate::base::{IUnknownVtbl, TUID, tresult};
use std::ffi::c_void;

/// IConnectionPoint vtable used to connect a component with its edit controller.
#[repr(C)]
pub struct IConnectionPointVtbl {
    pub unknown: IUnknownVtbl,
    pub connect: unsafe extern "system" fn(this: *mut c_void, other: *mut c_void) -> tresult,
    pub disconnect: unsafe extern "system" fn(this: *mut c_void, other: *mut c_void) -> tresult,
    pub notify: unsafe extern "system" fn(this: *mut c_void, message: *mut c_void) -> tresult,
}

/// Opaque IConnectionPoint interface pointer.
#[repr(C)]
pub struct IConnectionPoint {
    pub vtbl: *const IConnectionPointVtbl,
}

/// IID used by hosts to query IConnectionPoint.
pub const IID_ICONNECTION_POINT: TUID = crate::uid!(0x70A4156F, 0x6E6E4026, 0x989148BF, 0xAA60D8D1);
