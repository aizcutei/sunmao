//! SunMao Gain Effect Example
//!
//! A simple gain plugin demonstrating the SunMao framework.

use std::sync::Arc;
use sunmao_core::prelude::*;
use sunmao_macros::Params;

/// Gain plugin parameters.
#[derive(Params)]
#[cfg_attr(all(target_os = "macos", feature = "au"), sunmao_au)]
pub struct GainParams {
    /// Gain amount (0.0 to 2.0, default 1.0).
    #[unit = "LinearGain"]
    pub gain: FloatParam,
    /// Output polarity: 0 = normal, 1 = inverted.
    pub polarity: IntParam,
    /// Pass input through without applying gain or polarity.
    pub bypass: BoolParam,
}

impl Default for GainParams {
    fn default() -> Self {
        Self {
            gain: FloatParam::new("gain", "Gain", 1.0, 0.0, 2.0),
            polarity: IntParam::new("polarity", "Polarity", 0, 0, 1),
            bypass: BoolParam::new("bypass", "Bypass", false),
        }
    }
}

/// The Gain plugin.
pub struct GainPlugin {
    params: Arc<GainParams>,
}

impl Default for GainPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(GainParams::default()),
        }
    }
}

impl SunmaoPlugin for GainPlugin {
    const NAME: &'static str = "SunMao Gain";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";

    type Params = GainParams;

    fn params(&self) -> Arc<Self::Params> {
        self.params.clone()
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        buffer.copy_input_to_output();
        let mut gain = self.params.gain.get();
        let mut polarity = self.params.polarity.get();
        let mut bypass = self.params.bypass.get();
        let mut changes = events.param_changes().peekable();

        for sample_index in 0..buffer.num_samples() {
            while changes
                .peek()
                .is_some_and(|change| change.offset as usize <= sample_index)
            {
                let change = changes.next().expect("peeked parameter change");
                if change.id == self.params.gain.id {
                    gain = self.params.gain.min
                        + change.value.clamp(0.0, 1.0)
                            * (self.params.gain.max - self.params.gain.min);
                } else if change.id == self.params.polarity.id {
                    polarity = if change.value >= 0.5 { 1 } else { 0 };
                } else if change.id == self.params.bypass.id {
                    bypass = change.value >= 0.5;
                }
            }

            let multiplier = if bypass {
                1.0
            } else if polarity == 0 {
                gain
            } else {
                -gain
            };
            for channel in 0..buffer.num_output_channels() {
                buffer.output(channel)[sample_index] *= multiplier;
            }
        }

        ProcessStatus::Normal
    }

    fn vst3_info() -> Vst3Info {
        Vst3Info {
            class_id: *b"SunMaoFxGain!!!!",
            categories: &["Fx", "Tools"],
            ..Default::default()
        }
    }

    fn au_info() -> AuInfo {
        AuInfo {
            type_code: *b"aufx",
            subtype_code: *b"smgn",
            manufacturer_code: *b"SunM",
        }
    }

    fn clap_info() -> ClapInfo {
        ClapInfo {
            id: "com.sunmao.fx.gain",
            features: &["audio-effect", "utility", "stereo"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timed_gain_changes_apply_before_the_target_sample_and_last_wins() {
        let mut plugin = GainPlugin::default();
        let input_left = [1.0; 8];
        let input_right = [1.0; 8];
        let inputs: [&[f32]; 2] = [&input_left, &input_right];
        let mut output_left = [0.0; 8];
        let mut output_right = [0.0; 8];
        let mut outputs: [&mut [f32]; 2] = [&mut output_left, &mut output_right];
        let mut buffer = AudioBuffer::new(&inputs, &mut outputs, 8);
        let mut events = EventQueue::new();
        events.push_param_change(ParamChange {
            id: "gain",
            value: 0.25,
            offset: 2,
        });
        events.push_param_change(ParamChange {
            id: "gain",
            value: 0.75,
            offset: 5,
        });
        events.push_param_change(ParamChange {
            id: "gain",
            value: 0.5,
            offset: 5,
        });

        let status = plugin.process(
            &mut buffer,
            &events,
            &ProcessContext {
                sample_rate: 48_000.0,
                tempo: None,
                is_playing: true,
                sample_pos: 0,
            },
        );

        assert_eq!(status, ProcessStatus::Normal);
        assert_eq!(output_left, [1.0, 1.0, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0]);
        assert_eq!(output_right, output_left);
        assert_eq!(plugin.params.gain.get(), 1.0);
    }

    #[test]
    fn discrete_parameter_changes_affect_dsp_at_their_sample_offsets() {
        let mut plugin = GainPlugin::default();
        let input_left = [1.0; 6];
        let input_right = [1.0; 6];
        let inputs: [&[f32]; 2] = [&input_left, &input_right];
        let mut output_left = [0.0; 6];
        let mut output_right = [0.0; 6];
        let mut outputs: [&mut [f32]; 2] = [&mut output_left, &mut output_right];
        let mut buffer = AudioBuffer::new(&inputs, &mut outputs, 6);
        let mut events = EventQueue::new();
        events.push_param_change(ParamChange {
            id: "polarity",
            value: 1.0,
            offset: 2,
        });
        events.push_param_change(ParamChange {
            id: "bypass",
            value: 1.0,
            offset: 4,
        });

        let status = plugin.process(
            &mut buffer,
            &events,
            &ProcessContext {
                sample_rate: 48_000.0,
                tempo: None,
                is_playing: true,
                sample_pos: 0,
            },
        );

        assert_eq!(status, ProcessStatus::Normal);
        assert_eq!(output_left, [1.0, 1.0, -1.0, -1.0, 1.0, 1.0]);
        assert_eq!(output_right, output_left);
        assert_eq!(plugin.params.polarity.get(), 0);
        assert!(!plugin.params.bypass.get());
    }
}

// ============ Unified VST3 + CLAP Export ============
sunmao::sunmao_export!(GainPlugin);

// ============ AU Export (macOS only) ============
#[cfg(all(target_os = "macos", feature = "au"))]
mod au_export {
    use super::*;
    use sunmao_backend_au::SunmaoAuWrapper;
    use sunmao_backend_au::{au_params, export_au_plugin, fourcc, PluginInfo};

    const AU_INFO: PluginInfo = PluginInfo {
        name: "SunMao Gain",
        component_type: fourcc(b"aufx"),
        component_subtype: fourcc(b"smgn"),
        manufacturer: fourcc(b"SunM"),
        version: 0x00010000,
        flags: 0,
        flags_mask: 0,
        input_channels: 2,
        output_channels: 2,
        supports_midi: false,
    };

    export_au_plugin!(
        SunMaoGainFactory,
        SunmaoAuWrapper<GainPlugin>,
        AU_INFO,
        au_params::<GainParams>()
    );
}
