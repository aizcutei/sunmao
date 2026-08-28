//! SunMao Meter — Phase 3 acceptance fixture.
//!
//! M0 skeleton: an audio passthrough that measures per-block peak and RMS and
//! publishes both through lock-free atomics a GUI thread can read. M4
//! replaces the hand-rolled atomics with the `sunmao/dsp` metering component
//! (ballistics, configurable windows) while keeping the publish path
//! wait-free and the audio path allocation-free.

use std::sync::atomic::{AtomicU32, Ordering};

use sunmao::prelude::*;

/// Lock-free meter values shared between the audio thread (writer) and any
/// reader thread. Values are `f32` bits stored in atomics, so both sides stay
/// wait-free.
#[derive(Default)]
pub struct MeterState {
    peak_bits: AtomicU32,
    rms_bits: AtomicU32,
}

impl MeterState {
    /// Publishes one block's measurements. Called from the audio thread.
    pub fn publish(&self, peak: f32, rms: f32) {
        self.peak_bits.store(peak.to_bits(), Ordering::Release);
        self.rms_bits.store(rms.to_bits(), Ordering::Release);
    }

    /// Latest peak in linear amplitude. Safe to call from any thread.
    pub fn peak(&self) -> f32 {
        f32::from_bits(self.peak_bits.load(Ordering::Acquire))
    }

    /// Latest RMS in linear amplitude. Safe to call from any thread.
    pub fn rms(&self) -> f32 {
        f32::from_bits(self.rms_bits.load(Ordering::Acquire))
    }
}

/// Meter parameters.
#[derive(Params)]
pub struct MeterParams {
    /// Output gain applied after measurement, so the meter reads the input.
    pub gain: FloatParam,
}

impl Default for MeterParams {
    fn default() -> Self {
        Self {
            gain: FloatParam::new("gain", "Gain", 1.0, 0.0, 2.0),
        }
    }
}

/// The metering passthrough plugin.
pub struct MeterPlugin {
    params: Arc<MeterParams>,
    meters: Arc<MeterState>,
}

impl Default for MeterPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(MeterParams::default()),
            meters: Arc::new(MeterState::default()),
        }
    }
}

impl MeterPlugin {
    /// Reader handle for GUI/test threads.
    pub fn meters(&self) -> Arc<MeterState> {
        self.meters.clone()
    }
}

impl SunmaoPlugin for MeterPlugin {
    const NAME: &'static str = "SunMao Meter";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";

    type Params = MeterParams;

    fn params(&self) -> Arc<Self::Params> {
        self.params.clone()
    }

    fn reset(&mut self) {
        self.meters.publish(0.0, 0.0);
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        buffer.copy_input_to_output();

        // Skeleton semantics: parameter changes apply at block rate.
        let mut gain = self.params.gain.get();
        for change in events.param_changes() {
            if change.id == self.params.gain.id {
                let value = change.value.clamp(0.0, 1.0);
                gain = self.params.gain.min + value * (self.params.gain.max - self.params.gain.min);
            }
        }

        // Measure the input across all channels before the gain touches it.
        let channels = buffer.num_output_channels();
        let num_samples = buffer.num_samples();
        let mut peak = 0.0f32;
        let mut sum_squares = 0.0f64;
        for channel in 0..channels {
            for &sample in buffer.output(channel).iter() {
                peak = peak.max(sample.abs());
                sum_squares += f64::from(sample) * f64::from(sample);
            }
        }
        let rms = if channels > 0 && num_samples > 0 {
            (sum_squares / (channels * num_samples) as f64).sqrt() as f32
        } else {
            0.0
        };
        self.meters.publish(peak, rms);

        for channel in 0..channels {
            for sample in buffer.output(channel).iter_mut() {
                *sample *= gain;
            }
        }

        ProcessStatus::Normal
    }

    fn vst3_info() -> Vst3Info {
        Vst3Info {
            class_id: *b"SunMaoFxMeter!!!",
            categories: &["Fx", "Analyzer"],
            ..Default::default()
        }
    }

    fn clap_info() -> ClapInfo {
        ClapInfo {
            id: "com.sunmao.fx.meter",
            features: &["audio-effect", "analyzer", "stereo"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process_block(plugin: &mut MeterPlugin, input: &[f32]) -> Vec<f32> {
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

    #[test]
    fn unity_gain_is_an_exact_passthrough() {
        let mut plugin = MeterPlugin::default();
        plugin.initialize(48_000.0, 64);
        let input: Vec<f32> = (0..128).map(|i| (i as f32 / 16.0).sin() * 0.5).collect();
        let output = process_block(&mut plugin, &input);
        assert_eq!(output, input);
    }

    #[test]
    fn the_peak_meter_reports_the_largest_absolute_sample() {
        let mut plugin = MeterPlugin::default();
        plugin.initialize(48_000.0, 64);

        let mut input = vec![0.1f32; 64];
        input[17] = -0.75;
        process_block(&mut plugin, &input);

        assert!(
            (plugin.meters().peak() - 0.75).abs() < 1e-6,
            "peak must track the largest |sample|, got {}",
            plugin.meters().peak()
        );
    }

    #[test]
    fn the_rms_meter_matches_a_full_scale_square_wave() {
        let mut plugin = MeterPlugin::default();
        plugin.initialize(48_000.0, 64);

        let input: Vec<f32> = (0..64)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        process_block(&mut plugin, &input);

        assert!(
            (plugin.meters().rms() - 1.0).abs() < 1e-6,
            "a full-scale square has RMS 1.0, got {}",
            plugin.meters().rms()
        );
    }

    #[test]
    fn the_meter_measures_the_input_not_the_gained_output() {
        let mut plugin = MeterPlugin::default();
        plugin.initialize(48_000.0, 64);
        plugin.params.gain.set(0.0);

        let input = vec![0.5f32; 64];
        let output = process_block(&mut plugin, &input);

        assert!(
            output.iter().all(|s| *s == 0.0),
            "gain 0 silences the output"
        );
        assert!(
            (plugin.meters().peak() - 0.5).abs() < 1e-6,
            "the meter reads pre-gain, got {}",
            plugin.meters().peak()
        );
    }

    #[test]
    fn readers_on_another_thread_see_published_values() {
        let mut plugin = MeterPlugin::default();
        plugin.initialize(48_000.0, 64);
        let meters = plugin.meters();

        process_block(&mut plugin, &vec![0.25f32; 64]);

        let handle = std::thread::spawn(move || (meters.peak(), meters.rms()));
        let (peak, rms) = handle.join().expect("reader thread must not panic");
        assert!((peak - 0.25).abs() < 1e-6);
        assert!((rms - 0.25).abs() < 1e-6);
    }

    #[test]
    fn reset_zeroes_the_meters() {
        let mut plugin = MeterPlugin::default();
        plugin.initialize(48_000.0, 64);
        process_block(&mut plugin, &vec![0.9f32; 64]);
        plugin.reset();
        assert_eq!(plugin.meters().peak(), 0.0);
        assert_eq!(plugin.meters().rms(), 0.0);
    }
}

// ============ Unified VST3 + CLAP Export ============
sunmao::sunmao_export!(MeterPlugin);
