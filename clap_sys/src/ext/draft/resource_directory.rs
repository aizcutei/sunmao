use crate::host::clap_host_t;
use crate::plugin::clap_plugin_t;
use std::ffi::c_char;

pub const CLAP_EXT_RESOURCE_DIRECTORY: &str = "clap.resource-directory/1\0";

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_resource_directory_t {
    pub set_directory: Option<
        unsafe extern "C" fn(plugin: *const clap_plugin_t, path: *const c_char, is_shared: bool),
    >,
    pub collect: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, all: bool)>,
    pub get_files_count: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t) -> u32>,
    pub get_file_path: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            index: u32,
            path: *mut c_char,
            path_size: u32,
        ) -> i32,
    >,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_resource_directory_t {
    pub request_directory:
        Option<unsafe extern "C" fn(host: *const clap_host_t, is_shared: bool) -> bool>,
    pub release_directory: Option<unsafe extern "C" fn(host: *const clap_host_t, is_shared: bool)>,
}
