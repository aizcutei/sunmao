//! Metal layer support for baseview windows on macOS.
//!
//! Provides a `MetalLayer` that wraps a `CAMetalLayer`, enabling
//! wgpu/Metal rendering inside a baseview-managed NSView.

use cocoa::base::{id, nil, YES};
use cocoa::foundation::{NSRect, NSSize};
use objc::{class, msg_send, sel, sel_impl};

/// A CAMetalLayer attached to a baseview-managed NSView.
///
/// This layer can be used to create a wgpu surface for Metal-based rendering.
pub struct MetalLayer {
    layer: id,
}

impl MetalLayer {
    /// Create a new MetalLayer and attach it to the given NSView.
    ///
    /// # Safety
    /// The `ns_view` must be a valid, attached NSView on the AppKit thread and
    /// must outlive this wrapper and every use of its returned layer pointer.
    pub(crate) unsafe fn new(ns_view: id) -> Self {
        let layer: id = msg_send![class!(CAMetalLayer), layer];
        let _: () = msg_send![ns_view, setWantsLayer: YES];
        let _: () = msg_send![ns_view, setLayer: layer];

        let scale = backing_scale_factor(ns_view);
        if scale > 0.0 {
            let _: () = msg_send![layer, setContentsScale: scale];
        }

        let bounds: NSRect = msg_send![ns_view, bounds];
        let _: () = msg_send![layer, setFrame: bounds];

        if scale > 0.0 {
            let drawable_size = NSSize::new(bounds.size.width * scale, bounds.size.height * scale);
            let _: () = msg_send![layer, setDrawableSize: drawable_size];
        }

        Self { layer }
    }

    /// Get the raw `CAMetalLayer` pointer for wgpu surface creation.
    pub fn layer_ptr(&self) -> *mut std::ffi::c_void {
        self.layer as *mut std::ffi::c_void
    }

    /// Resize the metal layer to match the given NSView's current bounds.
    pub(crate) fn resize(&self, ns_view: id) {
        unsafe {
            let bounds: NSRect = msg_send![ns_view, bounds];
            let scale = backing_scale_factor(ns_view);
            let _: () = msg_send![self.layer, setFrame: bounds];

            if scale > 0.0 {
                let drawable_size =
                    NSSize::new(bounds.size.width * scale, bounds.size.height * scale);
                let _: () = msg_send![self.layer, setDrawableSize: drawable_size];
            }
        }
    }
}

unsafe fn backing_scale_factor(ns_view: id) -> f64 {
    let window: id = msg_send![ns_view, window];
    if window == nil {
        return 1.0;
    }

    let scale: f64 = msg_send![window, backingScaleFactor];
    if scale > 0.0 {
        scale
    } else {
        1.0
    }
}
