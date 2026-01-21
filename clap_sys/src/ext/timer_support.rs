use crate::plugin::clap_plugin_t;
use crate::host::clap_host_t;
use crate::id::clap_id;

pub const CLAP_EXT_TIMER_SUPPORT: &str = "clap.timer-support\0";

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_timer_support_t {
    pub on_timer: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, timer_id: clap_id)>,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_timer_support_t {
    pub register_timer: Option<unsafe extern "C" fn(host: *const clap_host_t, period_ms: u32, timer_id: *mut clap_id) -> bool>,
    pub unregister_timer: Option<unsafe extern "C" fn(host: *const clap_host_t, timer_id: clap_id) -> bool>,
}
