use crate::host::clap_host_t;
use crate::plugin::clap_plugin_t;
use crate::stream::{clap_istream_t, clap_ostream_t};

pub const CLAP_EXT_STATE: &str = "clap.state\0";

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_state_t {
    pub save: Option<
        unsafe extern "C" fn(plugin: *const clap_plugin_t, stream: *const clap_ostream_t) -> bool,
    >,
    pub load: Option<
        unsafe extern "C" fn(plugin: *const clap_plugin_t, stream: *const clap_istream_t) -> bool,
    >,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_state_t {
    pub mark_dirty: Option<unsafe extern "C" fn(host: *const clap_host_t)>,
}
