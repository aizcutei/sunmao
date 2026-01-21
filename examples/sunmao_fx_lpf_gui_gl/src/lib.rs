//! SunMao Lowpass Filter Plugin with OpenGL GUI
//!
//! This example demonstrates a simple lowpass effect with
//! frequency and Q controls, plus a custom OpenGL GUI.

use std::f32::consts::PI;
use std::sync::Arc;
use sunmao_core::prelude::*;
use sunmao_gui::gl::GlContext;
use sunmao_gui::ParameterWidget;
use sunmao_gui::{
    Color, Event as GuiEvent, Fill, GuiContext, MouseButton as GuiMouseButton, Rect, Slider, Widget,
};
use sunmao_macros::Params;
use sunmao_view_baseview::{BaseviewConfig, BaseviewView, ViewState, WindowScalePolicy};

const FREQ_MIN: f32 = 20.0;
const FREQ_MAX: f32 = 20000.0;
const Q_MIN: f32 = 0.1;
const Q_MAX: f32 = 10.0;

fn normalize(value: f32, min: f32, max: f32) -> f32 {
    ((value - min) / (max - min)).clamp(0.0, 1.0)
}

fn denormalize(norm: f32, min: f32, max: f32) -> f32 {
    min + norm.clamp(0.0, 1.0) * (max - min)
}

#[derive(Clone, Copy)]
struct BiquadCoeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Default for BiquadCoeffs {
    fn default() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }
    }
}

struct Biquad {
    coeffs: BiquadCoeffs,
    z1: [f32; 2],
    z2: [f32; 2],
    sample_rate: f32,
    freq: f32,
    q: f32,
}

impl Default for Biquad {
    fn default() -> Self {
        Self {
            coeffs: BiquadCoeffs::default(),
            z1: [0.0; 2],
            z2: [0.0; 2],
            sample_rate: 44100.0,
            freq: 1000.0,
            q: 0.707,
        }
    }
}

impl Biquad {
    fn reset(&mut self) {
        self.z1 = [0.0; 2];
        self.z2 = [0.0; 2];
    }

    fn update(&mut self, sample_rate: f32, freq: f32, q: f32) {
        if sample_rate <= 0.0 {
            return;
        }

        let max_freq = (sample_rate * 0.45).min(FREQ_MAX);
        let freq = freq.clamp(FREQ_MIN, max_freq);
        let q = q.clamp(Q_MIN, Q_MAX);

        let needs_update = (sample_rate - self.sample_rate).abs() > f32::EPSILON
            || (freq - self.freq).abs() > 1e-3
            || (q - self.q).abs() > 1e-3;

        if !needs_update {
            return;
        }

        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = (1.0 - cos_w0) * 0.5;
        let b1 = 1.0 - cos_w0;
        let b2 = (1.0 - cos_w0) * 0.5;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        self.coeffs = BiquadCoeffs {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        };

        self.sample_rate = sample_rate;
        self.freq = freq;
        self.q = q;
    }

    fn process(&mut self, buffer: &mut AudioBuffer) {
        buffer.copy_input_to_output();
        let channels = buffer
            .num_input_channels()
            .min(buffer.num_output_channels());

        for ch in 0..channels {
            let output = buffer.output(ch);
            let mut z1 = self.z1[ch];
            let mut z2 = self.z2[ch];

            for sample in output.iter_mut() {
                let x = *sample;
                let y = self.coeffs.b0 * x + z1;
                z1 = self.coeffs.b1 * x - self.coeffs.a1 * y + z2;
                z2 = self.coeffs.b2 * x - self.coeffs.a2 * y;
                *sample = y;
            }

            self.z1[ch] = z1;
            self.z2[ch] = z2;
        }

        for ch in channels..buffer.num_output_channels() {
            let output = buffer.output(ch);
            for sample in output.iter_mut() {
                *sample = 0.0;
            }
        }
    }
}

// ============ Plugin Definition ============

