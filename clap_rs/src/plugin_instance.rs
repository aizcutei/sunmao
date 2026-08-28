//! Plugin instance management for clap_rs
//!
//! This module provides the PluginInstance wrapper and lifecycle callbacks.

use crate::ext::audio_ports::{AudioPortInfo, create_audio_ports_ext, create_audio_ports_ext_gui};
use crate::ext::audio_ports_activation::{
    create_audio_ports_activation_ext, create_audio_ports_activation_ext_gui,
};
use crate::ext::audio_ports_config::{
    AudioPortsConfig, create_audio_ports_config_ext, create_audio_ports_config_ext_gui,
    create_audio_ports_config_info_ext, create_audio_ports_config_info_ext_gui,
};
use crate::ext::latency::{create_latency_ext, create_latency_ext_gui};
use crate::ext::note_ports::{NotePortInfo, create_note_ports_ext, create_note_ports_ext_gui};
use crate::ext::params::{ParameterInfo, create_params_ext, create_params_ext_gui};
use crate::ext::render::{create_render_ext, create_render_ext_gui};
use crate::ext::state::{create_state_ext, create_state_ext_gui};
use crate::ext::tail::{create_tail_ext, create_tail_ext_gui};
use crate::ext::voice_info::{create_voice_info_ext, create_voice_info_ext_gui};
use crate::plugin::{AudioProcessor, HostHandle, Plugin};
use crate::process::{MAX_PROCESS_FRAMES, ProcessBuffers};

use clap_sys::ext::audio_ports::{CLAP_EXT_AUDIO_PORTS, clap_plugin_audio_ports_t};
use clap_sys::ext::audio_ports_activation::{
    CLAP_EXT_AUDIO_PORTS_ACTIVATION, CLAP_EXT_AUDIO_PORTS_ACTIVATION_COMPAT,
    clap_plugin_audio_ports_activation_t,
};
use clap_sys::ext::audio_ports_config::{
    CLAP_EXT_AUDIO_PORTS_CONFIG, CLAP_EXT_AUDIO_PORTS_CONFIG_INFO,
    CLAP_EXT_AUDIO_PORTS_CONFIG_INFO_COMPAT, clap_plugin_audio_ports_config_info_t,
    clap_plugin_audio_ports_config_t,
};
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

/// Execute a Rust callback behind a C ABI boundary.
///
/// Rust cannot unwind through an `extern "C"` frame.  A panic from user plugin
/// code would therefore abort the host (or, on older runtimes, invoke
/// undefined behaviour).  Every lifecycle/process entry point uses this small
/// guard and translates a panic to the ABI's failure value.
#[inline]
pub(crate) fn ffi_guard<T>(fallback: T, callback: impl FnOnce() -> T) -> T {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(callback)).unwrap_or(fallback)
}

struct AudioThreadState<A: AudioProcessor> {
    processor: Option<A>,
    process_buffers: ProcessBuffers,
    /// A realtime panic poisons the active processor until its matching
    /// deactivation completes. If deactivation itself panics, only `destroy`
    /// may reclaim the instance afterwards.
    poisoned: bool,
}

