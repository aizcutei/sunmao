//! Baseview Integration for SunMao Plugin Editors
//!
//! This crate provides the `BaseviewView` struct that implements `SunmaoView`
//! using baseview for cross-platform windowing and OpenGL support.

use std::sync::Arc;

use baseview::{
    Event, EventStatus, MouseEvent, ScrollDelta, WindowEvent,
    Window, WindowHandler, WindowOpenOptions,
    gl::GlConfig,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle, WindowHandle};

use sunmao_core::{
    SunmaoView, ParentWindow, ViewContext, ViewHandle,
};
use sunmao_gui::{
    Event as GuiEvent, MouseButton as GuiMouseButton, Modifiers, Color,
};
use sunmao_gui::gl::GlContext;

/// Wrapper to make ParentWindow implement HasWindowHandle
struct ParentWindowWrapper(ParentWindow);

impl HasWindowHandle for ParentWindowWrapper {
    fn window_handle(&self) -> Result<WindowHandle<'_>, raw_window_handle::HandleError> {
        use std::ptr::NonNull;
        
        let raw = match self.0 {
            ParentWindow::AppKit(ptr) => {
                let handle = raw_window_handle::AppKitWindowHandle::new(
                    NonNull::new(ptr).expect("null NSView")
                );
                RawWindowHandle::AppKit(handle)
            }
            ParentWindow::Win32(ptr) => {
                use std::num::NonZeroIsize;
                let isize_ptr = ptr as isize;
                let handle = raw_window_handle::Win32WindowHandle::new(
                    NonZeroIsize::new(isize_ptr).expect("null HWND")
                );
                RawWindowHandle::Win32(handle)
            }
            ParentWindow::X11(window) => {
                use std::num::NonZeroU32;
                let handle = raw_window_handle::XcbWindowHandle::new(
                    NonZeroU32::new(window).expect("null X11 window")
                );
                RawWindowHandle::Xcb(handle)
            }
        };
        
        // Safety: The parent window handle is valid for the lifetime of the plugin
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

/// A SunmaoView implementation using baseview for windowing.
///
/// Generic over `S: ViewState` which holds your custom UI state and widgets.
pub struct BaseviewView<S: ViewState + 'static> {
    config: BaseviewConfig,
    builder: ViewBuilder<S>,
}

impl<S: ViewState + 'static> BaseviewView<S> {
    /// Create a new baseview-backed editor view.
    ///
    /// The builder closure is called to create the view state when the editor opens.
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

/// Trait for custom view state that receives events and draws the UI.
pub trait ViewState: Send + 'static {
    /// Called each frame to draw the UI.
    fn draw(&mut self, ctx: &mut GlContext, width: f32, height: f32);
    
    /// Called when a mouse event occurs. Return true if handled.
    fn on_mouse_event(&mut self, event: &GuiEvent) -> bool;
    
    /// Called when a keyboard event occurs. Return true if handled.
    fn on_keyboard_event(&mut self, _event: &GuiEvent) -> bool { false }
    
    /// Called when the window is resized.
    fn on_resize(&mut self, _width: f32, _height: f32) {}
}

impl<S: ViewState + 'static> SunmaoView for BaseviewView<S> {
    fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }
    
    fn open(&self, parent: ParentWindow, context: Arc<dyn ViewContext>) -> Option<ViewHandle> {
        let config = self.config.clone();
        let state = (self.builder)(context.clone());
        
        let options = WindowOpenOptions {
            title: config.title.clone(),
            size: Size::new(config.width as f64, config.height as f64),
            scale: config.scale_policy,
            gl_config: Some(GlConfig::default()),
        };
        
        let parent_wrapper = ParentWindowWrapper(parent);
        let background = config.background;
        
        let handle = Window::open_parented(
            &parent_wrapper,
            options,
            move |window| {
                BaseviewHandler::new(state, background, window)
            },
        );
        
        Some(Box::new(handle))
    }
}

/// Internal window handler that bridges baseview to our ViewState.
struct BaseviewHandler<S: ViewState> {
    state: S,
    gl: Option<GlContext>,
    background: Color,
    width: f32,
    height: f32,
    scale_factor: f32,
    mouse_x: f32,
    mouse_y: f32,
}

impl<S: ViewState> BaseviewHandler<S> {
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
    
    fn convert_mouse_event(&self, event: &MouseEvent) -> Option<GuiEvent> {
        match event {
            MouseEvent::CursorMoved { position, modifiers } => {
                Some(GuiEvent::MouseMove {
                    x: position.x as f32,
                    y: position.y as f32,
                    modifiers: self.convert_modifiers(modifiers),
                })
            }
            MouseEvent::ButtonPressed { button, modifiers } => {
                Some(GuiEvent::MouseDown {
                    x: self.mouse_x,
                    y: self.mouse_y,
                    button: self.convert_button(*button),
                    modifiers: self.convert_modifiers(modifiers),
                })
            }
            MouseEvent::ButtonReleased { button, modifiers } => {
                Some(GuiEvent::MouseUp {
                    x: self.mouse_x,
                    y: self.mouse_y,
                    button: self.convert_button(*button),
                    modifiers: self.convert_modifiers(modifiers),
                })
            }
            MouseEvent::WheelScrolled { delta, modifiers } => {
                let (dx, dy) = match delta {
                    ScrollDelta::Lines { x, y } => (*x * 20.0, *y * 20.0),
                    ScrollDelta::Pixels { x, y } => (*x, *y),
                };
                Some(GuiEvent::Scroll {
                    x: self.mouse_x,
                    y: self.mouse_y,
                    delta_x: dx,
                    delta_y: dy,
                    modifiers: self.convert_modifiers(modifiers),
                })
            }
            _ => None,
        }
    }
    
    fn convert_button(&self, button: baseview::MouseButton) -> GuiMouseButton {
        match button {
            baseview::MouseButton::Left => GuiMouseButton::Left,
            baseview::MouseButton::Middle => GuiMouseButton::Middle,
            baseview::MouseButton::Right => GuiMouseButton::Right,
            _ => GuiMouseButton::Left,
        }
    }
    
    fn convert_modifiers(&self, mods: &keyboard_types::Modifiers) -> Modifiers {
        Modifiers {
            shift: mods.contains(keyboard_types::Modifiers::SHIFT),
            ctrl: mods.contains(keyboard_types::Modifiers::CONTROL),
            alt: mods.contains(keyboard_types::Modifiers::ALT),
            meta: mods.contains(keyboard_types::Modifiers::META),
        }
    }
}

impl<S: ViewState> WindowHandler for BaseviewHandler<S> {
    fn on_frame(&mut self, window: &mut Window) {
        if let Some(gl_ctx) = window.gl_context() {
            unsafe { gl_ctx.make_current(); }
            
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
                // Track mouse position
                if let MouseEvent::CursorMoved { position, .. } = mouse_event {
                    self.mouse_x = position.x as f32;
                    self.mouse_y = position.y as f32;
                }
                
                if let Some(gui_event) = self.convert_mouse_event(mouse_event) {
                    if self.state.on_mouse_event(&gui_event) {
                        return EventStatus::Captured;
                    }
                }
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

// Re-exports for convenience
pub use baseview::{WindowScalePolicy, Size};
// Removed conflicting/unused re-exports
