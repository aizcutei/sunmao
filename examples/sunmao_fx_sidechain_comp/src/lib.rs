//! SunMao Sidechain Compressor — Phase 2 acceptance fixture.
//!
//! A feed-forward compressor whose detector keys off a declared sidechain
//! bus (M3). When the host leaves the sidechain unconnected the detector
//! falls back to the main signal path.

use sunmao::prelude::*;

/// Index of the declared sidechain bus within `input_buses`.
const SIDECHAIN_BUS: usize = 1;

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
    /// Whether the host has the key bus switched on. Hosts that never call
    /// the activation callback leave this at its default of `true`, which
    /// keeps the buffer-driven fallback in charge.
    sidechain_active: bool,
}

impl Default for SidechainCompPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(SidechainCompParams::default()),
            envelope: 0.0,
            release_coefficient: 0.999,
            sidechain_active: true,
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

    fn input_buses(&self) -> Vec<BusInfo> {
        vec![
            BusInfo::main("Input", 2),
            BusInfo::sidechain("Sidechain", 2),
        ]
    }

    fn set_bus_active(&mut self, is_input: bool, bus_index: u32, active: bool) -> bool {
        // A host that switches the key bus off gets the main-path detector
        // instead. The main buses are always accepted; there is no
        // configuration this plugin cannot run in.
        if is_input && bus_index as usize == SIDECHAIN_BUS {
            self.sidechain_active = active;
        }
        true
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
        // The detector keys off the sidechain bus when the host connected one,
        // and falls back to the main path otherwise so the plugin still works
        // in a host that leaves the key input unpatched.
        let key_channels = if self.sidechain_active {
            buffer
                .input_bus_channels(SIDECHAIN_BUS)
                .map(|range| range.len())
                .unwrap_or(0)
        } else {
            // The host deactivated the key bus; its samples are meaningless
            // even though the bus still occupies its buffer slot.
            0
        };
        for sample_index in 0..buffer.num_samples() {
            let mut key_peak = 0.0f32;
            if key_channels > 0 {
                for channel in 0..key_channels {
                    let key = buffer.input_bus(SIDECHAIN_BUS, channel);
                    if let Some(sample) = key.get(sample_index) {
                        key_peak = key_peak.max(sample.abs());
                    }
                }
            } else {
                for channel in 0..channels {
                    key_peak = key_peak.max(buffer.output(channel)[sample_index].abs());
                }
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

    /// Runs a block with an explicitly connected sidechain bus: the main path
    /// carries `main_level`, the key bus carries `key_level`.
    fn process_with_key(
        plugin: &mut SidechainCompPlugin,
        main_level: f32,
        key_level: f32,
        samples: usize,
    ) -> Vec<f32> {
        let main_left = vec![main_level; samples];
        let main_right = vec![main_level; samples];
        let key_left = vec![key_level; samples];
        let key_right = vec![key_level; samples];
        let inputs: [&[f32]; 4] = [&main_left, &main_right, &key_left, &key_right];
        let mut output_left = vec![0.0; samples];
        let mut output_right = vec![0.0; samples];
        let mut outputs: [&mut [f32]; 2] = [&mut output_left, &mut output_right];
        // Two stereo input buses, exactly what `input_buses` declares.
        let bounds = [0_usize, 2, 4];
        let mut buffer =
            AudioBuffer::new(&inputs, &mut outputs, samples).with_input_bus_bounds(&bounds);
        let events = EventQueue::new();
        let status = plugin.process(
            &mut buffer,
            &events,
            &ProcessContext {
                sample_rate: 48_000.0,
                is_playing: true,
                ..Default::default()
            },
        );
        assert_eq!(status, ProcessStatus::Normal);
        output_left
    }

    #[test]
    fn the_plugin_declares_a_stereo_sidechain_bus() {
        let plugin = SidechainCompPlugin::default();
        let buses = plugin.input_buses();

        assert_eq!(buses.len(), 2);
        assert_eq!(buses[0].role, BusRole::Main);
        assert_eq!(buses[SIDECHAIN_BUS].role, BusRole::Sidechain);
        assert_eq!(buses[SIDECHAIN_BUS].channels, 2);
    }

    #[test]
    fn a_loud_key_ducks_a_quiet_main_signal() {
        let mut plugin = SidechainCompPlugin::default();
        plugin.initialize(48_000.0, 64);

        // Main is far below the threshold, so only the key can trigger
        // gain reduction. Without the sidechain path this block would pass
        // through untouched.
        let ducked = process_with_key(&mut plugin, 0.05, 1.0, 64);
        assert!(
            ducked[63] < 0.05 * 0.6,
            "a 0 dBFS key must duck the quiet main path, got {}",
            ducked[63]
        );
    }

    #[test]
    fn deactivating_the_key_bus_returns_the_detector_to_the_main_path() {
        let mut plugin = SidechainCompPlugin::default();
        plugin.initialize(48_000.0, 64);

        // With the key bus live, a loud key ducks the quiet main path.
        assert!(plugin.set_bus_active(true, SIDECHAIN_BUS as u32, true));
        let keyed = process_with_key(&mut plugin, 0.05, 1.0, 64);
        assert!(keyed[63] < 0.05 * 0.6, "baseline: the key must duck");

        // Once the host switches the key bus off, the same buffer layout must
        // be detected from the main path instead — which is far below the
        // threshold, so nothing is ducked.
        plugin.reset();
        assert!(plugin.set_bus_active(true, SIDECHAIN_BUS as u32, false));
        let unkeyed = process_with_key(&mut plugin, 0.05, 1.0, 64);
        assert!(
            (unkeyed[63] - 0.05).abs() < 1e-3,
            "a deactivated key bus must not drive the detector, got {}",
            unkeyed[63]
        );

        // Reactivating restores the keyed behaviour.
        plugin.reset();
        assert!(plugin.set_bus_active(true, SIDECHAIN_BUS as u32, true));
        let rekeyed = process_with_key(&mut plugin, 0.05, 1.0, 64);
        assert!(
            rekeyed[63] < 0.05 * 0.6,
            "reactivation must restore ducking"
        );
    }

    #[test]
    fn activating_the_main_buses_is_always_accepted() {
        let mut plugin = SidechainCompPlugin::default();
        // Main in, main out: neither may be refused, and neither disturbs the
        // sidechain state.
        assert!(plugin.set_bus_active(true, 0, false));
        assert!(plugin.set_bus_active(false, 0, false));
        assert!(plugin.sidechain_active, "only bus 1 tracks the key state");
    }

    #[test]
    fn a_silent_key_leaves_a_loud_main_signal_alone() {
        let mut plugin = SidechainCompPlugin::default();
        plugin.initialize(48_000.0, 64);

        // The detector must ignore the main path entirely once a key bus is
        // connected, so a loud main signal with a silent key is untouched
        // apart from makeup gain.
        let passed = process_with_key(&mut plugin, 0.5, 0.0, 64);
        assert!(
            (passed[63] - 0.5).abs() < 1e-3,
            "a silent key must not trigger gain reduction, got {}",
            passed[63]
        );
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
