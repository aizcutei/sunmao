use crate::id::clap_id;
use std::ffi::c_void;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_event_header_t {
    pub size: u32,
    pub time: u32,
    pub space_id: u16,
    pub type_: u16,
    pub flags: u32,
}

pub const CLAP_CORE_EVENT_SPACE_ID: u16 = 0;

pub const CLAP_EVENT_IS_LIVE: u32 = 1 << 0;
pub const CLAP_EVENT_DONT_RECORD: u32 = 1 << 1;

pub const CLAP_EVENT_NOTE_ON: u16 = 0;
pub const CLAP_EVENT_NOTE_OFF: u16 = 1;
pub const CLAP_EVENT_NOTE_CHOKE: u16 = 2;
pub const CLAP_EVENT_NOTE_END: u16 = 3;
pub const CLAP_EVENT_NOTE_EXPRESSION: u16 = 4;
pub const CLAP_EVENT_PARAM_VALUE: u16 = 5;
pub const CLAP_EVENT_PARAM_MOD: u16 = 6;
pub const CLAP_EVENT_PARAM_GESTURE_BEGIN: u16 = 7;
pub const CLAP_EVENT_PARAM_GESTURE_END: u16 = 8;
pub const CLAP_EVENT_TRANSPORT: u16 = 9;
pub const CLAP_EVENT_MIDI: u16 = 10;
pub const CLAP_EVENT_MIDI_SYSEX: u16 = 11;
pub const CLAP_EVENT_MIDI2: u16 = 12;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_event_note_t {
    pub header: clap_event_header_t,
    pub note_id: i32,
    pub port_index: i16,
    pub channel: i16,
    pub key: i16,
    pub velocity: f64,
}

pub const CLAP_NOTE_EXPRESSION_VOLUME: i32 = 0;
pub const CLAP_NOTE_EXPRESSION_PAN: i32 = 1;
pub const CLAP_NOTE_EXPRESSION_TUNING: i32 = 2;
pub const CLAP_NOTE_EXPRESSION_VIBRATO: i32 = 3;
pub const CLAP_NOTE_EXPRESSION_EXPRESSION: i32 = 4;
pub const CLAP_NOTE_EXPRESSION_BRIGHTNESS: i32 = 5;
pub const CLAP_NOTE_EXPRESSION_PRESSURE: i32 = 6;

pub type clap_note_expression = i32;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_event_note_expression_t {
    pub header: clap_event_header_t,
    pub expression_id: clap_note_expression,
    pub note_id: i32,
    pub port_index: i16,
    pub channel: i16,
    pub key: i16,
    pub value: f64,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_event_param_value_t {
    pub header: clap_event_header_t,
    pub param_id: clap_id,
    pub cookie: *mut c_void,
    pub note_id: i32,
    pub port_index: i16,
    pub channel: i16,
    pub key: i16,
    pub value: f64,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_event_param_mod_t {
    pub header: clap_event_header_t,
    pub param_id: clap_id,
    pub cookie: *mut c_void,
    pub note_id: i32,
    pub port_index: i16,
    pub channel: i16,
    pub key: i16,
    pub amount: f64,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_event_param_gesture_t {
    pub header: clap_event_header_t,
    pub param_id: clap_id,
}

pub const CLAP_TRANSPORT_HAS_TEMPO: u32 = 1 << 0;
pub const CLAP_TRANSPORT_HAS_BEATS_TIMELINE: u32 = 1 << 1;
pub const CLAP_TRANSPORT_HAS_SECONDS_TIMELINE: u32 = 1 << 2;
pub const CLAP_TRANSPORT_HAS_TIME_SIGNATURE: u32 = 1 << 3;
pub const CLAP_TRANSPORT_IS_PLAYING: u32 = 1 << 4;
pub const CLAP_TRANSPORT_IS_RECORDING: u32 = 1 << 5;
pub const CLAP_TRANSPORT_IS_LOOP_ACTIVE: u32 = 1 << 6;
pub const CLAP_TRANSPORT_IS_WITHIN_PRE_ROLL: u32 = 1 << 7;

use crate::fixedpoint::{clap_beattime, clap_sectime};

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_event_transport_t {
    pub header: clap_event_header_t,
    pub flags: u32,
    pub song_pos_beats: clap_beattime,
    pub song_pos_seconds: clap_sectime,
    pub tempo: f64,
    pub tempo_inc: f64,
    pub loop_start_beats: clap_beattime,
    pub loop_end_beats: clap_beattime,
    pub loop_start_seconds: clap_sectime,
    pub loop_end_seconds: clap_sectime,
    pub bar_start: clap_beattime,
    pub bar_number: i32,
    pub tsig_num: u16,
    pub tsig_denom: u16,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_event_midi_t {
    pub header: clap_event_header_t,
    pub port_index: u16,
    pub data: [u8; 3],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_event_midi_sysex_t {
    pub header: clap_event_header_t,
    pub port_index: u16,
    pub buffer: *const u8,
    pub size: u32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_event_midi2_t {
    pub header: clap_event_header_t,
    pub port_index: u16,
    pub data: [u32; 4],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_input_events_t {
    pub ctx: *mut c_void,
    pub size: Option<unsafe extern "C" fn(list: *const clap_input_events_t) -> u32>,
    pub get: Option<unsafe extern "C" fn(list: *const clap_input_events_t, index: u32) -> *const clap_event_header_t>,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_output_events_t {
    pub ctx: *mut c_void,
    pub try_push: Option<unsafe extern "C" fn(list: *const clap_output_events_t, event: *const clap_event_header_t) -> bool>,
}
