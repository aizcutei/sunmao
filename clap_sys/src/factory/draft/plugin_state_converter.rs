use crate::version::clap_version_t;
use crate::universal_plugin_id::clap_universal_plugin_id_t;
use crate::stream::{clap_istream_t, clap_ostream_t};
use crate::id::clap_id;
use std::ffi::{c_char, c_void};

pub const CLAP_PLUGIN_STATE_CONVERTER_FACTORY_ID: &str = "clap.plugin-state-converter-factory/1\0";

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_state_converter_descriptor_t {
    pub clap_version: clap_version_t,
    pub src_plugin_id: clap_universal_plugin_id_t,
    pub dst_plugin_id: clap_universal_plugin_id_t,
    pub id: *const c_char,
    pub name: *const c_char,
    pub vendor: *const c_char,
    pub version: *const c_char,
    pub description: *const c_char,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_state_converter_t {
    pub desc: *const clap_plugin_state_converter_descriptor_t,
    pub converter_data: *mut c_void,
    pub destroy: Option<unsafe extern "C" fn(converter: *mut clap_plugin_state_converter_t)>,
    pub convert_state: Option<unsafe extern "C" fn(converter: *mut clap_plugin_state_converter_t, src: *const clap_istream_t, dst: *const clap_ostream_t, error_buffer: *mut c_char, error_buffer_size: usize) -> bool>,
    pub convert_normalized_value: Option<unsafe extern "C" fn(converter: *mut clap_plugin_state_converter_t, src_param_id: clap_id, src_normalized_value: f64, dst_param_id: *mut clap_id, dst_normalized_value: *mut f64) -> bool>,
    pub convert_plain_value: Option<unsafe extern "C" fn(converter: *mut clap_plugin_state_converter_t, src_param_id: clap_id, src_plain_value: f64, dst_param_id: *mut clap_id, dst_plain_value: *mut f64) -> bool>,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_plugin_state_converter_factory_t {
    pub count: Option<unsafe extern "C" fn(factory: *const clap_plugin_state_converter_factory_t) -> u32>,
    pub get_descriptor: Option<unsafe extern "C" fn(factory: *const clap_plugin_state_converter_factory_t, index: u32) -> *const clap_plugin_state_converter_descriptor_t>,
    pub create: Option<unsafe extern "C" fn(factory: *const clap_plugin_state_converter_factory_t, converter_id: *const c_char) -> *mut clap_plugin_state_converter_t>,
}
