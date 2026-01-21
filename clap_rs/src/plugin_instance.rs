//! Plugin instance management for clap_rs
//! 
//! This module provides the PluginInstance wrapper and lifecycle callbacks.

use crate::plugin::{Plugin, HostHandle};
use crate::process::ProcessContext;
use crate::ext::audio_ports::{AudioPortInfo, create_audio_ports_ext, create_audio_ports_ext_gui};
use crate::ext::note_ports::{NotePortInfo, create_note_ports_ext, create_note_ports_ext_gui};
use crate::ext::params::{ParameterInfo, create_params_ext, create_params_ext_gui, apply_param_events};
use crate::ext::state::{create_state_ext, create_state_ext_gui};
use crate::ext::latency::{create_latency_ext, create_latency_ext_gui};
use crate::ext::tail::{create_tail_ext, create_tail_ext_gui};
use crate::ext::voice_info::{create_voice_info_ext, create_voice_info_ext_gui};
use crate::ext::render::{create_render_ext, create_render_ext_gui};

use clap_sys::plugin::clap_plugin_t;
use clap_sys::process::{clap_process_t, clap_process_status};
use clap_sys::ext::audio_ports::{clap_plugin_audio_ports_t, CLAP_EXT_AUDIO_PORTS};
use clap_sys::ext::note_ports::{clap_plugin_note_ports_t, CLAP_EXT_NOTE_PORTS};
use clap_sys::ext::params::{clap_plugin_params_t, CLAP_EXT_PARAMS};
use clap_sys::ext::state::{clap_plugin_state_t, CLAP_EXT_STATE};
use clap_sys::ext::latency::{clap_plugin_latency_t, CLAP_EXT_LATENCY};
use clap_sys::ext::tail::{clap_plugin_tail_t, CLAP_EXT_TAIL};
use clap_sys::ext::voice_info::{clap_plugin_voice_info_t, CLAP_EXT_VOICE_INFO};
use clap_sys::ext::render::{clap_plugin_render_t, CLAP_EXT_RENDER};
use std::ffi::{c_void, c_char, CStr};
use std::ptr;

/// Plugin instance wrapper holding plugin data and extension caches
pub struct PluginInstance<P: Plugin> {
    pub plugin: P,
    // Caches
    pub params_cache: Vec<ParameterInfo>,
    pub audio_ports_cache: Vec<AudioPortInfo>,
    pub note_ports_cache: Vec<NotePortInfo>,
    // Extension struct storage (leaked, lives for plugin lifetime)
    pub audio_ports_ext: Option<*const clap_plugin_audio_ports_t>,
    pub note_ports_ext: Option<*const clap_plugin_note_ports_t>,
    pub params_ext: Option<*const clap_plugin_params_t>,
    pub state_ext: Option<*const clap_plugin_state_t>,
    pub latency_ext: Option<*const clap_plugin_latency_t>,
    pub tail_ext: Option<*const clap_plugin_tail_t>,
    pub voice_info_ext: Option<*const clap_plugin_voice_info_t>,
    pub render_ext: Option<*const clap_plugin_render_t>,
}

impl<P: Plugin> PluginInstance<P> {
    pub fn new(host: HostHandle) -> Self {
        let plugin = P::new(host);
        let params_cache = plugin.declare_parameters();
        let audio_ports_cache = plugin.audio_ports_config();
        let note_ports_cache = plugin.note_ports_config();
        Self {
            plugin,
            params_cache,
            audio_ports_cache,
            note_ports_cache,
            audio_ports_ext: None,
            note_ports_ext: None,
            params_ext: None,
            state_ext: None,
            latency_ext: None,
            tail_ext: None,
            voice_info_ext: None,
            render_ext: None,
        }
    }
}

// ======= LIFECYCLE CALLBACKS =======

