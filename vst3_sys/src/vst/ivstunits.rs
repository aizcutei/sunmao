//! `IUnitInfo` — VST3's parameter/program grouping interface.
//!
//! Values and layout transcribed from the upstream `vst/ivstunits.h` in
//! Steinberg's `vst3_pluginterfaces`. A host uses this to discover the unit
//! tree that `ParameterInfo::unit_id` refers to; without it, every parameter
//! appears flat regardless of the unit id it claims.

use crate::base::IUnknownVtbl;
use crate::base::types::{int16, int32, tresult};
use crate::vst::types::{BusDirection, MediaType, ProgramListID, UnitID};

/// VST3's fixed-size UTF-16 name field.
type String128 = [u16; 128];
use std::ffi::c_void;
use std::os::raw::c_char;

/// Identifier of the top-level unit.
pub const kRootUnitId: UnitID = 0;
/// Parent of the root unit, which has none.
pub const kNoParentUnitId: UnitID = -1;
/// The unit uses no program list.
pub const kNoProgramListId: ProgramListID = -1;
/// All program information is invalid.
pub const kAllProgramInvalid: int32 = -1;

/// One node of the unit tree.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct UnitInfo {
    pub id: UnitID,
    pub parent_unit_id: UnitID,
    pub name: String128,
    pub program_list_id: ProgramListID,
}

impl Default for UnitInfo {
    fn default() -> Self {
        Self {
            id: kRootUnitId,
            parent_unit_id: kNoParentUnitId,
            name: [0; 128],
            program_list_id: kNoProgramListId,
        }
    }
}

/// One program list a unit may reference.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct ProgramListInfo {
    pub id: ProgramListID,
    pub name: String128,
    pub program_count: int32,
}

impl Default for ProgramListInfo {
    fn default() -> Self {
        Self {
            id: kNoProgramListId,
            name: [0; 128],
            program_count: 0,
        }
    }
}

/// Method order matches the upstream header exactly; the ABI depends on it.
#[repr(C)]
pub struct IUnitInfoVtbl {
    pub unknown: IUnknownVtbl,
    pub get_unit_count: unsafe extern "system" fn(this: *mut c_void) -> int32,
    pub get_unit_info: unsafe extern "system" fn(
        this: *mut c_void,
        unit_index: int32,
        info: *mut UnitInfo,
    ) -> tresult,
    pub get_program_list_count: unsafe extern "system" fn(this: *mut c_void) -> int32,
    pub get_program_list_info: unsafe extern "system" fn(
        this: *mut c_void,
        list_index: int32,
        info: *mut ProgramListInfo,
    ) -> tresult,
    pub get_program_name: unsafe extern "system" fn(
        this: *mut c_void,
        list_id: ProgramListID,
        program_index: int32,
        name: *mut u16,
    ) -> tresult,
    pub get_program_info: unsafe extern "system" fn(
        this: *mut c_void,
        list_id: ProgramListID,
        program_index: int32,
        attribute_id: *const c_char,
        attribute_value: *mut u16,
    ) -> tresult,
    pub has_program_pitch_names: unsafe extern "system" fn(
        this: *mut c_void,
        list_id: ProgramListID,
        program_index: int32,
    ) -> tresult,
    pub get_program_pitch_name: unsafe extern "system" fn(
        this: *mut c_void,
        list_id: ProgramListID,
        program_index: int32,
        midi_pitch: int16,
        name: *mut u16,
    ) -> tresult,
    pub get_selected_unit: unsafe extern "system" fn(this: *mut c_void) -> UnitID,
    pub select_unit: unsafe extern "system" fn(this: *mut c_void, unit_id: UnitID) -> tresult,
    pub get_unit_by_bus: unsafe extern "system" fn(
        this: *mut c_void,
        media_type: MediaType,
        dir: BusDirection,
        bus_index: int32,
        channel: int32,
        unit_id: *mut UnitID,
    ) -> tresult,
    pub set_unit_program_data: unsafe extern "system" fn(
        this: *mut c_void,
        list_or_unit_id: int32,
        program_index: int32,
        data: *mut c_void,
    ) -> tresult,
}
