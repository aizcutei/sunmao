//! Baseview Integration for SunMao Plugin Editors
//!
//! This crate provides cross-platform view implementations that bridge
//! `SunmaoView` to baseview for windowing. Three rendering backends are
//! supported:
//!
//! - **OpenGL** (`gl` feature): `BaseviewView<S: ViewState>` — uses `GlContext`
//! - **wgpu** (`wgpu` feature): `BaseviewWgpuView<S: WgpuViewState>` — uses `WgpuContext`
//! - **WebView** (`webview` feature): `BaseviewWebView<S: WebViewState>`
//!
//! All three implement `SunmaoView`, so plugin authors can use them
//! interchangeably with `sunmao_export_au_with_view!` for unified
//! AU/VST3/CLAP GUI export.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use baseview::{
    Event, EventStatus, MouseEvent, ScrollDelta, Window, WindowEvent, WindowHandler,
    WindowOpenOptions,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle, WindowHandle};

use sunmao_core::{ParentWindow, SunmaoView, ViewContext, ViewHandle};
use sunmao_gui::{
    Color, Event as GuiEvent, KeyCode as GuiKeyCode, Modifiers, MouseButton as GuiMouseButton,
};

fn resize_baseview_window(handle: &mut baseview::WindowHandle, width: u32, height: u32) -> bool {
    handle.resize(baseview::Size::new(width as f64, height as f64));
    true
}

// ============ Shared Types ============

/// Wrapper to make ParentWindow implement HasWindowHandle
struct ParentWindowWrapper(ParentWindow);

impl HasWindowHandle for ParentWindowWrapper {
    fn window_handle(&self) -> Result<WindowHandle<'_>, raw_window_handle::HandleError> {
        use std::ptr::NonNull;

        let raw = match self.0 {
            ParentWindow::AppKit(ptr) => {
                let handle = raw_window_handle::AppKitWindowHandle::new(
                    NonNull::new(ptr).expect("null NSView"),
                );
                RawWindowHandle::AppKit(handle)
            }
            ParentWindow::Win32(ptr) => {
                use std::num::NonZeroIsize;
                let isize_ptr = ptr as isize;
                let handle = raw_window_handle::Win32WindowHandle::new(
                    NonZeroIsize::new(isize_ptr).expect("null HWND"),
                );
                RawWindowHandle::Win32(handle)
            }
            ParentWindow::X11(window) => {
                use std::num::NonZeroU32;
                let handle = raw_window_handle::XcbWindowHandle::new(
                    NonZeroU32::new(window).expect("null X11 window"),
                );
                RawWindowHandle::Xcb(handle)
            }
        };

        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

/// Configuration for a baseview-backed editor.
#[derive(Clone)]
pub struct BaseviewConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub scale_policy: WindowScalePolicy,
    pub background: Color,
}

impl Default for BaseviewConfig {
    fn default() -> Self {
        Self {
            title: "SunMao Editor".to_string(),
            width: 400,
            height: 300,
            scale_policy: WindowScalePolicy::SystemScaleFactor,
            background: Color::rgba(0.12, 0.12, 0.18, 1.0),
        }
    }
}

/// Type alias for the view builder function
pub type ViewBuilder<S> = Arc<dyn Fn(Arc<dyn ViewContext>) -> S + Send + Sync>;

// ============ Shared event conversion helpers ============