/// Plugin instance wrapper holding plugin data and extension caches
pub struct PluginInstance<P: Plugin> {
    controller: UnsafeCell<P>,
    pub(crate) host: HostHandle,
    /// CLAP calls `init` once in the normal lifecycle. Keep an explicit bit so
    /// a defensive repeated callback does not rebuild and leak extension
    /// tables or invoke user initialization twice.
    initialized: bool,
    // Caches
    pub params_cache: Vec<ParameterInfo>,
    pub audio_ports_cache: Vec<AudioPortInfo>,
    /// Host-selectable layouts, read once at construction.
    pub audio_ports_configs_cache: Vec<AudioPortsConfig>,
    pub note_ports_cache: Vec<NotePortInfo>,
    tail_cache: AtomicU32,
    audio_thread: UnsafeCell<AudioThreadState<P::AudioProcessor>>,
    // Extension struct storage (leaked, lives for plugin lifetime)
    pub audio_ports_ext: Option<*const clap_plugin_audio_ports_t>,
    pub audio_ports_activation_ext: Option<*const clap_plugin_audio_ports_activation_t>,
    pub audio_ports_config_ext: Option<*const clap_plugin_audio_ports_config_t>,
    pub audio_ports_config_info_ext: Option<*const clap_plugin_audio_ports_config_info_t>,
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
        let audio_ports_configs_cache = controller.audio_ports_configs();
        let note_ports_cache = controller.note_ports_config();
        let tail_cache = AtomicU32::new(controller.tail());
        let process_buffers = process_buffers_for(&audio_ports_cache);
        Self {
            controller: UnsafeCell::new(controller),
            host,
            initialized: false,
            params_cache,
            audio_ports_cache,
            audio_ports_configs_cache,
            note_ports_cache,
            tail_cache,
            audio_thread: UnsafeCell::new(AudioThreadState {
                processor: None,
                process_buffers,
                poisoned: false,
            }),
            audio_ports_ext: None,
            audio_ports_activation_ext: None,
            audio_ports_config_ext: None,
            audio_ports_config_info_ext: None,
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

/// Return the plugin-data pointer after validating the two ABI-owned pointer
/// levels. Individual callbacks still decide whether a missing instance is a
/// failure (`false`/`CLAP_PROCESS_ERROR`) or an empty no-op.
pub(crate) unsafe fn instance_ptr<P: Plugin>(
    plugin: *const clap_plugin_t,
) -> Option<*mut PluginInstance<P>> {
    if plugin.is_null() {
        return None;
    }
    let plugin_data = unsafe { (*plugin).plugin_data };
    (!plugin_data.is_null()).then_some(plugin_data.cast::<PluginInstance<P>>())
}

/// Disable audio callbacks after user code panics.
///
/// # Safety
///
/// The caller must be in a serialized CLAP lifecycle/process callback for
/// `plugin`. The plugin pointer must remain alive for the callback duration.
unsafe fn poison_audio_thread<P: Plugin>(plugin: *const clap_plugin_t) {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return;
    };
    let instance = unsafe { &*instance_ptr };
    unsafe { instance.audio_thread_mut() }.poisoned = true;
}

impl<P: Plugin> PluginInstance<P> {
    /// Release extension tables that were allocated by `init`.
    ///
    /// # Safety
    ///
    /// Each stored pointer must have been allocated by the corresponding
    /// `create_*_ext` function and must not be accessed after this call.
    unsafe fn clear_extensions(&mut self) {
        if let Some(ptr) = self.audio_ports_ext.take() {
            unsafe { drop(Box::from_raw(ptr as *mut clap_plugin_audio_ports_t)) };
        }
        if let Some(ptr) = self.audio_ports_activation_ext.take() {
            unsafe {
                drop(Box::from_raw(
                    ptr as *mut clap_plugin_audio_ports_activation_t,
                ))
            };
        }
        if let Some(ptr) = self.audio_ports_config_ext.take() {
            unsafe { drop(Box::from_raw(ptr as *mut clap_plugin_audio_ports_config_t)) };
        }
        if let Some(ptr) = self.audio_ports_config_info_ext.take() {
            unsafe {
                drop(Box::from_raw(
                    ptr as *mut clap_plugin_audio_ports_config_info_t,
                ))
            };
        }
        if let Some(ptr) = self.note_ports_ext.take() {
            unsafe { drop(Box::from_raw(ptr as *mut clap_plugin_note_ports_t)) };
        }
        if let Some(ptr) = self.params_ext.take() {
            unsafe { drop(Box::from_raw(ptr as *mut clap_plugin_params_t)) };
        }
        if let Some(ptr) = self.state_ext.take() {
            unsafe { drop(Box::from_raw(ptr as *mut clap_plugin_state_t)) };
        }
        if let Some(ptr) = self.latency_ext.take() {
            unsafe { drop(Box::from_raw(ptr as *mut clap_plugin_latency_t)) };
        }
        if let Some(ptr) = self.tail_ext.take() {
            unsafe { drop(Box::from_raw(ptr as *mut clap_plugin_tail_t)) };
        }
        if let Some(ptr) = self.voice_info_ext.take() {
            unsafe { drop(Box::from_raw(ptr as *mut clap_plugin_voice_info_t)) };
        }
        if let Some(ptr) = self.render_ext.take() {
            unsafe { drop(Box::from_raw(ptr as *mut clap_plugin_render_t)) };
        }
        if let Some(ptr) = self.gui_ext.take() {
            unsafe { drop(Box::from_raw(ptr as *mut clap_plugin_gui_t)) };
        }
    }
}

/// GUI and non-GUI plugins use the same storage layout. Only their exported
/// callback tables and initialized extensions differ.
pub type PluginInstanceWithGui<P> = PluginInstance<P>;

impl<P: Plugin> PluginInstance<P> {
    /// Rebuilds the audio-thread scratch buffers from the current port cache.
    ///
    /// Only valid while the plugin is deactivated — both the CLAP layout-switch
    /// and port-activation calls guarantee that — since it replaces state the
    /// audio thread would otherwise be reading.
    pub(crate) fn resize_process_buffers(&mut self) {
        let buffers = process_buffers_for(&self.audio_ports_cache);
        self.audio_thread.get_mut().process_buffers = buffers;
    }
}

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
    let initialized = ffi_guard(false, || unsafe { plugin_init_unchecked::<P>(plugin) });
    if !initialized {
        rollback_init::<P>(plugin);
    }
    initialized
}

