use crate::host::clap_host_t;
use crate::plugin::clap_plugin_t;

pub const CLAP_EXT_TAIL: &str = "clap.tail\0";

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_tail_t {
    pub get: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t) -> u32>,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_tail_t {
    pub changed: Option<unsafe extern "C" fn(host: *const clap_host_t)>,
}