#[derive(Params)]
pub struct LpfParams {
    pub cutoff: FloatParam,
    pub q: FloatParam,
}

impl Default for LpfParams {
    fn default() -> Self {
        Self {
            cutoff: FloatParam::new("cutoff", "Cutoff", 1000.0, FREQ_MIN, FREQ_MAX),
            q: FloatParam::new("q", "Q", 0.707, Q_MIN, Q_MAX),
        }
    }
}

pub struct LpfPlugin {
    params: Arc<LpfParams>,
    filter: Biquad,
    sample_rate: f32,
}

impl Default for LpfPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(LpfParams::default()),
            filter: Biquad::default(),
            sample_rate: 44100.0,
        }
    }
}

impl SunmaoPlugin for LpfPlugin {
    const NAME: &'static str = "SunMao LPF GL";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";

    type Params = LpfParams;

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

    fn initialize(&mut self, sample_rate: f64, _max_frames: u32) {
        self.sample_rate = sample_rate as f32;
        self.filter.update(
            self.sample_rate,
            self.params.cutoff.get(),
            self.params.q.get(),
        );
    }

    fn reset(&mut self) {
        self.filter.reset();
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        _events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        let cutoff = self.params.cutoff.get();
        let q = self.params.q.get();
        self.filter.update(self.sample_rate, cutoff, q);
        self.filter.process(buffer);
        ProcessStatus::Normal
    }

    fn view(&self) -> Option<Box<dyn SunmaoView>> {
        let config = BaseviewConfig {
            title: "SunMao LPF".to_string(),
            width: 420,
            height: 160,
            scale_policy: WindowScalePolicy::SystemScaleFactor,
            background: Color::rgb(0.12, 0.12, 0.18),
        };

        let view = BaseviewView::new(config, |context| LpfViewState::new(context, 420.0, 160.0));
        Some(Box::new(view))
    }
}

struct LpfViewState {
    freq_slider: Slider,
    q_slider: Slider,
    context: Arc<dyn ViewContext>,
    editing_freq: bool,
    editing_q: bool,
}

impl LpfViewState {
    fn new(context: Arc<dyn ViewContext>, width: f32, height: f32) -> Self {
        let freq_default = normalize(1000.0, FREQ_MIN, FREQ_MAX);
        let q_default = normalize(0.707, Q_MIN, Q_MAX);
        let freq_slider = Slider::new("cutoff").with_default(freq_default);
        let q_slider = Slider::new("q").with_default(q_default);

        let mut state = Self {
            freq_slider,
            q_slider,
            context,
            editing_freq: false,
            editing_q: false,
        };
        state.relayout(width, height);
        state
    }

    fn relayout(&mut self, width: f32, height: f32) {
        let slider_height = 24.0;
        let margin = 20.0;
        let width = (width - margin * 2.0).max(1.0);

        let top_y = (height * 0.33 - slider_height * 0.5).max(4.0);
        let bottom_y = (height * 0.66 - slider_height * 0.5).max(4.0);

        self.freq_slider
            .set_bounds(Rect::new(margin, top_y, width, slider_height));
        self.q_slider
            .set_bounds(Rect::new(margin, bottom_y, width, slider_height));
    }

    fn sync_from_params(&mut self) {
        if let Some(value) = self.context.get_param("cutoff") {
            self.freq_slider.set_value(value);
        }
        if let Some(value) = self.context.get_param("q") {
            self.q_slider.set_value(value);
        }
    }

    fn update_editing(
        context: &Arc<dyn ViewContext>,
        id: &str,
        handled: bool,
        editing: &mut bool,
        event: &GuiEvent,
    ) {
        match event {
            GuiEvent::MouseDown {
                button: GuiMouseButton::Left,
                ..
            } if handled => {
                *editing = true;
                context.begin_edit(id);
            }
            GuiEvent::MouseUp {
                button: GuiMouseButton::Left,
                ..
            } if *editing => {
                *editing = false;
                context.end_edit(id);
            }
            _ => {}
        }
    }
}

