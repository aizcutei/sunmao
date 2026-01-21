//! Editor/View types for SunMao plugins.
//!
//! This module defines the `SunmaoView` trait which is the core abstraction
//! for plugin GUIs that can be embedded in DAW windows.

use std::any::Any;
use std::ffi::c_void;
use std::sync::Arc;

use crate::params::Params;

/// Parent window handle for embedding GUI in DAW editor.
/// 
/// This is passed by the host when creating the plugin's editor view.
#[derive(Debug, Clone, Copy)]
pub enum ParentWindow {
    /// macOS NSView pointer
    AppKit(*mut c_void),
    /// Windows HWND handle
    Win32(*mut c_void),
    /// X11 window ID
    X11(u32),
}

unsafe impl Send for ParentWindow {}
unsafe impl Sync for ParentWindow {}

impl ParentWindow {
    /// Create from raw window handle based on platform type string (VST3 style)
    pub fn from_vst3(parent: *mut c_void, type_str: &str) -> Option<Self> {
        match type_str {
            "NSView" => Some(ParentWindow::AppKit(parent)),
            "HWND" => Some(ParentWindow::Win32(parent)),
            "X11EmbedWindowID" => Some(ParentWindow::X11(parent as u32)),
            _ => None,
        }
    }
    
    /// Create from AU view (always AppKit on macOS)
    #[cfg(target_os = "macos")]
    pub fn from_au(ns_view: *mut c_void) -> Self {
        ParentWindow::AppKit(ns_view)
    }
    
    /// Create from CLAP parent window
    pub fn from_clap(parent: *mut c_void, api: &str) -> Option<Self> {
        match api {
            "cocoa" => Some(ParentWindow::AppKit(parent)),
            "win32" => Some(ParentWindow::Win32(parent)),
            "x11" => Some(ParentWindow::X11(parent as u32)),
            _ => None,
        }
    }
}

/// Context provided to the editor for parameter access and host communication.
pub trait ViewContext: Send + Sync {
    /// Get normalized parameter value by ID
    fn get_param(&self, id: &str) -> Option<f32>;
    
    /// Set normalized parameter value by ID (will notify host)
    fn set_param(&self, id: &str, value: f32);
    
    /// Begin parameter edit gesture (for automation recording)
    fn begin_edit(&self, id: &str);
    
    /// End parameter edit gesture
    fn end_edit(&self, id: &str);
    
    /// Request the host to resize the editor window
    fn request_resize(&self, width: u32, height: u32) -> bool;
}

/// Handle returned by `SunmaoView::open()` that keeps the editor alive.
/// 
/// When this handle is dropped, the editor window will be closed.
/// Note: This is !Send because window handles cannot be sent across threads.
pub type ViewHandle = Box<dyn Any>;

/// Trait for plugin editor views.
/// 
/// This is the main abstraction for implementing DAW-embeddable GUIs.
/// Plugins that want custom UI should implement this trait.
/// 
/// The lifecycle is:
/// 1. Host calls `create()` to create the view instance
/// 2. Host calls `open(parent, context)` to embed and show the editor
/// 3. Editor runs, receiving events via the returned handle  
/// 4. When host closes editor, the ViewHandle is dropped
pub trait SunmaoView: Send + Sync {
    /// Get the initial size of the editor in logical pixels.
    fn size(&self) -> (u32, u32);
    
    /// Open the editor embedded in the parent window.
    /// 
    /// Returns a handle that keeps the editor alive. The editor
    /// will be closed when this handle is dropped.
    fn open(&self, parent: ParentWindow, context: Arc<dyn ViewContext>) -> Option<ViewHandle>;
    
    /// Called when the DPI scale factor changes.
    /// 
    /// On macOS, scaling is handled by the OS so this may not be called.
    /// On Windows/Linux, the editor should resize its content accordingly.
    fn set_scale_factor(&self, _factor: f32) -> bool {
        false
    }
    
    /// Called when a parameter value changed from the host.
    /// 
    /// This allows the editor to update its visual state.
    fn param_value_changed(&self, _id: &str, _normalized_value: f32) {}
}

/// Helper struct that implements ViewContext using a Params reference.
pub struct ParamsViewContext<P: Params> {
    params: Arc<P>,
    // In a real implementation, this would store host callbacks
    // for begin_edit, end_edit, request_resize, etc.
}

impl<P: Params> ParamsViewContext<P> {
    pub fn new(params: Arc<P>) -> Self {
        Self { params }
    }
}

impl<P: Params> ViewContext for ParamsViewContext<P> {
    fn get_param(&self, id: &str) -> Option<f32> {
        self.params.get_normalized(id)
    }
    
    fn set_param(&self, id: &str, value: f32) {
        self.params.set_normalized(id, value);
    }
    
    fn begin_edit(&self, _id: &str) {
        // Would notify host in real implementation
    }
    
    fn end_edit(&self, _id: &str) {
        // Would notify host in real implementation
    }
    
    fn request_resize(&self, _width: u32, _height: u32) -> bool {
        // Would request host resize in real implementation
        false
    }
}
