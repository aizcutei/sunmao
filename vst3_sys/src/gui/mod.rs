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
    pub const IEventHandler: TUID = uid!(0x561E65C9, 0x13A0496F, 0x813A2C35, 0x654D7983);
    pub const ITimerHandler: TUID = uid!(0x10BDD94F, 0x41424774, 0x821FAD8F, 0xECA72CA9);
    pub const IRunLoop: TUID = uid!(0x18C35366, 0x97764F1A, 0x9C5B8385, 0x7A871389);
}
