//! Plugin trait and related types

use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use vst3_sys::base::{IUnknownVtbl, kResultOk};
use vst3_sys::gui::{IPlugFrameVtbl, ViewRect};
use vst3_sys::vst::types::ProcessModes;
use vst3_sys::vst::{IComponentHandlerVtbl, SpeakerArrangement};

/// Plugin information for registration
#[derive(Clone, Debug)]
pub struct PluginInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub vendor: &'static str,
    pub url: &'static str,
    pub email: &'static str,
    pub version: &'static str,
    /// Plugin category: "Fx" for effects, "Instrument|Synth" for synths
    pub category: &'static str,
}

impl Default for PluginInfo {
    fn default() -> Self {
        Self {
            id: "com.example.plugin",
            name: "Example Plugin",
            vendor: "Example",
            url: "",
            email: "",
            version: "1.0.0",
            category: "Fx",
        }
    }
}

/// Host render mode negotiated through `ProcessSetup.process_mode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RenderMode {
    /// `kRealtime`, and `kPrefetch` which still runs under realtime rules.
    #[default]
    Realtime,
    /// `kOffline`: a bounce or export with no realtime deadline.
    Offline,
}

impl RenderMode {
    /// Maps a raw `ProcessModes` value; unknown modes are treated as realtime
    /// because that is the stricter contract.
    pub fn from_process_mode(mode: i32) -> Self {
        if mode == ProcessModes::kOffline {
            Self::Offline
        } else {
            Self::Realtime
        }
    }
}

/// Audio port configuration
#[derive(Clone, Debug)]
pub struct AudioConfig {
    pub inputs: Vec<PortConfig>,
    pub outputs: Vec<PortConfig>,
    /// Whether plugin accepts MIDI events
    pub accepts_midi: bool,
}

impl AudioConfig {
    /// Standard stereo effect (one stereo input, one stereo output)
    pub fn stereo_effect() -> Self {
        Self {
            inputs: vec![PortConfig::stereo("Input")],
            outputs: vec![PortConfig::stereo("Output")],
            accepts_midi: false,
        }
    }

    /// Stereo synth (no audio input, one stereo output, accepts MIDI)
    pub fn stereo_synth() -> Self {
        Self {
            inputs: vec![],
            outputs: vec![PortConfig::stereo("Output")],
            accepts_midi: true,
        }
    }
}

/// Single audio port configuration
#[derive(Clone, Debug)]
pub struct PortConfig {
    pub name: &'static str,
    pub channels: u32,
    pub port_type: PortType,
    /// Explicit VST3 speaker mask. Empty, mono, and stereo are inferred when omitted.
    pub speaker_arrangement: Option<SpeakerArrangement>,
}

impl PortConfig {
    pub fn stereo(name: &'static str) -> Self {
        Self {
            name,
            channels: 2,
            port_type: PortType::Main,
            speaker_arrangement: Some(vst3_sys::vst::SpeakerArr::kStereo),
        }
    }

    pub fn mono(name: &'static str) -> Self {
        Self {
            name,
            channels: 1,
            port_type: PortType::Main,
            speaker_arrangement: Some(vst3_sys::vst::SpeakerArr::kMono),
        }
    }

    pub fn with_speaker_arrangement(mut self, arrangement: SpeakerArrangement) -> Self {
        self.speaker_arrangement = Some(arrangement);
        self
    }
}

/// Port type (main or auxiliary)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PortType {
    Main,
    Aux,
}

struct ParameterValue {
    id: u32,
    value: AtomicU64,
}

struct ParameterValues {
    values: Vec<ParameterValue>,
    generation: AtomicU64,
}

/// Lock-free parameter values shared by one VST3 processor/controller pair.
pub struct ParameterBridge {
    local: Arc<ParameterValues>,
    active: AtomicPtr<ParameterValues>,
    retained_targets: Mutex<Vec<Arc<ParameterValues>>>,
}

impl ParameterBridge {
    pub(crate) fn new(params: &[crate::ParamInfo]) -> Self {
        let values = params
            .iter()
            .map(|param| ParameterValue {
                id: param.id,
                value: AtomicU64::new(normalize(param.default, 0.0).to_bits()),
            })
            .collect();
        let local = Arc::new(ParameterValues {
            values,
            generation: AtomicU64::new(0),
        });
        let active = AtomicPtr::new(Arc::as_ptr(&local).cast_mut());
        Self {
            local,
            active,
            retained_targets: Mutex::new(Vec::new()),
        }
    }

