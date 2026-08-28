//! Parameter types and traits.

use std::sync::atomic::{AtomicU32, Ordering};
use std::{error::Error, fmt};

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

/// Why a parameter layout cannot be exposed safely to a plug-in host.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamLayoutError {
    EmptyId {
        index: usize,
    },
    DuplicateId {
        id: &'static str,
    },
    InvalidNumericId {
        id: &'static str,
        expected: u32,
        actual: u32,
    },
    NumericIdCollision {
        first: &'static str,
        second: &'static str,
        numeric_id: u32,
    },
    InvalidDefault {
        id: &'static str,
        value: f32,
    },
    MissingValue {
        id: &'static str,
    },
    InvalidValue {
        id: &'static str,
        value: f32,
    },
}

impl fmt::Display for ParamLayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyId { index } => write!(formatter, "parameter {index} has an empty ID"),
            Self::DuplicateId { id } => write!(formatter, "duplicate parameter ID '{id}'"),
            Self::InvalidNumericId {
                id,
                expected,
                actual,
            } => write!(
                formatter,
                "parameter '{id}' has numeric ID {actual:#010x}, expected {expected:#010x}"
            ),
            Self::NumericIdCollision {
                first,
                second,
                numeric_id,
            } => write!(
                formatter,
                "parameter IDs '{first}' and '{second}' collide at {numeric_id:#010x}"
            ),
            Self::InvalidDefault { id, value } => write!(
                formatter,
                "parameter '{id}' has invalid normalized default {value}"
            ),
            Self::MissingValue { id } => {
                write!(
                    formatter,
                    "parameter '{id}' cannot be read by its descriptor ID"
                )
            }
            Self::InvalidValue { id, value } => write!(
                formatter,
                "parameter '{id}' has invalid normalized value {value}"
            ),
        }
    }
}

impl Error for ParamLayoutError {}

/// Validate the persistent ID and normalized-value contract shared by all formats.
pub fn validate_param_layout<P: Params + ?Sized>(
    params: &P,
    descriptors: &[ParamDescriptor],
) -> Result<(), ParamLayoutError> {
    for (index, descriptor) in descriptors.iter().enumerate() {
        if descriptor.id.is_empty() {
            return Err(ParamLayoutError::EmptyId { index });
        }

        let expected_numeric_id = stable_param_id(descriptor.id);
        if descriptor.numeric_id != expected_numeric_id {
            return Err(ParamLayoutError::InvalidNumericId {
                id: descriptor.id,
                expected: expected_numeric_id,
                actual: descriptor.numeric_id,
            });
        }

        if !descriptor.default_normalized.is_finite()
            || !(0.0..=1.0).contains(&descriptor.default_normalized)
        {
            return Err(ParamLayoutError::InvalidDefault {
                id: descriptor.id,
                value: descriptor.default_normalized,
            });
        }

        let value = params
            .get_normalized(descriptor.id)
            .ok_or(ParamLayoutError::MissingValue { id: descriptor.id })?;
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(ParamLayoutError::InvalidValue {
                id: descriptor.id,
                value,
            });
        }

        for previous in &descriptors[..index] {
            if previous.id == descriptor.id {
                return Err(ParamLayoutError::DuplicateId { id: descriptor.id });
            }
            if previous.numeric_id == descriptor.numeric_id {
                return Err(ParamLayoutError::NumericIdCollision {
                    first: previous.id,
                    second: descriptor.id,
                    numeric_id: descriptor.numeric_id,
                });
            }
        }
    }

    Ok(())
}

