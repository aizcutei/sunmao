//! SunMao Oversampled Distortion — Phase 3 acceptance fixture.
//!
//! A stereo `tanh` waveshaper running inside the `sunmao/dsp` 4x oversampler.
//! The waveshaper is the reason the oversampling is here: `tanh` generates
//! harmonics without limit, and at the host rate everything above Nyquist folds
//! back down as inharmonic aliasing.
//!
//! The oversampler's filters impose a fixed group delay, which the fixture
//! reports through the Phase 2 latency contract — this is what the unittest
//! runner asserts against, on both VST3 and CLAP.

use sunmao::prelude::*;
use sunmao_dsp::mixing::{DryWet, MixLaw};
use sunmao_dsp::oversampling::{Oversampler, OversamplingFactor};

/// 4x rather than 2x: `tanh` at high drive is a hard enough non-linearity that
/// 2x still leaves audible folding on high input frequencies.
const FACTOR: OversamplingFactor = OversamplingFactor::X4;

/// Falls back to this if a host activates without declaring a block size.
const DEFAULT_MAX_BLOCK: usize = 4096;

/// Distortion parameters.
#[derive(Params)]
pub struct OsDistParams {
    /// Input drive in linear gain applied before the shaper.
    pub drive: FloatParam,
    /// Output trim in linear gain applied after the shaper.
    pub trim: FloatParam,
    /// Dry/wet mix.
    pub mix: FloatParam,
}

impl Default for OsDistParams {
    fn default() -> Self {
        Self {
            drive: FloatParam::new("drive", "Drive", 1.0, 0.1, 20.0),
            trim: FloatParam::new("trim", "Trim", 1.0, 0.0, 2.0),
            mix: FloatParam::new("mix", "Mix", 1.0, 0.0, 1.0),
        }
    }
}

/// The oversampled distortion plugin.
pub struct OsDistPlugin {
    params: Arc<OsDistParams>,
    /// One oversampler per channel: the filters are stateful, and sharing one
    /// across channels would leak each channel's history into the next.
    oversamplers: Vec<Oversampler>,
    mixer: DryWet,
}

impl Default for OsDistPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(OsDistParams::default()),
            oversamplers: Vec::new(),
            mixer: DryWet::new(MixLaw::Linear, 1.0),
        }
    }
}

/// The waveshaper transfer curve, normalized so unity drive stays close to
/// identity for small signals: `tanh(drive * x) / tanh(drive)`.
fn shape(sample: f32, drive: f32) -> f32 {
    let denominator = drive.tanh();
    if denominator.abs() < f32::EPSILON {
        sample
    } else {
        (sample * drive).tanh() / denominator
    }
}

impl SunmaoPlugin for OsDistPlugin {
    const NAME: &'static str = "SunMao OS Distortion";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";

    type Params = OsDistParams;

    fn params(&self) -> Arc<Self::Params> {
        self.params.clone()
    }

    fn initialize(&mut self, _sample_rate: f64, max_block_size: u32) {
        // Allocate here, so `process` never has to. Two channels covers the
        // stereo bus this fixture declares; `process` tolerates more.
        let max_block = if max_block_size == 0 {
            DEFAULT_MAX_BLOCK
        } else {
            max_block_size as usize
        };
        self.oversamplers = (0..2)
            .map(|_| {
                let mut oversampler = Oversampler::new();
                oversampler.prepare(FACTOR, max_block);
                oversampler
            })
            .collect();
    }

    fn reset(&mut self) {
        for oversampler in self.oversamplers.iter_mut() {
            oversampler.reset();
        }
    }

