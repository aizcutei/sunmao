//! clap_rs Gain Plugin with GUI (baseview)
//!
//! Demonstrates how to use clap_rs with the GuiHandler trait for plugin UI.

use baseview::{
    Event, EventStatus, MouseButton, MouseEvent, PhySize, Point, Size, Window, WindowEvent,
    WindowHandler, WindowOpenOptions, WindowScalePolicy,
};
use clap_rs::{Plugin, PluginInfo, ParameterInfo, AudioPortInfo, HostHandle, CLAP_PROCESS_CONTINUE};
use clap_rs::ext::{GuiApi, GuiHandler, GuiResizeHints};
use clap_rs::process::ProcessContext;
use raw_window_handle::{
    HasWindowHandle, HasDisplayHandle, RawDisplayHandle, RawWindowHandle,
};
use softbuffer::{Context, Surface};
use std::ffi::{c_char, c_void};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const GUI_WIDTH: f64 = 400.0;
const GUI_HEIGHT: f64 = 120.0;
const SLIDER_MARGIN: f64 = 20.0;
const SLIDER_HEIGHT: f64 = 20.0;
const KNOB_WIDTH: f64 = 12.0;

// ======= Plugin Struct =======

struct GainGui {
    _host: HostHandle,
    gain_bits: Arc<AtomicU64>,
    gui_parent: Option<RawWindowHandle>,
    gui_handle: Option<baseview::WindowHandle>,
    gui_open: bool,
}

impl Plugin for GainGui {
    type AudioProcessor = ();

    fn new(host: HostHandle) -> Self {
        Self {
            _host: host,
            gain_bits: Arc::new(AtomicU64::new(1.0f64.to_bits())),
            gui_parent: None,
            gui_handle: None,
            gui_open: false,
        }
    }

    fn declare_parameters(&self) -> Vec<ParameterInfo> {
        vec![
            ParameterInfo {
                id: 0,
                name: "Gain".to_string(),
                module: "".to_string(),
                min_value: 0.0,
                max_value: 2.0,
                default_value: 1.0,
            }
        ]
    }
    
    fn audio_ports_config(&self) -> Vec<AudioPortInfo> {
        vec![
            AudioPortInfo {
                id: 0,
                name: "Main In".to_string(),
                channel_count: 2,
                is_main: true,
                is_input: true,
            },
            AudioPortInfo {
                id: 1,
                name: "Main Out".to_string(),
                channel_count: 2,
                is_main: true,
                is_input: false,
            },
        ]
    }

    fn get_parameter(&self, id: u32) -> f64 {
        if id == 0 {
            f64::from_bits(self.gain_bits.load(Ordering::Relaxed)).clamp(0.0, 2.0)
        } else {
            0.0
        }
    }

    fn set_parameter(&mut self, id: u32, value: f64) {
        if id == 0 {
            self.gain_bits.store(value.clamp(0.0, 2.0).to_bits(), Ordering::Relaxed);
        }
    }

    fn process(&mut self, mut ctx: ProcessContext) -> clap_rs::clap_process_status {
        let frames = ctx.frames_count as usize;
        let gain = f64::from_bits(self.gain_bits.load(Ordering::Relaxed)).clamp(0.0, 2.0) as f32;
        
        for (input, output) in ctx.audio_inputs.iter().zip(ctx.audio_outputs.iter_mut()) {
            let len = input.len().min(output.len()).min(frames);
            for i in 0..len {
                output[i] = input[i] * gain;
            }
        }
        
        CLAP_PROCESS_CONTINUE
    }
}

// ======= GUI Handler Implementation =======

impl GuiHandler for GainGui {
    fn is_api_supported(&self, api: GuiApi, is_floating: bool) -> bool {
        if is_floating { return false; }
        matches!(api, GuiApi::Cocoa | GuiApi::Win32 | GuiApi::X11)
    }

    fn preferred_api(&self) -> Option<(GuiApi, bool)> {
        #[cfg(target_os = "macos")]
        { Some((GuiApi::Cocoa, false)) }
        #[cfg(target_os = "windows")]
        { Some((GuiApi::Win32, false)) }
        #[cfg(all(unix, not(target_os = "macos")))]
        { Some((GuiApi::X11, false)) }
    }

    fn gui_create(&mut self, _api: GuiApi, _is_floating: bool) -> bool {
        self.gui_open = true;
        if self.gui_parent.is_some() {
            return self.open_gui_window();
        }
        true
    }

    fn gui_destroy(&mut self) {
        if let Some(mut handle) = self.gui_handle.take() {
            handle.close();
        }
        self.gui_parent = None;  // Clear parent so next cycle starts fresh
        self.gui_open = false;
    }

    fn gui_get_size(&self) -> Option<(u32, u32)> {
        Some((GUI_WIDTH as u32, GUI_HEIGHT as u32))
    }

    fn gui_can_resize(&self) -> bool { false }

