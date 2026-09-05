//! Audio Ports Extension for clap_rs

use crate::plugin::Plugin;
use crate::plugin_instance::instance_ptr;
use clap_sys::ext::audio_ports::{
    CLAP_AUDIO_PORT_IS_MAIN, CLAP_PORT_MONO, CLAP_PORT_STEREO, clap_audio_port_info_t,
    clap_plugin_audio_ports_t,
};
use clap_sys::id::CLAP_INVALID_ID;
use clap_sys::plugin::clap_plugin_t;
use std::ffi::c_char;

/// Audio port configuration info
#[derive(Clone, Debug)]
pub struct AudioPortInfo {
    pub id: u32,
    pub name: String,
    pub channel_count: u32,
    pub is_main: bool,
    pub is_input: bool,
}

/// The CLAP port type for a channel count, or null when the count has no
/// standard type. Reporting `stereo` for a one-channel port would misdescribe
/// it to the host, which matters as soon as a plugin offers a mono layout.
pub(crate) fn port_type_for(channel_count: u32) -> *const c_char {
    match channel_count {
        1 => CLAP_PORT_MONO.as_ptr() as *const c_char,
        2 => CLAP_PORT_STEREO.as_ptr() as *const c_char,
        _ => std::ptr::null(),
    }
}

pub(crate) fn write_cstr_to_array(dst: &mut [c_char], bytes: &[u8]) {
    dst.fill(0);
    let max = dst.len().saturating_sub(1);
    let len = bytes.len().min(max);
    for i in 0..len {
        dst[i] = bytes[i] as c_char;
    }
}

pub(crate) unsafe extern "C" fn audio_ports_count<P: Plugin>(
    plugin: *const clap_plugin_t,
    is_input: bool,
) -> u32 {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return 0;
    };
    let instance = unsafe { &*instance_ptr };
    instance
        .audio_ports_cache
        .iter()
        .filter(|p| p.is_input == is_input)
        .count() as u32
}

pub(crate) unsafe extern "C" fn audio_ports_get<P: Plugin>(
    plugin: *const clap_plugin_t,
    index: u32,
    is_input: bool,
    info: *mut clap_audio_port_info_t,
) -> bool {
    if info.is_null() {
        return false;
    }
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &*instance_ptr };
    let ports: Vec<_> = instance
        .audio_ports_cache
        .iter()
        .filter(|p| p.is_input == is_input)
        .collect();
    if (index as usize) >= ports.len() {
        return false;
    }
    let port = &ports[index as usize];
    let info = unsafe { &mut *info };
    info.id = port.id;
    write_cstr_to_array(&mut info.name, port.name.as_bytes());
    info.flags = if port.is_main {
        CLAP_AUDIO_PORT_IS_MAIN
    } else {
        0
    };
    info.channel_count = port.channel_count;
    info.port_type = port_type_for(port.channel_count);
    info.in_place_pair = CLAP_INVALID_ID;
    true
}

/// Create audio ports extension struct
pub(crate) fn create_audio_ports_ext<P: Plugin>() -> clap_plugin_audio_ports_t {
    clap_plugin_audio_ports_t {
        count: Some(audio_ports_count::<P>),
        get: Some(audio_ports_get::<P>),
    }
}

// ======= GUI Plugin Support =======
