//! SunMao Grouped Params Synth — Phase 3 acceptance fixture.
//!
//! M0 skeleton: a monophonic sine synth whose parameters are laid out in the
//! sections a host would show as groups (oscillator, filter, amplifier), but
//! still declared flat because the Phase 2 contract has no group model. M2
//! replaces the flat layout with host-visible parameter groups (VST3
//! `IUnitInfo` ↔ CLAP module paths), moves the block-rate parameter handling
//! onto zero-allocation smoothers, and rebuilds the plugin body on the
//! instrument template.

use sunmao::prelude::*;

/// Grouped synth parameters. The `osc_`/`filter_`/`amp_` prefixes mark the
/// future M2 groups; until then hosts see a flat list.
#[derive(Params)]
pub struct GroupedSynthParams {
    /// Oscillator section: output level.
    #[group = "Osc"]
    pub osc_level: FloatParam,
    /// Oscillator section: detune in semitones. Nested one level deeper to
    /// exercise hierarchy rather than a flat set of sections.
    #[group = "Osc/Tuning"]
    pub osc_detune: FloatParam,
    /// Filter section: one-pole lowpass cutoff in Hz.
    #[group = "Filter"]
    pub filter_cutoff: FloatParam,
    /// Amplifier section: attack time in milliseconds.
    #[group = "Amp/Envelope"]
    pub amp_attack_ms: FloatParam,
    /// Amplifier section: release time in milliseconds.
    #[group = "Amp/Envelope"]
    pub amp_release_ms: FloatParam,
}

impl Default for GroupedSynthParams {
    fn default() -> Self {
        Self {
            osc_level: FloatParam::new("osc_level", "Osc Level", 0.8, 0.0, 1.0),
            osc_detune: FloatParam::new("osc_detune", "Osc Detune", 0.0, -12.0, 12.0),
            filter_cutoff: FloatParam::new("filter_cutoff", "Cutoff", 8000.0, 20.0, 20000.0),
            amp_attack_ms: FloatParam::new("amp_attack_ms", "Attack", 5.0, 0.1, 500.0),
            amp_release_ms: FloatParam::new("amp_release_ms", "Release", 50.0, 1.0, 2000.0),
        }
    }
}

/// Amplifier envelope stage.
#[derive(Clone, Copy, PartialEq, Eq)]
enum EnvStage {
    Idle,
    Attack,
    Sustain,
    Release,
}

/// The grouped-parameter monophonic synth.
pub struct GroupedSynth {
    params: Arc<GroupedSynthParams>,
    sample_rate: f64,
    note: u8,
    phase: f64,
    stage: EnvStage,
    env: f32,
    /// One-pole lowpass state, the filter section's placeholder DSP.
    lp_state: f32,
}

impl Default for GroupedSynth {
    fn default() -> Self {
        Self {
            params: Arc::new(GroupedSynthParams::default()),
            sample_rate: 44_100.0,
            note: 0,
            phase: 0.0,
            stage: EnvStage::Idle,
            env: 0.0,
            lp_state: 0.0,
        }
    }
}

fn note_frequency(note: u8, detune_semitones: f64) -> f64 {
    440.0 * 2.0_f64.powf((f64::from(note) - 69.0 + detune_semitones) / 12.0)
}

impl SunmaoPlugin for GroupedSynth {
    const NAME: &'static str = "SunMao Grouped Params Synth";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";

    type Params = GroupedSynthParams;

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
        self.phase = 0.0;
        self.stage = EnvStage::Idle;
        self.env = 0.0;
        self.lp_state = 0.0;
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        // Skeleton semantics: notes and parameters apply at block rate. M2
        // replaces this with per-sample smoothing.
        for message in events.midi_events() {
            if message.is_note_on() {
                self.note = message.note();
                self.stage = EnvStage::Attack;
            } else if message.is_note_off() && message.note() == self.note {
                self.stage = EnvStage::Release;
            }
        }

        let level = self.params.osc_level.get();
        let detune = f64::from(self.params.osc_detune.get());
        let cutoff = f64::from(self.params.filter_cutoff.get());
        let attack_step =
            (1000.0 / (f64::from(self.params.amp_attack_ms.get()) * self.sample_rate)) as f32;
        let release_step =
            (1000.0 / (f64::from(self.params.amp_release_ms.get()) * self.sample_rate)) as f32;
        // One-pole coefficient; the exact response is irrelevant to the
        // fixture, only that the cutoff parameter audibly acts on the tone.
        let lp_coeff = (1.0 - (-std::f64::consts::TAU * cutoff / self.sample_rate).exp())
            .clamp(0.0, 1.0) as f32;
        let increment = note_frequency(self.note, detune) / self.sample_rate;

        let channels = buffer.num_output_channels();
        for sample_index in 0..buffer.num_samples() {
            match self.stage {
                EnvStage::Attack => {
                    self.env += attack_step;
                    if self.env >= 1.0 {
                        self.env = 1.0;
                        self.stage = EnvStage::Sustain;
                    }
                }
                EnvStage::Release => {
                    self.env -= release_step;
                    if self.env <= 0.0 {
                        self.env = 0.0;
                        self.stage = EnvStage::Idle;
                    }
                }
                EnvStage::Sustain | EnvStage::Idle => {}
            }

            let raw = if self.stage == EnvStage::Idle {
                0.0
            } else {
                (self.phase * std::f64::consts::TAU).sin() as f32 * self.env * level
            };
            self.phase = (self.phase + increment).fract();
            self.lp_state += lp_coeff * (raw - self.lp_state);

            for channel in 0..channels {
                buffer.output(channel)[sample_index] = self.lp_state;
            }
        }

