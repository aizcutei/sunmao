//! Params Extension for clap_rs

use crate::plugin::Plugin;
use crate::plugin_instance::{ffi_guard, instance_ptr};
use crate::process::MAX_PROCESS_EVENTS;
use clap_sys::events::{
    CLAP_EVENT_PARAM_VALUE, clap_event_param_value_t, clap_input_events_t, clap_output_events_t,
};
use clap_sys::ext::params::{
    CLAP_PARAM_IS_AUTOMATABLE, CLAP_PARAM_IS_STEPPED, clap_param_info_t, clap_plugin_params_t,
};
use clap_sys::id::clap_id;
use clap_sys::plugin::clap_plugin_t;
use std::ffi::{CStr, c_char};
use std::ptr;

unsafe fn is_param_value_event(header: *const clap_sys::events::clap_event_header_t) -> bool {
    !header.is_null()
        && unsafe { (*header).space_id == clap_sys::events::CLAP_CORE_EVENT_SPACE_ID }
        && unsafe { (*header).type_ == CLAP_EVENT_PARAM_VALUE }
        && unsafe { (*header).size >= std::mem::size_of::<clap_event_param_value_t>() as u32 }
}

/// Parameter configuration info
#[derive(Clone, Debug)]
pub struct ParameterInfo {
    pub id: u32,
    pub name: String,
    pub module: String,
    pub min_value: f64,
    pub max_value: f64,
    pub default_value: f64,
    pub is_stepped: bool,
}

fn write_cstr_to_array(dst: &mut [c_char], bytes: &[u8]) {
    dst.fill(0);
    let max = dst.len().saturating_sub(1);
    let len = bytes.len().min(max);
    for i in 0..len {
        dst[i] = bytes[i] as c_char;
    }
}

fn parameter_flags(param: &ParameterInfo) -> u32 {
    CLAP_PARAM_IS_AUTOMATABLE
        | if param.is_stepped {
            CLAP_PARAM_IS_STEPPED
        } else {
            0
        }
}

fn valid_parameter_value(param: &ParameterInfo, value: f64) -> bool {
    value.is_finite()
        && param.min_value.is_finite()
        && param.max_value.is_finite()
        && param.min_value <= param.max_value
        && (param.min_value..=param.max_value).contains(&value)
}

fn sanitize_parameter_value(param: &ParameterInfo, value: f64) -> Option<f64> {
    valid_parameter_value(param, value).then_some(value)
}

pub(crate) unsafe extern "C" fn params_count<P: Plugin>(plugin: *const clap_plugin_t) -> u32 {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return 0;
    };
    let instance = unsafe { &*instance_ptr };
    instance.params_cache.len() as u32
}

pub(crate) unsafe extern "C" fn params_get_info<P: Plugin>(
    plugin: *const clap_plugin_t,
    param_index: u32,
    param_info: *mut clap_param_info_t,
) -> bool {
    if param_info.is_null() {
        return false;
    }
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &*instance_ptr };
    if (param_index as usize) >= instance.params_cache.len() {
        return false;
    }
    let param = &instance.params_cache[param_index as usize];
    let info = unsafe { &mut *param_info };
    info.id = param.id;
    info.flags = parameter_flags(param);
    info.cookie = ptr::null_mut();
    write_cstr_to_array(&mut info.name, param.name.as_bytes());
    // `module` is the parameter's `/`-separated group path. It was previously
    // zeroed unconditionally, so a declared hierarchy never reached the host.
    write_cstr_to_array(&mut info.module, param.module.as_bytes());
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
    if out_value.is_null() {
        return false;
    }
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &*instance_ptr };
    if !instance.params_cache.iter().any(|p| p.id == param_id) {
        return false;
    }
    let Some(param) = instance.params_cache.iter().find(|p| p.id == param_id) else {
        return false;
    };
    let Some(value) = ffi_guard(None, || unsafe {
        sanitize_parameter_value(param, instance.controller().get_parameter(param_id))
    }) else {
        return false;
    };
    unsafe { *out_value = value };
    true
}

