//! CLAP Backend Adapter for SunMao.
//!
//! This crate wraps `SunmaoPlugin` to expose it as a CLAP plugin via `clap_rs`.

use clap_rs::clap_sys::id::CLAP_INVALID_ID;
use clap_rs::ext::audio_ports_config::AudioPortsConfig as ClapAudioPortsConfig;
use clap_rs::ext::gui::{GuiApi, GuiHandler, GuiResizeHints};
use clap_rs::ext::preset_load::PresetLocation as ClapPresetLocation;
use clap_rs::ext::voice_info::VoiceInfo as ClapVoiceInfo;
use clap_rs::ext::RenderMode;
use clap_rs::gui::prepare_view;
use clap_rs::process::{
    ProcessContext, MAX_PROCESS_AUDIO_SAMPLES, MAX_PROCESS_CHANNELS, MAX_PROCESS_EVENTS,
    MAX_PROCESS_FRAMES,
};
use clap_rs::{
    clap_sys::process::clap_process_status, events::Event as ClapEvent, AudioPortInfo,
    AudioProcessor, HostHandle, NotePortInfo, ParameterInfo, Plugin,
};
use raw_window_handle::{AppKitWindowHandle, RawWindowHandle, Win32WindowHandle, XlibWindowHandle};
use std::ffi::c_void;
use std::num::NonZeroIsize;
use std::ptr::NonNull;
use std::sync::Arc;
use sunmao_core::events::{
    MidiMessage, NoteExpression as SunmaoNoteExpression,
    NoteExpressionKind as SunmaoNoteExpressionKind,
};
use sunmao_core::plugin::{
    BusInfo as SunmaoBusInfo, BusRole as SunmaoBusRole, PresetLocation as SunmaoPresetLocation,
    ProcessContext as SunmaoProcessContext, RenderMode as SunmaoRenderMode,
    TailLength as SunmaoTailLength,
};
use sunmao_core::view::ViewContext;
use sunmao_core::{
    AudioBuffer, Event as SunmaoEvent, EventQueue, ParamChange, ParamDescriptor, Params,
    SunmaoPlugin,
};
use sunmao_core::{ParentWindow, SunmaoView, ViewHandle};

pub use clap_rs::{export_clap_plugin, export_clap_plugin_with_gui, PluginInfo};

/// Maps declared SunMao buses onto CLAP audio ports.
///
/// CLAP has no aux port type: a sidechain is an ordinary port that is simply
/// not flagged as main, so `BusRole` maps onto `is_main`. Shared by the live
/// port list and each selectable layout so the two cannot describe the same
/// bus differently.
fn clap_ports_for(inputs: &[SunmaoBusInfo], outputs: &[SunmaoBusInfo]) -> Vec<AudioPortInfo> {
    let mut ports = Vec::new();
    for (buses, is_input) in [(inputs, true), (outputs, false)] {
        for bus in buses.iter().filter(|bus| bus.channels > 0) {
            ports.push(AudioPortInfo {
                id: ports.len() as u32,
                name: bus.name.to_string(),
                channel_count: bus.channels,
                is_main: bus.role == SunmaoBusRole::Main,
                is_input,
            });
        }
    }
    ports
}

/// Total channel count carried by a set of declared buses.
fn total_bus_channels(buses: &[SunmaoBusInfo]) -> u32 {
    buses.iter().map(|bus| bus.channels).sum()
}

/// First channel index of each bus, plus a trailing end marker, for
/// `AudioBuffer::with_input_bus_bounds`.
fn bus_bounds(buses: &[SunmaoBusInfo]) -> Vec<usize> {
    if buses.is_empty() {
        return Vec::new();
    }
    let mut bounds = Vec::with_capacity(buses.len() + 1);
    let mut offset = 0usize;
    bounds.push(offset);
    for bus in buses {
        offset += bus.channels as usize;
        bounds.push(offset);
    }
    bounds
}

/// CLAP treats any tail at or above `i32::MAX` samples as unbounded.
const CLAP_INFINITE_TAIL: u32 = i32::MAX as u32;

/// Encodes a unified tail length for CLAP, keeping a finite tail strictly
/// below the threshold that the format reads as unbounded.
fn clamp_clap_tail(tail: SunmaoTailLength) -> u32 {
    match tail {
        SunmaoTailLength::None => 0,
        SunmaoTailLength::Samples(samples) => samples.min(CLAP_INFINITE_TAIL - 1),
        SunmaoTailLength::Infinite => CLAP_INFINITE_TAIL,
    }
}

const DEFAULT_EFFECT_FEATURES: &[&str] = &["audio-effect"];
const DEFAULT_SYNTH_FEATURES: &[&str] = &["instrument", "synthesizer"];

/// Resolve the default CLAP features without requiring format-specific code in
/// a minimal plugin implementation.
#[doc(hidden)]
pub fn default_clap_features<P: SunmaoPlugin>() -> &'static [&'static str] {
    if P::default().input_channels() == 0 {
        DEFAULT_SYNTH_FEATURES
    } else {
        DEFAULT_EFFECT_FEATURES
    }
}

#[doc(hidden)]
pub mod __private {
    pub use clap_rs;
    pub use sunmao_core;
}

#[doc(hidden)]
#[macro_export]
macro_rules! __export_sunmao_clap_plugin {
    ($plugin_type:ty, $entry_type:ident) => {
        mod __sunmao_clap_impl {
            use super::*;
            use std::ffi::{c_char, c_void, CStr, CString};
            use std::sync::OnceLock;

            use $crate::__private::clap_rs::clap_sys;

            struct OwnedDescriptor {
                descriptor: clap_sys::plugin::clap_plugin_descriptor_t,
                _id: CString,
                _name: CString,
                _vendor: CString,
                _url: CString,
                _manual_url: CString,
                _support_url: CString,
                _version: CString,
                _description: CString,
                _feature_strings: Vec<CString>,
                _feature_ptrs: Vec<*const c_char>,
            }

            unsafe impl Send for OwnedDescriptor {}
            unsafe impl Sync for OwnedDescriptor {}

            fn c_string(value: &str) -> Option<CString> {
                CString::new(value).ok()
            }

            fn owned_descriptor() -> Option<&'static OwnedDescriptor> {
                // Metadata is supplied by the plugin author, so descriptor
                // construction must remain fallible at the C ABI boundary.
                // Cache the failure as `None` to make every subsequent query
                // deterministic without repeatedly allocating or panicking.
                static DESCRIPTOR: OnceLock<Option<OwnedDescriptor>> = OnceLock::new();
                DESCRIPTOR
                    .get_or_init(|| {
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            let format_info = <$plugin_type as $crate::__private::sunmao_core::SunmaoPlugin>::clap_info();
                            let resolved_id = if format_info.id.is_empty() {
                                $crate::__private::sunmao_core::derive_clap_id(
                                    <$plugin_type as $crate::__private::sunmao_core::SunmaoPlugin>::VENDOR,
                                    <$plugin_type as $crate::__private::sunmao_core::SunmaoPlugin>::NAME,
                                )
                            } else {
                                format_info.id.to_owned()
                            };
                            let id = c_string(&resolved_id)?;
                            let name = c_string(
                                <$plugin_type as $crate::__private::sunmao_core::SunmaoPlugin>::NAME,
                            )?;
                            let vendor = c_string(
                                <$plugin_type as $crate::__private::sunmao_core::SunmaoPlugin>::VENDOR,
                            )?;
                            let url = c_string(
                                <$plugin_type as $crate::__private::sunmao_core::SunmaoPlugin>::URL,
                            )?;
                            let manual_url = c_string("")?;
                            let support_url = c_string("")?;
                            let version = c_string(
                                <$plugin_type as $crate::__private::sunmao_core::SunmaoPlugin>::VERSION,
                            )?;
                            let description = c_string(
                                <$plugin_type as $crate::__private::sunmao_core::SunmaoPlugin>::NAME,
                            )?;
                            let resolved_features = if format_info.features.is_empty() {
                                $crate::default_clap_features::<$plugin_type>()
                            } else {
                                format_info.features
                            };
                            let feature_strings: Vec<CString> = resolved_features
                                .iter()
                                .map(|feature| c_string(feature))
                                .collect::<Option<Vec<_>>>()?;
                    let mut feature_ptrs: Vec<*const c_char> = feature_strings
                        .iter()
                        .map(|feature| feature.as_ptr())
                        .collect();
                    feature_ptrs.push(std::ptr::null());

                    let descriptor = clap_sys::plugin::clap_plugin_descriptor_t {
                        clap_version: $crate::__private::clap_rs::CLAP_VERSION,
                        id: id.as_ptr(),
                        name: name.as_ptr(),
                        vendor: vendor.as_ptr(),
                        url: url.as_ptr(),
                        manual_url: manual_url.as_ptr(),
                        support_url: support_url.as_ptr(),
                        version: version.as_ptr(),
                        description: description.as_ptr(),
                        features: feature_ptrs.as_ptr(),
                    };

                            Some(OwnedDescriptor {
                        descriptor,
                        _id: id,
                        _name: name,
                        _vendor: vendor,
                        _url: url,
                        _manual_url: manual_url,
                        _support_url: support_url,
                        _version: version,
                        _description: description,
                        _feature_strings: feature_strings,
                        _feature_ptrs: feature_ptrs,
                            })
                        }))
                        .ok()
                        .flatten()
                    })
                    .as_ref()
            }

            fn descriptor() -> Option<&'static clap_sys::plugin::clap_plugin_descriptor_t> {
                owned_descriptor().map(|descriptor| &descriptor.descriptor)
            }

            #[repr(transparent)]
            struct SyncFactory(clap_sys::factory::plugin_factory::clap_plugin_factory_t);
            unsafe impl Send for SyncFactory {}
            unsafe impl Sync for SyncFactory {}

            static FACTORY: SyncFactory =
                SyncFactory(clap_sys::factory::plugin_factory::clap_plugin_factory_t {
                    get_plugin_count: Some(get_plugin_count),
                    get_plugin_descriptor: Some(get_plugin_descriptor),
                    create_plugin: Some(create_plugin),
                });

            unsafe extern "C" fn get_plugin_count(
                _factory: *const clap_sys::factory::plugin_factory::clap_plugin_factory_t,
            ) -> u32 {
                u32::from(descriptor().is_some())
            }

            unsafe extern "C" fn get_plugin_descriptor(
                _factory: *const clap_sys::factory::plugin_factory::clap_plugin_factory_t,
                index: u32,
            ) -> *const clap_sys::plugin::clap_plugin_descriptor_t {
                if index == 0 {
                    descriptor().map_or(std::ptr::null(), |descriptor| descriptor as *const _)
                } else {
                    std::ptr::null()
                }
            }

            unsafe extern "C" fn create_plugin(
                _factory: *const clap_sys::factory::plugin_factory::clap_plugin_factory_t,
                host: *const clap_sys::host::clap_host_t,
                plugin_id: *const c_char,
            ) -> *const clap_sys::plugin::clap_plugin_t {
                if plugin_id.is_null() {
                    return std::ptr::null();
                }
                let Some(descriptor) = descriptor() else {
                    return std::ptr::null();
                };
                if CStr::from_ptr(plugin_id) != CStr::from_ptr(descriptor.id) {
                    return std::ptr::null();
                }
                $crate::__private::clap_rs::entry::$entry_type::create_plugin::<
                    $crate::SunmaoClapWrapper<$plugin_type>,
                >(host, descriptor)
            }

            unsafe extern "C" fn entry_init(_path: *const c_char) -> bool {
                descriptor().is_some()
            }

            unsafe extern "C" fn entry_deinit() {}

            unsafe extern "C" fn entry_get_factory(factory_id: *const c_char) -> *const c_void {
                if factory_id.is_null() {
                    return std::ptr::null();
                }
                let expected = clap_sys::factory::plugin_factory::CLAP_PLUGIN_FACTORY_ID;
                if CStr::from_ptr(factory_id).to_bytes_with_nul() == expected.as_bytes() {
                    &FACTORY.0 as *const _ as *const c_void
                } else {
                    std::ptr::null()
                }
            }

            #[unsafe(no_mangle)]
            pub static clap_entry: clap_sys::entry::clap_plugin_entry_t =
                clap_sys::entry::clap_plugin_entry_t {
                    clap_version: $crate::__private::clap_rs::CLAP_VERSION,
                    init: Some(entry_init),
                    deinit: Some(entry_deinit),
                    get_factory: Some(entry_get_factory),
                };
        }
    };
}

/// Export a parameter-only SunMao plugin as CLAP.
#[macro_export]
macro_rules! export_sunmao_clap_plugin {
    ($plugin_type:ty) => {
        $crate::__export_sunmao_clap_plugin!($plugin_type, PluginEntry);
    };
}

/// Export a SunMao plugin with its GUI as CLAP.
#[macro_export]
macro_rules! export_sunmao_clap_plugin_with_gui {
    ($plugin_type:ty) => {
        $crate::__export_sunmao_clap_plugin!($plugin_type, PluginEntryWithGui);
    };
}

/// GUI handles remain exclusively owned by the main-thread controller.
struct MainThreadViewHandle {
    handle: ViewHandle,
}

fn is_native_gui_api(api: GuiApi) -> bool {
    #[cfg(target_os = "macos")]
    return api == GuiApi::Cocoa;

    #[cfg(target_os = "windows")]
    return api == GuiApi::Win32;

    #[cfg(target_os = "linux")]
    return api == GuiApi::X11;

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    return false;
}

fn parameter_to_clap_value(descriptor: &ParamDescriptor, normalized: f64) -> f64 {
    let normalized = if normalized.is_finite() {
        normalized.clamp(0.0, 1.0)
    } else {
        descriptor.default_normalized as f64
    };
    if descriptor.step_count > 0 {
        (normalized * descriptor.step_count as f64).round()
    } else {
        normalized
    }
}

fn parameter_from_clap_value(descriptor: &ParamDescriptor, value: f64) -> Option<f32> {
    if !value.is_finite() {
        return None;
    }
    if descriptor.step_count > 0 {
        let steps = descriptor.step_count as f64;
        Some((value.round().clamp(0.0, steps) / steps) as f32)
    } else {
        Some(value.clamp(0.0, 1.0) as f32)
    }
}

/// Map a host event timestamp to a sample offset that the core contract can
/// safely consume. CLAP requires timestamps to be inside the current block,
/// but a malformed host must not be able to make a user plugin index past the
/// active audio buffer.
fn event_sample_offset(time: u32, frames: usize) -> u32 {
    if frames == 0 {
        0
    } else {
        time.min(frames.saturating_sub(1) as u32)
    }
}

fn timed_parameter_change(
    descriptors: &[ParamDescriptor],
    event: &clap_rs::events::ParamValueEvent,
    frames: usize,
) -> Option<ParamChange> {
    let descriptor = descriptors
        .iter()
        .find(|descriptor| descriptor.numeric_id == event.param_id)?;
    let value = parameter_from_clap_value(descriptor, event.value)?;
    Some(ParamChange {
        id: descriptor.id,
        value,
        offset: event_sample_offset(event.time, frames),
    })
}

/// Converts a CLAP parameter modulation, dropping unknown parameters exactly
/// like an out-of-range automation event.
fn timed_parameter_mod(
    descriptors: &[ParamDescriptor],
    event: &clap_rs::events::ParamModEvent,
    frames: usize,
) -> Option<(&'static str, f32, u32)> {
    let descriptor = descriptors
        .iter()
        .find(|descriptor| descriptor.numeric_id == event.param_id)?;
    // A modulation is an offset, not a value, so it is not normalized against
    // the parameter range; only non-finite amounts are rejected.
    let amount = event.amount as f32;
    amount.is_finite().then(|| {
        (
            descriptor.id,
            amount,
            event_sample_offset(event.time, frames),
        )
    })
}

