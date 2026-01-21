use crate::plugin::clap_plugin_t;
use crate::host::clap_host_t;
use crate::id::clap_id;
use crate::string_sizes::CLAP_NAME_SIZE;
use std::ffi::c_char;

pub const CLAP_EXT_REMOTE_CONTROLS: &str = "clap.remote-controls/2\0";
pub const CLAP_EXT_REMOTE_CONTROLS_COMPAT: &str = "clap.remote-controls.draft/2\0";

pub const CLAP_REMOTE_CONTROLS_COUNT: usize = 8;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_remote_controls_page_t {
    pub section_name: [c_char; CLAP_NAME_SIZE],
    pub page_id: clap_id,
    pub page_name: [c_char; CLAP_NAME_SIZE],
    pub param_ids: [clap_id; CLAP_REMOTE_CONTROLS_COUNT],
    pub is_for_preset: bool,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_remote_controls_t {
    pub count: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t) -> u32>,
    pub get: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, page_index: u32, page: *mut clap_remote_controls_page_t) -> bool>,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_remote_controls_t {
    pub changed: Option<unsafe extern "C" fn(host: *const clap_host_t)>,
    pub suggest_page: Option<unsafe extern "C" fn(host: *const clap_host_t, page_id: clap_id)>,
}
