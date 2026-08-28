//! SunMao Sidechain Compressor — Phase 2 acceptance fixture.
//!
//! M0 skeleton: a feed-forward compressor keyed on its own input, built only
//! on the Phase 1 contract. M3 adds the sidechain input bus and switches the
//! detector to the external key signal.

use sunmao::prelude::*;

/// Compressor parameters.
#[derive(Params)]
pub struct SidechainCompParams {
    /// Threshold in dBFS.
    pub threshold_db: FloatParam,
    /// Compression ratio (n:1).
    pub ratio: FloatParam,
    /// Make-up gain in dB.
    pub makeup_db: FloatParam,
}

impl Default for SidechainCompParams {
    fn default() -> Self {
        Self {
            threshold_db: FloatParam::new("threshold_db", "Threshold", -24.0, -60.0, 0.0),
            ratio: FloatParam::new("ratio", "Ratio", 4.0, 1.0, 20.0),
            makeup_db: FloatParam::new("makeup_db", "Makeup", 0.0, 0.0, 24.0),
        }
    }
}

/// The sidechain compressor plugin.
pub struct SidechainCompPlugin {
    params: Arc<SidechainCompParams>,
    envelope: f32,
    release_coefficient: f32,
}

impl Default for SidechainCompPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(SidechainCompParams::default()),
            envelope: 0.0,
            release_coefficient: 0.999,
        }
    }
}

fn db_to_linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

fn linear_to_db(linear: f32) -> f32 {
    20.0 * linear.max(1e-6).log10()
}

impl SunmaoPlugin for SidechainCompPlugin {
    const NAME: &'static str = "SunMao Sidechain Comp";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";

    type Params = SidechainCompParams;

    fn params(&self) -> Arc<Self::Params> {
        self.params.clone()
    }

    fn initialize(&mut self, sample_rate: f64, _max_block_size: u32) {
        // ~50 ms release regardless of sample rate.
        let release_samples = (sample_rate * 0.05).max(1.0);
        self.release_coefficient = (-1.0 / release_samples).exp() as f32;
    }

    fn reset(&mut self) {
        self.envelope = 0.0;
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        buffer.copy_input_to_output();

        // Skeleton semantics: parameter changes apply at block rate (last one
        // wins). The milestone DSP replaces this with sample-offset handling.
        let mut threshold_db = self.params.threshold_db.get();
        let mut ratio = self.params.ratio.get();
        let mut makeup_db = self.params.makeup_db.get();
        for change in events.param_changes() {
            let value = change.value.clamp(0.0, 1.0);
            if change.id == self.params.threshold_db.id {
                threshold_db = self.params.threshold_db.min
                    + value * (self.params.threshold_db.max - self.params.threshold_db.min);
            } else if change.id == self.params.ratio.id {
                ratio =
                    self.params.ratio.min + value * (self.params.ratio.max - self.params.ratio.min);
            } else if change.id == self.params.makeup_db.id {
                makeup_db = self.params.makeup_db.min
                    + value * (self.params.makeup_db.max - self.params.makeup_db.min);
            }
        }
        let makeup = db_to_linear(makeup_db);

        let channels = buffer.num_output_channels();
        for sample_index in 0..buffer.num_samples() {
            // M3 replaces this self-keyed detector with the sidechain bus.
            let mut key_peak = 0.0f32;
            for channel in 0..channels {
                key_peak = key_peak.max(buffer.output(channel)[sample_index].abs());
            }
            self.envelope = if key_peak > self.envelope {
                key_peak
            } else {
                key_peak + (self.envelope - key_peak) * self.release_coefficient
            };

            let level_db = linear_to_db(self.envelope);
            let over_db = (level_db - threshold_db).max(0.0);
            let gain_reduction_db = over_db * (1.0 - 1.0 / ratio);
            let gain = db_to_linear(-gain_reduction_db) * makeup;

            for channel in 0..channels {
                buffer.output(channel)[sample_index] *= gain;
            }
        }

        ProcessStatus::Normal
    }

    fn vst3_info() -> Vst3Info {
        Vst3Info {
            class_id: *b"SunMaoFxSideCmp!",
            categories: &["Fx", "Dynamics"],
            ..Default::default()
        }
    }

    fn clap_info() -> ClapInfo {
        ClapInfo {
            id: "com.sunmao.fx.sidechain_comp",
            features: &["audio-effect", "compressor", "stereo"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process_block(plugin: &mut SidechainCompPlugin, level: f32, samples: usize) -> Vec<f32> {
        let input_left = vec![level; samples];
        let input_right = vec![level; samples];
        let inputs: [&[f32]; 2] = [&input_left, &input_right];
        let mut output_left = vec![0.0; samples];
        let mut output_right = vec![0.0; samples];
        let mut outputs: [&mut [f32]; 2] = [&mut output_left, &mut output_right];
        let mut buffer = AudioBuffer::new(&inputs, &mut outputs, samples);
        let events = EventQueue::new();
        let status = plugin.process(
            &mut buffer,
            &events,
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

    #[test]
    fn loud_signals_are_attenuated_and_quiet_signals_pass() {
        let mut plugin = SidechainCompPlugin::default();
        plugin.initialize(48_000.0, 64);

        let loud = process_block(&mut plugin, 1.0, 64);
        assert!(
            loud[63] < 0.6,
            "0 dBFS input over a -24 dB threshold at 4:1 must be reduced, got {}",
            loud[63]
        );

        plugin.reset();
        let quiet = process_block(&mut plugin, 0.01, 64);
        assert!(
            (quiet[63] - 0.01).abs() < 1e-3,
            "signal far below threshold should be nearly untouched, got {}",
            quiet[63]
        );
    }
}

// ============ Unified VST3 + CLAP Export ============
sunmao::sunmao_export!(SidechainCompPlugin);
