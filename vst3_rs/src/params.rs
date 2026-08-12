//! Parameter types and utilities

/// Parameter information for declaration
#[derive(Clone, Debug)]
pub struct ParamInfo {
    /// Unique parameter ID
    pub id: u32,
    /// Parameter display name
    pub name: &'static str,
    /// Short name for compact displays
    pub short_name: &'static str,
    /// Unit label (e.g., "dB", "%", "Hz")
    pub units: &'static str,
    /// Minimum value in plain units
    pub min: f64,
    /// Maximum value in plain units
    pub max: f64,
    /// Default value (normalized 0-1)
    pub default: f64,
    /// Step count (0 = continuous)
    pub step_count: i32,
    /// Parameter flags
    pub flags: ParamFlags,
}

impl ParamInfo {
    /// Create a new parameter with default settings
    pub fn new(id: u32, name: &'static str) -> Self {
        Self {
            id,
            name,
            short_name: name,
            units: "",
            min: 0.0,
            max: 1.0,
            default: 0.0,
            step_count: 0,
            flags: ParamFlags::CAN_AUTOMATE,
        }
    }

    /// Set the value range (plain units)
    pub fn range(mut self, min: f64, max: f64) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    /// Set default value (normalized 0-1)
    pub fn default(mut self, value: f64) -> Self {
        self.default = value;
        self
    }

    /// Set units label
    pub fn units(mut self, units: &'static str) -> Self {
        self.units = units;
        self
    }

    /// Set short name
    pub fn short_name(mut self, name: &'static str) -> Self {
        self.short_name = name;
        self
    }

    /// Set the number of intervals for a discrete parameter (zero is continuous).
    pub fn step_count(mut self, step_count: i32) -> Self {
        self.step_count = step_count.max(0);
        self
    }

    /// Convert normalized value (0-1) to plain value
    pub fn to_plain(&self, normalized: f64) -> f64 {
        self.min + normalized * (self.max - self.min)
    }

    /// Convert plain value to normalized (0-1)
    pub fn to_normalized(&self, plain: f64) -> f64 {
        if (self.max - self.min).abs() < f64::EPSILON {
            0.0
        } else {
            (plain - self.min) / (self.max - self.min)
        }
    }
}

/// Parameter flags
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamFlags(pub u32);

impl ParamFlags {
    pub const NONE: Self = Self(0);
    pub const CAN_AUTOMATE: Self = Self(1);
    pub const IS_READ_ONLY: Self = Self(1 << 1);
    pub const IS_WRAP_AROUND: Self = Self(1 << 2);
    pub const IS_LIST: Self = Self(1 << 3);
    pub const IS_HIDDEN: Self = Self(1 << 4);
    pub const IS_BYPASS: Self = Self(1 << 16);
}
