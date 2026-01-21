use crate::plugin::clap_plugin_t;
use crate::host::clap_host_t;
use crate::events::{clap_input_events_t, clap_output_events_t};
use crate::id::clap_id;
use crate::string_sizes::{CLAP_NAME_SIZE, CLAP_PATH_SIZE};
use std::ffi::{c_char, c_void};

pub const CLAP_EXT_PARAMS: &str = "clap.params\0";

pub const CLAP_PARAM_IS_STEPPED: u32 = 1 << 0;
pub const CLAP_PARAM_IS_PERIODIC: u32 = 1 << 1;
pub const CLAP_PARAM_IS_HIDDEN: u32 = 1 << 2;
pub const CLAP_PARAM_IS_READONLY: u32 = 1 << 3;
pub const CLAP_PARAM_IS_BYPASS: u32 = 1 << 4;
pub const CLAP_PARAM_IS_AUTOMATABLE: u32 = 1 << 5;
pub const CLAP_PARAM_IS_AUTOMATABLE_PER_NOTE_ID: u32 = 1 << 6;
pub const CLAP_PARAM_IS_AUTOMATABLE_PER_KEY: u32 = 1 << 7;
pub const CLAP_PARAM_IS_AUTOMATABLE_PER_CHANNEL: u32 = 1 << 8;
pub const CLAP_PARAM_IS_AUTOMATABLE_PER_PORT: u32 = 1 << 9;
pub const CLAP_PARAM_IS_MODULATABLE: u32 = 1 << 10;
pub const CLAP_PARAM_IS_MODULATABLE_PER_NOTE_ID: u32 = 1 << 11;
pub const CLAP_PARAM_IS_MODULATABLE_PER_KEY: u32 = 1 << 12;
pub const CLAP_PARAM_IS_MODULATABLE_PER_CHANNEL: u32 = 1 << 13;
pub const CLAP_PARAM_IS_MODULATABLE_PER_PORT: u32 = 1 << 14;
pub const CLAP_PARAM_REQUIRES_PROCESS: u32 = 1 << 15;
pub const CLAP_PARAM_IS_ENUM: u32 = 1 << 16;

pub type clap_param_info_flags = u32;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_param_info_t {
    pub id: clap_id,
    pub flags: clap_param_info_flags,
    pub cookie: *mut c_void,
    pub name: [c_char; CLAP_NAME_SIZE],
    pub module: [c_char; CLAP_PATH_SIZE],
    pub min_value: f64,
    pub max_value: f64,
    pub default_value: f64,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_params_t {
    pub count: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t) -> u32>,
    pub get_info: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, param_index: u32, param_info: *mut clap_param_info_t) -> bool>,
    pub get_value: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, param_id: clap_id, out_value: *mut f64) -> bool>,
    pub value_to_text: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, param_id: clap_id, value: f64, out_buffer: *mut c_char, out_buffer_capacity: u32) -> bool>,
    pub text_to_value: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, param_id: clap_id, param_value_text: *const c_char, out_value: *mut f64) -> bool>,
    pub flush: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, input: *const clap_input_events_t, output: *const clap_output_events_t)>,
}

pub const CLAP_PARAM_RESCAN_VALUES: u32 = 1 << 0;
pub const CLAP_PARAM_RESCAN_TEXT: u32 = 1 << 1;
pub const CLAP_PARAM_RESCAN_INFO: u32 = 1 << 2;
pub const CLAP_PARAM_RESCAN_ALL: u32 = 1 << 3;

pub type clap_param_rescan_flags = u32;

pub const CLAP_PARAM_CLEAR_ALL: u32 = 1 << 0;
pub const CLAP_PARAM_CLEAR_AUTOMATIONS: u32 = 1 << 1;
pub const CLAP_PARAM_CLEAR_MODULATIONS: u32 = 1 << 2;

pub type clap_param_clear_flags = u32;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_params_t {
    pub rescan: Option<unsafe extern "C" fn(host: *const clap_host_t, flags: clap_param_rescan_flags)>,
    pub clear: Option<unsafe extern "C" fn(host: *const clap_host_t, param_id: clap_id, flags: clap_param_clear_flags)>,
    pub request_flush: Option<unsafe extern "C" fn(host: *const clap_host_t)>,
}
