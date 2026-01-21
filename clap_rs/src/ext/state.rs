//! State Extension for clap_rs

use crate::plugin::Plugin;
use crate::plugin_instance::PluginInstance;
use clap_sys::ext::state::{clap_plugin_state_t, CLAP_EXT_STATE};
use clap_sys::stream::{clap_istream_t, clap_ostream_t};
use clap_sys::plugin::clap_plugin_t;

pub(crate) unsafe extern "C" fn state_save<P: Plugin>(
    plugin: *const clap_plugin_t,
    stream: *const clap_ostream_t,
) -> bool {
    if plugin.is_null() || stream.is_null() { return false; }
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstance<P>) };
    
    // Save each parameter value as f64 (8 bytes, little-endian)
    for param in &instance.params_cache {
        let value = instance.plugin.get_parameter(param.id);
        let bytes = value.to_le_bytes();
        if !stream_write_all(stream, bytes.as_ptr(), bytes.len()) {
            return false;
        }
    }
    true
}

pub(crate) unsafe extern "C" fn state_load<P: Plugin>(
    plugin: *const clap_plugin_t,
    stream: *const clap_istream_t,
) -> bool {
    if plugin.is_null() || stream.is_null() { return false; }
    let instance = unsafe { &mut *((*plugin).plugin_data as *mut PluginInstance<P>) };
    
    // Load each parameter value
    for param in &instance.params_cache.clone() {
        let mut bytes = [0u8; 8];
        if !stream_read_exact(stream, bytes.as_mut_ptr(), bytes.len()) {
            return false;
        }
        let value = f64::from_le_bytes(bytes);
        instance.plugin.set_parameter(param.id, value);
    }
    true
}

fn stream_write_all(stream: *const clap_ostream_t, mut buffer: *const u8, mut size: usize) -> bool {
    if stream.is_null() { return false; }
    let write_fn = unsafe { (*stream).write };
    let Some(write_fn) = write_fn else { return false; };
    while size > 0 {
        let written = unsafe { write_fn(stream, buffer as *const _, size as u64) };
        if written <= 0 { return false; }
        let written = written as usize;
        buffer = unsafe { buffer.add(written) };
        size -= written;
    }
    true
}

fn stream_read_exact(stream: *const clap_istream_t, mut buffer: *mut u8, mut size: usize) -> bool {
    if stream.is_null() { return false; }
    let read_fn = unsafe { (*stream).read };
    let Some(read_fn) = read_fn else { return false; };
    while size > 0 {
        let read = unsafe { read_fn(stream, buffer as *mut _, size as u64) };
        if read <= 0 { return false; }
        let read = read as usize;
        buffer = unsafe { buffer.add(read) };
        size -= read;
    }
    true
}

/// Create state extension struct
pub(crate) fn create_state_ext<P: Plugin>() -> clap_plugin_state_t {
    clap_plugin_state_t {
        save: Some(state_save::<P>),
        load: Some(state_load::<P>),
    }
}

// ======= GUI Plugin Support =======

use crate::plugin_instance::PluginInstanceWithGui;
use crate::ext::gui::GuiHandler;

pub(crate) unsafe extern "C" fn state_save_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    stream: *const clap_ostream_t,
) -> bool {
    if plugin.is_null() || stream.is_null() { return false; }
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstanceWithGui<P>) };
    
    for param in &instance.params_cache {
        let value = instance.plugin.get_parameter(param.id);
        let bytes = value.to_le_bytes();
        if !stream_write_all(stream, bytes.as_ptr(), bytes.len()) {
            return false;
        }
    }
    true
}

pub(crate) unsafe extern "C" fn state_load_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    stream: *const clap_istream_t,
) -> bool {
    if plugin.is_null() || stream.is_null() { return false; }
    let instance = unsafe { &mut *((*plugin).plugin_data as *mut PluginInstanceWithGui<P>) };
    
    for param in &instance.params_cache.clone() {
        let mut bytes = [0u8; 8];
        if !stream_read_exact(stream, bytes.as_mut_ptr(), bytes.len()) {
            return false;
        }
        let value = f64::from_le_bytes(bytes);
        instance.plugin.set_parameter(param.id, value);
    }
    true
}

pub(crate) fn create_state_ext_gui<P: Plugin + GuiHandler>() -> clap_plugin_state_t {
    clap_plugin_state_t {
        save: Some(state_save_gui::<P>),
        load: Some(state_load_gui::<P>),
    }
}
