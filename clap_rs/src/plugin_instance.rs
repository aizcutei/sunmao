//! Plugin instance management for clap_rs
//!
//! This module provides the PluginInstance wrapper and lifecycle callbacks.

use crate::ext::audio_ports::{AudioPortInfo, create_audio_ports_ext, create_audio_ports_ext_gui};
use crate::ext::latency::{create_latency_ext, create_latency_ext_gui};
use crate::ext::note_ports::{NotePortInfo, create_note_ports_ext, create_note_ports_ext_gui};
use crate::ext::params::{ParameterInfo, create_params_ext, create_params_ext_gui};
use crate::ext::render::{create_render_ext, create_render_ext_gui};
use crate::ext::state::{create_state_ext, create_state_ext_gui};
use crate::ext::tail::{create_tail_ext, create_tail_ext_gui};
use crate::ext::voice_info::{create_voice_info_ext, create_voice_info_ext_gui};
use crate::plugin::{AudioProcessor, HostHandle, Plugin};
use crate::process::ProcessBuffers;

use clap_sys::ext::audio_ports::{CLAP_EXT_AUDIO_PORTS, clap_plugin_audio_ports_t};
use clap_sys::ext::gui::clap_plugin_gui_t;
use clap_sys::ext::latency::{CLAP_EXT_LATENCY, clap_plugin_latency_t};
use clap_sys::ext::note_ports::{CLAP_EXT_NOTE_PORTS, clap_plugin_note_ports_t};
use clap_sys::ext::params::{CLAP_EXT_PARAMS, clap_plugin_params_t};
use clap_sys::ext::render::{CLAP_EXT_RENDER, clap_plugin_render_t};
use clap_sys::ext::state::{CLAP_EXT_STATE, clap_plugin_state_t};
use clap_sys::ext::tail::{CLAP_EXT_TAIL, clap_plugin_tail_t};
use clap_sys::ext::voice_info::{CLAP_EXT_VOICE_INFO, clap_plugin_voice_info_t};
use clap_sys::plugin::clap_plugin_t;
use clap_sys::process::{CLAP_PROCESS_ERROR, clap_process_status, clap_process_t};
use std::cell::UnsafeCell;
use std::ffi::{CStr, c_char, c_void};
use std::ptr;
use std::sync::atomic::{AtomicU32, Ordering};

struct AudioThreadState<A: AudioProcessor> {
    processor: Option<A>,
    process_buffers: ProcessBuffers,
}

