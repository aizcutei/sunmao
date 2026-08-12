//! IComponent interface

use crate::base::types::*;
use crate::vst::types::*;
use std::ffi::c_void;

// =============================================================================
// Structs
// =============================================================================

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BusInfo {
    pub media_type: MediaType,
    pub direction: BusDirection,
    pub channel_count: int32,
    pub name: String128,
    pub bus_type: BusType,
    pub flags: uint32,
}

impl Default for BusInfo {
    fn default() -> Self {
        Self {
            media_type: 0,
            direction: 0,
            channel_count: 0,
            name: [0; 128],
            bus_type: 0,
            flags: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RoutingInfo {
    pub media_type: MediaType,
    pub bus_index: int32,
    pub channel: int32,
}

// =============================================================================
// IComponent VTable
// =============================================================================

/// IComponent vtable (extends IPluginBase)
#[repr(C)]
pub struct IComponentVtbl {
    pub base: crate::base::IPluginBaseVtbl,
    pub get_controller_class_id:
        unsafe extern "system" fn(this: *mut c_void, class_id: *mut TUID) -> tresult,
    pub set_io_mode: unsafe extern "system" fn(this: *mut c_void, mode: IoMode) -> tresult,
    pub get_bus_count: unsafe extern "system" fn(
        this: *mut c_void,
        media_type: MediaType,
        dir: BusDirection,
    ) -> int32,
    pub get_bus_info: unsafe extern "system" fn(
        this: *mut c_void,
        media_type: MediaType,
        dir: BusDirection,
        index: int32,
        bus: *mut BusInfo,
    ) -> tresult,
    pub get_routing_info: unsafe extern "system" fn(
        this: *mut c_void,
        in_info: *mut RoutingInfo,
        out_info: *mut RoutingInfo,
    ) -> tresult,
    pub activate_bus: unsafe extern "system" fn(
        this: *mut c_void,
        media_type: MediaType,
        dir: BusDirection,
        index: int32,
        state: TBool,
    ) -> tresult,
    pub set_active: unsafe extern "system" fn(this: *mut c_void, state: TBool) -> tresult,
    pub set_state: unsafe extern "system" fn(this: *mut c_void, state: *mut c_void) -> tresult,
    pub get_state: unsafe extern "system" fn(this: *mut c_void, state: *mut c_void) -> tresult,
}
