//! Voice Info Extension for clap_rs
//!
//! Report voice allocation info for polyphonic synths.

use crate::plugin::Plugin;
use crate::plugin_instance::{ffi_guard, instance_ptr};
use clap_sys::ext::voice_info::{
    CLAP_VOICE_INFO_SUPPORTS_OVERLAPPING_NOTES, clap_plugin_voice_info_t, clap_voice_info_t,
};
use clap_sys::plugin::clap_plugin_t;

/// Voice allocation info
#[derive(Debug, Clone, Copy)]
pub struct VoiceInfo {
    /// Currently active voices
    pub voice_count: u32,
    /// Maximum voices supported
    pub voice_capacity: u32,
    /// Whether overlapping notes on same key are supported
    pub supports_overlapping_notes: bool,
}

impl Default for VoiceInfo {
    fn default() -> Self {
        Self {
            voice_count: 0,
            voice_capacity: 16,
            supports_overlapping_notes: true,
        }
    }
}

pub(crate) unsafe extern "C" fn voice_info_get<P: Plugin>(
    plugin: *const clap_plugin_t,
    info: *mut clap_voice_info_t,
) -> bool {
    if info.is_null() {
        return false;
    }
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &*instance_ptr };
    let voice_info = ffi_guard(None, || unsafe { instance.controller().voice_info() });
    if let Some(vi) = voice_info {
        let out = unsafe { &mut *info };
        out.voice_count = vi.voice_count;
        out.voice_capacity = vi.voice_capacity;
        out.flags = if vi.supports_overlapping_notes {
            CLAP_VOICE_INFO_SUPPORTS_OVERLAPPING_NOTES
        } else {
            0
        };
        true
    } else {
        false
    }
}

/// Create voice info extension struct
pub(crate) fn create_voice_info_ext<P: Plugin>() -> clap_plugin_voice_info_t {
    clap_plugin_voice_info_t {
        get: Some(voice_info_get::<P>),
    }
}

// ======= GUI Plugin Support =======
