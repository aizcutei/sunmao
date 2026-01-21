use crate::plugin::clap_plugin_descriptor_t;
use crate::plugin::clap_plugin_t;
use crate::host::clap_host_t;
use std::ffi::c_char;

pub const CLAP_PLUGIN_FACTORY_ID: &str = "clap.plugin-factory\0"; // Using &str for convenience, but C struct uses const char* convention.

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_factory_t {
    pub get_plugin_count: Option<unsafe extern "C" fn(factory: *const clap_plugin_factory_t) -> u32>,
    pub get_plugin_descriptor: Option<unsafe extern "C" fn(factory: *const clap_plugin_factory_t, index: u32) -> *const clap_plugin_descriptor_t>,
    pub create_plugin: Option<unsafe extern "C" fn(factory: *const clap_plugin_factory_t, host: *const clap_host_t, plugin_id: *const c_char) -> *const clap_plugin_t>,
}
