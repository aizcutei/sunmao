//! Standalone runtime for executing SunMao plugins.
//!
//! This module provides a way to run SunMao plugins as standalone applications
//! without needing a DAW host.
//!
//! ## Audio Input Modes
//! - **Auto**: Use external input for effects and no input for instruments (default)
//! - **System**: Capture system audio output using `ruhear` (macOS only, requires `system-capture` feature)
//! - **External**: Use microphone/line-in via `cpal`
//! - **None**: No audio input (for synths)
//!
//! macOS applications enabling `system-capture` must set
//! `MACOSX_DEPLOYMENT_TARGET=13.0` (or an equivalent final linker setting).
//! The ScreenCaptureKit Swift bridge requires macOS 13; targeting an older
//! version selects a Swift Concurrency back-deployment library that Cargo does
//! not bundle into downstream application packages.

use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use ringbuf::{Consumer, Producer, RingBuffer};
use sunmao_core::{
    plugin::{ProcessContext, ProcessStatus},
    AudioBuffer, Event, EventQueue, MidiMessage, Params, ParamsViewContext, StandaloneViewOptions,
    StandaloneViewResult, SunmaoPlugin, SunmaoView, ViewContext,
};

/// Largest sample rate accepted by the standalone convenience runtime.
///
/// The standalone path allocates a one-second interleaved input ring. Keeping
/// this value bounded prevents a user-provided configuration from turning that
/// allocation into an accidental multi-gigabyte request.
pub const MAX_STANDALONE_SAMPLE_RATE: u32 = 1_000_000;
/// Largest processing block accepted by the standalone convenience runtime.
pub const MAX_STANDALONE_BUFFER_SIZE: u32 = 1 << 20;
/// Largest number of channels accepted by the standalone convenience runtime.
pub const MAX_STANDALONE_CHANNELS: u32 = 256;
/// Largest event queue a standalone processor will allocate for one block.
pub const MAX_STANDALONE_EVENTS_PER_BLOCK: usize = 1 << 16;
/// Frame count used by the deterministic, device-free standalone smoke gate.
pub const STANDALONE_SMOKE_FRAMES: usize = 128;

// Keep each activation-owned audio scratch area bounded even when a plugin
// declares a large (but technically representable) channel count and block.
const MAX_STANDALONE_AUDIO_SAMPLES: usize = 16 << 20;
// A one-second ring is useful for external input, but should not be allowed to
// consume an unbounded amount of memory for unusual device configurations.
const MAX_STANDALONE_RING_SAMPLES: usize = 16 << 20;

fn input_ring_capacity(sample_rate: u32, input_channels: usize) -> Result<usize> {
    if sample_rate == 0 || sample_rate > MAX_STANDALONE_SAMPLE_RATE {
        bail!("Input ring buffer sample rate is outside the standalone limits");
    }
    if input_channels > MAX_STANDALONE_CHANNELS as usize {
        bail!(
            "Input channel count exceeds standalone limit of {}",
            MAX_STANDALONE_CHANNELS
        );
    }
    if input_channels == 0 {
        return Ok(1);
    }

    let samples = (sample_rate as usize)
        .checked_mul(input_channels)
        .context("Input ring buffer size overflow")?;
    if samples > MAX_STANDALONE_RING_SAMPLES {
        bail!("Input ring buffer exceeds standalone memory limit");
    }
    Ok(samples)
}

/// Audio input mode for standalone runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Select external input for effects and no audio input for instruments.
    Auto,
    /// Capture system audio output (macOS only, uses ruhear)
    System,
    /// Use external audio input (microphone/line-in via cpal)
    External,
    /// No audio input (for synths)
    None,
}

impl Default for InputMode {
    fn default() -> Self {
        InputMode::Auto
    }
}

impl InputMode {
    fn resolve(self, plugin_input_channels: usize) -> Self {
        match self {
            Self::Auto if plugin_input_channels == 0 => Self::None,
            Self::Auto => Self::External,
            explicit => explicit,
        }
    }
}

/// Configuration for the standalone runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeConfig {
    /// Audio input mode. The default automatically feeds effects from the
    /// system's external input and keeps instruments audio-input-free.
    pub input_mode: InputMode,
    /// Sample rate (default: use device default)
    pub sample_rate: Option<u32>,
    /// Buffer size (default: use device default)
    pub buffer_size: Option<u32>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            input_mode: InputMode::Auto,
            sample_rate: None,
            buffer_size: None,
        }
    }
}

impl RuntimeConfig {
    /// Validate values that would otherwise make a stream callback panic or
    /// produce an invalid processing context.
    pub fn validate(&self) -> Result<()> {
        if let Some(sample_rate) = self.sample_rate {
            if sample_rate == 0 {
                bail!("Sample rate must be greater than zero");
            }
            if sample_rate > MAX_STANDALONE_SAMPLE_RATE {
                bail!(
                    "Sample rate exceeds standalone limit of {} Hz",
                    MAX_STANDALONE_SAMPLE_RATE
                );
            }
        }
        if let Some(buffer_size) = self.buffer_size {
            if buffer_size == 0 {
                bail!("Buffer size must be greater than zero");
            }
            if buffer_size > MAX_STANDALONE_BUFFER_SIZE {
                bail!(
                    "Buffer size exceeds standalone limit of {} frames",
                    MAX_STANDALONE_BUFFER_SIZE
                );
            }
        }
        Ok(())
    }
}

/// Summary returned by [`smoke_test_standalone`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StandaloneSmokeReport {
    pub input_channels: usize,
    pub output_channels: usize,
    pub parameter_count: usize,
    pub processed_frames: usize,
    pub peak_output: f32,
}

/// Reusable processing session shared by device-backed standalone apps and
/// deterministic offline/smoke-test callers.
///
/// Construction owns all audio and event scratch. Successful calls to
/// [`Self::process`] do not allocate, resize a buffer, or acquire a lock.
pub struct StandaloneProcessor<P: SunmaoPlugin> {
    plugin: P,
    params: std::sync::Arc<P::Params>,
    param_descriptors: Vec<sunmao_core::ParamDescriptor>,
    // A process panic can leave user DSP state inconsistent. Keep the stream
    // alive long enough for the host to stop it, but never call that instance
    // again after the first panic.
    poisoned: bool,
    accepts_midi: bool,
    input_buffers: Vec<Vec<f32>>,
    output_buffers: Vec<Vec<f32>>,
    events: EventQueue,
    sample_rate: f64,
    sample_pos: i64,
    max_frames: usize,
}

