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
    /// Create from raw window handle based on platform type string (VST3 style).
    pub fn from_vst3(parent: *mut c_void, type_str: &str) -> Option<Self> {
        if parent.is_null() {
            return None;
        }
        match type_str {
            #[cfg(target_os = "macos")]
            "NSView" => Some(ParentWindow::AppKit(parent)),
            #[cfg(target_os = "windows")]
            "HWND" => Some(ParentWindow::Win32(parent)),
            #[cfg(target_os = "linux")]
            "X11EmbedWindowID" => Some(ParentWindow::X11(parent as u32)),
            _ => None,
        }
    }

    /// Create from AU view (always AppKit on macOS)
    #[cfg(target_os = "macos")]
    pub fn from_au(ns_view: *mut c_void) -> Self {
        ParentWindow::AppKit(ns_view)
    }

    /// Create from CLAP parent window.
    pub fn from_clap(parent: *mut c_void, api: &str) -> Option<Self> {
        if parent.is_null() {
            return None;
        }
        match api {
            #[cfg(target_os = "macos")]
            "cocoa" => Some(ParentWindow::AppKit(parent)),
            #[cfg(target_os = "windows")]
            "win32" => Some(ParentWindow::Win32(parent)),
            #[cfg(target_os = "linux")]
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

trait ErasedViewHandle {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn resize(&mut self, width: u32, height: u32) -> bool;
    fn is_resizable(&self) -> bool;
}

struct StoredViewHandle<T> {
    value: T,
    resize: Option<fn(&mut T, u32, u32) -> bool>,
}

impl<T: 'static> ErasedViewHandle for StoredViewHandle<T> {
    fn as_any(&self) -> &dyn Any {
        &self.value
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        &mut self.value
    }

    fn resize(&mut self, width: u32, height: u32) -> bool {
        self.resize
            .map(|resize| resize(&mut self.value, width, height))
            .unwrap_or(false)
    }

    fn is_resizable(&self) -> bool {
        self.resize.is_some()
    }
}

/// Handle returned by [`SunmaoView::open`] that owns the native editor.
///
/// The handle is intentionally not `Send`: native editor resources stay on
/// their UI thread. Backends can request a host-negotiated resize without
/// knowing which GUI implementation owns the handle.
pub struct ViewHandle {
    inner: Box<dyn ErasedViewHandle>,
}

impl ViewHandle {
    /// Wrap an editor resource that does not support host-driven resizing.
    pub fn new<T: 'static>(value: T) -> Self {
        Self {
            inner: Box::new(StoredViewHandle {
                value,
                resize: None,
            }),
        }
    }

    /// Wrap an editor resource with its host-driven resize operation.
    pub fn resizable<T: 'static>(
        value: T,
        resize: fn(&mut T, width: u32, height: u32) -> bool,
    ) -> Self {
        Self {
            inner: Box::new(StoredViewHandle {
                value,
                resize: Some(resize),
            }),
        }
    }

    /// Resize the native editor in logical pixels.
    pub fn resize(&mut self, width: u32, height: u32) -> bool {
        width > 0 && height > 0 && self.inner.resize(width, height)
    }

    pub fn is_resizable(&self) -> bool {
        self.inner.is_resizable()
    }

    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        self.inner.as_any().downcast_ref()
    }

    pub fn downcast_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.inner.as_any_mut().downcast_mut()
    }
}

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

    /// Whether the editor accepts host-driven size changes.
    fn can_resize(&self) -> bool {
        false
    }

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

#[cfg(test)]
mod tests {
    use super::{ParentWindow, ViewHandle};
    use std::ffi::c_void;

    #[derive(Default)]
    struct TestEditor {
        size: (u32, u32),
    }

    fn resize_test_editor(editor: &mut TestEditor, width: u32, height: u32) -> bool {
        editor.size = (width, height);
        true
    }

    #[test]
    fn view_handle_erases_ownership_and_preserves_resize_capability() {
        let mut fixed = ViewHandle::new(TestEditor::default());
        assert!(!fixed.is_resizable());
        assert!(!fixed.resize(640, 360));
        assert_eq!(fixed.downcast_ref::<TestEditor>().unwrap().size, (0, 0));

        let mut resizable = ViewHandle::resizable(TestEditor::default(), resize_test_editor);
        assert!(resizable.is_resizable());
        assert!(resizable.resize(640, 360));
        assert_eq!(
            resizable.downcast_ref::<TestEditor>().unwrap().size,
            (640, 360)
        );
        assert!(!resizable.resize(0, 360));
    }

    #[test]
    fn parent_window_constructors_reject_null_and_foreign_apis() {
        let non_null = std::ptr::dangling_mut::<c_void>();
        assert!(ParentWindow::from_vst3(std::ptr::null_mut(), "NSView").is_none());
        assert!(ParentWindow::from_clap(std::ptr::null_mut(), "cocoa").is_none());
        assert!(ParentWindow::from_vst3(non_null, "foreign").is_none());
        assert!(ParentWindow::from_clap(non_null, "foreign").is_none());

        #[cfg(target_os = "macos")]
        {
            assert!(matches!(
                ParentWindow::from_vst3(non_null, "NSView"),
                Some(ParentWindow::AppKit(_))
            ));
            assert!(matches!(
                ParentWindow::from_clap(non_null, "cocoa"),
                Some(ParentWindow::AppKit(_))
            ));
            assert!(ParentWindow::from_vst3(non_null, "HWND").is_none());
        }
        #[cfg(target_os = "windows")]
        {
            assert!(matches!(
                ParentWindow::from_vst3(non_null, "HWND"),
                Some(ParentWindow::Win32(_))
            ));
            assert!(matches!(
                ParentWindow::from_clap(non_null, "win32"),
                Some(ParentWindow::Win32(_))
            ));
            assert!(ParentWindow::from_vst3(non_null, "NSView").is_none());
        }
        #[cfg(target_os = "linux")]
        {
            assert!(matches!(
                ParentWindow::from_vst3(non_null, "X11EmbedWindowID"),
                Some(ParentWindow::X11(_))
            ));
            assert!(matches!(
                ParentWindow::from_clap(non_null, "x11"),
                Some(ParentWindow::X11(_))
            ));
            assert!(ParentWindow::from_vst3(non_null, "NSView").is_none());
        }
    }
}
