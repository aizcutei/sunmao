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
            "X11EmbedWindowID" => u32::try_from(parent as usize).ok().map(ParentWindow::X11),
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
            "x11" => u32::try_from(parent as usize).ok().map(ParentWindow::X11),
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

/// A host-forwarded key event, in a form both plugin formats can express.
///
/// VST3 hosts forward keys to the editor through
/// `IPlugView::onKeyDown`/`onKeyUp`. CLAP has no equivalent — a CLAP editor's
/// own window receives keys straight from the OS — so this arrives only on the
/// VST3 path. See `docs/phase2/semantics.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewKey {
    /// The character the key produced, if any. `None` for pure navigation and
    /// modifier keys.
    pub character: Option<char>,
    /// Which key it was, in format-neutral terms.
    ///
    /// The raw code stays in the backend that understands it: VST3's codes are
    /// the SDK's own enumeration, and letting them travel this far would make
    /// every consumer depend on one format's numbering.
    pub code: ViewKeyCode,
    /// `true` for a press, `false` for a release.
    pub pressed: bool,
}

/// The keys an editor acts on, independent of plugin format.
///
/// Anything a backend cannot map is [`ViewKeyCode::Unknown`], which widgets
/// ignore — so the host keeps its own shortcut rather than having it silently
/// swallowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewKeyCode {
    Backspace,
    Tab,
    Enter,
    Escape,
    Space,
    End,
    Home,
    Left,
    Up,
    Right,
    Down,
    PageUp,
    PageDown,
    Unknown,
}

trait ErasedViewHandle {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn resize(&mut self, width: u32, height: u32) -> bool;
    fn is_resizable(&self) -> bool;
    fn set_scale(&mut self, factor: f32) -> bool;
    fn send_key(&mut self, key: ViewKey) -> bool;
}

struct StoredViewHandle<T> {
    value: T,
    resize: Option<fn(&mut T, u32, u32) -> bool>,
    set_scale: Option<fn(&mut T, f32) -> bool>,
    send_key: Option<fn(&mut T, ViewKey) -> bool>,
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

    fn set_scale(&mut self, factor: f32) -> bool {
        self.set_scale
            .map(|set_scale| set_scale(&mut self.value, factor))
            .unwrap_or(false)
    }

    fn send_key(&mut self, key: ViewKey) -> bool {
        self.send_key
            .map(|send_key| send_key(&mut self.value, key))
            .unwrap_or(false)
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

/// Options used when a view owns a top-level standalone window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StandaloneViewOptions {
    close_after_frames: Option<u32>,
}

impl StandaloneViewOptions {
    /// Keep the top-level editor open until the user closes it.
    pub const fn interactive() -> Self {
        Self {
            close_after_frames: None,
        }
    }

    /// Close after a small number of rendered frames for deterministic GUI smoke tests.
    pub const fn smoke() -> Self {
        Self {
            close_after_frames: Some(3),
        }
    }

    /// Number of frames after which the standalone window should close.
    pub const fn close_after_frames(self) -> Option<u32> {
        self.close_after_frames
    }
}

impl Default for StandaloneViewOptions {
    fn default() -> Self {
        Self::interactive()
    }
}

/// Result of running a top-level standalone editor window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StandaloneViewResult {
    /// This view implementation only supports embedding in a plugin host.
    Unsupported,
    /// The top-level window initialized and then closed normally.
    Closed,
    /// The native window or renderer could not initialize.
    Failed,
}

impl ViewHandle {
    /// Wrap an editor resource that does not support host-driven resizing.
    pub fn new<T: 'static>(value: T) -> Self {
        Self {
            inner: Box::new(StoredViewHandle {
                value,
                resize: None,
                set_scale: None,
                send_key: None,
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
                set_scale: None,
                send_key: None,
            }),
        }
    }

    /// Wrap an editor resource that also honours host-driven DPI scale changes.
    ///
    /// `resize` stays optional so a fixed-size editor can still accept a scale
    /// factor. An editor without a `set_scale` callback reports `false`, which
    /// both backends translate into "the plugin scales itself" rather than an
    /// error — the correct answer on macOS, where the OS owns backing scale.
    pub fn scalable<T: 'static>(
        value: T,
        resize: Option<fn(&mut T, width: u32, height: u32) -> bool>,
        set_scale: fn(&mut T, factor: f32) -> bool,
    ) -> Self {
        Self {
            inner: Box::new(StoredViewHandle {
                value,
                resize,
                set_scale: Some(set_scale),
                send_key: None,
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

    /// Build a handle with any combination of capabilities.
    ///
    /// The three-argument [`ViewHandle::scalable`] stops scaling as more
    /// host-driven operations appear; this replaces it for new code.
    pub fn builder<T: 'static>(value: T) -> ViewHandleBuilder<T> {
        ViewHandleBuilder {
            value,
            resize: None,
            set_scale: None,
            send_key: None,
        }
    }

    /// Forward a host key event to the editor.
    ///
    /// `false` means the editor did not use it, which the VST3 wrapper reports
    /// as `kResultFalse` so the host can apply its own shortcut instead —
    /// swallowing every key would break the DAW's keyboard.
    pub fn send_key(&mut self, key: ViewKey) -> bool {
        self.inner.send_key(key)
    }

    /// Apply a host-provided DPI scale factor.
    ///
    /// Non-finite and non-positive factors are rejected here so neither ABI
    /// wrapper has to repeat the check, and so a hostile host cannot scale an
    /// editor out of existence.
    pub fn set_scale(&mut self, factor: f32) -> bool {
        factor.is_finite() && factor > 0.0 && self.inner.set_scale(factor)
    }

    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        self.inner.as_any().downcast_ref()
    }

    pub fn downcast_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.inner.as_any_mut().downcast_mut()
    }
}