/// Reclaim any extension tables published while an initialization attempt was
/// in progress. `init` is a transactional ABI callback: a failed attempt must
/// not leave `get_extension` exposing half-built tables to the host.
fn rollback_init<P: Plugin>(plugin: *const clap_plugin_t) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        let Some(instance_ptr) = instance_ptr::<P>(plugin) else {
            return;
        };
        let instance = &mut *instance_ptr;
        instance.initialized = false;
        instance.clear_extensions();
    }));
}

unsafe fn plugin_init_unchecked<P: Plugin>(plugin: *const clap_plugin_t) -> bool {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &mut *instance_ptr };
    if instance.initialized {
        return true;
    }
    // A failed earlier attempt may have left partially-built tables. They are
    // not visible to a valid host before init, so reclaim them before retrying.
    unsafe { instance.clear_extensions() };

    // Initialize extension structs
    if !instance.audio_ports_cache.is_empty() {
        let ext = Box::new(create_audio_ports_ext::<P>());
        instance.audio_ports_ext = Some(Box::into_raw(ext));
        let activation_ext = Box::new(create_audio_ports_activation_ext::<P>());
        instance.audio_ports_activation_ext = Some(Box::into_raw(activation_ext));
    }

    if !instance.audio_ports_configs_cache.is_empty() {
        let config_ext = Box::new(create_audio_ports_config_ext::<P>());
        instance.audio_ports_config_ext = Some(Box::into_raw(config_ext));
        let info_ext = Box::new(create_audio_ports_config_info_ext::<P>());
        instance.audio_ports_config_info_ext = Some(Box::into_raw(info_ext));
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
    if initialized {
        unsafe { instance.refresh_tail_cache() };
        instance.initialized = true;
    } else {
        unsafe { instance.clear_extensions() };
    }
    initialized
}

pub unsafe extern "C" fn plugin_destroy<P: Plugin>(plugin: *const clap_plugin_t) {
    ffi_guard((), || unsafe { plugin_destroy_unchecked::<P>(plugin) })
}

unsafe fn plugin_destroy_unchecked<P: Plugin>(plugin: *const clap_plugin_t) {
    if plugin.is_null() {
        return;
    }

    let plugin = unsafe { Box::from_raw(plugin as *mut clap_plugin_t) };
    if !plugin.plugin_data.is_null() {
        let mut instance = unsafe { Box::from_raw(plugin.plugin_data as *mut PluginInstance<P>) };
        // A conforming host deactivates first, but reclaim an accidentally
        // active processor here as well so user state is not leaked.
        let processor = unsafe { instance.audio_thread_mut() }.processor.take();
        unsafe { instance.audio_thread_mut() }
            .process_buffers
            .deactivate();
        if let Some(processor) = processor {
            // Destruction is best-effort: a faulty user deactivation must not
            // skip extension reclamation or unwind through the host callback.
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                instance.controller_mut().deactivate(processor);
            }));
        }
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            instance.clear_extensions();
        }));
        // A plugin's destructor is user code too. Catch it while the Box is
        // still owned so the raw callback remains non-unwinding.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(instance)));
    }
}

pub unsafe extern "C" fn plugin_activate<P: Plugin>(
    plugin: *const clap_plugin_t,
    sample_rate: f64,
    min_frames: u32,
    max_frames: u32,
) -> bool {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        plugin_activate_unchecked::<P>(plugin, sample_rate, min_frames, max_frames)
    })) {
        Ok(result) => result,
        Err(_) => {
            // `activate` allocates the scratch buffers before entering user
            // code. A later callback (for example tail refresh) can also
            // panic after activation returned a processor, so take it back
            // and run the matching deactivation before reporting failure.
            if let Some(instance_ptr) = unsafe { instance_ptr::<P>(plugin) } {
                let instance = unsafe { &*instance_ptr };
                let audio_thread = unsafe { instance.audio_thread_mut() };
                let processor = audio_thread.processor.take();
                audio_thread.process_buffers.deactivate();
                audio_thread.poisoned = false;
                if let Some(processor) = processor {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                        instance.controller_mut().deactivate(processor);
                    }));
                }
            }
            false
        }
    }
}

