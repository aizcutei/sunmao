use crate::plugin::clap_plugin_t;
use crate::host::clap_host_t;
use crate::id::clap_id;
use crate::string_sizes::CLAP_NAME_SIZE;
use std::ffi::c_char;

pub const CLAP_EXT_NOTE_PORTS: &str = "clap.note-ports\0";

pub const CLAP_NOTE_DIALECT_CLAP: u32 = 1 << 0;
pub const CLAP_NOTE_DIALECT_MIDI: u32 = 1 << 1;
pub const CLAP_NOTE_DIALECT_MIDI_MPE: u32 = 1 << 2;
pub const CLAP_NOTE_DIALECT_MIDI2: u32 = 1 << 3;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_note_port_info_t {
    pub id: clap_id,
    pub supported_dialects: u32,
    pub preferred_dialect: u32,
    pub name: [c_char; CLAP_NAME_SIZE],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_note_ports_t {
    pub count: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, is_input: bool) -> u32>,
    pub get: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, index: u32, is_input: bool, info: *mut clap_note_port_info_t) -> bool>,
}

pub const CLAP_NOTE_PORTS_RESCAN_ALL: u32 = 1 << 0;
pub const CLAP_NOTE_PORTS_RESCAN_NAMES: u32 = 1 << 1;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_note_ports_t {
    pub supported_dialects: Option<unsafe extern "C" fn(host: *const clap_host_t) -> u32>,
    pub rescan: Option<unsafe extern "C" fn(host: *const clap_host_t, flags: u32)>,
}
