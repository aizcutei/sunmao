use crate::plugin::clap_plugin_t;
use crate::host::clap_host_t;
use crate::id::clap_id;
use std::ffi::{c_char, c_void};

pub const CLAP_EXT_UNDO: &str = "clap.undo/4\0";
pub const CLAP_EXT_UNDO_CONTEXT: &str = "clap.undo_context/4\0";
pub const CLAP_EXT_UNDO_DELTA: &str = "clap.undo_delta/4\0";

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_undo_delta_properties_t {
    pub has_delta: bool,
    pub are_deltas_persistent: bool,
    pub format_version: clap_id,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_undo_delta_t {
    pub get_delta_properties: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, properties: *mut clap_undo_delta_properties_t)>,
    pub can_use_delta_format_version: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, format_version: clap_id) -> bool>,
    pub undo: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, format_version: clap_id, delta: *const c_void, delta_size: usize) -> bool>,
    pub redo: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, format_version: clap_id, delta: *const c_void, delta_size: usize) -> bool>,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_undo_context_t {
    pub set_can_undo: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, can_undo: bool)>,
    pub set_can_redo: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, can_redo: bool)>,
    pub set_undo_name: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, name: *const c_char)>,
    pub set_redo_name: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, name: *const c_char)>,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_undo_t {
    pub begin_change: Option<unsafe extern "C" fn(host: *const clap_host_t)>,
    pub cancel_change: Option<unsafe extern "C" fn(host: *const clap_host_t)>,
    pub change_made: Option<unsafe extern "C" fn(host: *const clap_host_t, name: *const c_char, delta: *const c_void, delta_size: usize, delta_can_undo: bool)>,
    pub request_undo: Option<unsafe extern "C" fn(host: *const clap_host_t)>,
    pub request_redo: Option<unsafe extern "C" fn(host: *const clap_host_t)>,
    pub set_wants_context_updates: Option<unsafe extern "C" fn(host: *const clap_host_t, is_subscribed: bool)>,
}
