//! Preset Load Extension for clap_rs.
//!
//! Lets the host ask the plugin to load a preset by location
//! (`clap.preset-load/2`). The plugin reads the file itself; the host only
//! names it.

use crate::plugin::Plugin;
use crate::plugin_instance::{ffi_guard, instance_ptr};
use clap_sys::ext::preset_load::clap_plugin_preset_load_t;
use clap_sys::plugin::clap_plugin_t;
use std::ffi::{CStr, c_char};

/// Where the host says a preset lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetLocation<'a> {
    /// A file on disk, with an optional key selecting one preset inside it.
    File { path: &'a str, key: Option<&'a str> },
    /// A preset built into the plugin binary.
    Internal { key: Option<&'a str> },
}

/// Borrows a host C string as UTF-8, or `None` when it is null or malformed.
///
/// A non-UTF-8 path is refused rather than lossily converted: silently loading
/// a *different* file than the host named would be worse than failing.
unsafe fn borrow_str<'a>(raw: *const c_char) -> Option<&'a str> {
    if raw.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(raw) }.to_str().ok()
}

unsafe fn from_location_impl<P: Plugin>(
    plugin: *const clap_plugin_t,
    location_kind: u32,
    location: *const c_char,
    load_key: *const c_char,
) -> bool {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &mut *instance_ptr };
    ffi_guard(false, || unsafe {
        let key = borrow_str(load_key);
        // `load_key` may legitimately be null (a file holding one preset), but
        // a null path for a file location is malformed.
        let location = match location_kind {
            clap_sys::factory::preset_discovery::CLAP_PRESET_DISCOVERY_LOCATION_FILE => {
                let Some(path) = borrow_str(location) else {
                    return false;
                };
                PresetLocation::File { path, key }
            }
            clap_sys::factory::preset_discovery::CLAP_PRESET_DISCOVERY_LOCATION_PLUGIN => {
                PresetLocation::Internal { key }
            }
            // An unknown location kind is refused rather than guessed at.
            _ => return false,
        };
        instance.controller_mut().load_preset(location)
    })
}

macro_rules! preset_load_ext {
    ($bound:path, $from_location:ident, $make:ident) => {
        pub(crate) unsafe extern "C" fn $from_location<P: $bound>(
            plugin: *const clap_plugin_t,
            location_kind: u32,
            location: *const c_char,
            load_key: *const c_char,
        ) -> bool {
            unsafe { from_location_impl::<P>(plugin, location_kind, location, load_key) }
        }

        pub(crate) fn $make<P: $bound>() -> clap_plugin_preset_load_t {
            clap_plugin_preset_load_t {
                from_location: Some($from_location::<P>),
            }
        }
    };
}

preset_load_ext!(Plugin, preset_from_location, create_preset_load_ext);

// ======= GUI Plugin Support =======

use crate::ext::gui::GuiHandler;

trait PluginWithGui: Plugin + GuiHandler {}
impl<T: Plugin + GuiHandler> PluginWithGui for T {}

preset_load_ext!(
    PluginWithGui,
    preset_from_location_gui,
    create_preset_load_ext_gui
);