    fn retained_targets(&self) -> std::sync::MutexGuard<'_, Vec<Arc<ParameterValues>>> {
        self.retained_targets
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn active(&self) -> &ParameterValues {
        let active = self.active.load(Ordering::Acquire);
        debug_assert!(!active.is_null());

        // `active` points to `local` or an Arc kept in `retained_targets`. Targets are never
        // removed, so readers can safely finish after a concurrent disconnect or reconnect.
        unsafe { &*active }
    }

    fn clone_active(&self) -> Option<Arc<ParameterValues>> {
        let retained_targets = self.retained_targets();
        let active = self.active.load(Ordering::Acquire);
        if active == Arc::as_ptr(&self.local).cast_mut() {
            return Some(self.local.clone());
        }
        retained_targets
            .iter()
            .find(|target| Arc::as_ptr(target).cast_mut() == active)
            .cloned()
    }

    pub(crate) fn link_to(&self, other: &Self) -> bool {
        let Some(target) = other.clone_active() else {
            return false;
        };
        let target_ptr = Arc::as_ptr(&target).cast_mut();
        let mut retained_targets = self.retained_targets();
        if self.active.load(Ordering::Acquire) == target_ptr {
            return true;
        }
        if target_ptr != Arc::as_ptr(&self.local).cast_mut()
            && !retained_targets
                .iter()
                .any(|retained| Arc::ptr_eq(retained, &target))
        {
            retained_targets.push(target);
        }
        self.active.store(target_ptr, Ordering::Release);
        true
    }

    pub(crate) fn disconnect_from(&self, other: &Self) -> bool {
        let Some(expected_target) = other.clone_active() else {
            return false;
        };
        let expected_ptr = Arc::as_ptr(&expected_target).cast_mut();
        let _retained_targets = self.retained_targets();
        let active_ptr = self.active.load(Ordering::Acquire);
        let local_ptr = Arc::as_ptr(&self.local).cast_mut();

        if active_ptr == local_ptr {
            return true;
        }
        if active_ptr != expected_ptr {
            return false;
        }

        let active = unsafe { &*active_ptr };
        for local in &self.local.values {
            if let Some(source) = active.values.iter().find(|entry| entry.id == local.id) {
                local
                    .value
                    .store(source.value.load(Ordering::Relaxed), Ordering::Relaxed);
            }
        }
        self.local.generation.fetch_add(1, Ordering::Release);
        self.active.store(local_ptr, Ordering::Release);
        true
    }

    /// Read a normalized parameter value.
    pub fn get(&self, id: u32) -> f64 {
        self.active()
            .values
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| f64::from_bits(entry.value.load(Ordering::Relaxed)))
            .unwrap_or(0.0)
    }

    /// Write a normalized parameter value. Returns false for an unknown ID.
    pub fn set(&self, id: u32, value: f64) -> bool {
        let values = self.active();
        let Some(entry) = values.values.iter().find(|entry| entry.id == id) else {
            return false;
        };
        let previous = f64::from_bits(entry.value.load(Ordering::Relaxed));
        let value = normalize(value, previous);
        if entry.value.swap(value.to_bits(), Ordering::Relaxed) != value.to_bits() {
            values.generation.fetch_add(1, Ordering::Release);
        }
        true
    }

    pub(crate) fn generation(&self) -> u64 {
        self.active().generation.load(Ordering::Acquire)
    }
}

fn normalize(value: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        fallback
    }
}

/// Handle to communicate with the host.
#[derive(Clone)]
pub struct HostHandle {
    inner: Arc<HostState>,
}

struct HostState {
    parameter_bridge: Arc<ParameterBridge>,
    component_handler: AtomicPtr<c_void>,
    plug_frame: AtomicPtr<c_void>,
    plug_view: AtomicPtr<c_void>,
}

unsafe fn retain_unknown(object: *mut c_void) -> bool {
    if object.is_null() {
        return true;
    }
    let vtbl = unsafe { *(object as *const *const IUnknownVtbl) };
    if vtbl.is_null() {
        return false;
    }
    unsafe { ((*vtbl).add_ref)(object) };
    true
}

unsafe fn release_unknown(object: *mut c_void) {
    if object.is_null() {
        return;
    }
    let vtbl = unsafe { *(object as *const *const IUnknownVtbl) };
    if !vtbl.is_null() {
        unsafe { ((*vtbl).release)(object) };
    }
}