    fn gui_get_resize_hints(&self) -> GuiResizeHints {
        GuiResizeHints {
            can_resize_horizontally: false,
            can_resize_vertically: false,
            preserve_aspect_ratio: true,
            aspect_ratio_width: GUI_WIDTH as u32,
            aspect_ratio_height: GUI_HEIGHT as u32,
        }
    }

    fn gui_set_size(&mut self, width: u32, height: u32) -> bool {
        width == GUI_WIDTH as u32 && height == GUI_HEIGHT as u32
    }

    fn gui_set_parent(&mut self, window: *mut c_void) -> bool {
        #[cfg(target_os = "macos")]
        {
            use raw_window_handle::AppKitWindowHandle;
            if let Some(ns_view) = std::ptr::NonNull::new(window) {
                let handle = AppKitWindowHandle::new(ns_view);
                self.gui_parent = Some(RawWindowHandle::AppKit(handle));
            } else {
                return false;
            }
        }
        #[cfg(target_os = "windows")]
        {
            use raw_window_handle::Win32WindowHandle;
            use std::num::NonZeroIsize;
            if let Some(hwnd) = NonZeroIsize::new(window as isize) {
                let handle = Win32WindowHandle::new(hwnd);
                self.gui_parent = Some(RawWindowHandle::Win32(handle));
            } else {
                return false;
            }
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            use raw_window_handle::XlibWindowHandle;
            let handle = XlibWindowHandle::new(window as u64);
            self.gui_parent = Some(RawWindowHandle::Xlib(handle));
        }

        if self.gui_parent.is_some() && self.gui_open {
            return self.open_gui_window();
        }
        self.gui_parent.is_some()
    }

    fn gui_show(&mut self) -> bool {
        self.gui_open = true;
        if self.gui_parent.is_some() {
            return self.open_gui_window();
        }
        false
    }

    fn gui_hide(&mut self) -> bool {
        // Don't close the window on hide - just mark it as not open
        // The window will be closed on gui_destroy
        self.gui_open = false;
        true
    }
}

impl GainGui {
    fn open_gui_window(&mut self) -> bool {
        if self.gui_handle.is_some() {
            return true;
        }
        let parent_handle = match &self.gui_parent {
            Some(handle) => *handle,
            None => return false,
        };

        struct ParentWindow(RawWindowHandle);
        impl HasWindowHandle for ParentWindow {
            fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
                Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(self.0) })
            }
        }

        let parent = ParentWindow(parent_handle);
        let options = WindowOpenOptions {
            title: "ClapRS Gain GUI".to_string(),
            size: Size::new(GUI_WIDTH, GUI_HEIGHT),
            scale: WindowScalePolicy::SystemScaleFactor,
        };
        let gain_bits = self.gain_bits.clone();
        let window_handle = Window::open_parented(&parent, options, move |window| {
            GainGuiHandler::new(window, gain_bits)
        });
        self.gui_handle = Some(window_handle);
        true
    }
}

// ======= Baseview Window Handler =======

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
    gain_bits: Arc<AtomicU64>,
    dragging: bool,
    cursor: Point,
}

