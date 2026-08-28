//! SunMao Oversampled Distortion — Phase 3 acceptance fixture.
//!
//! M0 skeleton: a stereo `tanh` waveshaper with drive and dry/wet mix, running
//! at the host rate with zero latency. M4 wraps the shaper in the
//! `sunmao/dsp` 2x/4x oversampler; the fixture then reports the oversampler's
//! group delay through the Phase 2 latency contract and the unittest runner
//! asserts the reported value against the measured impulse delay.

use sunmao::prelude::*;

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

/// The oversampled distortion plugin. The M0 skeleton is stateless; the M4
/// oversampler adds per-channel resampler state and real latency.
pub struct OsDistPlugin {
    params: Arc<OsDistParams>,
}

impl Default for OsDistPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(OsDistParams::default()),
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

    fn latency_samples(&self) -> u32 {
        // M0 runs at the host rate with no lookahead. M4's oversampler
        // replaces this with the resampling filters' group delay, which is
        // what the runner's latency assertion locks down.
        0
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

        let channels = buffer.num_output_channels();
        for channel in 0..channels {
            let samples = buffer.output(channel);
            for sample in samples.iter_mut() {
                let dry = *sample;
                let wet = shape(dry, drive) * trim;
                *sample = dry * (1.0 - mix) + wet * mix;
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

    fn sine(len: usize, cycles_per_len: f32, amplitude: f32) -> Vec<f32> {
        (0..len)
            .map(|i| {
                (i as f32 / len as f32 * cycles_per_len * std::f32::consts::TAU).sin() * amplitude
            })
            .collect()
    }

    #[test]
    fn the_skeleton_reports_zero_latency() {
        // M4 flips this to the oversampler's group delay; until then the
        // fixture pins the Phase 2 latency contract at zero.
        let plugin = OsDistPlugin::default();
        assert_eq!(plugin.latency_samples(), 0);
    }

    #[test]
    fn a_fully_dry_mix_is_an_exact_passthrough() {
        let mut plugin = OsDistPlugin::default();
        plugin.initialize(48_000.0, 64);
        plugin.params.mix.set(0.0);
        plugin.params.drive.set(20.0);

        let input = sine(256, 4.0, 0.9);
        let output = process_block(&mut plugin, &input);
        assert_eq!(output, input, "mix 0 must not touch the signal");
    }

    #[test]
    fn unity_drive_is_nearly_transparent_for_small_signals() {
        let mut plugin = OsDistPlugin::default();
        plugin.initialize(48_000.0, 64);
        plugin.params.drive.set(1.0);

        let input = sine(256, 4.0, 0.01);
        let output = process_block(&mut plugin, &input);
        for (o, i) in output.iter().zip(&input) {
            assert!(
                (o - i).abs() < 5e-3,
                "normalized tanh at unity drive must stay close to identity: {o} vs {i}"
            );
        }
    }

    #[test]
    fn high_drive_clamps_the_output_below_the_saturation_ceiling() {
        let mut plugin = OsDistPlugin::default();
        plugin.initialize(48_000.0, 64);
        plugin.params.drive.set(20.0);

        let input = sine(512, 4.0, 1.5);
        let output = process_block(&mut plugin, &input);
        let peak = output.iter().fold(0.0f32, |acc, s| acc.max(s.abs()));
        assert!(
            peak <= 1.0 + 1e-6,
            "tanh shaping must bound the output at ±1, peak {peak}"
        );
        assert!(peak > 0.9, "hot input must actually reach saturation");
    }

    #[test]
    fn the_trim_scales_the_wet_path_only() {
        let mut plugin = OsDistPlugin::default();
        plugin.initialize(48_000.0, 64);
        plugin.params.trim.set(0.5);
        plugin.params.mix.set(1.0);
        plugin.params.drive.set(1.0);

        let input = sine(256, 4.0, 0.01);
        let output = process_block(&mut plugin, &input);
        for (o, i) in output.iter().zip(&input) {
            assert!(
                (o - i * 0.5).abs() < 5e-3,
                "trim must halve the (near-linear) wet signal: {o} vs {i}"
            );
        }
    }

    #[test]
    fn the_shaper_never_emits_non_finite_samples() {
        let mut plugin = OsDistPlugin::default();
        plugin.initialize(48_000.0, 64);
        plugin.params.drive.set(20.0);

        let extreme: Vec<f32> = [f32::MAX, -f32::MAX, 1e30, -1e30, 0.0]
            .iter()
            .copied()
            .cycle()
            .take(64)
            .collect();
        let output = process_block(&mut plugin, &extreme);
        assert!(
            output.iter().all(|s| s.is_finite()),
            "the shaper must saturate, not overflow: {output:?}"
        );
    }
}

// ============ Unified VST3 + CLAP Export ============
sunmao::sunmao_export!(OsDistPlugin);
