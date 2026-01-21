//! SunMao Gain Plugin with OpenGL GUI
//!
//! This example demonstrates a gain effect plugin with a custom GUI
//! using the OpenGL renderer backend.

use std::sync::Arc;
use sunmao_core::prelude::*;
use sunmao_gui::gl::GlContext;
use sunmao_gui::ParameterWidget;
use sunmao_gui::{
    Color, Event as GuiEvent, Fill, GuiContext, MouseButton as GuiMouseButton, Rect, Slider, Widget,
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
    const NAME: &'static str = "SunMao Gain GL";
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
        flush_context, make_current_context, open_gl_context, set_best_resolution,
        set_needs_display, set_pixel_format, update_open_gl_view, view_backing_bounds, view_bounds,
        CocoaObject, GuiConfig, GuiHandler,
    };
    use sunmao_backend_au::{
        au_params, fourcc, get_parameter_local, gl_get_proc_address, set_parameter_local, NSPoint,
        NSRect, NSSize, PluginInfo,
    };
    use sunmao_gui::gl::GlContext;
    use sunmao_gui::{
        Color, Event as GuiEvent, Fill, MouseButton as GuiMouseButton, Rect, Slider, Widget,
    };

    const PARAM_INDEX: u32 = 0;
    const SLIDER_MARGIN: f32 = 20.0;
    const SLIDER_HEIGHT: f32 = 24.0;

    const AU_OPENGL_CONFIG: GuiConfig = GuiConfig {
        factory_class: "SunmaoAUCocoaViewFactoryOpenGL",
        view_class: "SunmaoAUCocoaViewOpenGL",
        view_superclass: "NSOpenGLView",
        description: "SunMao AU OpenGL",
    };

    fn normalize(value: f32) -> f32 {
        ((value - 0.0) / (2.0 - 0.0)).clamp(0.0, 1.0)
    }

    fn denormalize(norm: f32) -> f32 {
        0.0 + norm.clamp(0.0, 1.0) * (2.0 - 0.0)
    }

    struct AuGainOpenGlGui {
        slider: Slider,
        width: f32,
        height: f32,
        gl: Option<GlContext>,
        editing: bool,
    }

    impl Default for AuGainOpenGlGui {
        fn default() -> Self {
            let slider = Slider::new("param0").with_default(0.5);
            Self {
                slider,
                width: 400.0,
                height: 120.0,
                gl: None,
                editing: false,
            }
        }
    }

    impl AuGainOpenGlGui {
        fn update_slider_bounds(&mut self) {
            let bounds = Rect::new(
                SLIDER_MARGIN,
                (self.height - SLIDER_HEIGHT) * 0.5,
                (self.width - SLIDER_MARGIN * 2.0).max(1.0),
                SLIDER_HEIGHT,
            );
            self.slider.set_bounds(bounds);
        }

        fn refresh_from_au(&mut self, audio_unit: *mut c_void) {
            if audio_unit.is_null() {
                return;
            }
            if let Ok(value) = get_parameter_local(audio_unit, PARAM_INDEX) {
                self.slider.set_value(normalize(value));
            }
        }

        fn handle_event(&mut self, event: GuiEvent, audio_unit: *mut c_void) -> bool {
            let before = self.slider.value();
            let handled = self.slider.handle_event(&event);
            let after = self.slider.value();

            if handled && (after - before).abs() > f32::EPSILON {
                let value = denormalize(after);
                if !audio_unit.is_null() {
                    let _ = set_parameter_local(audio_unit, PARAM_INDEX, value);
                }
            }
            handled
        }
    }

    impl GuiHandler for AuGainOpenGlGui {
        fn init(&mut self, view: *mut CocoaObject, size: NSSize, audio_unit: *mut c_void) {
            self.width = size.width.max(1.0) as f32;
            self.height = size.height.max(1.0) as f32;

            let attrs = [
                99,     // NSOPENGLPFA_OPENGL_PROFILE
                0x3200, // NSOpenGLProfileVersion3_2Core
                73,     // NSOPENGLPFA_ACCELERATED
                5,      // NSOPENGLPFA_DOUBLEBUFFER
                0,
            ];
            set_pixel_format(view, &attrs);
            set_best_resolution(view, true);

            self.update_slider_bounds();
            self.refresh_from_au(audio_unit);
        }

        fn draw(&mut self, view: *mut CocoaObject, audio_unit: *mut c_void, _rect: NSRect) {
            let ctx = open_gl_context(view);
            make_current_context(ctx);

            if self.gl.is_none() {
                self.gl = unsafe {
                    GlContext::from_loader(
                        |s| gl_get_proc_address(s) as *const _,
                        self.width,
                        self.height,
                    )
                    .ok()
                };
            }

            let bounds = view_bounds(view);
            let backing = view_backing_bounds(view);
            let scale = if bounds.size.width > 0.0 {
                (backing.size.width / bounds.size.width) as f32
            } else {
                1.0
            };
            let physical_w = backing.size.width.max(1.0) as u32;
            let physical_h = backing.size.height.max(1.0) as u32;

            self.width = bounds.size.width.max(1.0) as f32;
            self.height = bounds.size.height.max(1.0) as f32;
            self.update_slider_bounds();
            self.refresh_from_au(audio_unit);

            if let Some(gl) = self.gl.as_mut() {
                gl.set_scale(scale);
                gl.set_viewport(physical_w, physical_h);
                gl.clear(Color::rgb(0.12, 0.12, 0.18));
                gl.begin_frame();

                gl.fill_rect(
                    0.0,
                    0.0,
                    self.width,
                    self.height,
                    Fill::Solid(Color::rgb(0.14, 0.14, 0.2)),
                );
                self.slider.draw(gl);

                gl.end_frame();
            }

            flush_context(ctx);
        }

        fn reshape(&mut self, view: *mut CocoaObject, _audio_unit: *mut c_void) {
            update_open_gl_view(view);
            let bounds = view_bounds(view);
            self.width = bounds.size.width.max(1.0) as f32;
            self.height = bounds.size.height.max(1.0) as f32;
            self.update_slider_bounds();
            set_needs_display(view);
        }

        fn mouse_down(
            &mut self,
            view: *mut CocoaObject,
            audio_unit: *mut c_void,
            point: NSPoint,
            _flags: u64,
        ) {
            let event = GuiEvent::MouseDown {
                x: point.x as f32,
                y: point.y as f32,
                button: GuiMouseButton::Left,
                modifiers: Default::default(),
            };
            if self.handle_event(event, audio_unit) {
                self.editing = true;
            }
            set_needs_display(view);
        }

        fn mouse_dragged(
            &mut self,
            view: *mut CocoaObject,
            audio_unit: *mut c_void,
            point: NSPoint,
            _flags: u64,
        ) {
            let event = GuiEvent::MouseMove {
                x: point.x as f32,
                y: point.y as f32,
                modifiers: Default::default(),
            };
            self.handle_event(event, audio_unit);
            set_needs_display(view);
        }

        fn mouse_up(
            &mut self,
            view: *mut CocoaObject,
            _audio_unit: *mut c_void,
            point: NSPoint,
            _flags: u64,
        ) {
            let event = GuiEvent::MouseUp {
                x: point.x as f32,
                y: point.y as f32,
                button: GuiMouseButton::Left,
                modifiers: Default::default(),
            };
            self.slider.handle_event(&event);
            self.editing = false;
            set_needs_display(view);
        }
    }

    const AU_INFO: PluginInfo = PluginInfo {
        name: "SunMao Gain GL",
        component_type: fourcc(b"aufx"),
        component_subtype: fourcc(b"smgg"),
        manufacturer: fourcc(b"SunM"),
        version: 0x00010000,
        flags: 0,
        flags_mask: 0,
        input_channels: 2,
        output_channels: 2,
        supports_midi: false,
    };

    sunmao_backend_au::sunmao_export_au!(
        SunMaoGainGlFactory,
        GainPlugin,
        AU_INFO,
        au_params::<GainParams>(),
        gui: { handler: AuGainOpenGlGui, config: AU_OPENGL_CONFIG }
    );
}

// ============ CLAP Export ============
mod clap_export {
    use super::*;
    use std::ffi::c_char;
    use sunmao_backend_clap::SunmaoClapWrapper;
    use sunmao_backend_clap::{export_clap_plugin_with_gui, ClapFeature, ClapFeatures, PluginInfo};

    static PLUGIN_INFO: PluginInfo = PluginInfo {
        id: "com.sunmao.fx.gain.gl\0",
        name: "SunMao Gain GL\0",
        vendor: "aizcutei\0",
        url: "https://aizcutei.github.io/sunmao\0",
        manual_url: "\0",
        support_url: "\0",
        version: "1.0.0\0",
        description: "Gain effect with OpenGL GUI\0",
    };

    const FEATURES_LIST: [*const c_char; 3] = [
        ClapFeature::AudioEffect.as_ptr(),
        ClapFeature::Utility.as_ptr(),
        std::ptr::null(),
    ];
    static FEATURES: ClapFeatures = ClapFeatures::new(&FEATURES_LIST);

    export_clap_plugin_with_gui!(SunmaoClapWrapper<GainPlugin>, PLUGIN_INFO, FEATURES);
}
