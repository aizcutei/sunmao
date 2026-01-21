//! SunMao DAW Info GUI (OpenGL)
//!
//! Displays host transport info (tempo, playing state, sample position)
//! using a simple OpenGL-based UI. Audio is passed through (bypass).

use std::sync::{Arc, Mutex, OnceLock};
use sunmao_core::prelude::*;
use sunmao_gui::gl::GlContext;
use sunmao_gui::{Color, Fill, GuiContext, Stroke};
use sunmao_macros::Params;
use sunmao_view_baseview::{BaseviewConfig, BaseviewView, ViewState, WindowScalePolicy};

// ============ Shared Transport State ============

#[derive(Debug, Clone, Copy, Default)]
struct TransportSnapshot {
    tempo: Option<f64>,
    is_playing: bool,
    sample_pos: i64,
}

static TRANSPORT_STATE: OnceLock<Arc<Mutex<TransportSnapshot>>> = OnceLock::new();


fn shared_transport() -> Arc<Mutex<TransportSnapshot>> {
    TRANSPORT_STATE
        .get_or_init(|| Arc::new(Mutex::new(TransportSnapshot::default())))
        .clone()
}

fn read_snapshot(state: &Arc<Mutex<TransportSnapshot>>) -> TransportSnapshot {
    match state.lock() {
        Ok(guard) => *guard,
        Err(_) => TransportSnapshot::default(),
    }
}

// ============ Plugin Definition ============

#[derive(Params)]
pub struct DawInfoParams {
    #[unit = "Generic"]
    pub dummy: FloatParam,
}

impl Default for DawInfoParams {
    fn default() -> Self {
        Self {
            dummy: FloatParam::new("dummy", "Dummy", 0.0, 0.0, 1.0),
        }
    }
}

pub struct DawInfoPlugin {
    params: Arc<DawInfoParams>,
    transport: Arc<Mutex<TransportSnapshot>>,
}

impl Default for DawInfoPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(DawInfoParams::default()),
            transport: shared_transport(),
        }
    }
}

impl SunmaoPlugin for DawInfoPlugin {
    const NAME: &'static str = "SunMao DAW Info GUI";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";

    type Params = DawInfoParams;

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

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        _events: &EventQueue,
        context: &ProcessContext,
    ) -> ProcessStatus {
        if let Ok(mut state) = self.transport.lock() {
            state.tempo = context.tempo;
            state.is_playing = context.is_playing;
            state.sample_pos = context.sample_pos;
        }

        buffer.copy_input_to_output();
        ProcessStatus::Normal
    }

    fn view(&self) -> Option<Box<dyn SunmaoView>> {
        let config = BaseviewConfig {
            title: "SunMao DAW Info".to_string(),
            width: 520,
            height: 160,
            scale_policy: WindowScalePolicy::SystemScaleFactor,
            background: Color::rgb(0.12, 0.12, 0.18),
        };

        let transport = self.transport.clone();
        let view = BaseviewView::new(config, move |_| {
            DawInfoViewState::new(transport.clone(), 520.0, 160.0)
        });
        Some(Box::new(view))
    }
}

// ============ GUI (Baseview + OpenGL) ============

struct DawInfoViewState {
    transport: Arc<Mutex<TransportSnapshot>>,
    width: f32,
    height: f32,
}

impl DawInfoViewState {
    fn new(transport: Arc<Mutex<TransportSnapshot>>, width: f32, height: f32) -> Self {
        Self {
            transport,
            width,
            height,
        }
    }
}

impl ViewState for DawInfoViewState {
    fn draw(&mut self, ctx: &mut GlContext, width: f32, height: f32) {
        self.width = width.max(1.0);
        self.height = height.max(1.0);
        let snapshot = read_snapshot(&self.transport);
        draw_daw_info(ctx, self.width, self.height, snapshot);
    }

    fn on_mouse_event(&mut self, _event: &sunmao_gui::Event) -> bool {
        false
    }

    fn on_resize(&mut self, width: f32, height: f32) {
        self.width = width.max(1.0);
        self.height = height.max(1.0);
    }
}

// ============ Drawing Helpers ============

