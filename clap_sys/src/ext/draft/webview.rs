use crate::host::clap_host_t;
use crate::plugin::clap_plugin_t;
use crate::stream::clap_ostream_t;
use std::ffi::{c_char, c_void};

pub const CLAP_EXT_WEBVIEW: &str = "clap.webview/3\0";
pub const CLAP_WINDOW_API_WEBVIEW: &str = "webview\0";

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_webview_t {
    pub get_uri: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            uri: *mut c_char,
            uri_capacity: u32,
        ) -> i32,
    >,
    pub get_resource: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            path: *const c_char,
            mime: *mut c_char,
            mime_capacity: u32,
            data_stream: *const clap_ostream_t,
        ) -> bool,
    >,
    pub receive: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            buffer: *const c_void,
            size: u32,
        ) -> bool,
    >,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_webview_t {
    pub send: Option<
        unsafe extern "C" fn(host: *const clap_host_t, buffer: *const c_void, size: u32) -> bool,
    >,
}