pub unsafe extern "C" fn plugin_init<P: Plugin>(plugin: *const clap_plugin_t) -> bool {
    let instance = unsafe { &mut *((*plugin).plugin_data as *mut PluginInstance<P>) };
    
    // Initialize extension structs
    if !instance.audio_ports_cache.is_empty() {
        let ext = Box::new(create_audio_ports_ext::<P>());
        instance.audio_ports_ext = Some(Box::into_raw(ext));
    }
    
    if !instance.note_ports_cache.is_empty() {
        let ext = Box::new(create_note_ports_ext::<P>());
        instance.note_ports_ext = Some(Box::into_raw(ext));
    }
    
    if !instance.params_cache.is_empty() {
        let ext = Box::new(create_params_ext::<P>());
        instance.params_ext = Some(Box::into_raw(ext));
    }
    
    // Always add state extension
    let state_ext = Box::new(create_state_ext::<P>());
    instance.state_ext = Some(Box::into_raw(state_ext));
    
    // Always add latency extension
    let latency_ext = Box::new(create_latency_ext::<P>());
    instance.latency_ext = Some(Box::into_raw(latency_ext));
    
    // Always add tail extension
    let tail_ext = Box::new(create_tail_ext::<P>());
    instance.tail_ext = Some(Box::into_raw(tail_ext));
    
    // Add voice_info only if plugin provides it
    if instance.plugin.voice_info().is_some() {
        let ext = Box::new(create_voice_info_ext::<P>());
        instance.voice_info_ext = Some(Box::into_raw(ext));
    }
    
    // Always add render extension
    let render_ext = Box::new(create_render_ext::<P>());
    instance.render_ext = Some(Box::into_raw(render_ext));
    
    instance.plugin.init()
}

pub unsafe extern "C" fn plugin_destroy<P: Plugin>(plugin: *const clap_plugin_t) {
    let plugin = unsafe { &mut *(plugin as *mut clap_plugin_t) };
    if !plugin.plugin_data.is_null() {
        let instance = unsafe { Box::from_raw(plugin.plugin_data as *mut PluginInstance<P>) };
        // Clean up leaked extension structs
        if let Some(ptr) = instance.audio_ports_ext {
            unsafe { let _ = Box::from_raw(ptr as *mut clap_plugin_audio_ports_t); }
        }
        if let Some(ptr) = instance.note_ports_ext {
            unsafe { let _ = Box::from_raw(ptr as *mut clap_plugin_note_ports_t); }
        }
        if let Some(ptr) = instance.params_ext {
            unsafe { let _ = Box::from_raw(ptr as *mut clap_plugin_params_t); }
        }
        if let Some(ptr) = instance.state_ext {
            unsafe { let _ = Box::from_raw(ptr as *mut clap_plugin_state_t); }
        }
        if let Some(ptr) = instance.latency_ext {
            unsafe { let _ = Box::from_raw(ptr as *mut clap_plugin_latency_t); }
        }
        if let Some(ptr) = instance.tail_ext {
            unsafe { let _ = Box::from_raw(ptr as *mut clap_plugin_tail_t); }
        }
        if let Some(ptr) = instance.voice_info_ext {
            unsafe { let _ = Box::from_raw(ptr as *mut clap_plugin_voice_info_t); }
        }
        if let Some(ptr) = instance.render_ext {
            unsafe { let _ = Box::from_raw(ptr as *mut clap_plugin_render_t); }
        }
        // instance drops here, cleaning up P
    }
}

pub unsafe extern "C" fn plugin_activate<P: Plugin>(
    plugin: *const clap_plugin_t,
    sample_rate: f64,
    min_frames: u32,
    max_frames: u32,
) -> bool {
    let instance = unsafe { &mut *((*plugin).plugin_data as *mut PluginInstance<P>) };
    instance.plugin.activate(sample_rate, min_frames, max_frames)
}

pub unsafe extern "C" fn plugin_deactivate<P: Plugin>(plugin: *const clap_plugin_t) {
    let instance = unsafe { &mut *((*plugin).plugin_data as *mut PluginInstance<P>) };
    instance.plugin.deactivate();
}

pub unsafe extern "C" fn plugin_start_processing<P: Plugin>(_plugin: *const clap_plugin_t) -> bool {
    true
}

pub unsafe extern "C" fn plugin_stop_processing<P: Plugin>(_plugin: *const clap_plugin_t) {}

pub unsafe extern "C" fn plugin_reset<P: Plugin>(plugin: *const clap_plugin_t) {
    let instance = unsafe { &mut *((*plugin).plugin_data as *mut PluginInstance<P>) };
    instance.plugin.reset();
}

pub unsafe extern "C" fn plugin_process<P: Plugin>(
    plugin: *const clap_plugin_t,
    process: *const clap_process_t,
) -> clap_process_status {
    let instance = unsafe { &mut *((*plugin).plugin_data as *mut PluginInstance<P>) };
    let process_ref = unsafe { &*process };
    
    // Apply param events BEFORE processing audio
    unsafe { apply_param_events(instance, process_ref.in_events); }
    
    let context = unsafe { ProcessContext::from_raw(process) };
    instance.plugin.process(context)
}

