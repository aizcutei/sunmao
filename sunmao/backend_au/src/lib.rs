//! Audio Unit Backend Adapter for SunMao.
//!
//! This crate wraps `SunmaoPlugin` to expose it as an Audio Unit plugin via `au_rs`.
//! **macOS only**.

#![cfg(target_os = "macos")]

use au_rs::{BufferList, Plugin};
use std::sync::Arc;
use sunmao_core::events::MidiMessage;
use sunmao_core::plugin::ProcessContext as SunmaoProcessContext;
use sunmao_core::{AudioBuffer, Event as SunmaoEvent, EventQueue, Params, SunmaoPlugin};

pub use au_rs::{
    export_au_plugin, fourcc, AudioComponentDescription, AudioComponentPlugInInterface,
    ParameterInfo, ParameterUnit, PluginInfo,
};
pub use au_rs::{
    get_parameter_local, gl_get_proc_address, set_parameter_local, NSPoint, NSRect, NSSize,
};

/// AU parameter list provider for auto-registration.
pub trait SunmaoAuParamList {
    fn au_params() -> &'static [ParameterInfo];
}

/// Helper to fetch AU params for a Params type.
pub fn au_params<P: SunmaoAuParamList>() -> &'static [ParameterInfo] {
    P::au_params()
}

/// Wrapper that adapts a SunmaoPlugin to au_rs::Plugin.
pub struct SunmaoAuWrapper<P: SunmaoPlugin> {
    plugin: P,
    params: Arc<P::Params>,
    sample_rate: f64,
    max_frames: u32,
    is_synth: bool,
    // Temporary buffers for plugin processing
    input_buffers: Vec<Vec<f32>>,
    output_buffers: Vec<Vec<f32>>,
    // MIDI event queue
    pending_midi: Vec<MidiMessage>,
}

unsafe impl<P: SunmaoPlugin> Send for SunmaoAuWrapper<P> {}
unsafe impl<P: SunmaoPlugin> Sync for SunmaoAuWrapper<P> {}

impl<P: SunmaoPlugin> Plugin for SunmaoAuWrapper<P> {
    fn init(sample_rate: f64, max_frames: u32) -> Self {
        let mut plugin = P::default();
        plugin.initialize(sample_rate, max_frames);
        let params = plugin.params();
        let is_synth = plugin.input_channels() == 0;
        let in_ch = plugin.input_channels() as usize;
        let out_ch = plugin.output_channels() as usize;

        Self {
            plugin,
            params,
            sample_rate,
            max_frames,
            is_synth,
            input_buffers: vec![vec![0.0; max_frames as usize]; in_ch.max(2)],
            output_buffers: vec![vec![0.0; max_frames as usize]; out_ch.max(2)],
            pending_midi: Vec::with_capacity(128),
        }
    }

    fn reset(&mut self) {
        self.plugin.reset();
        self.pending_midi.clear();
    }

    fn process(
        &mut self,
        inputs: Option<BufferList<'_>>,
        outputs: &mut BufferList<'_>,
        frames: usize,
    ) {
        // Copy AU input to temp buffers
        if let Some(mut inputs) = inputs {
            let num_in_ch = inputs.len();
            for ch in 0..num_in_ch.min(self.input_buffers.len()) {
                let src = unsafe { inputs.channel_mut(ch) };
                let dst = &mut self.input_buffers[ch];
                let len = frames.min(src.len()).min(dst.len());
                dst[..len].copy_from_slice(&src[..len]);
            }
        }

        // Prepare output buffers
        if self.is_synth {
            // Synth: clear output buffers
            for buf in &mut self.output_buffers {
                for sample in buf.iter_mut().take(frames) {
                    *sample = 0.0;
                }
            }
        } else {
            // Effect: pre-copy input to output
            for ch in 0..self.input_buffers.len().min(self.output_buffers.len()) {
                let len = frames
                    .min(self.input_buffers[ch].len())
                    .min(self.output_buffers[ch].len());
                self.output_buffers[ch][..len].copy_from_slice(&self.input_buffers[ch][..len]);
            }
        }

        // Create AudioBuffer for SunmaoPlugin
        let input_refs: Vec<&[f32]> = self.input_buffers.iter().map(|b| &b[..frames]).collect();
        let mut output_refs: Vec<&mut [f32]> = self
            .output_buffers
            .iter_mut()
            .map(|b| &mut b[..frames])
            .collect();

        let mut audio_buffer = AudioBuffer::new(&input_refs, &mut output_refs, frames);

        // Create process context
        let transport = au_rs::current_transport();
        let ctx = SunmaoProcessContext {
            sample_rate: self.sample_rate,
            tempo: transport.tempo,
            is_playing: transport.is_playing.unwrap_or(true),
            sample_pos: transport.sample_pos.unwrap_or(0),
        };

        // Create event queue with pending MIDI
        let mut events = EventQueue::new();
        for midi in self.pending_midi.drain(..) {
            events.push(SunmaoEvent::Midi(midi));
        }

        // Call the actual plugin process
        let _status = self.plugin.process(&mut audio_buffer, &events, &ctx);

        // Copy output back to AU buffers
        let num_out_ch = outputs.len();
        for ch in 0..num_out_ch.min(self.output_buffers.len()) {
            let src = &self.output_buffers[ch];
            let dst = unsafe { outputs.channel_mut(ch) };
            let len = frames.min(src.len()).min(dst.len());
            dst[..len].copy_from_slice(&src[..len]);
        }
    }

