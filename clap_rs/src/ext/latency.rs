//! Latency Extension for clap_rs
//!
//! Report processing latency to the host.

use crate::plugin::Plugin;
use crate::plugin_instance::PluginInstance;
use clap_sys::ext::latency::{CLAP_EXT_LATENCY, clap_plugin_latency_t};
use clap_sys::plugin::clap_plugin_t;

pub(crate) unsafe extern "C" fn latency_get<P: Plugin>(plugin: *const clap_plugin_t) -> u32 {
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstance<P>) };
    unsafe { instance.controller() }.latency()
}

/// Create latency extension struct
pub(crate) fn create_latency_ext<P: Plugin>() -> clap_plugin_latency_t {
    clap_plugin_latency_t {
        get: Some(latency_get::<P>),
    }
}

// ======= GUI Plugin Support =======

use crate::ext::gui::GuiHandler;
use crate::plugin_instance::PluginInstanceWithGui;

pub(crate) unsafe extern "C" fn latency_get_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
) -> u32 {
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstanceWithGui<P>) };
    unsafe { instance.controller() }.latency()
}

pub(crate) fn create_latency_ext_gui<P: Plugin + GuiHandler>() -> clap_plugin_latency_t {
    clap_plugin_latency_t {
        get: Some(latency_get_gui::<P>),
    }
}
