use crate::process::{clap_process_status, clap_process_t};
use crate::version::clap_version_t;
use std::ffi::{c_char, c_void};

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_descriptor_t {
    pub clap_version: clap_version_t,
    pub id: *const c_char,
    pub name: *const c_char,
    pub vendor: *const c_char,
    pub url: *const c_char,
    pub manual_url: *const c_char,
    pub support_url: *const c_char,
    pub version: *const c_char,
    pub description: *const c_char,
    pub features: *const *const c_char,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_t {
    pub desc: *const clap_plugin_descriptor_t,
    pub plugin_data: *mut c_void,
    pub init: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t) -> bool>,
    pub destroy: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t)>,
    pub activate: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            sample_rate: f64,
            min_frames_count: u32,
            max_frames_count: u32,
        ) -> bool,
    >,
    pub deactivate: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t)>,
    pub start_processing: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t) -> bool>,
    pub stop_processing: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t)>,
    pub reset: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t)>,
    pub process: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            process: *const clap_process_t,
        ) -> clap_process_status,
    >,
    pub get_extension: Option<
        unsafe extern "C" fn(plugin: *const clap_plugin_t, id: *const c_char) -> *const c_void,
    >,
    pub on_main_thread: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t)>,
}
