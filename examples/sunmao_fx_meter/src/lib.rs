//! SunMao Meter — Phase 3 acceptance fixture.
//!
//! An audio passthrough that measures per-channel peak and RMS with the
//! `sunmao/dsp` metering component and publishes both where a GUI thread can
//! read them.
//!
//! The publication path is the point of the fixture. The audio callback writes
//! and the editor reads, on different threads at unrelated rates; a lock
//! between them would let drawing stall the audio thread. `MeterHandle` is
//! backed by atomics instead, so a reader can never block a writer.

use sunmao::prelude::*;
use sunmao_dsp::metering::{Meter, MeterHandle};
use sunmao_dsp::mixing::db_to_gain;

/// Meter parameters.
#[derive(Params)]
pub struct MeterParams {
    /// Output gain in decibels, applied after measurement so the meter always
    /// reads the input rather than its own output.
    pub gain_db: FloatParam,
}

impl Default for MeterParams {
    fn default() -> Self {
        Self {
            gain_db: FloatParam::new("gain_db", "Gain", 0.0, -60.0, 12.0),
        }
    }
}

/// Number of metered channels. Fixed so the handles exist before the host
/// activates the plugin — an editor may open before audio ever runs.
const CHANNELS: usize = 2;

/// The metering passthrough plugin.
pub struct MeterPlugin {
    params: Arc<MeterParams>,
    /// Written by the audio thread only.
    meters: Vec<Meter>,
    /// Cloned out to readers; the same underlying atomics as `meters`.
    handles: Vec<MeterHandle>,
}

impl Default for MeterPlugin {
    fn default() -> Self {
        let meters: Vec<Meter> = (0..CHANNELS).map(|_| Meter::new()).collect();
        let handles = meters.iter().map(|meter| meter.handle()).collect();
        Self {
            params: Arc::new(MeterParams::default()),
            meters,
            handles,
        }
    }
}