// ======= GET EXTENSION =======

pub unsafe extern "C" fn plugin_get_extension<P: Plugin>(
    plugin: *const clap_plugin_t,
    id: *const c_char,
) -> *const c_void {
    if id.is_null() { return ptr::null(); }
    let id_cstr = unsafe { CStr::from_ptr(id) };
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstance<P>) };
    
    if id_cstr.to_bytes_with_nul() == CLAP_EXT_AUDIO_PORTS.as_bytes() {
        if let Some(ptr) = instance.audio_ports_ext {
            return ptr as *const c_void;
        }
    }

    if id_cstr.to_bytes_with_nul() == CLAP_EXT_NOTE_PORTS.as_bytes() {
        if let Some(ptr) = instance.note_ports_ext {
            return ptr as *const c_void;
        }
    }

    if id_cstr.to_bytes_with_nul() == CLAP_EXT_PARAMS.as_bytes() {
        if let Some(ptr) = instance.params_ext {
            return ptr as *const c_void;
        }
    }

    if id_cstr.to_bytes_with_nul() == CLAP_EXT_STATE.as_bytes() {
        if let Some(ptr) = instance.state_ext {
            return ptr as *const c_void;
        }
    }

    if id_cstr.to_bytes_with_nul() == CLAP_EXT_LATENCY.as_bytes() {
        if let Some(ptr) = instance.latency_ext {
            return ptr as *const c_void;
        }
    }

    if id_cstr.to_bytes_with_nul() == CLAP_EXT_TAIL.as_bytes() {
        if let Some(ptr) = instance.tail_ext {
            return ptr as *const c_void;
        }
    }

    if id_cstr.to_bytes_with_nul() == CLAP_EXT_VOICE_INFO.as_bytes() {
        if let Some(ptr) = instance.voice_info_ext {
            return ptr as *const c_void;
        }
    }

    if id_cstr.to_bytes_with_nul() == CLAP_EXT_RENDER.as_bytes() {
        if let Some(ptr) = instance.render_ext {
            return ptr as *const c_void;
        }
    }

    ptr::null()
}

pub unsafe extern "C" fn plugin_on_main_thread<P: Plugin>(
    _plugin: *const clap_plugin_t,
) {
}

// ======= GUI PLUGIN SUPPORT =======

use crate::ext::gui::{GuiHandler, create_gui_ext};
use clap_sys::ext::gui::{clap_plugin_gui_t, CLAP_EXT_GUI};

/// Plugin instance wrapper with GUI support
pub struct PluginInstanceWithGui<P: Plugin + GuiHandler> {
    pub plugin: P,
    // Caches
    pub params_cache: Vec<ParameterInfo>,
    pub audio_ports_cache: Vec<AudioPortInfo>,
    pub note_ports_cache: Vec<NotePortInfo>,
    // Extension struct storage (leaked, lives for plugin lifetime)
    pub audio_ports_ext: Option<*const clap_plugin_audio_ports_t>,
    pub note_ports_ext: Option<*const clap_plugin_note_ports_t>,
    pub params_ext: Option<*const clap_plugin_params_t>,
    pub state_ext: Option<*const clap_plugin_state_t>,
    pub latency_ext: Option<*const clap_plugin_latency_t>,
    pub tail_ext: Option<*const clap_plugin_tail_t>,
    pub voice_info_ext: Option<*const clap_plugin_voice_info_t>,
    pub render_ext: Option<*const clap_plugin_render_t>,
    pub gui_ext: Option<*const clap_plugin_gui_t>,
}

impl<P: Plugin + GuiHandler> PluginInstanceWithGui<P> {
    pub fn new(host: HostHandle) -> Self {
        let plugin = P::new(host);
        let params_cache = plugin.declare_parameters();
        let audio_ports_cache = plugin.audio_ports_config();
        let note_ports_cache = plugin.note_ports_config();
        Self {
            plugin,
            params_cache,
            audio_ports_cache,
            note_ports_cache,
            audio_ports_ext: None,
            note_ports_ext: None,
            params_ext: None,
            state_ext: None,
            latency_ext: None,
            tail_ext: None,
            voice_info_ext: None,
            render_ext: None,
            gui_ext: None,
        }
    }
}