/// Incremental builder for [`ViewHandle`].
pub struct ViewHandleBuilder<T> {
    value: T,
    resize: Option<fn(&mut T, u32, u32) -> bool>,
    set_scale: Option<fn(&mut T, f32) -> bool>,
    send_key: Option<fn(&mut T, ViewKey) -> bool>,
}

impl<T: 'static> ViewHandleBuilder<T> {
    pub fn resizable(mut self, resize: fn(&mut T, u32, u32) -> bool) -> Self {
        self.resize = Some(resize);
        self
    }

    pub fn scalable(mut self, set_scale: fn(&mut T, f32) -> bool) -> Self {
        self.set_scale = Some(set_scale);
        self
    }

    pub fn keyboard(mut self, send_key: fn(&mut T, ViewKey) -> bool) -> Self {
        self.send_key = Some(send_key);
        self
    }

    pub fn build(self) -> ViewHandle {
        ViewHandle {
            inner: Box::new(StoredViewHandle {
                value: self.value,
                resize: self.resize,
                set_scale: self.set_scale,
                send_key: self.send_key,
            }),
        }
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

    /// Open the editor as an application-owned top-level window.
    ///
    /// Implementations block until the window closes. The default preserves
    /// compatibility with view adapters that only support hosted embedding.
    fn open_standalone(
        &self,
        _context: Arc<dyn ViewContext>,
        _options: StandaloneViewOptions,
    ) -> StandaloneViewResult {
        StandaloneViewResult::Unsupported
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
        scale: Option<f32>,
    }

    fn resize_test_editor(editor: &mut TestEditor, width: u32, height: u32) -> bool {
        editor.size = (width, height);
        true
    }

    fn scale_test_editor(editor: &mut TestEditor, factor: f32) -> bool {
        editor.scale = Some(factor);
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
    fn view_handle_validates_scale_before_reaching_the_editor() {
        // An editor without a scale callback reports false, which both backends
        // translate into "the plugin scales itself" rather than an error.
        let mut unscalable = ViewHandle::resizable(TestEditor::default(), resize_test_editor);
        assert!(!unscalable.set_scale(2.0));
        assert!(unscalable
            .downcast_ref::<TestEditor>()
            .unwrap()
            .scale
            .is_none());

        let mut scalable = ViewHandle::scalable(
            TestEditor::default(),
            Some(resize_test_editor),
            scale_test_editor,
        );
        assert!(scalable.set_scale(1.5));
        assert_eq!(
            scalable.downcast_ref::<TestEditor>().unwrap().scale,
            Some(1.5)
        );
        // Resizing still works alongside scaling.
        assert!(scalable.resize(640, 360));

        // Nonsense factors never reach the editor, so each wrapper does not
        // have to repeat the check.
        for bad in [0.0f32, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(!scalable.set_scale(bad), "factor {bad} should be rejected");
        }
        assert_eq!(
            scalable.downcast_ref::<TestEditor>().unwrap().scale,
            Some(1.5),
            "a rejected factor overwrote the last good one"
        );

        // A fixed-size editor may still accept a scale factor.
        let mut fixed_but_scalable =
            ViewHandle::scalable(TestEditor::default(), None, scale_test_editor);
        assert!(!fixed_but_scalable.is_resizable());
        assert!(fixed_but_scalable.set_scale(2.0));
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
            #[cfg(target_pointer_width = "64")]
            {
                let truncated = usize::MAX as *mut c_void;
                assert!(ParentWindow::from_vst3(truncated, "X11EmbedWindowID").is_none());
                assert!(ParentWindow::from_clap(truncated, "x11").is_none());
            }
        }
    }
}
