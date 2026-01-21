use std::ffi::c_char;

pub const CLAP_PLUGIN_INVALIDATION_FACTORY_ID: &str = "clap.plugin-invalidation-factory/1\0";

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_invalidation_source_t {
    pub directory: *const c_char,
    pub filename_glob: *const c_char,
    pub recursive_scan: bool,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_invalidation_factory_t {
    pub count: Option<unsafe extern "C" fn(factory: *const clap_plugin_invalidation_factory_t) -> u32>,
    pub get: Option<unsafe extern "C" fn(factory: *const clap_plugin_invalidation_factory_t, index: u32) -> *const clap_plugin_invalidation_source_t>,
    pub refresh: Option<unsafe extern "C" fn(factory: *const clap_plugin_invalidation_factory_t) -> bool>,
}