fn draw_daw_info(ctx: &mut GlContext, width: f32, height: f32, snapshot: TransportSnapshot) {
    ctx.fill_rect(
        0.0,
        0.0,
        width,
        height,
        Fill::Solid(Color::rgb(0.14, 0.14, 0.2)),
    );

    let padding = 16.0;
    let header_h = 40.0;
    let panel_w = width - padding * 2.0;
    let panel_h = height - padding * 2.0;

    ctx.stroke_rect(
        padding,
        padding,
        panel_w,
        panel_h,
        Stroke::new(Color::rgb(0.25, 0.25, 0.32), 1.0),
    );

    // Play indicator
    let play_color = if snapshot.is_playing {
        Color::rgb(0.2, 0.8, 0.45)
    } else {
        Color::rgb(0.9, 0.35, 0.35)
    };
    ctx.fill_circle(padding + 16.0, padding + 16.0, 8.0, Fill::Solid(play_color));
    ctx.stroke_circle(
        padding + 16.0,
        padding + 16.0,
        8.0,
        Stroke::new(Color::rgb(0.05, 0.05, 0.08), 2.0),
    );

    // Tempo area
    let tempo_x = padding + 40.0;
    let tempo_y = padding + 8.0;
    let tempo_w = 180.0;
    let tempo_h = header_h - 8.0;
    let tempo_value = snapshot.tempo.map(|t| t.round().clamp(0.0, 999.0) as u32);
    draw_number(
        ctx,
        tempo_x,
        tempo_y,
        tempo_w,
        tempo_h,
        3,
        tempo_value,
        false,
    );

    // Tempo bar
    let bar_x = tempo_x + tempo_w + 12.0;
    let bar_y = padding + 14.0;
    let bar_w = 140.0;
    let bar_h = 12.0;
    let tempo_norm = snapshot.tempo.unwrap_or(0.0).clamp(0.0, 240.0) / 240.0;
    ctx.stroke_rect(
        bar_x,
        bar_y,
        bar_w,
        bar_h,
        Stroke::new(Color::rgb(0.25, 0.25, 0.32), 1.0),
    );
    ctx.fill_rect(
        bar_x + 1.0,
        bar_y + 1.0,
        (bar_w - 2.0) * tempo_norm as f32,
        bar_h - 2.0,
        Fill::Solid(Color::rgb(0.25, 0.6, 1.0)),
    );

    // Sample position (last 6 digits)
    let pos_x = padding + 8.0;
    let pos_y = padding + header_h + 16.0;
    let pos_w = width - padding * 2.0 - 16.0;
    let pos_h = panel_h - header_h - 24.0;
    let pos_val = snapshot.sample_pos.abs() as u64 % 1_000_000;
    draw_number(
        ctx,
        pos_x,
        pos_y,
        pos_w,
        pos_h,
        6,
        Some(pos_val as u32),
        true,
    );
}

fn draw_number(
    ctx: &mut GlContext,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    digits: usize,
    value: Option<u32>,
    leading_zero: bool,
) {
    let gap = 8.0;
    let total_gap = gap * (digits.saturating_sub(1) as f32);
    let digit_w = ((width - total_gap) / digits as f32).max(8.0);
    let digit_h = height.max(16.0);

    let text = match value {
        Some(v) => {
            if leading_zero {
                format!("{:0width$}", v, width = digits)
            } else {
                format!("{:>width$}", v, width = digits)
            }
        }
        None => "-".repeat(digits),
    };

    for (idx, ch) in text.chars().take(digits).enumerate() {
        let dx = x + idx as f32 * (digit_w + gap);
        draw_digit(ctx, dx, y, digit_w, digit_h, ch);
    }
}

fn draw_digit(ctx: &mut GlContext, x: f32, y: f32, w: f32, h: f32, ch: char) {
    let on = Color::rgb(0.86, 0.9, 0.98);
    let off = Color::rgb(0.18, 0.2, 0.25);
    let t = (w.min(h) * 0.18).max(2.0);

    let seg = match ch {
        '0' => [true, true, true, false, true, true, true],
        '1' => [false, false, true, false, false, true, false],
        '2' => [true, false, true, true, true, false, true],
        '3' => [true, false, true, true, false, true, true],
        '4' => [false, true, true, true, false, true, false],
        '5' => [true, true, false, true, false, true, true],
        '6' => [true, true, false, true, true, true, true],
        '7' => [true, false, true, false, false, true, false],
        '8' => [true, true, true, true, true, true, true],
        '9' => [true, true, true, true, false, true, true],
        '-' => [false, false, false, true, false, false, false],
        ' ' => [false, false, false, false, false, false, false],
        _ => [false, false, false, false, false, false, false],
    };

    let half = h * 0.5;
    let top = (x + t, y, w - 2.0 * t, t);
    let mid = (x + t, y + half - t * 0.5, w - 2.0 * t, t);
    let bot = (x + t, y + h - t, w - 2.0 * t, t);

    let left_top_h = (half - t * 1.5).max(1.0);
    let left_bot_h = (half - t * 1.5).max(1.0);
    let top_left = (x, y + t, t, left_top_h);
    let bot_left = (x, y + half + t * 0.5, t, left_bot_h);
    let top_right = (x + w - t, y + t, t, left_top_h);
    let bot_right = (x + w - t, y + half + t * 0.5, t, left_bot_h);

    draw_seg(ctx, seg[0], top, on, off);
    draw_seg(ctx, seg[1], top_left, on, off);
    draw_seg(ctx, seg[2], top_right, on, off);
    draw_seg(ctx, seg[3], mid, on, off);
    draw_seg(ctx, seg[4], bot_left, on, off);
    draw_seg(ctx, seg[5], bot_right, on, off);
    draw_seg(ctx, seg[6], bot, on, off);
}