impl GainGuiHandler {
    fn new(_window: &mut Window, gain_bits: Arc<AtomicU64>) -> Self {
        Self {
            _wrapped: None,
            _ctx: None,
            surface: None,
            physical_size: PhySize::new(GUI_WIDTH as u32, GUI_HEIGHT as u32),
            logical_size: Size::new(GUI_WIDTH, GUI_HEIGHT),
            gain_bits,
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

        // Platform-specific view preparation (handled by clap_rs)
        if clap_rs::gui::prepare_view(&mut raw_window).is_err() {
            return;
        }

        // Create wrapped window for softbuffer (needs owned reference with 'static lifetime)
        let wrapped = Rc::new(WrappedWindow {
            raw_window_handle: raw_window,
            raw_display_handle: raw_display,
        });

        let ctx = match Context::new(wrapped.clone()) {
            Ok(ctx) => ctx,
            Err(_) => return,
        };

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Surface::new(&ctx, wrapped.clone())
        }));

        if let Ok(Ok(mut s)) = result {
            if let (Some(width), Some(height)) = (
                NonZeroU32::new(self.physical_size.width),
                NonZeroU32::new(self.physical_size.height)
            ) {
                let _ = s.resize(width, height);
            }
            self.surface = Some(s);
            self._ctx = Some(ctx);
            self._wrapped = Some(wrapped);
        }
    }

    fn gain_value(&self) -> f64 {
        f64::from_bits(self.gain_bits.load(Ordering::Relaxed)).clamp(0.0, 2.0)
    }

    fn set_gain_from_position(&self, pos: Point) {
        let slider_width = (self.logical_size.width - 2.0 * SLIDER_MARGIN).max(1.0);
        let track_x = SLIDER_MARGIN;
        let normalized = ((pos.x - track_x) / slider_width).clamp(0.0, 1.0);
        let gain = normalized * 2.0;
        self.gain_bits.store(gain.to_bits(), Ordering::Relaxed);
    }

    fn slider_rect(&self) -> (f64, f64, f64, f64) {
        let slider_width = (self.logical_size.width - 2.0 * SLIDER_MARGIN).max(1.0);
        let x = SLIDER_MARGIN;
        let y = (self.logical_size.height * 0.5) - (SLIDER_HEIGHT * 0.5);
        (x, y, slider_width, SLIDER_HEIGHT)
    }

    fn knob_rect(&self, gain: f64) -> (f64, f64, f64, f64) {
        let (x, y, width, height) = self.slider_rect();
        let normalized = (gain / 2.0).clamp(0.0, 1.0);
        let knob_x = x + normalized * (width - KNOB_WIDTH);
        (knob_x, y - 6.0, KNOB_WIDTH, height + 12.0)
    }

    fn point_in_rect(pos: Point, rect: (f64, f64, f64, f64)) -> bool {
        let (x, y, w, h) = rect;
        pos.x >= x && pos.x <= x + w && pos.y >= y && pos.y <= y + h
    }

    fn draw(&mut self, window: &Window) {
        self.ensure_resources(window);
        
        let width = self.physical_size.width as usize;
        let height = self.physical_size.height as usize;
        if width == 0 || height == 0 { return; }

        let gain = self.gain_value();
        let track = self.slider_rect();
        let knob = self.knob_rect(gain);
        let scale_x = self.physical_size.width as f64 / self.logical_size.width.max(1.0);
        let scale_y = self.physical_size.height as f64 / self.logical_size.height.max(1.0);

        if let Some(surface) = &mut self.surface {
            if let Ok(mut buffer) = surface.buffer_mut() {
                buffer.fill(0xFF202020);
                fill_rect(&mut buffer, width, height, track, scale_x, scale_y, 0xFF3A3A3A);
                fill_rect(&mut buffer, width, height, knob, scale_x, scale_y, 0xFF1BA1E2);
                let _ = buffer.present();
            }
        }
    }
}

impl WindowHandler for GainGuiHandler {
    fn on_frame(&mut self, window: &mut Window) {
        self.draw(window);
    }

    fn on_event(&mut self, _window: &mut Window, event: Event) -> EventStatus {
        match event {
            Event::Window(WindowEvent::Resized(info)) => {
                let new_size = info.physical_size();
                self.physical_size = new_size;
                self.logical_size = info.logical_size();
                if let (Some(width), Some(height)) = (
                    NonZeroU32::new(new_size.width),
                    NonZeroU32::new(new_size.height)
                ) {
                    if let Some(surface) = &mut self.surface {
                        let _ = surface.resize(width, height);
                    }
                }
            }
            Event::Mouse(MouseEvent::CursorMoved { position, .. }) => {
                self.cursor = position;
                if self.dragging {
                    self.set_gain_from_position(position);
                }
            }
            Event::Mouse(MouseEvent::ButtonPressed { button: MouseButton::Left, .. }) => {
                if Self::point_in_rect(self.cursor, self.slider_rect()) {
                    self.dragging = true;
                    self.set_gain_from_position(self.cursor);
                }
            }
            Event::Mouse(MouseEvent::ButtonReleased { button: MouseButton::Left, .. }) => {
                self.dragging = false;
            }
            _ => {}
        }
        EventStatus::Captured
    }
}

fn fill_rect(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    rect: (f64, f64, f64, f64),
    scale_x: f64,
    scale_y: f64,
    color: u32,
) {
    let (x, y, w, h) = rect;
    let start_x = (x * scale_x).round().max(0.0) as usize;
    let start_y = (y * scale_y).round().max(0.0) as usize;
    let end_x = ((x + w) * scale_x).round().min(width as f64) as usize;
    let end_y = ((y + h) * scale_y).round().min(height as f64) as usize;
    for row in start_y..end_y {
        let row_start = row * width;
        for col in start_x..end_x {
            buffer[row_start + col] = color;
        }
    }
}

// ======= Export Plugin =======

use clap_rs::export_clap_plugin_with_gui;

struct SyncFeatures([*const c_char; 3]);
unsafe impl Sync for SyncFeatures {}

static FEATURES: SyncFeatures = SyncFeatures([
    b"audio-effect\0".as_ptr() as *const c_char,
    b"stereo\0".as_ptr() as *const c_char,
    std::ptr::null()
]);

export_clap_plugin_with_gui!(GainGui, PluginInfo {
    id: "com.sunmao.clap_rs.fx_gain_gui_baseview\0",
    name: "Clap Rs Fx Gain Gui Baseview\0",
    vendor: "aizcutei\0",
    url: "https://aizcutei.github.io/sunmao\0",
    manual_url: "https://aizcutei.github.io/sunmao/manual\0",
    support_url: "https://aizcutei.github.io/sunmao/support\0",
    version: "0.1\0",
    description: "A gain plugin with GUI using clap_rs\0",
}, FEATURES.0);
