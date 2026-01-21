//! Standalone runtime for executing SunMao plugins.
//!
//! This module provides a way to run SunMao plugins as standalone applications
//! without needing a DAW host.
//!
//! ## Audio Input Modes
//! - **System**: Capture system audio output using `ruhear` (macOS only, requires `system-capture` feature)
//! - **External**: Use microphone/line-in via `cpal`
//! - **None**: No audio input (for synths)

use std::sync::Arc;
use anyhow::{Result, Context};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::RingBuffer;
use sunmao_core::{
    SunmaoPlugin, EventQueue,
    plugin::ProcessContext,
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

/// Run a SunMao plugin as a standalone application.
pub fn run_standalone<P: SunmaoPlugin + Sync + Send + 'static>() -> Result<()> {
    run_standalone_with_config::<P>(RuntimeConfig::default())
}

/// Run a SunMao plugin as a standalone application with custom configuration.
pub fn run_standalone_with_config<P: SunmaoPlugin + Sync + Send + 'static>(
    config: RuntimeConfig,
) -> Result<()> {
    // Get audio host and output device
    let host = cpal::default_host();
    let output_device = host
        .default_output_device()
        .context("No output device available")?;

    println!("Output device: {}", output_device.name().unwrap_or_default());

    let output_config = output_device.default_output_config()?;
    let sample_rate = config
        .sample_rate
        .map(cpal::SampleRate)
        .unwrap_or(output_config.sample_rate());
    let channels = output_config.channels() as usize;

    println!("Audio: {} Hz, {} channels", sample_rate.0, channels);

    // Create plugin instance
    let mut plugin = P::default();
    plugin.initialize(sample_rate.0 as f64, 1024);
    let plugin = Arc::new(std::sync::Mutex::new(plugin));

    // Create ring buffer for audio input (if needed)
    let ring_size = sample_rate.0 as usize * channels; // 1 second buffer
    let rb = RingBuffer::<f32>::new(ring_size);
    let (mut producer, mut consumer) = rb.split();

    // Setup input based on mode
    let _input_stream: Option<cpal::Stream> = match config.input_mode {
        InputMode::External => {
            setup_cpal_input(&host, sample_rate.0, channels, producer)?
        }
        InputMode::System => {
            #[cfg(all(target_os = "macos", feature = "system-capture"))]
            {
                setup_ruhear_input(sample_rate.0, channels, producer)?
            }
            #[cfg(not(all(target_os = "macos", feature = "system-capture")))]
            {
                println!("System capture requires macOS and 'system-capture' feature");
                None
            }
        }
        InputMode::None => None,
    };

    // Setup output stream
    let plugin_clone = Arc::clone(&plugin);
    let stream_config = cpal::StreamConfig {
        channels: channels as u16,
        sample_rate,
        buffer_size: config
            .buffer_size
            .map(cpal::BufferSize::Fixed)
            .unwrap_or(cpal::BufferSize::Default),
    };

    let output_stream = output_device.build_output_stream(
        &stream_config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let _plugin = plugin_clone.lock().unwrap();

            // Read input from ring buffer if available
            for sample in data.iter_mut() {
                *sample = consumer.pop().unwrap_or(0.0);
            }

            // Process context (for future use)
            let _ctx = ProcessContext {
                sample_rate: sample_rate.0 as f64,
                tempo: Some(120.0),
                is_playing: true,
                sample_pos: 0,
            };

            let _events = EventQueue::new();

            // TODO: Proper plugin processing with deinterleaved buffers
        },
        |err| eprintln!("Audio stream error: {}", err),
        None,
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
    channels: usize,
    mut producer: ringbuf::Producer<f32>,
) -> Result<Option<cpal::Stream>> {
    let input_device = match host.default_input_device() {
        Some(d) => d,
        None => {
            println!("No input device available");
            return Ok(None);
        }
    };

    println!("Input device: {}", input_device.name().unwrap_or_default());

    let input_config = cpal::StreamConfig {
        channels: channels as u16,
        sample_rate: cpal::SampleRate(sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let stream = input_device.build_input_stream(
        &input_config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            for &sample in data {
                let _ = producer.push(sample);
            }
        },
        |err| eprintln!("Input stream error: {}", err),
        None,
    )?;

    stream.play()?;
    Ok(Some(stream))
}

#[cfg(all(target_os = "macos", feature = "system-capture"))]
fn setup_ruhear_input(
    sample_rate: u32,
    _channels: usize,
    mut producer: ringbuf::Producer<f32>,
) -> Result<Option<cpal::Stream>> {
    println!("Setting up system audio capture via ruhear...");

    // Start ruhear capture in a separate thread
    std::thread::spawn(move || {
        let result = ruhear::start_listening(
            Some(sample_rate as f64),
            move |samples: &[f32], _info| {
                for &sample in samples {
                    let _ = producer.push(sample);
                }
            },
        );

        if let Err(e) = result {
            eprintln!("ruhear error: {:?}", e);
        }
    });

    Ok(None)
}

/// Run a test tone to verify audio output.
pub fn run_test_tone() -> Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .context("No output device available")?;

    let config = device.default_output_config()?;
    let sample_rate = config.sample_rate().0 as f32;
    let channels = config.channels() as usize;

    let mut phase = 0.0f32;
    let frequency = 440.0f32;

    let stream = device.build_output_stream(
        &config.into(),
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            for frame in data.chunks_mut(channels) {
                let sample = (phase * std::f32::consts::TAU).sin() * 0.25;
                for sample_out in frame.iter_mut() {
                    *sample_out = sample;
                }
                phase += frequency / sample_rate;
                if phase >= 1.0 {
                    phase -= 1.0;
                }
            }
        },
        |err| eprintln!("Error: {}", err),
        None,
    )?;

    stream.play()?;
    println!("Playing 440Hz test tone. Press Ctrl+C to stop.");

    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