impl<P: SunmaoPlugin> StandaloneProcessor<P> {
    /// Initialize one plugin instance for standalone processing.
    pub fn new(mut plugin: P, sample_rate: f64, max_frames: usize) -> Result<Self> {
        if !sample_rate.is_finite()
            || sample_rate <= 0.0
            || sample_rate > MAX_STANDALONE_SAMPLE_RATE as f64
        {
            bail!("Sample rate is outside the standalone runtime limits");
        }
        if max_frames > MAX_STANDALONE_BUFFER_SIZE as usize {
            bail!("Processing block exceeds standalone runtime limit");
        }
        let max_frames = max_frames.max(1);
        let accepts_midi = plugin.accepts_midi();
        let input_channels = usize::try_from(plugin.input_channels())
            .map_err(|_| anyhow::anyhow!("plugin input channel count is not representable"))?;
        let output_channels = usize::try_from(plugin.output_channels())
            .map_err(|_| anyhow::anyhow!("plugin output channel count is not representable"))?;
        if input_channels > MAX_STANDALONE_CHANNELS as usize
            || output_channels > MAX_STANDALONE_CHANNELS as usize
        {
            bail!(
                "plugin channel count exceeds standalone limit of {}",
                MAX_STANDALONE_CHANNELS
            );
        }
        if input_channels
            .checked_mul(max_frames)
            .is_none_or(|samples| samples > MAX_STANDALONE_AUDIO_SAMPLES)
            || output_channels
                .checked_mul(max_frames)
                .is_none_or(|samples| samples > MAX_STANDALONE_AUDIO_SAMPLES)
        {
            bail!("plugin audio scratch exceeds standalone memory limit");
        }
        if P::MAX_EVENTS_PER_BLOCK > MAX_STANDALONE_EVENTS_PER_BLOCK {
            bail!(
                "plugin event capacity exceeds standalone limit of {}",
                MAX_STANDALONE_EVENTS_PER_BLOCK
            );
        }
        let params = plugin.params();
        let param_descriptors = params
            .validated_descriptors()
            .context("invalid standalone parameter layout")?;
        let input_buffers = allocate_audio_buffers(input_channels, max_frames)?;
        let output_buffers = allocate_audio_buffers(output_channels, max_frames)?;
        let events = EventQueue::try_with_capacity(P::MAX_EVENTS_PER_BLOCK)
            .map_err(|_| anyhow::anyhow!("plugin event capacity cannot be allocated"))?;
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            plugin.initialize(sample_rate, max_frames as u32);
        }))
        .is_err()
        {
            bail!("plugin initialization panicked");
        }
        Ok(Self {
            plugin,
            params,
            param_descriptors,
            poisoned: false,
            accepts_midi,
            input_buffers,
            output_buffers,
            events,
            sample_rate,
            sample_pos: 0,
            max_frames,
        })
    }

    /// Construct a session from the plugin's [`Default`] implementation.
    pub fn from_default(sample_rate: f64, max_frames: usize) -> Result<Self> {
        let plugin = std::panic::catch_unwind(P::default)
            .map_err(|_| anyhow::anyhow!("plugin default construction panicked"))?;
        Self::new(plugin, sample_rate, max_frames)
    }

    /// Shared parameter storage used by the plugin and a standalone editor.
    pub fn params(&self) -> std::sync::Arc<P::Params> {
        self.params.clone()
    }

    /// Number of planar input channels required by [`Self::process`].
    pub fn input_channels(&self) -> usize {
        self.input_buffers.len()
    }

    /// Number of planar output channels required by [`Self::process`].
    pub fn output_channels(&self) -> usize {
        self.output_buffers.len()
    }

    /// Maximum frame count accepted by one call to [`Self::process`].
    pub fn max_frames(&self) -> usize {
        self.max_frames
    }

    /// Sample rate supplied during initialization.
    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// Absolute sample position of the next block.
    pub fn sample_position(&self) -> i64 {
        self.sample_pos
    }

    /// Whether a plugin panic has permanently disabled this session.
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    /// Process one planar block without opening an audio device.
    ///
    /// Channel counts must exactly match the plugin declaration and every
    /// channel must contain at least `frames` samples. Event offsets are
    /// clamped to the active block, matching the format adapters' treatment of
    /// malformed host offsets. Output is silenced on a plugin or event-capacity
    /// error.
    pub fn process(
        &mut self,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        events: &[Event],
        frames: usize,
    ) -> Result<ProcessStatus> {
        for output in outputs.iter_mut() {
            output.fill(0.0);
        }
        if frames > self.max_frames {
            bail!(
                "processing block has {frames} frames but the session limit is {}",
                self.max_frames
            );
        }
        if inputs.len() != self.input_buffers.len() {
            bail!(
                "standalone input channel mismatch: expected {}, got {}",
                self.input_buffers.len(),
                inputs.len()
            );
        }
        if outputs.len() != self.output_buffers.len() {
            bail!(
                "standalone output channel mismatch: expected {}, got {}",
                self.output_buffers.len(),
                outputs.len()
            );
        }
        if inputs.iter().any(|input| input.len() < frames)
            || outputs.iter().any(|output| output.len() < frames)
        {
            bail!("standalone channel is shorter than the requested frame count");
        }
        if self.poisoned {
            return Ok(ProcessStatus::Error);
        }

        for (source, destination) in inputs.iter().zip(&mut self.input_buffers) {
            destination[..frames].copy_from_slice(&source[..frames]);
        }
        self.prepare_output_scratch(frames);

        self.events.clear();
        let mut event_overflow = false;
        for &event in events {
            if matches!(event, Event::Midi(_)) && !self.accepts_midi {
                continue;
            }
            if matches!(
                event,
                Event::ParamChange { value, .. } if !value.is_finite()
            ) {
                bail!("standalone parameter events must contain finite normalized values");
            }
            let event = match event {
                Event::ParamChange { id, value, offset } => {
                    let Some(descriptor) = self
                        .param_descriptors
                        .iter()
                        .find(|descriptor| descriptor.id == id)
                    else {
                        continue;
                    };
                    Event::ParamChange {
                        id: descriptor.id,
                        value,
                        offset,
                    }
                }
                event => event,
            };
            let event = clamp_event_to_block(event, frames);
            if !self.events.push(event) {
                event_overflow = true;
            }
        }
        if event_overflow {
            return Ok(ProcessStatus::Error);
        }

        let status = self.process_prepared_block(frames);
        if status == ProcessStatus::Error {
            return Ok(status);
        }
        for change in self.events.param_changes() {
            self.params.set_normalized(change.id, change.value);
        }
        for (source, destination) in self.output_buffers.iter().zip(outputs.iter_mut()) {
            destination[..frames].copy_from_slice(&source[..frames]);
        }
        Ok(status)
    }

    fn prepare_output_scratch(&mut self, frames: usize) {
        for output in &mut self.output_buffers {
            output[..frames].fill(0.0);
        }
        for channel in 0..self.input_buffers.len().min(self.output_buffers.len()) {
            self.output_buffers[channel][..frames]
                .copy_from_slice(&self.input_buffers[channel][..frames]);
        }
    }

    fn process_prepared_block(&mut self, frames: usize) -> ProcessStatus {
        let mut audio =
            AudioBuffer::from_planar(&self.input_buffers, &mut self.output_buffers, frames);
        // The standalone runtime has no host timeline; it advertises a fixed
        // tempo and its own sample cursor, and leaves the musical fields
        // absent rather than inventing a bar/loop structure.
        let context = ProcessContext {
            sample_rate: self.sample_rate,
            tempo: Some(120.0),
            is_playing: true,
            sample_pos: self.sample_pos,
            ..Default::default()
        };
        let status = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.plugin.process(&mut audio, &self.events, &context)
        })) {
            Ok(status) => status,
            Err(_) => {
                self.poisoned = true;
                ProcessStatus::Error
            }
        };
        if status == ProcessStatus::Error {
            for output in &mut self.output_buffers {
                output[..frames].fill(0.0);
            }
        } else {
            self.sample_pos = self.sample_pos.saturating_add(frames as i64);
        }
        status
    }

    fn process_device_interleaved<T, I, M>(
        &mut self,
        output: &mut [T],
        device_output_channels: usize,
        device_input_channels: usize,
        mut next_input: I,
        mut next_midi: M,
    ) -> ProcessStatus
    where
        T: Sample + Copy + FromSample<f32>,
        I: FnMut() -> Option<f32>,
        M: FnMut() -> Option<MidiMessage>,
    {
        let silence = T::from_sample(0.0);
        output.fill(silence);
        if self.poisoned || device_output_channels == 0 {
            return ProcessStatus::Error;
        }

        let complete_samples = output.len() / device_output_channels * device_output_channels;
        let Some(chunk_samples) = self.max_frames.checked_mul(device_output_channels) else {
            return ProcessStatus::Error;
        };
        if chunk_samples == 0 {
            return ProcessStatus::Error;
        }
        let mut final_status = ProcessStatus::Normal;

        for device_chunk in output[..complete_samples].chunks_mut(chunk_samples) {
            let frames = device_chunk.len() / device_output_channels;
            for input in &mut self.input_buffers {
                input[..frames].fill(0.0);
            }
            for frame in 0..frames {
                for channel in 0..device_input_channels {
                    let sample = next_input().unwrap_or(0.0);
                    if let Some(input) = self.input_buffers.get_mut(channel) {
                        input[frame] = sample;
                    }
                }
            }

            self.prepare_output_scratch(frames);

            self.events.clear();
            let mut event_overflow = false;
            while let Some(message) = next_midi() {
                if !self.accepts_midi {
                    continue;
                }
                if !self
                    .events
                    .push(clamp_event_to_block(Event::Midi(message), frames))
                {
                    event_overflow = true;
                }
            }

            let status = if event_overflow {
                ProcessStatus::Error
            } else {
                self.process_prepared_block(frames)
            };
            if status == ProcessStatus::Error {
                output.fill(silence);
                return status;
            }
            final_status = status;

            for frame in 0..frames {
                for channel in 0..device_output_channels {
                    let source_channel = if self.output_buffers.len() == 1 {
                        0
                    } else {
                        channel
                    };
                    let sample = self
                        .output_buffers
                        .get(source_channel)
                        .map_or(0.0, |buffer| buffer[frame]);
                    device_chunk[frame * device_output_channels + channel] = T::from_sample(sample);
                }
            }
        }

        final_status
    }
}

