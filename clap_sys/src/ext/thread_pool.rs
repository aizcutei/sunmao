use crate::host::clap_host_t;
use crate::plugin::clap_plugin_t;

pub const CLAP_EXT_THREAD_POOL: &str = "clap.thread-pool\0";

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_thread_pool_t {
    pub exec: Option<unsafe extern "C" fn(plugin: *const clap_plugin_t, task_index: u32)>,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_thread_pool_t {
    pub request_exec:
        Option<unsafe extern "C" fn(host: *const clap_host_t, num_tasks: u32) -> bool>,
}
