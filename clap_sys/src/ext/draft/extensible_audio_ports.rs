use crate::plugin::clap_plugin_t;
use std::ffi::{c_char, c_void};

pub const CLAP_EXT_EXTENSIBLE_AUDIO_PORTS: &str = "clap.extensible-audio-ports/1\0";

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_extensible_audio_ports_t {
    pub add_port: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            is_input: bool,
            channel_count: u32,
            port_type: *const c_char,
            port_details: *const c_void,
        ) -> bool,
    >,
    pub remove_port: Option<
        unsafe extern "C" fn(plugin: *const clap_plugin_t, is_input: bool, index: u32) -> bool,
    >,
}
