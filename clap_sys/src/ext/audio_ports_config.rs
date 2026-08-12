use crate::ext::audio_ports::clap_audio_port_info_t;
use crate::host::clap_host_t;
use crate::id::clap_id;
use crate::plugin::clap_plugin_t;
use crate::string_sizes::CLAP_NAME_SIZE;
use std::ffi::c_char;

pub const CLAP_EXT_AUDIO_PORTS_CONFIG: &str = "clap.audio-ports-config\0";
pub const CLAP_EXT_AUDIO_PORTS_CONFIG_INFO: &str = "clap.audio-ports-config-info/1\0";
pub const CLAP_EXT_AUDIO_PORTS_CONFIG_INFO_COMPAT: &str = "clap.audio-ports-config-info/draft-0\0";

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_audio_ports_config_t {
    pub id: clap_id,
    pub name: [c_char; CLAP_NAME_SIZE],
    pub input_port_count: u32,
    pub output_port_count: u32,
    pub has_main_input: bool,
    pub main_input_channel_count: u32,
    pub main_input_port_type: *const c_char,
    pub has_main_output: bool,
    pub main_output_channel_count: u32,
    pub main_output_port_type: *const c_char,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_audio_ports_config_t {
    pub count: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t) -> u32>,
    pub get: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            index: u32,
            config: *mut clap_audio_ports_config_t,
        ) -> bool,
    >,
    pub select:
        Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, config_id: clap_id) -> bool>,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_audio_ports_config_info_t {
    pub current_config: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t) -> clap_id>,
    pub get: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            config_id: clap_id,
            port_index: u32,
            is_input: bool,
            info: *mut clap_audio_port_info_t,
        ) -> bool,
    >,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_audio_ports_config_t {
    pub rescan: Option<unsafe extern "C" fn(host: *const clap_host_t)>,
}