/// Converts a CLAP note expression. An unnamed dimension keeps its raw id so
/// the plugin still sees the event.
fn note_expression_to_sunmao(
    event: &clap_rs::events::NoteExpressionEvent,
    frames: usize,
) -> Option<SunmaoNoteExpression> {
    if event.port_index > 0 || !event.value.is_finite() {
        return None;
    }
    let channel = u8::try_from(event.channel.max(0)).ok()?;
    let key = u8::try_from(event.key.max(0)).ok()?;
    if channel > 15 || key > 127 {
        return None;
    }
    let kind = match event.kind {
        Some(clap_rs::events::NoteExpressionKind::Volume) => SunmaoNoteExpressionKind::Volume,
        Some(clap_rs::events::NoteExpressionKind::Pan) => SunmaoNoteExpressionKind::Pan,
        Some(clap_rs::events::NoteExpressionKind::Tuning) => SunmaoNoteExpressionKind::Tuning,
        Some(clap_rs::events::NoteExpressionKind::Vibrato) => SunmaoNoteExpressionKind::Vibrato,
        Some(clap_rs::events::NoteExpressionKind::Expression) => {
            SunmaoNoteExpressionKind::Expression
        }
        Some(clap_rs::events::NoteExpressionKind::Brightness) => {
            SunmaoNoteExpressionKind::Brightness
        }
        Some(clap_rs::events::NoteExpressionKind::Pressure) => SunmaoNoteExpressionKind::Pressure,
        None => SunmaoNoteExpressionKind::Unknown(event.raw_kind),
    };
    Some(SunmaoNoteExpression {
        offset: event_sample_offset(event.time, frames),
        kind,
        note_id: (event.note_id >= 0).then_some(event.note_id),
        channel: Some(channel),
        key: Some(key),
        value: event.value,
    })
}

fn note_event_to_midi(
    note: &clap_rs::events::NoteEvent,
    frames: usize,
    note_on: bool,
) -> Option<MidiMessage> {
    // A single SunMao note port is exposed. CLAP permits -1 for an
    // unspecified port, which is equivalent to the only port here; any other
    // port cannot be represented without silently routing to the wrong input.
    if !matches!(note.port_index, -1 | 0)
        || !(0..=15).contains(&note.channel)
        || !(0..=127).contains(&note.key)
        || !note.velocity.is_finite()
    {
        return None;
    }
    let velocity = (note.velocity.clamp(0.0, 1.0) * 127.0).round() as u8;
    let offset = event_sample_offset(note.time, frames);
    Some(if note_on {
        MidiMessage::note_on(offset, note.channel as u8, note.key as u8, velocity)
    } else {
        MidiMessage::note_off(offset, note.channel as u8, note.key as u8, velocity)
    })
}

fn prepare_output_buffers(
    input_buffers: &[Vec<f32>],
    output_buffers: &mut [Vec<f32>],
    frames: usize,
    passthrough: bool,
) {
    for (channel, output) in output_buffers.iter_mut().enumerate() {
        let output_len = frames.min(output.len());
        let output = &mut output[..output_len];
        let mut copied = 0;
        if passthrough {
            if let Some(input) = input_buffers.get(channel) {
                copied = output.len().min(input.len());
                output[..copied].copy_from_slice(&input[..copied]);
            }
        }
        output[copied..].fill(0.0);
    }
}

/// Copy one host input channel into activation-owned scratch without retaining
/// samples from a previous block when the host provides a short or missing
/// channel.
fn copy_input_buffer(dst: &mut [f32], src: &[f32], frames: usize) {
    let active_len = frames.min(dst.len());
    dst[..active_len].fill(0.0);
    let copy_len = active_len.min(src.len());
    dst[..copy_len].copy_from_slice(&src[..copy_len]);
}

fn allocate_audio_buffers(channel_count: u32, max_frames: u32) -> Option<Vec<Vec<f32>>> {
    let channel_count = usize::try_from(channel_count).ok()?;
    let max_frames = usize::try_from(max_frames).ok()?;
    if max_frames > MAX_PROCESS_FRAMES as usize
        || channel_count > MAX_PROCESS_CHANNELS
        || channel_count
            .checked_mul(max_frames)
            .is_none_or(|samples| samples > MAX_PROCESS_AUDIO_SAMPLES)
    {
        return None;
    }
    let mut buffers = Vec::new();
    buffers.try_reserve_exact(channel_count).ok()?;
    for _ in 0..channel_count {
        let mut buffer = Vec::new();
        buffer.try_reserve_exact(max_frames).ok()?;
        buffer.resize(max_frames, 0.0);
        buffers.push(buffer);
    }
    Some(buffers)
}

/// Wrapper that adapts a SunmaoPlugin to clap_rs::Plugin.
pub struct SunmaoClapWrapper<P: SunmaoPlugin> {
    plugin: Option<P>,
    params: Arc<P::Params>,
    view: Option<Box<dyn SunmaoView>>,
    input_channels: u32,
    output_channels: u32,
    input_buses: Vec<SunmaoBusInfo>,
    output_buses: Vec<SunmaoBusInfo>,
    accepts_midi: bool,
    /// Latency and tail captured while the plugin was last reachable.
    ///
    /// `activate` moves the plugin into the audio processor, so between
    /// activate and deactivate `self.plugin` is `None` — which is exactly when
    /// a host queries `clap.latency`/`clap.tail`. Reporting 0 there would tell
    /// the host to align the track by nothing and let a tail be cut off, so the
    /// last known values stand in instead.
    active_latency: u32,
    active_tail: u32,
    // GUI State
    view_handle: Option<MainThreadViewHandle>,
    gui_api: Option<GuiApi>,
    host: HostHandle,
    param_descriptors: Vec<ParamDescriptor>,
}

struct ClapParamsViewContext<P: Params> {
    params: Arc<P>,
    host: HostHandle,
    descriptors: Vec<ParamDescriptor>,
}

impl<P: Params> ViewContext for ClapParamsViewContext<P> {
    fn get_param(&self, id: &str) -> Option<f32> {
        self.params.get_normalized(id)
    }

    fn set_param(&self, id: &str, value: f32) {
        let Some(descriptor) = self
            .descriptors
            .iter()
            .find(|descriptor| descriptor.id == id)
        else {
            return;
        };
        self.params.set_normalized(id, value);
        let normalized = self
            .params
            .get_normalized(id)
            .unwrap_or(value.clamp(0.0, 1.0));
        self.host.perform_edit(
            descriptor.numeric_id,
            parameter_to_clap_value(descriptor, normalized as f64),
        );
    }

    fn begin_edit(&self, id: &str) {
        if let Some(descriptor) = self
            .descriptors
            .iter()
            .find(|descriptor| descriptor.id == id)
        {
            self.host.begin_edit(descriptor.numeric_id);
        }
    }

    fn end_edit(&self, id: &str) {
        if let Some(descriptor) = self
            .descriptors
            .iter()
            .find(|descriptor| descriptor.id == id)
        {
            self.host.end_edit(descriptor.numeric_id);
        }
    }

    fn request_resize(&self, width: u32, height: u32) -> bool {
        self.host.request_resize(width, height)
    }
}

pub struct SunmaoClapProcessor<P: SunmaoPlugin> {
    plugin: P,
    params: Arc<P::Params>,
    param_descriptors: Vec<ParamDescriptor>,
    sample_rate: f64,
    is_synth: bool,
    accepts_midi: bool,
    input_buffers: Vec<Vec<f32>>,
    output_buffers: Vec<Vec<f32>>,
    /// Precomputed at activation so the audio thread never rebuilds it.
    input_bus_bounds: Vec<usize>,
    event_queue: EventQueue,
}

impl<P: SunmaoPlugin> Plugin for SunmaoClapWrapper<P> {
    type AudioProcessor = SunmaoClapProcessor<P>;

    fn new(host: HostHandle) -> Self {
        let plugin = P::default();
        let params = plugin.params();
        let param_descriptors = params
            .validated_descriptors()
            .unwrap_or_else(|error| panic!("invalid SunMao parameter layout: {error}"));
        // The declared buses are authoritative: the channel totals used for
        // scratch allocation are their sum, so a plugin that adds a sidechain
        // only has to override `input_buses`.
        let input_buses = plugin.input_buses();
        let output_buses = plugin.output_buses();
        let input_channels = total_bus_channels(&input_buses);
        let output_channels = total_bus_channels(&output_buses);
        let accepts_midi = plugin.accepts_midi();
        let view = plugin.view();

        Self {
            plugin: Some(plugin),
            params,
            view,
            input_channels,
            output_channels,
            input_buses,
            output_buses,
            accepts_midi,
            active_latency: 0,
            active_tail: 0,
            view_handle: None,
            gui_api: None,
            host,
            param_descriptors,
        }
    }

    fn activate(
        &mut self,
        sample_rate: f64,
        min_frames: u32,
        max_frames: u32,
    ) -> Option<Self::AudioProcessor> {
        if !sample_rate.is_finite()
            || sample_rate <= 0.0
            || min_frames > max_frames
            || max_frames > MAX_PROCESS_FRAMES
            || P::MAX_EVENTS_PER_BLOCK > MAX_PROCESS_EVENTS
        {
            return None;
        }
        let mut plugin = self.plugin.take()?;
        let Some(input_buffers) = allocate_audio_buffers(self.input_channels, max_frames) else {
            self.plugin = Some(plugin);
            return None;
        };
        let Some(output_buffers) = allocate_audio_buffers(self.output_channels, max_frames) else {
            self.plugin = Some(plugin);
            return None;
        };
        let Ok(event_queue) = EventQueue::try_with_capacity(P::MAX_EVENTS_PER_BLOCK) else {
            // Keep ownership available for a later activation attempt. In
            // particular, do not drop the plugin after a hostile or invalid
            // MAX_EVENTS_PER_BLOCK causes a fallible scratch allocation to
            // fail.
            self.plugin = Some(plugin);
            return None;
        };
        // Initialization is user code. Keep ownership recoverable if it
        // panics while the generic CLAP lifecycle guard converts the failure
        // to a rejected activation.
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            plugin.initialize(sample_rate, max_frames);
        }))
        .is_err()
        {
            self.plugin = Some(plugin);
            return None;
        }
        // Last chance to read these before ownership moves to the processor:
        // `initialize` has run, so they reflect the activated sample rate.
        self.active_latency = plugin.latency_samples();
        self.active_tail = clamp_clap_tail(plugin.tail());
        Some(SunmaoClapProcessor {
            plugin,
            params: self.params.clone(),
            param_descriptors: self.param_descriptors.clone(),
            sample_rate,
            input_bus_bounds: bus_bounds(&self.input_buses),
            is_synth: self.input_channels == 0,
            accepts_midi: self.accepts_midi,
            input_buffers,
            output_buffers,
            event_queue,
        })
    }

    fn deactivate(&mut self, mut processor: Self::AudioProcessor) {
        // A reset panic must not consume the processor before ownership is
        // returned to the controller. The host has no error return for
        // deactivate, so containment here is the only way to keep destroy and
        // a later lifecycle transition memory-safe.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            processor.plugin.reset();
        }));
        self.plugin = Some(processor.plugin);
    }

    fn latency(&self) -> u32 {
        self.plugin
            .as_ref()
            .map(|plugin| plugin.latency_samples())
            .unwrap_or(self.active_latency)
    }

    fn tail(&self) -> u32 {
        self.plugin
            .as_ref()
            .map(|plugin| clamp_clap_tail(plugin.tail()))
            .unwrap_or(self.active_tail)
    }

    const STATE_VERSION: u32 = P::STATE_VERSION;
    const SUPPORTS_PRESET_LOAD: bool = P::SUPPORTS_PRESET_LOAD;

    fn load_preset(&mut self, location: ClapPresetLocation<'_>) -> bool {
        // The two enums are the same shape; the backend only re-labels them so
        // plugins never see a clap_rs type.
        let location = match location {
            ClapPresetLocation::File { path, key } => SunmaoPresetLocation::File { path, key },
            ClapPresetLocation::Internal { key } => SunmaoPresetLocation::Internal { key },
        };
        self.plugin
            .as_mut()
            .map(|plugin| plugin.load_preset(location))
            .unwrap_or(false)
    }

    fn state_loaded(&mut self, from_version: u32) {
        // clap_rs only calls this once every value from the older blob has been
        // applied, so the plugin migrates from a complete state.
        if let Some(plugin) = self.plugin.as_mut() {
            plugin.migrate_state(from_version);
        }
    }

    fn set_audio_port_active(
        &mut self,
        is_input: bool,
        port_index: u32,
        is_active: bool,
        _sample_size: u32,
    ) -> bool {
        // CLAP declares one port per SunMao bus in declaration order, so the
        // port index is the bus index. `sample_size` is dropped: SunMao is
        // f32-only, and clap_rs already rejects other widths at activation.
        self.plugin
            .as_mut()
            .map(|plugin| plugin.set_bus_active(is_input, port_index, is_active))
            .unwrap_or(false)
    }

    fn voice_info(&self) -> Option<ClapVoiceInfo> {
        let info = self.plugin.as_ref()?.voice_info()?;
        Some(ClapVoiceInfo {
            voice_count: info.active,
            voice_capacity: info.capacity,
            supports_overlapping_notes: info.supports_overlapping_notes,
        })
    }

    fn set_render_mode(&mut self, mode: RenderMode) -> bool {
        let Some(plugin) = self.plugin.as_mut() else {
            return false;
        };
        let mode = match mode {
            RenderMode::Realtime => SunmaoRenderMode::Realtime,
            RenderMode::Offline => SunmaoRenderMode::Offline,
        };
        // Render mode switches run on the main thread, but they are still user
        // code reached over the CLAP ABI.
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            plugin.set_render_mode(mode);
        }))
        .is_ok()
    }

    fn audio_ports_config(&self) -> Vec<AudioPortInfo> {
        clap_ports_for(&self.input_buses, &self.output_buses)
    }

    fn audio_ports_configs(&self) -> Vec<ClapAudioPortsConfig> {
        // The config id is the index into the plugin's `bus_configs()`, which
        // is also what `select_bus_config` takes — so the id a CLAP host hands
        // back needs no lookup table.
        let Some(plugin) = self.plugin.as_ref() else {
            return Vec::new();
        };
        plugin
            .bus_configs()
            .iter()
            .enumerate()
            .map(|(index, config)| ClapAudioPortsConfig {
                id: index as u32,
                name: config.name.to_string(),
                ports: clap_ports_for(&config.inputs, &config.outputs),
            })
            .collect()
    }

    fn current_audio_ports_config_id(&self) -> u32 {
        self.plugin
            .as_ref()
            .map(|plugin| plugin.current_bus_config() as u32)
            .unwrap_or(CLAP_INVALID_ID)
    }

    fn select_audio_ports_config(&mut self, config_id: u32) -> bool {
        let Some(plugin) = self.plugin.as_mut() else {
            return false;
        };
        if !plugin.select_bus_config(config_id as usize) {
            return false;
        }
        // The plugin now reports a new layout, so refresh every cache derived
        // from the old one — the channel totals feed scratch allocation and the
        // bus lists feed the per-bus buffer bounds.
        self.input_buses = plugin.input_buses();
        self.output_buses = plugin.output_buses();
        self.input_channels = total_bus_channels(&self.input_buses);
        self.output_channels = total_bus_channels(&self.output_buses);
        true
    }

    fn note_ports_config(&self) -> Vec<NotePortInfo> {
        if self.accepts_midi {
            vec![NotePortInfo {
                id: 0,
                name: "MIDI In".to_string(),
                is_input: true,
            }]
        } else {
            vec![]
        }
    }

    fn declare_parameters(&self) -> Vec<ParameterInfo> {
        self.param_descriptors
            .iter()
            .map(|descriptor| ParameterInfo {
                id: descriptor.numeric_id,
                name: descriptor.name.to_string(),
                // CLAP takes the hierarchy as a `/`-separated path per
                // parameter. Normalizing through `group_segments` drops empty
                // segments so a stray slash cannot produce an unnamed level.
                module: sunmao_core::params::group_segments(descriptor.group)
                    .collect::<Vec<_>>()
                    .join("/"),
                min_value: 0.0,
                max_value: descriptor.step_count.max(1) as f64,
                default_value: parameter_to_clap_value(
                    descriptor,
                    descriptor.default_normalized as f64,
                ),
                is_stepped: descriptor.step_count > 0,
            })
            .collect()
    }

    fn get_parameter(&self, id: u32) -> f64 {
        if let Some(descriptor) = self
            .param_descriptors
            .iter()
            .find(|descriptor| descriptor.numeric_id == id)
        {
            parameter_to_clap_value(
                descriptor,
                self.params
                    .get_normalized(descriptor.id)
                    .unwrap_or(descriptor.default_normalized) as f64,
            )
        } else {
            0.0
        }
    }

    fn set_parameter(&mut self, id: u32, value: f64) {
        if let Some(descriptor) = self
            .param_descriptors
            .iter()
            .find(|descriptor| descriptor.numeric_id == id)
        {
            if let Some(normalized) = parameter_from_clap_value(descriptor, value) {
                self.params.set_normalized(descriptor.id, normalized);
            }
        }
    }
}

