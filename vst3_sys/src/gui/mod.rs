//! GUI module - plugin view interfaces

pub mod iplugview;

pub use iplugview::*;

// =============================================================================
// Interface IIDs
// =============================================================================

pub mod iid {
    use crate::base::types::TUID;
    use crate::uid;

    pub const IPlugView: TUID = uid!(0x5BC32507, 0xD06049EA, 0xA6151B52, 0x2B755B29);
    pub const IPlugFrame: TUID = uid!(0x367FAF01, 0xAFA94693, 0x8D4DA2A0, 0xED0882A3);
}
