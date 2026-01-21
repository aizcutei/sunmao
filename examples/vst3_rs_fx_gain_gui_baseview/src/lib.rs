//! VST3 Gain Effect with GUI using vst3_rs wrapper
//!
//! A gain plugin with a baseview GUI slider, demonstrating
//! the GuiPlugin trait from vst3_rs.

use baseview::{
    Event, EventStatus, MouseButton, MouseEvent, PhySize, Point, Size, Window, WindowEvent,
    WindowHandler, WindowOpenOptions, WindowScalePolicy,
};
use raw_window_handle::{
    HasWindowHandle, HasDisplayHandle, RawDisplayHandle, RawWindowHandle,
};
use softbuffer::{Context, Surface};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use vst3_rs::*;

const GUI_WIDTH: u32 = 400;
const GUI_HEIGHT: u32 = 120;
const SLIDER_MARGIN: f64 = 20.0;
const SLIDER_HEIGHT: f64 = 20.0;
const KNOB_WIDTH: f64 = 12.0;

/// Shared gain state between plugin and GUI - stored statically so processor and controller share
struct SharedState {
    gain_bits: AtomicU64,
}

impl SharedState {
    fn get(&self) -> f64 { f64::from_bits(self.gain_bits.load(Ordering::Relaxed)) }
    fn set(&self, v: f64) { self.gain_bits.store(v.to_bits(), Ordering::Relaxed); }
}

/// Global shared state - ensures processor and controller share the same state
static SHARED_STATE: std::sync::OnceLock<Arc<SharedState>> = std::sync::OnceLock::new();

fn get_shared_state() -> Arc<SharedState> {
    SHARED_STATE.get_or_init(|| {
        Arc::new(SharedState { gain_bits: AtomicU64::new(0.5_f64.to_bits()) })
    }).clone()
}

/// Gain Plugin with GUI
struct MyGuiGain {
    state: Arc<SharedState>,
}

impl Plugin for MyGuiGain {
    fn info() -> PluginInfo {
        PluginInfo {
            id: "com.sunmao.vst3_rs_fx_gain_gui_baseview",
            name: "Vst3 Rs Fx Gain Gui Baseview",
            vendor: "aizcutei",
            url: "https://aizcutei.github.io/sunmao",
            email: "info@example.com",
            version: "0.1.0",
            category: "Fx",
        }
    }
    
    fn new(_host: HostHandle) -> Self {
        Self { state: get_shared_state() }
    }
    
    fn params() -> Vec<ParamInfo> {
        vec![ParamInfo::new(0, "Gain").range(0.0, 2.0).default(0.5).units("")]
    }
    
    fn get_param(&self, _id: u32) -> f64 { self.state.get() / 2.0 }
    fn set_param(&mut self, _id: u32, value: f64) { self.state.set(value * 2.0); }
    
    fn process(&mut self, ctx: &mut ProcessContext) {
        let gain = self.state.get() as f32;
        let num_samples = ctx.num_samples;
        
        for ch in 0..ctx.num_outputs().min(ctx.num_inputs()) {
            let input_copy: Vec<f32> = ctx.input(ch).iter().take(num_samples).copied().collect();
            let output = ctx.output_mut(ch);
            for (i, o) in input_copy.iter().zip(output.iter_mut()) {
                *o = *i * gain;
            }
        }
    }
}

impl GuiPlugin for MyGuiGain {
    fn gui_size() -> gui::GuiSize { gui::GuiSize::new(GUI_WIDTH, GUI_HEIGHT) }
    
    fn gui_create(&mut self, parent: RawWindowHandle) -> bool {
        struct ParentWindow(RawWindowHandle);
        impl HasWindowHandle for ParentWindow {
            fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
                Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(self.0) })
            }
        }
        
        let options = WindowOpenOptions {
            title: "Gain".into(),
            size: Size::new(GUI_WIDTH as f64, GUI_HEIGHT as f64),
            scale: WindowScalePolicy::SystemScaleFactor,
        };
        
        let state = self.state.clone();
        // NOTE: We don't store this handle - the parented window is managed by the host
        let _handle = Window::open_parented(&ParentWindow(parent), options, move |window| {
            GainGuiHandler::new(window, state)
        });
        true
    }
    
    fn gui_destroy(&mut self) {
        // Window is destroyed by host - nothing to do here
    }
}

// GUI Handler

/// Wrapper to hold window handles for softbuffer 0.4 lifetime requirements
struct WrappedWindow {
    raw_window_handle: RawWindowHandle,
    raw_display_handle: RawDisplayHandle,
}

impl HasWindowHandle for WrappedWindow {
    fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(self.raw_window_handle) })
    }
}

impl HasDisplayHandle for WrappedWindow {
    fn display_handle(&self) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        Ok(unsafe { raw_window_handle::DisplayHandle::borrow_raw(self.raw_display_handle) })
    }
}

struct GainGuiHandler {
    _wrapped: Option<Rc<WrappedWindow>>,
    _ctx: Option<Context<Rc<WrappedWindow>>>,
    surface: Option<Surface<Rc<WrappedWindow>, Rc<WrappedWindow>>>,
    physical_size: PhySize,
    logical_size: Size,
    state: Arc<SharedState>,
    dragging: bool,
    cursor: Point,
}

impl GainGuiHandler {
    fn new(_window: &mut Window, state: Arc<SharedState>) -> Self {
        Self {
            _wrapped: None,
            _ctx: None,
            surface: None,
            physical_size: PhySize::new(GUI_WIDTH, GUI_HEIGHT),
            logical_size: Size::new(GUI_WIDTH as f64, GUI_HEIGHT as f64),
            state,
            dragging: false,
            cursor: Point::new(0.0, 0.0),
        }
    }

