//! Note Ports Extension for clap_rs

use crate::plugin::Plugin;
use crate::plugin_instance::instance_ptr;
use clap_sys::ext::note_ports::{
    CLAP_NOTE_DIALECT_MIDI, clap_note_port_info_t, clap_plugin_note_ports_t,
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

fn write_note_port_info(info: &mut clap_note_port_info_t, port: &NotePortInfo) {
    info.id = port.id;
    info.supported_dialects = CLAP_NOTE_DIALECT_MIDI;
    info.preferred_dialect = CLAP_NOTE_DIALECT_MIDI;
    write_cstr_to_array(&mut info.name, port.name.as_bytes());
}

pub(crate) unsafe extern "C" fn note_ports_count<P: Plugin>(
    plugin: *const clap_plugin_t,
    is_input: bool,
) -> u32 {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return 0;
    };
    let instance = unsafe { &*instance_ptr };
    instance
        .note_ports_cache
        .iter()
        .filter(|p| p.is_input == is_input)
        .count() as u32
}

pub(crate) unsafe extern "C" fn note_ports_get<P: Plugin>(
    plugin: *const clap_plugin_t,
    index: u32,
    is_input: bool,
    info: *mut clap_note_port_info_t,
) -> bool {
    if info.is_null() {
        return false;
    }
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &*instance_ptr };
    let ports: Vec<_> = instance
        .note_ports_cache
        .iter()
        .filter(|p| p.is_input == is_input)
        .collect();
    if (index as usize) >= ports.len() {
        return false;
    }
    let port = &ports[index as usize];
    let info = unsafe { &mut *info };
    write_note_port_info(info, port);
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

use crate::ext::gui::GuiHandler;

pub(crate) unsafe extern "C" fn note_ports_count_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    is_input: bool,
) -> u32 {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return 0;
    };
    let instance = unsafe { &*instance_ptr };
    instance
        .note_ports_cache
        .iter()
        .filter(|p| p.is_input == is_input)
        .count() as u32
}

pub(crate) unsafe extern "C" fn note_ports_get_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    index: u32,
    is_input: bool,
    info: *mut clap_note_port_info_t,
) -> bool {
    if info.is_null() {
        return false;
    }
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &*instance_ptr };
    let ports: Vec<_> = instance
        .note_ports_cache
        .iter()
        .filter(|p| p.is_input == is_input)
        .collect();
    if (index as usize) >= ports.len() {
        return false;
    }
    let port = &ports[index as usize];
    let info = unsafe { &mut *info };
    write_note_port_info(info, port);
    true
}

pub(crate) fn create_note_ports_ext_gui<P: Plugin + GuiHandler>() -> clap_plugin_note_ports_t {
    clap_plugin_note_ports_t {
        count: Some(note_ports_count_gui::<P>),
        get: Some(note_ports_get_gui::<P>),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap_sys::string_sizes::CLAP_NAME_SIZE;
    use std::ffi::CStr;

    #[test]
    fn note_ports_advertise_only_the_midi_dialect() {
        let port = NotePortInfo {
            id: 42,
            name: "MIDI Input".to_owned(),
            is_input: true,
        };
        let mut info = clap_note_port_info_t {
            id: 0,
            supported_dialects: u32::MAX,
            preferred_dialect: u32::MAX,
            name: [0; CLAP_NAME_SIZE],
        };

        write_note_port_info(&mut info, &port);

        assert_eq!(info.id, 42);
        assert_eq!(info.supported_dialects, CLAP_NOTE_DIALECT_MIDI);
        assert_eq!(info.preferred_dialect, CLAP_NOTE_DIALECT_MIDI);
        assert_eq!(
            unsafe { CStr::from_ptr(info.name.as_ptr()) }.to_bytes(),
            b"MIDI Input"
        );
    }
}