impl<P: SunmaoPlugin> AudioProcessor for SunmaoClapProcessor<P> {
    fn reset(&mut self) {
        // CLAP reset is delivered while the audio processor is still owned by
        // the audio thread. Forward it to the actual SunMao instance so DSP
        // state (voices, delay lines, etc.) is cleared immediately.
        self.plugin.reset();
        self.event_queue.clear();
    }

    fn process(&mut self, mut ctx: ProcessContext) -> clap_process_status {
        let frames = ctx.frames_count as usize;
        // Channel topology is fixed for an instance. Cache it at activation so
        // a user-defined topology method cannot allocate or panic on the
        // realtime thread.
        let is_synth = self.is_synth;

        let Some(event_count) = ctx.event_count() else {
            return clap_rs::clap_sys::process::CLAP_PROCESS_ERROR;
        };
        if event_count as usize > P::MAX_EVENTS_PER_BLOCK
            || event_count as usize > MAX_PROCESS_EVENTS
        {
            return clap_rs::clap_sys::process::CLAP_PROCESS_ERROR;
        }

        // Preserve CLAP input order so same-offset parameter changes remain deterministic.
        self.event_queue.clear();
        // The note-port extension is omitted for plugins that do not opt into
        // MIDI. Hosts normally respect that declaration, but malformed or
        // older hosts can still send note/MIDI events. Do not let those events
        // cross the format-neutral boundary into a plugin that cannot consume
        // them.
        let accepts_midi = self.accepts_midi;
        for event in ctx.events() {
            match event {
                ClapEvent::NoteOn(note) if accepts_midi => {
                    if let Some(midi) = note_event_to_midi(&note, frames, true) {
                        if !self.event_queue.push(SunmaoEvent::Midi(midi)) {
                            return clap_rs::clap_sys::process::CLAP_PROCESS_ERROR;
                        }
                    }
                }
                ClapEvent::NoteOff(note) if accepts_midi => {
                    if let Some(midi) = note_event_to_midi(&note, frames, false) {
                        if !self.event_queue.push(SunmaoEvent::Midi(midi)) {
                            return clap_rs::clap_sys::process::CLAP_PROCESS_ERROR;
                        }
                    }
                }
                ClapEvent::Midi(midi) if accepts_midi => {
                    // This adapter exposes exactly one MIDI/note port.
                    if midi.port_index == 0 {
                        let msg = MidiMessage {
                            offset: event_sample_offset(midi.time, frames),
                            data: midi.data,
                        };
                        if !self.event_queue.push(SunmaoEvent::Midi(msg)) {
                            return clap_rs::clap_sys::process::CLAP_PROCESS_ERROR;
                        }
                    }
                }
                ClapEvent::ParamValue(param) => {
                    if let Some(change) =
                        timed_parameter_change(&self.param_descriptors, &param, frames)
                    {
                        if !self.event_queue.push_param_change(change) {
                            return clap_rs::clap_sys::process::CLAP_PROCESS_ERROR;
                        }
                    }
                }
                ClapEvent::ParamMod(param) => {
                    if let Some((id, amount, offset)) =
                        timed_parameter_mod(&self.param_descriptors, &param, frames)
                    {
                        if !self
                            .event_queue
                            .push(SunmaoEvent::ParamMod { id, amount, offset })
                        {
                            return clap_rs::clap_sys::process::CLAP_PROCESS_ERROR;
                        }
                    }
                }
                ClapEvent::NoteExpression(expression) if accepts_midi => {
                    if let Some(expression) = note_expression_to_sunmao(&expression, frames) {
                        if !self
                            .event_queue
                            .push(SunmaoEvent::NoteExpression(expression))
                        {
                            return clap_rs::clap_sys::process::CLAP_PROCESS_ERROR;
                        }
                    }
                }
                _ => {}
            }
        }

        // Copy CLAP input to temp buffers. Clear every declared channel first
        // so a short/missing host channel cannot expose a prior block.
        for buffer in &mut self.input_buffers {
            let active_len = frames.min(buffer.len());
            buffer[..active_len].fill(0.0);
        }
        for (ch, input) in ctx.audio_inputs.iter().enumerate() {
            if ch < self.input_buffers.len() {
                copy_input_buffer(&mut self.input_buffers[ch], input, frames);
            }
        }

        // Effects begin with passthrough; synths begin with silence. Channels
        // without matching inputs must also be reset on every block.
        prepare_output_buffers(
            &self.input_buffers,
            &mut self.output_buffers,
            frames,
            !is_synth,
        );

        let mut audio_buffer =
            AudioBuffer::from_planar(&self.input_buffers, &mut self.output_buffers, frames)
                .with_input_bus_bounds(&self.input_bus_bounds);

        // Create process context. A host may omit the transport entirely, in
        // which case every musical field stays absent and the plugin sees a
        // free-running block.
        let mut sunmao_ctx = SunmaoProcessContext {
            sample_rate: self.sample_rate,
            is_playing: true,
            ..Default::default()
        };
        if let Some(transport) = ctx.transport() {
            sunmao_ctx.tempo = transport.tempo();
            sunmao_ctx.is_playing = transport.is_playing();
            sunmao_ctx.is_recording = transport.is_recording();
            sunmao_ctx.is_loop_active = transport.is_loop_active();
            sunmao_ctx.time_signature = transport.time_signature();
            sunmao_ctx.song_pos_beats = transport.song_pos_beats();
            sunmao_ctx.song_pos_seconds = transport.song_pos_seconds();
            sunmao_ctx.bar_start_beats = transport.bar_start_beats();
            sunmao_ctx.bar_number = transport.bar_number();
            sunmao_ctx.loop_beats = transport.loop_beats();
            if let Some(seconds) = transport.song_pos_seconds() {
                sunmao_ctx.sample_pos = (seconds * self.sample_rate) as i64;
            }
        }

        // Call the actual plugin process
        let status = self
            .plugin
            .process(&mut audio_buffer, &self.event_queue, &sunmao_ctx);
        if status == sunmao_core::ProcessStatus::Error {
            return clap_rs::clap_sys::process::CLAP_PROCESS_ERROR;
        }

        // Publish the final automated values only after DSP has consumed the timed events.
        // Iteration order makes the last event win when offsets are equal.
        for change in self.event_queue.param_changes() {
            self.params.set_normalized(change.id, change.value);
        }

        // Copy output back to CLAP buffers
        for (ch, output) in ctx.audio_outputs.iter_mut().enumerate() {
            if ch < self.output_buffers.len() {
                let len = frames.min(output.len()).min(self.output_buffers[ch].len());
                output[..len].copy_from_slice(&self.output_buffers[ch][..len]);
            }
        }

        clap_rs::CLAP_PROCESS_CONTINUE
    }

    fn set_parameter(&mut self, id: u32, value: f64) {
        if let Some(descriptor) = self
            .param_descriptors
            .iter()
            .find(|descriptor| descriptor.numeric_id == id)
        {
            if let Some(normalized) = parameter_from_clap_value(descriptor, value) {
                self.params.set_normalized(descriptor.id, normalized);
            }
        }
    }
}

impl<P: SunmaoPlugin> GuiHandler for SunmaoClapWrapper<P> {
    fn is_api_supported(&self, api: GuiApi, is_floating: bool) -> bool {
        if is_floating {
            return false;
        }
        is_native_gui_api(api)
    }

    fn preferred_api(&self) -> Option<(GuiApi, bool)> {
        #[cfg(target_os = "macos")]
        {
            Some((GuiApi::Cocoa, false))
        }
        #[cfg(target_os = "windows")]
        {
            Some((GuiApi::Win32, false))
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            Some((GuiApi::X11, false))
        }
    }

    fn gui_create(&mut self, api: GuiApi, is_floating: bool) -> bool {
        if is_floating || !self.is_api_supported(api, is_floating) {
            return false;
        }
        if self.view.is_none() {
            return false;
        }
        self.gui_api = Some(api);
        true
    }

    fn gui_destroy(&mut self) {
        self.view_handle = None;
        self.gui_api = None;
    }

    fn gui_get_size(&self) -> Option<(u32, u32)> {
        self.view.as_ref().map(|view| view.size())
    }

    fn gui_can_resize(&self) -> bool {
        self.view
            .as_ref()
            .map(|view| view.can_resize())
            .unwrap_or(false)
    }

    fn gui_get_resize_hints(&self) -> GuiResizeHints {
        GuiResizeHints {
            can_resize_horizontally: self.gui_can_resize(),
            can_resize_vertically: self.gui_can_resize(),
            ..GuiResizeHints::default()
        }
    }

    fn gui_set_size(&mut self, width: u32, height: u32) -> bool {
        self.view_handle
            .as_mut()
            .map(|handle| handle.handle.resize(width, height))
            .unwrap_or(false)
    }

