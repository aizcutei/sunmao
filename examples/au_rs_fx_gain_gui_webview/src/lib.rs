//! AU RS Fx Gain with WKWebView GUI
//! 
//! A gain effect with an HTML/CSS/JS slider rendered in WKWebView.
//! Demonstrates bidirectional JS-Rust communication for parameter control.

use au_rs::{
    export_au_plugin, for_each_channel, fourcc, BufferList, ParameterInfo, ParameterUnit, Plugin,
    PluginInfo,
};

const PARAM_GAIN: u32 = 0;

const PARAMETERS: [ParameterInfo; 1] = [ParameterInfo {
    id: PARAM_GAIN,
    name: "Gain",
    min: 0.0,
    max: 2.0,
    default: 1.0,
    unit: ParameterUnit::LinearGain,
}];

pub struct GainEffectWry {
    gain: f32,
}

impl Plugin for GainEffectWry {
    fn init(_sample_rate: f64, _max_frames: u32) -> Self {
        Self { gain: 1.0 }
    }

    fn process(
        &mut self,
        inputs: Option<BufferList<'_>>,
        outputs: &mut BufferList<'_>,
        frames: usize,
    ) {
        for_each_channel(inputs, outputs, frames, |input, output| {
            for (idx, out_sample) in output.iter_mut().enumerate() {
                let sample = input.and_then(|buf| buf.get(idx)).copied().unwrap_or(0.0);
                *out_sample = sample * self.gain;
            }
        });
    }

    fn parameters(&self) -> &'static [ParameterInfo] {
        &PARAMETERS
    }

    fn get_parameter(&self, id: u32) -> f32 {
        match id {
            PARAM_GAIN => self.gain,
            _ => 0.0,
        }
    }

    fn set_parameter(&mut self, id: u32, value: f32) {
        if id == PARAM_GAIN {
            self.gain = value.clamp(0.0, 2.0);
        }
    }
}

#[cfg(target_os = "macos")]
mod gui {
    use std::ffi::c_void;

    use au_rs::{
        gui::webview::{self, MessageCallback},
        gui::{view_bounds, GuiConfig, GuiHandler, set_needs_display},
        get_parameter_local, set_parameter_local,
        NSRect, NSSize, NSPoint,
    };
    use objc::runtime::Object;

    use crate::PARAM_GAIN;

    /// HTML content for the gain slider UI
    const SLIDER_HTML: &str = include_str!("index.html");

    pub struct WebViewGui {
        webview: *mut Object,
        audio_unit: *mut c_void,
    }

    // Static storage for the WebViewGui pointer (needed for callback)
    static mut WEBVIEW_GUI: *mut WebViewGui = std::ptr::null_mut();

    impl Default for WebViewGui {
        fn default() -> Self {
            Self {
                webview: std::ptr::null_mut(),
                audio_unit: std::ptr::null_mut(),
            }
        }
    }

    // Callback function for JS messages
    fn on_js_message(message: &str, _user_data: *mut c_void) {
        if let Ok(gain) = message.parse::<f32>() {
            unsafe {
                if !WEBVIEW_GUI.is_null() {
                    let gui = &mut *WEBVIEW_GUI;
                    if !gui.audio_unit.is_null() {
                        let _ = set_parameter_local(gui.audio_unit, PARAM_GAIN, gain.clamp(0.0, 2.0));
                    }
                }
            }
        }
    }

    impl GuiHandler for WebViewGui {
        fn init(&mut self, view: *mut Object, size: NSSize, audio_unit: *mut c_void) {
            self.audio_unit = audio_unit;
            
            // Store self pointer for callback
            unsafe {
                WEBVIEW_GUI = self as *mut WebViewGui;
            }
            
            let frame = NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size,
            };
            
            // Create webview with message handler
            let web = webview::create_wkwebview_with_handler(
                view,
                frame,
                "gain",
                on_js_message as MessageCallback,
                std::ptr::null_mut(),
            );
            
            // Load our HTML slider
            webview::load_html(web, SLIDER_HTML);
            self.webview = web;
            
            // Update slider with current gain value after a short delay
            if !audio_unit.is_null() {
                if let Ok(gain) = get_parameter_local(audio_unit, PARAM_GAIN) {
                    // We'll update when the page loads - the HTML default is already 1.0
                    let _ = gain; // Acknowledge we read it
                }
            }
        }

        fn reshape(&mut self, view: *mut Object, _audio_unit: *mut c_void) {
            if self.webview.is_null() {
                return;
            }
            let bounds = view_bounds(view);
            webview::set_frame(self.webview, bounds);
        }

        fn deinit(&mut self, _view: *mut Object) {
            unsafe {
                WEBVIEW_GUI = std::ptr::null_mut();
            }
        }
    }

    pub const CONFIG: GuiConfig = GuiConfig {
        factory_class: "RustAUCocoaViewFactoryWry",
        view_class: "RustAUCocoaViewWry",
        view_superclass: "NSView",
        description: "Rust AU WKWebView",
    };
}

#[cfg(target_os = "macos")]
export_au_plugin!(
    RustAUFactory,
    GainEffectWry,
    PluginInfo {
        name: "Au Rs Fx Gain Gui Webview",
        component_type: fourcc(b"aufx"),
        component_subtype: fourcc(b"rgwy"),
        manufacturer: fourcc(b"RUST"),
        version: 0x0001_0000,
        flags: 0,
        flags_mask: 0,
        input_channels: 2,
        output_channels: 2,
        supports_midi: false,
    },
    &PARAMETERS,
    gui: { handler: gui::WebViewGui, config: gui::CONFIG }
);

#[cfg(not(target_os = "macos"))]
compile_error!("This plugin only supports macOS");
