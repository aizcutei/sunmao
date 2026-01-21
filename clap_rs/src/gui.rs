//! Cross-platform GUI utilities for CLAP plugins
//!
//! This module provides platform-specific utilities for preparing window handles
//! for rendering. Works with any rendering backend (OpenGL, WebGPU, softbuffer, etc.).

use raw_window_handle::RawWindowHandle;

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
/// - **macOS**: Sets `setWantsLayer:YES` on NSView (required for layer-backed rendering)
/// - **Windows**: No-op (currently no preparation needed)
/// - **Linux**: No-op (currently no preparation needed)
///
/// # Note
/// In raw-window-handle 0.6+, `ns_window` was removed as it can be retrieved from NSView.
pub fn prepare_view(handle: &mut RawWindowHandle) -> Result<(), GuiError> {
    #[cfg(target_os = "macos")]
    return macos::prepare_nsview(handle);

    #[cfg(target_os = "windows")]
    return Ok(()); // No preparation needed currently

    #[cfg(target_os = "linux")]
    return Ok(()); // No preparation needed currently

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    return Err(GuiError::UnsupportedPlatform);
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use objc::runtime::{Object, YES};
    use objc::{msg_send, sel, sel_impl};

    pub fn prepare_nsview(handle: &mut RawWindowHandle) -> Result<(), GuiError> {
        if let RawWindowHandle::AppKit(h) = handle {
            // In raw-window-handle 0.6+, ns_view is NonNull<c_void>
            let view = h.ns_view.as_ptr() as *mut Object;
            unsafe {
                // Enable layer-backed rendering (required for most GPU backends)
                let _: () = msg_send![view, setWantsLayer: YES];
            }
            // Note: ns_window was removed in 0.6 - hosts should get window from view if needed
        }
        Ok(())
    }
}
