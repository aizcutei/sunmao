//! IEventList and Event structs

use crate::base::types::*;
use crate::vst::types::*;
use std::ffi::c_void;

// =============================================================================
// Event Types
// =============================================================================

/// Note-on event data
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct NoteOnEvent {
    pub channel: int16,
    pub pitch: int16,
    pub tuning: f32,
    pub velocity: f32,
    pub length: int32,
    pub note_id: int32,
}

/// Note-off event data
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct NoteOffEvent {
    pub channel: int16,
    pub pitch: int16,
    pub velocity: f32,
    pub note_id: int32,
    pub tuning: f32,
}

/// Data event (e.g., SysEx)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DataEvent {
    pub size: uint32,
    pub type_: uint32,
    pub bytes: *const uint8,
}

/// Poly pressure event
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct PolyPressureEvent {
    pub channel: int16,
    pub pitch: int16,
    pub pressure: f32,
    pub note_id: int32,
}

/// Note expression value event
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct NoteExpressionValueEvent {
    pub type_id: uint32,
    pub note_id: int32,
    pub value: f64,
}

/// Note expression text event
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NoteExpressionTextEvent {
    pub type_id: uint32,
    pub note_id: int32,
    pub text_len: uint32,
    pub text: *const TChar,
}

/// Chord event
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ChordEvent {
    pub root: int16,
    pub bass_note: int16,
    pub mask: int16,
    pub text_len: uint16,
    pub text: *const TChar,
}

/// Scale event  
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ScaleEvent {
    pub root: int16,
    pub mask: int16,
    pub text_len: uint16,
    pub text: *const TChar,
}

/// Legacy MIDI CC out event
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct LegacyMIDICCOutEvent {
    pub control_number: uint8,
    pub channel: int8,
    pub value: int8,
    pub value2: int8,
}

/// Event types
pub mod EventTypes {
    use crate::base::types::uint16;
    pub const kNoteOnEvent: uint16 = 0;
    pub const kNoteOffEvent: uint16 = 1;
    pub const kDataEvent: uint16 = 2;
    pub const kPolyPressureEvent: uint16 = 3;
    pub const kNoteExpressionValueEvent: uint16 = 4;
    pub const kNoteExpressionTextEvent: uint16 = 5;
    pub const kChordEvent: uint16 = 6;
    pub const kScaleEvent: uint16 = 7;
    pub const kLegacyMIDICCOutEvent: uint16 = 65535;
}

/// Event flags
pub mod EventFlags {
    use crate::base::types::uint16;
    pub const kIsLive: uint16 = 1 << 0;
    pub const kUserReserved1: uint16 = 1 << 14;
    pub const kUserReserved2: uint16 = 1 << 15;
}

/// Generic event structure
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Event {
    pub bus_index: int32,
    pub sample_offset: int32,
    pub ppq_position: TQuarterNotes,
    pub flags: uint16,
    pub type_: uint16,
    pub event: EventData,
}

/// Union of all event data types
#[repr(C)]
#[derive(Clone, Copy)]
pub union EventData {
    pub note_on: NoteOnEvent,
    pub note_off: NoteOffEvent,
    pub data: DataEvent,
    pub poly_pressure: PolyPressureEvent,
    pub note_expression_value: NoteExpressionValueEvent,
    pub note_expression_text: NoteExpressionTextEvent,
    pub chord: ChordEvent,
    pub scale: ScaleEvent,
    pub midi_cc_out: LegacyMIDICCOutEvent,
}

impl Default for Event {
    fn default() -> Self {
        Self {
            bus_index: 0,
            sample_offset: 0,
            ppq_position: 0.0,
            flags: 0,
            type_: 0,
            event: EventData {
                note_on: NoteOnEvent::default(),
            },
        }
    }
}

// =============================================================================
// IEventList VTable
// =============================================================================

/// IEventList vtable
#[repr(C)]
pub struct IEventListVtbl {
    pub unknown: IUnknownVtbl,
    pub get_event_count: unsafe extern "system" fn(this: *mut c_void) -> int32,
    pub get_event:
        unsafe extern "system" fn(this: *mut c_void, index: int32, e: *mut Event) -> tresult,
    pub add_event: unsafe extern "system" fn(this: *mut c_void, e: *mut Event) -> tresult,
}
