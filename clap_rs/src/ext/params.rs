//! Params Extension for clap_rs

use crate::plugin::Plugin;
use crate::plugin_instance::PluginInstance;
use clap_sys::ext::params::{
    clap_plugin_params_t, clap_param_info_t, 
    CLAP_PARAM_IS_AUTOMATABLE, CLAP_EXT_PARAMS
};
use clap_sys::id::clap_id;
use clap_sys::string_sizes::CLAP_PATH_SIZE;
use clap_sys::events::{
    clap_input_events_t, clap_output_events_t, 
    CLAP_EVENT_PARAM_VALUE, clap_event_param_value_t
};
use clap_sys::plugin::clap_plugin_t;
use std::ffi::{c_char, CStr};
use std::ptr;

/// Parameter configuration info
#[derive(Clone, Debug)]
pub struct ParameterInfo {
    pub id: u32,
    pub name: String,
    pub module: String,
    pub min_value: f64,
    pub max_value: f64,
    pub default_value: f64,
}

fn write_cstr_to_array(dst: &mut [c_char], bytes: &[u8]) {
    dst.fill(0);
    let max = dst.len().saturating_sub(1);
    let len = bytes.len().min(max);
    for i in 0..len {
        dst[i] = bytes[i] as c_char;
    }
}

pub(crate) unsafe extern "C" fn params_count<P: Plugin>(plugin: *const clap_plugin_t) -> u32 {
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstance<P>) };
    instance.params_cache.len() as u32
}

pub(crate) unsafe extern "C" fn params_get_info<P: Plugin>(
    plugin: *const clap_plugin_t,
    param_index: u32,
    param_info: *mut clap_param_info_t,
) -> bool {
    if param_info.is_null() { return false; }
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstance<P>) };
    if (param_index as usize) >= instance.params_cache.len() { return false; }
    let param = &instance.params_cache[param_index as usize];
    let info = unsafe { &mut *param_info };
    info.id = param.id;
    info.flags = CLAP_PARAM_IS_AUTOMATABLE;
    info.cookie = ptr::null_mut();
    write_cstr_to_array(&mut info.name, param.name.as_bytes());
    info.module = [0; CLAP_PATH_SIZE];
    info.min_value = param.min_value;
    info.max_value = param.max_value;
    info.default_value = param.default_value;
    true
}

pub(crate) unsafe extern "C" fn params_get_value<P: Plugin>(
    plugin: *const clap_plugin_t,
    param_id: clap_id,
    out_value: *mut f64,
) -> bool {
    if out_value.is_null() { return false; }
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstance<P>) };
    if !instance.params_cache.iter().any(|p| p.id == param_id) { return false; }
    unsafe { *out_value = instance.plugin.get_parameter(param_id); }
    true
}

pub(crate) unsafe extern "C" fn params_value_to_text<P: Plugin>(
    _plugin: *const clap_plugin_t,
    _param_id: clap_id,
    value: f64,
    out_buffer: *mut c_char,
    out_buffer_capacity: u32,
) -> bool {
    if out_buffer.is_null() || out_buffer_capacity == 0 { return false; }
    let text = format!("{:.3}", value);
    let bytes = text.as_bytes();
    let capacity = out_buffer_capacity as usize;
    let len = bytes.len().min(capacity.saturating_sub(1));
    let dst = unsafe { std::slice::from_raw_parts_mut(out_buffer, capacity) };
    dst.fill(0);
    for i in 0..len {
        dst[i] = bytes[i] as c_char;
    }
    true
}

pub(crate) unsafe extern "C" fn params_text_to_value<P: Plugin>(
    _plugin: *const clap_plugin_t,
    _param_id: clap_id,
    param_value_text: *const c_char,
    out_value: *mut f64,
) -> bool {
    if param_value_text.is_null() || out_value.is_null() { return false; }
    let text = unsafe { CStr::from_ptr(param_value_text) };
    if let Ok(value_str) = text.to_str() {
        if let Ok(parsed) = value_str.parse::<f64>() {
            unsafe { *out_value = parsed; }
            return true;
        }
    }
    false
}

pub(crate) unsafe extern "C" fn params_flush<P: Plugin>(
    plugin: *const clap_plugin_t,
    input: *const clap_input_events_t,
    _output: *const clap_output_events_t,
) {
    if input.is_null() { return; }
    let instance = unsafe { &mut *((*plugin).plugin_data as *mut PluginInstance<P>) };
    let size_fn = unsafe { (*input).size };
    let get_fn = unsafe { (*input).get };
    if size_fn.is_none() || get_fn.is_none() { return; }
    let size = unsafe { size_fn.unwrap()(input) };
    for index in 0..size {
        let header = unsafe { get_fn.unwrap()(input, index) };
        if header.is_null() { continue; }
        if unsafe { (*header).space_id == clap_sys::events::CLAP_CORE_EVENT_SPACE_ID && (*header).type_ == CLAP_EVENT_PARAM_VALUE } {
            let event = unsafe { &*(header as *const clap_event_param_value_t) };
            instance.plugin.set_parameter(event.param_id, event.value);
        }
    }
}

