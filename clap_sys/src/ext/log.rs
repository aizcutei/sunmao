use crate::host::clap_host_t;
use std::ffi::c_char;

pub const CLAP_EXT_LOG: &str = "clap.log\0";

pub const CLAP_LOG_DEBUG: i32 = 0;
pub const CLAP_LOG_INFO: i32 = 1;
pub const CLAP_LOG_WARNING: i32 = 2;
pub const CLAP_LOG_ERROR: i32 = 3;
pub const CLAP_LOG_FATAL: i32 = 4;
pub const CLAP_LOG_HOST_MISBEHAVING: i32 = 5;
pub const CLAP_LOG_PLUGIN_MISBEHAVING: i32 = 6;

pub type clap_log_severity = i32;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_log_t {
    pub log: Option<unsafe extern "C" fn(host: *const clap_host_t, severity: clap_log_severity, msg: *const c_char)>,
}
