//! Render Extension for clap_rs
//!
//! Switch between realtime and offline rendering modes.

use crate::plugin::Plugin;
use crate::plugin_instance::{ffi_guard, instance_ptr};
use clap_sys::ext::render::{
    CLAP_RENDER_OFFLINE, CLAP_RENDER_REALTIME, clap_plugin_render_mode, clap_plugin_render_t,
};
use clap_sys::plugin::clap_plugin_t;

/// Rendering mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    /// Realtime rendering (normal playback)
    Realtime,
    /// Offline rendering (bounce, export)
    Offline,
}

impl From<clap_plugin_render_mode> for RenderMode {
    fn from(mode: clap_plugin_render_mode) -> Self {
        match mode {
            CLAP_RENDER_OFFLINE => RenderMode::Offline,
            _ => RenderMode::Realtime,
        }
    }
}

impl From<RenderMode> for clap_plugin_render_mode {
    fn from(mode: RenderMode) -> Self {
        match mode {
            RenderMode::Realtime => CLAP_RENDER_REALTIME,
            RenderMode::Offline => CLAP_RENDER_OFFLINE,
        }
    }
}

pub(crate) unsafe extern "C" fn render_has_hard_realtime_requirement<P: Plugin>(
    plugin: *const clap_plugin_t,
) -> bool {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &*instance_ptr };
    ffi_guard(false, || unsafe {
        instance.controller().has_hard_realtime_requirement()
    })
}

pub(crate) unsafe extern "C" fn render_set<P: Plugin>(
    plugin: *const clap_plugin_t,
    mode: clap_plugin_render_mode,
) -> bool {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &*instance_ptr };
    let updated = ffi_guard(false, || unsafe {
        instance
            .controller_mut()
            .set_render_mode(RenderMode::from(mode))
    });
    if updated {
        return ffi_guard(false, || unsafe {
            instance.refresh_tail_cache();
            true
        });
    }
    updated
}

/// Create render extension struct
pub(crate) fn create_render_ext<P: Plugin>() -> clap_plugin_render_t {
    clap_plugin_render_t {
        has_hard_realtime_requirement: Some(render_has_hard_realtime_requirement::<P>),
        set: Some(render_set::<P>),
    }
}

// ======= GUI Plugin Support =======

use crate::ext::gui::GuiHandler;

pub(crate) unsafe extern "C" fn render_has_hard_realtime_requirement_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
) -> bool {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &*instance_ptr };
    ffi_guard(false, || unsafe {
        instance.controller().has_hard_realtime_requirement()
    })
}

pub(crate) unsafe extern "C" fn render_set_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    mode: clap_plugin_render_mode,
) -> bool {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &*instance_ptr };
    let updated = ffi_guard(false, || unsafe {
        instance
            .controller_mut()
            .set_render_mode(RenderMode::from(mode))
    });
    if updated {
        return ffi_guard(false, || unsafe {
            instance.refresh_tail_cache();
            true
        });
    }
    updated
}

pub(crate) fn create_render_ext_gui<P: Plugin + GuiHandler>() -> clap_plugin_render_t {
    clap_plugin_render_t {
        has_hard_realtime_requirement: Some(render_has_hard_realtime_requirement_gui::<P>),
        set: Some(render_set_gui::<P>),
    }
}
