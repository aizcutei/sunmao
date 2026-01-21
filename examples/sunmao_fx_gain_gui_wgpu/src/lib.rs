//! SunMao Gain Plugin with WGPU GUI
//!
//! This example demonstrates a gain effect plugin with a custom GUI
//! using the WGPU renderer backend.

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
    const NAME: &'static str = "SunMao Gain WGPU";
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
        get_view_layer, set_needs_display, view_bounds, CocoaObject, GuiConfig, GuiHandler, Layer,
        TransactionGuard,
    };
    use sunmao_backend_au::{
        au_params, fourcc, get_parameter_local, set_parameter_local, NSPoint, NSSize, PluginInfo,
    };

    const PARAM_INDEX: u32 = 0;
    const SLIDER_MARGIN: f64 = 20.0;
    const SLIDER_HEIGHT: f64 = 24.0;
    const SLIDER_TRACK_HEIGHT: f64 = 6.0;
    const KNOB_WIDTH: f64 = 12.0;

    const AU_WGPU_CONFIG: GuiConfig = GuiConfig {
        factory_class: "SunmaoAUCocoaViewFactoryWgpu",
        view_class: "SunmaoAUCocoaViewWgpu",
        view_superclass: "MTKView",
        description: "SunMao AU WGPU",
    };

    fn normalize(value: f32) -> f32 {
        ((value - 0.0) / (2.0 - 0.0)).clamp(0.0, 1.0)
    }

    fn denormalize(norm: f32) -> f32 {
        0.0 + norm.clamp(0.0, 1.0) * (2.0 - 0.0)
    }

    struct AuGainWgpuGui {
        track_layer: Layer,
        knob_layer: Layer,
        value: f32,
        dragging: bool,
        width: f64,
        height: f64,
    }

    impl Default for AuGainWgpuGui {
        fn default() -> Self {
            Self {
                track_layer: Layer::from_ptr(std::ptr::null_mut()),
                knob_layer: Layer::from_ptr(std::ptr::null_mut()),
                value: 0.5,
                dragging: false,
                width: 400.0,
                height: 120.0,
            }
        }
    }

    impl AuGainWgpuGui {
        fn refresh_from_au(&mut self, audio_unit: *mut c_void) {
            if audio_unit.is_null() {
                return;
            }
            if let Ok(value) = get_parameter_local(audio_unit, PARAM_INDEX) {
                self.value = normalize(value);
            }
        }

        fn gain_from_x(&self, x: f64) -> f32 {
            let slider_width = (self.width - 2.0 * SLIDER_MARGIN - KNOB_WIDTH).max(1.0);
            let local_x = (x - SLIDER_MARGIN).clamp(0.0, slider_width);
            (local_x / slider_width) as f32
        }

        fn update_knob_position(&self) {
            if self.knob_layer.is_null() {
                return;
            }
            let slider_width = self.width - 2.0 * SLIDER_MARGIN - KNOB_WIDTH;
            let knob_x = SLIDER_MARGIN + (self.value as f64).clamp(0.0, 1.0) * slider_width;
            let knob_height = SLIDER_HEIGHT * 2.0;
            let knob_y = (self.height - knob_height) / 2.0;
            let _guard = TransactionGuard::begin_no_animation();
            self.knob_layer
                .set_frame(knob_x, knob_y, KNOB_WIDTH, knob_height);
        }

        fn set_value(&mut self, view: *mut CocoaObject, audio_unit: *mut c_void, normalized: f32) {
            self.value = normalized.clamp(0.0, 1.0);
            if !audio_unit.is_null() {
                let value = denormalize(self.value);
                let _ = set_parameter_local(audio_unit, PARAM_INDEX, value);
            }
            self.update_knob_position();
            set_needs_display(view);
        }
    }

    const AU_INFO: PluginInfo = PluginInfo {
        name: "SunMao Gain WGPU",
        component_type: fourcc(b"aufx"),
        component_subtype: fourcc(b"smgw"),
        manufacturer: fourcc(b"SunM"),
        version: 0x00010000,
        flags: 0,
        flags_mask: 0,
        input_channels: 2,
        output_channels: 2,
        supports_midi: false,
    };

    impl GuiHandler for AuGainWgpuGui {
        fn init(&mut self, view: *mut CocoaObject, size: NSSize, audio_unit: *mut c_void) {
            self.width = size.width.max(1.0);
            self.height = size.height.max(1.0);
            self.refresh_from_au(audio_unit);

            let root_layer = get_view_layer(view);
            if root_layer.is_null() {
                return;
            }

            root_layer.set_background_color(0.12, 0.12, 0.18, 1.0);

            let track_layer = Layer::new();
            let track_width = self.width - 2.0 * SLIDER_MARGIN;
            let track_y = (self.height - SLIDER_TRACK_HEIGHT) / 2.0;
            track_layer.set_frame(SLIDER_MARGIN, track_y, track_width, SLIDER_TRACK_HEIGHT);
            track_layer.set_background_color(0.3, 0.3, 0.35, 1.0);
            root_layer.add_sublayer(&track_layer);
            self.track_layer = track_layer;

            let knob_layer = Layer::new();
            knob_layer.set_background_color(0.2, 0.7, 1.0, 1.0);
            knob_layer.set_corner_radius(4.0);
            root_layer.add_sublayer(&knob_layer);
            self.knob_layer = knob_layer;

            self.update_knob_position();
            set_needs_display(view);
        }

        fn reshape(&mut self, view: *mut CocoaObject, _audio_unit: *mut c_void) {
            let bounds = view_bounds(view);
            self.width = bounds.size.width.max(1.0);
            self.height = bounds.size.height.max(1.0);

            if !self.track_layer.is_null() {
                let track_width = self.width - 2.0 * SLIDER_MARGIN;
                let track_y = (self.height - SLIDER_TRACK_HEIGHT) / 2.0;
                let _guard = TransactionGuard::begin_no_animation();
                self.track_layer.set_frame(
                    SLIDER_MARGIN,
                    track_y,
                    track_width,
                    SLIDER_TRACK_HEIGHT,
                );
            }
            self.update_knob_position();
            set_needs_display(view);
        }

        fn mouse_down(
            &mut self,
            view: *mut CocoaObject,
            audio_unit: *mut c_void,
            point: NSPoint,
            _flags: u64,
        ) {
            self.dragging = true;
            let value = self.gain_from_x(point.x);
            self.set_value(view, audio_unit, value);
        }

        fn mouse_dragged(
            &mut self,
            view: *mut CocoaObject,
            audio_unit: *mut c_void,
            point: NSPoint,
            _flags: u64,
        ) {
            if self.dragging {
                let value = self.gain_from_x(point.x);
                self.set_value(view, audio_unit, value);
            }
        }

        fn mouse_up(
            &mut self,
            view: *mut CocoaObject,
            _audio_unit: *mut c_void,
            _point: NSPoint,
            _flags: u64,
        ) {
            self.dragging = false;
            set_needs_display(view);
        }
    }

    sunmao_backend_au::sunmao_export_au!(
        SunMaoGainWgpuFactory,
        GainPlugin,
        AU_INFO,
        au_params::<GainParams>(),
        gui: { handler: AuGainWgpuGui, config: AU_WGPU_CONFIG }
    );
}

// ============ CLAP Export ============
mod clap_export {
    use super::*;
    use std::ffi::c_char;
    use sunmao_backend_clap::SunmaoClapWrapper;
    use sunmao_backend_clap::{export_clap_plugin_with_gui, ClapFeature, ClapFeatures, PluginInfo};

    static PLUGIN_INFO: PluginInfo = PluginInfo {
        id: "com.sunmao.fx.gain.wgpu\0",
        name: "SunMao Gain WGPU\0",
        vendor: "aizcutei\0",
        url: "https://aizcutei.github.io/sunmao\0",
        manual_url: "\0",
        support_url: "\0",
        version: "1.0.0\0",
        description: "Gain effect with WGPU GUI\0",
    };

    const FEATURES_LIST: [*const c_char; 3] = [
        ClapFeature::AudioEffect.as_ptr(),
        ClapFeature::Utility.as_ptr(),
        std::ptr::null(),
    ];
    static FEATURES: ClapFeatures = ClapFeatures::new(&FEATURES_LIST);

    export_clap_plugin_with_gui!(SunmaoClapWrapper<GainPlugin>, PLUGIN_INFO, FEATURES);
}
