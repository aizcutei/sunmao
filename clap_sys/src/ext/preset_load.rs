use crate::host::clap_host_t;
use crate::plugin::clap_plugin_t;
use std::ffi::c_char;

pub const CLAP_EXT_PRESET_LOAD: &str = "clap.preset-load/2\0";
pub const CLAP_EXT_PRESET_LOAD_COMPAT: &str = "clap.preset-load.draft/2\0";

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_preset_load_t {
    pub from_location: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            location_kind: u32,
            location: *const c_char,
            load_key: *const c_char,
        ) -> bool,
    >,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_preset_load_t {
    pub on_error: Option<
        unsafe extern "C" fn(
            host: *const clap_host_t,
            location_kind: u32,
            location: *const c_char,
            load_key: *const c_char,
            os_error: i32,
            msg: *const c_char,
        ),
    >,
    pub loaded: Option<
        unsafe extern "C" fn(
            host: *const clap_host_t,
            location_kind: u32,
            location: *const c_char,
            load_key: *const c_char,
        ),
    >,
}
