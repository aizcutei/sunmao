//! SunMao Gain Effect Example
//!
//! A simple gain plugin demonstrating the SunMao framework.

use std::sync::Arc;
use sunmao_core::prelude::*;
use sunmao_macros::Params;

/// Gain plugin parameters.
#[derive(Params)]
pub struct GainParams {
    /// Gain amount (0.0 to 2.0, default 1.0).
    #[unit = "LinearGain"]
    pub gain: FloatParam,
}

impl Default for GainParams {
    fn default() -> Self {
        Self {
            gain: FloatParam::new("gain", "Gain", 1.0, 0.0, 2.0),
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
        _events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        let gain = self.params.gain.get();

        // Copy input to output with gain applied
        buffer.copy_input_to_output();
        buffer.apply_gain(gain);

        ProcessStatus::Normal
    }

    fn vst3_info() -> Vst3Info {
        Vst3Info {
            class_id: *b"SunMaoFxGain!!!!",
            categories: &["Fx", "Tools"],
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

// ============ VST3 Export ============
// Use vst3_rs to export the plugin wrapped by our backend adapter

use sunmao_backend_vst3::SunmaoVst3Wrapper;

sunmao_backend_vst3::export_vst3_plugin!(SunmaoVst3Wrapper<GainPlugin>);

// ============ AU Export (macOS only) ============
#[cfg(target_os = "macos")]
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

// ============ CLAP Export ============
mod clap_export {
    use super::*;
    use std::ffi::c_char;
    use sunmao_backend_clap::SunmaoClapWrapper;
    use sunmao_backend_clap::{export_clap_plugin, ClapFeature, ClapFeatures, PluginInfo};

    static PLUGIN_INFO: PluginInfo = PluginInfo {
        id: "com.sunmao.fx.gain\0",
        name: "SunMao Gain\0",
        vendor: "aizcutei\0",
        url: "https://aizcutei.github.io/sunmao\0",
        manual_url: "\0",
        support_url: "\0",
        version: "1.0.0\0",
        description: "Simple gain effect\0",
    };

    const FEATURES_LIST: [*const c_char; 3] = [
        ClapFeature::AudioEffect.as_ptr(),
        ClapFeature::Utility.as_ptr(),
        std::ptr::null(),
    ];
    static FEATURES: ClapFeatures = ClapFeatures::new(&FEATURES_LIST);

    export_clap_plugin!(SunmaoClapWrapper<GainPlugin>, PLUGIN_INFO, FEATURES);
}
