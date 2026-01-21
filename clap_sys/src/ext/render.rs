use crate::plugin::clap_plugin_t;

pub const CLAP_EXT_RENDER: &str = "clap.render\0";

pub const CLAP_RENDER_REALTIME: i32 = 0;
pub const CLAP_RENDER_OFFLINE: i32 = 1;

pub type clap_plugin_render_mode = i32;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_render_t {
    pub has_hard_realtime_requirement: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t) -> bool>,
    pub set: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, mode: clap_plugin_render_mode) -> bool>,
}