impl ViewState for LpfViewState {
    fn draw(&mut self, ctx: &mut GlContext, width: f32, height: f32) {
        self.sync_from_params();
        ctx.fill_rect(
            0.0,
            0.0,
            width,
            height,
            Fill::Solid(Color::rgb(0.14, 0.14, 0.2)),
        );
        self.freq_slider.draw(ctx);
        self.q_slider.draw(ctx);
    }

    fn on_mouse_event(&mut self, event: &GuiEvent) -> bool {
        let freq_before = self.freq_slider.value();
        let freq_handled = self.freq_slider.handle_event(event);
        let freq_after = self.freq_slider.value();

        if freq_handled && (freq_after - freq_before).abs() > f32::EPSILON {
            self.context.set_param("cutoff", freq_after);
        }

        Self::update_editing(
            &self.context,
            "cutoff",
            freq_handled,
            &mut self.editing_freq,
            event,
        );

        if freq_handled {
            return true;
        }

        let q_before = self.q_slider.value();
        let q_handled = self.q_slider.handle_event(event);
        let q_after = self.q_slider.value();

        if q_handled && (q_after - q_before).abs() > f32::EPSILON {
            self.context.set_param("q", q_after);
        }

        Self::update_editing(&self.context, "q", q_handled, &mut self.editing_q, event);

        q_handled
    }

    fn on_resize(&mut self, width: f32, height: f32) {
        self.relayout(width, height);
    }
}