/// Create params extension struct
pub(crate) fn create_params_ext<P: Plugin>() -> clap_plugin_params_t {
    clap_plugin_params_t {
        count: Some(params_count::<P>),
        get_info: Some(params_get_info::<P>),
        get_value: Some(params_get_value::<P>),
        value_to_text: Some(params_value_to_text::<P>),
        text_to_value: Some(params_text_to_value::<P>),
        flush: Some(params_flush::<P>),
    }
}

/// Apply parameter events from input events
pub(crate) unsafe fn apply_param_events<P: Plugin>(
    instance: &mut PluginInstance<P>, 
    in_events: *const clap_input_events_t
) {
    if in_events.is_null() { return; }
    let size_fn = unsafe { (*in_events).size };
    let get_fn = unsafe { (*in_events).get };
    if size_fn.is_none() || get_fn.is_none() { return; }
    let size = unsafe { size_fn.unwrap()(in_events) };
    for index in 0..size {
        let header = unsafe { get_fn.unwrap()(in_events, index) };
        if header.is_null() { continue; }
        if unsafe { (*header).space_id == clap_sys::events::CLAP_CORE_EVENT_SPACE_ID && (*header).type_ == CLAP_EVENT_PARAM_VALUE } {
            let event = unsafe { &*(header as *const clap_event_param_value_t) };
            instance.plugin.set_parameter(event.param_id, event.value);
        }
    }
}

// ======= GUI Plugin Support =======

use crate::plugin_instance::PluginInstanceWithGui;
use crate::ext::gui::GuiHandler;

pub(crate) unsafe extern "C" fn params_count_gui<P: Plugin + GuiHandler>(plugin: *const clap_plugin_t) -> u32 {
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstanceWithGui<P>) };
    instance.params_cache.len() as u32
}

pub(crate) unsafe extern "C" fn params_get_info_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    param_index: u32,
    param_info: *mut clap_param_info_t,
) -> bool {
    if param_info.is_null() { return false; }
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstanceWithGui<P>) };
    if (param_index as usize) >= instance.params_cache.len() { return false; }
    let param = &instance.params_cache[param_index as usize];
    let info = unsafe { &mut *param_info };
    info.id = param.id;
    info.flags = CLAP_PARAM_IS_AUTOMATABLE;
    info.cookie = ptr::null_mut();
    write_cstr_to_array(&mut info.name, param.name.as_bytes());
    info.module = [0; CLAP_PATH_SIZE];
    info.min_value = param.min_value;
    info.max_value = param.max_value;
    info.default_value = param.default_value;
    true
}

pub(crate) unsafe extern "C" fn params_get_value_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    param_id: clap_id,
    out_value: *mut f64,
) -> bool {
    if out_value.is_null() { return false; }
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstanceWithGui<P>) };
    if !instance.params_cache.iter().any(|p| p.id == param_id) { return false; }
    unsafe { *out_value = instance.plugin.get_parameter(param_id); }
    true
}

pub(crate) unsafe extern "C" fn params_flush_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    input: *const clap_input_events_t,
    _output: *const clap_output_events_t,
) {
    if input.is_null() { return; }
    let instance = unsafe { &mut *((*plugin).plugin_data as *mut PluginInstanceWithGui<P>) };
    let size_fn = unsafe { (*input).size };
    let get_fn = unsafe { (*input).get };
    if size_fn.is_none() || get_fn.is_none() { return; }
    let size = unsafe { size_fn.unwrap()(input) };
    for index in 0..size {
        let header = unsafe { get_fn.unwrap()(input, index) };
        if header.is_null() { continue; }
        if unsafe { (*header).space_id == clap_sys::events::CLAP_CORE_EVENT_SPACE_ID && (*header).type_ == CLAP_EVENT_PARAM_VALUE } {
            let event = unsafe { &*(header as *const clap_event_param_value_t) };
            instance.plugin.set_parameter(event.param_id, event.value);
        }
    }
}

pub(crate) fn create_params_ext_gui<P: Plugin + GuiHandler>() -> clap_plugin_params_t {
    clap_plugin_params_t {
        count: Some(params_count_gui::<P>),
        get_info: Some(params_get_info_gui::<P>),
        get_value: Some(params_get_value_gui::<P>),
        value_to_text: Some(params_value_to_text::<P>),
        text_to_value: Some(params_text_to_value::<P>),
        flush: Some(params_flush_gui::<P>),
    }
}
