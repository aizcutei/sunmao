use crate::plugin::clap_plugin_t;
use crate::host::clap_host_t;
use crate::id::clap_id;
use crate::events::clap_event_header_t;
use crate::string_sizes::{CLAP_NAME_SIZE, CLAP_PATH_SIZE};
use std::ffi::c_void;

pub const CLAP_EXT_TRIGGERS: &str = "clap.triggers/1\0";

pub const CLAP_TRIGGER_IS_AUTOMATABLE_PER_NOTE_ID: u32 = 1 << 0;
pub const CLAP_TRIGGER_IS_AUTOMATABLE_PER_KEY: u32 = 1 << 1;
pub const CLAP_TRIGGER_IS_AUTOMATABLE_PER_CHANNEL: u32 = 1 << 2;
pub const CLAP_TRIGGER_IS_AUTOMATABLE_PER_PORT: u32 = 1 << 3;

pub type clap_trigger_info_flags = u32;

pub const CLAP_EVENT_TRIGGER: u16 = 0;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_event_trigger_t {
    pub header: clap_event_header_t,
    pub trigger_id: clap_id,
    pub cookie: *mut c_void,
    pub note_id: i32,
    pub port_index: i16,
    pub channel: i16,
    pub key: i16,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_trigger_info_t {
    pub id: clap_id,
    pub flags: clap_trigger_info_flags,
    pub cookie: *mut c_void,
    pub name: [std::ffi::c_char; CLAP_NAME_SIZE],
    pub module: [std::ffi::c_char; CLAP_PATH_SIZE],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_triggers_t {
    pub count: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t) -> u32>,
    pub get_info: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, index: u32, trigger_info: *mut clap_trigger_info_t) -> bool>,
}

pub const CLAP_TRIGGER_RESCAN_INFO: u32 = 1 << 0;
pub const CLAP_TRIGGER_RESCAN_ALL: u32 = 1 << 1;

pub type clap_trigger_rescan_flags = u32;

pub const CLAP_TRIGGER_CLEAR_ALL: u32 = 1 << 0;
pub const CLAP_TRIGGER_CLEAR_AUTOMATIONS: u32 = 1 << 1;

pub type clap_trigger_clear_flags = u32;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_triggers_t {
    pub rescan: Option<unsafe extern "C" fn(host: *const clap_host_t, flags: clap_trigger_rescan_flags)>,
    pub clear: Option<unsafe extern "C" fn(host: *const clap_host_t, trigger_id: clap_id, flags: clap_trigger_clear_flags)>,
}
