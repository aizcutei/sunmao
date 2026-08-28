//! VST3 Backend Adapter for SunMao.
//!
//! This crate wraps `SunmaoPlugin` to expose it as a VST3 plugin via `vst3_rs`.

use raw_window_handle::RawWindowHandle;
use std::collections::HashMap;
#[cfg(target_os = "windows")]
use std::ffi::c_void;
use std::sync::Arc;
use sunmao_core::events::MidiMessage;
use sunmao_core::plugin::{
    BusRole as SunmaoBusRole, ProcessContext as SunmaoProcessContext,
    RenderMode as SunmaoRenderMode, TailLength as SunmaoTailLength,
};
use sunmao_core::view::ViewContext;
use sunmao_core::{
    AudioBuffer, Event, EventQueue, ParamDescriptor, Params, ParentWindow, SunmaoPlugin, ViewHandle,
};
use vst3_rs::gui::prepare_view;
use vst3_rs::gui::GuiSize;
use vst3_rs::process::{
    MAX_PROCESS_AUDIO_SAMPLES, MAX_PROCESS_CHANNELS, MAX_PROCESS_EVENTS, MAX_PROCESS_FRAMES,
};
use vst3_rs::{
    AudioConfig, GuiPlugin, HostHandle, ParamChange as Vst3ParamChange, ParamInfo, ParameterBridge,
    Plugin, PluginInfo, PortConfig, PortType, ProcessContext, ProcessError, ProcessResult,
};

pub use vst3_rs::{export_vst3_plugin, export_vst3_plugin_with_gui};

/// VST3's "no tail" encoding (`kNoTail`).
const fn vst3_no_tail() -> u32 {
    vst3_rs::vst3_sys::vst::types::kNoTail
}

/// VST3's "unbounded tail" encoding (`kInfiniteTail`).
const fn vst3_infinite_tail() -> u32 {
    vst3_rs::vst3_sys::vst::types::kInfiniteTail
}

