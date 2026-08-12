use crate::host::clap_host_t;
use crate::plugin::clap_plugin_t;

pub const CLAP_EXT_LATENCY: &str = "clap.latency\0";

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_latency_t {
    pub get: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t) -> u32>,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_latency_t {
    pub changed: Option<unsafe extern "C" fn(host: *const clap_host_t)>,
}