    fn latency_samples(&self) -> u32 {
        // Answered from the factor rather than from a prepared oversampler:
        // a host may ask before it activates the plugin, and reporting zero
        // then would leave the track misaligned for the whole session.
        FACTOR.latency_samples()
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        buffer.copy_input_to_output();

        // Skeleton semantics: parameter changes apply at block rate (last one
        // wins); M2's smoothers take over from there.
        let mut drive = self.params.drive.get();
        let mut trim = self.params.trim.get();
        let mut mix = self.params.mix.get();
        for change in events.param_changes() {
            let value = change.value.clamp(0.0, 1.0);
            if change.id == self.params.drive.id {
                drive =
                    self.params.drive.min + value * (self.params.drive.max - self.params.drive.min);
            } else if change.id == self.params.trim.id {
                trim = self.params.trim.min + value * (self.params.trim.max - self.params.trim.min);
            } else if change.id == self.params.mix.id {
                mix = self.params.mix.min + value * (self.params.mix.max - self.params.mix.min);
            }
        }

        // The mix happens *inside* the oversampled callback, so the dry signal
        // travels the same filters and the same delay as the wet one. Mixing a
        // base-rate dry against a delayed wet would comb-filter the result —
        // the classic latency bug in an oversampled effect.
        self.mixer.set_amount(mix);
        let mixer = self.mixer;

        let channels = buffer.num_output_channels();
        for channel in 0..channels {
            let samples = buffer.output(channel);
            match self.oversamplers.get_mut(channel) {
                Some(oversampler) => oversampler.process(samples, |upsampled| {
                    for sample in upsampled.iter_mut() {
                        let dry = *sample;
                        *sample = mixer.mix(dry, shape(dry, drive) * trim);
                    }
                }),
                // More channels than prepared for: shape at the host rate
                // rather than allocate, and rather than drop the channel.
                None => {
                    for sample in samples.iter_mut() {
                        let dry = *sample;
                        *sample = mixer.mix(dry, shape(dry, drive) * trim);
                    }
                }
            }
        }

        ProcessStatus::Normal
    }

    fn vst3_info() -> Vst3Info {
        Vst3Info {
            class_id: *b"SunMaoFxOsDist!!",
            categories: &["Fx", "Distortion"],
            ..Default::default()
        }
    }