pub unsafe extern "C" fn plugin_init_with_gui<P: Plugin + GuiHandler>(plugin: *const clap_plugin_t) -> bool {
    let instance = unsafe { &mut *((*plugin).plugin_data as *mut PluginInstanceWithGui<P>) };
    
    // Initialize extension structs (use _gui versions for proper type casts)
    if !instance.audio_ports_cache.is_empty() {
        let ext = Box::new(create_audio_ports_ext_gui::<P>());
        instance.audio_ports_ext = Some(Box::into_raw(ext));
    }
    
    if !instance.note_ports_cache.is_empty() {
        let ext = Box::new(create_note_ports_ext_gui::<P>());
        instance.note_ports_ext = Some(Box::into_raw(ext));
    }
    
    if !instance.params_cache.is_empty() {
        let ext = Box::new(create_params_ext_gui::<P>());
        instance.params_ext = Some(Box::into_raw(ext));
    }
    
    // Always add state extension
    let state_ext = Box::new(create_state_ext_gui::<P>());
    instance.state_ext = Some(Box::into_raw(state_ext));
    
    // Always add latency extension
    let latency_ext = Box::new(create_latency_ext_gui::<P>());
    instance.latency_ext = Some(Box::into_raw(latency_ext));
    
    // Always add tail extension
    let tail_ext = Box::new(create_tail_ext_gui::<P>());
    instance.tail_ext = Some(Box::into_raw(tail_ext));
    
    // Add voice_info only if plugin provides it
    if instance.plugin.voice_info().is_some() {
        let ext = Box::new(create_voice_info_ext_gui::<P>());
        instance.voice_info_ext = Some(Box::into_raw(ext));
    }
    
    // Always add render extension
    let render_ext = Box::new(create_render_ext_gui::<P>());
    instance.render_ext = Some(Box::into_raw(render_ext));
    
    // Always add GUI extension for GUI plugins
    let gui_ext = Box::new(create_gui_ext::<P>());
    instance.gui_ext = Some(Box::into_raw(gui_ext));
    
    instance.plugin.init()
}

pub unsafe extern "C" fn plugin_destroy_with_gui<P: Plugin + GuiHandler>(plugin: *const clap_plugin_t) {
    let plugin = unsafe { &mut *(plugin as *mut clap_plugin_t) };
    if !plugin.plugin_data.is_null() {
        let instance = unsafe { Box::from_raw(plugin.plugin_data as *mut PluginInstanceWithGui<P>) };
        // Clean up leaked extension structs
        if let Some(ptr) = instance.audio_ports_ext {
            unsafe { let _ = Box::from_raw(ptr as *mut clap_plugin_audio_ports_t); }
        }
        if let Some(ptr) = instance.note_ports_ext {
            unsafe { let _ = Box::from_raw(ptr as *mut clap_plugin_note_ports_t); }
        }
        if let Some(ptr) = instance.params_ext {
            unsafe { let _ = Box::from_raw(ptr as *mut clap_plugin_params_t); }
        }
        if let Some(ptr) = instance.state_ext {
            unsafe { let _ = Box::from_raw(ptr as *mut clap_plugin_state_t); }
        }
        if let Some(ptr) = instance.latency_ext {
            unsafe { let _ = Box::from_raw(ptr as *mut clap_plugin_latency_t); }
        }
        if let Some(ptr) = instance.tail_ext {
            unsafe { let _ = Box::from_raw(ptr as *mut clap_plugin_tail_t); }
        }
        if let Some(ptr) = instance.voice_info_ext {
            unsafe { let _ = Box::from_raw(ptr as *mut clap_plugin_voice_info_t); }
        }
        if let Some(ptr) = instance.render_ext {
            unsafe { let _ = Box::from_raw(ptr as *mut clap_plugin_render_t); }
        }
        if let Some(ptr) = instance.gui_ext {
            unsafe { let _ = Box::from_raw(ptr as *mut clap_plugin_gui_t); }
        }
    }
}

pub unsafe extern "C" fn plugin_activate_with_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    sample_rate: f64,
    min_frames: u32,
    max_frames: u32,
) -> bool {
    let instance = unsafe { &mut *((*plugin).plugin_data as *mut PluginInstanceWithGui<P>) };
    instance.plugin.activate(sample_rate, min_frames, max_frames)
}

pub unsafe extern "C" fn plugin_deactivate_with_gui<P: Plugin + GuiHandler>(plugin: *const clap_plugin_t) {
    let instance = unsafe { &mut *((*plugin).plugin_data as *mut PluginInstanceWithGui<P>) };
    instance.plugin.deactivate();
}