/// Plugin instance wrapper holding plugin data and extension caches
pub struct PluginInstance<P: Plugin> {
    controller: UnsafeCell<P>,
    pub(crate) host: HostHandle,
    // Caches
    pub params_cache: Vec<ParameterInfo>,
    pub audio_ports_cache: Vec<AudioPortInfo>,
    pub note_ports_cache: Vec<NotePortInfo>,
    tail_cache: AtomicU32,
    audio_thread: UnsafeCell<AudioThreadState<P::AudioProcessor>>,
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

impl<P: Plugin> PluginInstance<P> {
    pub fn new(host: HostHandle) -> Self {
        let controller = P::new(host.clone());
        let params_cache = controller.declare_parameters();
        let audio_ports_cache = controller.audio_ports_config();
        let note_ports_cache = controller.note_ports_config();
        let tail_cache = AtomicU32::new(controller.tail());
        let process_buffers = process_buffers_for(&audio_ports_cache);
        Self {
            controller: UnsafeCell::new(controller),
            host,
            params_cache,
            audio_ports_cache,
            note_ports_cache,
            tail_cache,
            audio_thread: UnsafeCell::new(AudioThreadState {
                processor: None,
                process_buffers,
            }),
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

    /// # Safety
    ///
    /// The caller must obey CLAP's main-thread callback rules and must not
    /// create overlapping mutable controller borrows.
    pub(crate) unsafe fn controller(&self) -> &P {
        unsafe { &*self.controller.get() }
    }

    /// # Safety
    ///
    /// The caller must obey CLAP's main-thread callback rules and must not
    /// create overlapping controller borrows.
    pub(crate) unsafe fn controller_mut(&self) -> &mut P {
        unsafe { &mut *self.controller.get() }
    }

    /// # Safety
    ///
    /// The caller must be in a CLAP audio-thread callback serialized with
    /// processing and activation/deactivation.
    unsafe fn audio_thread_mut(&self) -> &mut AudioThreadState<P::AudioProcessor> {
        unsafe { &mut *self.audio_thread.get() }
    }

    /// Routes `params.flush` to the active audio processor or inactive
    /// main-thread controller.
    ///
    /// # Safety
    ///
    /// The caller must obey CLAP's `params.flush` threading contract: audio
    /// thread while active, main thread while inactive, never concurrent with
    /// `process`.
    pub(crate) unsafe fn set_parameter_for_current_thread(&self, id: u32, value: f64) {
        let audio_thread = unsafe { self.audio_thread_mut() };
        if let Some(processor) = audio_thread.processor.as_mut() {
            processor.set_parameter(id, value);
        } else {
            unsafe { self.controller_mut() }.set_parameter(id, value);
            unsafe { self.refresh_tail_cache() };
        }
    }

    /// Returns the last tail value published from a serialized main-thread
    /// controller transition. `tail.get` may run concurrently on either CLAP
    /// thread, so it must not access the controller directly.
    pub(crate) fn cached_tail(&self) -> u32 {
        self.tail_cache.load(Ordering::Relaxed)
    }

    /// # Safety
    ///
    /// The caller must be in a serialized main-thread controller callback.
    pub(crate) unsafe fn refresh_tail_cache(&self) {
        let tail = unsafe { self.controller() }.tail();
        self.tail_cache.store(tail, Ordering::Relaxed);
    }
}

/// GUI and non-GUI plugins use the same storage layout. Only their exported
/// callback tables and initialized extensions differ.
pub type PluginInstanceWithGui<P> = PluginInstance<P>;

fn process_buffers_for(audio_ports: &[AudioPortInfo]) -> ProcessBuffers {
    ProcessBuffers::new(
        audio_ports
            .iter()
            .filter(|port| port.is_input)
            .map(|port| port.channel_count)
            .collect(),
        audio_ports
            .iter()
            .filter(|port| !port.is_input)
            .map(|port| port.channel_count)
            .collect(),
    )
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
    if unsafe { instance.controller() }.voice_info().is_some() {
        let ext = Box::new(create_voice_info_ext::<P>());
        instance.voice_info_ext = Some(Box::into_raw(ext));
    }

    // Always add render extension
    let render_ext = Box::new(create_render_ext::<P>());
    instance.render_ext = Some(Box::into_raw(render_ext));

    let initialized = unsafe { instance.controller_mut() }.init();
    unsafe { instance.refresh_tail_cache() };
    initialized
}

pub unsafe extern "C" fn plugin_destroy<P: Plugin>(plugin: *const clap_plugin_t) {
    if plugin.is_null() {
        return;
    }

    let plugin = unsafe { Box::from_raw(plugin as *mut clap_plugin_t) };
    if !plugin.plugin_data.is_null() {
        let instance = unsafe { Box::from_raw(plugin.plugin_data as *mut PluginInstance<P>) };
        // Clean up leaked extension structs
        if let Some(ptr) = instance.audio_ports_ext {
            unsafe {
                let _ = Box::from_raw(ptr as *mut clap_plugin_audio_ports_t);
            }
        }
        if let Some(ptr) = instance.note_ports_ext {
            unsafe {
                let _ = Box::from_raw(ptr as *mut clap_plugin_note_ports_t);
            }
        }
        if let Some(ptr) = instance.params_ext {
            unsafe {
                let _ = Box::from_raw(ptr as *mut clap_plugin_params_t);
            }
        }
        if let Some(ptr) = instance.state_ext {
            unsafe {
                let _ = Box::from_raw(ptr as *mut clap_plugin_state_t);
            }
        }
        if let Some(ptr) = instance.latency_ext {
            unsafe {
                let _ = Box::from_raw(ptr as *mut clap_plugin_latency_t);
            }
        }
        if let Some(ptr) = instance.tail_ext {
            unsafe {
                let _ = Box::from_raw(ptr as *mut clap_plugin_tail_t);
            }
        }
        if let Some(ptr) = instance.voice_info_ext {
            unsafe {
                let _ = Box::from_raw(ptr as *mut clap_plugin_voice_info_t);
            }
        }
        if let Some(ptr) = instance.render_ext {
            unsafe {
                let _ = Box::from_raw(ptr as *mut clap_plugin_render_t);
            }
        }
        if let Some(ptr) = instance.gui_ext {
            unsafe {
                let _ = Box::from_raw(ptr as *mut clap_plugin_gui_t);
            }
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
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstance<P>) };
    let audio_thread = unsafe { instance.audio_thread_mut() };
    if audio_thread.processor.is_some() || !audio_thread.process_buffers.activate(max_frames) {
        return false;
    }
    let Some(processor) =
        (unsafe { instance.controller_mut() }).activate(sample_rate, min_frames, max_frames)
    else {
        audio_thread.process_buffers.deactivate();
        return false;
    };
    unsafe { instance.refresh_tail_cache() };
    audio_thread.processor = Some(processor);
    true
}

pub unsafe extern "C" fn plugin_deactivate<P: Plugin>(plugin: *const clap_plugin_t) {
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstance<P>) };
    let audio_thread = unsafe { instance.audio_thread_mut() };
    let processor = audio_thread.processor.take();
    audio_thread.process_buffers.deactivate();
    if let Some(processor) = processor {
        unsafe { instance.controller_mut() }.deactivate(processor);
        unsafe { instance.refresh_tail_cache() };
    }
}

pub unsafe extern "C" fn plugin_start_processing<P: Plugin>(plugin: *const clap_plugin_t) -> bool {
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstance<P>) };
    unsafe { instance.audio_thread_mut() }
        .processor
        .as_mut()
        .is_some_and(AudioProcessor::start_processing)
}