impl MeterPlugin {
    /// Reader handle for a GUI or test thread. `None` for a channel the plugin
    /// does not meter.
    pub fn meter(&self, channel: usize) -> Option<MeterHandle> {
        self.handles.get(channel).cloned()
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

    fn initialize(&mut self, sample_rate: f64, _max_block_size: u32) {
        // The ballistics depend on the rate; leaving them at the default would
        // make the meter fall four times too slowly at 192 kHz.
        for meter in self.meters.iter_mut() {
            meter.set_sample_rate(sample_rate);
        }
    }

    fn reset(&mut self) {
        for meter in self.meters.iter_mut() {
            meter.reset();
        }
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        buffer.copy_input_to_output();

        // Parameter changes apply at block rate (last one wins).
        let mut gain_db = self.params.gain_db.get();
        for change in events.param_changes() {
            if change.id == self.params.gain_db.id {
                let value = change.value.clamp(0.0, 1.0);
                gain_db = self.params.gain_db.min
                    + value * (self.params.gain_db.max - self.params.gain_db.min);
            }
        }
        let gain = db_to_gain(gain_db);

        // Measure the input first, then apply the gain: a meter that read its
        // own output would show the fader position rather than the signal.
        let channels = buffer.num_output_channels();
        for channel in 0..channels {
            let samples = buffer.output(channel);
            if let Some(meter) = self.meters.get_mut(channel) {
                meter.process_block(samples);
            }
            sunmao_dsp::mixing::apply_gain(samples, gain);
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

    fn make_plugin() -> MeterPlugin {
        let mut plugin = MeterPlugin::default();
        plugin.initialize(48_000.0, 512);
        plugin
    }

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

    /// Feeds the same block repeatedly, so the RMS integrator reaches level.
    ///
    /// The meter has ballistics now rather than reporting per-block statistics:
    /// RMS is a running average with a 100 ms time constant, so a single
    /// 64-sample block reads far below the signal's true level. That is the
    /// component behaving correctly, not a lag to be tuned away — an
    /// instantaneous RMS would be unreadable on screen.
    fn settle(plugin: &mut MeterPlugin, block: &[f32], seconds: f64) {
        let blocks = (seconds * 48_000.0 / block.len() as f64) as usize;
        for _ in 0..blocks {
            process_block(plugin, block);
        }
    }

    fn left_meter(plugin: &MeterPlugin) -> MeterHandle {
        plugin.meter(0).expect("channel 0 is metered")
    }

    #[test]
    fn unity_gain_is_an_exact_passthrough() {
        // 0 dB must be exactly 1.0, not 0.9999: a metering plugin that alters
        // the signal it measures is worse than useless.
        let mut plugin = make_plugin();
        let input: Vec<f32> = (0..128).map(|i| (i as f32 / 16.0).sin() * 0.5).collect();
        let output = process_block(&mut plugin, &input);
        assert_eq!(output, input);
    }

    #[test]
    fn the_peak_meter_catches_a_single_loud_sample() {
        // Peak has no attack time — a one-sample transient must register in
        // the block it arrives in.
        let mut plugin = make_plugin();
        let mut input = vec![0.1f32; 64];
        input[17] = -0.75;
        process_block(&mut plugin, &input);

        // The transient is 46 samples before the end of the block, and the peak
        // decays at -20 dB/s from the moment it lands — about 0.2 % over that
        // distance. The reading must be the transient minus that decay, not
        // the 0.1 background it sits in.
        let peak = left_meter(&plugin).peak();
        assert!(
            (peak - 0.75).abs() < 1e-2,
            "peak must track the largest |sample|, got {peak}"
        );
    }

    #[test]
    fn the_rms_meter_settles_on_the_level_of_a_full_scale_square() {
        let mut plugin = make_plugin();
        let block: Vec<f32> = (0..64)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        settle(&mut plugin, &block, 0.5);

        let rms = left_meter(&plugin).rms();
        assert!(
            (rms - 1.0).abs() < 0.01,
            "a full-scale square has RMS 1.0, got {rms}"
        );
    }

    #[test]
    fn the_rms_meter_settles_below_peak_for_a_sine() {
        let mut plugin = make_plugin();
        let block: Vec<f32> = (0..512).map(|i| (i as f32 * 0.05).sin()).collect();
        settle(&mut plugin, &block, 0.5);

        let handle = left_meter(&plugin);
        // A sine's RMS is its amplitude over root two.
        let expected = 1.0 / std::f32::consts::SQRT_2;
        assert!(
            (handle.rms() - expected).abs() < 0.05,
            "sine RMS was {}, expected {expected}",
            handle.rms()
        );
        assert!(handle.rms() < handle.peak());
    }

    #[test]
    fn the_meter_measures_the_input_not_the_gained_output() {
        let mut plugin = make_plugin();
        plugin.params.gain_db.set(-60.0);

        let input = vec![0.5f32; 64];
        let output = process_block(&mut plugin, &input);

        let peak = output.iter().fold(0.0f32, |acc, s| acc.max(s.abs()));
        assert!(peak < 1e-3, "-60 dB should be near silence, peak {peak}");
        assert!(
            (left_meter(&plugin).peak() - 0.5).abs() < 1e-3,
            "the meter reads pre-gain, got {}",
            left_meter(&plugin).peak()
        );
    }

    #[test]
    fn the_gain_parameter_is_in_decibels() {
        let mut plugin = make_plugin();
        plugin.params.gain_db.set(-6.0);
        let output = process_block(&mut plugin, &vec![1.0f32; 64]);
        // -6 dB is a little over half amplitude, not a gain of -6.
        assert!(
            (output[0] - 0.501_187).abs() < 1e-3,
            "-6 dB should halve the amplitude, got {}",
            output[0]
        );
    }

    #[test]
    fn each_channel_is_metered_independently() {
        // One meter shared across channels would let a loud right channel show
        // up on the left's display.
        let mut plugin = make_plugin();
        let loud = vec![0.9f32; 64];
        let inputs: [&[f32]; 2] = [&[0.0; 64], &loud];
        let mut left = vec![0.0; 64];
        let mut right = vec![0.0; 64];
        let mut outputs: [&mut [f32]; 2] = [&mut left, &mut right];
        let mut buffer = AudioBuffer::new(&inputs, &mut outputs, 64);
        plugin.process(&mut buffer, &EventQueue::new(), &ProcessContext::default());

        assert!(
            left_meter(&plugin).peak() < 1e-6,
            "silence on the left must read as silence"
        );
        let right_peak = plugin.meter(1).expect("channel 1 is metered").peak();
        assert!(
            (right_peak - 0.9).abs() < 1e-3,
            "right channel peak was {right_peak}"
        );
    }

    #[test]
    fn readers_on_another_thread_see_published_values() {
        let mut plugin = make_plugin();
        let handle = left_meter(&plugin);

        settle(&mut plugin, &vec![0.25f32; 64], 0.5);

        let reader = std::thread::spawn(move || (handle.peak(), handle.rms()));
        let (peak, rms) = reader.join().expect("reader thread must not panic");
        assert!((peak - 0.25).abs() < 1e-3, "peak {peak}");
        assert!((rms - 0.25).abs() < 0.01, "rms {rms}");
    }

    #[test]
    fn a_handle_taken_before_activation_still_works() {
        // An editor can open before the host ever activates the plugin, so the
        // handles must exist from construction rather than from `initialize`.
        let plugin = MeterPlugin::default();
        let handle = plugin.meter(0).expect("handle available before activation");
        assert_eq!(handle.peak(), 0.0);

        let mut plugin = plugin;
        plugin.initialize(48_000.0, 512);
        process_block(&mut plugin, &vec![0.6f32; 64]);
        assert!(
            (handle.peak() - 0.6).abs() < 1e-3,
            "the early handle went stale: {}",
            handle.peak()
        );
    }

    #[test]
    fn reset_zeroes_the_meters() {
        let mut plugin = make_plugin();
        process_block(&mut plugin, &vec![0.9f32; 64]);
        plugin.reset();
        assert_eq!(left_meter(&plugin).peak(), 0.0);
        assert_eq!(left_meter(&plugin).rms(), 0.0);
    }
}

// ============ Unified VST3 + CLAP Export ============
sunmao::sunmao_export!(MeterPlugin);