        ProcessStatus::Normal
    }

    fn vst3_info() -> Vst3Info {
        Vst3Info {
            class_id: *b"SunMaoSynGrpPrm!",
            categories: &["Instrument", "Synth"],
            ..Default::default()
        }
    }

    fn clap_info() -> ClapInfo {
        ClapInfo {
            id: "com.sunmao.synth.grouped_params",
            features: &["instrument", "synthesizer", "stereo"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(plugin: &mut GroupedSynth, events: &EventQueue, samples: usize) -> Vec<f32> {
        let inputs: [&[f32]; 0] = [];
        let mut left = vec![0.0; samples];
        let mut right = vec![0.0; samples];
        let num_samples = samples;
        let mut outputs: [&mut [f32]; 2] = [&mut left, &mut right];
        let mut buffer = AudioBuffer::new(&inputs, &mut outputs, num_samples);
        let status = plugin.process(&mut buffer, events, &ProcessContext::default());
        assert_eq!(status, ProcessStatus::Normal);
        left
    }

    fn note_on() -> EventQueue {
        let mut events = EventQueue::new();
        events.push(Event::Midi(MidiMessage::note_on(0, 0, 69, 100)));
        events
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn the_parameter_layout_is_valid() {
        let params = GroupedSynthParams::default();
        let descriptors = params.descriptors();
        assert_eq!(descriptors.len(), 5);
        validate_param_layout(&params, &descriptors).expect("layout must validate");
    }

    #[test]
    fn a_note_rises_with_the_attack_and_dies_after_release() {
        let mut plugin = GroupedSynth::default();
        plugin.initialize(48_000.0, 64);
        plugin.params.amp_attack_ms.set(1.0);
        plugin.params.amp_release_ms.set(1.0);

        let sounding = render(&mut plugin, &note_on(), 512);
        assert!(
            sounding.iter().any(|s| s.abs() > 1e-3),
            "a held note must produce audio"
        );

        let mut off = EventQueue::new();
        off.push(Event::Midi(MidiMessage::note_off(0, 0, 69, 0)));
        render(&mut plugin, &off, 512);

        // After the release has fully decayed only the filter memory remains,
        // which itself decays toward zero.
        let silent = render(&mut plugin, &EventQueue::new(), 512);
        assert!(
            rms(&silent[256..]) < 1e-4,
            "a released note must decay to silence, rms {}",
            rms(&silent[256..])
        );
    }

    #[test]
    fn the_filter_cutoff_shapes_the_tone() {
        let mut bright = GroupedSynth::default();
        bright.initialize(48_000.0, 64);
        bright.params.amp_attack_ms.set(0.1);
        bright.params.filter_cutoff.set(20_000.0);
        let open = rms(&render(&mut bright, &note_on(), 4096)[2048..]);

        let mut dark = GroupedSynth::default();
        dark.initialize(48_000.0, 64);
        dark.params.amp_attack_ms.set(0.1);
        dark.params.filter_cutoff.set(60.0);
        let closed = rms(&render(&mut dark, &note_on(), 4096)[2048..]);

        assert!(
            closed < open * 0.5,
            "a 60 Hz cutoff must attenuate a 440 Hz tone: closed {closed} vs open {open}"
        );
    }

    #[test]
    fn detune_shifts_the_oscillator_frequency() {
        assert!((note_frequency(69, 0.0) - 440.0).abs() < 1e-9);
        assert!((note_frequency(69, 12.0) - 880.0).abs() < 1e-6);
    }

    #[test]
    fn every_parameter_declares_its_host_visible_group() {
        let params = GroupedSynthParams::default();
        let descriptors = params.descriptors();
        let group_of = |id: &str| {
            descriptors
                .iter()
                .find(|descriptor| descriptor.id == id)
                .map(|descriptor| descriptor.group)
                .expect("declared parameter")
        };

        assert_eq!(group_of("osc_level"), "Osc");
        // Nesting is expressed as a path, which is what both formats consume.
        assert_eq!(group_of("osc_detune"), "Osc/Tuning");
        assert_eq!(group_of("filter_cutoff"), "Filter");
        assert_eq!(group_of("amp_attack_ms"), "Amp/Envelope");
        assert_eq!(group_of("amp_release_ms"), "Amp/Envelope");
    }

    #[test]
    fn the_declared_groups_form_one_coherent_tree() {
        use sunmao::params::group_segments;
        let params = GroupedSynthParams::default();
        // Two parameters sharing a group must land in the same place, and every
        // segment must be non-empty or a host would show an unnamed level.
        for descriptor in params.descriptors() {
            for segment in group_segments(descriptor.group) {
                assert!(!segment.trim().is_empty(), "{}", descriptor.id);
            }
        }
        let attack = group_segments("Amp/Envelope").collect::<Vec<_>>();
        assert_eq!(attack, vec!["Amp", "Envelope"]);
    }
}

// ============ Unified VST3 + CLAP Export ============
sunmao::sunmao_export!(GroupedSynth);