fn convert_mouse_event(event: &MouseEvent, mouse_x: f32, mouse_y: f32) -> Option<GuiEvent> {
    match event {
        MouseEvent::CursorMoved {
            position,
            modifiers,
        } => Some(GuiEvent::MouseMove {
            x: position.x as f32,
            y: position.y as f32,
            modifiers: convert_modifiers(modifiers),
        }),
        MouseEvent::ButtonPressed { button, modifiers } => Some(GuiEvent::MouseDown {
            x: mouse_x,
            y: mouse_y,
            button: convert_button(*button),
            modifiers: convert_modifiers(modifiers),
        }),
        MouseEvent::ButtonReleased { button, modifiers } => Some(GuiEvent::MouseUp {
            x: mouse_x,
            y: mouse_y,
            button: convert_button(*button),
            modifiers: convert_modifiers(modifiers),
        }),
        MouseEvent::WheelScrolled { delta, modifiers } => {
            let (dx, dy) = match delta {
                ScrollDelta::Lines { x, y } => (*x * 20.0, *y * 20.0),
                ScrollDelta::Pixels { x, y } => (*x, *y),
            };
            Some(GuiEvent::Scroll {
                x: mouse_x,
                y: mouse_y,
                delta_x: dx,
                delta_y: dy,
                modifiers: convert_modifiers(modifiers),
            })
        }
        _ => None,
    }
}

fn convert_button(button: baseview::MouseButton) -> GuiMouseButton {
    match button {
        baseview::MouseButton::Left => GuiMouseButton::Left,
        baseview::MouseButton::Middle => GuiMouseButton::Middle,
        baseview::MouseButton::Right => GuiMouseButton::Right,
        _ => GuiMouseButton::Left,
    }
}

fn convert_modifiers(mods: &keyboard_types::Modifiers) -> Modifiers {
    Modifiers {
        shift: mods.contains(keyboard_types::Modifiers::SHIFT),
        ctrl: mods.contains(keyboard_types::Modifiers::CONTROL),
        alt: mods.contains(keyboard_types::Modifiers::ALT),
        meta: mods.contains(keyboard_types::Modifiers::META),
    }
}

fn convert_key_code(code: keyboard_types::Code) -> GuiKeyCode {
    use keyboard_types::Code;

    match code {
        Code::KeyA => GuiKeyCode::A,
        Code::KeyB => GuiKeyCode::B,
        Code::KeyC => GuiKeyCode::C,
        Code::KeyD => GuiKeyCode::D,
        Code::KeyE => GuiKeyCode::E,
        Code::KeyF => GuiKeyCode::F,
        Code::KeyG => GuiKeyCode::G,
        Code::KeyH => GuiKeyCode::H,
        Code::KeyI => GuiKeyCode::I,
        Code::KeyJ => GuiKeyCode::J,
        Code::KeyK => GuiKeyCode::K,
        Code::KeyL => GuiKeyCode::L,
        Code::KeyM => GuiKeyCode::M,
        Code::KeyN => GuiKeyCode::N,
        Code::KeyO => GuiKeyCode::O,
        Code::KeyP => GuiKeyCode::P,
        Code::KeyQ => GuiKeyCode::Q,
        Code::KeyR => GuiKeyCode::R,
        Code::KeyS => GuiKeyCode::S,
        Code::KeyT => GuiKeyCode::T,
        Code::KeyU => GuiKeyCode::U,
        Code::KeyV => GuiKeyCode::V,
        Code::KeyW => GuiKeyCode::W,
        Code::KeyX => GuiKeyCode::X,
        Code::KeyY => GuiKeyCode::Y,
        Code::KeyZ => GuiKeyCode::Z,
        Code::Digit0 | Code::Numpad0 => GuiKeyCode::Num0,
        Code::Digit1 | Code::Numpad1 => GuiKeyCode::Num1,
        Code::Digit2 | Code::Numpad2 => GuiKeyCode::Num2,
        Code::Digit3 | Code::Numpad3 => GuiKeyCode::Num3,
        Code::Digit4 | Code::Numpad4 => GuiKeyCode::Num4,
        Code::Digit5 | Code::Numpad5 => GuiKeyCode::Num5,
        Code::Digit6 | Code::Numpad6 => GuiKeyCode::Num6,
        Code::Digit7 | Code::Numpad7 => GuiKeyCode::Num7,
        Code::Digit8 | Code::Numpad8 => GuiKeyCode::Num8,
        Code::Digit9 | Code::Numpad9 => GuiKeyCode::Num9,
        Code::F1 => GuiKeyCode::F1,
        Code::F2 => GuiKeyCode::F2,
        Code::F3 => GuiKeyCode::F3,
        Code::F4 => GuiKeyCode::F4,
        Code::F5 => GuiKeyCode::F5,
        Code::F6 => GuiKeyCode::F6,
        Code::F7 => GuiKeyCode::F7,
        Code::F8 => GuiKeyCode::F8,
        Code::F9 => GuiKeyCode::F9,
        Code::F10 => GuiKeyCode::F10,
        Code::F11 => GuiKeyCode::F11,
        Code::F12 => GuiKeyCode::F12,
        Code::Escape => GuiKeyCode::Escape,
        Code::Tab => GuiKeyCode::Tab,
        Code::Space => GuiKeyCode::Space,
        Code::Enter | Code::NumpadEnter => GuiKeyCode::Enter,
        Code::Backspace | Code::NumpadBackspace => GuiKeyCode::Backspace,
        Code::Delete => GuiKeyCode::Delete,
        Code::ArrowLeft => GuiKeyCode::Left,
        Code::ArrowRight => GuiKeyCode::Right,
        Code::ArrowUp => GuiKeyCode::Up,
        Code::ArrowDown => GuiKeyCode::Down,
        Code::Home => GuiKeyCode::Home,
        Code::End => GuiKeyCode::End,
        Code::PageUp => GuiKeyCode::PageUp,
        Code::PageDown => GuiKeyCode::PageDown,
        Code::ShiftLeft | Code::ShiftRight => GuiKeyCode::Shift,
        Code::ControlLeft | Code::ControlRight => GuiKeyCode::Control,
        Code::AltLeft | Code::AltRight => GuiKeyCode::Alt,
        Code::MetaLeft | Code::MetaRight => GuiKeyCode::Meta,
        _ => GuiKeyCode::Unknown,
    }
}

