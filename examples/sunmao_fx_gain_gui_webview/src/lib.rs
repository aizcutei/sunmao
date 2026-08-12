//! SunMao Gain Plugin with WebView GUI
//!
//! This example demonstrates a gain effect plugin with a custom GUI
//! using the platform WebView renderer. The same codebase exports to
//! AU, VST3, and CLAP — no format-specific GUI code needed.

use std::sync::Arc;
use sunmao_core::prelude::*;
use sunmao_macros::Params;
use sunmao_view_baseview::{BaseviewConfig, BaseviewWebView, WebViewState, WindowScalePolicy};

// ============ Plugin Definition ============

#[derive(Params)]
#[cfg_attr(all(target_os = "macos", feature = "au"), sunmao_au)]
pub struct GainParams {
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
    const NAME: &'static str = "SunMao Gain WebView";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";

    type Params = GainParams;

    fn params(&self) -> Arc<Self::Params> {
        self.params.clone()
    }

    fn input_channels(&self) -> u32 {
        2
    }
    fn output_channels(&self) -> u32 {
        2
    }
    fn accepts_midi(&self) -> bool {
        false
    }

    fn initialize(&mut self, _sample_rate: f64, _max_frames: u32) {}
    fn reset(&mut self) {}

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        buffer.copy_input_to_output();
        let mut gain = self.params.gain.get();
        let mut changes = events
            .param_changes()
            .filter(|change| change.id == self.params.gain.id)
            .peekable();

        for sample_index in 0..buffer.num_samples() {
            while changes
                .peek()
                .is_some_and(|change| change.offset as usize <= sample_index)
            {
                let change = changes.next().expect("peeked parameter change");
                gain = self.params.gain.min
                    + change.value.clamp(0.0, 1.0) * (self.params.gain.max - self.params.gain.min);
            }
            for channel in 0..buffer.num_output_channels() {
                buffer.output(channel)[sample_index] *= gain;
            }
        }
        ProcessStatus::Normal
    }

    fn view(&self) -> Option<Box<dyn SunmaoView>> {
        let config = BaseviewConfig {
            title: "SunMao Gain".to_string(),
            width: 400,
            height: 120,
            scale_policy: WindowScalePolicy::SystemScaleFactor,
            background: sunmao_gui::Color::rgb(0.12, 0.12, 0.18),
        };

        let view = BaseviewWebView::new(config, |context| GainWebViewState::new(context), "sunmao");
        Some(Box::new(view))
    }
}

// ============ WebView State ============

struct GainWebViewState {
    context: Arc<dyn ViewContext>,
}

impl GainWebViewState {
    fn new(context: Arc<dyn ViewContext>) -> Self {
        Self { context }
    }
}

