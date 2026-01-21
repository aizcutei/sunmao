use crate::plugin::clap_plugin_t;
use crate::stream::{clap_istream_t, clap_ostream_t};

pub const CLAP_EXT_STATE_CONTEXT: &str = "clap.state-context/2\0";

pub const CLAP_STATE_CONTEXT_FOR_PRESET: u32 = 1;
pub const CLAP_STATE_CONTEXT_FOR_DUPLICATE: u32 = 2;
pub const CLAP_STATE_CONTEXT_FOR_PROJECT: u32 = 3;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_state_context_t {
    pub save: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, stream: *const clap_ostream_t, context_type: u32) -> bool>,
    pub load: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, stream: *const clap_istream_t, context_type: u32) -> bool>,
}