    fn gui_set_parent(&mut self, window: *mut c_void) -> bool {
        if let Some(view) = self.view.as_ref() {
            let mut raw_handle = match self.gui_api {
                Some(GuiApi::Cocoa) => {
                    let Some(ns_view) = NonNull::new(window) else {
                        return false;
                    };
                    RawWindowHandle::AppKit(AppKitWindowHandle::new(ns_view))
                }
                Some(GuiApi::Win32) => {
                    let Some(hwnd) = NonZeroIsize::new(window as isize) else {
                        return false;
                    };
                    RawWindowHandle::Win32(Win32WindowHandle::new(hwnd))
                }
                Some(GuiApi::X11) => {
                    if window.is_null() {
                        return false;
                    }
                    RawWindowHandle::Xlib(XlibWindowHandle::new(window as _))
                }
                _ => return false,
            };

            if prepare_view(&mut raw_handle).is_err() {
                return false;
            }

            let parent_window = match self.gui_api {
                Some(GuiApi::Cocoa) => ParentWindow::AppKit(window),
                Some(GuiApi::Win32) => ParentWindow::Win32(window),
                Some(GuiApi::X11) => ParentWindow::X11(window as u32),
                _ => return false,
            };

            let context = Arc::new(ClapParamsViewContext {
                params: self.params.clone(),
                host: self.host.clone(),
                descriptors: self.param_descriptors.clone(),
            });
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                view.open(parent_window, context)
            })) {
                Ok(Some(handle)) => {
                    self.view_handle = Some(MainThreadViewHandle { handle });
                    true
                }
                Ok(None) => false,
                Err(_) => {
                    eprintln!("SunMao CLAP view creation panicked");
                    false
                }
            }
        } else {
            false
        }
    }

    fn gui_show(&mut self) -> bool {
        self.view_handle.is_some()
    }

    fn gui_hide(&mut self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap_rs::clap_sys;
    use clap_rs::clap_sys::audio_buffer::clap_audio_buffer_t;
    use clap_rs::clap_sys::events::{
        clap_event_header_t, clap_event_note_expression_t, clap_event_note_t,
        clap_event_param_mod_t, clap_event_param_value_t, clap_event_transport_t,
        clap_input_events_t, CLAP_CORE_EVENT_SPACE_ID, CLAP_EVENT_NOTE_EXPRESSION,
        CLAP_EVENT_NOTE_ON, CLAP_EVENT_PARAM_MOD, CLAP_EVENT_PARAM_VALUE,
        CLAP_NOTE_EXPRESSION_TUNING, CLAP_TRANSPORT_HAS_BEATS_TIMELINE,
        CLAP_TRANSPORT_HAS_SECONDS_TIMELINE, CLAP_TRANSPORT_HAS_TEMPO,
        CLAP_TRANSPORT_HAS_TIME_SIGNATURE, CLAP_TRANSPORT_IS_LOOP_ACTIVE,
        CLAP_TRANSPORT_IS_PLAYING, CLAP_TRANSPORT_IS_RECORDING,
    };
    use clap_rs::clap_sys::ext::gui::{clap_host_gui_t, CLAP_EXT_GUI};
    use clap_rs::clap_sys::ext::params::{clap_host_params_t, CLAP_EXT_PARAMS};
    use clap_rs::clap_sys::fixedpoint::{CLAP_BEATTIME_FACTOR, CLAP_SECTIME_FACTOR};
    use clap_rs::clap_sys::host::clap_host_t;
    use clap_rs::clap_sys::process::{clap_process_t, CLAP_PROCESS_CONTINUE, CLAP_PROCESS_ERROR};
    use clap_rs::clap_sys::stream::clap_istream_t;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::ffi::{c_char, c_void, CStr};
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
    use std::sync::{Mutex, OnceLock};
    use sunmao_core::{BoolParam, FloatParam, IntParam, ParamDescriptor, ParamKind, ProcessStatus};

    #[cfg(target_os = "macos")]
    #[link(name = "AppKit", kind = "framework")]
    unsafe extern "C" {
        fn NSApplicationLoad() -> bool;
    }

    struct TestAllocator;

    thread_local! {
        static ALLOCATOR_CALL_COUNT: Cell<isize> = const { Cell::new(-1) };
    }

    fn record_allocator_call() {
        let _ = ALLOCATOR_CALL_COUNT.try_with(|count| {
            let current = count.get();
            if current >= 0 {
                count.set(current + 1);
            }
        });
    }

    unsafe impl GlobalAlloc for TestAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            record_allocator_call();
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            record_allocator_call();
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            record_allocator_call();
            unsafe { System.alloc_zeroed(layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            record_allocator_call();
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    #[global_allocator]
    static TEST_ALLOCATOR: TestAllocator = TestAllocator;

    struct AllocationScope;

    impl Drop for AllocationScope {
        fn drop(&mut self) {
            ALLOCATOR_CALL_COUNT.with(|count| count.set(-1));
        }
    }

    fn count_allocator_calls<R>(callback: impl FnOnce() -> R) -> (R, usize) {
        ALLOCATOR_CALL_COUNT.with(|count| {
            assert_eq!(count.get(), -1);
            count.set(0);
        });
        let scope = AllocationScope;
        let result = callback();
        let allocator_calls = ALLOCATOR_CALL_COUNT.with(|count| count.get() as usize);
        drop(scope);
        (result, allocator_calls)
    }

    #[test]
    fn only_the_native_clap_window_api_is_reported() {
        #[cfg(target_os = "macos")]
        let native = GuiApi::Cocoa;
        #[cfg(target_os = "windows")]
        let native = GuiApi::Win32;
        #[cfg(target_os = "linux")]
        let native = GuiApi::X11;

        assert!(is_native_gui_api(native));
        for foreign in [GuiApi::Cocoa, GuiApi::Win32, GuiApi::X11, GuiApi::Wayland] {
            if foreign != native {
                assert!(!is_native_gui_api(foreign));
            }
        }
    }

    #[test]
    fn output_preparation_clears_channels_without_matching_inputs() {
        let inputs = vec![vec![1.0, 2.0]];
        let mut outputs = vec![vec![9.0; 3], vec![8.0; 3]];

        prepare_output_buffers(&inputs, &mut outputs, 3, true);

        assert_eq!(outputs[0], [1.0, 2.0, 0.0]);
        assert_eq!(outputs[1], [0.0, 0.0, 0.0]);
    }

    #[test]
    fn short_input_channels_do_not_retain_previous_block_samples() {
        let mut buffer = vec![9.0; 4];
        copy_input_buffer(&mut buffer, &[1.0, 2.0], 4);
        assert_eq!(buffer, [1.0, 2.0, 0.0, 0.0]);

        copy_input_buffer(&mut buffer, &[], 4);
        assert_eq!(buffer, [0.0; 4]);
    }

    #[test]
    fn clap_event_offsets_stay_inside_the_active_block() {
        assert_eq!(event_sample_offset(0, 8), 0);
        assert_eq!(event_sample_offset(7, 8), 7);
        assert_eq!(event_sample_offset(u32::MAX, 8), 7);
        assert_eq!(event_sample_offset(u32::MAX, 0), 0);
    }

    #[test]
    fn malformed_note_fields_are_rejected_before_midi_conversion() {
        let mut note = clap_rs::events::NoteEvent {
            time: 99,
            port_index: 0,
            channel: 0,
            key: 60,
            note_id: -1,
            velocity: 0.5,
        };
        assert!(note_event_to_midi(&note, 8, true).is_some());

        note.channel = -1;
        assert!(note_event_to_midi(&note, 8, true).is_none());
        note.channel = 0;
        note.key = 128;
        assert!(note_event_to_midi(&note, 8, true).is_none());
        note.key = 60;
        note.velocity = f64::NAN;
        assert!(note_event_to_midi(&note, 8, true).is_none());
    }

    #[derive(Default)]
    struct RealtimeParams;

    impl Params for RealtimeParams {
        fn get_normalized(&self, _id: &str) -> Option<f32> {
            None
        }

        fn set_normalized(&self, _id: &str, _value: f32) {}

        fn descriptors(&self) -> Vec<ParamDescriptor> {
            Vec::new()
        }
    }

    #[derive(Default)]
    struct RealtimePlugin;

    static REALTIME_PROCESS_CALLS: AtomicUsize = AtomicUsize::new(0);
    static REALTIME_EVENT_COUNT: AtomicUsize = AtomicUsize::new(0);

    impl SunmaoPlugin for RealtimePlugin {
        const NAME: &'static str = "Realtime Test";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        const MAX_EVENTS_PER_BLOCK: usize = 8;
        type Params = RealtimeParams;

        fn input_channels(&self) -> u32 {
            2
        }

        fn output_channels(&self) -> u32 {
            2
        }

        fn accepts_midi(&self) -> bool {
            true
        }

        fn params(&self) -> Arc<Self::Params> {
            Arc::new(RealtimeParams)
        }

        fn process(
            &mut self,
            _buffer: &mut AudioBuffer,
            events: &EventQueue,
            _context: &SunmaoProcessContext,
        ) -> ProcessStatus {
            REALTIME_PROCESS_CALLS.fetch_add(1, Ordering::SeqCst);
            REALTIME_EVENT_COUNT.store(events.iter().count(), Ordering::SeqCst);
            ProcessStatus::Normal
        }
    }

    #[derive(Default)]
    struct NoMidiPlugin;

    static NO_MIDI_EVENT_COUNT: AtomicUsize = AtomicUsize::new(0);

    impl SunmaoPlugin for NoMidiPlugin {
        const NAME: &'static str = "No MIDI Test";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        type Params = RealtimeParams;

        fn input_channels(&self) -> u32 {
            0
        }

        fn output_channels(&self) -> u32 {
            0
        }

        fn params(&self) -> Arc<Self::Params> {
            Arc::new(RealtimeParams)
        }

        fn process(
            &mut self,
            _buffer: &mut AudioBuffer,
            events: &EventQueue,
            _context: &SunmaoProcessContext,
        ) -> ProcessStatus {
            NO_MIDI_EVENT_COUNT.store(events.iter().count(), Ordering::SeqCst);
            ProcessStatus::Normal
        }
    }

    struct PanickingView;

    impl SunmaoView for PanickingView {
        fn size(&self) -> (u32, u32) {
            (320, 180)
        }

        fn open(
            &self,
            _parent: ParentWindow,
            _context: Arc<dyn ViewContext>,
        ) -> Option<ViewHandle> {
            panic!("intentional view creation failure")
        }
    }

    #[derive(Default)]
    struct PanickingViewPlugin;

    impl SunmaoPlugin for PanickingViewPlugin {
        const NAME: &'static str = "Panicking GUI";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        type Params = RealtimeParams;

        fn params(&self) -> Arc<Self::Params> {
            Arc::new(RealtimeParams)
        }

        fn view(&self) -> Option<Box<dyn SunmaoView>> {
            Some(Box::new(PanickingView))
        }

        fn process(
            &mut self,
            _buffer: &mut AudioBuffer,
            _events: &EventQueue,
            _context: &SunmaoProcessContext,
        ) -> ProcessStatus {
            ProcessStatus::Normal
        }
    }

    #[test]
    fn view_creation_panic_is_contained_before_returning_through_clap_abi() {
        let host = unsafe { HostHandle::from_raw(std::ptr::null()) };
        let mut wrapper = <SunmaoClapWrapper<PanickingViewPlugin> as Plugin>::new(host);
        let (api, is_floating) = wrapper.preferred_api().expect("native GUI API");
        assert!(wrapper.gui_create(api, is_floating));

        #[cfg(target_os = "macos")]
        let parent = unsafe {
            use objc::runtime::Object;
            use objc::{class, msg_send, sel, sel_impl};
            let _ = NSApplicationLoad();
            let parent: *mut Object = msg_send![class!(NSView), new];
            assert!(!parent.is_null());
            parent.cast::<c_void>()
        };
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        let parent = 1usize as *mut c_void;

        assert!(!wrapper.gui_set_parent(parent));

        #[cfg(target_os = "macos")]
        unsafe {
            use objc::runtime::Object;
            use objc::{msg_send, sel, sel_impl};
            let _: () = msg_send![parent as *mut Object, release];
        }
    }

    struct NoteInputEvents {
        events: [clap_event_note_t; RealtimePlugin::MAX_EVENTS_PER_BLOCK],
    }

    unsafe extern "C" fn note_event_count(_list: *const clap_input_events_t) -> u32 {
        RealtimePlugin::MAX_EVENTS_PER_BLOCK as u32
    }

    unsafe extern "C" fn note_event_get(
        list: *const clap_input_events_t,
        index: u32,
    ) -> *const clap_event_header_t {
        let events = unsafe { &*((*list).ctx as *const NoteInputEvents) };
        events
            .events
            .get(index as usize)
            .map(|event| &event.header as *const clap_event_header_t)
            .unwrap_or(std::ptr::null())
    }

    fn raw_note_event(time: u32) -> clap_event_note_t {
        clap_event_note_t {
            header: clap_event_header_t {
                size: std::mem::size_of::<clap_event_note_t>() as u32,
                time,
                space_id: CLAP_CORE_EVENT_SPACE_ID,
                type_: CLAP_EVENT_NOTE_ON,
                flags: 0,
            },
            note_id: time as i32,
            port_index: 0,
            channel: 0,
            key: 60,
            velocity: 0.75,
        }
    }

    #[test]
    fn in_budget_dense_clap_processing_does_not_use_the_allocator() {
        REALTIME_PROCESS_CALLS.store(0, Ordering::SeqCst);
        REALTIME_EVENT_COUNT.store(0, Ordering::SeqCst);

        let plugin = unsafe {
            clap_rs::entry::PluginEntry::create_plugin::<SunmaoClapWrapper<RealtimePlugin>>(
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert!(!plugin.is_null());
        unsafe {
            assert!(((*plugin).init.unwrap())(plugin));
            assert!(((*plugin).activate.unwrap())(plugin, 48_000.0, 1, 8));
            assert!(((*plugin).start_processing.unwrap())(plugin));
        }

        let mut raw_events = NoteInputEvents {
            events: std::array::from_fn(|index| raw_note_event(index as u32)),
        };
        let input_events = clap_input_events_t {
            ctx: (&mut raw_events as *mut NoteInputEvents).cast::<c_void>(),
            size: Some(note_event_count),
            get: Some(note_event_get),
        };
        let input_left = [0.1_f32, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];
        let input_right = [-0.1_f32, -0.2, -0.3, -0.4, -0.5, -0.6, -0.7, -0.8];
        let mut input_channels = [
            input_left.as_ptr() as *mut f32,
            input_right.as_ptr() as *mut f32,
        ];
        let input_buffers = [clap_audio_buffer_t {
            data32: input_channels.as_mut_ptr(),
            data64: std::ptr::null_mut(),
            channel_count: 2,
            latency: 0,
            constant_mask: 0,
        }];
        let mut output_left = [0.0_f32; 8];
        let mut output_right = [0.0_f32; 8];
        let mut output_channels = [output_left.as_mut_ptr(), output_right.as_mut_ptr()];
        let mut output_buffers = [clap_audio_buffer_t {
            data32: output_channels.as_mut_ptr(),
            data64: std::ptr::null_mut(),
            channel_count: 2,
            latency: 0,
            constant_mask: 0,
        }];
        let process = clap_process_t {
            steady_time: 0,
            frames_count: 8,
            transport: std::ptr::null(),
            audio_inputs: input_buffers.as_ptr(),
            audio_outputs: output_buffers.as_mut_ptr(),
            audio_inputs_count: 1,
            audio_outputs_count: 1,
            in_events: &input_events,
            out_events: std::ptr::null(),
        };

        let (status, allocator_calls) =
            count_allocator_calls(|| unsafe { ((*plugin).process.unwrap())(plugin, &process) });
        assert_eq!(status, CLAP_PROCESS_CONTINUE);
        assert_eq!(allocator_calls, 0);
        assert_eq!(REALTIME_PROCESS_CALLS.load(Ordering::SeqCst), 1);
        assert_eq!(
            REALTIME_EVENT_COUNT.load(Ordering::SeqCst),
            RealtimePlugin::MAX_EVENTS_PER_BLOCK
        );
        assert_eq!(output_left, input_left);
        assert_eq!(output_right, input_right);

        unsafe {
            ((*plugin).stop_processing.unwrap())(plugin);
            ((*plugin).deactivate.unwrap())(plugin);
            ((*plugin).destroy.unwrap())(plugin);
        }
    }

    #[test]
    fn midi_events_are_not_delivered_to_plugins_without_a_note_port() {
        NO_MIDI_EVENT_COUNT.store(usize::MAX, Ordering::SeqCst);

        let plugin = unsafe {
            clap_rs::entry::PluginEntry::create_plugin::<SunmaoClapWrapper<NoMidiPlugin>>(
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert!(!plugin.is_null());

        unsafe {
            assert!(((*plugin).init.unwrap())(plugin));
            assert!(((*plugin).activate.unwrap())(plugin, 48_000.0, 1, 8));
            assert!(((*plugin).start_processing.unwrap())(plugin));
        }

        // Deliberately send note events even though this plugin advertises no
        // note input. The adapter should keep them out of the core queue and
        // continue processing the block.
        let mut raw_events = NoteInputEvents {
            events: std::array::from_fn(|index| raw_note_event(index as u32)),
        };
        let input_events = clap_input_events_t {
            ctx: (&mut raw_events as *mut NoteInputEvents).cast::<c_void>(),
            size: Some(note_event_count),
            get: Some(note_event_get),
        };
        let process = clap_process_t {
            steady_time: 0,
            frames_count: 8,
            transport: std::ptr::null(),
            audio_inputs: std::ptr::null(),
            audio_outputs: std::ptr::null_mut(),
            audio_inputs_count: 0,
            audio_outputs_count: 0,
            in_events: &input_events,
            out_events: std::ptr::null(),
        };

        let status = unsafe { ((*plugin).process.unwrap())(plugin, &process) };
        assert_eq!(status, CLAP_PROCESS_CONTINUE);
        assert_eq!(NO_MIDI_EVENT_COUNT.load(Ordering::SeqCst), 0);

        unsafe {
            ((*plugin).stop_processing.unwrap())(plugin);
            ((*plugin).deactivate.unwrap())(plugin);
            ((*plugin).destroy.unwrap())(plugin);
        }
    }

    struct AutomationParams {
        automated: FloatParam,
    }

    impl Default for AutomationParams {
        fn default() -> Self {
            Self {
                automated: FloatParam::new("automated", "Automated", 0.1, 0.0, 1.0),
            }
        }
    }

    impl Params for AutomationParams {
        fn get_normalized(&self, id: &str) -> Option<f32> {
            (id == "automated").then(|| self.automated.get_normalized())
        }

        fn set_normalized(&self, id: &str, value: f32) {
            if id == "automated" {
                self.automated.set_normalized(value);
            }
        }

        fn descriptors(&self) -> Vec<ParamDescriptor> {
            vec![ParamDescriptor {
                id: "automated",
                numeric_id: sunmao_core::stable_param_id("automated"),
                name: self.automated.name,
                default_normalized: 0.1,
                step_count: 0,
                kind: ParamKind::Float,
                group: "",
            }]
        }
    }

    struct AutomationPlugin {
        params: Arc<AutomationParams>,
    }

    fn automation_params_slot() -> &'static Mutex<Option<Arc<AutomationParams>>> {
        static SLOT: OnceLock<Mutex<Option<Arc<AutomationParams>>>> = OnceLock::new();
        SLOT.get_or_init(|| Mutex::new(None))
    }

    fn observed_events() -> &'static Mutex<Vec<ParamChange>> {
        static EVENTS: OnceLock<Mutex<Vec<ParamChange>>> = OnceLock::new();
        EVENTS.get_or_init(|| Mutex::new(Vec::new()))
    }

    fn automation_test_lock() -> &'static Mutex<()> {
        static LOCK: Mutex<()> = Mutex::new(());
        &LOCK
    }

    static PROCESS_CALLS: AtomicUsize = AtomicUsize::new(0);
    static VALUE_DURING_PROCESS: AtomicU32 = AtomicU32::new(0);

    impl Default for AutomationPlugin {
        fn default() -> Self {
            let params = Arc::new(AutomationParams::default());
            *automation_params_slot().lock().unwrap() = Some(params.clone());
            Self { params }
        }
    }

    impl SunmaoPlugin for AutomationPlugin {
        const NAME: &'static str = "Automation Test";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        const MAX_EVENTS_PER_BLOCK: usize = 5;
        type Params = AutomationParams;

        fn input_channels(&self) -> u32 {
            0
        }

        fn output_channels(&self) -> u32 {
            0
        }

        fn params(&self) -> Arc<Self::Params> {
            self.params.clone()
        }

        fn process(
            &mut self,
            _buffer: &mut AudioBuffer,
            events: &EventQueue,
            _context: &SunmaoProcessContext,
        ) -> ProcessStatus {
            PROCESS_CALLS.fetch_add(1, Ordering::SeqCst);
            VALUE_DURING_PROCESS.store(
                self.params.automated.get_normalized().to_bits(),
                Ordering::SeqCst,
            );
            *observed_events().lock().unwrap() = events
                .param_changes()
                .map(|change| {
                    assert_eq!(change.id, "automated");
                    ParamChange {
                        id: "automated",
                        value: change.value,
                        offset: change.offset,
                    }
                })
                .collect();
            ProcessStatus::Normal
        }
    }

    unsafe extern "C" fn input_event_count(list: *const clap_input_events_t) -> u32 {
        let events = unsafe { &*((*list).ctx as *const Vec<clap_event_param_value_t>) };
        events.len() as u32
    }

    unsafe extern "C" fn input_event_get(
        list: *const clap_input_events_t,
        index: u32,
    ) -> *const clap_event_header_t {
        let events = unsafe { &*((*list).ctx as *const Vec<clap_event_param_value_t>) };
        events
            .get(index as usize)
            .map(|event| &event.header as *const clap_event_header_t)
            .unwrap_or(std::ptr::null())
    }

    fn raw_param_event(time: u32, param_id: u32, value: f64) -> clap_event_param_value_t {
        clap_event_param_value_t {
            header: clap_event_header_t {
                size: std::mem::size_of::<clap_event_param_value_t>() as u32,
                time,
                space_id: CLAP_CORE_EVENT_SPACE_ID,
                type_: CLAP_EVENT_PARAM_VALUE,
                flags: 0,
            },
            param_id,
            cookie: std::ptr::null_mut(),
            note_id: -1,
            port_index: -1,
            channel: -1,
            key: -1,
            value,
        }
    }

    #[test]
    fn process_preserves_timed_parameter_events_and_publishes_the_final_value_after_dsp() {
        let _guard = automation_test_lock().lock().unwrap();
        PROCESS_CALLS.store(0, Ordering::SeqCst);
        VALUE_DURING_PROCESS.store(0, Ordering::SeqCst);
        observed_events().lock().unwrap().clear();
        *automation_params_slot().lock().unwrap() = None;

        let plugin = unsafe {
            clap_rs::entry::PluginEntry::create_plugin::<SunmaoClapWrapper<AutomationPlugin>>(
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert!(!plugin.is_null());

        unsafe {
            assert!(((*plugin).init.unwrap())(plugin));
            assert!(((*plugin).activate.unwrap())(plugin, 48_000.0, 1, 8));
            assert!(((*plugin).start_processing.unwrap())(plugin));
        }

        let automated = sunmao_core::stable_param_id("automated");
        let mut raw_events = vec![
            raw_param_event(2, automated, 0.25),
            raw_param_event(3, sunmao_core::stable_param_id("unknown"), 0.9),
            raw_param_event(4, automated, f64::NAN),
            raw_param_event(5, automated, 0.5),
            raw_param_event(5, automated, 0.75),
        ];
        let input_events = clap_input_events_t {
            ctx: &mut raw_events as *mut Vec<clap_event_param_value_t> as *mut c_void,
            size: Some(input_event_count),
            get: Some(input_event_get),
        };
        let process = clap_process_t {
            steady_time: 0,
            frames_count: 8,
            transport: std::ptr::null(),
            audio_inputs: std::ptr::null(),
            audio_outputs: std::ptr::null_mut(),
            audio_inputs_count: 0,
            audio_outputs_count: 0,
            in_events: &input_events,
            out_events: std::ptr::null(),
        };

        let status = unsafe { ((*plugin).process.unwrap())(plugin, &process) };
        assert_eq!(status, CLAP_PROCESS_CONTINUE);
        assert_eq!(PROCESS_CALLS.load(Ordering::SeqCst), 1);
        assert_eq!(
            f32::from_bits(VALUE_DURING_PROCESS.load(Ordering::SeqCst)),
            0.1
        );
        assert_eq!(
            *observed_events().lock().unwrap(),
            [
                ParamChange {
                    id: "automated",
                    value: 0.25,
                    offset: 2,
                },
                ParamChange {
                    id: "automated",
                    value: 0.5,
                    offset: 5,
                },
                ParamChange {
                    id: "automated",
                    value: 0.75,
                    offset: 5,
                },
            ]
        );

        let params = automation_params_slot()
            .lock()
            .unwrap()
            .clone()
            .expect("automation params");
        assert_eq!(params.automated.get_normalized(), 0.75);

        unsafe {
            ((*plugin).stop_processing.unwrap())(plugin);
            ((*plugin).deactivate.unwrap())(plugin);
            ((*plugin).destroy.unwrap())(plugin);
        }
    }

    #[test]
    fn dense_host_event_input_returns_error_before_dsp() {
        let _guard = automation_test_lock().lock().unwrap();
        PROCESS_CALLS.store(0, Ordering::SeqCst);
        observed_events().lock().unwrap().clear();

        let plugin = unsafe {
            clap_rs::entry::PluginEntry::create_plugin::<SunmaoClapWrapper<AutomationPlugin>>(
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert!(!plugin.is_null());

        unsafe {
            assert!(((*plugin).init.unwrap())(plugin));
            assert!(((*plugin).activate.unwrap())(plugin, 48_000.0, 1, 8));
            assert!(((*plugin).start_processing.unwrap())(plugin));
        }

        let automated = sunmao_core::stable_param_id("automated");
        let mut raw_events = (0..6)
            .map(|offset| raw_param_event(offset, automated, offset as f64 / 10.0))
            .collect::<Vec<_>>();
        let input_events = clap_input_events_t {
            ctx: &mut raw_events as *mut Vec<clap_event_param_value_t> as *mut c_void,
            size: Some(input_event_count),
            get: Some(input_event_get),
        };
        let process = clap_process_t {
            steady_time: 0,
            frames_count: 8,
            transport: std::ptr::null(),
            audio_inputs: std::ptr::null(),
            audio_outputs: std::ptr::null_mut(),
            audio_inputs_count: 0,
            audio_outputs_count: 0,
            in_events: &input_events,
            out_events: std::ptr::null(),
        };

        let (status, allocator_calls) =
            count_allocator_calls(|| unsafe { ((*plugin).process.unwrap())(plugin, &process) });
        assert_eq!(status, CLAP_PROCESS_ERROR);
        assert_eq!(allocator_calls, 0);
        assert_eq!(PROCESS_CALLS.load(Ordering::SeqCst), 0);
        assert!(observed_events().lock().unwrap().is_empty());

        unsafe {
            ((*plugin).stop_processing.unwrap())(plugin);
            ((*plugin).deactivate.unwrap())(plugin);
            ((*plugin).destroy.unwrap())(plugin);
        }
    }

    #[test]
    fn malformed_host_event_list_returns_error_before_dsp() {
        let _guard = automation_test_lock().lock().unwrap();
        PROCESS_CALLS.store(0, Ordering::SeqCst);
        observed_events().lock().unwrap().clear();

        let plugin = unsafe {
            clap_rs::entry::PluginEntry::create_plugin::<SunmaoClapWrapper<AutomationPlugin>>(
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert!(!plugin.is_null());

        unsafe {
            assert!(((*plugin).init.unwrap())(plugin));
            assert!(((*plugin).activate.unwrap())(plugin, 48_000.0, 1, 8));
            assert!(((*plugin).start_processing.unwrap())(plugin));
        }

        let mut raw_events = Vec::<clap_event_param_value_t>::new();
        let malformed_lists = [
            clap_input_events_t {
                ctx: (&mut raw_events as *mut Vec<clap_event_param_value_t>).cast::<c_void>(),
                size: None,
                get: Some(input_event_get),
            },
            clap_input_events_t {
                ctx: (&mut raw_events as *mut Vec<clap_event_param_value_t>).cast::<c_void>(),
                size: Some(input_event_count),
                get: None,
            },
        ];

        for input_events in &malformed_lists {
            let process = clap_process_t {
                steady_time: 0,
                frames_count: 8,
                transport: std::ptr::null(),
                audio_inputs: std::ptr::null(),
                audio_outputs: std::ptr::null_mut(),
                audio_inputs_count: 0,
                audio_outputs_count: 0,
                in_events: input_events,
                out_events: std::ptr::null(),
            };

            let (status, allocator_calls) =
                count_allocator_calls(|| unsafe { ((*plugin).process.unwrap())(plugin, &process) });
            assert_eq!(status, CLAP_PROCESS_ERROR);
            assert_eq!(allocator_calls, 0);
        }
        assert_eq!(PROCESS_CALLS.load(Ordering::SeqCst), 0);
        assert!(observed_events().lock().unwrap().is_empty());

        unsafe {
            ((*plugin).stop_processing.unwrap())(plugin);
            ((*plugin).deactivate.unwrap())(plugin);
            ((*plugin).destroy.unwrap())(plugin);
        }
    }

    struct MetadataParams {
        mix: FloatParam,
        voices: IntParam,
        bypass: BoolParam,
    }

    impl Default for MetadataParams {
        fn default() -> Self {
            Self {
                mix: FloatParam::new("mix", "Dry/Wet", 0.25, 0.0, 1.0),
                voices: IntParam::new("voices", "Voices", 3, 1, 5),
                bypass: BoolParam::new("bypass", "Bypass", false),
            }
        }
    }

    impl Params for MetadataParams {
        fn get_normalized(&self, id: &str) -> Option<f32> {
            match id {
                "mix" => Some(self.mix.get_normalized()),
                "voices" => Some((self.voices.get() - 1) as f32 / 4.0),
                "bypass" => Some(if self.bypass.get() { 1.0 } else { 0.0 }),
                _ => None,
            }
        }

        fn set_normalized(&self, id: &str, value: f32) {
            match id {
                "mix" => self.mix.set_normalized(value),
                "voices" => self
                    .voices
                    .set((1.0 + value.clamp(0.0, 1.0) * 4.0).round() as i32),
                "bypass" => self.bypass.set(value >= 0.5),
                _ => {}
            }
        }

        fn descriptors(&self) -> Vec<ParamDescriptor> {
            vec![
                ParamDescriptor {
                    id: "mix",
                    numeric_id: sunmao_core::stable_param_id("mix"),
                    name: self.mix.name,
                    default_normalized: 0.25,
                    step_count: 0,
                    kind: ParamKind::Float,
                    group: "",
                },
                ParamDescriptor {
                    id: "voices",
                    numeric_id: sunmao_core::stable_param_id("voices"),
                    name: self.voices.name,
                    default_normalized: 0.5,
                    step_count: 4,
                    kind: ParamKind::Int,
                    group: "",
                },
                ParamDescriptor {
                    id: "bypass",
                    numeric_id: sunmao_core::stable_param_id("bypass"),
                    name: self.bypass.name,
                    default_normalized: 0.0,
                    step_count: 1,
                    kind: ParamKind::Bool,
                    group: "",
                },
            ]
        }
    }

    #[derive(Default)]
    struct MetadataPlugin;

    impl SunmaoPlugin for MetadataPlugin {
        const NAME: &'static str = "Metadata";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        type Params = MetadataParams;

        fn params(&self) -> Arc<Self::Params> {
            Arc::new(MetadataParams::default())
        }

        fn process(
            &mut self,
            _buffer: &mut AudioBuffer,
            _events: &EventQueue,
            _context: &SunmaoProcessContext,
        ) -> ProcessStatus {
            ProcessStatus::Normal
        }
    }

    #[derive(Default)]
    struct NotificationHostState {
        flush_requests: usize,
        resize_requests: Vec<(u32, u32)>,
        resize_result: bool,
    }

    unsafe extern "C" fn notification_host_get_extension(
        _host: *const clap_host_t,
        extension_id: *const c_char,
    ) -> *const c_void {
        if extension_id.is_null() {
            return std::ptr::null();
        }
        match unsafe { CStr::from_ptr(extension_id) }.to_bytes_with_nul() {
            bytes if bytes == CLAP_EXT_PARAMS.as_bytes() => {
                (&NOTIFICATION_HOST_PARAMS as *const clap_host_params_t).cast()
            }
            bytes if bytes == CLAP_EXT_GUI.as_bytes() => {
                (&NOTIFICATION_HOST_GUI as *const clap_host_gui_t).cast()
            }
            _ => std::ptr::null(),
        }
    }

    unsafe extern "C" fn notification_host_request_flush(host: *const clap_host_t) {
        let state = unsafe { &mut *((*host).host_data as *mut NotificationHostState) };
        state.flush_requests += 1;
    }

    unsafe extern "C" fn notification_host_request_resize(
        host: *const clap_host_t,
        width: u32,
        height: u32,
    ) -> bool {
        let state = unsafe { &mut *((*host).host_data as *mut NotificationHostState) };
        state.resize_requests.push((width, height));
        state.resize_result
    }

    static NOTIFICATION_HOST_PARAMS: clap_host_params_t = clap_host_params_t {
        rescan: None,
        clear: None,
        request_flush: Some(notification_host_request_flush),
    };

    static NOTIFICATION_HOST_GUI: clap_host_gui_t = clap_host_gui_t {
        resize_hints_changed: None,
        request_resize: Some(notification_host_request_resize),
        request_show: None,
        request_hide: None,
        closed: None,
    };

    fn notification_host(state: &mut NotificationHostState) -> clap_host_t {
        clap_host_t {
            clap_version: clap_rs::CLAP_VERSION,
            host_data: (state as *mut NotificationHostState).cast(),
            name: std::ptr::null(),
            vendor: std::ptr::null(),
            url: std::ptr::null(),
            version: std::ptr::null(),
            get_extension: Some(notification_host_get_extension),
            request_restart: None,
            request_process: None,
            request_callback: None,
        }
    }

    #[test]
    fn unified_view_context_updates_params_and_notifies_the_clap_host() {
        let mut host_state = NotificationHostState {
            resize_result: true,
            ..NotificationHostState::default()
        };
        let raw_host = notification_host(&mut host_state);
        let host = unsafe { HostHandle::from_raw(&raw_host) };
        let params = Arc::new(MetadataParams::default());
        let descriptors = params.descriptors();
        let context = ClapParamsViewContext {
            params: params.clone(),
            host,
            descriptors: descriptors.clone(),
        };

        context.begin_edit("mix");
        context.set_param("mix", 0.75);
        context.end_edit("mix");
        context.set_param("voices", 0.74);
        context.set_param("bypass", 0.9);
        context.begin_edit("unknown");
        context.set_param("unknown", 0.5);
        context.end_edit("unknown");

        assert_eq!(context.get_param("mix"), Some(0.75));
        assert_eq!(context.get_param("voices"), Some(0.75));
        assert_eq!(context.get_param("bypass"), Some(1.0));
        assert_eq!(context.get_param("unknown"), None);
        assert_eq!(host_state.flush_requests, 5);

        assert_eq!(parameter_to_clap_value(&descriptors[0], 0.75), 0.75);
        assert_eq!(parameter_to_clap_value(&descriptors[1], 0.75), 3.0);
        assert_eq!(parameter_to_clap_value(&descriptors[2], 1.0), 1.0);

        assert!(context.request_resize(900, 500));
        host_state.resize_result = false;
        assert!(!context.request_resize(1200, 700));
        assert_eq!(host_state.resize_requests, vec![(900, 500), (1200, 700)]);
    }

    #[test]
    fn exposes_float_int_and_bool_parameter_metadata() {
        let host = unsafe { HostHandle::from_raw(std::ptr::null()) };
        let mut wrapper = <SunmaoClapWrapper<MetadataPlugin> as Plugin>::new(host);
        let params = wrapper.declare_parameters();
        assert_eq!(params.len(), 3);

        assert_eq!(params[0].name, "Dry/Wet");
        assert_eq!(params[0].id, sunmao_core::stable_param_id("mix"));
        assert_eq!(params[0].default_value, 0.25);
        assert!(!params[0].is_stepped);

        assert_eq!(params[1].name, "Voices");
        assert_eq!(params[1].id, sunmao_core::stable_param_id("voices"));
        assert_eq!(params[1].min_value, 0.0);
        assert_eq!(params[1].max_value, 4.0);
        assert_eq!(params[1].default_value, 2.0);
        assert!(params[1].is_stepped);

        assert_eq!(params[2].name, "Bypass");
        assert_eq!(params[2].id, sunmao_core::stable_param_id("bypass"));
        assert_eq!(params[2].default_value, 0.0);
        assert!(params[2].is_stepped);

        let voices = sunmao_core::stable_param_id("voices");
        for step in 0..=4 {
            wrapper.set_parameter(voices, step as f64);
            assert_eq!(wrapper.get_parameter(voices), step as f64);
            assert_eq!(wrapper.params.voices.get(), step + 1);
        }
    }

    #[test]
    fn adapter_activation_rejects_oversized_host_block() {
        let host = unsafe { HostHandle::from_raw(std::ptr::null()) };
        let mut wrapper = <SunmaoClapWrapper<MetadataPlugin> as Plugin>::new(host);
        assert!(<SunmaoClapWrapper<MetadataPlugin> as Plugin>::activate(
            &mut wrapper,
            48_000.0,
            9,
            8,
        )
        .is_none());
        assert!(<SunmaoClapWrapper<MetadataPlugin> as Plugin>::activate(
            &mut wrapper,
            48_000.0,
            1,
            MAX_PROCESS_FRAMES + 1,
        )
        .is_none());

        let processor = <SunmaoClapWrapper<MetadataPlugin> as Plugin>::activate(
            &mut wrapper,
            48_000.0,
            1,
            8192,
        )
        .expect("normal host block should activate");
        <SunmaoClapWrapper<MetadataPlugin> as Plugin>::deactivate(&mut wrapper, processor);
    }

    #[test]
    fn adapter_audio_allocation_rejects_oversized_channel_and_sample_budgets() {
        assert!(allocate_audio_buffers(MAX_PROCESS_CHANNELS as u32 + 1, 1).is_none());
        let channels_over_sample_budget =
            (MAX_PROCESS_AUDIO_SAMPLES / MAX_PROCESS_FRAMES as usize + 1) as u32;
        assert!(allocate_audio_buffers(channels_over_sample_budget, MAX_PROCESS_FRAMES).is_none());
    }

    #[derive(Default)]
    struct HugeEventCapacityPlugin;

    impl SunmaoPlugin for HugeEventCapacityPlugin {
        const NAME: &'static str = "Huge Event Capacity";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        const MAX_EVENTS_PER_BLOCK: usize = usize::MAX;
        type Params = RealtimeParams;

        fn input_channels(&self) -> u32 {
            0
        }

        fn output_channels(&self) -> u32 {
            0
        }

        fn params(&self) -> Arc<Self::Params> {
            Arc::new(RealtimeParams)
        }

        fn process(
            &mut self,
            _buffer: &mut AudioBuffer,
            _events: &EventQueue,
            _context: &SunmaoProcessContext,
        ) -> ProcessStatus {
            ProcessStatus::Normal
        }
    }

    #[test]
    fn activation_rejects_unallocatable_event_capacity_and_retains_plugin() {
        let host = unsafe { HostHandle::from_raw(std::ptr::null()) };
        let mut wrapper = <SunmaoClapWrapper<HugeEventCapacityPlugin> as Plugin>::new(host);
        let processor = <SunmaoClapWrapper<HugeEventCapacityPlugin> as Plugin>::activate(
            &mut wrapper,
            48_000.0,
            1,
            8,
        );
        assert!(processor.is_none());
        assert!(wrapper.plugin.is_some());
    }

    #[derive(Default)]
    struct TailPlugin;

    impl SunmaoPlugin for TailPlugin {
        const NAME: &'static str = "Tail Test";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        type Params = RealtimeParams;

        fn params(&self) -> Arc<Self::Params> {
            Arc::new(RealtimeParams)
        }

        fn latency_samples(&self) -> u32 {
            256
        }

        fn tail(&self) -> SunmaoTailLength {
            SunmaoTailLength::Infinite
        }

        fn process(
            &mut self,
            _buffer: &mut AudioBuffer,
            _events: &EventQueue,
            _context: &SunmaoProcessContext,
        ) -> ProcessStatus {
            ProcessStatus::Normal
        }
    }

    #[test]
    fn latency_and_infinite_tail_reach_the_clap_contract() {
        let host = unsafe { HostHandle::from_raw(std::ptr::null()) };
        let wrapper = <SunmaoClapWrapper<TailPlugin> as Plugin>::new(host);

        assert_eq!(Plugin::latency(&wrapper), 256);
        assert_eq!(Plugin::tail(&wrapper), CLAP_INFINITE_TAIL);
    }

    #[test]
    fn latency_and_tail_survive_activation() {
        // `activate` moves the plugin into the processor, so a naive
        // `self.plugin.as_ref()` reports 0 for the entire time the plugin is
        // active — which is precisely when a host reads these. A host that
        // believes latency is 0 misaligns the track.
        let host = unsafe { HostHandle::from_raw(std::ptr::null()) };
        let mut wrapper = <SunmaoClapWrapper<TailPlugin> as Plugin>::new(host);

        let processor =
            <SunmaoClapWrapper<TailPlugin> as Plugin>::activate(&mut wrapper, 48_000.0, 1, 128)
                .expect("activation succeeds");
        assert!(
            wrapper.plugin.is_none(),
            "precondition: activation takes ownership of the plugin"
        );

        assert_eq!(
            Plugin::latency(&wrapper),
            256,
            "latency must still be readable while active"
        );
        assert_eq!(
            Plugin::tail(&wrapper),
            CLAP_INFINITE_TAIL,
            "tail must still be readable while active"
        );

        // And after deactivation the plugin itself is authoritative again.
        <SunmaoClapWrapper<TailPlugin> as Plugin>::deactivate(&mut wrapper, processor);
        assert!(wrapper.plugin.is_some());
        assert_eq!(Plugin::latency(&wrapper), 256);
        assert_eq!(Plugin::tail(&wrapper), CLAP_INFINITE_TAIL);
    }

    #[derive(Default)]
    struct BusActivationPlugin {
        /// Activation states recorded in call order.
        calls: Vec<(bool, u32, bool)>,
    }

    impl SunmaoPlugin for BusActivationPlugin {
        const NAME: &'static str = "Bus Activation Test";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        type Params = RealtimeParams;

        fn params(&self) -> Arc<Self::Params> {
            Arc::new(RealtimeParams)
        }

        fn input_buses(&self) -> Vec<SunmaoBusInfo> {
            vec![
                SunmaoBusInfo::main("Input", 2),
                SunmaoBusInfo::sidechain("Sidechain", 2),
            ]
        }

        fn set_bus_active(&mut self, is_input: bool, bus_index: u32, active: bool) -> bool {
            self.calls.push((is_input, bus_index, active));
            // Refuse exactly one configuration so the rejection path is
            // observable through the CLAP contract too.
            !(is_input && bus_index == 1 && active)
        }

        fn process(
            &mut self,
            _buffer: &mut AudioBuffer,
            _events: &EventQueue,
            _context: &SunmaoProcessContext,
        ) -> ProcessStatus {
            ProcessStatus::Normal
        }
    }

    #[test]
    fn clap_port_activation_maps_onto_the_sunmao_bus_callback() {
        let host = unsafe { HostHandle::from_raw(std::ptr::null()) };
        let mut wrapper = <SunmaoClapWrapper<BusActivationPlugin> as Plugin>::new(host);

        // The CLAP port index is the SunMao bus index, in declaration order.
        assert!(Plugin::set_audio_port_active(
            &mut wrapper,
            true,
            0,
            true,
            32
        ));
        assert!(Plugin::set_audio_port_active(
            &mut wrapper,
            false,
            0,
            false,
            32
        ));
        // A plugin-side refusal must surface as `false`, not be swallowed.
        assert!(!Plugin::set_audio_port_active(
            &mut wrapper,
            true,
            1,
            true,
            32
        ));
        assert!(Plugin::set_audio_port_active(
            &mut wrapper,
            true,
            1,
            false,
            32
        ));

        let plugin = wrapper.plugin.as_ref().expect("plugin present when idle");
        assert_eq!(
            plugin.calls.as_slice(),
            &[
                (true, 0, true),
                (false, 0, false),
                (true, 1, true),
                (true, 1, false)
            ],
            "every host request reaches the plugin, in order and unaltered"
        );
    }

    #[test]
    fn clap_declares_one_audio_port_per_sunmao_bus() {
        // The activation bridge relies on port index == bus index, so pin it.
        let host = unsafe { HostHandle::from_raw(std::ptr::null()) };
        let wrapper = <SunmaoClapWrapper<BusActivationPlugin> as Plugin>::new(host);
        let ports = Plugin::audio_ports_config(&wrapper);
        let inputs: Vec<_> = ports.iter().filter(|port| port.is_input).collect();
        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[0].name, "Input");
        assert!(inputs[0].is_main);
        assert_eq!(inputs[1].name, "Sidechain");
        assert!(!inputs[1].is_main);
    }

    /// A plugin offering mono and stereo, like the `sunmao_fx_layout_gain`
    /// fixture.
    #[derive(Default)]
    struct LayoutPlugin {
        layout: usize,
    }

    impl SunmaoPlugin for LayoutPlugin {
        const NAME: &'static str = "Layout Test";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        type Params = RealtimeParams;

        fn params(&self) -> Arc<Self::Params> {
            Arc::new(RealtimeParams)
        }

        fn input_buses(&self) -> Vec<SunmaoBusInfo> {
            vec![SunmaoBusInfo::main(
                "Input",
                if self.layout == 0 { 1 } else { 2 },
            )]
        }

        fn output_buses(&self) -> Vec<SunmaoBusInfo> {
            vec![SunmaoBusInfo::main(
                "Output",
                if self.layout == 0 { 1 } else { 2 },
            )]
        }

        fn bus_configs(&self) -> Vec<sunmao_core::plugin::BusConfig> {
            vec![
                sunmao_core::plugin::BusConfig::new(
                    "Mono",
                    vec![SunmaoBusInfo::main("Input", 1)],
                    vec![SunmaoBusInfo::main("Output", 1)],
                ),
                sunmao_core::plugin::BusConfig::new(
                    "Stereo",
                    vec![SunmaoBusInfo::main("Input", 2)],
                    vec![SunmaoBusInfo::main("Output", 2)],
                ),
            ]
        }

        fn current_bus_config(&self) -> usize {
            self.layout
        }

        fn select_bus_config(&mut self, index: usize) -> bool {
            if index > 1 {
                return false;
            }
            self.layout = index;
            true
        }

        fn process(
            &mut self,
            _buffer: &mut AudioBuffer,
            _events: &EventQueue,
            _context: &SunmaoProcessContext,
        ) -> ProcessStatus {
            ProcessStatus::Normal
        }
    }

    #[test]
    fn clap_publishes_one_config_per_sunmao_bus_config() {
        let host = unsafe { HostHandle::from_raw(std::ptr::null()) };
        let wrapper = <SunmaoClapWrapper<LayoutPlugin> as Plugin>::new(host);
        let configs = Plugin::audio_ports_configs(&wrapper);
        assert_eq!(configs.len(), 2);
        // The id is the index, which is what `select` round-trips on.
        assert_eq!(configs[0].id, 0);
        assert_eq!(configs[0].name, "Mono");
        assert_eq!(configs[1].id, 1);
        assert_eq!(configs[1].name, "Stereo");

        // Each config describes its own ports, not the live layout.
        let mono_inputs: Vec<_> = configs[0].ports_in_direction(true).collect();
        assert_eq!(mono_inputs.len(), 1);
        assert_eq!(mono_inputs[0].channel_count, 1);
        let stereo_inputs: Vec<_> = configs[1].ports_in_direction(true).collect();
        assert_eq!(stereo_inputs[0].channel_count, 2);

        // The default layout is reported as current.
        assert_eq!(Plugin::current_audio_ports_config_id(&wrapper), 0);
    }

    #[test]
    fn clap_selecting_a_config_reconfigures_the_live_port_list() {
        let host = unsafe { HostHandle::from_raw(std::ptr::null()) };
        let mut wrapper = <SunmaoClapWrapper<LayoutPlugin> as Plugin>::new(host);
        // Mono is the default here, so the live ports start at one channel.
        assert_eq!(Plugin::audio_ports_config(&wrapper)[0].channel_count, 1);

        assert!(Plugin::select_audio_ports_config(&mut wrapper, 1));
        assert_eq!(Plugin::current_audio_ports_config_id(&wrapper), 1);

        // The live port list must follow the selection, otherwise the host
        // would keep seeing the layout it just replaced.
        let ports = Plugin::audio_ports_config(&wrapper);
        assert!(ports.iter().all(|port| port.channel_count == 2));
        // And the cached channel totals that drive scratch allocation.
        assert_eq!(wrapper.input_channels, 2);
        assert_eq!(wrapper.output_channels, 2);
    }

    #[test]
    fn clap_refuses_a_config_the_plugin_rejects() {
        let host = unsafe { HostHandle::from_raw(std::ptr::null()) };
        let mut wrapper = <SunmaoClapWrapper<LayoutPlugin> as Plugin>::new(host);
        assert!(!Plugin::select_audio_ports_config(&mut wrapper, 9));
        // The refusal leaves the previous layout in force.
        assert_eq!(Plugin::current_audio_ports_config_id(&wrapper), 0);
        assert_eq!(Plugin::audio_ports_config(&wrapper)[0].channel_count, 1);
    }

    #[test]
    fn a_plugin_without_alternatives_publishes_no_configs() {
        // The extension must stay hidden for the Phase 1/2 plugins, which have
        // exactly one layout.
        let host = unsafe { HostHandle::from_raw(std::ptr::null()) };
        let wrapper = <SunmaoClapWrapper<BusActivationPlugin> as Plugin>::new(host);
        assert!(Plugin::audio_ports_configs(&wrapper).is_empty());
    }

    #[test]
    fn a_finite_clap_tail_never_reaches_the_infinite_threshold() {
        // CLAP treats anything at or above `i32::MAX` as unbounded, so a
        // finite tail must stay strictly below it.
        assert_eq!(
            clamp_clap_tail(SunmaoTailLength::Samples(u32::MAX)),
            CLAP_INFINITE_TAIL - 1
        );
        assert_eq!(clamp_clap_tail(SunmaoTailLength::Samples(48_000)), 48_000);
        assert_eq!(clamp_clap_tail(SunmaoTailLength::None), 0);
    }

    #[derive(Default)]
    struct TransportPlugin;

    static OBSERVED_TRANSPORT: Mutex<Option<SunmaoProcessContext>> = Mutex::new(None);
    /// `OBSERVED_TRANSPORT` is process-wide, so transport observations must not
    /// run concurrently with each other.
    static TRANSPORT_OBSERVATION: Mutex<()> = Mutex::new(());

    impl SunmaoPlugin for TransportPlugin {
        const NAME: &'static str = "Transport Test";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        type Params = RealtimeParams;

        fn params(&self) -> Arc<Self::Params> {
            Arc::new(RealtimeParams)
        }

        fn process(
            &mut self,
            _buffer: &mut AudioBuffer,
            _events: &EventQueue,
            context: &SunmaoProcessContext,
        ) -> ProcessStatus {
            *OBSERVED_TRANSPORT.lock().unwrap() = Some(context.clone());
            ProcessStatus::Normal
        }
    }

    fn beats(value: f64) -> i64 {
        (value * CLAP_BEATTIME_FACTOR as f64) as i64
    }

    fn seconds(value: f64) -> i64 {
        (value * CLAP_SECTIME_FACTOR as f64) as i64
    }

    /// Runs one block through the CLAP wrapper with the given transport and
    /// returns the context the plugin observed.
    fn observe_transport(transport: Option<&clap_event_transport_t>) -> SunmaoProcessContext {
        let _serialized = TRANSPORT_OBSERVATION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *OBSERVED_TRANSPORT.lock().unwrap() = None;

        let plugin = unsafe {
            clap_rs::entry::PluginEntry::create_plugin::<SunmaoClapWrapper<TransportPlugin>>(
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert!(!plugin.is_null());
        unsafe {
            assert!(((*plugin).init.unwrap())(plugin));
            assert!(((*plugin).activate.unwrap())(plugin, 48_000.0, 1, 8));
            assert!(((*plugin).start_processing.unwrap())(plugin));
        }

        let input_left = [0.0_f32; 8];
        let input_right = [0.0_f32; 8];
        let mut input_channels = [
            input_left.as_ptr() as *mut f32,
            input_right.as_ptr() as *mut f32,
        ];
        let input_buffers = [clap_audio_buffer_t {
            data32: input_channels.as_mut_ptr(),
            data64: std::ptr::null_mut(),
            channel_count: 2,
            latency: 0,
            constant_mask: 0,
        }];
        let mut output_left = [0.0_f32; 8];
        let mut output_right = [0.0_f32; 8];
        let mut output_channels = [output_left.as_mut_ptr(), output_right.as_mut_ptr()];
        let mut output_buffers = [clap_audio_buffer_t {
            data32: output_channels.as_mut_ptr(),
            data64: std::ptr::null_mut(),
            channel_count: 2,
            latency: 0,
            constant_mask: 0,
        }];
        let mut empty_events = NoteInputEvents {
            events: std::array::from_fn(|index| raw_note_event(index as u32)),
        };
        let input_events = clap_input_events_t {
            ctx: (&mut empty_events as *mut NoteInputEvents).cast::<c_void>(),
            size: Some(no_events),
            get: Some(note_event_get),
        };
        let process = clap_process_t {
            steady_time: 0,
            frames_count: 8,
            transport: transport
                .map(|value| value as *const clap_event_transport_t)
                .unwrap_or(std::ptr::null()),
            audio_inputs: input_buffers.as_ptr(),
            audio_outputs: output_buffers.as_mut_ptr(),
            audio_inputs_count: 1,
            audio_outputs_count: 1,
            in_events: &input_events,
            out_events: std::ptr::null(),
        };

        unsafe {
            assert_eq!(
                ((*plugin).process.unwrap())(plugin, &process),
                CLAP_PROCESS_CONTINUE
            );
            ((*plugin).stop_processing.unwrap())(plugin);
            ((*plugin).deactivate.unwrap())(plugin);
            ((*plugin).destroy.unwrap())(plugin);
        }

        OBSERVED_TRANSPORT
            .lock()
            .unwrap()
            .take()
            .expect("plugin must observe a process context")
    }

    unsafe extern "C" fn no_events(_list: *const clap_input_events_t) -> u32 {
        0
    }

    #[test]
    fn clap_transport_fields_reach_the_plugin() {
        let transport = clap_event_transport_t {
            header: clap_event_header_t {
                size: std::mem::size_of::<clap_event_transport_t>() as u32,
                time: 0,
                space_id: CLAP_CORE_EVENT_SPACE_ID,
                type_: 0,
                flags: 0,
            },
            flags: CLAP_TRANSPORT_HAS_TEMPO
                | CLAP_TRANSPORT_HAS_BEATS_TIMELINE
                | CLAP_TRANSPORT_HAS_SECONDS_TIMELINE
                | CLAP_TRANSPORT_HAS_TIME_SIGNATURE
                | CLAP_TRANSPORT_IS_PLAYING
                | CLAP_TRANSPORT_IS_RECORDING
                | CLAP_TRANSPORT_IS_LOOP_ACTIVE,
            song_pos_beats: beats(8.5),
            song_pos_seconds: seconds(2.0),
            tempo: 128.0,
            tempo_inc: 0.0,
            loop_start_beats: beats(4.0),
            loop_end_beats: beats(12.0),
            loop_start_seconds: seconds(1.0),
            loop_end_seconds: seconds(3.0),
            bar_start: beats(8.0),
            bar_number: 3,
            tsig_num: 7,
            tsig_denom: 8,
        };

        let context = observe_transport(Some(&transport));

        assert_eq!(context.tempo, Some(128.0));
        assert!(context.is_playing);
        assert!(context.is_recording);
        assert!(context.is_loop_active);
        assert_eq!(context.time_signature, Some((7, 8)));
        assert_eq!(context.song_pos_beats, Some(8.5));
        assert_eq!(context.song_pos_seconds, Some(2.0));
        assert_eq!(context.bar_start_beats, Some(8.0));
        assert_eq!(context.bar_number, Some(3));
        assert_eq!(context.loop_beats, Some((4.0, 12.0)));
        // The sample cursor is derived from the seconds timeline.
        assert_eq!(context.sample_pos, 96_000);
    }

    #[test]
    fn a_host_without_a_transport_leaves_every_musical_field_absent() {
        let context = observe_transport(None);

        assert_eq!(context.tempo, None);
        assert!(context.is_playing, "a transport-less host keeps running");
        assert!(!context.is_recording);
        assert!(!context.is_loop_active);
        assert_eq!(context.time_signature, None);
        assert_eq!(context.song_pos_beats, None);
        assert_eq!(context.bar_number, None);
        assert_eq!(context.loop_beats, None);
        assert_eq!(context.sample_pos, 0);
    }

    // ======= Raw host event -> core queue (Phase 3 M1 item 4) =======

    /// What the plugin actually found in its core `EventQueue`, recorded from
    /// inside `process`. The `_rs` layer and core are tested separately; this
    /// closes the gap in the middle — the backend adapter.
    #[derive(Debug, Default, PartialEq)]
    struct SeenEvents {
        param_mods: Vec<(String, f32, u32)>,
        expressions: Vec<(String, Option<u8>, Option<u8>, Option<i32>, f64, u32)>,
        param_changes: Vec<String>,
    }

    static SEEN: Mutex<Option<SeenEvents>> = Mutex::new(None);

    struct ExprParams {
        depth: FloatParam,
    }

    impl Default for ExprParams {
        fn default() -> Self {
            Self {
                depth: FloatParam::new("depth", "Depth", 0.5, 0.0, 1.0),
            }
        }
    }

    impl Params for ExprParams {
        fn get_normalized(&self, id: &str) -> Option<f32> {
            (id == "depth").then(|| self.depth.get())
        }

        fn set_normalized(&self, id: &str, value: f32) {
            if id == "depth" {
                self.depth.set(value);
            }
        }

        fn descriptors(&self) -> Vec<ParamDescriptor> {
            vec![ParamDescriptor {
                id: "depth",
                numeric_id: sunmao_core::stable_param_id("depth"),
                name: self.depth.name,
                default_normalized: 0.5,
                step_count: 0,
                kind: ParamKind::Float,
                group: "",
            }]
        }
    }

    #[derive(Default)]
    struct ExprPlugin {
        params: Arc<ExprParams>,
    }

    impl SunmaoPlugin for ExprPlugin {
        const NAME: &'static str = "Expression Mapping Test";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        type Params = ExprParams;

        fn accepts_midi(&self) -> bool {
            true
        }

        fn params(&self) -> Arc<Self::Params> {
            self.params.clone()
        }

        fn process(
            &mut self,
            _buffer: &mut AudioBuffer,
            events: &EventQueue,
            _context: &SunmaoProcessContext,
        ) -> ProcessStatus {
            let mut seen = SeenEvents::default();
            for event in events.iter() {
                match event {
                    SunmaoEvent::ParamMod { id, amount, offset } => {
                        seen.param_mods.push((id.to_string(), *amount, *offset));
                    }
                    SunmaoEvent::NoteExpression(expression) => {
                        seen.expressions.push((
                            format!("{:?}", expression.kind),
                            expression.channel,
                            expression.key,
                            expression.note_id,
                            expression.value,
                            expression.offset,
                        ));
                    }
                    _ => {}
                }
            }
            // Modulation must never surface as automation, whatever route it
            // took to get here.
            for change in events.param_changes() {
                seen.param_changes.push(change.id.to_string());
            }
            *SEEN.lock().unwrap() = Some(seen);
            ProcessStatus::Normal
        }
    }

    /// Raw CLAP events of mixed kinds, laid out as a host would deliver them.
    struct MixedInputEvents {
        expression: clap_event_note_expression_t,
        param_mod: clap_event_param_mod_t,
    }

    unsafe extern "C" fn mixed_event_count(_list: *const clap_input_events_t) -> u32 {
        2
    }

    unsafe extern "C" fn mixed_event_get(
        list: *const clap_input_events_t,
        index: u32,
    ) -> *const clap_event_header_t {
        let events = unsafe { &*((*list).ctx as *const MixedInputEvents) };
        match index {
            0 => &events.expression.header as *const clap_event_header_t,
            1 => &events.param_mod.header as *const clap_event_header_t,
            _ => std::ptr::null(),
        }
    }

    #[test]
    fn raw_clap_expression_and_mod_events_reach_the_core_queue() {
        *SEEN.lock().unwrap() = None;

        let numeric_depth_id = sunmao_core::stable_param_id("depth");

        let plugin = unsafe {
            clap_rs::entry::PluginEntry::create_plugin::<SunmaoClapWrapper<ExprPlugin>>(
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert!(!plugin.is_null());
        unsafe {
            assert!(((*plugin).init.unwrap())(plugin));
            assert!(((*plugin).activate.unwrap())(plugin, 48_000.0, 1, 8));
            assert!(((*plugin).start_processing.unwrap())(plugin));
        }

        let mut raw_events = MixedInputEvents {
            expression: clap_event_note_expression_t {
                header: clap_event_header_t {
                    size: std::mem::size_of::<clap_event_note_expression_t>() as u32,
                    time: 3,
                    space_id: CLAP_CORE_EVENT_SPACE_ID,
                    type_: CLAP_EVENT_NOTE_EXPRESSION,
                    flags: 0,
                },
                expression_id: CLAP_NOTE_EXPRESSION_TUNING,
                note_id: 42,
                port_index: 0,
                channel: 1,
                key: 64,
                value: 0.25,
            },
            param_mod: clap_event_param_mod_t {
                header: clap_event_header_t {
                    size: std::mem::size_of::<clap_event_param_mod_t>() as u32,
                    time: 5,
                    space_id: CLAP_CORE_EVENT_SPACE_ID,
                    type_: CLAP_EVENT_PARAM_MOD,
                    flags: 0,
                },
                param_id: numeric_depth_id,
                cookie: std::ptr::null_mut(),
                note_id: -1,
                port_index: -1,
                channel: -1,
                key: -1,
                amount: -0.75,
            },
        };
        let input_events = clap_input_events_t {
            ctx: (&mut raw_events as *mut MixedInputEvents).cast::<c_void>(),
            size: Some(mixed_event_count),
            get: Some(mixed_event_get),
        };

        let input_left = [0.0_f32; 8];
        let input_right = [0.0_f32; 8];
        let mut input_channels = [
            input_left.as_ptr() as *mut f32,
            input_right.as_ptr() as *mut f32,
        ];
        let input_buffers = [clap_audio_buffer_t {
            data32: input_channels.as_mut_ptr(),
            data64: std::ptr::null_mut(),
            channel_count: 2,
            latency: 0,
            constant_mask: 0,
        }];
        let mut output_left = [0.0_f32; 8];
        let mut output_right = [0.0_f32; 8];
        let mut output_channels = [output_left.as_mut_ptr(), output_right.as_mut_ptr()];
        let mut output_buffers = [clap_audio_buffer_t {
            data32: output_channels.as_mut_ptr(),
            data64: std::ptr::null_mut(),
            channel_count: 2,
            latency: 0,
            constant_mask: 0,
        }];
        let process = clap_process_t {
            steady_time: 0,
            frames_count: 8,
            transport: std::ptr::null(),
            audio_inputs: input_buffers.as_ptr(),
            audio_outputs: output_buffers.as_mut_ptr(),
            audio_inputs_count: 1,
            audio_outputs_count: 1,
            in_events: &input_events,
            out_events: std::ptr::null(),
        };

        let status = unsafe { ((*plugin).process.unwrap())(plugin, &process) };
        assert_eq!(status, CLAP_PROCESS_CONTINUE);

        let seen = SEEN.lock().unwrap().take().expect("process ran");

        // The expression survives the whole chain with its addressing intact.
        // CLAP carries channel and key, so unlike VST3 they must be `Some`.
        assert_eq!(
            seen.expressions,
            vec![("Tuning".to_string(), Some(1), Some(64), Some(42), 0.25, 3)],
            "raw CLAP note expression must arrive as a core NoteExpression"
        );

        // The modulation arrives keyed by the *string* id, translated from the
        // numeric CLAP id by the backend.
        assert_eq!(
            seen.param_mods,
            vec![("depth".to_string(), -0.75, 5)],
            "raw CLAP param mod must arrive as a core ParamMod"
        );

        // And it must not have leaked into the automation stream, which is what
        // would put a modulation into saved state.
        assert!(
            seen.param_changes.is_empty(),
            "modulation must never appear as a parameter change, got {:?}",
            seen.param_changes
        );

        unsafe {
            ((*plugin).stop_processing.unwrap())(plugin);
            ((*plugin).deactivate.unwrap())(plugin);
            ((*plugin).destroy.unwrap())(plugin);
        }
    }

    // ======= State migration through the real CLAP state extension =======

    static MIGRATIONS: Mutex<Vec<u32>> = Mutex::new(Vec::new());
    static MIGRATION_TEST_LOCK: Mutex<()> = Mutex::new(());

    struct MigrationParams {
        depth: FloatParam,
    }

    impl Default for MigrationParams {
        fn default() -> Self {
            Self {
                depth: FloatParam::new("depth", "Depth", 0.5, 0.0, 1.0),
            }
        }
    }

    impl Params for MigrationParams {
        fn get_normalized(&self, id: &str) -> Option<f32> {
            (id == "depth").then(|| self.depth.get())
        }

        fn set_normalized(&self, id: &str, value: f32) {
            if id == "depth" {
                self.depth.set(value);
            }
        }

        fn descriptors(&self) -> Vec<ParamDescriptor> {
            vec![ParamDescriptor {
                id: "depth",
                numeric_id: sunmao_core::stable_param_id("depth"),
                name: self.depth.name,
                default_normalized: 0.5,
                step_count: 0,
                kind: ParamKind::Float,
                group: "",
            }]
        }
    }

    /// A build whose state format has moved on to version 2.
    #[derive(Default)]
    struct MigrationPlugin {
        params: Arc<MigrationParams>,
    }

    impl SunmaoPlugin for MigrationPlugin {
        const NAME: &'static str = "State Migration Test";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        const STATE_VERSION: u32 = 2;
        type Params = MigrationParams;

        fn params(&self) -> Arc<Self::Params> {
            self.params.clone()
        }

        fn migrate_state(&mut self, from_version: u32) {
            MIGRATIONS.lock().unwrap().push(from_version);
        }

        fn process(
            &mut self,
            _buffer: &mut AudioBuffer,
            _events: &EventQueue,
            _context: &SunmaoProcessContext,
        ) -> ProcessStatus {
            ProcessStatus::Normal
        }
    }

    struct ByteReader {
        bytes: Vec<u8>,
        position: usize,
    }

    unsafe extern "C" fn byte_reader_read(
        stream: *const clap_istream_t,
        buffer: *mut c_void,
        size: u64,
    ) -> i64 {
        let reader = unsafe { &mut *((*stream).ctx as *mut ByteReader) };
        let remaining = reader.bytes.len() - reader.position;
        let count = remaining.min(size as usize);
        unsafe {
            std::ptr::copy_nonoverlapping(
                reader.bytes[reader.position..].as_ptr(),
                buffer.cast::<u8>(),
                count,
            );
        }
        reader.position += count;
        count as i64
    }

    /// Builds a parameter-state blob exactly as a build stamped `version`
    /// would have written it.
    fn state_blob(version: u32, numeric_id: u32, value: f64) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"SMCLPRM\0");
        bytes.extend_from_slice(&version.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&numeric_id.to_le_bytes());
        bytes.extend_from_slice(&value.to_le_bytes());
        bytes
    }

    /// Loads `bytes` through the plugin's real `clap.state` extension.
    fn load_state_blob(plugin: *const clap_sys::plugin::clap_plugin_t, bytes: Vec<u8>) -> bool {
        let mut reader = ByteReader { bytes, position: 0 };
        let stream = clap_istream_t {
            ctx: (&mut reader as *mut ByteReader).cast::<c_void>(),
            read: Some(byte_reader_read),
        };
        unsafe {
            let ext = ((*plugin).get_extension.unwrap())(
                plugin,
                clap_sys::ext::state::CLAP_EXT_STATE.as_ptr().cast(),
            ) as *const clap_sys::ext::state::clap_plugin_state_t;
            assert!(!ext.is_null(), "plugin must expose clap.state");
            ((*ext).load.unwrap())(plugin, &stream)
        }
    }

    #[test]
    fn clap_state_from_an_older_build_triggers_migration() {
        let _serialize = MIGRATION_TEST_LOCK.lock().unwrap();
        MIGRATIONS.lock().unwrap().clear();
        let numeric_id = sunmao_core::stable_param_id("depth");

        let plugin = unsafe {
            clap_rs::entry::PluginEntry::create_plugin::<SunmaoClapWrapper<MigrationPlugin>>(
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert!(!plugin.is_null());
        unsafe { assert!(((*plugin).init.unwrap())(plugin)) };

        // A blob written by version 1 is accepted, and the plugin is told which
        // version it came from so it can reinterpret values.
        assert!(load_state_blob(plugin, state_blob(1, numeric_id, 0.25)));
        assert_eq!(
            MIGRATIONS.lock().unwrap().as_slice(),
            &[1],
            "loading an older state must call migrate_state with its version"
        );

        // The current version needs no migration.
        MIGRATIONS.lock().unwrap().clear();
        assert!(load_state_blob(plugin, state_blob(2, numeric_id, 0.5)));
        assert!(
            MIGRATIONS.lock().unwrap().is_empty(),
            "a same-version state must not be migrated"
        );

        // A future version is refused outright rather than misread, and no
        // migration is attempted.
        assert!(!load_state_blob(plugin, state_blob(3, numeric_id, 0.5)));
        assert!(MIGRATIONS.lock().unwrap().is_empty());

        unsafe { ((*plugin).destroy.unwrap())(plugin) };
    }

    #[test]
    fn clap_saved_state_carries_the_plugin_state_version() {
        // The blob must be stamped with the plugin's version, not a constant,
        // or a future build could never tell what it is reading.
        let _serialize = MIGRATION_TEST_LOCK.lock().unwrap();
        let plugin = unsafe {
            clap_rs::entry::PluginEntry::create_plugin::<SunmaoClapWrapper<MigrationPlugin>>(
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        unsafe { assert!(((*plugin).init.unwrap())(plugin)) };

        struct ByteWriter {
            bytes: Vec<u8>,
        }
        unsafe extern "C" fn byte_writer_write(
            stream: *const clap_sys::stream::clap_ostream_t,
            buffer: *const c_void,
            size: u64,
        ) -> i64 {
            let writer = unsafe { &mut *((*stream).ctx as *mut ByteWriter) };
            let slice = unsafe { std::slice::from_raw_parts(buffer.cast::<u8>(), size as usize) };
            writer.bytes.extend_from_slice(slice);
            size as i64
        }

        let mut writer = ByteWriter { bytes: Vec::new() };
        let stream = clap_sys::stream::clap_ostream_t {
            ctx: (&mut writer as *mut ByteWriter).cast::<c_void>(),
            write: Some(byte_writer_write),
        };
        unsafe {
            let ext = ((*plugin).get_extension.unwrap())(
                plugin,
                clap_sys::ext::state::CLAP_EXT_STATE.as_ptr().cast(),
            ) as *const clap_sys::ext::state::clap_plugin_state_t;
            assert!(((*ext).save.unwrap())(plugin, &stream));
            ((*plugin).destroy.unwrap())(plugin);
        }

        assert_eq!(&writer.bytes[..8], b"SMCLPRM\0");
        assert_eq!(
            u32::from_le_bytes(writer.bytes[8..12].try_into().unwrap()),
            MigrationPlugin::STATE_VERSION
        );
    }

    // ======= Preset load through the real CLAP extension =======

    static PRESETS: Mutex<Vec<String>> = Mutex::new(Vec::new());

    #[derive(Default)]
    struct PresetPlugin;

    impl SunmaoPlugin for PresetPlugin {
        const NAME: &'static str = "Preset Load Test";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        const SUPPORTS_PRESET_LOAD: bool = true;
        type Params = RealtimeParams;

        fn params(&self) -> Arc<Self::Params> {
            Arc::new(RealtimeParams)
        }

        fn load_preset(&mut self, location: SunmaoPresetLocation<'_>) -> bool {
            PRESETS.lock().unwrap().push(format!("{location:?}"));
            // Refuse one key so the failure path is observable through the ABI.
            !matches!(
                location,
                SunmaoPresetLocation::Internal { key: Some("bad") }
            )
        }

        fn process(
            &mut self,
            _buffer: &mut AudioBuffer,
            _events: &EventQueue,
            _context: &SunmaoProcessContext,
        ) -> ProcessStatus {
            ProcessStatus::Normal
        }
    }

    /// A plugin that has not opted in must not advertise the extension.
    #[derive(Default)]
    struct NoPresetPlugin;

    impl SunmaoPlugin for NoPresetPlugin {
        const NAME: &'static str = "No Preset Test";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        type Params = RealtimeParams;

        fn params(&self) -> Arc<Self::Params> {
            Arc::new(RealtimeParams)
        }

        fn process(
            &mut self,
            _buffer: &mut AudioBuffer,
            _events: &EventQueue,
            _context: &SunmaoProcessContext,
        ) -> ProcessStatus {
            ProcessStatus::Normal
        }
    }

    #[test]
    fn clap_preset_load_reaches_the_plugin_with_its_location() {
        use clap_rs::clap_sys::ext::preset_load::{
            clap_plugin_preset_load_t, CLAP_EXT_PRESET_LOAD, CLAP_EXT_PRESET_LOAD_COMPAT,
        };
        use clap_rs::clap_sys::factory::preset_discovery::{
            CLAP_PRESET_DISCOVERY_LOCATION_FILE, CLAP_PRESET_DISCOVERY_LOCATION_PLUGIN,
        };
        use std::ffi::CString;

        PRESETS.lock().unwrap().clear();

        let plugin = unsafe {
            clap_rs::entry::PluginEntry::create_plugin::<SunmaoClapWrapper<PresetPlugin>>(
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert!(!plugin.is_null());
        unsafe { assert!(((*plugin).init.unwrap())(plugin)) };

        let ext = unsafe {
            ((*plugin).get_extension.unwrap())(plugin, CLAP_EXT_PRESET_LOAD.as_ptr().cast())
        } as *const clap_plugin_preset_load_t;
        assert!(
            !ext.is_null(),
            "an opted-in plugin must expose the extension"
        );
        // Hosts may still probe the draft id.
        let compat = unsafe {
            ((*plugin).get_extension.unwrap())(plugin, CLAP_EXT_PRESET_LOAD_COMPAT.as_ptr().cast())
        };
        assert_eq!(compat, ext as *const c_void);

        let from_location = unsafe { (*ext).from_location.unwrap() };
        let path = CString::new("/presets/bank.clap-preset").unwrap();
        let key = CString::new("lead").unwrap();

        // A file location carries both path and key through to the plugin.
        assert!(unsafe {
            from_location(
                plugin,
                CLAP_PRESET_DISCOVERY_LOCATION_FILE,
                path.as_ptr(),
                key.as_ptr(),
            )
        });
        // A plugin-internal location has no path.
        assert!(unsafe {
            from_location(
                plugin,
                CLAP_PRESET_DISCOVERY_LOCATION_PLUGIN,
                std::ptr::null(),
                key.as_ptr(),
            )
        });
        // A plugin refusal surfaces as false rather than being swallowed.
        let bad = CString::new("bad").unwrap();
        assert!(!unsafe {
            from_location(
                plugin,
                CLAP_PRESET_DISCOVERY_LOCATION_PLUGIN,
                std::ptr::null(),
                bad.as_ptr(),
            )
        });

        assert_eq!(
            PRESETS.lock().unwrap().as_slice(),
            &[
                "File { path: \"/presets/bank.clap-preset\", key: Some(\"lead\") }".to_string(),
                "Internal { key: Some(\"lead\") }".to_string(),
                "Internal { key: Some(\"bad\") }".to_string(),
            ],
            "each location must reach the plugin unaltered"
        );

        // A null path for a file location, and an unknown location kind, are
        // both refused before the plugin is asked.
        let before = PRESETS.lock().unwrap().len();
        assert!(!unsafe {
            from_location(
                plugin,
                CLAP_PRESET_DISCOVERY_LOCATION_FILE,
                std::ptr::null(),
                std::ptr::null(),
            )
        });
        assert!(!unsafe { from_location(plugin, 9_999, path.as_ptr(), std::ptr::null()) });
        assert_eq!(PRESETS.lock().unwrap().len(), before);

        unsafe { ((*plugin).destroy.unwrap())(plugin) };
    }

    #[test]
    fn a_plugin_without_preset_support_does_not_expose_the_extension() {
        use clap_rs::clap_sys::ext::preset_load::CLAP_EXT_PRESET_LOAD;

        let plugin = unsafe {
            clap_rs::entry::PluginEntry::create_plugin::<SunmaoClapWrapper<NoPresetPlugin>>(
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        unsafe { assert!(((*plugin).init.unwrap())(plugin)) };
        let ext = unsafe {
            ((*plugin).get_extension.unwrap())(plugin, CLAP_EXT_PRESET_LOAD.as_ptr().cast())
        };
        assert!(
            ext.is_null(),
            "a host must not be offered a preset loader that always refuses"
        );
        unsafe { ((*plugin).destroy.unwrap())(plugin) };
    }

    // ======= Parameter grouping =======

    #[derive(Default)]
    struct GroupedPlugin {
        params: Arc<GroupedTestParams>,
    }

    pub struct GroupedTestParams {
        level: FloatParam,
        detune: FloatParam,
        cutoff: FloatParam,
    }

    impl Default for GroupedTestParams {
        fn default() -> Self {
            Self {
                level: FloatParam::new("level", "Level", 0.5, 0.0, 1.0),
                detune: FloatParam::new("detune", "Detune", 0.0, -1.0, 1.0),
                cutoff: FloatParam::new("cutoff", "Cutoff", 0.5, 0.0, 1.0),
            }
        }
    }

    impl Params for GroupedTestParams {
        fn get_normalized(&self, id: &str) -> Option<f32> {
            match id {
                "level" => Some(self.level.get()),
                "detune" => Some(self.detune.get()),
                "cutoff" => Some(self.cutoff.get()),
                _ => None,
            }
        }

        fn set_normalized(&self, _id: &str, _value: f32) {}

        fn descriptors(&self) -> Vec<ParamDescriptor> {
            vec![
                ParamDescriptor {
                    id: "level",
                    numeric_id: sunmao_core::stable_param_id("level"),
                    name: "Level",
                    default_normalized: 0.5,
                    step_count: 0,
                    kind: ParamKind::Float,
                    group: "Osc",
                },
                ParamDescriptor {
                    id: "detune",
                    numeric_id: sunmao_core::stable_param_id("detune"),
                    name: "Detune",
                    default_normalized: 0.5,
                    step_count: 0,
                    kind: ParamKind::Float,
                    group: "Osc/Tuning",
                },
                ParamDescriptor {
                    id: "cutoff",
                    numeric_id: sunmao_core::stable_param_id("cutoff"),
                    name: "Cutoff",
                    default_normalized: 0.5,
                    step_count: 0,
                    kind: ParamKind::Float,
                    group: "",
                },
            ]
        }
    }

    impl SunmaoPlugin for GroupedPlugin {
        const NAME: &'static str = "Grouped Test";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        type Params = GroupedTestParams;

        fn params(&self) -> Arc<Self::Params> {
            self.params.clone()
        }

        fn process(
            &mut self,
            _buffer: &mut AudioBuffer,
            _events: &EventQueue,
            _context: &SunmaoProcessContext,
        ) -> ProcessStatus {
            ProcessStatus::Normal
        }
    }

    #[test]
    fn clap_reports_each_parameter_group_as_a_module_path() {
        use clap_rs::clap_sys::ext::params::{clap_param_info_t, CLAP_EXT_PARAMS};

        let plugin = unsafe {
            clap_rs::entry::PluginEntry::create_plugin::<SunmaoClapWrapper<GroupedPlugin>>(
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert!(!plugin.is_null());
        unsafe { assert!(((*plugin).init.unwrap())(plugin)) };

        let ext =
            unsafe { ((*plugin).get_extension.unwrap())(plugin, CLAP_EXT_PARAMS.as_ptr().cast()) }
                as *const clap_rs::clap_sys::ext::params::clap_plugin_params_t;
        assert!(!ext.is_null());

        let count = unsafe { ((*ext).count.unwrap())(plugin) };
        assert_eq!(count, 3);

        let mut modules = Vec::new();
        for index in 0..count {
            let mut info: clap_param_info_t = unsafe { std::mem::zeroed() };
            assert!(unsafe { ((*ext).get_info.unwrap())(plugin, index, &mut info) });
            let len = info
                .module
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(info.module.len());
            let bytes: Vec<u8> = info.module[..len].iter().map(|b| *b as u8).collect();
            let name_len = info
                .name
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(info.name.len());
            let name: Vec<u8> = info.name[..name_len].iter().map(|b| *b as u8).collect();
            modules.push((
                String::from_utf8_lossy(&name).into_owned(),
                String::from_utf8_lossy(&bytes).into_owned(),
            ));
        }

        // The path reaches the host verbatim, including the nested level. This
        // field was previously zeroed unconditionally.
        assert_eq!(
            modules,
            vec![
                ("Level".to_string(), "Osc".to_string()),
                ("Detune".to_string(), "Osc/Tuning".to_string()),
                ("Cutoff".to_string(), String::new()),
            ]
        );

        unsafe { ((*plugin).destroy.unwrap())(plugin) };
    }
}

/// Entry type alias for sunmao_export! macro
pub type ClapEntry = clap_rs::clap_sys::entry::clap_plugin_entry_t;

/// Thread-safe wrapper for CLAP feature lists.
/// CLAP feature identifiers.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ClapFeature {
    Instrument,
    AudioEffect,
    NoteEffect,
    NoteDetector,
    Analyzer,
    Synthesizer,
    Sampler,
    Drum,
    DrumMachine,
    Filter,
    Phaser,
    Equalizer,
    DeEsser,
    PhaseVocoder,
    Granular,
    FrequencyShifter,
    PitchShifter,
    Distortion,
    TransientShaper,
    Compressor,
    Expander,
    Gate,
    Limiter,
    Flanger,
    Chorus,
    Delay,
    Reverb,
    Tremolo,
    Glitch,
    Utility,
    Mono,
    Stereo,
    Surround,
    Ambisonic,
    PitchCorrection,
    Restoration,
    MultiEffects,
    Mixing,
    Mastering,
}

impl ClapFeature {
    pub const fn as_ptr(self) -> *const std::ffi::c_char {
        match self {
            ClapFeature::Instrument => b"instrument\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::AudioEffect => b"audio-effect\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::NoteEffect => b"note-effect\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::NoteDetector => b"note-detector\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Analyzer => b"analyzer\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Synthesizer => b"synthesizer\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Sampler => b"sampler\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Drum => b"drum\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::DrumMachine => b"drum-machine\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Filter => b"filter\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Phaser => b"phaser\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Equalizer => b"equalizer\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::DeEsser => b"de-esser\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::PhaseVocoder => b"phase-vocoder\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Granular => b"granular\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::FrequencyShifter => {
                b"frequency-shifter\0".as_ptr() as *const std::ffi::c_char
            }
            ClapFeature::PitchShifter => b"pitch-shifter\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Distortion => b"distortion\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::TransientShaper => {
                b"transient-shaper\0".as_ptr() as *const std::ffi::c_char
            }
            ClapFeature::Compressor => b"compressor\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Expander => b"expander\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Gate => b"gate\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Limiter => b"limiter\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Flanger => b"flanger\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Chorus => b"chorus\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Delay => b"delay\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Reverb => b"reverb\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Tremolo => b"tremolo\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Glitch => b"glitch\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Utility => b"utility\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::PitchCorrection => {
                b"pitch-correction\0".as_ptr() as *const std::ffi::c_char
            }
            ClapFeature::Restoration => b"restoration\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::MultiEffects => b"multi-effects\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Mixing => b"mixing\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Mastering => b"mastering\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Mono => b"mono\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Stereo => b"stereo\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Surround => b"surround\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Ambisonic => b"ambisonic\0".as_ptr() as *const std::ffi::c_char,
        }
    }
}

/// Thread-safe wrapper for CLAP feature lists (null-terminated).
pub struct ClapFeatures(&'static [*const std::ffi::c_char]);
unsafe impl Sync for ClapFeatures {}
unsafe impl Send for ClapFeatures {}

impl ClapFeatures {
    pub const fn new(features: &'static [*const std::ffi::c_char]) -> Self {
        Self(features)
    }

    pub const fn as_ptr(&self) -> *const *const std::ffi::c_char {
        self.0.as_ptr()
    }
}