fn dispatch_keyboard_event(
    event: &keyboard_types::KeyboardEvent,
    handler: impl FnOnce(&GuiEvent) -> bool,
) -> EventStatus {
    let key = convert_key_code(event.code);
    let modifiers = convert_modifiers(&event.modifiers);
    let gui_event = match event.state {
        keyboard_types::KeyState::Down => GuiEvent::KeyDown { key, modifiers },
        keyboard_types::KeyState::Up => GuiEvent::KeyUp { key, modifiers },
    };

    if handler(&gui_event) {
        EventStatus::Captured
    } else {
        EventStatus::Ignored
    }
}

// ============ OpenGL Backend ============

#[cfg(feature = "gl")]
mod gl_backend {
    use super::*;
    use baseview::gl::GlConfig;
    use sunmao_gui::gl::GlContext;

    /// Trait for custom view state that receives events and draws the UI using OpenGL.
    pub trait ViewState: Send + 'static {
        fn draw(&mut self, ctx: &mut GlContext, width: f32, height: f32);
        fn on_mouse_event(&mut self, event: &GuiEvent) -> bool;
        fn on_keyboard_event(&mut self, _event: &GuiEvent) -> bool {
            false
        }
        fn on_resize(&mut self, _width: f32, _height: f32) {}
    }

    /// A SunmaoView implementation using baseview + OpenGL.
    pub struct BaseviewView<S: ViewState + 'static> {
        config: BaseviewConfig,
        builder: ViewBuilder<S>,
    }

    impl<S: ViewState + 'static> BaseviewView<S> {
        pub fn new<B>(config: BaseviewConfig, builder: B) -> Self
        where
            B: Fn(Arc<dyn ViewContext>) -> S + Send + Sync + 'static,
        {
            Self {
                config,
                builder: Arc::new(builder),
            }
        }
    }

    impl<S: ViewState + 'static> SunmaoView for BaseviewView<S> {
        fn size(&self) -> (u32, u32) {
            (self.config.width, self.config.height)
        }

        fn can_resize(&self) -> bool {
            true
        }

        fn open(&self, parent: ParentWindow, context: Arc<dyn ViewContext>) -> Option<ViewHandle> {
            let config = self.config.clone();
            let state = (self.builder)(context.clone());

            let mut options = WindowOpenOptions::new(
                config.title.clone(),
                baseview::Size::new(config.width as f64, config.height as f64),
                config.scale_policy,
            );
            options.gl_config = Some(GlConfig::default());

            let parent_wrapper = ParentWindowWrapper(parent);
            let background = config.background;

            let initialized = Arc::new(AtomicBool::new(false));
            let initialized_in_builder = Arc::clone(&initialized);
            let mut handle = Window::open_parented(&parent_wrapper, options, move |window| {
                let handler = GlHandler::new(state, background, window);
                if handler.gl.is_some() {
                    initialized_in_builder.store(true, Ordering::Release);
                } else {
                    window.close();
                }
                handler
            });

            if initialized.load(Ordering::Acquire) && handle.is_open() {
                Some(ViewHandle::resizable(handle, resize_baseview_window))
            } else {
                handle.close();
                None
            }
        }
    }

    struct GlHandler<S: ViewState> {
        state: S,
        gl: Option<GlContext>,
        background: Color,
        width: f32,
        height: f32,
        scale_factor: f32,
        mouse_x: f32,
        mouse_y: f32,
    }

    impl<S: ViewState> GlHandler<S> {
        fn new(state: S, background: Color, window: &mut Window) -> Self {
            let gl = if let Some(gl_ctx) = window.gl_context() {
                unsafe {
                    gl_ctx.make_current();
                    match GlContext::from_loader(|s| gl_ctx.get_proc_address(s), 400.0, 300.0) {
                        Ok(ctx) => Some(ctx),
                        Err(e) => {
                            eprintln!("Failed to create GL context: {}", e);
                            None
                        }
                    }
                }
            } else {
                None
            };

            Self {
                state,
                gl,
                background,
                width: 400.0,
                height: 300.0,
                scale_factor: 1.0,
                mouse_x: 0.0,
                mouse_y: 0.0,
            }
        }
    }

    impl<S: ViewState> WindowHandler for GlHandler<S> {
        fn on_frame(&mut self, window: &mut Window) {
            if let Some(gl_ctx) = window.gl_context() {
                unsafe {
                    gl_ctx.make_current();
                }

                if let Some(ref mut gl) = self.gl {
                    let physical_width = (self.width * self.scale_factor) as u32;
                    let physical_height = (self.height * self.scale_factor) as u32;

                    gl.set_viewport(physical_width, physical_height);
                    gl.set_scale(self.scale_factor);
                    gl.clear(self.background);
                    gl.begin_frame();

                    self.state.draw(gl, self.width, self.height);

                    gl.end_frame();
                }

                gl_ctx.swap_buffers();
            }
        }

        fn on_event(&mut self, _window: &mut Window, event: Event) -> EventStatus {
            match event {
                Event::Mouse(ref mouse_event) => {
                    if let MouseEvent::CursorMoved { position, .. } = mouse_event {
                        self.mouse_x = position.x as f32;
                        self.mouse_y = position.y as f32;
                    }

                    if let Some(gui_event) =
                        convert_mouse_event(mouse_event, self.mouse_x, self.mouse_y)
                    {
                        if self.state.on_mouse_event(&gui_event) {
                            return EventStatus::Captured;
                        }
                    }
                }
                Event::Keyboard(ref keyboard_event) => {
                    return dispatch_keyboard_event(keyboard_event, |gui_event| {
                        self.state.on_keyboard_event(gui_event)
                    });
                }
                Event::Window(WindowEvent::Resized(info)) => {
                    self.width = info.logical_size().width as f32;
                    self.height = info.logical_size().height as f32;
                    self.scale_factor = info.scale() as f32;
                    self.state.on_resize(self.width, self.height);
                }
                _ => {}
            }

            EventStatus::Ignored
        }
    }
}

