use crate::host::clap_host_t;

pub const CLAP_EXT_THREAD_CHECK: &str = "clap.thread-check\0";

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_thread_check_t {
    pub is_main_thread: Option<unsafe extern "C" fn(host: *const clap_host_t) -> bool>,
    pub is_audio_thread: Option<unsafe extern "C" fn(host: *const clap_host_t) -> bool>,
}
