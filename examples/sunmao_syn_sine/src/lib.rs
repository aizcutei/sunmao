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
#[cfg_attr(all(target_os = "macos", feature = "au"), sunmao_au)]
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

    fn reset(&mut self) {
        for voice in &mut self.voices {
            voice.active = false;
            voice.velocity = 0.0;
            voice.phase = 0.0;
        }
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        let mut volume = self.params.volume.get();
        let num_samples = buffer.num_samples();
        buffer.clear();
        let mut events = events.timed_events().peekable();

        for sample_index in 0..num_samples {
            while events
                .peek()
                .is_some_and(|event| event.offset() as usize <= sample_index)
            {
                match events.next().expect("peeked event") {
                    Event::Midi(event) if event.is_note_on() => {
                        if let Some(voice) = self.voices.iter_mut().find(|voice| !voice.active) {
                            voice.note = event.note();
                            voice.velocity = event.velocity() as f32 / 127.0;
                            voice.phase = 0.0;
                            voice.active = true;
                        }
                    }
                    Event::Midi(event) if event.is_note_off() => {
                        if let Some(voice) = self
                            .voices
                            .iter_mut()
                            .find(|voice| voice.active && voice.note == event.note())
                        {
                            voice.active = false;
                        }
                    }
                    Event::ParamChange { id, value, .. } if id == self.params.volume.id => {
                        volume = self.params.volume.min
                            + value.clamp(0.0, 1.0)
                                * (self.params.volume.max - self.params.volume.min);
                    }
                    _ => {}
                }
            }

            for voice in self.voices.iter_mut().filter(|voice| voice.active) {
                let freq = 440.0 * 2.0_f64.powf((voice.note as f64 - 69.0) / 12.0);
                let phase_inc = freq / self.sample_rate;
                let sample = (voice.phase * std::f64::consts::TAU).sin() as f32;
                let out = sample * voice.velocity * volume;

                if buffer.num_output_channels() > 0 {
                    buffer.output(0)[sample_index] += out;
                }
                if buffer.num_output_channels() > 1 {
                    buffer.output(1)[sample_index] += out;
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
            ..Default::default()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(samples: &[f32]) -> f32 {
        samples
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()))
    }

    #[test]
    fn timed_volume_changes_apply_at_sample_offsets_without_advancing_params() {
        let mut synth = SineSynth::default();
        synth.initialize(48_000.0, 16);
        let inputs: [&[f32]; 0] = [];
        let mut left = [0.0; 12];
        let mut right = [0.0; 12];
        let mut outputs: [&mut [f32]; 2] = [&mut left, &mut right];
        let mut buffer = AudioBuffer::new(&inputs, &mut outputs, 12);
        let mut events = EventQueue::new();
        events.push(Event::Midi(MidiMessage::note_on(0, 0, 69, 127)));
        events.push_param_change(ParamChange {
            id: "volume",
            value: 0.0,
            offset: 4,
        });
        events.push_param_change(ParamChange {
            id: "volume",
            value: 0.25,
            offset: 7,
        });
        events.push_param_change(ParamChange {
            id: "volume",
            value: 1.0,
            offset: 7,
        });

        let status = synth.process(
            &mut buffer,
            &events,
            &ProcessContext {
                sample_rate: 48_000.0,
                tempo: None,
                is_playing: true,
                sample_pos: 0,
            },
        );

        assert_eq!(status, ProcessStatus::Normal);
        assert!(peak(&left[..4]) > 1.0e-6);
        assert_eq!(peak(&left[4..7]), 0.0);
        assert!(peak(&left[7..]) > 1.0e-6);
        assert_eq!(right, left);
        assert_eq!(synth.params.volume.get(), 0.5);
    }
}

// ============ Unified VST3 + CLAP Export ============
sunmao::sunmao_export!(SineSynth);

// ============ AU Export (macOS only) ============
#[cfg(all(target_os = "macos", feature = "au"))]
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
