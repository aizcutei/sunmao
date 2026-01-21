use crate::plugin::clap_plugin_t;
use crate::host::clap_host_t;

pub const CLAP_EXT_VOICE_INFO: &str = "clap.voice-info\0";

pub const CLAP_VOICE_INFO_SUPPORTS_OVERLAPPING_NOTES: u64 = 1 << 0;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_voice_info_t {
    pub voice_count: u32,
    pub voice_capacity: u32,
    pub flags: u64,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_voice_info_t {
    pub get: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, info: *mut clap_voice_info_t) -> bool>,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_voice_info_t {
    pub changed: Option<unsafe extern "C" fn(host: *const clap_host_t)>,
}