#[cfg(feature = "gl")]
pub use gl_backend::{BaseviewView, ViewState};

// ============ wgpu Backend ============

#[cfg(feature = "wgpu")]
mod wgpu_backend {
    use super::*;
    use sunmao_gui::wgpu::WgpuContext;

    /// Trait for custom view state that draws using wgpu.
    pub trait WgpuViewState: Send + 'static {
        fn draw(&mut self, ctx: &mut WgpuContext, width: f32, height: f32);
        fn on_mouse_event(&mut self, event: &GuiEvent) -> bool;
        fn on_keyboard_event(&mut self, _event: &GuiEvent) -> bool {
            false
        }
        fn on_resize(&mut self, _width: f32, _height: f32) {}
    }

    /// A SunmaoView implementation using baseview + wgpu.
    pub struct BaseviewWgpuView<S: WgpuViewState + 'static> {
        config: BaseviewConfig,
        builder: ViewBuilder<S>,
    }

    impl<S: WgpuViewState + 'static> BaseviewWgpuView<S> {
        pub fn new<B>(config: BaseviewConfig, builder: B) -> Self
        where
            B: Fn(Arc<dyn ViewContext>) -> S + Send + Sync + 'static,
        {
            Self {
                config,
                builder: Arc::new(builder),
            }
        }
    }

    impl<S: WgpuViewState + 'static> SunmaoView for BaseviewWgpuView<S> {
        fn size(&self) -> (u32, u32) {
            (self.config.width, self.config.height)
        }

        fn can_resize(&self) -> bool {
            true
        }

        fn open(&self, parent: ParentWindow, context: Arc<dyn ViewContext>) -> Option<ViewHandle> {
            let config = self.config.clone();
            let state = (self.builder)(context.clone());

            let options = WindowOpenOptions::new(
                config.title.clone(),
                baseview::Size::new(config.width as f64, config.height as f64),
                config.scale_policy,
            );

            let parent_wrapper = ParentWindowWrapper(parent);
            let background = config.background;
            let width = config.width;
            let height = config.height;

            let initialized = Arc::new(AtomicBool::new(false));
            let initialized_in_builder = Arc::clone(&initialized);
            let mut handle = Window::open_parented(&parent_wrapper, options, move |window| {
                match pollster::block_on(WgpuHandler::new(state, background, width, height, window))
                {
                    Some(handler) => {
                        initialized_in_builder.store(true, Ordering::Release);
                        WgpuHandlerState::Ready(handler)
                    }
                    None => {
                        window.close();
                        WgpuHandlerState::Failed
                    }
                }
            });

            if initialized.load(Ordering::Acquire) && handle.is_open() {
                Some(ViewHandle::resizable(handle, resize_baseview_window))
            } else {
                handle.close();
                None
            }
        }
    }

    enum WgpuHandlerState<S: WgpuViewState> {
        Ready(WgpuHandler<S>),
        Failed,
    }

    struct WgpuHandler<S: WgpuViewState> {
        state: S,
        wgpu_ctx: WgpuContext,
        surface: wgpu::Surface<'static>,
        surface_config: wgpu::SurfaceConfiguration,
        background: Color,
        width: f32,
        height: f32,
        scale_factor: f32,
        mouse_x: f32,
        mouse_y: f32,
    }

    impl<S: WgpuViewState> WgpuHandler<S> {
        async fn new(
            state: S,
            background: Color,
            width: u32,
            height: u32,
            window: &mut Window<'_>,
        ) -> Option<Self> {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: wgpu::util::backend_bits_from_env()
                    .filter(|backends| !backends.is_empty())
                    .unwrap_or(wgpu::Backends::all()),
                ..Default::default()
            });
            // The baseview child window outlives the handler and its surface. Using
            // raw handles lets wgpu select Metal, DX12/Vulkan, or Vulkan/GL without
            // coupling this backend to a platform-specific layer type.
            let target = unsafe { wgpu::SurfaceTargetUnsafe::from_window(window) }.ok()?;
            let surface = unsafe { instance.create_surface_unsafe(target) }.ok()?;

            let adapter_options = wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            };
            let adapter = match instance.request_adapter(&adapter_options).await {
                Some(adapter) => adapter,
                None => {
                    // Hosted/headless Windows runners may expose only the
                    // software WARP adapter. Keep the same surface constraint
                    // while allowing that adapter as a deterministic fallback.
                    instance
                        .request_adapter(&wgpu::RequestAdapterOptions {
                            force_fallback_adapter: true,
                            ..adapter_options
                        })
                        .await?
                }
            };

            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default(), None)
                .await
                .ok()?;

            let width = width.max(1);
            let height = height.max(1);

            let surface_caps = surface.get_capabilities(&adapter);
            let surface_format = surface_caps
                .formats
                .iter()
                .find(|f| f.is_srgb())
                .copied()
                .unwrap_or(surface_caps.formats[0]);

            let surface_config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: surface_format,
                width,
                height,
                present_mode: wgpu::PresentMode::AutoVsync,
                alpha_mode: surface_caps.alpha_modes[0],
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };
            surface.configure(&device, &surface_config);

            let wgpu_ctx =
                WgpuContext::new(device, queue, surface_format, width as f32, height as f32);

            Some(Self {
                state,
                wgpu_ctx,
                surface,
                surface_config,
                background,
                width: width as f32,
                height: height as f32,
                scale_factor: 1.0,
                mouse_x: 0.0,
                mouse_y: 0.0,
            })
        }

        fn configure_surface(&mut self) {
            let pw = (self.width * self.scale_factor).max(1.0) as u32;
            let ph = (self.height * self.scale_factor).max(1.0) as u32;
            if pw > 0 && ph > 0 {
                self.surface_config.width = pw;
                self.surface_config.height = ph;
                self.surface
                    .configure(self.wgpu_ctx.device(), &self.surface_config);
                // Drawing coordinates are logical pixels. The surface alone uses
                // physical pixels, otherwise high-DPI views render at half size.
                self.wgpu_ctx.resize(self.width, self.height);
                self.wgpu_ctx.set_scale(self.scale_factor);
            }
        }
    }

    impl<S: WgpuViewState> WindowHandler for WgpuHandler<S> {
        fn on_frame(&mut self, _window: &mut Window) {
            let output = match self.surface.get_current_texture() {
                Ok(t) => t,
                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                    self.configure_surface();
                    return;
                }
                Err(_) => return,
            };

            let view = output
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());

            self.wgpu_ctx.begin_frame();
            self.state.draw(&mut self.wgpu_ctx, self.width, self.height);
            self.wgpu_ctx.end_frame_with_clear(&view, self.background);

            output.present();
        }

        fn on_event(&mut self, _window: &mut Window, event: Event) -> EventStatus {
            match event {
                Event::Mouse(ref mouse_event) => {
                    if let MouseEvent::CursorMoved { position, .. } = mouse_event {
                        self.mouse_x = position.x as f32;
                        self.mouse_y = position.y as f32;
                    }

                    if let Some(gui_event) =
                        convert_mouse_event(mouse_event, self.mouse_x, self.mouse_y)
                    {
                        if self.state.on_mouse_event(&gui_event) {
                            return EventStatus::Captured;
                        }
                    }
                }
                Event::Keyboard(ref keyboard_event) => {
                    return dispatch_keyboard_event(keyboard_event, |gui_event| {
                        self.state.on_keyboard_event(gui_event)
                    });
                }
                Event::Window(WindowEvent::Resized(info)) => {
                    self.width = info.logical_size().width as f32;
                    self.height = info.logical_size().height as f32;
                    self.scale_factor = info.scale() as f32;
                    self.configure_surface();
                    self.state.on_resize(self.width, self.height);
                }
                _ => {}
            }

            EventStatus::Ignored
        }
    }

    impl<S: WgpuViewState> WindowHandler for WgpuHandlerState<S> {
        fn on_frame(&mut self, window: &mut Window) {
            match self {
                Self::Ready(handler) => handler.on_frame(window),
                Self::Failed => window.close(),
            }
        }

        fn on_event(&mut self, window: &mut Window, event: Event) -> EventStatus {
            match self {
                Self::Ready(handler) => handler.on_event(window, event),
                Self::Failed => EventStatus::Ignored,
            }
        }
    }
}

