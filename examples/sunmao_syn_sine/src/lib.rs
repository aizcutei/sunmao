//! SunMao Sine Synth Example
//!
//! A simple sine wave synthesizer demonstrating the SunMao framework with MIDI.

use std::sync::Arc;
use sunmao_core::prelude::*;
use sunmao_macros::Params;

/// Voice state for polyphony.
struct Voice {
    note: u8,
    velocity: f32,
    phase: f64,
    active: bool,
}

impl Voice {
    fn new() -> Self {
        Self {
            note: 0,
            velocity: 0.0,
            phase: 0.0,
            active: false,
        }
    }
}

/// Synth parameters.
#[derive(Params)]
pub struct SineParams {
    /// Master volume (0.0 to 1.0).
    #[unit = "LinearGain"]
    pub volume: FloatParam,
}

impl Default for SineParams {
    fn default() -> Self {
        Self {
            volume: FloatParam::new("volume", "Volume", 0.5, 0.0, 1.0),
        }
    }
}

/// The Sine Synth plugin.
pub struct SineSynth {
    params: Arc<SineParams>,
    voices: [Voice; 8],
    sample_rate: f64,
}

impl Default for SineSynth {
    fn default() -> Self {
        Self {
            params: Arc::new(SineParams::default()),
            voices: std::array::from_fn(|_| Voice::new()),
            sample_rate: 44100.0,
        }
    }
}

impl SunmaoPlugin for SineSynth {
    const NAME: &'static str = "SunMao Sine Synth";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";

    type Params = SineParams;

    fn input_channels(&self) -> u32 {
        0
    } // Synth, no audio input
    fn output_channels(&self) -> u32 {
        2
    }
    fn accepts_midi(&self) -> bool {
        true
    }

    fn params(&self) -> Arc<Self::Params> {
        self.params.clone()
    }

    fn initialize(&mut self, sample_rate: f64, _max_block_size: u32) {
        self.sample_rate = sample_rate;
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        // Handle MIDI events
        for midi in events.midi_events() {
            if midi.is_note_on() {
                // Find free voice
                if let Some(voice) = self.voices.iter_mut().find(|v| !v.active) {
                    voice.note = midi.note();
                    voice.velocity = midi.velocity() as f32 / 127.0;
                    voice.phase = 0.0;
                    voice.active = true;
                }
            } else if midi.is_note_off() {
                // Release matching voice
                if let Some(voice) = self
                    .voices
                    .iter_mut()
                    .find(|v| v.active && v.note == midi.note())
                {
                    voice.active = false;
                }
            }
        }

        let volume = self.params.volume.get();
        let num_samples = buffer.num_samples();

        // Clear buffer
        buffer.clear();

        // Render voices
        for voice in self.voices.iter_mut().filter(|v| v.active) {
            let freq = 440.0 * 2.0_f64.powf((voice.note as f64 - 69.0) / 12.0);
            let phase_inc = freq / self.sample_rate;

            for i in 0..num_samples {
                let sample = (voice.phase * std::f64::consts::TAU).sin() as f32;
                let out = sample * voice.velocity * volume;

                // Add to both channels
                if buffer.num_output_channels() > 0 {
                    buffer.output(0)[i] += out;
                }
                if buffer.num_output_channels() > 1 {
                    buffer.output(1)[i] += out;
                }

                voice.phase += phase_inc;
                if voice.phase >= 1.0 {
                    voice.phase -= 1.0;
                }
            }
        }

        ProcessStatus::Normal
    }

    fn vst3_info() -> Vst3Info {
        Vst3Info {
            class_id: *b"SunMaoSynSine!!!",
            categories: &["Instrument", "Synth"],
        }
    }

    fn au_info() -> AuInfo {
        AuInfo {
            type_code: *b"aumu",
            subtype_code: *b"smss",
            manufacturer_code: *b"SunM",
        }
    }

    fn clap_info() -> ClapInfo {
        ClapInfo {
            id: "com.sunmao.synth.sine",
            features: &["instrument", "synthesizer", "stereo"],
        }
    }
}

// ============ VST3 Export ============
use sunmao_backend_vst3::SunmaoVst3Wrapper;

sunmao_backend_vst3::export_vst3_plugin!(SunmaoVst3Wrapper<SineSynth>);

// ============ AU Export (macOS only) ============
#[cfg(target_os = "macos")]
mod au_export {
    use super::*;
    use sunmao_backend_au::SunmaoAuWrapper;
    use sunmao_backend_au::{au_params, export_au_plugin, fourcc, PluginInfo};

    const AU_INFO: PluginInfo = PluginInfo {
        name: "SunMao Sine Synth",
        component_type: fourcc(b"aumu"),
        component_subtype: fourcc(b"smss"),
        manufacturer: fourcc(b"SunM"),
        version: 0x00010000,
        flags: 0,
        flags_mask: 0,
        input_channels: 0,
        output_channels: 2,
        supports_midi: true,
    };

    export_au_plugin!(
        SunMaoSineSynthFactory,
        SunmaoAuWrapper<SineSynth>,
        AU_INFO,
        au_params::<SineParams>()
    );
}

// ============ CLAP Export ============
mod clap_export {
    use super::*;
    use std::ffi::c_char;
    use sunmao_backend_clap::SunmaoClapWrapper;
    use sunmao_backend_clap::{export_clap_plugin, ClapFeature, ClapFeatures, PluginInfo};

    static PLUGIN_INFO: PluginInfo = PluginInfo {
        id: "com.sunmao.synth.sine\0",
        name: "SunMao Sine Synth\0",
        vendor: "aizcutei\0",
        url: "https://aizcutei.github.io/sunmao\0",
        manual_url: "\0",
        support_url: "\0",
        version: "1.0.0\0",
        description: "Simple sine wave synthesizer\0",
    };

    const FEATURES_LIST: [*const c_char; 3] = [
        ClapFeature::Instrument.as_ptr(),
        ClapFeature::Synthesizer.as_ptr(),
        std::ptr::null(),
    ];
    static FEATURES: ClapFeatures = ClapFeatures::new(&FEATURES_LIST);

    export_clap_plugin!(SunmaoClapWrapper<SineSynth>, PLUGIN_INFO, FEATURES);
}
