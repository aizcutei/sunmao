use crate::plugin::clap_plugin_t;
use crate::host::clap_host_t;
use crate::color::clap_color_t;
use crate::string_sizes::CLAP_NAME_SIZE;
use std::ffi::c_char;

pub const CLAP_EXT_TRACK_INFO: &str = "clap.track-info/1\0";
pub const CLAP_EXT_TRACK_INFO_COMPAT: &str = "clap.track-info.draft/1\0";

pub const CLAP_TRACK_INFO_HAS_TRACK_NAME: u64 = 1 << 0;
pub const CLAP_TRACK_INFO_HAS_TRACK_COLOR: u64 = 1 << 1;
pub const CLAP_TRACK_INFO_HAS_AUDIO_CHANNEL: u64 = 1 << 2;
pub const CLAP_TRACK_INFO_IS_FOR_RETURN_TRACK: u64 = 1 << 3;
pub const CLAP_TRACK_INFO_IS_FOR_BUS: u64 = 1 << 4;
pub const CLAP_TRACK_INFO_IS_FOR_MASTER: u64 = 1 << 5;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_track_info_t {
    pub flags: u64,
    pub name: [c_char; CLAP_NAME_SIZE],
    pub color: clap_color_t,
    pub audio_channel_count: i32,
    pub audio_port_type: *const c_char,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_track_info_t {
    pub changed: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t)>,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_track_info_t {
    pub get: Option<unsafe extern "C" fn(host: *const clap_host_t, info: *mut clap_track_info_t) -> bool>,
}
