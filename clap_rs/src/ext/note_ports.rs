//! Note Ports Extension for clap_rs

use crate::plugin::Plugin;
use crate::plugin_instance::PluginInstance;
use clap_sys::ext::note_ports::{
    clap_plugin_note_ports_t, clap_note_port_info_t,
    CLAP_NOTE_DIALECT_CLAP, CLAP_NOTE_DIALECT_MIDI, CLAP_EXT_NOTE_PORTS
};
use clap_sys::plugin::clap_plugin_t;
use std::ffi::c_char;

/// Note port configuration info
#[derive(Clone, Debug)]
pub struct NotePortInfo {
    pub id: u32,
    pub name: String,
    pub is_input: bool,
}

fn write_cstr_to_array(dst: &mut [c_char], bytes: &[u8]) {
    dst.fill(0);
    let max = dst.len().saturating_sub(1);
    let len = bytes.len().min(max);
    for i in 0..len {
        dst[i] = bytes[i] as c_char;
    }
}

pub(crate) unsafe extern "C" fn note_ports_count<P: Plugin>(
    plugin: *const clap_plugin_t,
    is_input: bool
) -> u32 {
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstance<P>) };
    instance.note_ports_cache.iter().filter(|p| p.is_input == is_input).count() as u32
}

pub(crate) unsafe extern "C" fn note_ports_get<P: Plugin>(
    plugin: *const clap_plugin_t,
    index: u32,
    is_input: bool,
    info: *mut clap_note_port_info_t,
) -> bool {
    if info.is_null() { return false; }
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstance<P>) };
    let ports: Vec<_> = instance.note_ports_cache.iter().filter(|p| p.is_input == is_input).collect();
    if (index as usize) >= ports.len() { return false; }
    let port = &ports[index as usize];
    let info = unsafe { &mut *info };
    info.id = port.id;
    info.supported_dialects = CLAP_NOTE_DIALECT_CLAP | CLAP_NOTE_DIALECT_MIDI;
    info.preferred_dialect = CLAP_NOTE_DIALECT_CLAP;
    write_cstr_to_array(&mut info.name, port.name.as_bytes());
    true
}

/// Create note ports extension struct
pub(crate) fn create_note_ports_ext<P: Plugin>() -> clap_plugin_note_ports_t {
    clap_plugin_note_ports_t {
        count: Some(note_ports_count::<P>),
        get: Some(note_ports_get::<P>),
    }
}

// ======= GUI Plugin Support =======

use crate::plugin_instance::PluginInstanceWithGui;
use crate::ext::gui::GuiHandler;

pub(crate) unsafe extern "C" fn note_ports_count_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    is_input: bool
) -> u32 {
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstanceWithGui<P>) };
    instance.note_ports_cache.iter().filter(|p| p.is_input == is_input).count() as u32
}

pub(crate) unsafe extern "C" fn note_ports_get_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    index: u32,
    is_input: bool,
    info: *mut clap_note_port_info_t,
) -> bool {
    if info.is_null() { return false; }
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstanceWithGui<P>) };
    let ports: Vec<_> = instance.note_ports_cache.iter().filter(|p| p.is_input == is_input).collect();
    if (index as usize) >= ports.len() { return false; }
    let port = &ports[index as usize];
    let info = unsafe { &mut *info };
    info.id = port.id;
    info.supported_dialects = CLAP_NOTE_DIALECT_CLAP | CLAP_NOTE_DIALECT_MIDI;
    info.preferred_dialect = CLAP_NOTE_DIALECT_CLAP;
    write_cstr_to_array(&mut info.name, port.name.as_bytes());
    true
}

pub(crate) fn create_note_ports_ext_gui<P: Plugin + GuiHandler>() -> clap_plugin_note_ports_t {
    clap_plugin_note_ports_t {
        count: Some(note_ports_count_gui::<P>),
        get: Some(note_ports_get_gui::<P>),
    }
}