/// Exercise a default plugin through the same processing session used by the
/// device callback, without requiring an audio or MIDI device.
pub fn smoke_test_standalone<P: SunmaoPlugin>() -> Result<StandaloneSmokeReport> {
    let mut processor = StandaloneProcessor::<P>::from_default(48_000.0, STANDALONE_SMOKE_FRAMES)?;
    let params = processor.params();
    let descriptors = params.descriptors();
    for descriptor in &descriptors {
        let value = params
            .get_normalized(descriptor.id)
            .context("standalone smoke parameter descriptor has no value")?;
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            bail!(
                "standalone smoke parameter '{}' returned an invalid normalized value",
                descriptor.id
            );
        }
    }

    let inputs: Vec<Vec<f32>> = (0..processor.input_channels())
        .map(|channel| {
            (0..STANDALONE_SMOKE_FRAMES)
                .map(|frame| ((frame + channel + 1) as f32 / STANDALONE_SMOKE_FRAMES as f32) * 0.25)
                .collect()
        })
        .collect();
    let input_refs: Vec<&[f32]> = inputs.iter().map(Vec::as_slice).collect();
    let mut outputs = vec![vec![0.0_f32; STANDALONE_SMOKE_FRAMES]; processor.output_channels()];
    let mut output_refs: Vec<&mut [f32]> = outputs.iter_mut().map(Vec::as_mut_slice).collect();
    let midi = processor
        .accepts_midi
        .then_some(Event::Midi(MidiMessage::note_on(7, 0, 69, 100)));
    let events = midi.as_slice();

    let status = processor.process(
        &input_refs,
        &mut output_refs,
        events,
        STANDALONE_SMOKE_FRAMES,
    )?;
    if status == ProcessStatus::Error {
        bail!("standalone smoke processing returned an error");
    }
    drop(output_refs);

    let peak_output = outputs
        .iter()
        .flatten()
        .try_fold(0.0_f32, |peak, &sample| {
            sample.is_finite().then_some(peak.max(sample.abs()))
        })
        .context("standalone smoke produced a non-finite sample")?;
    if processor.output_channels() > 0
        && (processor.input_channels() > 0 || processor.accepts_midi)
        && peak_output <= f32::EPSILON
    {
        bail!("standalone smoke produced only silence for an active fixture");
    }
    if processor.sample_position() != STANDALONE_SMOKE_FRAMES as i64 {
        bail!("standalone smoke transport position did not advance by one block");
    }

    Ok(StandaloneSmokeReport {
        input_channels: processor.input_channels(),
        output_channels: processor.output_channels(),
        parameter_count: descriptors.len(),
        processed_frames: STANDALONE_SMOKE_FRAMES,
        peak_output,
    })
}

/// Open a plugin's editor as a device-free top-level window and close it
/// automatically after several rendered frames.
pub fn smoke_test_standalone_gui<P: SunmaoPlugin>() -> Result<()> {
    let plugin = std::panic::catch_unwind(P::default)
        .map_err(|_| anyhow::anyhow!("plugin default construction panicked"))?;
    let (view, context) = standalone_view(&plugin)?
        .context("plugin does not provide a custom view for standalone GUI smoke")?;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        view.open_standalone(context, StandaloneViewOptions::smoke())
    }))
    .map_err(|_| anyhow::anyhow!("standalone view panicked"))?;
    match result {
        StandaloneViewResult::Closed => Ok(()),
        StandaloneViewResult::Unsupported => {
            bail!("plugin view does not support a top-level standalone window")
        }
        StandaloneViewResult::Failed => bail!("standalone view failed to initialize or render"),
    }
}

type StandaloneView = (Box<dyn SunmaoView>, std::sync::Arc<dyn ViewContext>);

fn standalone_view<P: SunmaoPlugin>(plugin: &P) -> Result<Option<StandaloneView>> {
    let view = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| plugin.view()))
        .map_err(|_| anyhow::anyhow!("plugin view construction panicked"))?;
    let Some(view) = view else {
        return Ok(None);
    };
    let params = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| plugin.params()))
        .map_err(|_| anyhow::anyhow!("plugin parameter access panicked"))?;
    let context: std::sync::Arc<dyn ViewContext> =
        std::sync::Arc::new(ParamsViewContext::new(params));
    Ok(Some((view, context)))
}

/// Standard command-line entry point for generated standalone executables.
///
/// With no arguments this opens the default audio/MIDI devices and, when the
/// plugin provides one, its top-level editor. `--smoke` validates DSP/MIDI;
/// `--gui-smoke` validates the top-level GUI without opening an audio device.
pub fn run_standalone_entry<P: SunmaoPlugin>() -> Result<()> {
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next()) {
        (None, None) => run_standalone::<P>(),
        (Some("--smoke"), None) => {
            let report = smoke_test_standalone::<P>()?;
            println!(
                "Standalone smoke passed: {} in={} out={} params={} frames={} peak={:.6}",
                P::NAME,
                report.input_channels,
                report.output_channels,
                report.parameter_count,
                report.processed_frames,
                report.peak_output
            );
            Ok(())
        }
        (Some("--gui-smoke"), None) => {
            smoke_test_standalone_gui::<P>()?;
            println!("Standalone GUI smoke passed: {}", P::NAME);
            Ok(())
        }
        (Some("--help" | "-h"), None) => {
            println!("Usage: <standalone> [--smoke|--gui-smoke]");
            println!("  --smoke  Run device-free DSP/MIDI validation and exit");
            println!("  --gui-smoke  Open, render, and close the top-level editor");
            Ok(())
        }
        _ => bail!("unknown standalone arguments; use --help for usage"),
    }
}

fn clamp_event_to_block(event: Event, frames: usize) -> Event {
    let max_offset = frames.saturating_sub(1).min(u32::MAX as usize) as u32;
    match event {
        Event::Midi(mut message) => {
            message.offset = message.offset.min(max_offset);
            Event::Midi(message)
        }
        Event::ParamChange { id, value, offset } => Event::ParamChange {
            id,
            value: if value.is_finite() {
                value.clamp(0.0, 1.0)
            } else {
                0.0
            },
            offset: offset.min(max_offset),
        },
    }
}

impl<P: SunmaoPlugin> Drop for StandaloneProcessor<P> {
    fn drop(&mut self) {
        // Destructors run from the cpal callback-owning thread. A plugin reset
        // is user code and must not unwind through that callback or abort the
        // process during normal stream teardown.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.plugin.reset();
        }));
    }
}

fn allocate_audio_buffers(channel_count: usize, max_frames: usize) -> Result<Vec<Vec<f32>>> {
    let mut buffers = Vec::new();
    buffers
        .try_reserve_exact(channel_count)
        .map_err(|_| anyhow::anyhow!("plugin channel scratch cannot be allocated"))?;
    for _ in 0..channel_count {
        let mut buffer = Vec::new();
        buffer
            .try_reserve_exact(max_frames)
            .map_err(|_| anyhow::anyhow!("plugin audio scratch cannot be allocated"))?;
        buffer.resize(max_frames, 0.0);
        buffers.push(buffer);
    }
    Ok(buffers)
}

