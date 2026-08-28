//! Audio Ports Config Extension for clap_rs
//!
//! Lets the plugin publish a list of whole-plugin port layouts
//! (`clap.audio-ports-config`) that the host picks from by id — CLAP's way of
//! negotiating channel layouts. `clap.audio-ports-config-info/1` additionally
//! lets a host inspect the ports of a configuration it has not selected.

use crate::ext::audio_ports::{AudioPortInfo, port_type_for, write_cstr_to_array};
use crate::plugin::Plugin;
use crate::plugin_instance::{ffi_guard, instance_ptr};
use clap_sys::ext::audio_ports::{CLAP_AUDIO_PORT_IS_MAIN, clap_audio_port_info_t};
use clap_sys::ext::audio_ports_config::{
    clap_audio_ports_config_t, clap_plugin_audio_ports_config_info_t,
    clap_plugin_audio_ports_config_t,
};
use clap_sys::id::{CLAP_INVALID_ID, clap_id};
use clap_sys::plugin::clap_plugin_t;

/// One host-selectable port layout.
///
/// `ports` holds every port of the configuration, in both directions, using
/// the same shape as [`Plugin::audio_ports_config`]. The counts CLAP asks for
/// are derived from it, so a configuration cannot describe itself
/// inconsistently.
#[derive(Clone, Debug)]
pub struct AudioPortsConfig {
    pub id: u32,
    pub name: String,
    pub ports: Vec<AudioPortInfo>,
}

impl AudioPortsConfig {
    /// Ports of one direction, in declaration order.
    pub fn ports_in_direction(&self, is_input: bool) -> impl Iterator<Item = &AudioPortInfo> {
        self.ports.iter().filter(move |p| p.is_input == is_input)
    }

    /// The main port of one direction, if the configuration has one.
    fn main_port(&self, is_input: bool) -> Option<&AudioPortInfo> {
        self.ports_in_direction(is_input).find(|p| p.is_main)
    }
}

fn fill_config(config: &AudioPortsConfig, out: &mut clap_audio_ports_config_t) {
    out.id = config.id;
    write_cstr_to_array(&mut out.name, config.name.as_bytes());
    out.input_port_count = config.ports_in_direction(true).count() as u32;
    out.output_port_count = config.ports_in_direction(false).count() as u32;

    match config.main_port(true) {
        Some(port) => {
            out.has_main_input = true;
            out.main_input_channel_count = port.channel_count;
            out.main_input_port_type = port_type_for(port.channel_count);
        }
        None => {
            out.has_main_input = false;
            out.main_input_channel_count = 0;
            out.main_input_port_type = std::ptr::null();
        }
    }
    match config.main_port(false) {
        Some(port) => {
            out.has_main_output = true;
            out.main_output_channel_count = port.channel_count;
            out.main_output_port_type = port_type_for(port.channel_count);
        }
        None => {
            out.has_main_output = false;
            out.main_output_channel_count = 0;
            out.main_output_port_type = std::ptr::null();
        }
    }
}

fn fill_port_info(port: &AudioPortInfo, out: &mut clap_audio_port_info_t) {
    out.id = port.id;
    write_cstr_to_array(&mut out.name, port.name.as_bytes());
    out.flags = if port.is_main {
        CLAP_AUDIO_PORT_IS_MAIN
    } else {
        0
    };
    out.channel_count = port.channel_count;
    out.port_type = port_type_for(port.channel_count);
    out.in_place_pair = CLAP_INVALID_ID;
}

unsafe fn count_impl<P: Plugin>(plugin: *const clap_plugin_t) -> u32 {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return 0;
    };
    let instance = unsafe { &*instance_ptr };
    instance.audio_ports_configs_cache.len() as u32
}

unsafe fn get_impl<P: Plugin>(
    plugin: *const clap_plugin_t,
    index: u32,
    config: *mut clap_audio_ports_config_t,
) -> bool {
    if config.is_null() {
        return false;
    }
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &*instance_ptr };
    let Some(entry) = instance.audio_ports_configs_cache.get(index as usize) else {
        return false;
    };
    fill_config(entry, unsafe { &mut *config });
    true
}

