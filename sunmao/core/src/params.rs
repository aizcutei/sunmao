//! Parameter types and traits.

use std::sync::atomic::{AtomicU32, Ordering};

/// Trait for parameter structs.
///
/// Implemented via `#[derive(Params)]` macro.
pub trait Params: Send + Sync + 'static {
    /// Get all parameter IDs.
    fn ids() -> &'static [&'static str];
    /// Get parameter value by ID (normalized 0.0-1.0).
    fn get_normalized(&self, id: &str) -> Option<f32>;
    /// Set parameter value by ID (normalized 0.0-1.0).
    fn set_normalized(&self, id: &str, value: f32);
}

/// A floating-point parameter with thread-safe atomic backing.
pub struct FloatParam {
    /// Parameter ID.
    pub id: &'static str,
    /// Human-readable name.
    pub name: &'static str,
    /// Current value (stored as bits for atomic access).
    value: AtomicU32,
    /// Default value.
    pub default: f32,
    /// Minimum value.
    pub min: f32,
    /// Maximum value.
    pub max: f32,
}

impl FloatParam {
    /// Create a new FloatParam.
    pub fn new(id: &'static str, name: &'static str, default: f32, min: f32, max: f32) -> Self {
        Self {
            id,
            name,
            value: AtomicU32::new(default.to_bits()),
            default,
            min,
            max,
        }
    }

    /// Get the current value.
    pub fn get(&self) -> f32 {
        f32::from_bits(self.value.load(Ordering::Relaxed))
    }

    /// Set the value.
    pub fn set(&self, value: f32) {
        self.value.store(value.to_bits(), Ordering::Relaxed);
    }

    /// Get normalized value (0.0-1.0).
    pub fn get_normalized(&self) -> f32 {
        (self.get() - self.min) / (self.max - self.min)
    }

    /// Set from normalized value (0.0-1.0).
    pub fn set_normalized(&self, norm: f32) {
        let value = self.min + norm * (self.max - self.min);
        self.set(value);
    }
}

/// An integer parameter.
pub struct IntParam {
    pub id: &'static str,
    pub name: &'static str,
    value: std::sync::atomic::AtomicI32,
    pub default: i32,
    pub min: i32,
    pub max: i32,
}

impl IntParam {
    pub fn new(id: &'static str, name: &'static str, default: i32, min: i32, max: i32) -> Self {
        Self {
            id,
            name,
            value: std::sync::atomic::AtomicI32::new(default),
            default,
            min,
            max,
        }
    }

    pub fn get(&self) -> i32 {
        self.value.load(Ordering::Relaxed)
    }

    pub fn set(&self, value: i32) {
        self.value.store(value, Ordering::Relaxed);
    }
}

/// A boolean parameter.
pub struct BoolParam {
    pub id: &'static str,
    pub name: &'static str,
    value: std::sync::atomic::AtomicBool,
    pub default: bool,
}

impl BoolParam {
    pub fn new(id: &'static str, name: &'static str, default: bool) -> Self {
        Self {
            id,
            name,
            value: std::sync::atomic::AtomicBool::new(default),
            default,
        }
    }

    pub fn get(&self) -> bool {
        self.value.load(Ordering::Relaxed)
    }

    pub fn set(&self, value: bool) {
        self.value.store(value, Ordering::Relaxed);
    }
}
