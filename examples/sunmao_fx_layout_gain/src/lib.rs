//! Layout-negotiating gain — Phase 3 M1 acceptance fixture.
//!
//! Exercises host-driven speaker-layout negotiation. The plugin publishes two
//! layouts, mono and stereo, and reconfigures itself when the host selects one.
//! The two formats reach the same code from opposite directions:
//!
//! - a CLAP host reads the published list and *selects* an id
//!   (`clap.audio-ports-config`);
//! - a VST3 host *proposes* a speaker arrangement per bus
//!   (`setBusArrangements`) and the backend matches it against the same list.
//!
//! Either way `select_bus_config` is what runs, so a layout that works in one
//! format cannot silently fail in the other.

use sunmao::prelude::*;

/// Index into [`LayoutGainPlugin::bus_configs`].
const MONO: usize = 0;
const STEREO: usize = 1;

/// Parameters.
#[derive(Params)]
pub struct LayoutGainParams {
    /// Gain amount (0.0 to 2.0, default 1.0).
    #[unit = "LinearGain"]
    pub gain: FloatParam,
}

impl Default for LayoutGainParams {
    fn default() -> Self {
        Self {
            gain: FloatParam::new("gain", "Gain", 1.0, 0.0, 2.0),
        }
    }
}

/// A gain that can run mono or stereo, whichever the host negotiates.
pub struct LayoutGainPlugin {
    params: Arc<LayoutGainParams>,
    /// The layout currently in force. Drives `input_buses`/`output_buses`,
    /// which is what the backends use to lay out the audio buffer.
    layout: usize,
}

impl Default for LayoutGainPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(LayoutGainParams::default()),
            // Stereo is the default. `current_bus_config()` reports it, which
            // is what the contract requires agree with `input_buses()` before
            // the host negotiates anything.
            layout: STEREO,
        }
    }
}

impl LayoutGainPlugin {
    /// Channel count of the layout in force.
    fn channels(&self) -> u32 {
        match self.layout {
            MONO => 1,
            _ => 2,
        }
    }
}

impl SunmaoPlugin for LayoutGainPlugin {
    const NAME: &'static str = "SunMao Layout Gain";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";

    type Params = LayoutGainParams;

    fn params(&self) -> Arc<Self::Params> {
        self.params.clone()
    }

    fn input_buses(&self) -> Vec<BusInfo> {
        vec![BusInfo::main("Input", self.channels())]
    }

    fn output_buses(&self) -> Vec<BusInfo> {
        vec![BusInfo::main("Output", self.channels())]
    }

    fn bus_configs(&self) -> Vec<BusConfig> {
        vec![
            BusConfig::new(
                "Mono",
                vec![BusInfo::main("Input", 1)],
                vec![BusInfo::main("Output", 1)],
            ),
            BusConfig::new(
                "Stereo",
                vec![BusInfo::main("Input", 2)],
                vec![BusInfo::main("Output", 2)],
            ),
        ]
    }

    fn current_bus_config(&self) -> usize {
        self.layout
    }

    fn select_bus_config(&mut self, index: usize) -> bool {
        // Only the two published layouts exist; anything else is refused
        // rather than silently clamped, so a host cannot end up believing the
        // plugin is in a layout it is not.
        match index {
            MONO | STEREO => {
                self.layout = index;
                true
            }
            _ => false,
        }
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        buffer.copy_input_to_output();
        let mut gain = self.params.gain.get();
        let mut changes = events.param_changes().peekable();

        for sample_index in 0..buffer.num_samples() {
            while changes
                .peek()
                .is_some_and(|change| change.offset as usize <= sample_index)
            {
                let change = changes.next().expect("peeked");
                if change.id == self.params.gain.id {
                    gain = self.params.gain.min
                        + change.value.clamp(0.0, 1.0)
                            * (self.params.gain.max - self.params.gain.min);
                }
            }
            for channel in 0..buffer.num_output_channels() {
                buffer.output(channel)[sample_index] *= gain;
            }
        }

        ProcessStatus::Normal
    }

    fn vst3_info() -> Vst3Info {
        Vst3Info {
            class_id: *b"SunMaoFxLayout!!",
            categories: &["Fx", "Tools"],
            ..Default::default()
        }
    }

