use crate::version::clap_version_t;
use std::ffi::{c_char, c_void};

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_t {
    pub clap_version: clap_version_t,
    pub host_data: *mut c_void,
    pub name: *const c_char,
    pub vendor: *const c_char,
    pub url: *const c_char,
    pub version: *const c_char,
    pub get_extension: Option<unsafe extern "C" fn(host: *const clap_host_t, extension_id: *const c_char) -> *const c_void>,
    pub request_restart: Option<unsafe extern "C" fn(host: *const clap_host_t)>,
    pub request_process: Option<unsafe extern "C" fn(host: *const clap_host_t)>,
    pub request_callback: Option<unsafe extern "C" fn(host: *const clap_host_t)>,
}