/// Selects a configuration by id.
///
/// On success the cached port list is rebuilt from the plugin's new layout, so
/// a host that reads `clap.audio-ports` straight after selecting sees the
/// layout it just chose rather than the one from `init`.
unsafe fn select_impl<P: Plugin>(plugin: *const clap_plugin_t, config_id: clap_id) -> bool {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &mut *instance_ptr };
    ffi_guard(false, || unsafe {
        // An unknown id must be refused before the plugin is asked, so a
        // plugin never has to defend against ids it did not publish.
        if !instance
            .audio_ports_configs_cache
            .iter()
            .any(|entry| entry.id == config_id)
        {
            return false;
        }
        if !instance
            .controller_mut()
            .select_audio_ports_config(config_id)
        {
            return false;
        }
        instance.audio_ports_cache = instance.controller().audio_ports_config();
        // The audio-thread scratch buffers are sized from the port list, so a
        // layout switch has to resize them too — otherwise a mono-to-stereo
        // switch would process through buffers built for one channel. CLAP
        // requires the plugin to be deactivated for this call, so there is no
        // concurrent processing to race with.
        instance.resize_process_buffers();
        true
    })
}

unsafe fn current_config_impl<P: Plugin>(plugin: *const clap_plugin_t) -> clap_id {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return CLAP_INVALID_ID;
    };
    let instance = unsafe { &*instance_ptr };
    ffi_guard(CLAP_INVALID_ID, || unsafe {
        instance.controller().current_audio_ports_config_id()
    })
}

unsafe fn config_info_get_impl<P: Plugin>(
    plugin: *const clap_plugin_t,
    config_id: clap_id,
    port_index: u32,
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
    let Some(entry) = instance
        .audio_ports_configs_cache
        .iter()
        .find(|entry| entry.id == config_id)
    else {
        return false;
    };
    let Some(port) = entry.ports_in_direction(is_input).nth(port_index as usize) else {
        return false;
    };
    fill_port_info(port, unsafe { &mut *info });
    true
}

macro_rules! audio_ports_config_ext {
    ($bound:path, $count:ident, $get:ident, $select:ident, $current:ident, $info_get:ident,
     $make_config:ident, $make_info:ident) => {
        pub(crate) unsafe extern "C" fn $count<P: $bound>(plugin: *const clap_plugin_t) -> u32 {
            unsafe { count_impl::<P>(plugin) }
        }

        pub(crate) unsafe extern "C" fn $get<P: $bound>(
            plugin: *const clap_plugin_t,
            index: u32,
            config: *mut clap_audio_ports_config_t,
        ) -> bool {
            unsafe { get_impl::<P>(plugin, index, config) }
        }

        pub(crate) unsafe extern "C" fn $select<P: $bound>(
            plugin: *const clap_plugin_t,
            config_id: clap_id,
        ) -> bool {
            unsafe { select_impl::<P>(plugin, config_id) }
        }

        pub(crate) unsafe extern "C" fn $current<P: $bound>(
            plugin: *const clap_plugin_t,
        ) -> clap_id {
            unsafe { current_config_impl::<P>(plugin) }
        }

        pub(crate) unsafe extern "C" fn $info_get<P: $bound>(
            plugin: *const clap_plugin_t,
            config_id: clap_id,
            port_index: u32,
            is_input: bool,
            info: *mut clap_audio_port_info_t,
        ) -> bool {
            unsafe { config_info_get_impl::<P>(plugin, config_id, port_index, is_input, info) }
        }

        pub(crate) fn $make_config<P: $bound>() -> clap_plugin_audio_ports_config_t {
            clap_plugin_audio_ports_config_t {
                count: Some($count::<P>),
                get: Some($get::<P>),
                select: Some($select::<P>),
            }
        }

        pub(crate) fn $make_info<P: $bound>() -> clap_plugin_audio_ports_config_info_t {
            clap_plugin_audio_ports_config_info_t {
                current_config: Some($current::<P>),
                get: Some($info_get::<P>),
            }
        }
    };
}

audio_ports_config_ext!(
    Plugin,
    config_count,
    config_get,
    config_select,
    config_current,
    config_info_get,
    create_audio_ports_config_ext,
    create_audio_ports_config_info_ext
);

// ======= GUI Plugin Support =======

use crate::ext::gui::GuiHandler;

trait PluginWithGui: Plugin + GuiHandler {}
impl<T: Plugin + GuiHandler> PluginWithGui for T {}

audio_ports_config_ext!(
    PluginWithGui,
    config_count_gui,
    config_get_gui,
    config_select_gui,
    config_current_gui,
    config_info_get_gui,
    create_audio_ports_config_ext_gui,
    create_audio_ports_config_info_ext_gui
);
