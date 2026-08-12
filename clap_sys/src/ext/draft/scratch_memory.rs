use crate::host::clap_host_t;
use std::ffi::c_void;

pub const CLAP_EXT_SCRATCH_MEMORY: &str = "clap.scratch-memory/1\0";

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_scratch_memory_t {
    pub reserve: Option<
        unsafe extern "C" fn(
            host: *const clap_host_t,
            scratch_size_bytes: u32,
            max_concurrency_hint: u32,
        ) -> bool,
    >,
    pub access: Option<unsafe extern "C" fn(host: *const clap_host_t) -> *mut c_void>,
}