fn replace_retained(slot: &AtomicPtr<c_void>, object: *mut c_void) -> bool {
    if !unsafe { retain_unknown(object) } {
        return false;
    }
    let previous = slot.swap(object, Ordering::AcqRel);
    unsafe { release_unknown(previous) };
    true
}

impl Drop for HostState {
    fn drop(&mut self) {
        unsafe {
            release_unknown(*self.component_handler.get_mut());
            release_unknown(*self.plug_frame.get_mut());
        }
    }
}

impl HostHandle {
    pub(crate) fn new(parameter_bridge: Arc<ParameterBridge>) -> Self {
        Self {
            inner: Arc::new(HostState {
                parameter_bridge,
                component_handler: AtomicPtr::new(std::ptr::null_mut()),
                plug_frame: AtomicPtr::new(std::ptr::null_mut()),
                plug_view: AtomicPtr::new(std::ptr::null_mut()),
            }),
        }
    }

    /// Parameter bridge for this processor or controller instance.
    pub fn parameter_bridge(&self) -> Arc<ParameterBridge> {
        self.inner.parameter_bridge.clone()
    }

    /// Notify the host that a GUI parameter gesture has started.
    pub fn begin_edit(&self, id: u32) -> bool {
        let handler = self.inner.component_handler.load(Ordering::Acquire);
        if handler.is_null() {
            return false;
        }
        let vtbl = unsafe { *(handler as *const *const IComponentHandlerVtbl) };
        !vtbl.is_null() && unsafe { ((*vtbl).begin_edit)(handler, id) == kResultOk }
    }

    /// Notify the host of a normalized GUI parameter value.
    pub fn perform_edit(&self, id: u32, value: f64) -> bool {
        if !value.is_finite() {
            return false;
        }
        let handler = self.inner.component_handler.load(Ordering::Acquire);
        if handler.is_null() {
            return false;
        }
        let vtbl = unsafe { *(handler as *const *const IComponentHandlerVtbl) };
        !vtbl.is_null()
            && unsafe { ((*vtbl).perform_edit)(handler, id, value.clamp(0.0, 1.0)) == kResultOk }
    }

    /// Notify the host that a GUI parameter gesture has ended.
    pub fn end_edit(&self, id: u32) -> bool {
        let handler = self.inner.component_handler.load(Ordering::Acquire);
        if handler.is_null() {
            return false;
        }
        let vtbl = unsafe { *(handler as *const *const IComponentHandlerVtbl) };
        !vtbl.is_null() && unsafe { ((*vtbl).end_edit)(handler, id) == kResultOk }
    }

    /// Ask the host to resize the attached editor view.
    pub fn request_resize(&self, width: u32, height: u32) -> bool {
        let Ok(width) = i32::try_from(width) else {
            return false;
        };
        let Ok(height) = i32::try_from(height) else {
            return false;
        };
        let frame = self.inner.plug_frame.load(Ordering::Acquire);
        let view = self.inner.plug_view.load(Ordering::Acquire);
        if frame.is_null() || view.is_null() {
            return false;
        }
        let vtbl = unsafe { *(frame as *const *const IPlugFrameVtbl) };
        if vtbl.is_null() {
            return false;
        }
        let mut size = ViewRect::new(0, 0, width, height);
        unsafe { ((*vtbl).resize_view)(frame, view, &mut size) == kResultOk }
    }

    pub(crate) fn set_component_handler(&self, handler: *mut c_void) -> bool {
        replace_retained(&self.inner.component_handler, handler)
    }

    pub(crate) fn set_plug_frame(&self, frame: *mut c_void, view: *mut c_void) -> bool {
        if !replace_retained(&self.inner.plug_frame, frame) {
            return false;
        }
        self.inner.plug_view.store(
            if frame.is_null() {
                std::ptr::null_mut()
            } else {
                view
            },
            Ordering::Release,
        );
        true
    }

    pub(crate) fn clear_plug_frame(&self, view: *mut c_void) {
        if self
            .inner
            .plug_view
            .compare_exchange(
                view,
                std::ptr::null_mut(),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            let frame = self
                .inner
                .plug_frame
                .swap(std::ptr::null_mut(), Ordering::AcqRel);
            unsafe { release_unknown(frame) };
        }
    }
}

unsafe impl Send for HostHandle {}
unsafe impl Sync for HostHandle {}