pub unsafe extern "C" fn plugin_stop_processing<P: Plugin>(plugin: *const clap_plugin_t) {
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstance<P>) };
    if let Some(processor) = unsafe { instance.audio_thread_mut() }.processor.as_mut() {
        processor.stop_processing();
    }
}

pub unsafe extern "C" fn plugin_reset<P: Plugin>(plugin: *const clap_plugin_t) {
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstance<P>) };
    if let Some(processor) = unsafe { instance.audio_thread_mut() }.processor.as_mut() {
        processor.reset();
    }
}

pub unsafe extern "C" fn plugin_process<P: Plugin>(
    plugin: *const clap_plugin_t,
    process: *const clap_process_t,
) -> clap_process_status {
    if plugin.is_null() || process.is_null() {
        return CLAP_PROCESS_ERROR;
    }
    let instance = unsafe { &*((*plugin).plugin_data as *const PluginInstance<P>) };
    let audio_thread = unsafe { instance.audio_thread_mut() };
    let Some(processor) = audio_thread.processor.as_mut() else {
        return CLAP_PROCESS_ERROR;
    };
    unsafe {
        audio_thread
            .process_buffers
            .process(process, |context| processor.process(context))
    }
    .unwrap_or(CLAP_PROCESS_ERROR)
}

// ======= GET EXTENSION =======

pub unsafe extern "C" fn plugin_get_extension<P: Plugin>(
    plugin: *const clap_plugin_t,
    id: *const c_char,
) -> *const c_void {
    if id.is_null() {
        return ptr::null();
    }
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

pub unsafe extern "C" fn plugin_on_main_thread<P: Plugin>(_plugin: *const clap_plugin_t) {}

// ======= GUI PLUGIN SUPPORT =======

use crate::ext::gui::{GuiHandler, create_gui_ext};
use clap_sys::ext::gui::CLAP_EXT_GUI;

pub unsafe extern "C" fn plugin_init_with_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
) -> bool {
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
    if unsafe { instance.controller() }.voice_info().is_some() {
        let ext = Box::new(create_voice_info_ext_gui::<P>());
        instance.voice_info_ext = Some(Box::into_raw(ext));
    }

    // Always add render extension
    let render_ext = Box::new(create_render_ext_gui::<P>());
    instance.render_ext = Some(Box::into_raw(render_ext));

    // Always add GUI extension for GUI plugins
    let gui_ext = Box::new(create_gui_ext::<P>());
    instance.gui_ext = Some(Box::into_raw(gui_ext));

    let initialized = unsafe { instance.controller_mut() }.init();
    unsafe { instance.refresh_tail_cache() };
    initialized
}

pub unsafe extern "C" fn plugin_destroy_with_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
) {
    unsafe { plugin_destroy::<P>(plugin) }
}

pub unsafe extern "C" fn plugin_activate_with_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    sample_rate: f64,
    min_frames: u32,
    max_frames: u32,
) -> bool {
    unsafe { plugin_activate::<P>(plugin, sample_rate, min_frames, max_frames) }
}

pub unsafe extern "C" fn plugin_deactivate_with_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
) {
    unsafe { plugin_deactivate::<P>(plugin) }
}

pub unsafe extern "C" fn plugin_start_processing_with_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
) -> bool {
    unsafe { plugin_start_processing::<P>(plugin) }
}

pub unsafe extern "C" fn plugin_stop_processing_with_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
) {
    unsafe { plugin_stop_processing::<P>(plugin) }
}

pub unsafe extern "C" fn plugin_reset_with_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
) {
    unsafe { plugin_reset::<P>(plugin) }
}

pub unsafe extern "C" fn plugin_process_with_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    process: *const clap_process_t,
) -> clap_process_status {
    unsafe { plugin_process::<P>(plugin, process) }
}

pub unsafe extern "C" fn plugin_get_extension_with_gui<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
    id: *const c_char,
) -> *const c_void {
    if id.is_null() {
        return ptr::null();
    }
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
