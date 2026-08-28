//! Tail Extension for clap_rs
//!
//! Report effect tail length (reverb, delay, etc).

use crate::plugin::Plugin;
use crate::plugin_instance::instance_ptr;
use clap_sys::ext::tail::clap_plugin_tail_t;
use clap_sys::plugin::clap_plugin_t;

pub(crate) unsafe extern "C" fn tail_get<P: Plugin>(plugin: *const clap_plugin_t) -> u32 {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return 0;
    };
    let instance = unsafe { &*instance_ptr };
    instance.cached_tail()
}

/// Create tail extension struct
pub(crate) fn create_tail_ext<P: Plugin>() -> clap_plugin_tail_t {
    clap_plugin_tail_t {
        get: Some(tail_get::<P>),
    }
}

// ======= GUI Plugin Support =======

use crate::ext::gui::GuiHandler;

pub(crate) unsafe extern "C" fn tail_get_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
) -> u32 {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return 0;
    };
    let instance = unsafe { &*instance_ptr };
    instance.cached_tail()
}

pub(crate) fn create_tail_ext_gui<P: Plugin + GuiHandler>() -> clap_plugin_tail_t {
    clap_plugin_tail_t {
        get: Some(tail_get_gui::<P>),
    }
}