pub(crate) unsafe extern "C" fn params_value_to_text<P: Plugin>(
    plugin: *const clap_plugin_t,
    param_id: clap_id,
    value: f64,
    out_buffer: *mut c_char,
    out_buffer_capacity: u32,
) -> bool {
    if out_buffer.is_null() || out_buffer_capacity == 0 {
        return false;
    }
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &*instance_ptr };
    let Some(param) = instance
        .params_cache
        .iter()
        .find(|param| param.id == param_id)
    else {
        return false;
    };
    if !valid_parameter_value(param, value) {
        return false;
    }
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
    plugin: *const clap_plugin_t,
    param_id: clap_id,
    param_value_text: *const c_char,
    out_value: *mut f64,
) -> bool {
    if param_value_text.is_null() || out_value.is_null() {
        return false;
    }
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &*instance_ptr };
    let Some(param) = instance
        .params_cache
        .iter()
        .find(|param| param.id == param_id)
    else {
        return false;
    };
    let text = unsafe { CStr::from_ptr(param_value_text) };
    if let Ok(value_str) = text.to_str() {
        if let Ok(parsed) = value_str.parse::<f64>() {
            if valid_parameter_value(param, parsed) {
                unsafe {
                    *out_value = parsed;
                }
                return true;
            }
        }
    }
    false
}

pub(crate) unsafe extern "C" fn params_flush<P: Plugin>(
    plugin: *const clap_plugin_t,
    input: *const clap_input_events_t,
    output: *const clap_output_events_t,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        params_flush_unchecked::<P>(plugin, input, output);
    }));
}

unsafe fn params_flush_unchecked<P: Plugin>(
    plugin: *const clap_plugin_t,
    input: *const clap_input_events_t,
    output: *const clap_output_events_t,
) {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return;
    };
    let instance = unsafe { &*instance_ptr };
    if !input.is_null() {
        let size_fn = unsafe { (*input).size };
        let get_fn = unsafe { (*input).get };
        if let (Some(size_fn), Some(get_fn)) = (size_fn, get_fn) {
            let size = unsafe { size_fn(input) };
            for index in 0..size.min(MAX_PROCESS_EVENTS as u32) {
                let header = unsafe { get_fn(input, index) };
                if header.is_null() {
                    continue;
                }
                if unsafe { is_param_value_event(header) } {
                    let event = unsafe { &*(header as *const clap_event_param_value_t) };
                    if instance
                        .params_cache
                        .iter()
                        .find(|param| param.id == event.param_id)
                        .is_some_and(|param| valid_parameter_value(param, event.value))
                    {
                        unsafe {
                            instance.set_parameter_for_current_thread(event.param_id, event.value)
                        };
                    }
                }
            }
        }
    }
    unsafe { instance.host.flush_parameter_events(output) };
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

// ======= GUI Plugin Support =======

#[cfg(test)]
mod tests {
    use super::*;

    fn parameter(is_stepped: bool) -> ParameterInfo {
        ParameterInfo {
            id: 7,
            name: "Mode".to_string(),
            module: String::new(),
            min_value: 0.0,
            max_value: 1.0,
            default_value: 0.5,
            is_stepped,
        }
    }

    #[test]
    fn stepped_parameters_set_the_clap_stepped_flag() {
        let continuous_flags = parameter_flags(&parameter(false));
        assert_eq!(continuous_flags, CLAP_PARAM_IS_AUTOMATABLE);

        let stepped_flags = parameter_flags(&parameter(true));
        assert_ne!(stepped_flags & CLAP_PARAM_IS_STEPPED, 0);
        assert_ne!(stepped_flags & CLAP_PARAM_IS_AUTOMATABLE, 0);
    }

    #[test]
    fn parameter_event_cast_requires_core_space_and_full_size() {
        let mut header = clap_sys::events::clap_event_header_t {
            size: std::mem::size_of::<clap_sys::events::clap_event_header_t>() as u32,
            time: 0,
            space_id: clap_sys::events::CLAP_CORE_EVENT_SPACE_ID,
            type_: CLAP_EVENT_PARAM_VALUE,
            flags: 0,
        };
        assert!(!unsafe { is_param_value_event(&header) });

        header.size = std::mem::size_of::<clap_event_param_value_t>() as u32;
        assert!(unsafe { is_param_value_event(&header) });

        header.space_id = 1;
        assert!(!unsafe { is_param_value_event(&header) });
    }
}