fn build_output_stream<P: SunmaoPlugin>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: SampleFormat,
    output_channels: usize,
    input_channels: usize,
    processor: StandaloneProcessor<P>,
    input: Consumer<f32>,
    midi: Consumer<MidiMessage>,
) -> Result<cpal::Stream> {
    match sample_format {
        SampleFormat::I8 => build_output_stream_typed::<i8, P>(
            device,
            config,
            output_channels,
            input_channels,
            processor,
            input,
            midi,
        ),
        SampleFormat::I16 => build_output_stream_typed::<i16, P>(
            device,
            config,
            output_channels,
            input_channels,
            processor,
            input,
            midi,
        ),
        SampleFormat::I32 => build_output_stream_typed::<i32, P>(
            device,
            config,
            output_channels,
            input_channels,
            processor,
            input,
            midi,
        ),
        SampleFormat::I64 => build_output_stream_typed::<i64, P>(
            device,
            config,
            output_channels,
            input_channels,
            processor,
            input,
            midi,
        ),
        SampleFormat::U8 => build_output_stream_typed::<u8, P>(
            device,
            config,
            output_channels,
            input_channels,
            processor,
            input,
            midi,
        ),
        SampleFormat::U16 => build_output_stream_typed::<u16, P>(
            device,
            config,
            output_channels,
            input_channels,
            processor,
            input,
            midi,
        ),
        SampleFormat::U32 => build_output_stream_typed::<u32, P>(
            device,
            config,
            output_channels,
            input_channels,
            processor,
            input,
            midi,
        ),
        SampleFormat::U64 => build_output_stream_typed::<u64, P>(
            device,
            config,
            output_channels,
            input_channels,
            processor,
            input,
            midi,
        ),
        SampleFormat::F32 => build_output_stream_typed::<f32, P>(
            device,
            config,
            output_channels,
            input_channels,
            processor,
            input,
            midi,
        ),
        SampleFormat::F64 => build_output_stream_typed::<f64, P>(
            device,
            config,
            output_channels,
            input_channels,
            processor,
            input,
            midi,
        ),
        _ => bail!("Unsupported output sample format: {sample_format}"),
    }
}

fn build_output_stream_typed<T, P>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    output_channels: usize,
    input_channels: usize,
    mut processor: StandaloneProcessor<P>,
    mut input: Consumer<f32>,
    mut midi: Consumer<MidiMessage>,
) -> Result<cpal::Stream>
where
    T: SizedSample + FromSample<f32>,
    P: SunmaoPlugin,
{
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                processor.process_device_interleaved(
                    data,
                    output_channels,
                    input_channels,
                    || input.pop(),
                    || midi.pop(),
                );
            },
            |error| eprintln!("Audio output stream error: {error}"),
            None,
        )
        .context("Failed to build output stream")
}

fn build_input_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: SampleFormat,
    channels: usize,
    output_sample_rate: u32,
    producer: Producer<f32>,
) -> Result<cpal::Stream> {
    let input_sample_rate = config.sample_rate.0;
    match sample_format {
        SampleFormat::I8 => build_input_stream_typed::<i8>(
            device,
            config,
            channels,
            input_sample_rate,
            output_sample_rate,
            producer,
        ),
        SampleFormat::I16 => build_input_stream_typed::<i16>(
            device,
            config,
            channels,
            input_sample_rate,
            output_sample_rate,
            producer,
        ),
        SampleFormat::I32 => build_input_stream_typed::<i32>(
            device,
            config,
            channels,
            input_sample_rate,
            output_sample_rate,
            producer,
        ),
        SampleFormat::I64 => build_input_stream_typed::<i64>(
            device,
            config,
            channels,
            input_sample_rate,
            output_sample_rate,
            producer,
        ),
        SampleFormat::U8 => build_input_stream_typed::<u8>(
            device,
            config,
            channels,
            input_sample_rate,
            output_sample_rate,
            producer,
        ),
        SampleFormat::U16 => build_input_stream_typed::<u16>(
            device,
            config,
            channels,
            input_sample_rate,
            output_sample_rate,
            producer,
        ),
        SampleFormat::U32 => build_input_stream_typed::<u32>(
            device,
            config,
            channels,
            input_sample_rate,
            output_sample_rate,
            producer,
        ),
        SampleFormat::U64 => build_input_stream_typed::<u64>(
            device,
            config,
            channels,
            input_sample_rate,
            output_sample_rate,
            producer,
        ),
        SampleFormat::F32 => build_input_stream_typed::<f32>(
            device,
            config,
            channels,
            input_sample_rate,
            output_sample_rate,
            producer,
        ),
        SampleFormat::F64 => build_input_stream_typed::<f64>(
            device,
            config,
            channels,
            input_sample_rate,
            output_sample_rate,
            producer,
        ),
        _ => bail!("Unsupported input sample format: {sample_format}"),
    }
}

struct InputResampler {
    channels: usize,
    input_frames_per_output_frame: f64,
    next_output_position: f64,
    previous_frame: Vec<f32>,
    output_frame: Vec<f32>,
    initialized: bool,
    valid: bool,
}

impl InputResampler {
    fn new(channels: usize, input_sample_rate: u32, output_sample_rate: u32) -> Self {
        let valid = channels > 0
            && channels <= MAX_STANDALONE_CHANNELS as usize
            && input_sample_rate > 0
            && input_sample_rate <= MAX_STANDALONE_SAMPLE_RATE
            && output_sample_rate > 0
            && output_sample_rate <= MAX_STANDALONE_SAMPLE_RATE;
        // Keep construction infallible for callers that build the resampler
        // before device validation, while making invalid input a no-op rather
        // than allowing a divide-by-zero in `process`.
        let safe_channels = channels.clamp(1, MAX_STANDALONE_CHANNELS as usize);
        let safe_input_rate = input_sample_rate.max(1);
        let safe_output_rate = output_sample_rate.max(1);
        Self {
            channels: safe_channels,
            input_frames_per_output_frame: safe_input_rate as f64 / safe_output_rate as f64,
            next_output_position: 0.0,
            previous_frame: vec![0.0; safe_channels],
            output_frame: vec![0.0; safe_channels],
            initialized: false,
            valid,
        }
    }

    fn maximum_output_samples(&self, input_frames: usize) -> Option<usize> {
        if !self.valid
            || !self.input_frames_per_output_frame.is_finite()
            || self.input_frames_per_output_frame <= 0.0
        {
            return None;
        }
        let ratio = 1.0 / self.input_frames_per_output_frame;
        let output_frames = ((input_frames as f64 * ratio).ceil() as usize).checked_add(1)?;
        output_frames.checked_mul(self.channels)
    }

    fn process<T>(&mut self, data: &[T], producer: &mut Producer<f32>)
    where
        T: Sample + Copy,
        f32: FromSample<T>,
    {
        if !self.valid {
            return;
        }
        let input_frames = data.len() / self.channels;
        if input_frames == 0 {
            return;
        }
        let Some(maximum_output_samples) = self.maximum_output_samples(input_frames) else {
            self.initialized = false;
            return;
        };
        if producer.remaining() < maximum_output_samples {
            self.initialized = false;
            return;
        }

        let mut frames = data[..input_frames * self.channels].chunks_exact(self.channels);
        if !self.initialized {
            let first = frames.next().expect("input frame count was checked above");
            for (previous, &sample) in self.previous_frame.iter_mut().zip(first) {
                *previous = f32::from_sample(sample);
            }
            let pushed = producer.push_slice(&self.previous_frame);
            debug_assert_eq!(pushed, self.channels);
            self.next_output_position = self.input_frames_per_output_frame;
            self.initialized = true;
        }

        for frame in frames {
            while self.next_output_position <= 1.0 + f64::EPSILON {
                let fraction = self.next_output_position.min(1.0) as f32;
                for (channel, &sample) in frame.iter().enumerate() {
                    let current = f32::from_sample(sample);
                    self.output_frame[channel] = self.previous_frame[channel]
                        + (current - self.previous_frame[channel]) * fraction;
                }
                // Publish complete interleaved frames so the output callback cannot
                // observe one channel before the rest of the frame is available.
                let pushed = producer.push_slice(&self.output_frame);
                debug_assert_eq!(pushed, self.channels);
                self.next_output_position += self.input_frames_per_output_frame;
            }
            self.next_output_position -= 1.0;
            for (previous, &sample) in self.previous_frame.iter_mut().zip(frame) {
                *previous = f32::from_sample(sample);
            }
        }
    }
}

