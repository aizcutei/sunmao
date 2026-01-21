use crate::plugin::clap_plugin_t;
use crate::host::clap_host_t;
use crate::id::clap_id;
use crate::events::clap_event_header_t;
use crate::string_sizes::CLAP_NAME_SIZE;
use std::ffi::c_char;

pub const CLAP_EXT_TUNING: &str = "clap.tuning/2\0";

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_event_tuning_t {
    pub header: clap_event_header_t,
    pub port_index: i16,
    pub channel: i16,
    pub tuning_id: clap_id,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_tuning_info_t {
    pub tuning_id: clap_id,
    pub name: [c_char; CLAP_NAME_SIZE],
    pub is_dynamic: bool,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_tuning_t {
    pub changed: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t)>,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_tuning_t {
    pub get_relative: Option<unsafe extern "C" fn(host: *const clap_host_t, tuning_id: clap_id, channel: i32, key: i32, sample_offset: u32) -> f64>,
    pub should_play: Option<unsafe extern "C" fn(host: *const clap_host_t, tuning_id: clap_id, channel: i32, key: i32) -> bool>,
    pub get_tuning_count: Option<unsafe extern "C" fn(host: *const clap_host_t) -> u32>,
    pub get_info: Option<unsafe extern "C" fn(host: *const clap_host_t, tuning_index: u32, info: *mut clap_tuning_info_t) -> bool>,
}