    fn clap_info() -> ClapInfo {
        ClapInfo {
            id: "com.sunmao.fx.os_dist",
            features: &["audio-effect", "distortion", "stereo"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Block size used everywhere below. It must not exceed what `initialize`
    /// prepared: an oversized block deliberately falls back to the host rate,
    /// which would quietly turn these into tests of nothing.
    const BLOCK: usize = 256;

    fn make_plugin() -> OsDistPlugin {
        let mut plugin = OsDistPlugin::default();
        plugin.initialize(48_000.0, BLOCK as u32);
        plugin
    }

    fn process_block(plugin: &mut OsDistPlugin, input: &[f32]) -> Vec<f32> {
        let input_right = input.to_vec();
        let inputs: [&[f32]; 2] = [input, &input_right];
        let mut left = vec![0.0; input.len()];
        let mut right = vec![0.0; input.len()];
        let num_samples = input.len();
        let mut outputs: [&mut [f32]; 2] = [&mut left, &mut right];
        let mut buffer = AudioBuffer::new(&inputs, &mut outputs, num_samples);
        let events = EventQueue::new();
        let status = plugin.process(&mut buffer, &events, &ProcessContext::default());
        assert_eq!(status, ProcessStatus::Normal);
        left
    }

    /// Runs a signal through in `BLOCK`-sized chunks and returns the whole
    /// output, so the filters' ramp-in is visible rather than hidden.
    fn process_signal(plugin: &mut OsDistPlugin, input: &[f32]) -> Vec<f32> {
        let mut output = Vec::with_capacity(input.len());
        for chunk in input.chunks(BLOCK) {
            output.extend_from_slice(&process_block(plugin, chunk));
        }
        output
    }

    fn sine(len: usize, cycles_per_len: f32, amplitude: f32) -> Vec<f32> {
        (0..len)
            .map(|i| {
                (i as f32 / len as f32 * cycles_per_len * std::f32::consts::TAU).sin() * amplitude
            })
            .collect()
    }

    /// Compares `output` against `expected` shifted by the reported latency.
    fn assert_matches_delayed(output: &[f32], expected: &[f32], tolerance: f32, what: &str) {
        let latency = FACTOR.latency_samples() as usize;
        // Skip a little more than the delay so the FIRs are fully primed.
        let skip = latency + 64;
        for index in skip..expected.len() {
            let produced = output[index];
            let reference = expected[index - latency];
            assert!(
                (produced - reference).abs() < tolerance,
                "{what}: sample {index} was {produced}, expected {reference}"
            );
        }
    }

    #[test]
    fn the_fixture_reports_the_oversamplers_group_delay() {
        // This is the number the host aligns the track with, and the value the
        // unittest runner asserts against over both VST3 and CLAP.
        let plugin = OsDistPlugin::default();
        assert_eq!(plugin.latency_samples(), FACTOR.latency_samples());
        assert!(
            plugin.latency_samples() > 0,
            "an oversampled effect that reports zero latency is misaligned"
        );
    }

    #[test]
    fn latency_is_reported_before_the_plugin_is_initialized() {
        // Hosts query latency before activating; answering zero here and
        // something else later is how a plugin ends up permanently offset.
        let uninitialized = OsDistPlugin::default();
        let mut initialized = OsDistPlugin::default();
        initialized.initialize(48_000.0, BLOCK as u32);
        assert_eq!(
            uninitialized.latency_samples(),
            initialized.latency_samples()
        );
    }

    #[test]
    fn a_fully_dry_mix_returns_the_signal_delayed_by_the_reported_latency() {
        // Not bit-exact any more: the dry path goes through the resampling
        // filters too, precisely so that it stays aligned with the wet one.
        let mut plugin = make_plugin();
        plugin.params.mix.set(0.0);
        plugin.params.drive.set(20.0);

        let input = sine(BLOCK * 4, 16.0, 0.9);
        let output = process_signal(&mut plugin, &input);
        assert_matches_delayed(&output, &input, 5e-3, "dry mix");
    }

    #[test]
    fn unity_drive_is_nearly_transparent_for_small_signals() {
        let mut plugin = make_plugin();
        plugin.params.drive.set(1.0);

        let input = sine(BLOCK * 4, 16.0, 0.01);
        let output = process_signal(&mut plugin, &input);
        assert_matches_delayed(&output, &input, 5e-3, "unity drive");
    }

    #[test]
    fn high_drive_clamps_the_output_below_the_saturation_ceiling() {
        let mut plugin = make_plugin();
        plugin.params.drive.set(20.0);

        let input = sine(BLOCK * 4, 16.0, 1.5);
        let output = process_signal(&mut plugin, &input);
        let peak = output.iter().fold(0.0f32, |acc, s| acc.max(s.abs()));
        assert!(
            peak <= 1.1,
            "tanh shaping must bound the output near ±1, peak {peak}"
        );
        assert!(peak > 0.9, "hot input must actually reach saturation");
    }

    #[test]
    fn the_trim_scales_the_wet_path_only() {
        let mut plugin = make_plugin();
        plugin.params.trim.set(0.5);
        plugin.params.mix.set(1.0);
        plugin.params.drive.set(1.0);

        let input = sine(BLOCK * 4, 16.0, 0.01);
        let halved: Vec<f32> = input.iter().map(|s| s * 0.5).collect();
        let output = process_signal(&mut plugin, &input);
        assert_matches_delayed(&output, &halved, 5e-3, "trim");
    }

    #[test]
    fn the_shaper_never_emits_non_finite_samples() {
        let mut plugin = make_plugin();
        plugin.params.drive.set(20.0);

        let extreme: Vec<f32> = [f32::MAX, -f32::MAX, 1e30, -1e30, 0.0]
            .iter()
            .copied()
            .cycle()
            .take(BLOCK)
            .collect();
        let output = process_block(&mut plugin, &extreme);
        assert!(
            output.iter().all(|s| s.is_finite()),
            "the shaper must saturate, not overflow: {output:?}"
        );
    }

    #[test]
    fn oversampling_beats_the_host_rate_on_a_high_frequency_input() {
        // The reason the fixture oversamples at all. A 7 kHz sine driven hard
        // at 48 kHz folds its harmonics down into the audible band; measure the
        // energy where only an alias could land.
        let sample_rate = 48_000.0f64;
        let frequency = 7_000.0f64;
        let length = BLOCK * 16;
        let input: Vec<f32> = (0..length)
            .map(|i| (i as f64 * frequency / sample_rate * std::f64::consts::TAU).sin() as f32)
            .collect();

        let mut oversampled = make_plugin();
        oversampled.params.drive.set(20.0);
        let with_os = process_signal(&mut oversampled, &input);

        // The same shaper applied straight at the host rate.
        let without_os: Vec<f32> = input.iter().map(|s| shape(*s, 20.0)).collect();

        let goertzel = |samples: &[f32], target: f64| -> f64 {
            let omega = std::f64::consts::TAU * target / sample_rate;
            let coefficient = 2.0 * omega.cos();
            let (mut s1, mut s2) = (0.0f64, 0.0f64);
            for sample in samples.iter().skip(BLOCK * 2) {
                let s0 = f64::from(*sample) + coefficient * s1 - s2;
                s2 = s1;
                s1 = s0;
            }
            (s1 * s1 + s2 * s2 - coefficient * s1 * s2).abs().sqrt()
        };

        // 5th harmonic of 7 kHz is 35 kHz, which folds to 13 kHz.
        let alias = 13_000.0;
        let with_alias = goertzel(&with_os, alias);
        let without_alias = goertzel(&without_os, alias);
        assert!(
            with_alias < without_alias * 0.5,
            "oversampling did not reduce the alias: {with_alias} vs {without_alias}"
        );
    }
}

// ============ Unified VST3 + CLAP Export ============
sunmao::sunmao_export!(OsDistPlugin);