fn build_input_stream_typed<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    input_sample_rate: u32,
    output_sample_rate: u32,
    mut producer: Producer<f32>,
) -> Result<cpal::Stream>
where
    T: SizedSample + Copy,
    f32: FromSample<T>,
{
    if channels == 0
        || channels > MAX_STANDALONE_CHANNELS as usize
        || input_sample_rate == 0
        || input_sample_rate > MAX_STANDALONE_SAMPLE_RATE
        || output_sample_rate == 0
        || output_sample_rate > MAX_STANDALONE_SAMPLE_RATE
    {
        bail!("Audio input reported an invalid channel count or sample rate");
    }
    if input_sample_rate == output_sample_rate {
        return device
            .build_input_stream(
                config,
                move |data: &[T], _| {
                    let complete_samples = data.len() / channels * channels;
                    if producer.remaining() < complete_samples {
                        return;
                    }
                    let mut samples = data[..complete_samples]
                        .iter()
                        .copied()
                        .map(f32::from_sample);
                    producer.push_iter(&mut samples);
                },
                |error| eprintln!("Audio input stream error: {error}"),
                None,
            )
            .context("Failed to build input stream");
    }
    let mut resampler = InputResampler::new(channels, input_sample_rate, output_sample_rate);
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                resampler.process(data, &mut producer);
            },
            |error| eprintln!("Audio input stream error: {error}"),
            None,
        )
        .context("Failed to build input stream")
}

/// Run a SunMao plugin as a standalone application.
pub fn run_standalone<P: SunmaoPlugin>() -> Result<()> {
    run_standalone_with_config::<P>(RuntimeConfig::default())
}

/// Run a SunMao plugin as a standalone application with custom configuration.
pub fn run_standalone_with_config<P: SunmaoPlugin>(config: RuntimeConfig) -> Result<()> {
    config.validate()?;
    // Get audio host and output device
    let host = cpal::default_host();
    let output_device = host
        .default_output_device()
        .context("No output device available")?;

    println!(
        "Output device: {}",
        output_device.name().unwrap_or_default()
    );

    let output_config = output_device.default_output_config()?;
    let sample_format = output_config.sample_format();
    let sample_rate = config
        .sample_rate
        .map(cpal::SampleRate)
        .unwrap_or(output_config.sample_rate());
    let channels = output_config.channels() as usize;
    if sample_rate.0 == 0
        || sample_rate.0 > MAX_STANDALONE_SAMPLE_RATE
        || channels == 0
        || channels > MAX_STANDALONE_CHANNELS as usize
    {
        bail!("Output device reported an invalid stream configuration");
    }
    println!(
        "Audio: {} Hz, {} channels, {}",
        sample_rate.0, channels, sample_format
    );

    let plugin = std::panic::catch_unwind(P::default)
        .map_err(|_| anyhow::anyhow!("plugin default construction panicked"))?;
    let standalone_view = standalone_view(&plugin)?;
    let processing_block_size = config.buffer_size.unwrap_or(1024).max(1) as usize;
    let processor = StandaloneProcessor::new(plugin, sample_rate.0 as f64, processing_block_size)?;
    let accepts_midi = processor.accepts_midi;
    let input_mode = config.input_mode.resolve(processor.input_channels());
    let external_input = if input_mode == InputMode::External {
        discover_cpal_input(&host, sample_rate.0)?
    } else {
        None
    };
    let input_channels = match input_mode {
        InputMode::Auto => unreachable!("automatic input mode must be resolved before setup"),
        InputMode::External => external_input.as_ref().map_or(0, |input| input.channels),
        InputMode::System => channels,
        InputMode::None => 0,
    };

    // One second of interleaved input. Size this from the capture device, not
    // the output or plugin layout: an interface may expose many more input
    // channels, and capture callbacks publish complete device frames.
    let ring_size = input_ring_capacity(sample_rate.0, input_channels)?;
    let rb = RingBuffer::<f32>::new(ring_size);
    let (producer, consumer) = rb.split();

    // Setup input based on mode
    #[cfg(all(target_os = "macos", feature = "system-capture"))]
    let mut _system_capture = None;
    let _input_stream: Option<cpal::Stream> = match input_mode {
        InputMode::Auto => unreachable!("automatic input mode must be resolved before setup"),
        InputMode::External => match external_input {
            Some(input) => Some(start_cpal_input(input, sample_rate.0, producer)?),
            None => {
                drop(producer);
                None
            }
        },
        InputMode::System => {
            #[cfg(all(target_os = "macos", feature = "system-capture"))]
            {
                _system_capture = Some(setup_ruhear_input(channels, producer)?);
                None
            }
            #[cfg(not(all(target_os = "macos", feature = "system-capture")))]
            {
                let _ = producer;
                bail!("System capture requires macOS and the 'system-capture' feature");
            }
        }
        InputMode::None => {
            drop(producer);
            None
        }
    };

    let midi_capacity = P::MAX_EVENTS_PER_BLOCK
        .max(1)
        .checked_mul(4)
        .filter(|capacity| *capacity <= MAX_STANDALONE_EVENTS_PER_BLOCK.saturating_mul(4))
        .context("MIDI event queue exceeds standalone memory limit")?;
    let midi_rb = RingBuffer::<MidiMessage>::new(midi_capacity);
    let (midi_producer, midi_consumer) = midi_rb.split();
    let _midi_connection = if accepts_midi {
        setup_midi_input(midi_producer)?
    } else {
        drop(midi_producer);
        None
    };

    // Setup output stream
    let stream_config = cpal::StreamConfig {
        channels: channels as u16,
        sample_rate,
        buffer_size: config
            .buffer_size
            .map(cpal::BufferSize::Fixed)
            .unwrap_or(cpal::BufferSize::Default),
    };
    let output_stream = build_output_stream(
        &output_device,
        &stream_config,
        sample_format,
        channels,
        input_channels,
        processor,
        consumer,
        midi_consumer,
    )?;

    output_stream.play()?;

    println!("SunMao Standalone: {} running...", P::NAME);
    println!("Input mode: {:?}", input_mode);
    if let Some((view, context)) = standalone_view {
        return match view.open_standalone(context, StandaloneViewOptions::interactive()) {
            StandaloneViewResult::Closed => Ok(()),
            StandaloneViewResult::Unsupported => {
                bail!("plugin view does not support a top-level standalone window")
            }
            StandaloneViewResult::Failed => bail!("standalone view failed to initialize"),
        };
    }

    println!("Press Ctrl+C to exit");

    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

struct CpalInputConfig {
    device: cpal::Device,
    stream: cpal::StreamConfig,
    sample_format: SampleFormat,
    channels: usize,
}

fn discover_cpal_input(
    host: &cpal::Host,
    output_sample_rate: u32,
) -> Result<Option<CpalInputConfig>> {
    let input_device = match host.default_input_device() {
        Some(d) => d,
        None => {
            println!("No input device available");
            return Ok(None);
        }
    };

    println!("Input device: {}", input_device.name().unwrap_or_default());
    let supported = input_device.default_input_config()?;
    let sample_format = supported.sample_format();
    let channels = supported.channels() as usize;
    let input_sample_rate = supported.sample_rate();
    if channels == 0
        || channels > MAX_STANDALONE_CHANNELS as usize
        || input_sample_rate.0 == 0
        || input_sample_rate.0 > MAX_STANDALONE_SAMPLE_RATE
        || output_sample_rate == 0
        || output_sample_rate > MAX_STANDALONE_SAMPLE_RATE
    {
        bail!("Input device reported an invalid stream configuration");
    }
    let input_config = cpal::StreamConfig {
        channels: supported.channels(),
        sample_rate: input_sample_rate,
        buffer_size: cpal::BufferSize::Default,
    };
    if input_sample_rate.0 == output_sample_rate {
        println!(
            "Input audio: {} Hz, {} channels, {}",
            input_sample_rate.0, channels, sample_format
        );
    } else {
        println!(
            "Input audio: {} Hz -> {} Hz, {} channels, {} (resampling)",
            input_sample_rate.0, output_sample_rate, channels, sample_format
        );
    }
    Ok(Some(CpalInputConfig {
        device: input_device,
        stream: input_config,
        sample_format,
        channels,
    }))
}

fn start_cpal_input(
    input: CpalInputConfig,
    output_sample_rate: u32,
    producer: Producer<f32>,
) -> Result<cpal::Stream> {
    let stream = build_input_stream(
        &input.device,
        &input.stream,
        input.sample_format,
        input.channels,
        output_sample_rate,
        producer,
    )?;

    stream.play()?;
    Ok(stream)
}

fn setup_midi_input(
    mut producer: Producer<MidiMessage>,
) -> Result<Option<midir::MidiInputConnection<()>>> {
    let mut input = midir::MidiInput::new("SunMao Standalone MIDI")?;
    input.ignore(midir::Ignore::None);
    let Some(port) = input.ports().into_iter().next() else {
        println!("No MIDI input available");
        return Ok(None);
    };
    let port_name = input.port_name(&port).unwrap_or_default();
    println!("MIDI input: {port_name}");
    let connection = input
        .connect(
            &port,
            "sunmao-midi-input",
            move |_timestamp, bytes, _state| {
                if bytes.is_empty() || bytes[0] == 0xf0 {
                    return;
                }
                let mut data = [0; 3];
                let len = bytes.len().min(data.len());
                data[..len].copy_from_slice(&bytes[..len]);
                let _ = producer.push(MidiMessage { offset: 0, data });
            },
            (),
        )
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    Ok(Some(connection))
}

#[cfg(all(target_os = "macos", feature = "system-capture"))]
fn setup_ruhear_input(channels: usize, mut producer: Producer<f32>) -> Result<ruhear::RUHear> {
    println!("Setting up system audio capture via ruhear...");
    let mut interleaved_frame = vec![0.0_f32; channels];

    let callback = move |audio_buffers: ruhear::RUBuffers| {
        let Some(frames) = audio_buffers.iter().map(Vec::len).min() else {
            return;
        };
        let Some(complete_samples) = frames.checked_mul(channels) else {
            return;
        };
        if producer.remaining() < complete_samples {
            return;
        }

        for frame in 0..frames {
            for channel in 0..channels {
                let source_channel = if audio_buffers.len() == 1 { 0 } else { channel };
                let sample = audio_buffers
                    .get(source_channel)
                    .and_then(|buffer| buffer.get(frame))
                    .copied()
                    .unwrap_or(0.0);
                interleaved_frame[channel] = sample;
            }
            let _ = producer.push_slice(&interleaved_frame);
        }
    };
    let callback = std::sync::Arc::new(std::sync::Mutex::new(callback));
    let mut capture = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ruhear::RUHear::new(callback)
    }))
    .map_err(|_| {
        anyhow::anyhow!(
            "failed to initialize system audio capture; grant Screen Recording permission and ensure a display is available"
        )
    })?;
    capture
        .start()
        .context("Failed to start system audio capture")?;

    Ok(capture)
}

