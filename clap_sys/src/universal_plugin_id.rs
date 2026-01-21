use std::ffi::c_char;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_universal_plugin_id_t {
    pub abi: *const c_char,
    pub id: *const c_char,
}
