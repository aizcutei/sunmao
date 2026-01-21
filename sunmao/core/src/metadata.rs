//! Format-specific metadata structures.

/// VST3 plugin metadata.
pub struct Vst3Info {
    /// Unique 16-byte class ID.
    pub class_id: [u8; 16],
    /// VST3 subcategories.
    pub categories: &'static [&'static str],
}

impl Default for Vst3Info {
    fn default() -> Self {
        Self {
            class_id: *b"SunMaoDefaultID!",
            categories: &["Fx"],
        }
    }
}

/// Audio Unit plugin metadata.
pub struct AuInfo {
    /// Component type (e.g., "aufx" for effect, "aumu" for instrument).
    pub type_code: [u8; 4],
    /// Component subtype (unique identifier).
    pub subtype_code: [u8; 4],
    /// Manufacturer code.
    pub manufacturer_code: [u8; 4],
}

impl Default for AuInfo {
    fn default() -> Self {
        Self {
            type_code: *b"aufx",
            subtype_code: *b"sunm",
            manufacturer_code: *b"SunM",
        }
    }
}

/// CLAP plugin metadata.
pub struct ClapInfo {
    /// Unique plugin ID (reverse domain notation).
    pub id: &'static str,
    /// CLAP features.
    pub features: &'static [&'static str],
}

impl Default for ClapInfo {
    fn default() -> Self {
        Self {
            id: "com.sunmao.plugin",
            features: &["audio-effect"],
        }
    }
}