    fn ensure_resources(&mut self, window: &Window) {
        if self.surface.is_some() { return; }
        
        // Extract raw handles from baseview window (raw-window-handle 0.6)
        let Ok(window_handle) = window.window_handle() else { return; };
        let Ok(display_handle) = window.display_handle() else { return; };
        
        let mut raw_window = window_handle.as_raw();
        let raw_display = display_handle.as_raw();
        
        // Platform-specific view preparation (handled by vst3_rs)
        if vst3_rs::gui::prepare_view(&mut raw_window).is_err() {
            return;
        }
        
        // Create wrapped window for softbuffer (needs owned reference with 'static lifetime)
        let wrapped = Rc::new(WrappedWindow {
            raw_window_handle: raw_window,
            raw_display_handle: raw_display,
        });
        
        let ctx = match Context::new(wrapped.clone()) { Ok(c) => c, Err(_) => return };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Surface::new(&ctx, wrapped.clone())));
        if let Ok(Ok(mut s)) = result {
            if let (Some(w), Some(h)) = (NonZeroU32::new(self.physical_size.width), NonZeroU32::new(self.physical_size.height)) {
                let _ = s.resize(w, h);
            }
            self.surface = Some(s);
            self._ctx = Some(ctx);
            self._wrapped = Some(wrapped);
        }
    }

    fn gain_value(&self) -> f64 { self.state.get().clamp(0.0, 2.0) }

    fn set_gain_from_position(&self, pos: Point) {
        let slider_width = (self.logical_size.width - 2.0 * SLIDER_MARGIN).max(1.0);
        let normalized = ((pos.x - SLIDER_MARGIN) / slider_width).clamp(0.0, 1.0);
        self.state.set(normalized * 2.0);
    }

    fn slider_rect(&self) -> (f64, f64, f64, f64) {
        let w = (self.logical_size.width - 2.0 * SLIDER_MARGIN).max(1.0);
        (SLIDER_MARGIN, (self.logical_size.height * 0.5) - (SLIDER_HEIGHT * 0.5), w, SLIDER_HEIGHT)
    }

    fn knob_rect(&self, gain: f64) -> (f64, f64, f64, f64) {
        let (x, y, w, h) = self.slider_rect();
        let norm = (gain / 2.0).clamp(0.0, 1.0);
        (x + norm * (w - KNOB_WIDTH), y - 6.0, KNOB_WIDTH, h + 12.0)
    }

    fn point_in_rect(pos: Point, r: (f64, f64, f64, f64)) -> bool {
        pos.x >= r.0 && pos.x <= r.0 + r.2 && pos.y >= r.1 && pos.y <= r.1 + r.3
    }

    fn draw(&mut self, window: &Window) {
        self.ensure_resources(window);
        let (w, h) = (self.physical_size.width as usize, self.physical_size.height as usize);
        if w == 0 || h == 0 { return; }

        let gain = self.gain_value();
        let (sx, sy) = (self.physical_size.width as f64 / self.logical_size.width.max(1.0),
                        self.physical_size.height as f64 / self.logical_size.height.max(1.0));
        
        // Compute rects before borrowing surface
        let track = self.slider_rect();
        let knob = self.knob_rect(gain);

        if let Some(surface) = &mut self.surface {
            if let Ok(mut buffer) = surface.buffer_mut() {
                buffer.fill(0xFF202020);
                fill_rect(&mut buffer, w, h, track, sx, sy, 0xFF3A3A3A);
                fill_rect(&mut buffer, w, h, knob, sx, sy, 0xFF1BA1E2);
                let _ = buffer.present();
            }
        }
    }
}

impl WindowHandler for GainGuiHandler {
    fn on_frame(&mut self, window: &mut Window) { self.draw(window); }

    fn on_event(&mut self, _window: &mut Window, event: Event) -> EventStatus {
        match event {
            Event::Window(WindowEvent::Resized(info)) => {
                self.physical_size = info.physical_size();
                self.logical_size = info.logical_size();
                if let (Some(w), Some(h)) = (NonZeroU32::new(self.physical_size.width), NonZeroU32::new(self.physical_size.height)) {
                    if let Some(s) = &mut self.surface { let _ = s.resize(w, h); }
                }
            }
            Event::Mouse(MouseEvent::CursorMoved { position, .. }) => {
                self.cursor = position;
                if self.dragging { self.set_gain_from_position(position); }
            }
            Event::Mouse(MouseEvent::ButtonPressed { button: MouseButton::Left, .. }) => {
                if Self::point_in_rect(self.cursor, self.slider_rect()) {
                    self.dragging = true;
                    self.set_gain_from_position(self.cursor);
                }
            }
            Event::Mouse(MouseEvent::ButtonReleased { button: MouseButton::Left, .. }) => self.dragging = false,
            _ => {}
        }
        EventStatus::Captured
    }
}

fn fill_rect(buf: &mut [u32], w: usize, h: usize, r: (f64, f64, f64, f64), sx: f64, sy: f64, c: u32) {
    let (x0, y0) = ((r.0 * sx).round().max(0.0) as usize, (r.1 * sy).round().max(0.0) as usize);
    let (x1, y1) = (((r.0 + r.2) * sx).round().min(w as f64) as usize, ((r.1 + r.3) * sy).round().min(h as f64) as usize);
    for row in y0..y1 { for col in x0..x1 { buf[row * w + col] = c; } }
}

export_vst3_plugin_with_gui!(MyGuiGain);