pub unsafe extern "C" fn plugin_start_processing_with_gui<P: Plugin + GuiHandler>(_plugin: *const clap_plugin_t) -> bool {
    true
}

pub unsafe extern "C" fn plugin_stop_processing_with_gui<P: Plugin + GuiHandler>(_plugin: *const clap_plugin_t) {}

pub unsafe extern "C" fn plugin_reset_with_gui<P: Plugin + GuiHandler>(plugin: *const clap_plugin_t) {
    let instance = unsafe { &mut *((*plugin).plugin_data as *mut PluginInstanceWithGui<P>) };
    instance.plugin.reset();
}

pub unsafe extern "C" fn plugin_process_with_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    process: *const clap_process_t,
) -> clap_process_status {
    let instance = unsafe { &mut *((*plugin).plugin_data as *mut PluginInstanceWithGui<P>) };
    let process_ref = unsafe { &*process };
    
    // Apply param events BEFORE processing audio
    unsafe { apply_param_events_gui(instance, process_ref.in_events); }
    
    let context = unsafe { ProcessContext::from_raw(process) };
    instance.plugin.process(context)
}

unsafe fn apply_param_events_gui<P: Plugin + GuiHandler>(
    instance: &mut PluginInstanceWithGui<P>,
    in_events: *const clap_sys::events::clap_input_events_t
) {
    if in_events.is_null() { return; }
    let size_fn = unsafe { (*in_events).size };
    let get_fn = unsafe { (*in_events).get };
    if size_fn.is_none() || get_fn.is_none() { return; }
    let size = unsafe { size_fn.unwrap()(in_events) };
    for index in 0..size {
        let header = unsafe { get_fn.unwrap()(in_events, index) };
        if header.is_null() { continue; }
        if unsafe { (*header).space_id == clap_sys::events::CLAP_CORE_EVENT_SPACE_ID && (*header).type_ == clap_sys::events::CLAP_EVENT_PARAM_VALUE } {
            let event = unsafe { &*(header as *const clap_sys::events::clap_event_param_value_t) };
            instance.plugin.set_parameter(event.param_id, event.value);
        }
    }
}

pub unsafe extern "C" fn plugin_get_extension_with_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    id: *const c_char,
) -> *const c_void {
    if id.is_null() { return ptr::null(); }
    let id_cstr = unsafe { CStr::from_ptr(id) };
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstanceWithGui<P>) };
    
    // GUI extension first!
    if id_cstr.to_bytes_with_nul() == CLAP_EXT_GUI.as_bytes() {
        if let Some(ptr) = instance.gui_ext {
            return ptr as *const c_void;
        }
    }
    
    if id_cstr.to_bytes_with_nul() == CLAP_EXT_AUDIO_PORTS.as_bytes() {
        if let Some(ptr) = instance.audio_ports_ext {
            return ptr as *const c_void;
        }
    }

    if id_cstr.to_bytes_with_nul() == CLAP_EXT_NOTE_PORTS.as_bytes() {
        if let Some(ptr) = instance.note_ports_ext {
            return ptr as *const c_void;
        }
    }

    if id_cstr.to_bytes_with_nul() == CLAP_EXT_PARAMS.as_bytes() {
        if let Some(ptr) = instance.params_ext {
            return ptr as *const c_void;
        }
    }

    if id_cstr.to_bytes_with_nul() == CLAP_EXT_STATE.as_bytes() {
        if let Some(ptr) = instance.state_ext {
            return ptr as *const c_void;
        }
    }

    if id_cstr.to_bytes_with_nul() == CLAP_EXT_LATENCY.as_bytes() {
        if let Some(ptr) = instance.latency_ext {
            return ptr as *const c_void;
        }
    }

    if id_cstr.to_bytes_with_nul() == CLAP_EXT_TAIL.as_bytes() {
        if let Some(ptr) = instance.tail_ext {
            return ptr as *const c_void;
        }
    }

    if id_cstr.to_bytes_with_nul() == CLAP_EXT_VOICE_INFO.as_bytes() {
        if let Some(ptr) = instance.voice_info_ext {
            return ptr as *const c_void;
        }
    }

    if id_cstr.to_bytes_with_nul() == CLAP_EXT_RENDER.as_bytes() {
        if let Some(ptr) = instance.render_ext {
            return ptr as *const c_void;
        }
    }

    ptr::null()
}

pub unsafe extern "C" fn plugin_on_main_thread_with_gui<P: Plugin + GuiHandler>(
    _plugin: *const clap_plugin_t,
) {
}
