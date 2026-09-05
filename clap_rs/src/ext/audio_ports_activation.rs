//! Audio Ports Activation Extension for clap_rs
//!
//! Lets the host activate and deactivate individual audio ports
//! (`clap.audio-ports-activation/2`). Deactivating an unused sidechain, for
//! example, tells the plugin it can skip that path entirely.

use crate::plugin::Plugin;
use crate::plugin_instance::{ffi_guard, instance_ptr};
use clap_sys::ext::audio_ports_activation::clap_plugin_audio_ports_activation_t;
use clap_sys::plugin::clap_plugin_t;

/// Validates the port index against the declared port list before forwarding,
/// so a plugin callback never sees an out-of-range port.
unsafe fn set_active_impl<P: Plugin>(
    plugin: *const clap_plugin_t,
    is_input: bool,
    port_index: u32,
    is_active: bool,
    sample_size: u32,
) -> bool {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &mut *instance_ptr };
    ffi_guard(false, || unsafe {
        let declared = instance
            .audio_ports_cache
            .iter()
            .filter(|port| port.is_input == is_input)
            .count();
        if port_index as usize >= declared {
            return false;
        }
        instance.controller_mut().set_audio_port_active(
            is_input,
            port_index,
            is_active,
            sample_size,
        )
    })
}

pub(crate) unsafe extern "C" fn activation_can_activate_while_processing<P: Plugin>(
    plugin: *const clap_plugin_t,
) -> bool {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &*instance_ptr };
    ffi_guard(false, || unsafe {
        instance.controller().can_activate_ports_while_processing()
    })
}

pub(crate) unsafe extern "C" fn activation_set_active<P: Plugin>(
    plugin: *const clap_plugin_t,
    is_input: bool,
    port_index: u32,
    is_active: bool,
    sample_size: u32,
) -> bool {
    unsafe { set_active_impl::<P>(plugin, is_input, port_index, is_active, sample_size) }
}

/// Create audio-ports-activation extension struct
pub(crate) fn create_audio_ports_activation_ext<P: Plugin>() -> clap_plugin_audio_ports_activation_t
{
    clap_plugin_audio_ports_activation_t {
        can_activate_while_processing: Some(activation_can_activate_while_processing::<P>),
        set_active: Some(activation_set_active::<P>),
    }
}

// ======= GUI Plugin Support =======