unsafe fn plugin_activate_unchecked<P: Plugin>(
    plugin: *const clap_plugin_t,
    sample_rate: f64,
    min_frames: u32,
    max_frames: u32,
) -> bool {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    if !sample_rate.is_finite()
        || sample_rate <= 0.0
        || min_frames > max_frames
        || max_frames > MAX_PROCESS_FRAMES
    {
        return false;
    }
    let instance = unsafe { &*instance_ptr };
    if !instance.initialized {
        return false;
    }
    let audio_thread = unsafe { instance.audio_thread_mut() };
    if audio_thread.poisoned
        || audio_thread.processor.is_some()
        || !audio_thread.process_buffers.activate(max_frames)
    {
        return false;
    }
    let Some(processor) =
        (unsafe { instance.controller_mut() }).activate(sample_rate, min_frames, max_frames)
    else {
        audio_thread.process_buffers.deactivate();
        return false;
    };
    audio_thread.processor = Some(processor);
    unsafe { instance.refresh_tail_cache() };
    true
}

pub unsafe extern "C" fn plugin_deactivate<P: Plugin>(plugin: *const clap_plugin_t) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        plugin_deactivate_unchecked::<P>(plugin)
    }))
    .is_err()
    {
        unsafe { poison_audio_thread::<P>(plugin) };
    }
}

unsafe fn plugin_deactivate_unchecked<P: Plugin>(plugin: *const clap_plugin_t) {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return;
    };
    let instance = unsafe { &*instance_ptr };
    let audio_thread = unsafe { instance.audio_thread_mut() };
    let processor = audio_thread.processor.take();
    audio_thread.process_buffers.deactivate();
    if let Some(processor) = processor {
        unsafe { instance.controller_mut() }.deactivate(processor);
        unsafe { instance.refresh_tail_cache() };
        // Clear poison only after the matching processor was handed back and
        // every serialized controller transition completed. A deactivation
        // panic leaves no processor for a later callback to recover with.
        audio_thread.poisoned = false;
    }
}

pub unsafe extern "C" fn plugin_start_processing<P: Plugin>(plugin: *const clap_plugin_t) -> bool {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        plugin_start_processing_unchecked::<P>(plugin)
    })) {
        Ok(started) => started,
        Err(_) => {
            unsafe { poison_audio_thread::<P>(plugin) };
            false
        }
    }
}

unsafe fn plugin_start_processing_unchecked<P: Plugin>(plugin: *const clap_plugin_t) -> bool {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &*instance_ptr };
    let audio_thread = unsafe { instance.audio_thread_mut() };
    if audio_thread.poisoned {
        return false;
    }
    audio_thread
        .processor
        .as_mut()
        .is_some_and(AudioProcessor::start_processing)
}

pub unsafe extern "C" fn plugin_stop_processing<P: Plugin>(plugin: *const clap_plugin_t) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        plugin_stop_processing_unchecked::<P>(plugin)
    }))
    .is_err()
    {
        unsafe { poison_audio_thread::<P>(plugin) };
    }
}

unsafe fn plugin_stop_processing_unchecked<P: Plugin>(plugin: *const clap_plugin_t) {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return;
    };
    let instance = unsafe { &*instance_ptr };
    let audio_thread = unsafe { instance.audio_thread_mut() };
    if audio_thread.poisoned {
        return;
    }
    if let Some(processor) = audio_thread.processor.as_mut() {
        processor.stop_processing();
    }
}

pub unsafe extern "C" fn plugin_reset<P: Plugin>(plugin: *const clap_plugin_t) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        plugin_reset_unchecked::<P>(plugin)
    }))
    .is_err()
    {
        unsafe { poison_audio_thread::<P>(plugin) };
    }
}

unsafe fn plugin_reset_unchecked<P: Plugin>(plugin: *const clap_plugin_t) {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return;
    };
    let instance = unsafe { &*instance_ptr };
    let audio_thread = unsafe { instance.audio_thread_mut() };
    if audio_thread.poisoned {
        return;
    }
    if let Some(processor) = audio_thread.processor.as_mut() {
        processor.reset();
    }
}

pub unsafe extern "C" fn plugin_process<P: Plugin>(
    plugin: *const clap_plugin_t,
    process: *const clap_process_t,
) -> clap_process_status {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        plugin_process_unchecked::<P>(plugin, process)
    })) {
        Ok(status) => status,
        Err(_) => {
            // Keep the processor available for the required deactivate call,
            // but do not invoke a potentially poisoned state again.
            unsafe { poison_audio_thread::<P>(plugin) };
            CLAP_PROCESS_ERROR
        }
    }
}

