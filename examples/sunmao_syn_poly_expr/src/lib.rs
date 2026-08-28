//! SunMao Poly Expression Synth — Phase 2 acceptance fixture.
//!
//! M0 skeleton: an 8-voice sine synth driven by plain MIDI notes, built only
//! on the Phase 1 contract. M4 adds per-note expression/MPE input and
//! voice-info reporting.

use sunmao::prelude::*;

const VOICE_COUNT: usize = 8;

struct Voice {
    note: u8,
    velocity: f32,
    phase: f64,
    active: bool,
}

impl Voice {
    const fn idle() -> Self {
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
pub struct PolyExprParams {
    /// Master volume.
    pub volume: FloatParam,
}

impl Default for PolyExprParams {
    fn default() -> Self {
        Self {
            volume: FloatParam::new("volume", "Volume", 0.5, 0.0, 1.0),
        }
    }
}

/// The polyphonic expression synth plugin.
pub struct PolyExprSynth {
    params: Arc<PolyExprParams>,
    voices: [Voice; VOICE_COUNT],
    sample_rate: f64,
}

impl Default for PolyExprSynth {
    fn default() -> Self {
        Self {
            params: Arc::new(PolyExprParams::default()),
            voices: std::array::from_fn(|_| Voice::idle()),
            sample_rate: 44_100.0,
        }
    }
}

fn note_frequency(note: u8) -> f64 {
    440.0 * 2.0_f64.powf((f64::from(note) - 69.0) / 12.0)
}

impl SunmaoPlugin for PolyExprSynth {
    const NAME: &'static str = "SunMao Poly Expr Synth";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";

    type Params = PolyExprParams;

    fn input_channels(&self) -> u32 {
        0
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
            *voice = Voice::idle();
        }
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        // Skeleton semantics: notes apply at block rate; the M4 DSP handles
        // sample offsets and per-note expression.
        for message in events.midi_events() {
            if message.is_note_on() {
                // Take the first free voice, otherwise steal the oldest slot.
                let slot = self
                    .voices
                    .iter()
                    .position(|voice| !voice.active)
                    .unwrap_or(0);
                if let Some(voice) = self.voices.get_mut(slot) {
                    voice.note = message.note();
                    voice.velocity = f32::from(message.velocity()) / 127.0;
                    voice.phase = 0.0;
                    voice.active = true;
                }
            } else if message.is_note_off() {
                for voice in &mut self.voices {
                    if voice.active && voice.note == message.note() {
                        voice.active = false;
                    }
                }
            }
        }

        let volume = self.params.volume.get();
        let channels = buffer.num_output_channels();
        for sample_index in 0..buffer.num_samples() {
            let mut mixed = 0.0f32;
            for voice in &mut self.voices {
                if !voice.active {
                    continue;
                }
                let increment = note_frequency(voice.note) / self.sample_rate;
                mixed += (voice.phase * std::f64::consts::TAU).sin() as f32 * voice.velocity;
                voice.phase = (voice.phase + increment).fract();
            }
            let sample = mixed * volume / VOICE_COUNT as f32;
            for channel in 0..channels {
                buffer.output(channel)[sample_index] = sample;
            }
        }

        ProcessStatus::Normal
    }

    fn vst3_info() -> Vst3Info {
        Vst3Info {
            class_id: *b"SunMaoSynPolyEx!",
            categories: &["Instrument", "Synth"],
            ..Default::default()
        }
    }

    fn clap_info() -> ClapInfo {
        ClapInfo {
            id: "com.sunmao.synth.poly_expr",
            features: &["instrument", "synthesizer", "stereo"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(plugin: &mut PolyExprSynth, events: &EventQueue, samples: usize) -> Vec<f32> {
        let inputs: [&[f32]; 0] = [];
        let mut output_left = vec![0.0; samples];
        let mut output_right = vec![0.0; samples];
        let mut outputs: [&mut [f32]; 2] = [&mut output_left, &mut output_right];
        let mut buffer = AudioBuffer::new(&inputs, &mut outputs, samples);
        let status = plugin.process(
            &mut buffer,
            events,
            &ProcessContext {
                sample_rate: 48_000.0,
                tempo: None,
                is_playing: true,
                sample_pos: 0,
            },
        );
        assert_eq!(status, ProcessStatus::Normal);
        output_left
    }

    #[test]
    fn a_note_on_produces_audio_and_note_off_silences_it() {
        let mut plugin = PolyExprSynth::default();
        plugin.initialize(48_000.0, 64);

        let mut events = EventQueue::new();
        events.push(Event::Midi(MidiMessage::note_on(0, 0, 69, 100)));
        let sounding = render(&mut plugin, &events, 64);
        assert!(
            sounding.iter().any(|sample| sample.abs() > 1e-4),
            "an active voice must produce audio"
        );

        let mut off_events = EventQueue::new();
        off_events.push(Event::Midi(MidiMessage::note_off(0, 0, 69, 0)));
        let silent = render(&mut plugin, &off_events, 64);
        assert!(
            silent.iter().all(|sample| *sample == 0.0),
            "all voices released, output must be silent"
        );
    }

    #[test]
    fn chords_use_multiple_voices() {
        let mut plugin = PolyExprSynth::default();
        plugin.initialize(48_000.0, 64);

        let mut events = EventQueue::new();
        events.push(Event::Midi(MidiMessage::note_on(0, 0, 60, 100)));
        events.push(Event::Midi(MidiMessage::note_on(0, 0, 64, 100)));
        events.push(Event::Midi(MidiMessage::note_on(0, 0, 67, 100)));
        render(&mut plugin, &events, 16);
        assert_eq!(
            plugin.voices.iter().filter(|voice| voice.active).count(),
            3,
            "each held note occupies its own voice"
        );
    }
}

// ============ Unified VST3 + CLAP Export ============
sunmao::sunmao_export!(PolyExprSynth);
