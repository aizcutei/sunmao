//! VST3 host application interface passed to `IPluginBase::initialize`.

use crate::base::{IUnknownVtbl, String128, TUID, tresult};
use std::ffi::c_void;

/// Basic host context exposed to VST3 components and edit controllers.
#[repr(C)]
pub struct IHostApplicationVtbl {
    pub unknown: IUnknownVtbl,
    pub get_name: unsafe extern "system" fn(this: *mut c_void, name: *mut String128) -> tresult,
    pub create_instance: unsafe extern "system" fn(
        this: *mut c_void,
        cid: *const TUID,
        iid: *const TUID,
        object: *mut *mut c_void,
    ) -> tresult,
}
