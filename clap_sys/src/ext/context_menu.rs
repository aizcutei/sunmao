use crate::plugin::clap_plugin_t;
use crate::host::clap_host_t;
use crate::id::clap_id;
use std::ffi::{c_char, c_void};

pub const CLAP_EXT_CONTEXT_MENU: &str = "clap.context-menu/1\0";
pub const CLAP_EXT_CONTEXT_MENU_COMPAT: &str = "clap.context-menu.draft/0\0";

pub const CLAP_CONTEXT_MENU_TARGET_KIND_GLOBAL: u32 = 0;
pub const CLAP_CONTEXT_MENU_TARGET_KIND_PARAM: u32 = 1;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_context_menu_target_t {
    pub kind: u32,
    pub id: clap_id,
}

pub const CLAP_CONTEXT_MENU_ITEM_ENTRY: u32 = 0;
pub const CLAP_CONTEXT_MENU_ITEM_CHECK_ENTRY: u32 = 1;
pub const CLAP_CONTEXT_MENU_ITEM_SEPARATOR: u32 = 2;
pub const CLAP_CONTEXT_MENU_ITEM_BEGIN_SUBMENU: u32 = 3;
pub const CLAP_CONTEXT_MENU_ITEM_END_SUBMENU: u32 = 4;
pub const CLAP_CONTEXT_MENU_ITEM_TITLE: u32 = 5;

pub type clap_context_menu_item_kind_t = u32;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_context_menu_entry_t {
    pub label: *const c_char,
    pub is_enabled: bool,
    pub action_id: clap_id,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_context_menu_check_entry_t {
    pub label: *const c_char,
    pub is_enabled: bool,
    pub is_checked: bool,
    pub action_id: clap_id,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_context_menu_item_title_t {
    pub title: *const c_char,
    pub is_enabled: bool,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_context_menu_submenu_t {
    pub label: *const c_char,
    pub is_enabled: bool,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_context_menu_builder_t {
    pub ctx: *mut c_void,
    pub add_item: Option<unsafe extern "C" fn(builder: *const clap_context_menu_builder_t, item_kind: clap_context_menu_item_kind_t, item_data: *const c_void) -> bool>,
    pub supports: Option<unsafe extern "C" fn(builder: *const clap_context_menu_builder_t, item_kind: clap_context_menu_item_kind_t) -> bool>,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_context_menu_t {
    pub populate: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, target: *const clap_context_menu_target_t, builder: *const clap_context_menu_builder_t) -> bool>,
    pub perform: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, target: *const clap_context_menu_target_t, action_id: clap_id) -> bool>,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_context_menu_t {
    pub populate: Option<unsafe extern "C" fn(host: *const clap_host_t, target: *const clap_context_menu_target_t, builder: *const clap_context_menu_builder_t) -> bool>,
    pub perform: Option<unsafe extern "C" fn(host: *const clap_host_t, target: *const clap_context_menu_target_t, action_id: clap_id) -> bool>,
    pub can_popup: Option<unsafe extern "C" fn(host: *const clap_host_t) -> bool>,
    pub popup: Option<unsafe extern "C" fn(host: *const clap_host_t, target: *const clap_context_menu_target_t, screen_index: i32, x: i32, y: i32) -> bool>,
}