/// Main plugin trait - implement this to create a VST3 plugin.
///
/// VST3 hands processor and controller instances between host-managed threads
/// through raw COM pointers. The wrapper relies on VST3's serialized instance
/// callback contract instead of claiming that shared Rust references may be
/// used concurrently. Higher-level adapters can impose `Send` where their DSP
/// ownership model requires it.
pub trait Plugin: Sized + 'static {
    /// Maximum number of host events accepted in one processing block.
    const MAX_EVENTS_PER_BLOCK: usize = 4096;

    /// Plugin information (ID, name, vendor, etc.)
    fn info() -> PluginInfo;

    /// Stable 16-byte VST3 processor class ID.
    ///
    /// The default is deterministically derived from [`PluginInfo::id`]. Plugins may override
    /// this when compatibility with an existing VST3 class ID is required.
    fn class_id() -> [i8; 16] {
        class_id_from_str(Self::info().id)
    }

    /// Stable 16-byte VST3 controller class ID.
    ///
    /// This must differ from the processor class ID. The default applies a reversible namespace
    /// transform to [`Plugin::class_id`].
    fn controller_class_id() -> [i8; 16] {
        let mut id = Self::class_id();
        id[0] ^= 0x5A_u8 as i8;
        id[15] ^= 0xA5_u8 as i8;
        id
    }

    /// Create a new plugin instance
    fn new(host: HostHandle) -> Self;

    // === Lifecycle ===

    /// Called after construction, before any processing
    fn init(&mut self) -> bool {
        true
    }

    /// Called when plugin is activated (before processing starts)
    fn activate(&mut self, _sample_rate: f64, _max_frames: u32) -> bool {
        true
    }

    /// Called when plugin is deactivated
    fn deactivate(&mut self) {}

    /// Reset plugin state (clear buffers, etc.)
    fn reset(&mut self) {}

    // === Configuration ===

    /// Audio port configuration
    fn audio_config() -> AudioConfig {
        AudioConfig::stereo_effect()
    }

    /// Latency in samples (processing delay)
    fn latency(&self) -> u32 {
        0
    }

    /// Tail in samples (reverb tail, etc.)
    fn tail(&self) -> u32 {
        0
    }

    /// Called from `setupProcessing` with the host's negotiated process mode.
    ///
    /// VST3 only renegotiates the setup while the component is inactive, so
    /// this is a valid place to change the reported latency.
    fn set_render_mode(&mut self, _mode: RenderMode) {}

    // === Parameters ===

    /// Declare plugin parameters
    fn params() -> Vec<crate::ParamInfo> {
        vec![]
    }

    /// Get normalized parameter value (0.0 - 1.0)
    fn get_param(&self, id: u32) -> f64;

    /// Set normalized parameter value (0.0 - 1.0)
    fn set_param(&mut self, id: u32, value: f64);

    // === MIDI Events ===

    /// Called when a MIDI note on is received.
    fn note_on(&mut self, _sample_offset: u32, _channel: i16, _pitch: i16, _velocity: f32) {}

    /// Called when a MIDI note off is received.
    fn note_off(&mut self, _sample_offset: u32, _channel: i16, _pitch: i16, _velocity: f32) {}

    /// Per-note expression for a note previously delivered by
    /// [`Plugin::note_on`].
    ///
    /// VST3 identifies the target note only by its `note_id`, so an
    /// implementation that needs the channel and key must remember them from
    /// the note-on event.
    fn note_expression(&mut self, _sample_offset: u32, _type_id: u32, _note_id: i32, _value: f64) {}

    // === Processing ===

    /// Process audio
    fn process(&mut self, ctx: &mut crate::ProcessContext) -> crate::ProcessResult;
}

