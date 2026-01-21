//! Safe wrappers for Core Animation CALayer operations.
//!
//! This module provides safe Rust APIs for creating and manipulating CALayer
//! objects without requiring unsafe code in plugin implementations.

use objc::runtime::Object;
use objc::{class, msg_send, sel, sel_impl};

/// Opaque handle to a CALayer.
#[derive(Clone, Copy)]
pub struct Layer(*mut Object);

impl Layer {
    /// Creates a new CALayer.
    pub fn new() -> Self {
        unsafe {
            let layer: *mut Object = msg_send![class!(CALayer), layer];
            Self(layer)
        }
    }

    /// Returns the raw pointer (for internal use).
    pub fn as_ptr(&self) -> *mut Object {
        self.0
    }

    /// Creates a Layer from a raw pointer.
    /// 
    /// # Safety
    /// The pointer must be a valid CALayer or null.
    pub fn from_ptr(ptr: *mut Object) -> Self {
        Self(ptr)
    }

    /// Returns true if the layer is null.
    pub fn is_null(&self) -> bool {
        self.0.is_null()
    }

    /// Sets the frame (position and size) of the layer.
    pub fn set_frame(&self, x: f64, y: f64, width: f64, height: f64) {
        if self.0.is_null() {
            return;
        }
        unsafe {
            let frame = CGRect {
                origin: CGPoint { x, y },
                size: CGSize { width, height },
            };
            let _: () = msg_send![self.0, setFrame: frame];
        }
    }

    /// Sets the background color using RGBA values (0.0-1.0).
    pub fn set_background_color(&self, r: f64, g: f64, b: f64, a: f64) {
        if self.0.is_null() {
            return;
        }
        unsafe {
            let ns_color: *mut Object = msg_send![class!(NSColor), colorWithSRGBRed: r green: g blue: b alpha: a];
            let cg_color: *mut Object = msg_send![ns_color, CGColor];
            let _: () = msg_send![self.0, setBackgroundColor: cg_color];
        }
    }

    /// Sets the corner radius for rounded corners.
    pub fn set_corner_radius(&self, radius: f64) {
        if self.0.is_null() {
            return;
        }
        unsafe {
            let _: () = msg_send![self.0, setCornerRadius: radius];
        }
    }

    /// Adds a sublayer to this layer.
    pub fn add_sublayer(&self, sublayer: &Layer) {
        if self.0.is_null() || sublayer.0.is_null() {
            return;
        }
        unsafe {
            let _: () = msg_send![self.0, addSublayer: sublayer.0];
        }
    }
}

impl Default for Layer {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard for CATransaction that disables animations.
/// Automatically commits when dropped.
pub struct TransactionGuard;

impl TransactionGuard {
    /// Begins a CATransaction with animations disabled.
    pub fn begin_no_animation() -> Self {
        unsafe {
            let _: () = msg_send![class!(CATransaction), begin];
            let _: () = msg_send![class!(CATransaction), setDisableActions: true];
        }
        Self
    }
}

impl Drop for TransactionGuard {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![class!(CATransaction), commit];
        }
    }
}

/// Gets the root layer of a view.
pub fn get_view_layer(view: *mut Object) -> Layer {
    if view.is_null() {
        return Layer(std::ptr::null_mut());
    }
    unsafe {
        let _: () = msg_send![view, setWantsLayer: true];
        let layer: *mut Object = msg_send![view, layer];
        Layer(layer)
    }
}

// Internal CGRect types for objc messaging
#[repr(C)]
#[derive(Copy, Clone)]
struct CGPoint { x: f64, y: f64 }

#[repr(C)]
#[derive(Copy, Clone)]
struct CGSize { width: f64, height: f64 }

#[repr(C)]
#[derive(Copy, Clone)]
struct CGRect { origin: CGPoint, size: CGSize }
