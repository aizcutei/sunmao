//! SunMao Gain Plugin with WebView GUI
//!
//! This example demonstrates a gain effect plugin with a custom GUI
//! using the WebView renderer backend.

use std::sync::Arc;
use sunmao_core::prelude::*;
use sunmao_gui::gl::GlContext;
use sunmao_gui::{
    Color, Event as GuiEvent, Fill, GuiContext, MouseButton as GuiMouseButton, ParameterWidget,
    Rect, Slider, Widget,
};
use sunmao_macros::Params;
use sunmao_view_baseview::{BaseviewConfig, BaseviewView, ViewState, WindowScalePolicy};

// ============ Plugin Definition ============

#[derive(Params)]
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
        _events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        let gain = self.params.gain.get();
        buffer.copy_input_to_output();
        buffer.apply_gain(gain);
        ProcessStatus::Normal
    }

    fn view(&self) -> Option<Box<dyn SunmaoView>> {
        let config = BaseviewConfig {
            title: "SunMao Gain".to_string(),
            width: 400,
            height: 120,
            scale_policy: WindowScalePolicy::SystemScaleFactor,
            background: Color::rgb(0.12, 0.12, 0.18),
        };

        let view = BaseviewView::new(config, |context| GainViewState::new(context, 400.0, 120.0));
        Some(Box::new(view))
    }
}

struct GainViewState {
    slider: Slider,
    context: Arc<dyn ViewContext>,
    editing: bool,
}

impl GainViewState {
    fn new(context: Arc<dyn ViewContext>, width: f32, height: f32) -> Self {
        let mut slider = Slider::new("gain").with_default(0.5);
        slider.set_bounds(Rect::new(20.0, (height - 24.0) * 0.5, width - 40.0, 24.0));
        Self {
            slider,
            context,
            editing: false,
        }
    }

    fn relayout(&mut self, width: f32, height: f32) {
        self.slider
            .set_bounds(Rect::new(20.0, (height - 24.0) * 0.5, width - 40.0, 24.0));
    }

    fn sync_from_params(&mut self) {
        if let Some(value) = self.context.get_param("gain") {
            self.slider.set_value(value);
        }
    }
}

impl ViewState for GainViewState {
    fn draw(&mut self, ctx: &mut GlContext, width: f32, height: f32) {
        self.sync_from_params();
        ctx.fill_rect(
            0.0,
            0.0,
            width,
            height,
            Fill::Solid(Color::rgb(0.14, 0.14, 0.2)),
        );
        self.slider.draw(ctx);
    }

    fn on_mouse_event(&mut self, event: &GuiEvent) -> bool {
        let before = self.slider.value();
        let handled = self.slider.handle_event(event);
        let after = self.slider.value();

        if handled && (after - before).abs() > f32::EPSILON {
            self.context.set_param("gain", after);
        }

        match event {
            GuiEvent::MouseDown {
                button: GuiMouseButton::Left,
                ..
            } if handled => {
                self.editing = true;
                self.context.begin_edit("gain");
            }
            GuiEvent::MouseUp {
                button: GuiMouseButton::Left,
                ..
            } if self.editing => {
                self.editing = false;
                self.context.end_edit("gain");
            }
            _ => {}
        }

        handled
    }

    fn on_resize(&mut self, width: f32, height: f32) {
        self.relayout(width, height);
    }
}

// ============ VST3 Export ============
use sunmao_backend_vst3::SunmaoVst3Wrapper;
sunmao_backend_vst3::export_vst3_plugin_with_gui!(SunmaoVst3Wrapper<GainPlugin>);

// ============ AU Export (macOS only) ============
#[cfg(target_os = "macos")]
mod au_export {
    use super::*;
    use std::ffi::c_void;
    use sunmao_backend_au::gui::{
        view_bounds, webview, CocoaObject, GuiConfig, GuiHandler, MessageCallback,
    };
    use sunmao_backend_au::{
        au_params, fourcc, get_parameter_local, set_parameter_local, NSPoint, NSRect, NSSize,
        PluginInfo,
    };

    const PARAM_INDEX: u32 = 0;