// ============ VST3 Export ============
use sunmao_backend_vst3::SunmaoVst3Wrapper;
sunmao_backend_vst3::export_vst3_plugin_with_gui!(SunmaoVst3Wrapper<LpfPlugin>);

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

    const PARAM_CUTOFF_INDEX: u32 = 0;
    const PARAM_Q_INDEX: u32 = 1;
    const SLIDER_MARGIN: f32 = 20.0;
    const SLIDER_HEIGHT: f32 = 24.0;

    const AU_OPENGL_CONFIG: GuiConfig = GuiConfig {
        factory_class: "SunmaoAUCocoaViewFactoryLPFGL",
        view_class: "SunmaoAUCocoaViewLPFGL",
        view_superclass: "NSOpenGLView",
        description: "SunMao AU LPF GL",
    };

    fn normalize_freq(value: f32) -> f32 {
        normalize(value, FREQ_MIN, FREQ_MAX)
    }

    fn denormalize_freq(norm: f32) -> f32 {
        denormalize(norm, FREQ_MIN, FREQ_MAX)
    }

    fn normalize_q(value: f32) -> f32 {
        normalize(value, Q_MIN, Q_MAX)
    }

    fn denormalize_q(norm: f32) -> f32 {
        denormalize(norm, Q_MIN, Q_MAX)
    }

    struct AuLpfOpenGlGui {
        cutoff_slider: Slider,
        q_slider: Slider,
        width: f32,
        height: f32,
        gl: Option<GlContext>,
    }

    impl Default for AuLpfOpenGlGui {
        fn default() -> Self {
            let cutoff_slider = Slider::new("param0").with_default(normalize_freq(1000.0));
            let q_slider = Slider::new("param1").with_default(normalize_q(0.707));
            Self {
                cutoff_slider,
                q_slider,
                width: 420.0,
                height: 160.0,
                gl: None,
            }
        }
    }

    impl AuLpfOpenGlGui {
        fn update_slider_bounds(&mut self) {
            let width = (self.width - SLIDER_MARGIN * 2.0).max(1.0);
            let top_y = (self.height * 0.33 - SLIDER_HEIGHT * 0.5).max(4.0);
            let bottom_y = (self.height * 0.66 - SLIDER_HEIGHT * 0.5).max(4.0);
            self.cutoff_slider
                .set_bounds(Rect::new(SLIDER_MARGIN, top_y, width, SLIDER_HEIGHT));
            self.q_slider
                .set_bounds(Rect::new(SLIDER_MARGIN, bottom_y, width, SLIDER_HEIGHT));
        }

        fn refresh_from_au(&mut self, audio_unit: *mut c_void) {
            if audio_unit.is_null() {
                return;
            }
            if let Ok(value) = get_parameter_local(audio_unit, PARAM_CUTOFF_INDEX) {
                self.cutoff_slider.set_value(normalize_freq(value));
            }
            if let Ok(value) = get_parameter_local(audio_unit, PARAM_Q_INDEX) {
                self.q_slider.set_value(normalize_q(value));
            }
        }

        fn handle_event(&mut self, event: GuiEvent, audio_unit: *mut c_void) -> bool {
            let cutoff_before = self.cutoff_slider.value();
            let cutoff_handled = self.cutoff_slider.handle_event(&event);
            let cutoff_after = self.cutoff_slider.value();

            if cutoff_handled && (cutoff_after - cutoff_before).abs() > f32::EPSILON {
                let value = denormalize_freq(cutoff_after);
                if !audio_unit.is_null() {
                    let _ = set_parameter_local(audio_unit, PARAM_CUTOFF_INDEX, value);
                }
                return true;
            }

            let q_before = self.q_slider.value();
            let q_handled = self.q_slider.handle_event(&event);
            let q_after = self.q_slider.value();

            if q_handled && (q_after - q_before).abs() > f32::EPSILON {
                let value = denormalize_q(q_after);
                if !audio_unit.is_null() {
                    let _ = set_parameter_local(audio_unit, PARAM_Q_INDEX, value);
                }
                return true;
            }

            cutoff_handled || q_handled
        }
    }

    impl GuiHandler for AuLpfOpenGlGui {
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
            if ctx.is_null() {
                return;
            }
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
                self.cutoff_slider.draw(gl);
                self.q_slider.draw(gl);

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
            self.handle_event(event, audio_unit);
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
            self.cutoff_slider.handle_event(&event);
            self.q_slider.handle_event(&event);
            set_needs_display(view);
        }
    }

    const AU_INFO: PluginInfo = PluginInfo {
        name: "SunMao LPF GL",
        component_type: fourcc(b"aufx"),
        component_subtype: fourcc(b"slpf"),
        manufacturer: fourcc(b"SunM"),
        version: 0x00010000,
        flags: 0,
        flags_mask: 0,
        input_channels: 2,
        output_channels: 2,
        supports_midi: false,
    };

    sunmao_backend_au::sunmao_export_au!(
        SunMaoLpfGlFactory,
        LpfPlugin,
        AU_INFO,
        au_params::<LpfParams>(),
        gui: { handler: AuLpfOpenGlGui, config: AU_OPENGL_CONFIG }
    );
}

// ============ CLAP Export ============
mod clap_export {
    use super::*;
    use std::ffi::c_char;
    use sunmao_backend_clap::SunmaoClapWrapper;
    use sunmao_backend_clap::{export_clap_plugin_with_gui, ClapFeature, ClapFeatures, PluginInfo};

    static PLUGIN_INFO: PluginInfo = PluginInfo {
        id: "com.sunmao.fx.lpf.gl\0",
        name: "SunMao LPF GL\0",
        vendor: "aizcutei\0",
        url: "https://aizcutei.github.io/sunmao\0",
        manual_url: "\0",
        support_url: "\0",
        version: "1.0.0\0",
        description: "Lowpass filter with OpenGL GUI\0",
    };

    const FEATURES_LIST: [*const c_char; 3] = [
        ClapFeature::AudioEffect.as_ptr(),
        ClapFeature::Filter.as_ptr(),
        std::ptr::null(),
    ];
    static FEATURES: ClapFeatures = ClapFeatures::new(&FEATURES_LIST);

    export_clap_plugin_with_gui!(SunmaoClapWrapper<LpfPlugin>, PLUGIN_INFO, FEATURES);
}