fn draw_seg(ctx: &mut GlContext, enabled: bool, rect: (f32, f32, f32, f32), on: Color, off: Color) {
    let (x, y, w, h) = rect;
    let color = if enabled { on } else { off };
    ctx.fill_rect(x, y, w, h, Fill::Solid(color));
}

// ============ VST3 Export ============
use sunmao_backend_vst3::SunmaoVst3Wrapper;
sunmao_backend_vst3::export_vst3_plugin_with_gui!(SunmaoVst3Wrapper<DawInfoPlugin>);

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
        au_params, fourcc, gl_get_proc_address, AudioComponentDescription,
        AudioComponentPlugInInterface, NSRect, NSSize, PluginInfo,
    };
    use sunmao_gui::gl::GlContext;
    use sunmao_gui::Color;

    const AU_OPENGL_CONFIG: GuiConfig = GuiConfig {
        factory_class: "SunmaoAUCocoaViewFactoryDawInfo",
        view_class: "SunmaoAUCocoaViewDawInfo",
        view_superclass: "NSOpenGLView",
        description: "SunMao AU DAW Info",
    };

    struct AuDawInfoOpenGlGui {
        width: f32,
        height: f32,
        gl: Option<GlContext>,
    }

    impl Default for AuDawInfoOpenGlGui {
        fn default() -> Self {
            Self {
                width: 520.0,
                height: 160.0,
                gl: None,
            }
        }
    }

    impl GuiHandler for AuDawInfoOpenGlGui {
        fn init(&mut self, view: *mut CocoaObject, size: NSSize, _audio_unit: *mut c_void) {
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
            update_open_gl_view(view);
            set_needs_display(view);
        }

        fn draw(&mut self, view: *mut CocoaObject, _audio_unit: *mut c_void, _rect: NSRect) {
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

            if let Some(gl) = self.gl.as_mut() {
                gl.set_scale(scale);
                gl.set_viewport(physical_w, physical_h);
                gl.clear(Color::rgb(0.12, 0.12, 0.18));
                gl.begin_frame();

                let snapshot = super::read_snapshot(&super::shared_transport());
                super::draw_daw_info(gl, self.width, self.height, snapshot);

                gl.end_frame();
            }

            flush_context(ctx);
        }

        fn reshape(&mut self, view: *mut CocoaObject, _audio_unit: *mut c_void) {
            update_open_gl_view(view);
            let bounds = view_bounds(view);
            self.width = bounds.size.width.max(1.0) as f32;
            self.height = bounds.size.height.max(1.0) as f32;
            set_needs_display(view);
        }
    }

    const AU_INFO: PluginInfo = PluginInfo {
        name: "SunMao DAW Info GUI",
        component_type: fourcc(b"aufx"),
        component_subtype: fourcc(b"smdi"),
        manufacturer: fourcc(b"SunM"),
        version: 0x00010000,
        flags: 0,
        flags_mask: 0,
        input_channels: 2,
        output_channels: 2,
        supports_midi: false,
    };

    sunmao_backend_au::sunmao_export_au!(
        SunMaoDawInfoFactoryInner,
        DawInfoPlugin,
        AU_INFO,
        au_params::<DawInfoParams>(),
        gui: { handler: AuDawInfoOpenGlGui, config: AU_OPENGL_CONFIG }
    );

    #[unsafe(no_mangle)]
    pub extern "C" fn SunMaoDawInfoFactory(
        in_desc: *const AudioComponentDescription,
    ) -> *mut AudioComponentPlugInInterface {
        let result = std::panic::catch_unwind(|| SunMaoDawInfoFactoryInner(in_desc));
        match result {
            Ok(ptr) => ptr,
            Err(_) => std::ptr::null_mut(),
        }
    }
}

// ============ CLAP Export ============
mod clap_export {
    use super::*;
    use std::ffi::c_char;
    use sunmao_backend_clap::SunmaoClapWrapper;
    use sunmao_backend_clap::{export_clap_plugin_with_gui, ClapFeature, ClapFeatures, PluginInfo};

    static PLUGIN_INFO: PluginInfo = PluginInfo {
        id: "com.sunmao.daw.info.gui\0",
        name: "SunMao DAW Info GUI\0",
        vendor: "aizcutei\0",
        url: "https://aizcutei.github.io/sunmao\0",
        manual_url: "\0",
        support_url: "\0",
        version: "1.0.0\0",
        description: "DAW transport info viewer (OpenGL GUI)\0",
    };

    const FEATURES_LIST: [*const c_char; 3] = [
        ClapFeature::Analyzer.as_ptr(),
        ClapFeature::Utility.as_ptr(),
        std::ptr::null(),
    ];
    static FEATURES: ClapFeatures = ClapFeatures::new(&FEATURES_LIST);

    export_clap_plugin_with_gui!(SunmaoClapWrapper<DawInfoPlugin>, PLUGIN_INFO, FEATURES);
}