/// First channel index of each bus, plus a trailing end marker, for
/// `AudioBuffer::with_input_bus_bounds`.
fn bus_bounds(buses: &[sunmao_core::plugin::BusInfo]) -> Vec<usize> {
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

/// Encodes a unified tail length for VST3.
///
/// `kInfiniteTail` is a reserved magic value, so a finite tail that happens to
/// land on it is reported one sample shorter rather than silently becoming
/// unbounded.
fn clamp_vst3_tail(tail: SunmaoTailLength) -> u32 {
    match tail {
        SunmaoTailLength::None => vst3_no_tail(),
        SunmaoTailLength::Samples(samples) => samples.min(vst3_infinite_tail() - 1),
        SunmaoTailLength::Infinite => vst3_infinite_tail(),
    }
}

/// Wrapper that adapts a SunmaoPlugin to vst3_rs::Plugin.
pub struct SunmaoVst3Wrapper<P: SunmaoPlugin> {
    plugin: P,
    params: Arc<P::Params>,
    sample_rate: f64,
    input_channel_count: usize,
    output_channel_count: usize,
    // Temporary buffers for deinterleaving
    input_buffers: Vec<Vec<f32>>,
    output_buffers: Vec<Vec<f32>>,
    // MIDI event queue for synths
    pending_midi: Vec<PendingMidi>,
    // False when the plugin-declared event bound could not be reserved during
    // construction. In that case note callbacks must fail closed instead of
    // allocating on the realtime thread.
    pending_midi_capacity_available: bool,
    event_capacity_available: bool,
    event_overflowed: bool,
    event_queue: EventQueue,
    // GUI View Handle
    view_handle: Option<ViewHandle>,
    // Shared parameter store (shared with GUI/controller)
    shared_params: Arc<ParameterBridge>,
    host: HostHandle,
    param_descriptors: Vec<ParamDescriptor>,
    /// Precomputed at construction so the audio thread never rebuilds it.
    input_bus_bounds: Vec<usize>,
}

#[derive(Clone, Copy)]
struct PendingMidi {
    message: MidiMessage,
    sequence: u32,
}

struct Vst3ParamsViewContext<P: Params> {
    params: Arc<P>,
    shared: Arc<ParameterBridge>,
    host: HostHandle,
    id_to_numeric: HashMap<&'static str, u32>,
}

fn parent_window_from_raw(parent: RawWindowHandle) -> Option<ParentWindow> {
    match parent {
        #[cfg(target_os = "macos")]
        RawWindowHandle::AppKit(handle) => {
            Some(ParentWindow::AppKit(handle.ns_view.as_ptr().cast()))
        }
        #[cfg(target_os = "windows")]
        RawWindowHandle::Win32(handle) => {
            Some(ParentWindow::Win32(handle.hwnd.get() as *mut c_void))
        }
        #[cfg(target_os = "linux")]
        RawWindowHandle::Xcb(handle) => Some(ParentWindow::X11(handle.window.get())),
        #[cfg(target_os = "linux")]
        RawWindowHandle::Xlib(handle) => u32::try_from(handle.window).ok().map(ParentWindow::X11),
        _ => None,
    }
}

fn append_timed_events(
    parameter_changes: &[Vst3ParamChange],
    midi_events: &mut Vec<PendingMidi>,
    descriptors: &[ParamDescriptor],
    events: &mut EventQueue,
) -> bool {
    midi_events.sort_unstable_by_key(|event| (event.message.offset, event.sequence));
    let mut parameter_index = 0;
    let mut midi_index = 0;
    let mut success = true;

    while parameter_index < parameter_changes.len() || midi_index < midi_events.len() {
        let take_parameter = parameter_index < parameter_changes.len()
            && (midi_index == midi_events.len()
                || parameter_changes[parameter_index].sample_offset
                    <= midi_events[midi_index].message.offset);

        if take_parameter {
            let change = parameter_changes[parameter_index];
            parameter_index += 1;
            if let Some(descriptor) = descriptors
                .iter()
                .find(|descriptor| descriptor.numeric_id == change.id)
            {
                if !events.push_param_change(sunmao_core::ParamChange {
                    id: descriptor.id,
                    value: change.value as f32,
                    offset: change.sample_offset,
                }) {
                    success = false;
                    break;
                }
            }
        } else {
            if !events.push(Event::Midi(midi_events[midi_index].message)) {
                success = false;
                break;
            }
            midi_index += 1;
        }
    }

    midi_events.clear();
    success
}

/// Convert a VST3 note payload to the framework's bounded MIDI representation.
///
/// VST3 transports channel and pitch as signed values and velocity as a float.
/// Do not let malformed host data wrap through integer casts into a different
/// MIDI note/channel; finite velocity values outside the MIDI range are
/// clamped, matching the CLAP adapter's policy.
fn note_event_to_midi(
    sample_offset: u32,
    channel: i16,
    pitch: i16,
    velocity: f32,
    note_on: bool,
) -> Option<MidiMessage> {
    if !(0..=15).contains(&channel) || !(0..=127).contains(&pitch) || !velocity.is_finite() {
        return None;
    }
    let velocity = (velocity.clamp(0.0, 1.0) * 127.0).round() as u8;
    Some(if note_on {
        MidiMessage::note_on(sample_offset, channel as u8, pitch as u8, velocity)
    } else {
        MidiMessage::note_off(sample_offset, channel as u8, pitch as u8, velocity)
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

fn allocate_audio_buffers(channel_count: usize, max_frames: u32) -> Option<Vec<Vec<f32>>> {
    if max_frames > MAX_PROCESS_FRAMES
        || channel_count > MAX_PROCESS_CHANNELS
        || channel_count
            .checked_mul(max_frames as usize)
            .is_none_or(|samples| samples > MAX_PROCESS_AUDIO_SAMPLES)
    {
        return None;
    }
    let max_frames = usize::try_from(max_frames).ok()?;
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

impl<P: Params> Vst3ParamsViewContext<P> {
    fn new(params: Arc<P>, shared: Arc<ParameterBridge>, host: HostHandle) -> Self {
        let id_to_numeric = params
            .descriptors()
            .into_iter()
            .map(|descriptor| (descriptor.id, descriptor.numeric_id))
            .collect();
        Self {
            params,
            shared,
            host,
            id_to_numeric,
        }
    }
}

impl<P: Params> ViewContext for Vst3ParamsViewContext<P> {
    fn get_param(&self, id: &str) -> Option<f32> {
        let numeric_id = *self.id_to_numeric.get(id)?;
        let value = self.shared.get(numeric_id) as f32;
        self.params.set_normalized(id, value);
        Some(value)
    }

    fn set_param(&self, id: &str, value: f32) {
        let Some(&numeric_id) = self.id_to_numeric.get(id) else {
            return;
        };
        self.params.set_normalized(id, value);
        let normalized = self
            .params
            .get_normalized(id)
            .unwrap_or(value.clamp(0.0, 1.0));
        self.shared.set(numeric_id, normalized as f64);
        self.host.perform_edit(numeric_id, normalized as f64);
    }

    fn begin_edit(&self, id: &str) {
        if let Some(&numeric_id) = self.id_to_numeric.get(id) {
            self.host.begin_edit(numeric_id);
        }
    }

    fn end_edit(&self, id: &str) {
        if let Some(&numeric_id) = self.id_to_numeric.get(id) {
            self.host.end_edit(numeric_id);
        }
    }

    fn request_resize(&self, width: u32, height: u32) -> bool {
        self.host.request_resize(width, height)
    }
}

impl<P: SunmaoPlugin> Plugin for SunmaoVst3Wrapper<P> {
    const MAX_EVENTS_PER_BLOCK: usize = P::MAX_EVENTS_PER_BLOCK;

    fn info() -> PluginInfo {
        PluginInfo {
            id: P::NAME,
            name: P::NAME,
            vendor: P::VENDOR,
            url: P::URL,
            email: "",
            version: P::VERSION,
            category: if P::default().input_channels() == 0 {
                "Instrument|Synth"
            } else {
                "Fx"
            },
        }
    }

    fn class_id() -> [i8; 16] {
        let configured = P::vst3_info().class_id;
        if configured == [0; 16] {
            vst3_rs::class_id_from_str(&format!("{}::{}", P::VENDOR, P::NAME))
        } else {
            configured.map(|byte| byte as i8)
        }
    }

    fn new(host: HostHandle) -> Self {
        let plugin = P::default();
        let params = plugin.params();
        let param_descriptors = params
            .validated_descriptors()
            .unwrap_or_else(|error| panic!("invalid SunMao parameter layout: {error}"));
        let shared_params = host.parameter_bridge();
        // The declared buses are authoritative for the flattened channel
        // topology, matching what `audio_config` reports to the host.
        let input_buses = plugin.input_buses();
        let input_bus_bounds = bus_bounds(&input_buses);
        let input_channels = input_buses
            .iter()
            .map(|bus| bus.channels as usize)
            .sum::<usize>();
        let output_channels = plugin
            .output_buses()
            .iter()
            .map(|bus| bus.channels as usize)
            .sum::<usize>();
        let mut pending_midi = Vec::new();
        let pending_midi_capacity_available = P::MAX_EVENTS_PER_BLOCK <= MAX_PROCESS_EVENTS
            && pending_midi
                .try_reserve_exact(P::MAX_EVENTS_PER_BLOCK)
                .is_ok();
        // `Plugin::new` is an ABI-mandated infallible constructor. A malformed
        // event bound must therefore degrade to a fixed zero-capacity queue;
        // processing will report an out-of-memory result rather than allowing
        // an allocation or panic on the audio thread.
        let (event_queue, event_queue_capacity_available) =
            if P::MAX_EVENTS_PER_BLOCK <= MAX_PROCESS_EVENTS {
                match EventQueue::try_with_capacity(P::MAX_EVENTS_PER_BLOCK) {
                    Ok(queue) => (queue, true),
                    Err(_) => (EventQueue::with_capacity(0), false),
                }
            } else {
                (EventQueue::with_capacity(0), false)
            };
        Self {
            plugin,
            params,
            sample_rate: 44100.0,
            input_channel_count: input_channels,
            output_channel_count: output_channels,
            // Audio scratch is activation-owned. Keeping construction empty
            // avoids an infallible 4096-frame allocation based solely on a
            // plugin-declared channel count.
            input_buffers: Vec::new(),
            output_buffers: Vec::new(),
            pending_midi,
            pending_midi_capacity_available,
            event_capacity_available: event_queue_capacity_available
                && pending_midi_capacity_available,
            event_overflowed: false,
            event_queue,
            view_handle: None,
            shared_params,
            host,
            param_descriptors,
            input_bus_bounds,
        }
    }

    fn activate(&mut self, sample_rate: f64, max_frames: u32) -> bool {
        if !self.event_capacity_available
            || !sample_rate.is_finite()
            || sample_rate <= 0.0
            || max_frames > MAX_PROCESS_FRAMES
            || P::MAX_EVENTS_PER_BLOCK > MAX_PROCESS_EVENTS
        {
            return false;
        }
        let Some(input_buffers) = allocate_audio_buffers(self.input_channel_count, max_frames)
        else {
            return false;
        };
        let Some(output_buffers) = allocate_audio_buffers(self.output_channel_count, max_frames)
        else {
            return false;
        };
        self.input_buffers = input_buffers;
        self.output_buffers = output_buffers;
        self.sample_rate = sample_rate;
        self.plugin.initialize(sample_rate, max_frames);
        true
    }

    fn deactivate(&mut self) {
        self.plugin.reset();
        self.pending_midi.clear();
        self.event_overflowed = false;
        self.event_queue.clear();
    }

    fn latency(&self) -> u32 {
        self.plugin.latency_samples()
    }

    fn tail(&self) -> u32 {
        clamp_vst3_tail(self.plugin.tail())
    }

    fn reset(&mut self) {
        // VST3 delivers an in-place reset through IAudioProcessor's
        // setProcessing(false) callback while the processor remains active.
        // Forward it to the actual SunMao instance and discard queued events.
        self.plugin.reset();
        self.pending_midi.clear();
        self.event_overflowed = false;
        self.event_queue.clear();
        for buffer in &mut self.input_buffers {
            buffer.fill(0.0);
        }
        for buffer in &mut self.output_buffers {
            buffer.fill(0.0);
        }
    }

    fn audio_config() -> AudioConfig {
        let plugin = P::default();
        let input_channels = plugin.input_channels();
        let output_channels = plugin.output_channels();
        let vst3_info = P::vst3_info();
        // VST3 has an explicit aux bus type, so a declared sidechain becomes a
        // `kAux` bus rather than an extra main bus. The plugin-declared layout
        // only applies to the main bus; aux buses infer their arrangement from
        // the channel count.
        let to_ports =
            |buses: Vec<sunmao_core::plugin::BusInfo>,
             main_layout: Option<sunmao_core::metadata::Vst3SpeakerLayout>| {
                buses
                    .into_iter()
                    .filter(|bus| bus.channels > 0)
                    .map(|bus| {
                        let is_main = bus.role == SunmaoBusRole::Main;
                        PortConfig {
                            name: bus.name,
                            channels: bus.channels,
                            port_type: if is_main {
                                PortType::Main
                            } else {
                                PortType::Aux
                            },
                            speaker_arrangement: if is_main {
                                main_layout.map(|layout| layout.mask())
                            } else {
                                None
                            },
                        }
                    })
                    .collect::<Vec<_>>()
            };
        AudioConfig {
            inputs: to_ports(plugin.input_buses(), vst3_info.input_layout),
            outputs: to_ports(plugin.output_buses(), vst3_info.output_layout),
            accepts_midi: plugin.accepts_midi(),
        }
    }

    fn params() -> Vec<ParamInfo> {
        let plugin = P::default();
        let params = plugin.params();
        params
            .validated_descriptors()
            .unwrap_or_else(|error| panic!("invalid SunMao parameter layout: {error}"))
            .into_iter()
            .map(|descriptor| {
                ParamInfo::new(descriptor.numeric_id, descriptor.name)
                    .range(0.0, 1.0)
                    .default(descriptor.default_normalized as f64)
                    .step_count(descriptor.step_count.min(i32::MAX as u32) as i32)
            })
            .collect()
    }

    fn get_param(&self, id: u32) -> f64 {
        if self
            .param_descriptors
            .iter()
            .any(|descriptor| descriptor.numeric_id == id)
        {
            self.shared_params.get(id)
        } else {
            0.0
        }
    }

    fn set_param(&mut self, id: u32, value: f64) {
        if let Some(descriptor) = self
            .param_descriptors
            .iter()
            .find(|descriptor| descriptor.numeric_id == id)
        {
            let v = value as f32;
            self.params.set_normalized(descriptor.id, v);
            self.shared_params.set(id, value);
        }
    }

    fn note_on(&mut self, sample_offset: u32, channel: i16, pitch: i16, velocity: f32) {
        let Some(midi) = note_event_to_midi(sample_offset, channel, pitch, velocity, true) else {
            return;
        };
        if !self.pending_midi_capacity_available
            || self.pending_midi.len() >= P::MAX_EVENTS_PER_BLOCK
        {
            self.event_overflowed = true;
            return;
        }
        let sequence = self.pending_midi.len() as u32;
        self.pending_midi.push(PendingMidi {
            message: midi,
            sequence,
        });
    }

    fn note_off(&mut self, sample_offset: u32, channel: i16, pitch: i16, velocity: f32) {
        let Some(midi) = note_event_to_midi(sample_offset, channel, pitch, velocity, false) else {
            return;
        };
        if !self.pending_midi_capacity_available
            || self.pending_midi.len() >= P::MAX_EVENTS_PER_BLOCK
        {
            self.event_overflowed = true;
            return;
        }
        let sequence = self.pending_midi.len() as u32;
        self.pending_midi.push(PendingMidi {
            message: midi,
            sequence,
        });
    }

    fn process(&mut self, ctx: &mut ProcessContext) -> ProcessResult {
        if self.event_overflowed {
            self.pending_midi.clear();
            self.event_overflowed = false;
            return Err(ProcessError::OutOfMemory);
        }
        let num_samples = ctx.num_samples;
        let num_in = ctx.num_inputs();
        let num_out = ctx.num_outputs();

        // Copy input data to our buffers. Clear every declared channel first
        // so a short/missing host channel cannot expose a prior block.
        for buffer in &mut self.input_buffers {
            let active_len = num_samples.min(buffer.len());
            buffer[..active_len].fill(0.0);
        }
        for ch in 0..num_in.min(self.input_buffers.len()) {
            let src = ctx.input(ch);
            copy_input_buffer(&mut self.input_buffers[ch], src, num_samples);
        }

        // Effects begin with passthrough; synths begin with silence. Use the
        // actual plugin instance, not P::default(), and reset every output
        // channel even when it has no matching input.
        let is_synth = self.plugin.input_channels() == 0;
        prepare_output_buffers(
            &self.input_buffers,
            &mut self.output_buffers,
            num_samples,
            !is_synth,
        );

        let mut audio_buffer =
            AudioBuffer::from_planar(&self.input_buffers, &mut self.output_buffers, num_samples)
                .with_input_bus_bounds(&self.input_bus_bounds);

        // Create process context
        let sunmao_ctx = SunmaoProcessContext {
            sample_rate: self.sample_rate,
            tempo: ctx.tempo(),
            is_playing: ctx.is_playing(),
            is_recording: ctx.is_recording(),
            is_loop_active: ctx.is_loop_active(),
            sample_pos: ctx.sample_pos(),
            time_signature: ctx.time_signature(),
            song_pos_beats: ctx.song_pos_beats(),
            song_pos_seconds: ctx.song_pos_seconds(),
            bar_start_beats: ctx.bar_start_beats(),
            // VST3 reports the bar's musical start, not an index.
            bar_number: None,
            loop_beats: ctx.loop_beats(),
        };

        // VST3 supplies one queue per parameter. vst3_rs has already flattened
        // those queues into a stable sample-ordered stream for this block.
        self.event_queue.clear();
        if !append_timed_events(
            ctx.param_changes(),
            &mut self.pending_midi,
            &self.param_descriptors,
            &mut self.event_queue,
        ) {
            return Err(ProcessError::OutOfMemory);
        }

        // Call the actual plugin process
        let status = self
            .plugin
            .process(&mut audio_buffer, &self.event_queue, &sunmao_ctx);
        if status == sunmao_core::ProcessStatus::Error {
            return Err(ProcessError::Internal);
        }

        // Copy output data back to vst3 context
        for ch in 0..num_out.min(self.output_buffers.len()) {
            let src = &self.output_buffers[ch];
            let dst = ctx.output_mut(ch);
            let len = num_samples.min(src.len()).min(dst.len());
            dst[..len].copy_from_slice(&src[..len]);
        }
        Ok(())
    }
}

impl<P: SunmaoPlugin> GuiPlugin for SunmaoVst3Wrapper<P> {
    fn gui_size() -> GuiSize {
        let plugin = P::default();
        if let Some(view) = plugin.view() {
            let (w, h) = view.size();
            GuiSize::new(w, h)
        } else {
            GuiSize::new(0, 0)
        }
    }

    fn gui_create(&mut self, parent: RawWindowHandle) -> bool {
        let mut parent = parent;
        if prepare_view(&mut parent).is_err() {
            return false;
        }
        if let Some(view) = self.plugin.view() {
            let Some(parent_window) = parent_window_from_raw(parent) else {
                return false;
            };

            let context = Arc::new(Vst3ParamsViewContext::new(
                self.params.clone(),
                self.shared_params.clone(),
                self.host.clone(),
            ));
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                view.open(parent_window, context)
            })) {
                Ok(Some(handle)) => {
                    self.view_handle = Some(handle);
                    true
                }
                Ok(None) => false,
                Err(_) => {
                    eprintln!("SunMao VST3 view creation panicked");
                    false
                }
            }
        } else {
            false
        }
    }

    fn gui_destroy(&mut self) {
        self.view_handle = None;
    }

    fn gui_can_resize(&self) -> bool {
        self.plugin
            .view()
            .map(|view| view.can_resize())
            .unwrap_or(false)
    }

    fn gui_resize(&mut self, size: GuiSize) {
        if let Some(handle) = self.view_handle.as_mut() {
            let _ = handle.resize(size.width, size.height);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    use raw_window_handle::{XcbWindowHandle, XlibWindowHandle};
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::ffi::c_void;
    #[cfg(target_os = "linux")]
    use std::num::NonZeroU32;
    use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicUsize, Ordering};
    use std::sync::{Mutex, OnceLock};
    use sunmao_core::{
        BoolParam, FloatParam, IntParam, ParamDescriptor, ParamKind, ProcessStatus, SunmaoView,
    };
    use vst3_rs::vst3_sys::base::{
        kInvalidArgument, kNoInterface, kNotImplemented, kResultOk, IUnknownVtbl, TUID,
    };
    #[cfg(target_os = "windows")]
    use vst3_rs::vst3_sys::gui::kPlatformTypeHWND;
    #[cfg(target_os = "macos")]
    use vst3_rs::vst3_sys::gui::kPlatformTypeNSView;
    #[cfg(target_os = "linux")]
    use vst3_rs::vst3_sys::gui::kPlatformTypeX11EmbedWindowID;
    use vst3_rs::vst3_sys::gui::{IPlugFrameVtbl, IPlugViewVtbl, ViewRect};
    use vst3_rs::vst3_sys::vst::iaudioprocessor::{
        AudioBusBuffers, IAudioProcessorVtbl, ProcessData, ProcessSetup,
    };
    use vst3_rs::vst3_sys::vst::icomponent::IComponentVtbl;
    use vst3_rs::vst3_sys::vst::ieditcontroller::{IComponentHandlerVtbl, IEditControllerVtbl};
    use vst3_rs::vst3_sys::vst::ievents::{
        Event as Vst3Event, EventData as Vst3EventData, EventTypes, IEventListVtbl, NoteOnEvent,
    };
    use vst3_rs::vst3_sys::vst::processcontext::{
        ProcessContext as SysProcessContext, ProcessContextFlags,
    };
    use vst3_rs::vst3_sys::vst::types::{ProcessModes, SymbolicSampleSizes};
    use vst3_rs::wrapper::{GuiControllerWrapper, ProcessorWrapper};

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

    #[repr(C)]
    struct DenseEventList {
        vtbl: *const IEventListVtbl,
        events: [Vst3Event; SynthPlugin::MAX_EVENTS_PER_BLOCK],
    }

    unsafe extern "system" fn event_query_interface(
        _this: *mut c_void,
        _iid: *const TUID,
        object: *mut *mut c_void,
    ) -> i32 {
        if !object.is_null() {
            unsafe { *object = std::ptr::null_mut() };
        }
        kNoInterface
    }

    unsafe extern "system" fn event_add_ref(_this: *mut c_void) -> u32 {
        1
    }

    unsafe extern "system" fn event_release(_this: *mut c_void) -> u32 {
        1
    }

    unsafe extern "system" fn event_count(_this: *mut c_void) -> i32 {
        SynthPlugin::MAX_EVENTS_PER_BLOCK as i32
    }

    unsafe extern "system" fn event_get(
        this: *mut c_void,
        index: i32,
        event: *mut Vst3Event,
    ) -> i32 {
        if this.is_null() || event.is_null() || index < 0 {
            return kInvalidArgument;
        }
        let events = unsafe { &(*(this as *const DenseEventList)).events };
        let Some(source) = events.get(index as usize) else {
            return kInvalidArgument;
        };
        unsafe { *event = *source };
        kResultOk
    }

    unsafe extern "system" fn event_add(_this: *mut c_void, _event: *mut Vst3Event) -> i32 {
        kNotImplemented
    }

    static DENSE_EVENT_LIST_VTBL: IEventListVtbl = IEventListVtbl {
        unknown: IUnknownVtbl {
            query_interface: event_query_interface,
            add_ref: event_add_ref,
            release: event_release,
        },
        get_event_count: event_count,
        get_event: event_get,
        add_event: event_add,
    };

    fn vst3_note_event(sample_offset: i32) -> Vst3Event {
        Vst3Event {
            bus_index: 0,
            sample_offset,
            ppq_position: 0.0,
            flags: 0,
            type_: EventTypes::kNoteOnEvent,
            event: Vst3EventData {
                note_on: NoteOnEvent {
                    channel: 0,
                    pitch: 60,
                    tuning: 0.0,
                    velocity: 0.75,
                    length: 0,
                    note_id: sample_offset,
                },
            },
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn accepts_both_vst3_xlib_and_xcb_parent_handles() {
        let xlib = RawWindowHandle::Xlib(XlibWindowHandle::new(42));
        assert!(matches!(
            parent_window_from_raw(xlib),
            Some(ParentWindow::X11(42))
        ));

        let xcb = RawWindowHandle::Xcb(XcbWindowHandle::new(NonZeroU32::new(43).unwrap()));
        assert!(matches!(
            parent_window_from_raw(xcb),
            Some(ParentWindow::X11(43))
        ));

        // Xlib uses `c_ulong`: 64-bit Unix can represent an ID that the
        // framework's u32 X11 handle cannot, while Windows' `c_ulong` is u32.
        if std::mem::size_of::<std::os::raw::c_ulong>() > std::mem::size_of::<u32>() {
            let window = (u32::MAX as u64 + 1) as std::os::raw::c_ulong;
            let oversized = RawWindowHandle::Xlib(XlibWindowHandle::new(window));
            assert!(parent_window_from_raw(oversized).is_none());
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
    fn vst3_note_conversion_rejects_invalid_channel_pitch_and_velocity() {
        assert!(note_event_to_midi(4, -1, 60, 0.5, true).is_none());
        assert!(note_event_to_midi(4, 16, 60, 0.5, true).is_none());
        assert!(note_event_to_midi(4, 0, -1, 0.5, true).is_none());
        assert!(note_event_to_midi(4, 0, 128, 0.5, true).is_none());
        assert!(note_event_to_midi(4, 0, 60, f32::NAN, true).is_none());
        assert!(note_event_to_midi(4, 0, 60, f32::INFINITY, false).is_none());
    }

    #[test]
    fn vst3_note_conversion_clamps_finite_velocity_without_wrapping_note_data() {
        let low = note_event_to_midi(4, 15, 127, -1.0, true).expect("valid note");
        assert_eq!(low.data, [0x9f, 127, 0]);

        let high = note_event_to_midi(4, 15, 127, 2.0, false).expect("valid note");
        assert_eq!(high.data, [0x8f, 127, 127]);
    }

    #[derive(Default)]
    struct EmptyParams;

    impl Params for EmptyParams {
        fn get_normalized(&self, _id: &str) -> Option<f32> {
            None
        }

        fn set_normalized(&self, _id: &str, _value: f32) {}

        fn descriptors(&self) -> Vec<sunmao_core::ParamDescriptor> {
            Vec::new()
        }
    }

    macro_rules! default_id_plugin {
        ($plugin:ident, $name:literal) => {
            #[derive(Default)]
            struct $plugin;

            impl SunmaoPlugin for $plugin {
                const NAME: &'static str = $name;
                const VENDOR: &'static str = "SunMao Test";
                const URL: &'static str = "https://example.invalid";
                type Params = EmptyParams;

                fn params(&self) -> Arc<Self::Params> {
                    Arc::new(EmptyParams)
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
        };
    }

    default_id_plugin!(DefaultIdPluginA, "Default A");
    default_id_plugin!(DefaultIdPluginB, "Default B");

    #[derive(Default)]
    struct HugeEventCapacityPlugin;

    impl SunmaoPlugin for HugeEventCapacityPlugin {
        const NAME: &'static str = "Huge Event Capacity";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        const MAX_EVENTS_PER_BLOCK: usize = usize::MAX;
        type Params = EmptyParams;

        fn input_channels(&self) -> u32 {
            0
        }

        fn output_channels(&self) -> u32 {
            0
        }

        fn params(&self) -> Arc<Self::Params> {
            Arc::new(EmptyParams)
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
    struct MonoPlugin;

    impl SunmaoPlugin for MonoPlugin {
        const NAME: &'static str = "Mono";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        type Params = EmptyParams;

        fn input_channels(&self) -> u32 {
            1
        }

        fn output_channels(&self) -> u32 {
            1
        }

        fn params(&self) -> Arc<Self::Params> {
            Arc::new(EmptyParams)
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
    struct SurroundPlugin;

    impl SunmaoPlugin for SurroundPlugin {
        const NAME: &'static str = "Surround";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        type Params = EmptyParams;

        fn input_channels(&self) -> u32 {
            6
        }

        fn output_channels(&self) -> u32 {
            8
        }

        fn params(&self) -> Arc<Self::Params> {
            Arc::new(EmptyParams)
        }

        fn process(
            &mut self,
            _buffer: &mut AudioBuffer,
            _events: &EventQueue,
            _context: &SunmaoProcessContext,
        ) -> ProcessStatus {
            ProcessStatus::Normal
        }

        fn vst3_info() -> sunmao_core::Vst3Info {
            sunmao_core::Vst3Info {
                input_layout: Some(sunmao_core::Vst3SpeakerLayout::SURROUND_5_1),
                output_layout: Some(sunmao_core::Vst3SpeakerLayout::MUSIC_7_1),
                ..Default::default()
            }
        }
    }

    #[derive(Default)]
    struct SynthPlugin;

    static SYNTH_EVENT_COUNT: AtomicUsize = AtomicUsize::new(0);

    impl SunmaoPlugin for SynthPlugin {
        const NAME: &'static str = "Synth";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        const MAX_EVENTS_PER_BLOCK: usize = 8;
        type Params = EmptyParams;

        fn input_channels(&self) -> u32 {
            0
        }

        fn accepts_midi(&self) -> bool {
            true
        }

        fn params(&self) -> Arc<Self::Params> {
            Arc::new(EmptyParams)
        }

        fn process(
            &mut self,
            _buffer: &mut AudioBuffer,
            events: &EventQueue,
            _context: &SunmaoProcessContext,
        ) -> ProcessStatus {
            SYNTH_EVENT_COUNT.store(events.iter().count(), Ordering::SeqCst);
            ProcessStatus::Normal
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
                },
                ParamDescriptor {
                    id: "voices",
                    numeric_id: sunmao_core::stable_param_id("voices"),
                    name: self.voices.name,
                    default_normalized: 0.5,
                    step_count: 4,
                    kind: ParamKind::Int,
                },
                ParamDescriptor {
                    id: "bypass",
                    numeric_id: sunmao_core::stable_param_id("bypass"),
                    name: self.bypass.name,
                    default_normalized: 0.0,
                    step_count: 1,
                    kind: ParamKind::Bool,
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

    fn notification_context_slot() -> &'static Mutex<Option<Arc<dyn ViewContext>>> {
        static SLOT: OnceLock<Mutex<Option<Arc<dyn ViewContext>>>> = OnceLock::new();
        SLOT.get_or_init(|| Mutex::new(None))
    }

    struct NotificationView;

    impl SunmaoView for NotificationView {
        fn size(&self) -> (u32, u32) {
            (640, 360)
        }

        fn open(&self, _parent: ParentWindow, context: Arc<dyn ViewContext>) -> Option<ViewHandle> {
            *notification_context_slot().lock().unwrap() = Some(context);
            Some(ViewHandle::new(()))
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
        type Params = EmptyParams;

        fn params(&self) -> Arc<Self::Params> {
            Arc::new(EmptyParams)
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

    #[derive(Default)]
    struct NotificationPlugin;

    impl SunmaoPlugin for NotificationPlugin {
        const NAME: &'static str = "GUI Notification";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        type Params = MetadataParams;

        fn params(&self) -> Arc<Self::Params> {
            Arc::new(MetadataParams::default())
        }

        fn view(&self) -> Option<Box<dyn SunmaoView>> {
            Some(Box::new(NotificationView))
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

    #[derive(Debug, PartialEq)]
    enum HostNotification {
        Begin(u32),
        Perform(u32, f64),
        End(u32),
    }

    #[repr(C)]
    struct NotificationComponentHandler {
        vtbl: *const IComponentHandlerVtbl,
        refs: AtomicU32,
        notifications: Mutex<Vec<HostNotification>>,
    }

    unsafe extern "system" fn notification_query_interface(
        _this: *mut c_void,
        _iid: *const TUID,
        object: *mut *mut c_void,
    ) -> i32 {
        if !object.is_null() {
            unsafe { *object = std::ptr::null_mut() };
        }
        kNoInterface
    }

    unsafe extern "system" fn notification_handler_add_ref(this: *mut c_void) -> u32 {
        unsafe {
            (*(this as *mut NotificationComponentHandler))
                .refs
                .fetch_add(1, Ordering::SeqCst)
                + 1
        }
    }

    unsafe extern "system" fn notification_handler_release(this: *mut c_void) -> u32 {
        unsafe {
            (*(this as *mut NotificationComponentHandler))
                .refs
                .fetch_sub(1, Ordering::SeqCst)
                - 1
        }
    }

    unsafe extern "system" fn notification_begin_edit(this: *mut c_void, id: u32) -> i32 {
        unsafe { &*(this as *const NotificationComponentHandler) }
            .notifications
            .lock()
            .unwrap()
            .push(HostNotification::Begin(id));
        kResultOk
    }

    unsafe extern "system" fn notification_perform_edit(
        this: *mut c_void,
        id: u32,
        value: f64,
    ) -> i32 {
        unsafe { &*(this as *const NotificationComponentHandler) }
            .notifications
            .lock()
            .unwrap()
            .push(HostNotification::Perform(id, value));
        kResultOk
    }

    unsafe extern "system" fn notification_end_edit(this: *mut c_void, id: u32) -> i32 {
        unsafe { &*(this as *const NotificationComponentHandler) }
            .notifications
            .lock()
            .unwrap()
            .push(HostNotification::End(id));
        kResultOk
    }

    unsafe extern "system" fn notification_restart_component(
        _this: *mut c_void,
        _flags: i32,
    ) -> i32 {
        kResultOk
    }

    static NOTIFICATION_COMPONENT_HANDLER_VTBL: IComponentHandlerVtbl = IComponentHandlerVtbl {
        unknown: IUnknownVtbl {
            query_interface: notification_query_interface,
            add_ref: notification_handler_add_ref,
            release: notification_handler_release,
        },
        begin_edit: notification_begin_edit,
        perform_edit: notification_perform_edit,
        end_edit: notification_end_edit,
        restart_component: notification_restart_component,
    };

    #[repr(C)]
    struct NotificationPlugFrame {
        vtbl: *const IPlugFrameVtbl,
        refs: AtomicU32,
        view: AtomicPtr<c_void>,
        width: AtomicU32,
        height: AtomicU32,
    }

    unsafe extern "system" fn notification_frame_add_ref(this: *mut c_void) -> u32 {
        unsafe {
            (*(this as *mut NotificationPlugFrame))
                .refs
                .fetch_add(1, Ordering::SeqCst)
                + 1
        }
    }

    unsafe extern "system" fn notification_frame_release(this: *mut c_void) -> u32 {
        unsafe {
            (*(this as *mut NotificationPlugFrame))
                .refs
                .fetch_sub(1, Ordering::SeqCst)
                - 1
        }
    }

    unsafe extern "system" fn notification_resize_view(
        this: *mut c_void,
        view: *mut c_void,
        size: *mut ViewRect,
    ) -> i32 {
        if size.is_null() {
            return kInvalidArgument;
        }
        let frame = unsafe { &*(this as *const NotificationPlugFrame) };
        let size = unsafe { &*size };
        frame.view.store(view, Ordering::SeqCst);
        frame.width.store(size.width() as u32, Ordering::SeqCst);
        frame.height.store(size.height() as u32, Ordering::SeqCst);
        kResultOk
    }

    static NOTIFICATION_PLUG_FRAME_VTBL: IPlugFrameVtbl = IPlugFrameVtbl {
        unknown: IUnknownVtbl {
            query_interface: notification_query_interface,
            add_ref: notification_frame_add_ref,
            release: notification_frame_release,
        },
        resize_view: notification_resize_view,
    };

    #[derive(Default)]
    struct ExplicitIdPlugin;

    impl SunmaoPlugin for ExplicitIdPlugin {
        const NAME: &'static str = "Explicit";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        type Params = EmptyParams;

        fn params(&self) -> Arc<Self::Params> {
            Arc::new(EmptyParams)
        }

        fn process(
            &mut self,
            _buffer: &mut AudioBuffer,
            _events: &EventQueue,
            _context: &SunmaoProcessContext,
        ) -> ProcessStatus {
            ProcessStatus::Normal
        }

        fn vst3_info() -> sunmao_core::Vst3Info {
            sunmao_core::Vst3Info {
                class_id: *b"ExplicitVst3ID!!",
                categories: &["Fx"],
                ..Default::default()
            }
        }
    }

    #[test]
    fn default_class_ids_are_plugin_specific() {
        let first = <SunmaoVst3Wrapper<DefaultIdPluginA> as Plugin>::class_id();
        let second = <SunmaoVst3Wrapper<DefaultIdPluginB> as Plugin>::class_id();
        assert_ne!(first, second);
        assert_ne!(first, [0; 16]);
    }

    #[test]
    fn unified_view_context_notifies_vst3_handler_and_plug_frame() {
        *notification_context_slot().lock().unwrap() = None;

        unsafe {
            let controller = GuiControllerWrapper::<SunmaoVst3Wrapper<NotificationPlugin>>::new();
            let controller_object = controller.cast::<c_void>();
            let controller_vtbl = *(controller_object as *const *const IEditControllerVtbl);
            let mut handler = NotificationComponentHandler {
                vtbl: &NOTIFICATION_COMPONENT_HANDLER_VTBL,
                refs: AtomicU32::new(1),
                notifications: Mutex::new(Vec::new()),
            };
            assert_eq!(
                ((*controller_vtbl).set_component_handler)(
                    controller_object,
                    (&mut handler as *mut NotificationComponentHandler).cast(),
                ),
                kResultOk
            );
            assert_eq!(handler.refs.load(Ordering::SeqCst), 2);

            let view =
                ((*controller_vtbl).create_view)(controller_object, b"editor\0".as_ptr().cast());
            assert!(!view.is_null());
            let view_vtbl = *(view as *const *const IPlugViewVtbl);
            let mut frame = NotificationPlugFrame {
                vtbl: &NOTIFICATION_PLUG_FRAME_VTBL,
                refs: AtomicU32::new(1),
                view: AtomicPtr::new(std::ptr::null_mut()),
                width: AtomicU32::new(0),
                height: AtomicU32::new(0),
            };
            assert_eq!(
                ((*view_vtbl).set_frame)(view, (&mut frame as *mut NotificationPlugFrame).cast(),),
                kResultOk
            );
            assert_eq!(frame.refs.load(Ordering::SeqCst), 2);

            #[cfg(target_os = "macos")]
            let (parent, platform) = {
                use objc::runtime::Object;
                use objc::{class, msg_send, sel, sel_impl};
                let _ = NSApplicationLoad();
                let parent: *mut Object = msg_send![class!(NSView), new];
                assert!(!parent.is_null());
                (parent.cast::<c_void>(), kPlatformTypeNSView.as_ptr().cast())
            };
            #[cfg(target_os = "windows")]
            let (parent, platform) = (1usize as *mut c_void, kPlatformTypeHWND.as_ptr().cast());
            #[cfg(target_os = "linux")]
            let (parent, platform) = (
                1usize as *mut c_void,
                kPlatformTypeX11EmbedWindowID.as_ptr().cast(),
            );

            assert_eq!(((*view_vtbl).attached)(view, parent, platform), kResultOk);
            let context = notification_context_slot()
                .lock()
                .unwrap()
                .as_ref()
                .expect("view context")
                .clone();
            let mix = sunmao_core::stable_param_id("mix");

            context.begin_edit("mix");
            context.set_param("mix", 0.75);
            context.end_edit("mix");
            context.begin_edit("unknown");
            context.set_param("unknown", 0.5);
            context.end_edit("unknown");
            assert_eq!(context.get_param("mix"), Some(0.75));
            assert_eq!(context.get_param("unknown"), None);
            assert_eq!(
                *handler.notifications.lock().unwrap(),
                vec![
                    HostNotification::Begin(mix),
                    HostNotification::Perform(mix, 0.75),
                    HostNotification::End(mix),
                ]
            );

            assert!(context.request_resize(1024, 576));
            assert_eq!(frame.view.load(Ordering::SeqCst), view);
            assert_eq!(frame.width.load(Ordering::SeqCst), 1024);
            assert_eq!(frame.height.load(Ordering::SeqCst), 576);

            drop(context);
            *notification_context_slot().lock().unwrap() = None;
            assert_eq!(((*view_vtbl).removed)(view), kResultOk);
            assert_eq!(((*view_vtbl).unknown.release)(view), 0);
            assert_eq!(frame.refs.load(Ordering::SeqCst), 1);
            assert_eq!(
                ((*controller_vtbl).base.unknown.release)(controller_object),
                0
            );
            assert_eq!(handler.refs.load(Ordering::SeqCst), 1);

            #[cfg(target_os = "macos")]
            {
                use objc::runtime::Object;
                use objc::{msg_send, sel, sel_impl};
                let _: () = msg_send![parent as *mut Object, release];
            }
        }
    }

    #[test]
    fn view_creation_panic_is_contained_before_returning_through_vst3_abi() {
        unsafe {
            let controller = GuiControllerWrapper::<SunmaoVst3Wrapper<PanickingViewPlugin>>::new();
            let controller_object = controller.cast::<c_void>();
            let controller_vtbl = *(controller_object as *const *const IEditControllerVtbl);
            let view =
                ((*controller_vtbl).create_view)(controller_object, b"editor\0".as_ptr().cast());
            assert!(!view.is_null());
            let view_vtbl = *(view as *const *const IPlugViewVtbl);

            #[cfg(target_os = "macos")]
            let (parent, platform) = {
                use objc::runtime::Object;
                use objc::{class, msg_send, sel, sel_impl};
                let _ = NSApplicationLoad();
                let parent: *mut Object = msg_send![class!(NSView), new];
                assert!(!parent.is_null());
                (parent.cast::<c_void>(), kPlatformTypeNSView.as_ptr().cast())
            };
            #[cfg(target_os = "windows")]
            let (parent, platform) = (1usize as *mut c_void, kPlatformTypeHWND.as_ptr().cast());
            #[cfg(target_os = "linux")]
            let (parent, platform) = (
                1usize as *mut c_void,
                kPlatformTypeX11EmbedWindowID.as_ptr().cast(),
            );

            assert_eq!(
                ((*view_vtbl).attached)(view, parent, platform),
                vst3_rs::vst3_sys::base::kResultFalse
            );
            assert_eq!(((*view_vtbl).unknown.release)(view), 0);
            assert_eq!(
                ((*controller_vtbl).base.unknown.release)(controller_object),
                0
            );

            #[cfg(target_os = "macos")]
            {
                use objc::runtime::Object;
                use objc::{msg_send, sel, sel_impl};
                let _: () = msg_send![parent as *mut Object, release];
            }
        }
    }

    #[test]
    fn explicit_class_id_is_preserved() {
        let actual = <SunmaoVst3Wrapper<ExplicitIdPlugin> as Plugin>::class_id();
        assert_eq!(actual, (*b"ExplicitVst3ID!!").map(|byte| byte as i8));
    }

    #[test]
    fn exposes_float_int_and_bool_parameter_metadata() {
        let params = <SunmaoVst3Wrapper<MetadataPlugin> as Plugin>::params();
        assert_eq!(params.len(), 3);

        assert_eq!(params[0].name, "Dry/Wet");
        assert_eq!(params[0].id, sunmao_core::stable_param_id("mix"));
        assert_eq!(params[0].default, 0.25);
        assert_eq!(params[0].step_count, 0);

        assert_eq!(params[1].name, "Voices");
        assert_eq!(params[1].id, sunmao_core::stable_param_id("voices"));
        assert_eq!(params[1].default, 0.5);
        assert_eq!(params[1].step_count, 4);

        assert_eq!(params[2].name, "Bypass");
        assert_eq!(params[2].id, sunmao_core::stable_param_id("bypass"));
        assert_eq!(params[2].default, 0.0);
        assert_eq!(params[2].step_count, 1);
    }

    #[test]
    fn audio_config_reflects_plugin_channels_and_midi_capability() {
        let mono = <SunmaoVst3Wrapper<MonoPlugin> as Plugin>::audio_config();
        assert_eq!(mono.inputs.len(), 1);
        assert_eq!(mono.inputs[0].channels, 1);
        assert_eq!(mono.outputs.len(), 1);
        assert_eq!(mono.outputs[0].channels, 1);
        assert!(!mono.accepts_midi);

        let synth = <SunmaoVst3Wrapper<SynthPlugin> as Plugin>::audio_config();
        assert!(synth.inputs.is_empty());
        assert_eq!(synth.outputs.len(), 1);
        assert_eq!(synth.outputs[0].channels, 2);
        assert!(synth.accepts_midi);

        let surround = <SunmaoVst3Wrapper<SurroundPlugin> as Plugin>::audio_config();
        assert_eq!(surround.inputs[0].channels, 6);
        assert_eq!(
            surround.inputs[0].speaker_arrangement,
            Some(vst3_rs::vst3_sys::vst::SpeakerArr::k51)
        );
        assert_eq!(surround.outputs[0].channels, 8);
        assert_eq!(
            surround.outputs[0].speaker_arrangement,
            Some(vst3_rs::vst3_sys::vst::SpeakerArr::k71Music)
        );
    }

    #[test]
    fn adapter_audio_allocation_rejects_oversized_resource_budgets() {
        assert!(allocate_audio_buffers(2, 8192).is_some());
        assert!(allocate_audio_buffers(2, MAX_PROCESS_FRAMES + 1).is_none());
        assert!(allocate_audio_buffers(MAX_PROCESS_CHANNELS + 1, 1).is_none());
        let channels_over_sample_budget =
            MAX_PROCESS_AUDIO_SAMPLES / MAX_PROCESS_FRAMES as usize + 1;
        assert!(allocate_audio_buffers(channels_over_sample_budget, MAX_PROCESS_FRAMES).is_none());
    }

    #[test]
    fn wrapper_construction_contains_unallocatable_event_capacity() {
        let processor =
            ProcessorWrapper::<SunmaoVst3Wrapper<HugeEventCapacityPlugin>>::new([0; 16]);
        assert!(!processor.is_null());
        unsafe {
            let component = processor.cast::<c_void>();
            let component_vtbl = *(component as *const *const IComponentVtbl);
            assert_eq!(((*component_vtbl).base.unknown.release)(component), 0);
        }
    }

    #[test]
    fn unified_vst3_audio_processing_does_not_use_the_allocator() {
        unsafe {
            SYNTH_EVENT_COUNT.store(0, Ordering::SeqCst);
            let processor = ProcessorWrapper::<SunmaoVst3Wrapper<SynthPlugin>>::new([0; 16]);
            let component = processor.cast::<c_void>();
            let component_vtbl = *(component as *const *const IComponentVtbl);
            assert_eq!(
                ((*component_vtbl).base.initialize)(component, std::ptr::null_mut()),
                kResultOk
            );

            let mut audio = std::ptr::null_mut();
            assert_eq!(
                ((*component_vtbl).base.unknown.query_interface)(
                    component,
                    &vst3_rs::vst3_sys::vst::iid::IAudioProcessor,
                    &mut audio,
                ),
                kResultOk
            );
            assert!(!audio.is_null());
            let audio_vtbl = *(audio as *const *const IAudioProcessorVtbl);
            let mut setup = ProcessSetup {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                max_samples_per_block: 8,
                sample_rate: 48_000.0,
            };
            assert_eq!(
                ((*audio_vtbl).setup_processing)(audio, &mut setup),
                kResultOk
            );
            assert_eq!(((*component_vtbl).set_active)(component, 1), kResultOk);
            assert_eq!(((*audio_vtbl).set_processing)(audio, 1), kResultOk);

            let mut left = [0.0_f32; 8];
            let mut right = [0.0_f32; 8];
            let mut channels = [
                left.as_mut_ptr().cast::<c_void>(),
                right.as_mut_ptr().cast::<c_void>(),
            ];
            let mut outputs = [AudioBusBuffers {
                num_channels: 2,
                silence_flags: 0,
                buffers: channels.as_mut_ptr(),
            }];
            let mut events = DenseEventList {
                vtbl: &DENSE_EVENT_LIST_VTBL,
                events: std::array::from_fn(|index| vst3_note_event(index as i32)),
            };
            let mut data = ProcessData {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                num_samples: 8,
                num_inputs: 0,
                num_outputs: 1,
                inputs: std::ptr::null_mut(),
                outputs: outputs.as_mut_ptr(),
                input_parameter_changes: std::ptr::null_mut(),
                output_parameter_changes: std::ptr::null_mut(),
                input_events: (&mut events as *mut DenseEventList).cast::<c_void>(),
                output_events: std::ptr::null_mut(),
                process_context: std::ptr::null_mut(),
            };

            let (status, allocator_calls) =
                count_allocator_calls(|| ((*audio_vtbl).process)(audio, &mut data));
            assert_eq!(status, kResultOk);
            assert_eq!(allocator_calls, 0);
            assert_eq!(
                SYNTH_EVENT_COUNT.load(Ordering::SeqCst),
                SynthPlugin::MAX_EVENTS_PER_BLOCK
            );
            assert_eq!(left, [0.0; 8]);
            assert_eq!(right, [0.0; 8]);

            assert_eq!(((*audio_vtbl).set_processing)(audio, 0), kResultOk);
            assert_eq!(((*component_vtbl).set_active)(component, 0), kResultOk);
            assert_eq!(((*component_vtbl).base.terminate)(component), kResultOk);
            assert_eq!(((*audio_vtbl).unknown.release)(audio), 1);
            assert_eq!(((*component_vtbl).base.unknown.release)(component), 0);
        }
    }

    #[test]
    fn unified_vst3_effect_processing_does_not_use_the_allocator() {
        unsafe {
            let processor = ProcessorWrapper::<SunmaoVst3Wrapper<MonoPlugin>>::new([0; 16]);
            let component = processor.cast::<c_void>();
            let component_vtbl = *(component as *const *const IComponentVtbl);
            assert_eq!(
                ((*component_vtbl).base.initialize)(component, std::ptr::null_mut()),
                kResultOk
            );

            let mut audio = std::ptr::null_mut();
            assert_eq!(
                ((*component_vtbl).base.unknown.query_interface)(
                    component,
                    &vst3_rs::vst3_sys::vst::iid::IAudioProcessor,
                    &mut audio,
                ),
                kResultOk
            );
            let audio_vtbl = *(audio as *const *const IAudioProcessorVtbl);
            let mut setup = ProcessSetup {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                max_samples_per_block: 8,
                sample_rate: 48_000.0,
            };
            assert_eq!(
                ((*audio_vtbl).setup_processing)(audio, &mut setup),
                kResultOk
            );
            assert_eq!(((*component_vtbl).set_active)(component, 1), kResultOk);
            assert_eq!(((*audio_vtbl).set_processing)(audio, 1), kResultOk);

            let input = [0.1_f32, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];
            let mut input_channels = [input.as_ptr() as *mut c_void];
            let mut inputs = [AudioBusBuffers {
                num_channels: 1,
                silence_flags: 0,
                buffers: input_channels.as_mut_ptr(),
            }];
            let mut output = [0.0_f32; 8];
            let mut output_channels = [output.as_mut_ptr().cast::<c_void>()];
            let mut outputs = [AudioBusBuffers {
                num_channels: 1,
                silence_flags: 0,
                buffers: output_channels.as_mut_ptr(),
            }];
            let mut data = ProcessData {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                num_samples: 8,
                num_inputs: 1,
                num_outputs: 1,
                inputs: inputs.as_mut_ptr(),
                outputs: outputs.as_mut_ptr(),
                input_parameter_changes: std::ptr::null_mut(),
                output_parameter_changes: std::ptr::null_mut(),
                input_events: std::ptr::null_mut(),
                output_events: std::ptr::null_mut(),
                process_context: std::ptr::null_mut(),
            };

            let (status, allocator_calls) =
                count_allocator_calls(|| ((*audio_vtbl).process)(audio, &mut data));
            assert_eq!(status, kResultOk);
            assert_eq!(allocator_calls, 0);
            assert_eq!(output, input);

            assert_eq!(((*audio_vtbl).set_processing)(audio, 0), kResultOk);
            assert_eq!(((*component_vtbl).set_active)(component, 0), kResultOk);
            assert_eq!(((*component_vtbl).base.terminate)(component), kResultOk);
            assert_eq!(((*audio_vtbl).unknown.release)(audio), 1);
            assert_eq!(((*component_vtbl).base.unknown.release)(component), 0);
        }
    }

    #[test]
    fn merges_vst3_parameter_changes_with_midi_as_core_events() {
        let descriptor = ParamDescriptor {
            id: "mix",
            numeric_id: 17,
            name: "Mix",
            default_normalized: 0.25,
            step_count: 0,
            kind: ParamKind::Float,
        };
        let parameter_changes = [
            Vst3ParamChange {
                sample_offset: 3,
                id: 17,
                value: 0.5,
            },
            Vst3ParamChange {
                sample_offset: 7,
                id: 999,
                value: 0.8,
            },
            Vst3ParamChange {
                sample_offset: 7,
                id: 17,
                value: 0.75,
            },
        ];
        let mut midi = vec![
            PendingMidi {
                message: MidiMessage::note_off(7, 0, 60, 0),
                sequence: 0,
            },
            PendingMidi {
                message: MidiMessage::note_on(1, 0, 60, 100),
                sequence: 1,
            },
        ];
        let mut events = EventQueue::with_capacity(4);

        let (merged, allocator_calls) = count_allocator_calls(|| {
            append_timed_events(&parameter_changes, &mut midi, &[descriptor], &mut events)
        });
        assert!(merged);
        assert_eq!(allocator_calls, 0);

        let actual: Vec<_> = events
            .timed_events()
            .map(|event| match event {
                Event::Midi(message) => ("midi", message.offset, None),
                Event::ParamChange { id, value, offset } => {
                    assert_eq!(id, "mix");
                    ("param", offset, Some(value))
                }
            })
            .collect();
        assert_eq!(
            actual,
            vec![
                ("midi", 1, None),
                ("param", 3, Some(0.5)),
                ("param", 7, Some(0.75)),
                ("midi", 7, None),
            ]
        );
        assert!(midi.is_empty());
    }

    #[test]
    fn merging_into_a_full_core_event_queue_reports_overflow_without_growth() {
        let descriptor = ParamDescriptor {
            id: "mix",
            numeric_id: 17,
            name: "Mix",
            default_normalized: 0.25,
            step_count: 0,
            kind: ParamKind::Float,
        };
        let parameter_changes = [Vst3ParamChange {
            sample_offset: 3,
            id: 17,
            value: 0.5,
        }];
        let mut midi = vec![PendingMidi {
            message: MidiMessage::note_on(1, 0, 60, 100),
            sequence: 0,
        }];
        let mut events = EventQueue::with_capacity(1);

        assert!(!append_timed_events(
            &parameter_changes,
            &mut midi,
            &[descriptor],
            &mut events
        ));
        assert_eq!(events.max_events(), 1);
        assert_eq!(events.iter().count(), 1);
        assert!(matches!(events.iter().next(), Some(Event::Midi(_))));
        assert!(midi.is_empty());
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
        type Params = EmptyParams;

        fn params(&self) -> Arc<Self::Params> {
            Arc::new(EmptyParams)
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

    /// Runs one block through the VST3 wrapper with the given host context and
    /// returns the context the plugin observed.
    fn observe_transport(context: Option<&mut SysProcessContext>) -> SunmaoProcessContext {
        let _serialized = TRANSPORT_OBSERVATION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *OBSERVED_TRANSPORT.lock().unwrap() = None;

        unsafe {
            let processor = ProcessorWrapper::<SunmaoVst3Wrapper<TransportPlugin>>::new([0; 16]);
            let component = processor.cast::<c_void>();
            let component_vtbl = *(component as *const *const IComponentVtbl);
            assert_eq!(
                ((*component_vtbl).base.initialize)(component, std::ptr::null_mut()),
                kResultOk
            );

            let mut audio = std::ptr::null_mut();
            assert_eq!(
                ((*component_vtbl).base.unknown.query_interface)(
                    component,
                    &vst3_rs::vst3_sys::vst::iid::IAudioProcessor,
                    &mut audio,
                ),
                kResultOk
            );
            let audio_vtbl = *(audio as *const *const IAudioProcessorVtbl);
            let mut setup = ProcessSetup {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                max_samples_per_block: 8,
                sample_rate: 48_000.0,
            };
            assert_eq!(
                ((*audio_vtbl).setup_processing)(audio, &mut setup),
                kResultOk
            );
            assert_eq!(((*component_vtbl).set_active)(component, 1), kResultOk);
            assert_eq!(((*audio_vtbl).set_processing)(audio, 1), kResultOk);

            let mut left = [0.0_f32; 8];
            let mut right = [0.0_f32; 8];
            let mut channels = [
                left.as_mut_ptr().cast::<c_void>(),
                right.as_mut_ptr().cast::<c_void>(),
            ];
            let mut outputs = [AudioBusBuffers {
                num_channels: 2,
                silence_flags: 0,
                buffers: channels.as_mut_ptr(),
            }];
            let mut data = ProcessData {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                num_samples: 8,
                num_inputs: 0,
                num_outputs: 1,
                inputs: std::ptr::null_mut(),
                outputs: outputs.as_mut_ptr(),
                input_parameter_changes: std::ptr::null_mut(),
                output_parameter_changes: std::ptr::null_mut(),
                input_events: std::ptr::null_mut(),
                output_events: std::ptr::null_mut(),
                process_context: context
                    .map(|value| (value as *mut SysProcessContext).cast::<c_void>())
                    .unwrap_or(std::ptr::null_mut()),
            };

            assert_eq!(((*audio_vtbl).process)(audio, &mut data), kResultOk);
            assert_eq!(((*audio_vtbl).set_processing)(audio, 0), kResultOk);
            assert_eq!(((*component_vtbl).set_active)(component, 0), kResultOk);
            ((*component_vtbl).base.terminate)(component);
            ((*component_vtbl).base.unknown.release)(component);
        }

        OBSERVED_TRANSPORT
            .lock()
            .unwrap()
            .take()
            .expect("plugin must observe a process context")
    }

    fn full_host_context(state: u32) -> SysProcessContext {
        SysProcessContext {
            state,
            sample_rate: 48_000.0,
            project_time_samples: 96_000,
            system_time: 0,
            continous_time_samples: 0,
            project_time_music: 8.5,
            bar_position_music: 8.0,
            cycle_start_music: 4.0,
            cycle_end_music: 12.0,
            tempo: 128.0,
            time_sig_numerator: 7,
            time_sig_denominator: 8,
            chord: Default::default(),
            smpte_offset_subframes: 0,
            frame_rate: Default::default(),
            samples_to_next_clock: 0,
        }
    }

    #[derive(Default)]
    struct TailPlugin;

    impl SunmaoPlugin for TailPlugin {
        const NAME: &'static str = "Tail Test";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        type Params = EmptyParams;

        fn params(&self) -> Arc<Self::Params> {
            Arc::new(EmptyParams)
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
    fn latency_and_infinite_tail_reach_the_vst3_contract() {
        unsafe {
            let processor = ProcessorWrapper::<SunmaoVst3Wrapper<TailPlugin>>::new([0; 16]);
            let component = processor.cast::<c_void>();
            let component_vtbl = *(component as *const *const IComponentVtbl);
            assert_eq!(
                ((*component_vtbl).base.initialize)(component, std::ptr::null_mut()),
                kResultOk
            );

            let mut audio = std::ptr::null_mut();
            assert_eq!(
                ((*component_vtbl).base.unknown.query_interface)(
                    component,
                    &vst3_rs::vst3_sys::vst::iid::IAudioProcessor,
                    &mut audio,
                ),
                kResultOk
            );
            let audio_vtbl = *(audio as *const *const IAudioProcessorVtbl);

            assert_eq!(((*audio_vtbl).get_latency_samples)(audio), 256);
            assert_eq!(
                ((*audio_vtbl).get_tail_samples)(audio),
                vst3_infinite_tail()
            );

            ((*component_vtbl).base.terminate)(component);
            ((*component_vtbl).base.unknown.release)(component);
        }
    }

    #[test]
    fn a_finite_tail_never_collides_with_the_infinite_encoding() {
        // `kInfiniteTail` is a magic value, so a finite tail reported at that
        // exact length must be clamped one sample below it.
        assert_eq!(
            clamp_vst3_tail(SunmaoTailLength::Samples(u32::MAX)),
            vst3_infinite_tail() - 1
        );
        assert_eq!(clamp_vst3_tail(SunmaoTailLength::Samples(48_000)), 48_000);
        assert_eq!(clamp_vst3_tail(SunmaoTailLength::None), vst3_no_tail());
    }

    #[test]
    fn vst3_transport_fields_reach_the_plugin() {
        let mut host_context = full_host_context(
            ProcessContextFlags::kPlaying
                | ProcessContextFlags::kRecording
                | ProcessContextFlags::kCycleActive
                | ProcessContextFlags::kTempoValid
                | ProcessContextFlags::kTimeSigValid
                | ProcessContextFlags::kProjectTimeMusicValid
                | ProcessContextFlags::kBarPositionValid
                | ProcessContextFlags::kCycleValid,
        );

        let context = observe_transport(Some(&mut host_context));

        assert_eq!(context.tempo, Some(128.0));
        assert!(context.is_playing);
        assert!(context.is_recording);
        assert!(context.is_loop_active);
        assert_eq!(context.time_signature, Some((7, 8)));
        assert_eq!(context.song_pos_beats, Some(8.5));
        assert_eq!(context.bar_start_beats, Some(8.0));
        assert_eq!(context.loop_beats, Some((4.0, 12.0)));
        assert_eq!(context.sample_pos, 96_000);
        assert_eq!(context.song_pos_seconds, Some(2.0));
        // VST3 has no bar index; the documented degradation is `None`.
        assert_eq!(context.bar_number, None);
    }

    #[test]
    fn a_host_without_a_process_context_leaves_every_musical_field_absent() {
        let context = observe_transport(None);

        assert_eq!(context.tempo, None);
        assert!(context.is_playing, "a context-less host keeps running");
        assert!(!context.is_recording);
        assert!(!context.is_loop_active);
        assert_eq!(context.time_signature, None);
        assert_eq!(context.song_pos_beats, None);
        assert_eq!(context.bar_number, None);
        assert_eq!(context.loop_beats, None);
        assert_eq!(context.sample_pos, 0);
    }
}
