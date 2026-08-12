//! Format-specific metadata structures.

/// Speaker arrangement for one VST3 audio bus.
///
/// Named constants cover common layouts. [`Self::from_mask`] supports other arrangements defined
/// by the VST3 SDK without forcing the format-specific bitmask into the generic audio API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vst3SpeakerLayout(u64);

impl Vst3SpeakerLayout {
    pub const EMPTY: Self = Self(0);
    pub const MONO: Self = Self(1 << 19);
    pub const STEREO: Self = Self((1 << 0) | (1 << 1));
    pub const CINE_3_0: Self = Self((1 << 0) | (1 << 1) | (1 << 2));
    pub const MUSIC_3_0: Self = Self((1 << 0) | (1 << 1) | (1 << 8));
    pub const CINE_3_1: Self = Self(Self::CINE_3_0.0 | (1 << 3));
    pub const MUSIC_3_1: Self = Self(Self::MUSIC_3_0.0 | (1 << 3));
    pub const CINE_4_0: Self = Self(Self::CINE_3_0.0 | (1 << 8));
    pub const QUAD_4_0: Self = Self((1 << 0) | (1 << 1) | (1 << 4) | (1 << 5));
    pub const CINE_4_1: Self = Self(Self::CINE_4_0.0 | (1 << 3));
    pub const QUAD_4_1: Self = Self(Self::QUAD_4_0.0 | (1 << 3));
    pub const SURROUND_5_0: Self = Self((1 << 0) | (1 << 1) | (1 << 2) | (1 << 4) | (1 << 5));
    pub const SURROUND_5_1: Self = Self(Self::SURROUND_5_0.0 | (1 << 3));
    pub const CINE_6_0: Self = Self(Self::SURROUND_5_0.0 | (1 << 8));
    pub const MUSIC_6_0: Self =
        Self((1 << 0) | (1 << 1) | (1 << 4) | (1 << 5) | (1 << 9) | (1 << 10));
    pub const CINE_6_1: Self = Self(Self::CINE_6_0.0 | (1 << 3));
    pub const MUSIC_6_1: Self = Self(Self::MUSIC_6_0.0 | (1 << 3));
    pub const CINE_7_0: Self = Self(Self::SURROUND_5_0.0 | (1 << 6) | (1 << 7));
    pub const MUSIC_7_0: Self = Self(Self::SURROUND_5_0.0 | (1 << 9) | (1 << 10));
    pub const CINE_7_1: Self = Self(Self::CINE_7_0.0 | (1 << 3));
    pub const MUSIC_7_1: Self = Self(Self::MUSIC_7_0.0 | (1 << 3));

    pub const fn from_mask(mask: u64) -> Self {
        Self(mask)
    }

    pub const fn mask(self) -> u64 {
        self.0
    }

    pub const fn channel_count(self) -> u32 {
        self.0.count_ones()
    }
}

/// VST3 plugin metadata.
pub struct Vst3Info {
    /// Unique 16-byte class ID. An all-zero ID asks the backend to derive a stable ID from the
    /// plugin vendor and name. Published plugins should set this explicitly.
    pub class_id: [u8; 16],
    /// VST3 subcategories.
    pub categories: &'static [&'static str],
    /// Explicit input-bus speaker layout. Empty, mono, and stereo are inferred when omitted.
    pub input_layout: Option<Vst3SpeakerLayout>,
    /// Explicit output-bus speaker layout. Empty, mono, and stereo are inferred when omitted.
    pub output_layout: Option<Vst3SpeakerLayout>,
}

impl Default for Vst3Info {
    fn default() -> Self {
        Self {
            class_id: [0; 16],
            categories: &["Fx"],
            input_layout: None,
            output_layout: None,
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

#[cfg(test)]
mod tests {
    use super::Vst3SpeakerLayout;

    #[test]
    fn common_vst3_layouts_use_sdk_masks_and_channel_counts() {
        assert_eq!(Vst3SpeakerLayout::MONO.mask(), 0x0008_0000);
        assert_eq!(Vst3SpeakerLayout::STEREO.mask(), 0x0000_0003);
        assert_eq!(Vst3SpeakerLayout::MUSIC_3_0.mask(), 0x0000_0103);
        assert_eq!(Vst3SpeakerLayout::QUAD_4_0.mask(), 0x0000_0033);
        assert_eq!(Vst3SpeakerLayout::SURROUND_5_1.mask(), 0x0000_003f);
        assert_eq!(Vst3SpeakerLayout::CINE_7_1.mask(), 0x0000_00ff);
        assert_eq!(Vst3SpeakerLayout::MUSIC_7_1.mask(), 0x0000_063f);
        assert_eq!(Vst3SpeakerLayout::SURROUND_5_1.channel_count(), 6);
        assert_eq!(Vst3SpeakerLayout::MUSIC_7_1.channel_count(), 8);
    }
}