    const AU_WEBVIEW_CONFIG: GuiConfig = GuiConfig {
        factory_class: "SunmaoAUCocoaViewFactoryWebView",
        view_class: "SunmaoAUCocoaViewWebView",
        view_superclass: "NSView",
        description: "SunMao AU WebView",
    };

    struct AuGainWebViewGui {
        webview: *mut CocoaObject,
        audio_unit: *mut c_void,
    }

    impl Default for AuGainWebViewGui {
        fn default() -> Self {
            Self {
                webview: std::ptr::null_mut(),
                audio_unit: std::ptr::null_mut(),
            }
        }
    }

    impl AuGainWebViewGui {
        fn html() -> &'static str {
            r#"<!DOCTYPE html>
<html>
<head>
    <meta charset=\"utf-8\" />
    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />
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
        input[type=\"range\"] {
            -webkit-appearance: none;
            width: 100%;
            height: 8px;
            background: rgba(255, 255, 255, 0.1);
            border-radius: 4px;
            outline: none;
            cursor: pointer;
        }
        input[type=\"range\"]::-webkit-slider-thumb {
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
    <div class=\"container\">
        <div class=\"title\">Gain</div>
        <div class=\"value-display\" id=\"value\">1.00</div>
        <div class=\"slider-container\">
            <input type=\"range\" id=\"slider\" min=\"0\" max=\"200\" value=\"100\" />
        </div>
        <div class=\"labels\">
            <span>0.0</span>
            <span>1.0</span>
            <span>2.0</span>
        </div>
    </div>
    <script>
        const slider = document.getElementById('slider');
        const valueDisplay = document.getElementById('value');

        function updateValue(val) {
            const gain = val / 100;
            valueDisplay.textContent = gain.toFixed(2);
        }

        slider.addEventListener('input', function () {
            const gain = this.value / 100;
            updateValue(this.value);
            if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.sunmao) {
                window.webkit.messageHandlers.sunmao.postMessage(gain.toString());
            }
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

        fn update_slider_from_au(&self, audio_unit: *mut c_void) {
            if self.webview.is_null() || audio_unit.is_null() {
                return;
            }
            if let Ok(value) = get_parameter_local(audio_unit, PARAM_INDEX) {
                let clamped = value.clamp(0.0, 2.0);
                let script = format!("setGain({});", clamped);
                webview::evaluate_js(self.webview, &script);
            }
        }
    }

    impl GuiHandler for AuGainWebViewGui {
        fn init(&mut self, view: *mut CocoaObject, size: NSSize, audio_unit: *mut c_void) {
            self.audio_unit = audio_unit;
            let frame = NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size,
            };
            let web = webview::create_wkwebview_with_handler(
                view,
                frame,
                "sunmao",
                on_js_message as MessageCallback,
                self as *mut _ as *mut c_void,
            );
            webview::load_html(web, Self::html());
            self.webview = web;
            self.update_slider_from_au(audio_unit);
        }

        fn reshape(&mut self, view: *mut CocoaObject, _audio_unit: *mut c_void) {
            if self.webview.is_null() {
                return;
            }
            let bounds = view_bounds(view);
            webview::set_frame(self.webview, bounds);
        }

        fn deinit(&mut self, _view: *mut CocoaObject) {
            self.webview = std::ptr::null_mut();
        }
    }

    fn on_js_message(message: &str, user_data: *mut c_void) {
        if user_data.is_null() {
            return;
        }
        let gui = unsafe { &mut *(user_data as *mut AuGainWebViewGui) };
        if gui.audio_unit.is_null() {
            return;
        }
        if let Ok(value) = message.parse::<f32>() {
            let clamped = value.clamp(0.0, 2.0);
            let _ = set_parameter_local(gui.audio_unit, PARAM_INDEX, clamped);
        }
    }

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

    sunmao_backend_au::sunmao_export_au!(
        SunMaoGainWebViewFactory,
        GainPlugin,
        AU_INFO,
        au_params::<GainParams>(),
        gui: { handler: AuGainWebViewGui, config: AU_WEBVIEW_CONFIG }
    );
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
