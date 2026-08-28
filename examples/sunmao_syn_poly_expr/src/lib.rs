//! SunMao Poly Expression Synth — Phase 2 acceptance fixture.
//!
//! An 8-voice sine synth driven by MIDI notes with per-note expression (M4):
//! tuning bends a single voice and volume scales it, keyed by the host's note
//! id where available and by channel/key otherwise.

use sunmao::prelude::*;

const VOICE_COUNT: usize = 8;

struct Voice {
    note: u8,
    channel: u8,
    /// Host-assigned note identity; voices started before a host supplied one
    /// keep `None` and are matched by channel/key instead.
    note_id: Option<i32>,
    velocity: f32,
    phase: f64,
    /// Per-note tuning in semitones.
    tuning: f64,
    /// Per-note volume multiplier.
    expression_gain: f32,
    active: bool,
}

impl Voice {
    const fn idle() -> Self {
        Self {
            note: 0,
            channel: 0,
            note_id: None,
            velocity: 0.0,
            phase: 0.0,
            tuning: 0.0,
            expression_gain: 1.0,
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
    note_frequency_with_tuning(note, 0.0)
}

/// Note frequency with a per-note tuning offset in semitones.
fn note_frequency_with_tuning(note: u8, semitones: f64) -> f64 {
    440.0 * 2.0_f64.powf((f64::from(note) - 69.0 + semitones) / 12.0)
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

    fn voice_info(&self) -> Option<VoiceInfo> {
        Some(VoiceInfo {
            active: self.voices.iter().filter(|voice| voice.active).count() as u32,
            capacity: VOICE_COUNT as u32,
            supports_overlapping_notes: false,
        })
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
                    *voice = Voice::idle();
                    voice.note = message.note();
                    voice.channel = message.channel();
                    voice.velocity = f32::from(message.velocity()) / 127.0;
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

        // Per-note expression is applied at block rate alongside the notes.
        for expression in events.note_expressions() {
            for voice in self.voices.iter_mut().filter(|voice| voice.active) {
                let matches = match (expression.note_id, voice.note_id) {
                    (Some(event_id), Some(voice_id)) => event_id == voice_id,
                    // Without note ids on both sides, fall back to the
                    // channel/key pair the format did provide.
                    _ => {
                        expression.key.is_none_or(|key| key == voice.note)
                            && expression.channel.is_none_or(|ch| ch == voice.channel)
                    }
                };
                if !matches {
                    continue;
                }
                match expression.kind {
                    NoteExpressionKind::Tuning => voice.tuning = expression.value,
                    NoteExpressionKind::Volume => {
                        voice.expression_gain = expression.value.clamp(0.0, 4.0) as f32
                    }
                    // Other dimensions are accepted but not rendered by this
                    // fixture.
                    _ => {}
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
                let increment =
                    note_frequency_with_tuning(voice.note, voice.tuning) / self.sample_rate;
                mixed += (voice.phase * std::f64::consts::TAU).sin() as f32
                    * voice.velocity
                    * voice.expression_gain;
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
                ..Default::default()
            },
        );
        assert_eq!(status, ProcessStatus::Normal);
        output_left
    }

    /// Renders one block and returns the peak absolute sample.
    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0f32, |acc, s| acc.max(s.abs()))
    }

    fn expression(kind: NoteExpressionKind, key: u8, value: f64) -> Event {
        Event::NoteExpression(NoteExpression {
            offset: 0,
            kind,
            note_id: None,
            channel: Some(0),
            key: Some(key),
            value,
        })
    }

    #[test]
    fn a_volume_expression_scales_only_the_addressed_voice() {
        let mut plugin = PolyExprSynth::default();
        plugin.initialize(48_000.0, 64);

        let mut events = EventQueue::new();
        events.push(Event::Midi(MidiMessage::note_on(0, 0, 69, 100)));
        let reference = peak(&render(&mut plugin, &events, 64));

        plugin.reset();
        let mut ducked_events = EventQueue::new();
        ducked_events.push(Event::Midi(MidiMessage::note_on(0, 0, 69, 100)));
        ducked_events.push(expression(NoteExpressionKind::Volume, 69, 0.25));
        let ducked = peak(&render(&mut plugin, &ducked_events, 64));

        assert!(
            ducked < reference * 0.5,
            "a 0.25 volume expression must attenuate the voice: {ducked} vs {reference}"
        );
    }

    #[test]
    fn a_tuning_expression_bends_the_addressed_voice() {
        let mut plugin = PolyExprSynth::default();
        plugin.initialize(48_000.0, 64);

        let mut events = EventQueue::new();
        events.push(Event::Midi(MidiMessage::note_on(0, 0, 69, 100)));
        events.push(expression(NoteExpressionKind::Tuning, 69, 12.0));
        render(&mut plugin, &events, 8);

        let voice = plugin
            .voices
            .iter()
            .find(|voice| voice.active)
            .expect("the note must be sounding");
        assert_eq!(voice.tuning, 12.0);
        // One octave up from A440.
        assert!((note_frequency_with_tuning(voice.note, voice.tuning) - 880.0).abs() < 1e-6);
    }

    #[test]
    fn an_expression_for_another_key_leaves_the_voice_untouched() {
        let mut plugin = PolyExprSynth::default();
        plugin.initialize(48_000.0, 64);

        let mut events = EventQueue::new();
        events.push(Event::Midi(MidiMessage::note_on(0, 0, 69, 100)));
        events.push(expression(NoteExpressionKind::Tuning, 60, 12.0));
        render(&mut plugin, &events, 8);

        let voice = plugin
            .voices
            .iter()
            .find(|voice| voice.active)
            .expect("the note must be sounding");
        assert_eq!(voice.tuning, 0.0, "expression addressed a different key");
    }

    #[test]
    fn an_unnamed_expression_dimension_is_accepted_without_effect() {
        let mut plugin = PolyExprSynth::default();
        plugin.initialize(48_000.0, 64);

        // The event must not be dropped at the queue level and must not
        // corrupt the voice either.
        let mut events = EventQueue::new();
        events.push(Event::Midi(MidiMessage::note_on(0, 0, 69, 100)));
        events.push(expression(NoteExpressionKind::Unknown(4242), 69, 1.0));
        assert_eq!(events.note_expressions().count(), 1);
        render(&mut plugin, &events, 8);

        let voice = plugin.voices.iter().find(|voice| voice.active).unwrap();
        assert_eq!(voice.tuning, 0.0);
        assert_eq!(voice.expression_gain, 1.0);
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
