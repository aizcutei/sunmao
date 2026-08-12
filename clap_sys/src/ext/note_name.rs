use crate::host::clap_host_t;
use crate::plugin::clap_plugin_t;
use crate::string_sizes::CLAP_NAME_SIZE;
use std::ffi::c_char;

pub const CLAP_EXT_NOTE_NAME: &str = "clap.note-name\0";

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_note_name_t {
    pub name: [c_char; CLAP_NAME_SIZE],
    pub port: i16,
    pub key: i16,
    pub channel: i16,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_note_name_t {
    pub count: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t) -> u32>,
    pub get: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            index: u32,
            note_name: *mut clap_note_name_t,
        ) -> bool,
    >,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_note_name_t {
    pub changed: Option<unsafe extern "C" fn(host: *const clap_host_t)>,
}
