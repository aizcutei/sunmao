//! Standalone runtime for executing SunMao plugins.
//!
//! This module provides a way to run SunMao plugins as standalone applications
//! without needing a DAW host.
//!
//! ## Audio Input Modes
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
    AudioBuffer, Event, EventQueue, MidiMessage, SunmaoPlugin,
};

/// Audio input mode for standalone runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Capture system audio output (macOS only, uses ruhear)
    System,
    /// Use external audio input (microphone/line-in via cpal)
    External,
    /// No audio input (for synths)
    None,
}

impl Default for InputMode {
    fn default() -> Self {
        InputMode::None
    }
}

/// Configuration for the standalone runtime.
pub struct RuntimeConfig {
    /// Audio input mode
    pub input_mode: InputMode,
    /// Sample rate (default: use device default)
    pub sample_rate: Option<u32>,
    /// Buffer size (default: use device default)
    pub buffer_size: Option<u32>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            input_mode: InputMode::None,
            sample_rate: None,
            buffer_size: None,
        }
    }
}

struct StandaloneProcessor<P: SunmaoPlugin> {
    plugin: P,
    input_buffers: Vec<Vec<f32>>,
    output_buffers: Vec<Vec<f32>>,
    events: EventQueue,
    sample_rate: f64,
    sample_pos: i64,
    max_frames: usize,
}

impl<P: SunmaoPlugin> StandaloneProcessor<P> {
    fn new(mut plugin: P, sample_rate: f64, max_frames: usize) -> Self {
        let max_frames = max_frames.max(1);
        let input_channels = plugin.input_channels() as usize;
        let output_channels = plugin.output_channels() as usize;
        let input_buffers = vec![vec![0.0; max_frames]; input_channels];
        let output_buffers = vec![vec![0.0; max_frames]; output_channels];
        plugin.initialize(sample_rate, max_frames as u32);
        Self {
            plugin,
            input_buffers,
            output_buffers,
            events: EventQueue::with_capacity(P::MAX_EVENTS_PER_BLOCK),
            sample_rate,
            sample_pos: 0,
            max_frames,
        }
    }

    fn input_channels(&self) -> usize {
        self.input_buffers.len()
    }

    fn process_interleaved<T, I, M>(
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
        if device_output_channels == 0 {
            return ProcessStatus::Error;
        }

        let complete_samples = output.len() / device_output_channels * device_output_channels;
        let chunk_samples = self.max_frames.saturating_mul(device_output_channels);
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

            for output in &mut self.output_buffers {
                output[..frames].fill(0.0);
            }
            for channel in 0..self.input_buffers.len().min(self.output_buffers.len()) {
                self.output_buffers[channel][..frames]
                    .copy_from_slice(&self.input_buffers[channel][..frames]);
            }

            self.events.clear();
            let mut event_overflow = false;
            while let Some(mut message) = next_midi() {
                message.offset = message.offset.min(frames.saturating_sub(1) as u32);
                if !self.events.push(Event::Midi(message)) {
                    event_overflow = true;
                }
            }

            let status = if event_overflow {
                ProcessStatus::Error
            } else {
                let mut audio =
                    AudioBuffer::from_planar(&self.input_buffers, &mut self.output_buffers, frames);
                let context = ProcessContext {
                    sample_rate: self.sample_rate,
                    tempo: Some(120.0),
                    is_playing: true,
                    sample_pos: self.sample_pos,
                };
                self.plugin.process(&mut audio, &self.events, &context)
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

            self.sample_pos = self.sample_pos.saturating_add(frames as i64);
        }

        final_status
    }
}

impl<P: SunmaoPlugin> Drop for StandaloneProcessor<P> {
    fn drop(&mut self) {
        self.plugin.reset();
    }
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
                processor.process_interleaved(
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
}

impl InputResampler {
    fn new(channels: usize, input_sample_rate: u32, output_sample_rate: u32) -> Self {
        Self {
            channels,
            input_frames_per_output_frame: input_sample_rate as f64 / output_sample_rate as f64,
            next_output_position: 0.0,
            previous_frame: vec![0.0; channels],
            output_frame: vec![0.0; channels],
            initialized: false,
        }
    }

    fn maximum_output_samples(&self, input_frames: usize) -> Option<usize> {
        let ratio = 1.0 / self.input_frames_per_output_frame;
        let output_frames = ((input_frames as f64 * ratio).ceil() as usize).checked_add(1)?;
        output_frames.checked_mul(self.channels)
    }

