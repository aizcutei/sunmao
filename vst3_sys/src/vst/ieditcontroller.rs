//! IEditController and related interfaces

use std::ffi::c_void;
use crate::base::types::*;
use crate::vst::types::*;

// =============================================================================
// ParameterInfo
// =============================================================================

/// Parameter info
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ParameterInfo {
    pub id: ParamID,
    pub title: String128,
    pub short_title: String128,
    pub units: String128,
    pub step_count: int32,
    pub default_normalized_value: ParamValue,
    pub unit_id: UnitID,
    pub flags: int32,
}

/// Parameter flags
pub mod ParameterFlags {
    use crate::base::types::int32;
    pub const kNoFlags: int32 = 0;
    pub const kCanAutomate: int32 = 1 << 0;
    pub const kIsReadOnly: int32 = 1 << 1;
    pub const kIsWrapAround: int32 = 1 << 2;
    pub const kIsList: int32 = 1 << 3;
    pub const kIsHidden: int32 = 1 << 4;
    pub const kIsProgramChange: int32 = 1 << 15;
    pub const kIsBypass: int32 = 1 << 16;
}

// =============================================================================
// IEditController VTable
// =============================================================================

/// IEditController vtable
#[repr(C)]
pub struct IEditControllerVtbl {
    pub base: crate::base::IPluginBaseVtbl,
    pub set_component_state: unsafe extern "system" fn(this: *mut c_void, state: *mut c_void) -> tresult,
    pub set_state: unsafe extern "system" fn(this: *mut c_void, state: *mut c_void) -> tresult,
    pub get_state: unsafe extern "system" fn(this: *mut c_void, state: *mut c_void) -> tresult,
    pub get_parameter_count: unsafe extern "system" fn(this: *mut c_void) -> int32,
    pub get_parameter_info: unsafe extern "system" fn(this: *mut c_void, param_index: int32, info: *mut ParameterInfo) -> tresult,
    pub get_param_string_by_value: unsafe extern "system" fn(
        this: *mut c_void,
        id: ParamID,
        value_normalized: ParamValue,
        string: *mut String128,
    ) -> tresult,
    pub get_param_value_by_string: unsafe extern "system" fn(
        this: *mut c_void,
        id: ParamID,
        string: *const TChar,
        value_normalized: *mut ParamValue,
    ) -> tresult,
    pub normalized_param_to_plain: unsafe extern "system" fn(
        this: *mut c_void,
        id: ParamID,
        value_normalized: ParamValue,
    ) -> ParamValue,
    pub plain_param_to_normalized: unsafe extern "system" fn(
        this: *mut c_void,
        id: ParamID,
        plain_value: ParamValue,
    ) -> ParamValue,
    pub get_param_normalized: unsafe extern "system" fn(this: *mut c_void, id: ParamID) -> ParamValue,
    pub set_param_normalized: unsafe extern "system" fn(this: *mut c_void, id: ParamID, value: ParamValue) -> tresult,
    pub set_component_handler: unsafe extern "system" fn(this: *mut c_void, handler: *mut c_void) -> tresult,
    pub create_view: unsafe extern "system" fn(this: *mut c_void, name: FIDString) -> *mut c_void,
}

// =============================================================================
// IComponentHandler VTable
// =============================================================================

/// IComponentHandler vtable
#[repr(C)]
pub struct IComponentHandlerVtbl {
    pub unknown: IUnknownVtbl,
    pub begin_edit: unsafe extern "system" fn(this: *mut c_void, id: ParamID) -> tresult,
    pub perform_edit: unsafe extern "system" fn(this: *mut c_void, id: ParamID, value_normalized: ParamValue) -> tresult,
    pub end_edit: unsafe extern "system" fn(this: *mut c_void, id: ParamID) -> tresult,
    pub restart_component: unsafe extern "system" fn(this: *mut c_void, flags: int32) -> tresult,
}

/// Restart flags for IComponentHandler::restartComponent
pub mod RestartFlags {
    use crate::base::types::int32;
    pub const kReloadComponent: int32 = 1 << 0;
    pub const kIoChanged: int32 = 1 << 1;
    pub const kParamValuesChanged: int32 = 1 << 2;
    pub const kLatencyChanged: int32 = 1 << 3;
    pub const kParamTitlesChanged: int32 = 1 << 4;
    pub const kMidiCCAssignmentChanged: int32 = 1 << 5;
    pub const kNoteExpressionChanged: int32 = 1 << 6;
    pub const kIoTitlesChanged: int32 = 1 << 7;
    pub const kPrefetchableSupportChanged: int32 = 1 << 8;
    pub const kRoutingInfoChanged: int32 = 1 << 9;
}
