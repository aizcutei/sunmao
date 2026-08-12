use crate::host::clap_host_t;
use crate::id::clap_id;
use crate::plugin::clap_plugin_t;
use crate::string_sizes::CLAP_NAME_SIZE;
use std::ffi::c_char;

pub const CLAP_EXT_AUDIO_PORTS: &str = "clap.audio-ports\0";
pub const CLAP_PORT_MONO: &str = "mono\0";
pub const CLAP_PORT_STEREO: &str = "stereo\0";

pub const CLAP_AUDIO_PORT_IS_MAIN: u32 = 1 << 0;
pub const CLAP_AUDIO_PORT_SUPPORTS_64BITS: u32 = 1 << 1;
pub const CLAP_AUDIO_PORT_PREFERS_64BITS: u32 = 1 << 2;
pub const CLAP_AUDIO_PORT_REQUIRES_COMMON_SAMPLE_SIZE: u32 = 1 << 3;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_audio_port_info_t {
    pub id: clap_id,
    pub name: [c_char; CLAP_NAME_SIZE],
    pub flags: u32,
    pub channel_count: u32,
    pub port_type: *const c_char,
    pub in_place_pair: clap_id,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_audio_ports_t {
    pub count: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, is_input: bool) -> u32>,
    pub get: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            index: u32,
            is_input: bool,
            info: *mut clap_audio_port_info_t,
        ) -> bool,
    >,
}

pub const CLAP_AUDIO_PORTS_RESCAN_NAMES: u32 = 1 << 0;
pub const CLAP_AUDIO_PORTS_RESCAN_FLAGS: u32 = 1 << 1;
pub const CLAP_AUDIO_PORTS_RESCAN_CHANNEL_COUNT: u32 = 1 << 2;
pub const CLAP_AUDIO_PORTS_RESCAN_PORT_TYPE: u32 = 1 << 3;
pub const CLAP_AUDIO_PORTS_RESCAN_IN_PLACE_PAIR: u32 = 1 << 4;
pub const CLAP_AUDIO_PORTS_RESCAN_LIST: u32 = 1 << 5;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_audio_ports_t {
    pub is_rescan_flag_supported:
        Option<unsafe extern "C" fn(host: *const clap_host_t, flag: u32) -> bool>,
    pub rescan: Option<unsafe extern "C" fn(host: *const clap_host_t, flags: u32)>,
}
