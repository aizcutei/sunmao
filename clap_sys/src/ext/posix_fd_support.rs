use crate::host::clap_host_t;
use crate::plugin::clap_plugin_t;

pub const CLAP_EXT_POSIX_FD_SUPPORT: &str = "clap.posix-fd-support\0";

pub const CLAP_POSIX_FD_READ: u32 = 1 << 0;
pub const CLAP_POSIX_FD_WRITE: u32 = 1 << 1;
pub const CLAP_POSIX_FD_ERROR: u32 = 1 << 2;

pub type clap_posix_fd_flags_t = u32;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_posix_fd_support_t {
    pub on_fd: Option<
        unsafe extern "C" fn(plugin: *const clap_plugin_t, fd: i32, flags: clap_posix_fd_flags_t),
    >,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_host_posix_fd_support_t {
    pub register_fd: Option<
        unsafe extern "C" fn(
            host: *const clap_host_t,
            fd: i32,
            flags: clap_posix_fd_flags_t,
        ) -> bool,
    >,
    pub modify_fd: Option<
        unsafe extern "C" fn(
            host: *const clap_host_t,
            fd: i32,
            flags: clap_posix_fd_flags_t,
        ) -> bool,
    >,
    pub unregister_fd: Option<unsafe extern "C" fn(host: *const clap_host_t, fd: i32) -> bool>,
}
