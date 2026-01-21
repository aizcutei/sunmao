//! IParameterChanges and IParamValueQueue interfaces

use std::ffi::c_void;
use crate::base::types::*;
use crate::vst::types::*;

// =============================================================================
// IParamValueQueue VTable
// =============================================================================

/// IParamValueQueue vtable
#[repr(C)]
pub struct IParamValueQueueVtbl {
    pub unknown: IUnknownVtbl,
    pub get_parameter_id: unsafe extern "system" fn(this: *mut c_void) -> ParamID,
    pub get_point_count: unsafe extern "system" fn(this: *mut c_void) -> int32,
    pub get_point: unsafe extern "system" fn(
        this: *mut c_void,
        index: int32,
        sample_offset: *mut int32,
        value: *mut ParamValue,
    ) -> tresult,
    pub add_point: unsafe extern "system" fn(
        this: *mut c_void,
        sample_offset: int32,
        value: ParamValue,
        index: *mut int32,
    ) -> tresult,
}

// =============================================================================
// IParameterChanges VTable
// =============================================================================

/// IParameterChanges vtable
#[repr(C)]
pub struct IParameterChangesVtbl {
    pub unknown: IUnknownVtbl,
    pub get_parameter_count: unsafe extern "system" fn(this: *mut c_void) -> int32,
    pub get_parameter_data: unsafe extern "system" fn(this: *mut c_void, index: int32) -> *mut c_void,
    pub add_parameter_data: unsafe extern "system" fn(
        this: *mut c_void,
        id: *const ParamID,
        index: *mut int32,
    ) -> *mut c_void,
}