/// Derive a stable VST3 class ID from a persistent textual identifier.
///
/// Changing `value` changes the resulting class ID, so published plugins should keep the input
/// stable or override [`Plugin::class_id`] with an explicit ID.
pub fn class_id_from_str(value: &str) -> [i8; 16] {
    const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;
    const SEEDS: [u64; 2] = [0xCBF2_9CE4_8422_2325, 0x8422_2325_CBF2_9CE4];

    let mut result = [0i8; 16];
    for (chunk, seed) in result.chunks_exact_mut(8).zip(SEEDS) {
        let mut hash = seed;
        for &byte in value.as_bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        for (dst, byte) in chunk.iter_mut().zip(hash.to_be_bytes()) {
            *dst = byte as i8;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{ParameterBridge, ProcessModes, RenderMode, class_id_from_str};
    use crate::ParamInfo;
    use std::sync::Arc;

    #[test]
    fn generated_class_ids_are_stable_and_plugin_specific() {
        let gain = class_id_from_str("com.sunmao.gain");
        assert_eq!(gain, class_id_from_str("com.sunmao.gain"));
        assert_ne!(gain, class_id_from_str("com.sunmao.synth"));
        assert_ne!(gain, [0; 16]);
    }

    #[test]
    fn linked_parameter_bridges_are_isolated_per_instance() {
        let params = [ParamInfo::new(42, "Gain").default(0.25)];
        let processor_a = ParameterBridge::new(&params);
        let controller_a = ParameterBridge::new(&params);
        let processor_b = ParameterBridge::new(&params);
        let controller_b = ParameterBridge::new(&params);

        assert!(controller_a.link_to(&processor_a));
        assert!(controller_b.link_to(&processor_b));
        assert!(controller_a.set(42, 0.8));

        assert_eq!(processor_a.get(42), 0.8);
        assert_eq!(processor_b.get(42), 0.25);
        assert_eq!(controller_b.get(42), 0.25);
    }

    #[test]
    fn parameter_bridge_disconnects_and_reconnects_to_another_processor() {
        let params = [ParamInfo::new(42, "Gain").default(0.25)];
        let processor_a = ParameterBridge::new(&params);
        let processor_b = ParameterBridge::new(&params);
        let controller = ParameterBridge::new(&params);

        assert!(controller.link_to(&processor_a));
        assert!(controller.set(42, 0.8));
        assert_eq!(processor_a.get(42), 0.8);

        assert!(controller.disconnect_from(&processor_a));
        assert_eq!(controller.get(42), 0.8);
        assert!(controller.set(42, 0.6));
        assert_eq!(processor_a.get(42), 0.8);

        assert!(processor_b.set(42, 0.4));
        assert!(controller.link_to(&processor_b));
        assert_eq!(controller.get(42), 0.4);
        assert!(controller.set(42, 0.9));
        assert_eq!(processor_a.get(42), 0.8);
        assert_eq!(processor_b.get(42), 0.9);
        assert!(!controller.disconnect_from(&processor_a));
        assert_eq!(controller.get(42), 0.9);
    }

    #[test]
    fn concurrent_parameter_access_survives_repeated_retargeting() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::thread;

        let params = [ParamInfo::new(42, "Gain").default(0.25)];
        let processor_a = Arc::new(ParameterBridge::new(&params));
        let processor_b = Arc::new(ParameterBridge::new(&params));
        let controller = Arc::new(ParameterBridge::new(&params));
        let running = Arc::new(AtomicBool::new(true));

        assert!(controller.link_to(&processor_a));

        let reader = {
            let controller = controller.clone();
            let running = running.clone();
            thread::spawn(move || {
                while running.load(Ordering::Acquire) {
                    let value = controller.get(42);
                    assert!(value.is_finite() && (0.0..=1.0).contains(&value));
                    let _ = controller.generation();
                }
            })
        };
        let writer = {
            let controller = controller.clone();
            let running = running.clone();
            thread::spawn(move || {
                let mut value = 0.0;
                while running.load(Ordering::Acquire) {
                    assert!(controller.set(42, value));
                    value = if value == 0.0 { 1.0 } else { 0.0 };
                }
            })
        };

        for _ in 0..10_000 {
            assert!(controller.disconnect_from(&processor_a));
            assert!(controller.link_to(&processor_b));
            assert!(controller.disconnect_from(&processor_b));
            assert!(controller.link_to(&processor_a));
        }

        running.store(false, Ordering::Release);
        reader.join().unwrap();
        writer.join().unwrap();

        assert!(controller.link_to(&processor_b));
        assert!(controller.set(42, 0.73));
        assert_eq!(processor_b.get(42), 0.73);
        assert!(controller.disconnect_from(&processor_b));
        assert!(controller.set(42, 0.91));
        assert_eq!(controller.get(42), 0.91);
        assert_eq!(processor_b.get(42), 0.73);
    }

    #[test]
    fn only_the_offline_process_mode_maps_to_offline_rendering() {
        assert_eq!(
            RenderMode::from_process_mode(ProcessModes::kOffline),
            RenderMode::Offline
        );
        assert_eq!(
            RenderMode::from_process_mode(ProcessModes::kRealtime),
            RenderMode::Realtime
        );
        // Prefetch still runs under realtime rules.
        assert_eq!(
            RenderMode::from_process_mode(ProcessModes::kPrefetch),
            RenderMode::Realtime
        );
        // An unknown mode falls back to the stricter contract.
        assert_eq!(RenderMode::from_process_mode(9_999), RenderMode::Realtime);
    }
}
