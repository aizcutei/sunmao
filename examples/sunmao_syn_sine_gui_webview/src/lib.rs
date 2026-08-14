//! SunMao sine synthesizer with an embeddable platform WebView editor.

use std::sync::Arc;

use sunmao_core::prelude::*;
use sunmao_view_baseview::{BaseviewConfig, BaseviewWebView, WebViewState, WindowScalePolicy};

#[path = "../../sunmao_sine_engine.rs"]
mod sine_engine;
use sine_engine::{SineEngine, SineParams};

pub struct SineSynthWebView {
    engine: SineEngine,
}

impl Default for SineSynthWebView {
    fn default() -> Self {
        Self {
            engine: SineEngine::default(),
        }
    }
}

impl SunmaoPlugin for SineSynthWebView {
    const NAME: &'static str = "SunMao Sine Synth WebView";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";

    type Params = SineParams;

    fn input_channels(&self) -> u32 {
        0
    }

    fn output_channels(&self) -> u32 {
        2
    }

    fn accepts_midi(&self) -> bool {
        true
    }

    fn params(&self) -> Arc<Self::Params> {
        self.engine.params()
    }

    fn initialize(&mut self, sample_rate: f64, _max_block_size: u32) {
        self.engine.initialize(sample_rate);
    }

    fn reset(&mut self) {
        self.engine.reset();
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        events: &EventQueue,
        context: &ProcessContext,
    ) -> ProcessStatus {
        self.engine.process(buffer, events, context)
    }

    fn view(&self) -> Option<Box<dyn SunmaoView>> {
        let config = BaseviewConfig {
            title: Self::NAME.to_string(),
            width: 400,
            height: 120,
            scale_policy: WindowScalePolicy::SystemScaleFactor,
            background: sunmao_gui::Color::rgb(0.08, 0.11, 0.14),
        };
        Some(Box::new(BaseviewWebView::new(
            config,
            |_context| SineWebViewState,
            "sunmao",
        )))
    }

    fn vst3_info() -> Vst3Info {
        Vst3Info {
            class_id: *b"SunMaoSineWeb!!!",
            categories: &["Instrument", "Synth"],
            ..Default::default()
        }
    }

    fn clap_info() -> ClapInfo {
        ClapInfo {
            id: "com.sunmao.synth.sine.webview",
            features: &["instrument", "synthesizer", "stereo"],
        }
    }
}

struct SineWebViewState;

impl WebViewState for SineWebViewState {
    fn html(&self) -> &str {
        r#"<!doctype html>
<html>
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<style>
* { box-sizing: border-box; }
html, body { margin: 0; width: 100%; height: 100%; }
body { display: flex; align-items: center; justify-content: center; background: #111c23; color: #e8f1f5; font: 14px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; user-select: none; }
.panel { width: 90%; max-width: 360px; padding: 22px 24px; background: #1b2a33; border: 1px solid #38505c; border-radius: 10px; }
/* Explicit line heights and a fixed slider box keep the layout identical
   across WebKit macOS, WebKitGTK, and WebView2 so host-side input tests can
   target the slider at a known position (center y=138 in a 520x220 view). */
.label { color: #9dc4d2; letter-spacing: .08em; text-transform: uppercase; font-size: 12px; line-height: 14px; margin-bottom: 8px; }
.value { font-size: 38px; line-height: 1; font-variant-numeric: tabular-nums; margin-bottom: 18px; }
input[type=range] { display: block; width: 100%; height: 24px; margin: 0; accent-color: #68c5d8; }
.limits { display: flex; justify-content: space-between; color: #7b9aa5; font-size: 11px; line-height: 13px; margin-top: 8px; }
</style>
</head>
<body>
<div class="panel">
<div class="label">Sine volume</div>
<div class="value" id="value">0.50</div>
<input id="slider" type="range" min="0" max="100" value="50" />
<div class="limits"><span>0.0</span><span>0.5</span><span>1.0</span></div>
</div>
<script>
const slider = document.getElementById('slider');
const value = document.getElementById('value');
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
function update() {
  const accessibilityEdit = !pointerEditing && !editing;
  if (accessibilityEdit) beginEdit();
  const volume = Number(slider.value) / 100;
  value.textContent = volume.toFixed(2);
  if (window.sunmao) window.sunmao.postMessage('value:' + volume);
  if (accessibilityEdit) endEdit();
}
slider.addEventListener('pointerdown', () => { pointerEditing = true; beginEdit(); });
slider.addEventListener('pointerup', () => { pointerEditing = false; endEdit(); });
slider.addEventListener('pointercancel', () => { pointerEditing = false; endEdit(); });
slider.addEventListener('input', update);
</script>
</body>
</html>"#
    }

    fn on_message(&mut self, message: &str, context: &dyn ViewContext) {
        match message {
            "begin" => context.begin_edit("volume"),
            "end" => context.end_edit("volume"),
            value if value.starts_with("value:") => {
                if let Ok(volume) = value[6..].parse::<f32>() {
                    context.set_param("volume", volume.clamp(0.0, 1.0));
                }
            }
            _ => {}
        }
    }
}

sunmao::sunmao_export!(SineSynthWebView, gui);