    fn clap_info() -> ClapInfo {
        ClapInfo {
            id: "com.sunmao.fx.layout_gain",
            features: &["audio-effect", "utility"],
        }
    }
}

sunmao::sunmao_export!(LayoutGainPlugin);

#[cfg(test)]
mod tests {
    use super::*;

    fn process_channels(plugin: &mut LayoutGainPlugin, channels: usize) -> Vec<f32> {
        let input = vec![vec![1.0f32; 4]; channels];
        let inputs: Vec<&[f32]> = input.iter().map(Vec::as_slice).collect();
        let mut output_storage = vec![vec![0.0f32; 4]; channels];
        let mut outputs: Vec<&mut [f32]> = output_storage
            .iter_mut()
            .map(|channel| channel.as_mut_slice())
            .collect();
        let mut buffer = AudioBuffer::new(&inputs, &mut outputs, 4);
        let events = EventQueue::new();
        let context = ProcessContext::default();
        plugin.process(&mut buffer, &events, &context);
        output_storage.iter().map(|channel| channel[0]).collect()
    }

    #[test]
    fn the_default_layout_agrees_with_the_declared_buses() {
        let plugin = LayoutGainPlugin::default();
        let configs = plugin.bus_configs();
        // The contract: `input_buses()`/`output_buses()` must agree with the
        // config the plugin reports as current before the host selects one.
        let current = plugin.current_bus_config();
        assert_eq!(plugin.input_buses(), configs[current].inputs);
        assert_eq!(plugin.output_buses(), configs[current].outputs);
    }

    #[test]
    fn selecting_mono_reconfigures_the_declared_buses() {
        let mut plugin = LayoutGainPlugin::default();
        assert_eq!(plugin.input_buses()[0].channels, 2, "starts stereo");

        assert!(plugin.select_bus_config(MONO));
        assert_eq!(plugin.current_bus_config(), MONO);
        assert_eq!(plugin.input_buses()[0].channels, 1);
        assert_eq!(plugin.output_buses()[0].channels, 1);

        // And back again — negotiation is not one-way.
        assert!(plugin.select_bus_config(STEREO));
        assert_eq!(plugin.input_buses()[0].channels, 2);
    }

    #[test]
    fn an_unpublished_layout_is_refused() {
        let mut plugin = LayoutGainPlugin::default();
        assert!(!plugin.select_bus_config(7));
        // The refusal must leave the previous layout untouched.
        assert_eq!(plugin.current_bus_config(), STEREO);
        assert_eq!(plugin.input_buses()[0].channels, 2);
    }

    #[test]
    fn a_vst3_style_proposal_matches_exactly_one_published_layout() {
        let plugin = LayoutGainPlugin::default();
        let configs = plugin.bus_configs();
        // This is the rule the VST3 backend uses to turn a proposed speaker
        // arrangement into a config index.
        let matches: Vec<usize> = configs
            .iter()
            .enumerate()
            .filter(|(_, config)| config.matches(&[1], &[1]))
            .map(|(index, _)| index)
            .collect();
        assert_eq!(matches, vec![MONO]);

        let stereo: Vec<usize> = configs
            .iter()
            .enumerate()
            .filter(|(_, config)| config.matches(&[2], &[2]))
            .map(|(index, _)| index)
            .collect();
        assert_eq!(stereo, vec![STEREO]);

        // A layout nobody published matches nothing, so the backend refuses.
        assert!(!configs.iter().any(|config| config.matches(&[6], &[6])));
        // Mismatched input/output counts must not match either.
        assert!(!configs.iter().any(|config| config.matches(&[1], &[2])));
    }

    #[test]
    fn gain_applies_in_whichever_layout_is_selected() {
        let mut plugin = LayoutGainPlugin::default();
        plugin.params.gain.set(0.5);

        let stereo = process_channels(&mut plugin, 2);
        assert_eq!(stereo.len(), 2);
        assert!(stereo.iter().all(|sample| (sample - 0.5).abs() < 1e-6));

        assert!(plugin.select_bus_config(MONO));
        let mono = process_channels(&mut plugin, 1);
        assert_eq!(mono.len(), 1);
        assert!((mono[0] - 0.5).abs() < 1e-6);
    }
}