/// Run a test tone to verify audio output.
pub fn run_test_tone() -> Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .context("No output device available")?;

    let supported = device.default_output_config()?;
    let sample_format = supported.sample_format();
    let sample_rate = supported.sample_rate().0 as f32;
    let channels = supported.channels() as usize;
    let config = supported.into();

    let stream = build_test_tone_stream(&device, &config, sample_format, sample_rate, channels)?;

    stream.play()?;
    println!("Playing 440Hz test tone. Press Ctrl+C to stop.");

    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn build_test_tone_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: SampleFormat,
    sample_rate: f32,
    channels: usize,
) -> Result<cpal::Stream> {
    if channels == 0
        || channels > MAX_STANDALONE_CHANNELS as usize
        || !sample_rate.is_finite()
        || sample_rate <= 0.0
        || sample_rate > MAX_STANDALONE_SAMPLE_RATE as f32
    {
        bail!("Output device reported an invalid channel count or sample rate");
    }
    match sample_format {
        SampleFormat::I8 => {
            build_test_tone_stream_typed::<i8>(device, config, sample_rate, channels)
        }
        SampleFormat::I16 => {
            build_test_tone_stream_typed::<i16>(device, config, sample_rate, channels)
        }
        SampleFormat::I32 => {
            build_test_tone_stream_typed::<i32>(device, config, sample_rate, channels)
        }
        SampleFormat::I64 => {
            build_test_tone_stream_typed::<i64>(device, config, sample_rate, channels)
        }
        SampleFormat::U8 => {
            build_test_tone_stream_typed::<u8>(device, config, sample_rate, channels)
        }
        SampleFormat::U16 => {
            build_test_tone_stream_typed::<u16>(device, config, sample_rate, channels)
        }
        SampleFormat::U32 => {
            build_test_tone_stream_typed::<u32>(device, config, sample_rate, channels)
        }
        SampleFormat::U64 => {
            build_test_tone_stream_typed::<u64>(device, config, sample_rate, channels)
        }
        SampleFormat::F32 => {
            build_test_tone_stream_typed::<f32>(device, config, sample_rate, channels)
        }
        SampleFormat::F64 => {
            build_test_tone_stream_typed::<f64>(device, config, sample_rate, channels)
        }
        _ => bail!("Unsupported output sample format: {sample_format}"),
    }
}

