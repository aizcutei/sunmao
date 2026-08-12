//! Parameter types and traits.

use std::sync::atomic::{AtomicU32, Ordering};

/// The host-visible shape of a parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    Float,
    Int,
    Bool,
}

/// Derive a stable VST3/CLAP parameter ID from SunMao's string ID.
///
/// This uses 32-bit FNV-1a. The algorithm is part of the persistence contract:
/// changing it would break host automation and saved state. `u32::MAX`, which
/// both plugin APIs reserve as an invalid ID, is deterministically remapped.
pub const fn stable_param_id(id: &str) -> u32 {
    const FNV_OFFSET_BASIS: u32 = 0x811c_9dc5;
    const FNV_PRIME: u32 = 0x0100_0193;

    let bytes = id.as_bytes();
    let mut hash = FNV_OFFSET_BASIS;
    let mut index = 0;
    while index < bytes.len() {
        hash ^= bytes[index] as u32;
        hash = hash.wrapping_mul(FNV_PRIME);
        index += 1;
    }

    if hash == u32::MAX {
        u32::MAX - 1
    } else {
        hash
    }
}

/// Format-neutral parameter metadata used by backend adapters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamDescriptor {
    /// Stable string ID used by plugin code and views.
    pub id: &'static str,
    /// Stable host-facing ID used by VST3 and CLAP.
    pub numeric_id: u32,
    pub name: &'static str,
    /// Default value normalized to the host-facing 0.0..=1.0 range.
    pub default_normalized: f32,
    /// Number of intervals between discrete values. Zero means continuous.
    pub step_count: u32,
    pub kind: ParamKind,
}

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
    /// Describe all parameters in the same stable order as [`Params::ids`].
    fn descriptors(&self) -> Vec<ParamDescriptor>;
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
        assert!(
            min.is_finite() && max.is_finite() && min <= max,
            "invalid FloatParam range"
        );
        let default = if default.is_finite() {
            default.clamp(min, max)
        } else {
            min
        };
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
        let value = if value.is_finite() {
            value.clamp(self.min, self.max)
        } else {
            self.default
        };
        self.value.store(value.to_bits(), Ordering::Relaxed);
    }

    /// Get normalized value (0.0-1.0).
    pub fn get_normalized(&self) -> f32 {
        let range = self.max - self.min;
        if range.abs() <= f32::EPSILON {
            0.0
        } else {
            ((self.get() - self.min) / range).clamp(0.0, 1.0)
        }
    }

    /// Set from normalized value (0.0-1.0).
    pub fn set_normalized(&self, norm: f32) {
        let norm = if norm.is_finite() {
            norm.clamp(0.0, 1.0)
        } else {
            self.get_normalized()
        };
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
        assert!(min <= max, "invalid IntParam range");
        Self {
            id,
            name,
            value: std::sync::atomic::AtomicI32::new(default.clamp(min, max)),
            default: default.clamp(min, max),
            min,
            max,
        }
    }

    pub fn get(&self) -> i32 {
        self.value.load(Ordering::Relaxed)
    }

    pub fn set(&self, value: i32) {
        self.value
            .store(value.clamp(self.min, self.max), Ordering::Relaxed);
    }

    /// Get the current value normalized to the host-facing 0.0..=1.0 range.
    pub fn get_normalized(&self) -> f32 {
        let range = self.max.saturating_sub(self.min) as f32;
        if range <= f32::EPSILON {
            0.0
        } else {
            ((self.get() - self.min) as f32 / range).clamp(0.0, 1.0)
        }
    }

    /// Set the value from the host-facing 0.0..=1.0 range.
    pub fn set_normalized(&self, normalized: f32) {
        let normalized = if normalized.is_finite() {
            normalized.clamp(0.0, 1.0)
        } else {
            self.get_normalized()
        };
        let range = self.max.saturating_sub(self.min) as f32;
        let value = self.min as f32 + normalized * range;
        self.set(value.round() as i32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_parameter_clamps_plain_and_normalized_values() {
        let param = FloatParam::new("gain", "Gain", 1.0, 0.0, 2.0);
        param.set(3.0);
        assert_eq!(param.get(), 2.0);
        param.set_normalized(-1.0);
        assert_eq!(param.get(), 0.0);
        param.set_normalized(0.25);
        assert_eq!(param.get(), 0.5);
    }

    #[test]
    fn degenerate_float_range_has_finite_normalized_value() {
        let param = FloatParam::new("fixed", "Fixed", 2.0, 2.0, 2.0);
        assert_eq!(param.get_normalized(), 0.0);
        param.set_normalized(1.0);
        assert_eq!(param.get(), 2.0);
    }

    #[test]
    fn integer_parameter_clamps_to_declared_range() {
        let param = IntParam::new("voices", "Voices", 8, 1, 16);
        param.set(32);
        assert_eq!(param.get(), 16);
        param.set(-4);
        assert_eq!(param.get(), 1);
    }

    #[test]
    fn discrete_parameters_round_trip_normalized_values() {
        let integer = IntParam::new("voices", "Voices", 3, 1, 5);
        assert_eq!(integer.get_normalized(), 0.5);
        integer.set_normalized(0.0);
        assert_eq!(integer.get(), 1);
        integer.set_normalized(1.0);
        assert_eq!(integer.get(), 5);

        let boolean = BoolParam::new("bypass", "Bypass", false);
        assert_eq!(boolean.get_normalized(), 0.0);
        boolean.set_normalized(0.75);
        assert!(boolean.get());
        boolean.set_normalized(f32::NAN);
        assert!(boolean.get());
    }

    #[test]
    fn stable_parameter_ids_match_fnv1a_and_reserve_invalid_id() {
        assert_eq!(stable_param_id(""), 0x811c_9dc5);
        assert_eq!(stable_param_id("a"), 0xe40c_292c);
        assert_eq!(stable_param_id("foobar"), 0xbf9c_f968);
        assert_ne!(stable_param_id("gain"), u32::MAX);
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

    /// Get the current value in the host-facing normalized range.
    pub fn get_normalized(&self) -> f32 {
        if self.get() {
            1.0
        } else {
            0.0
        }
    }

    /// Set the value from the host-facing normalized range.
    pub fn set_normalized(&self, normalized: f32) {
        let normalized = if normalized.is_finite() {
            normalized.clamp(0.0, 1.0)
        } else {
            self.get_normalized()
        };
        self.set(normalized >= 0.5);
    }
}
