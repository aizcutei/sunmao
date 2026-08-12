//! GUI support types and cross-platform utilities

use raw_window_handle::RawWindowHandle;

/// GUI size in logical pixels
#[derive(Clone, Copy, Debug)]
pub struct GuiSize {
    pub width: u32,
    pub height: u32,
}

fn is_native_platform_supported(platform: &str) -> bool {
    #[cfg(target_os = "macos")]
    return platform == "NSView";

    #[cfg(target_os = "windows")]
    return platform == "HWND";

    #[cfg(target_os = "linux")]
    return platform == "X11EmbedWindowID";

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    return false;
}

impl GuiSize {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

/// Trait for plugins with GUI support
pub trait GuiPlugin: crate::Plugin {
    /// Initial GUI size
    fn gui_size() -> GuiSize;

    /// Check if a platform type is supported
    fn is_platform_supported(platform: &str) -> bool {
        is_native_platform_supported(platform)
    }

    /// Create and attach GUI to parent window
    fn gui_create(&mut self, parent: RawWindowHandle) -> bool;

    /// Destroy GUI
    fn gui_destroy(&mut self);

    /// Whether the editor accepts host-driven size changes.
    fn gui_can_resize(&self) -> bool {
        false
    }

    /// Called when GUI should be resized
    fn gui_resize(&mut self, _size: GuiSize) {}
}

// =============================================================================
// Cross-platform GUI Utilities
// =============================================================================

/// GUI initialization error
#[derive(Debug, Clone, Copy)]
pub enum GuiError {
    /// NSView's window property returned null (macOS)
    WindowNotAvailable,
    /// Unsupported platform or window handle type
    UnsupportedPlatform,
}

impl std::fmt::Display for GuiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GuiError::WindowNotAvailable => write!(f, "Window not available from view"),
            GuiError::UnsupportedPlatform => write!(f, "Unsupported platform or handle type"),
        }
    }
}

impl std::error::Error for GuiError {}

/// Prepare a window handle for rendering on the current platform.
///
/// This function performs platform-specific setup required before creating
/// a rendering surface. Call this before initializing your rendering backend.
///
/// # Platform-specific behavior:
/// - **macOS**: Sets `setWantsLayer:YES` on NSView, fills in `ns_window` if missing
/// - **Windows**: No-op (currently no preparation needed)
/// - **Linux**: No-op (currently no preparation needed)
pub fn prepare_view(handle: &mut RawWindowHandle) -> Result<(), GuiError> {
    #[cfg(target_os = "macos")]
    return macos::prepare_nsview(handle);

    #[cfg(target_os = "windows")]
    return Ok(());

    #[cfg(target_os = "linux")]
    return Ok(());

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    return Err(GuiError::UnsupportedPlatform);
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use objc::runtime::{Object, YES};
    use objc::{msg_send, sel, sel_impl};

    pub fn prepare_nsview(handle: &mut RawWindowHandle) -> Result<(), GuiError> {
        let RawWindowHandle::AppKit(h) = handle else {
            return Err(GuiError::UnsupportedPlatform);
        };

        // In raw-window-handle 0.6+, ns_view is NonNull<c_void>.
        let view = h.ns_view.as_ptr() as *mut Object;
        unsafe {
            // Enable layer-backed rendering (required for most GPU backends).
            let _: () = msg_send![view, setWantsLayer: YES];
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::is_native_platform_supported;

    #[test]
    fn only_the_native_vst3_window_api_is_reported() {
        #[cfg(target_os = "macos")]
        let native = "NSView";
        #[cfg(target_os = "windows")]
        let native = "HWND";
        #[cfg(target_os = "linux")]
        let native = "X11EmbedWindowID";

        assert!(is_native_platform_supported(native));
        for foreign in ["NSView", "HWND", "X11EmbedWindowID", "UIView"] {
            if foreign != native {
                assert!(!is_native_platform_supported(foreign));
            }
        }
    }
}
