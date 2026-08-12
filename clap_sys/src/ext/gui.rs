use crate::host::clap_host_t;
use crate::plugin::clap_plugin_t;
use std::ffi::{c_char, c_void};

pub const CLAP_EXT_GUI: &str = "clap.gui\0";
pub const CLAP_WINDOW_API_WIN32: &str = "win32\0";
pub const CLAP_WINDOW_API_COCOA: &str = "cocoa\0";
pub const CLAP_WINDOW_API_X11: &str = "x11\0";
pub const CLAP_WINDOW_API_WAYLAND: &str = "wayland\0";

pub type clap_hwnd = *mut c_void;
pub type clap_nsview = *mut c_void;
pub type clap_xwnd = std::os::raw::c_ulong;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct clap_window_t {
    pub api: *const c_char,
    pub handle: clap_window_handle_u,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union clap_window_handle_u {
    pub cocoa: clap_nsview,
    pub x11: clap_xwnd,
    pub win32: clap_hwnd,
    pub ptr: *mut c_void,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_gui_resize_hints_t {
    pub can_resize_horizontally: bool,
    pub can_resize_vertically: bool,
    pub preserve_aspect_ratio: bool,
    pub aspect_ratio_width: u32,
    pub aspect_ratio_height: u32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_gui_t {
    pub is_api_supported: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            api: *const c_char,
            is_floating: bool,
        ) -> bool,
    >,
    pub get_preferred_api: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            api: *mut *const c_char,
            is_floating: *mut bool,
        ) -> bool,
    >,
    pub create: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            api: *const c_char,
            is_floating: bool,
        ) -> bool,
    >,
    pub destroy: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t)>,
    pub set_scale: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, scale: f64) -> bool>,
    pub get_size: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            width: *mut u32,
            height: *mut u32,
        ) -> bool,
    >,
    pub can_resize: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t) -> bool>,
    pub get_resize_hints: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            hints: *mut clap_gui_resize_hints_t,
        ) -> bool,
    >,
    pub adjust_size: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin_t,
            width: *mut u32,
            height: *mut u32,
        ) -> bool,
    >,
    pub set_size:
        Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, width: u32, height: u32) -> bool>,
    pub set_parent: Option<
        unsafe extern "C" fn(plugin: *const clap_plugin_t, window: *const clap_window_t) -> bool,
    >,
    pub set_transient: Option<
        unsafe extern "C" fn(plugin: *const clap_plugin_t, window: *const clap_window_t) -> bool,
    >,
    pub suggest_title:
        Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, title: *const c_char)>,
    pub show: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t) -> bool>,
    pub hide: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t) -> bool>,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_gui_t {
    pub resize_hints_changed: Option<unsafe extern "C" fn(host: *const clap_host_t)>,
    pub request_resize:
        Option<unsafe extern "C" fn(host: *const clap_host_t, width: u32, height: u32) -> bool>,
    pub request_show: Option<unsafe extern "C" fn(host: *const clap_host_t) -> bool>,
    pub request_hide: Option<unsafe extern "C" fn(host: *const clap_host_t) -> bool>,
    pub closed: Option<unsafe extern "C" fn(host: *const clap_host_t, was_destroyed: bool)>,
}