/// Trait for parameter structs.
///
/// Implemented via `#[derive(Params)]` macro.
pub trait Params: Send + Sync + 'static {
    /// Get parameter value by ID (normalized 0.0-1.0).
    fn get_normalized(&self, id: &str) -> Option<f32>;
    /// Set parameter value by ID (normalized 0.0-1.0).
    fn set_normalized(&self, id: &str, value: f32);
    /// Describe all parameters in their stable host-facing order.
    fn descriptors(&self) -> Vec<ParamDescriptor>;

    /// Build and validate the descriptor set used by every format adapter.
    fn validated_descriptors(&self) -> Result<Vec<ParamDescriptor>, ParamLayoutError> {
        let descriptors = self.descriptors();
        validate_param_layout(self, &descriptors)?;
        Ok(descriptors)
    }
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
        // Do the range arithmetic in f64. Two finite f32 endpoints can have
        // a difference larger than `f32::MAX`; doing this in f32 would produce
        // infinity and make every non-degenerate parameter report 0.0.
        let range = self.max as f64 - self.min as f64;
        if range == 0.0 {
            0.0
        } else {
            ((self.get() as f64 - self.min as f64) / range).clamp(0.0, 1.0) as f32
        }
    }

    /// Set from normalized value (0.0-1.0).
    pub fn set_normalized(&self, norm: f32) {
        let norm = if norm.is_finite() {
            norm.clamp(0.0, 1.0)
        } else {
            self.get_normalized()
        };
        let value = self.min as f64 + norm as f64 * (self.max as f64 - self.min as f64);
        self.set(value as f32);
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
        // Widen before subtracting: `i32::MAX - i32::MIN` overflows in i32
        // even though the declared range itself is valid.
        let range = i64::from(self.max) - i64::from(self.min);
        if range == 0 {
            0.0
        } else {
            ((i64::from(self.get()) - i64::from(self.min)) as f64 / range as f64).clamp(0.0, 1.0)
                as f32
        }
    }

    /// Set the value from the host-facing 0.0..=1.0 range.
    pub fn set_normalized(&self, normalized: f32) {
        let normalized = if normalized.is_finite() {
            normalized.clamp(0.0, 1.0)
        } else {
            self.get_normalized()
        };
        let range = i64::from(self.max) - i64::from(self.min);
        let value = self.min as f64 + normalized as f64 * range as f64;
        // Float-to-int casts saturate on current Rust, but clamp explicitly so
        // this remains obvious and stable across supported toolchains.
        let value = value.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32;
        self.set(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct LayoutParams {
        descriptors: Vec<ParamDescriptor>,
        values: Vec<(&'static str, f32)>,
    }

    impl Params for LayoutParams {
        fn get_normalized(&self, id: &str) -> Option<f32> {
            self.values
                .iter()
                .find_map(|(candidate, value)| (*candidate == id).then_some(*value))
        }

        fn set_normalized(&self, _id: &str, _value: f32) {}

        fn descriptors(&self) -> Vec<ParamDescriptor> {
            self.descriptors.clone()
        }
    }

    fn descriptor(id: &'static str) -> ParamDescriptor {
        ParamDescriptor {
            id,
            numeric_id: stable_param_id(id),
            name: id,
            default_normalized: 0.5,
            step_count: 0,
            kind: ParamKind::Float,
        }
    }

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
    fn normalization_handles_full_integer_range_without_overflow() {
        let parameter = IntParam::new("full", "Full", 0, i32::MIN, i32::MAX);

        assert!((parameter.get_normalized() - 0.5).abs() < 1.0e-7);
        parameter.set_normalized(0.0);
        assert_eq!(parameter.get(), i32::MIN);
        parameter.set_normalized(1.0);
        assert_eq!(parameter.get(), i32::MAX);
    }

    #[test]
    fn normalization_handles_f32_endpoints_whose_range_exceeds_f32() {
        let parameter = FloatParam::new("wide", "Wide", 0.0, -f32::MAX, f32::MAX);

        assert!((parameter.get_normalized() - 0.5).abs() < 1.0e-7);
        parameter.set_normalized(0.0);
        assert_eq!(parameter.get(), -f32::MAX);
        parameter.set_normalized(1.0);
        assert_eq!(parameter.get(), f32::MAX);
    }

    #[test]
    fn stable_parameter_ids_match_fnv1a_and_reserve_invalid_id() {
        assert_eq!(stable_param_id(""), 0x811c_9dc5);
        assert_eq!(stable_param_id("a"), 0xe40c_292c);
        assert_eq!(stable_param_id("foobar"), 0xbf9c_f968);
        assert_ne!(stable_param_id("gain"), u32::MAX);
    }

    #[test]
    fn valid_parameter_layout_is_accepted() {
        let params = LayoutParams {
            descriptors: vec![descriptor("gain"), descriptor("mix")],
            values: vec![("gain", 0.25), ("mix", 1.0)],
        };

        assert_eq!(params.validated_descriptors().unwrap(), params.descriptors);
    }

    #[test]
    fn parameter_layout_rejects_empty_and_duplicate_string_ids() {
        let empty = LayoutParams {
            descriptors: vec![descriptor("")],
            values: vec![("", 0.5)],
        };
        assert_eq!(
            empty.validated_descriptors(),
            Err(ParamLayoutError::EmptyId { index: 0 })
        );

        let duplicate = LayoutParams {
            descriptors: vec![descriptor("gain"), descriptor("gain")],
            values: vec![("gain", 0.5)],
        };
        assert_eq!(
            duplicate.validated_descriptors(),
            Err(ParamLayoutError::DuplicateId { id: "gain" })
        );
    }

    #[test]
    fn parameter_layout_rejects_numeric_id_drift_and_real_hash_collisions() {
        let mut drifted = descriptor("gain");
        drifted.numeric_id = drifted.numeric_id.wrapping_add(1);
        let params = LayoutParams {
            descriptors: vec![drifted],
            values: vec![("gain", 0.5)],
        };
        assert!(matches!(
            params.validated_descriptors(),
            Err(ParamLayoutError::InvalidNumericId { id: "gain", .. })
        ));

        // These distinct strings are a known 32-bit FNV-1a collision. Keeping
        // the concrete pair here protects the persisted host-ID contract.
        assert_eq!(stable_param_id("costarring"), stable_param_id("liquid"));
        let collision = LayoutParams {
            descriptors: vec![descriptor("costarring"), descriptor("liquid")],
            values: vec![("costarring", 0.25), ("liquid", 0.75)],
        };
        assert!(matches!(
            collision.validated_descriptors(),
            Err(ParamLayoutError::NumericIdCollision {
                first: "costarring",
                second: "liquid",
                ..
            })
        ));
    }

    #[test]
    fn parameter_layout_rejects_invalid_defaults_and_current_values() {
        for invalid in [f32::NAN, f32::INFINITY, -0.01, 1.01] {
            let mut invalid_default = descriptor("gain");
            invalid_default.default_normalized = invalid;
            let params = LayoutParams {
                descriptors: vec![invalid_default],
                values: vec![("gain", 0.5)],
            };
            assert!(matches!(
                params.validated_descriptors(),
                Err(ParamLayoutError::InvalidDefault { id: "gain", .. })
            ));

            let params = LayoutParams {
                descriptors: vec![descriptor("gain")],
                values: vec![("gain", invalid)],
            };
            assert!(matches!(
                params.validated_descriptors(),
                Err(ParamLayoutError::InvalidValue { id: "gain", .. })
            ));
        }
    }

    #[test]
    fn parameter_layout_rejects_descriptors_without_readable_values() {
        let params = LayoutParams {
            descriptors: vec![descriptor("gain")],
            values: Vec::new(),
        };

        assert_eq!(
            params.validated_descriptors(),
            Err(ParamLayoutError::MissingValue { id: "gain" })
        );
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