    fn parameters(&self) -> &'static [ParameterInfo] {
        // Parameters are provided via export_au_plugin! macro
        &[]
    }

    fn get_parameter(&self, id: u32) -> f32 {
        let ids = P::Params::ids();
        if let Some(&param_id) = ids.get(id as usize) {
            self.params.get_normalized(param_id).unwrap_or(0.0)
        } else {
            0.0
        }
    }

    fn set_parameter(&mut self, id: u32, value: f32) {
        let ids = P::Params::ids();
        if let Some(&param_id) = ids.get(id as usize) {
            self.params.set_normalized(param_id, value);
        }
    }

    fn handle_midi_event(&mut self, status: u8, data1: u8, data2: u8, offset: u32) {
        // Queue raw MIDI events
        let midi = MidiMessage {
            offset,
            data: [status, data1, data2],
        };
        self.pending_midi.push(midi);
    }

    fn start_note(&mut self, pitch: f32, velocity: f32, offset: u32) -> u32 {
        // Convert AU note to MIDI note on
        let note = pitch as u8;
        let vel = (velocity * 127.0) as u8;
        let midi = MidiMessage::note_on(offset, 0, note, vel);
        self.pending_midi.push(midi);
        note as u32
    }

    fn stop_note(&mut self, note_id: u32, offset: u32) {
        // Convert AU note to MIDI note off
        let note = note_id as u8;
        let midi = MidiMessage::note_off(offset, 0, note, 0);
        self.pending_midi.push(midi);
    }
}

/// AU GUI configuration for Sunmao plugins.
///
/// Defaults to a normalized 0.0-1.0 range for the first parameter.
/// Plugins can override this by implementing `SunmaoAuGuiParams`.
pub trait SunmaoAuGuiParams: SunmaoPlugin {
    /// The value range for the primary parameter exposed by the AU GUI.
    fn au_gui_range() -> (f32, f32) {
        (0.0, 1.0)
    }
}

/// AU GUI re-exports for building custom AU GUIs in examples.
pub mod gui {
    pub use au_rs::gui::layer::{get_view_layer, Layer, TransactionGuard};
    pub use au_rs::gui::webview::{self, MessageCallback};
    pub use au_rs::gui::{
        flush_context, make_current_context, open_gl_context, set_best_resolution,
        set_needs_display, set_pixel_format, update_open_gl_view, view_backing_bounds, view_bounds,
        CocoaObject, GuiConfig, GuiHandler,
    };
}

/// Export helper macro for AU with default WGPU GUI.
#[macro_export]
macro_rules! sunmao_export_au {
    ($factory_fn:ident, $plugin:ty, $info:expr, $parameters:expr $(,)?) => {
        $crate::export_au_plugin!(
            $factory_fn,
            $crate::SunmaoAuWrapper<$plugin>,
            $info,
            $parameters,
            gui: None
        );
    };
    ($factory_fn:ident, $plugin:ty, $info:expr, $parameters:expr, gui: None $(,)?) => {
        $crate::export_au_plugin!(
            $factory_fn,
            $crate::SunmaoAuWrapper<$plugin>,
            $info,
            $parameters,
            gui: None
        );
    };
    ($factory_fn:ident, $plugin:ty, $info:expr, $parameters:expr, gui: { handler: $handler:ty, config: $config:expr } $(,)?) => {
        $crate::export_au_plugin!(
            $factory_fn,
            $crate::SunmaoAuWrapper<$plugin>,
            $info,
            $parameters,
            gui: { handler: $handler, config: $config }
        );
    };
}