impl WebViewState for GainWebViewState {
    fn html(&self) -> &str {
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%);
            height: 100vh;
            display: flex;
            justify-content: center;
            align-items: center;
            color: #fff;
            user-select: none;
        }
        .container {
            width: 90%;
            max-width: 360px;
            background: rgba(255, 255, 255, 0.05);
            border-radius: 16px;
            padding: 24px;
            backdrop-filter: blur(10px);
            box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
        }
        .title {
            font-size: 14px;
            font-weight: 500;
            color: #8892b0;
            margin-bottom: 8px;
            text-transform: uppercase;
            letter-spacing: 1px;
        }
        .value-display {
            font-size: 48px;
            font-weight: 700;
            background: linear-gradient(90deg, #64ffda, #7f5af0);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
            margin-bottom: 16px;
        }
        .slider-container { width: 100%; padding: 8px 0; }
        input[type="range"] {
            -webkit-appearance: none;
            width: 100%;
            height: 8px;
            background: rgba(255, 255, 255, 0.1);
            border-radius: 4px;
            outline: none;
            cursor: pointer;
        }
        input[type="range"]::-webkit-slider-thumb {
            -webkit-appearance: none;
            width: 24px;
            height: 24px;
            background: linear-gradient(135deg, #64ffda, #7f5af0);
            border-radius: 50%;
            cursor: pointer;
            box-shadow: 0 2px 8px rgba(100, 255, 218, 0.4);
        }
        .labels {
            display: flex;
            justify-content: space-between;
            margin-top: 8px;
            font-size: 12px;
            color: #5f6c8c;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="title">Gain</div>
        <div class="value-display" id="value">1.00</div>
        <div class="slider-container">
            <input type="range" id="slider" min="0" max="200" value="100" />
        </div>
        <div class="labels">
            <span>0.0</span>
            <span>1.0</span>
            <span>2.0</span>
        </div>
    </div>
    <script>
        const slider = document.getElementById('slider');
        const valueDisplay = document.getElementById('value');
        let pointerEditing = false;
        let editing = false;

        function beginEdit() {
            if (!editing && window.sunmao) {
                editing = true;
                window.sunmao.postMessage('begin');
            }
        }

        function endEdit() {
            if (editing && window.sunmao) {
                editing = false;
                window.sunmao.postMessage('end');
            }
        }

        function updateValue(val) {
            const gain = val / 100;
            valueDisplay.textContent = gain.toFixed(2);
        }

        slider.addEventListener('input', function () {
            const accessibilityEdit = !pointerEditing && !editing;
            if (accessibilityEdit) beginEdit();
            const gain = this.value / 100;
            updateValue(this.value);
            if (window.sunmao) {
                window.sunmao.postMessage('value:' + gain.toString());
            }
            if (accessibilityEdit) endEdit();
        });
        slider.addEventListener('pointerdown', function () {
            pointerEditing = true;
            beginEdit();
        });
        slider.addEventListener('pointerup', function () {
            pointerEditing = false;
            endEdit();
        });
        slider.addEventListener('pointercancel', function () {
            pointerEditing = false;
            endEdit();
        });

        function setGain(gain) {
            const val = Math.round(gain * 100);
            slider.value = val;
            updateValue(val);
        }
    </script>
</body>
</html>"#
    }

    fn on_message(&mut self, message: &str, context: &dyn ViewContext) {
        if message == "begin" {
            context.begin_edit("gain");
        } else if message == "end" {
            context.end_edit("gain");
        } else if let Some(value) = message
            .strip_prefix("value:")
            .and_then(|value| value.parse::<f32>().ok())
        {
            let clamped = value.clamp(0.0, 2.0);
            context.set_param("gain", clamped / 2.0);
        }
    }
}

// ============ VST3 Export ============
use sunmao_backend_vst3::SunmaoVst3Wrapper;
sunmao_backend_vst3::export_vst3_plugin_with_gui!(SunmaoVst3Wrapper<GainPlugin>);

// ============ AU Export (macOS only) ============
#[cfg(all(target_os = "macos", feature = "au"))]
mod au_export {
    use super::*;
    use sunmao_backend_au::{au_params, fourcc, PluginInfo};

    const AU_INFO: PluginInfo = PluginInfo {
        name: "SunMao Gain WebView",
        component_type: fourcc(b"aufx"),
        component_subtype: fourcc(b"smgv"),
        manufacturer: fourcc(b"SunM"),
        version: 0x00010000,
        flags: 0,
        flags_mask: 0,
        input_channels: 2,
        output_channels: 2,
        supports_midi: false,
    };

    // One macro call — no format-specific GUI code needed!
    sunmao_backend_au::sunmao_export_au_with_view!(GainPlugin, AU_INFO, au_params::<GainParams>());
}

// ============ CLAP Export ============
mod clap_export {
    use super::*;
    use std::ffi::c_char;
    use sunmao_backend_clap::SunmaoClapWrapper;
    use sunmao_backend_clap::{export_clap_plugin_with_gui, ClapFeature, ClapFeatures, PluginInfo};

    static PLUGIN_INFO: PluginInfo = PluginInfo {
        id: "com.sunmao.fx.gain.webview\0",
        name: "SunMao Gain WebView\0",
        vendor: "aizcutei\0",
        url: "https://aizcutei.github.io/sunmao\0",
        manual_url: "\0",
        support_url: "\0",
        version: "1.0.0\0",
        description: "Gain effect with WebView GUI\0",
    };

    const FEATURES_LIST: [*const c_char; 3] = [
        ClapFeature::AudioEffect.as_ptr(),
        ClapFeature::Utility.as_ptr(),
        std::ptr::null(),
    ];
    static FEATURES: ClapFeatures = ClapFeatures::new(&FEATURES_LIST);

    export_clap_plugin_with_gui!(SunmaoClapWrapper<GainPlugin>, PLUGIN_INFO, FEATURES);
}