unsafe fn plugin_process_unchecked<P: Plugin>(
    plugin: *const clap_plugin_t,
    process: *const clap_process_t,
) -> clap_process_status {
    if plugin.is_null() || process.is_null() {
        return CLAP_PROCESS_ERROR;
    }
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return CLAP_PROCESS_ERROR;
    };
    let instance = unsafe { &*instance_ptr };
    let audio_thread = unsafe { instance.audio_thread_mut() };
    if audio_thread.poisoned {
        return CLAP_PROCESS_ERROR;
    }
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
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return ptr::null();
    };
    let instance = unsafe { &*instance_ptr };

    if id_cstr.to_bytes_with_nul() == CLAP_EXT_AUDIO_PORTS.as_bytes() {
        if let Some(ptr) = instance.audio_ports_ext {
            return ptr as *const c_void;
        }
    }

    if id_cstr.to_bytes_with_nul() == CLAP_EXT_AUDIO_PORTS_ACTIVATION.as_bytes()
        || id_cstr.to_bytes_with_nul() == CLAP_EXT_AUDIO_PORTS_ACTIVATION_COMPAT.as_bytes()
    {
        if let Some(ptr) = instance.audio_ports_activation_ext {
            return ptr as *const c_void;
        }
    }

    if id_cstr.to_bytes_with_nul() == CLAP_EXT_AUDIO_PORTS_CONFIG.as_bytes() {
        if let Some(ptr) = instance.audio_ports_config_ext {
            return ptr as *const c_void;
        }
    }

    if id_cstr.to_bytes_with_nul() == CLAP_EXT_AUDIO_PORTS_CONFIG_INFO.as_bytes()
        || id_cstr.to_bytes_with_nul() == CLAP_EXT_AUDIO_PORTS_CONFIG_INFO_COMPAT.as_bytes()
    {
        if let Some(ptr) = instance.audio_ports_config_info_ext {
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
    let initialized = ffi_guard(false, || unsafe {
        plugin_init_with_gui_unchecked::<P>(plugin)
    });
    if !initialized {
        rollback_init::<P>(plugin);
    }
    initialized
}

unsafe fn plugin_init_with_gui_unchecked<P: Plugin + GuiHandler>(
    plugin: *const clap_plugin_t,
) -> bool {
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return false;
    };
    let instance = unsafe { &mut *instance_ptr };
    if instance.initialized {
        return true;
    }
    unsafe { instance.clear_extensions() };

    // Initialize extension structs (use _gui versions for proper type casts)
    if !instance.audio_ports_cache.is_empty() {
        let ext = Box::new(create_audio_ports_ext_gui::<P>());
        instance.audio_ports_ext = Some(Box::into_raw(ext));
        let activation_ext = Box::new(create_audio_ports_activation_ext_gui::<P>());
        instance.audio_ports_activation_ext = Some(Box::into_raw(activation_ext));
    }

    if !instance.audio_ports_configs_cache.is_empty() {
        let config_ext = Box::new(create_audio_ports_config_ext_gui::<P>());
        instance.audio_ports_config_ext = Some(Box::into_raw(config_ext));
        let info_ext = Box::new(create_audio_ports_config_info_ext_gui::<P>());
        instance.audio_ports_config_info_ext = Some(Box::into_raw(info_ext));
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
    if initialized {
        unsafe { instance.refresh_tail_cache() };
        instance.initialized = true;
    } else {
        unsafe { instance.clear_extensions() };
    }
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
    let Some(instance_ptr) = (unsafe { instance_ptr::<P>(plugin) }) else {
        return ptr::null();
    };
    let instance = unsafe { &*instance_ptr };

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

    if id_cstr.to_bytes_with_nul() == CLAP_EXT_AUDIO_PORTS_ACTIVATION.as_bytes()
        || id_cstr.to_bytes_with_nul() == CLAP_EXT_AUDIO_PORTS_ACTIVATION_COMPAT.as_bytes()
    {
        if let Some(ptr) = instance.audio_ports_activation_ext {
            return ptr as *const c_void;
        }
    }

    if id_cstr.to_bytes_with_nul() == CLAP_EXT_AUDIO_PORTS_CONFIG.as_bytes() {
        if let Some(ptr) = instance.audio_ports_config_ext {
            return ptr as *const c_void;
        }
    }

    if id_cstr.to_bytes_with_nul() == CLAP_EXT_AUDIO_PORTS_CONFIG_INFO.as_bytes()
        || id_cstr.to_bytes_with_nul() == CLAP_EXT_AUDIO_PORTS_CONFIG_INFO_COMPAT.as_bytes()
    {
        if let Some(ptr) = instance.audio_ports_config_info_ext {
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