    fn process<T>(&mut self, data: &[T], producer: &mut Producer<f32>)
    where
        T: Sample + Copy,
        f32: FromSample<T>,
    {
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
    if channels == 0 || input_sample_rate == 0 || output_sample_rate == 0 {
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
    if sample_rate.0 == 0 || channels == 0 {
        bail!("Output device reported an invalid stream configuration");
    }
    if config.buffer_size == Some(0) {
        bail!("Buffer size must be greater than zero");
    }

    println!(
        "Audio: {} Hz, {} channels, {}",
        sample_rate.0, channels, sample_format
    );

    let plugin = P::default();
    let accepts_midi = plugin.accepts_midi();
    let processing_block_size = config.buffer_size.unwrap_or(1024).max(1) as usize;
    let processor = StandaloneProcessor::new(plugin, sample_rate.0 as f64, processing_block_size);

    // One second of interleaved input. Capture callbacks only publish whole frames.
    let ring_size = (sample_rate.0 as usize)
        .checked_mul(channels.max(processor.input_channels()).max(2))
        .context("Input ring buffer size overflow")?;
    let rb = RingBuffer::<f32>::new(ring_size);
    let (producer, consumer) = rb.split();

    // Setup input based on mode
    #[cfg(all(target_os = "macos", feature = "system-capture"))]
    let mut _system_capture = None;
    let (_input_stream, input_channels): (Option<cpal::Stream>, usize) = match config.input_mode {
        InputMode::External => setup_cpal_input(&host, sample_rate.0, producer)?,
        InputMode::System => {
            #[cfg(all(target_os = "macos", feature = "system-capture"))]
            {
                _system_capture = Some(setup_ruhear_input(channels, producer)?);
                (None, channels)
            }
            #[cfg(not(all(target_os = "macos", feature = "system-capture")))]
            {
                let _ = producer;
                bail!("System capture requires macOS and the 'system-capture' feature");
            }
        }
        InputMode::None => {
            drop(producer);
            (None, 0)
        }
    };

    let midi_capacity = P::MAX_EVENTS_PER_BLOCK.max(1).saturating_mul(4);
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
    println!("Input mode: {:?}", config.input_mode);
    println!("Press Ctrl+C to exit");

    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn setup_cpal_input(
    host: &cpal::Host,
    sample_rate: u32,
    producer: Producer<f32>,
) -> Result<(Option<cpal::Stream>, usize)> {
    let input_device = match host.default_input_device() {
        Some(d) => d,
        None => {
            println!("No input device available");
            return Ok((None, 0));
        }
    };

    println!("Input device: {}", input_device.name().unwrap_or_default());
    let supported = input_device.default_input_config()?;
    let sample_format = supported.sample_format();
    let channels = supported.channels() as usize;
    let input_sample_rate = supported.sample_rate();
    if channels == 0 || input_sample_rate.0 == 0 || sample_rate == 0 {
        bail!("Input device reported an invalid stream configuration");
    }
    let input_config = cpal::StreamConfig {
        channels: supported.channels(),
        sample_rate: input_sample_rate,
        buffer_size: cpal::BufferSize::Default,
    };
    if input_sample_rate.0 == sample_rate {
        println!(
            "Input audio: {} Hz, {} channels, {}",
            input_sample_rate.0, channels, sample_format
        );
    } else {
        println!(
            "Input audio: {} Hz -> {} Hz, {} channels, {} (resampling)",
            input_sample_rate.0, sample_rate, channels, sample_format
        );
    }
    let stream = build_input_stream(
        &input_device,
        &input_config,
        sample_format,
        channels,
        sample_rate,
        producer,
    )?;

    stream.play()?;
    Ok((Some(stream), channels))
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
        fn ids() -> &'static [&'static str] {
            &[]
        }

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

    #[test]
    fn effect_processing_deinterleaves_chunks_and_duplicates_mono_output() {
        let state = Arc::new(EffectState::default());
        let plugin = TestEffect {
            state: Arc::clone(&state),
        };
        let mut processor = StandaloneProcessor::new(plugin, 48_000.0, 2);
        assert_eq!(state.initialized_frames.load(Ordering::SeqCst), 2);

        let input = [1.0, 0.9, 0.5, 0.4, -0.5, -0.4];
        let mut input = input.into_iter();
        let mut output = [9.0_f32; 7];
        assert_eq!(
            processor.process_interleaved(&mut output, 2, 2, || input.next(), || None,),
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
        let mut processor = StandaloneProcessor::new(plugin, 44_100.0, 8);
        let mut output = [0_i16; 6];
        let mut midi = Some(MidiMessage::note_on(99, 0, 60, 100));

        assert_eq!(
            processor.process_interleaved(
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
        let mut processor = StandaloneProcessor::new(plugin, 44_100.0, 8);
        let mut output = [1.0_f32; 4];
        let events = [
            MidiMessage::note_on(0, 0, 60, 100),
            MidiMessage::note_on(0, 0, 64, 100),
        ];
        let mut events = events.into_iter();

        assert_eq!(
            processor.process_interleaved(&mut output, 2, 0, || None, || events.next(),),
            ProcessStatus::Error
        );
        assert_eq!(output, [0.0; 4]);
        assert_eq!(state.process_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn standalone_dsp_callback_does_not_allocate_or_lock() {
        let plugin = TestEffect::default();
        let mut processor = StandaloneProcessor::new(plugin, 48_000.0, 4);
        let input = [0.25_f32; 8];
        let mut input = input.into_iter();
        let mut output = [0.0_f32; 8];

        let (status, allocator_calls) = count_allocator_calls(|| {
            processor.process_interleaved(&mut output, 2, 2, || input.next(), || None)
        });
        assert_eq!(status, ProcessStatus::Normal);
        assert_eq!(allocator_calls, 0);
        assert_eq!(output, [0.125; 8]);
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
