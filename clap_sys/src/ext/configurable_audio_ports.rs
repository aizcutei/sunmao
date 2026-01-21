use crate::plugin::clap_plugin_t;
use std::ffi::{c_char, c_void};

pub const CLAP_EXT_CONFIGURABLE_AUDIO_PORTS: &str = "clap.configurable-audio-ports/1\0";
pub const CLAP_EXT_CONFIGURABLE_AUDIO_PORTS_COMPAT: &str = "clap.configurable-audio-ports.draft1\0";

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_audio_port_configuration_request_t {
    pub is_input: bool,
    pub port_index: u32,
    pub channel_count: u32,
    pub port_type: *const c_char,
    pub port_details: *const c_void,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_configurable_audio_ports_t {
    pub can_apply_configuration: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, requests: *const clap_audio_port_configuration_request_t, request_count: u32) -> bool>,
    pub apply_configuration: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, requests: *const clap_audio_port_configuration_request_t, request_count: u32) -> bool>,
}