fn build_test_tone_stream_typed<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_rate: f32,
    channels: usize,
) -> Result<cpal::Stream>
where
    T: SizedSample + FromSample<f32>,
{
    if channels == 0
        || channels > MAX_STANDALONE_CHANNELS as usize
        || !sample_rate.is_finite()
        || sample_rate <= 0.0
        || sample_rate > MAX_STANDALONE_SAMPLE_RATE as f32
    {
        bail!("Output device reported an invalid channel count or sample rate");
    }
    let mut phase = 0.0f32;
    let frequency = 440.0f32;

    device
        .build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                for frame in data.chunks_mut(channels) {
                    let sample = (phase * std::f32::consts::TAU).sin() * 0.25;
                    for sample_out in frame.iter_mut() {
                        *sample_out = T::from_sample(sample);
                    }
                    phase += frequency / sample_rate;
                    if phase >= 1.0 {
                        phase -= 1.0;
                    }
                }
            },
            |err| eprintln!("Error: {}", err),
            None,
        )
        .context("Failed to build test-tone output stream")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::sync::atomic::{AtomicI64, AtomicU32, AtomicUsize, Ordering};
    use std::sync::Arc;
    use sunmao_core::{ParamDescriptor, Params};

    #[test]
    fn runtime_config_rejects_zero_stream_values() {
        assert!(RuntimeConfig {
            sample_rate: Some(0),
            ..RuntimeConfig::default()
        }
        .validate()
        .is_err());
        assert!(RuntimeConfig {
            buffer_size: Some(0),
            ..RuntimeConfig::default()
        }
        .validate()
        .is_err());
        assert!(RuntimeConfig {
            sample_rate: Some(MAX_STANDALONE_SAMPLE_RATE + 1),
            ..RuntimeConfig::default()
        }
        .validate()
        .is_err());
        assert!(RuntimeConfig {
            buffer_size: Some(MAX_STANDALONE_BUFFER_SIZE + 1),
            ..RuntimeConfig::default()
        }
        .validate()
        .is_err());
        assert!(RuntimeConfig {
            sample_rate: Some(48_000),
            buffer_size: Some(256),
            ..RuntimeConfig::default()
        }
        .validate()
        .is_ok());
    }

    #[test]
    fn automatic_input_mode_keeps_synths_device_free_and_feeds_effects() {
        assert_eq!(InputMode::default(), InputMode::Auto);
        assert_eq!(RuntimeConfig::default().input_mode, InputMode::Auto);
        assert_eq!(InputMode::Auto.resolve(0), InputMode::None);
        assert_eq!(InputMode::Auto.resolve(1), InputMode::External);
        assert_eq!(InputMode::Auto.resolve(2), InputMode::External);
        assert_eq!(InputMode::System.resolve(0), InputMode::System);
        assert_eq!(InputMode::None.resolve(2), InputMode::None);
    }

    #[test]
    fn input_ring_capacity_tracks_the_capture_device_layout() {
        assert_eq!(input_ring_capacity(48_000, 0).unwrap(), 1);
        assert_eq!(input_ring_capacity(48_000, 2).unwrap(), 96_000);
        assert_eq!(input_ring_capacity(48_000, 32).unwrap(), 1_536_000);
        assert!(input_ring_capacity(0, 2).is_err());
        assert!(input_ring_capacity(48_000, MAX_STANDALONE_CHANNELS as usize + 1).is_err());
        assert!(input_ring_capacity(MAX_STANDALONE_SAMPLE_RATE, 32).is_err());
    }

    #[test]
    fn invalid_resampler_configuration_is_a_noop() {
        let rb = RingBuffer::<f32>::new(16);
        let (mut producer, mut consumer) = rb.split();
        let mut resampler = InputResampler::new(0, 0, 0);
        resampler.process(&[1.0, 2.0], &mut producer);
        assert!(consumer.pop().is_none());
    }

    struct TestAllocator;

    thread_local! {
        static ALLOCATOR_CALLS: Cell<isize> = const { Cell::new(-1) };
    }

    fn record_allocator_call() {
        let _ = ALLOCATOR_CALLS.try_with(|calls| {
            let current = calls.get();
            if current >= 0 {
                calls.set(current + 1);
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
            ALLOCATOR_CALLS.with(|calls| calls.set(-1));
        }
    }

    fn count_allocator_calls<R>(callback: impl FnOnce() -> R) -> (R, usize) {
        ALLOCATOR_CALLS.with(|calls| {
            assert_eq!(calls.get(), -1);
            calls.set(0);
        });
        let scope = AllocationScope;
        let result = callback();
        let calls = ALLOCATOR_CALLS.with(|calls| calls.get() as usize);
        drop(scope);
        (result, calls)
    }

    struct EmptyParams;

    impl Params for EmptyParams {
        fn get_normalized(&self, _id: &str) -> Option<f32> {
            None
        }

        fn set_normalized(&self, _id: &str, _value: f32) {}

        fn descriptors(&self) -> Vec<ParamDescriptor> {
            Vec::new()
        }
    }

    #[derive(Default)]
    struct EffectState {
        initialized_frames: AtomicUsize,
        process_calls: AtomicUsize,
        last_sample_pos: AtomicI64,
        resets: AtomicUsize,
    }

    #[derive(Default)]
    struct TestEffect {
        state: Arc<EffectState>,
    }

    impl SunmaoPlugin for TestEffect {
        const NAME: &'static str = "Runtime Test Effect";
        const VENDOR: &'static str = "SunMao";
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

        fn initialize(&mut self, _sample_rate: f64, max_block_size: u32) {
            self.state
                .initialized_frames
                .store(max_block_size as usize, Ordering::SeqCst);
        }

        fn reset(&mut self) {
            self.state.resets.fetch_add(1, Ordering::SeqCst);
        }

        fn process(
            &mut self,
            buffer: &mut AudioBuffer,
            _events: &EventQueue,
            context: &ProcessContext,
        ) -> ProcessStatus {
            self.state.process_calls.fetch_add(1, Ordering::SeqCst);
            self.state
                .last_sample_pos
                .store(context.sample_pos, Ordering::SeqCst);
            for sample in 0..buffer.num_samples() {
                buffer.output(0)[sample] = buffer.input(0)[sample] * 0.5;
            }
            ProcessStatus::Normal
        }
    }

    #[derive(Default)]
    struct SynthState {
        midi_count: AtomicUsize,
        midi_offset: AtomicU32,
        process_calls: AtomicUsize,
    }

    #[derive(Default)]
    struct TestSynth {
        state: Arc<SynthState>,
    }

    impl SunmaoPlugin for TestSynth {
        const NAME: &'static str = "Runtime Test Synth";
        const VENDOR: &'static str = "SunMao";
        const URL: &'static str = "https://example.invalid";
        const MAX_EVENTS_PER_BLOCK: usize = 1;
        type Params = EmptyParams;

        fn input_channels(&self) -> u32 {
            0
        }

        fn output_channels(&self) -> u32 {
            2
        }

        fn accepts_midi(&self) -> bool {
            true
        }

        fn params(&self) -> Arc<Self::Params> {
            Arc::new(EmptyParams)
        }

        fn process(
            &mut self,
            buffer: &mut AudioBuffer,
            events: &EventQueue,
            _context: &ProcessContext,
        ) -> ProcessStatus {
            self.state.process_calls.fetch_add(1, Ordering::SeqCst);
            let mut count = 0;
            for message in events.midi_events() {
                count += 1;
                self.state
                    .midi_offset
                    .store(message.offset, Ordering::SeqCst);
            }
            self.state.midi_count.store(count, Ordering::SeqCst);
            for sample in 0..buffer.num_samples() {
                buffer.output(0)[sample] = 0.25;
                buffer.output(1)[sample] = -0.25;
            }
            ProcessStatus::Normal
        }
    }

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
            _context: &ProcessContext,
        ) -> ProcessStatus {
            ProcessStatus::Normal
        }
    }

    #[test]
    fn standalone_constructor_rejects_unallocatable_event_capacity() {
        let result = StandaloneProcessor::new(HugeEventCapacityPlugin, 48_000.0, 8);
        assert!(result.is_err());
    }

    #[derive(Default)]
    struct HugeChannelCountPlugin;

    impl SunmaoPlugin for HugeChannelCountPlugin {
        const NAME: &'static str = "Huge Channel Count";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        type Params = EmptyParams;

        fn input_channels(&self) -> u32 {
            u32::MAX
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
            _context: &ProcessContext,
        ) -> ProcessStatus {
            ProcessStatus::Normal
        }
    }

    #[test]
    fn standalone_constructor_rejects_huge_channel_and_block_limits() {
        assert!(StandaloneProcessor::new(HugeChannelCountPlugin, 48_000.0, 8).is_err());
        assert!(StandaloneProcessor::new(TestEffect::default(), 48_000.0, usize::MAX).is_err());
        assert!(StandaloneProcessor::new(TestEffect::default(), f64::INFINITY, 8).is_err());
    }

    #[derive(Default)]
    struct PanickingResetPlugin;

    impl SunmaoPlugin for PanickingResetPlugin {
        const NAME: &'static str = "Panicking Reset";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
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

        fn reset(&mut self) {
            panic!("intentional reset panic");
        }

        fn process(
            &mut self,
            _buffer: &mut AudioBuffer,
            _events: &EventQueue,
            _context: &ProcessContext,
        ) -> ProcessStatus {
            ProcessStatus::Normal
        }
    }

    #[test]
    fn standalone_drop_contains_plugin_reset_panics() {
        let result = std::panic::catch_unwind(|| {
            let processor =
                StandaloneProcessor::new(PanickingResetPlugin, 48_000.0, 8).expect("construct");
            drop(processor);
        });
        assert!(result.is_ok());
    }

    #[derive(Default)]
    struct PanickingInitializePlugin;

    impl SunmaoPlugin for PanickingInitializePlugin {
        const NAME: &'static str = "Panicking Initialize";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        type Params = EmptyParams;

        fn params(&self) -> Arc<Self::Params> {
            Arc::new(EmptyParams)
        }

        fn initialize(&mut self, _sample_rate: f64, _max_block_size: u32) {
            panic!("intentional initialize panic");
        }

        fn process(
            &mut self,
            _buffer: &mut AudioBuffer,
            _events: &EventQueue,
            _context: &ProcessContext,
        ) -> ProcessStatus {
            ProcessStatus::Normal
        }
    }

    #[test]
    fn standalone_constructor_converts_initialize_panics_to_errors() {
        let result = std::panic::catch_unwind(|| {
            StandaloneProcessor::new(PanickingInitializePlugin, 48_000.0, 8)
        });
        assert!(result.is_ok());
        assert!(result.unwrap().is_err());
    }

    #[derive(Default)]
    struct PanickingProcessPlugin;

    impl SunmaoPlugin for PanickingProcessPlugin {
        const NAME: &'static str = "Panicking Process";
        const VENDOR: &'static str = "SunMao Test";
        const URL: &'static str = "https://example.invalid";
        type Params = EmptyParams;

        fn input_channels(&self) -> u32 {
            0
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
            _context: &ProcessContext,
        ) -> ProcessStatus {
            panic!("intentional process panic");
        }
    }

    #[test]
    fn standalone_process_panics_are_contained_and_poison_the_processor() {
        let mut processor =
            StandaloneProcessor::new(PanickingProcessPlugin, 48_000.0, 8).expect("construct");
        let mut output = [1.0_f32; 2];
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            processor.process_device_interleaved(&mut output, 1, 0, || None, || None)
        }));
        assert_eq!(
            result.expect("panic must be contained"),
            ProcessStatus::Error
        );
        assert_eq!(output, [0.0; 2]);

        output.fill(1.0);
        assert_eq!(
            processor.process_device_interleaved(&mut output, 1, 0, || None, || None),
            ProcessStatus::Error
        );
        assert_eq!(output, [0.0; 2]);
    }

    #[test]
    fn effect_processing_deinterleaves_chunks_and_duplicates_mono_output() {
        let state = Arc::new(EffectState::default());
        let plugin = TestEffect {
            state: Arc::clone(&state),
        };
        let mut processor =
            StandaloneProcessor::new(plugin, 48_000.0, 2).expect("test event capacity");
        assert_eq!(state.initialized_frames.load(Ordering::SeqCst), 2);

        let input = [1.0, 0.9, 0.5, 0.4, -0.5, -0.4];
        let mut input = input.into_iter();
        let mut output = [9.0_f32; 7];
        assert_eq!(
            processor.process_device_interleaved(&mut output, 2, 2, || input.next(), || None,),
            ProcessStatus::Normal
        );
        assert_eq!(output, [0.5, 0.5, 0.25, 0.25, -0.25, -0.25, 0.0]);
        assert_eq!(state.process_calls.load(Ordering::SeqCst), 2);
        assert_eq!(state.last_sample_pos.load(Ordering::SeqCst), 2);
        assert_eq!(input.next(), None);

        drop(processor);
        assert_eq!(state.resets.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn synth_receives_midi_and_writes_stereo_typed_output() {
        let state = Arc::new(SynthState {
            midi_offset: AtomicU32::new(u32::MAX),
            ..SynthState::default()
        });
        let plugin = TestSynth {
            state: Arc::clone(&state),
        };
        let mut processor =
            StandaloneProcessor::new(plugin, 44_100.0, 8).expect("test event capacity");
        let mut output = [0_i16; 6];
        let mut midi = Some(MidiMessage::note_on(99, 0, 60, 100));

        assert_eq!(
            processor.process_device_interleaved(
                &mut output,
                2,
                0,
                || panic!("synth must not consume audio input"),
                || midi.take(),
            ),
            ProcessStatus::Normal
        );
        assert_eq!(state.midi_count.load(Ordering::SeqCst), 1);
        assert_eq!(state.midi_offset.load(Ordering::SeqCst), 2);
        assert_eq!(state.process_calls.load(Ordering::SeqCst), 1);
        assert!(output
            .chunks_exact(2)
            .all(|frame| frame[0] > 0 && frame[1] < 0));
    }

    #[test]
    fn event_overflow_silences_the_entire_device_callback() {
        let state = Arc::new(SynthState::default());
        let plugin = TestSynth {
            state: Arc::clone(&state),
        };
        let mut processor =
            StandaloneProcessor::new(plugin, 44_100.0, 8).expect("test event capacity");
        let mut output = [1.0_f32; 4];
        let events = [
            MidiMessage::note_on(0, 0, 60, 100),
            MidiMessage::note_on(0, 0, 64, 100),
        ];
        let mut events = events.into_iter();

        assert_eq!(
            processor.process_device_interleaved(&mut output, 2, 0, || None, || events.next(),),
            ProcessStatus::Error
        );
        assert_eq!(output, [0.0; 4]);
        assert_eq!(state.process_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn standalone_dsp_callback_does_not_allocate_or_lock() {
        let plugin = TestEffect::default();
        let mut processor =
            StandaloneProcessor::new(plugin, 48_000.0, 4).expect("test event capacity");
        let input = [0.25_f32; 8];
        let mut input = input.into_iter();
        let mut output = [0.0_f32; 8];

        let (status, allocator_calls) = count_allocator_calls(|| {
            processor.process_device_interleaved(&mut output, 2, 2, || input.next(), || None)
        });
        assert_eq!(status, ProcessStatus::Normal);
        assert_eq!(allocator_calls, 0);
        assert_eq!(output, [0.125; 8]);
    }

    #[test]
    fn public_planar_processor_validates_channels_and_advances_transport() {
        let state = Arc::new(EffectState::default());
        let plugin = TestEffect {
            state: Arc::clone(&state),
        };
        let mut processor = StandaloneProcessor::new(plugin, 48_000.0, 4).expect("construct");
        assert_eq!(processor.input_channels(), 1);
        assert_eq!(processor.output_channels(), 1);
        assert_eq!(processor.max_frames(), 4);
        assert_eq!(processor.sample_rate(), 48_000.0);
        assert_eq!(processor.sample_position(), 0);
        assert!(!processor.is_poisoned());

        let input = [1.0_f32, 0.5, -0.5, -1.0];
        let inputs: [&[f32]; 1] = [&input];
        let mut output = [9.0_f32; 4];
        let mut outputs: [&mut [f32]; 1] = [&mut output];
        let status = processor
            .process(&inputs, &mut outputs, &[], 4)
            .expect("valid planar block");

        assert_eq!(status, ProcessStatus::Normal);
        drop(outputs);
        assert_eq!(output, [0.5, 0.25, -0.25, -0.5]);
        assert_eq!(processor.sample_position(), 4);
        let mut outputs: [&mut [f32]; 1] = [&mut output];
        assert!(processor.process(&[], &mut outputs, &[], 4).is_err());
        drop(outputs);
        assert_eq!(output, [0.0; 4]);
        assert_eq!(processor.sample_position(), 4);
    }

    #[test]
    fn input_resampler_is_continuous_and_allocation_free_across_callbacks() {
        let rb = RingBuffer::<f32>::new(4096);
        let (mut producer, mut consumer) = rb.split();
        let mut resampler = InputResampler::new(1, 44_100, 48_000);
        let first: Vec<f32> = (0..441).map(|sample| sample as f32 / 1_000.0).collect();
        let second: Vec<f32> = (441..882).map(|sample| sample as f32 / 1_000.0).collect();

        resampler.process(&first, &mut producer);
        let (_, allocator_calls) = count_allocator_calls(|| {
            resampler.process(&second, &mut producer);
        });
        assert_eq!(allocator_calls, 0);

        let mut output = Vec::new();
        while let Some(sample) = consumer.pop() {
            output.push(sample);
        }
        assert_eq!(output.len(), 959);
        assert_eq!(output.first().copied(), Some(0.0));
        assert!(output.windows(2).all(|pair| pair[1] >= pair[0]));
        assert!((output[1] - 0.000_918_75).abs() < 1.0e-5);
        assert!((output[958] - 0.880_912_5).abs() < 1.0e-3);
    }

    #[test]
    fn input_resampler_preserves_complete_interleaved_stereo_frames() {
        let rb = RingBuffer::<f32>::new(4096);
        let (mut producer, mut consumer) = rb.split();
        let mut resampler = InputResampler::new(2, 44_100, 48_000);
        let input: Vec<f32> = (0..441)
            .flat_map(|frame| [frame as f32 / 1_000.0, frame as f32 / 1_000.0 + 10.0])
            .collect();

        let (_, allocator_calls) = count_allocator_calls(|| {
            resampler.process(&input, &mut producer);
        });
        assert_eq!(allocator_calls, 0);

        let mut output = Vec::new();
        while let Some(sample) = consumer.pop() {
            output.push(sample);
        }
        assert_eq!(output.len() % 2, 0);
        assert!(output
            .chunks_exact(2)
            .all(|frame| (frame[1] - frame[0] - 10.0).abs() < 1.0e-5));
    }
}