#[cfg(feature = "wgpu")]
pub use wgpu_backend::{BaseviewWgpuView, WgpuViewState};

// ============ WebView Backend ============

#[cfg(feature = "webview")]
mod webview_backend {
    use super::*;
    use baseview::webview::WebView;
    use std::sync::mpsc;

    /// Trait for view state that communicates with a platform WebView.
    pub trait WebViewState: Send + 'static {
        /// Return the HTML content to load.
        fn html(&self) -> &str;

        /// Called when JavaScript sends a message via the message handler.
        fn on_message(&mut self, message: &str, context: &dyn ViewContext);

        /// Called when the view is resized.
        fn on_resize(&mut self, _width: f32, _height: f32) {}
    }

    /// A SunmaoView implementation using baseview + the platform WebView.
    pub struct BaseviewWebView<S: WebViewState + 'static> {
        config: BaseviewConfig,
        builder: ViewBuilder<S>,
        message_handler_name: String,
    }

    impl<S: WebViewState + 'static> BaseviewWebView<S> {
        pub fn new<B>(config: BaseviewConfig, builder: B, message_handler_name: &str) -> Self
        where
            B: Fn(Arc<dyn ViewContext>) -> S + Send + Sync + 'static,
        {
            Self {
                config,
                builder: Arc::new(builder),
                message_handler_name: message_handler_name.to_string(),
            }
        }
    }

    impl<S: WebViewState + 'static> SunmaoView for BaseviewWebView<S> {
        fn size(&self) -> (u32, u32) {
            (self.config.width, self.config.height)
        }

        fn can_resize(&self) -> bool {
            true
        }

        fn open(&self, parent: ParentWindow, context: Arc<dyn ViewContext>) -> Option<ViewHandle> {
            let config = self.config.clone();
            let state = (self.builder)(context.clone());
            let handler_name = self.message_handler_name.clone();

            let options = WindowOpenOptions::new(
                config.title.clone(),
                baseview::Size::new(config.width as f64, config.height as f64),
                config.scale_policy,
            );

            let parent_wrapper = ParentWindowWrapper(parent);

            let initialized = Arc::new(AtomicBool::new(false));
            let initialized_in_builder = Arc::clone(&initialized);
            let mut handle = Window::open_parented(&parent_wrapper, options, move |window| {
                match WebView::new(
                    window,
                    config.width as f64,
                    config.height as f64,
                    &handler_name,
                    state.html(),
                ) {
                    Ok((webview, receiver)) => {
                        initialized_in_builder.store(true, Ordering::Release);
                        WebviewHandlerState::Ready(WebviewHandler {
                            state,
                            context,
                            webview,
                            receiver,
                            width: config.width as f32,
                            height: config.height as f32,
                        })
                    }
                    Err(error) => {
                        eprintln!("Failed to create WebView: {error}");
                        window.close();
                        WebviewHandlerState::Failed
                    }
                }
            });

            if initialized.load(Ordering::Acquire) && handle.is_open() {
                Some(ViewHandle::resizable(handle, resize_baseview_window))
            } else {
                handle.close();
                None
            }
        }
    }

    struct WebviewHandler<S: WebViewState> {
        state: S,
        context: Arc<dyn ViewContext>,
        webview: WebView,
        receiver: mpsc::Receiver<String>,
        width: f32,
        height: f32,
    }

    impl<S: WebViewState> WindowHandler for WebviewHandler<S> {
        fn on_frame(&mut self, _window: &mut Window) {
            self.webview.poll_events();
            while let Ok(msg) = self.receiver.try_recv() {
                self.state.on_message(&msg, self.context.as_ref());
            }
        }

        fn on_event(&mut self, _window: &mut Window, event: Event) -> EventStatus {
            match event {
                Event::Window(WindowEvent::Resized(info)) => {
                    self.width = info.logical_size().width as f32;
                    self.height = info.logical_size().height as f32;
                    if let Err(error) = self.webview.set_size(self.width as f64, self.height as f64)
                    {
                        eprintln!("Failed to resize WebView: {error}");
                    }
                    self.state.on_resize(self.width, self.height);
                }
                _ => {}
            }
            EventStatus::Ignored
        }
    }

    enum WebviewHandlerState<S: WebViewState> {
        Ready(WebviewHandler<S>),
        Failed,
    }

    impl<S: WebViewState> WindowHandler for WebviewHandlerState<S> {
        fn on_frame(&mut self, window: &mut Window) {
            match self {
                Self::Ready(handler) => handler.on_frame(window),
                Self::Failed => window.close(),
            }
        }

        fn on_event(&mut self, window: &mut Window, event: Event) -> EventStatus {
            match self {
                Self::Ready(handler) => handler.on_event(window, event),
                Self::Failed => EventStatus::Ignored,
            }
        }
    }
}

#[cfg(feature = "webview")]
pub use webview_backend::{BaseviewWebView, WebViewState};

// Re-exports for convenience
pub use baseview::{Size, WindowScalePolicy};

#[cfg(test)]
mod tests {
    use super::*;
    use keyboard_types::{Code, Key, KeyState, KeyboardEvent, Location};

    fn keyboard_event(
        state: KeyState,
        code: Code,
        modifiers: keyboard_types::Modifiers,
    ) -> KeyboardEvent {
        KeyboardEvent {
            state,
            key: Key::Unidentified,
            code,
            location: Location::Standard,
            modifiers,
            repeat: false,
            is_composing: false,
        }
    }

    #[test]
    fn keyboard_dispatch_converts_down_events_and_capture_status() {
        let event = keyboard_event(
            KeyState::Down,
            Code::KeyA,
            keyboard_types::Modifiers::SHIFT | keyboard_types::Modifiers::CONTROL,
        );
        let mut called = false;

        let status = dispatch_keyboard_event(&event, |event| {
            called = true;
            match event {
                GuiEvent::KeyDown { key, modifiers } => {
                    assert_eq!(*key, GuiKeyCode::A);
                    assert!(modifiers.shift);
                    assert!(modifiers.ctrl);
                    assert!(!modifiers.alt);
                    assert!(!modifiers.meta);
                }
                _ => panic!("expected key-down event"),
            }
            true
        });

        assert!(called);
        assert_eq!(status, EventStatus::Captured);
    }

    #[test]
    fn keyboard_dispatch_converts_up_events_and_ignored_status() {
        let event = keyboard_event(KeyState::Up, Code::Escape, keyboard_types::Modifiers::META);

        let status = dispatch_keyboard_event(&event, |event| {
            match event {
                GuiEvent::KeyUp { key, modifiers } => {
                    assert_eq!(*key, GuiKeyCode::Escape);
                    assert!(modifiers.meta);
                }
                _ => panic!("expected key-up event"),
            }
            false
        });

        assert_eq!(status, EventStatus::Ignored);
    }

    #[test]
    fn keyboard_code_conversion_covers_numpad_and_unknown_keys() {
        assert_eq!(convert_key_code(Code::Numpad7), GuiKeyCode::Num7);
        assert_eq!(convert_key_code(Code::NumpadEnter), GuiKeyCode::Enter);
        assert_eq!(convert_key_code(Code::Unidentified), GuiKeyCode::Unknown);
    }
}
