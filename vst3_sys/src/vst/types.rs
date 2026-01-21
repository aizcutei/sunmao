//! VST-specific types

use crate::base::types::*;

// =============================================================================
// VST Types
// =============================================================================

pub type ParamID = uint32;
pub type ParamValue = f64;
pub type Sample32 = f32;
pub type Sample64 = f64;
pub type SampleRate = f64;
pub type SpeakerArrangement = uint64;
pub type Speaker = uint64;

pub type MediaType = int32;
pub type BusDirection = int32;
pub type BusType = int32;
pub type IoMode = int32;

pub type TQuarterNotes = f64;
pub type TSamples = int64;
pub type UnitID = int32;
pub type ProgramListID = int32;
pub type CtrlNumber = int16;
pub type TChar = char16;
pub type CString = *const char8;

// =============================================================================
// Media Types
// =============================================================================

pub mod MediaTypes {
    use super::MediaType;
    pub const kAudio: MediaType = 0;
    pub const kEvent: MediaType = 1;
}

// =============================================================================
// Bus Directions
// =============================================================================

pub mod BusDirections {
    use super::BusDirection;
    pub const kInput: BusDirection = 0;
    pub const kOutput: BusDirection = 1;
}

// =============================================================================
// Bus Types
// =============================================================================

pub mod BusTypes {
    use super::BusType;
    pub const kMain: BusType = 0;
    pub const kAux: BusType = 1;
}

// =============================================================================
// Symbolic Sample Sizes
// =============================================================================

pub mod SymbolicSampleSizes {
    use crate::base::types::int32;
    pub const kSample32: int32 = 0;
    pub const kSample64: int32 = 1;
}

// =============================================================================
// Process Modes
// =============================================================================

pub mod ProcessModes {
    use crate::base::types::int32;
    pub const kRealtime: int32 = 0;
    pub const kPrefetch: int32 = 1;
    pub const kOffline: int32 = 2;
}

// =============================================================================
// Component Flags
// =============================================================================

pub mod ComponentFlags {
    use crate::base::types::uint32;
    pub const kDistributable: uint32 = 1 << 0;
    pub const kSimpleModeSupported: uint32 = 1 << 1;
}

// =============================================================================
// Bus Flags
// =============================================================================

pub mod BusFlags {
    use crate::base::types::uint32;
    pub const kDefaultActive: uint32 = 1 << 0;
    pub const kIsControlVoltage: uint32 = 1 << 1;
}

// =============================================================================
// Speaker Arrangements
// =============================================================================

pub mod SpeakerArr {
    use super::SpeakerArrangement;
    pub const kEmpty: SpeakerArrangement = 0;
    pub const kMono: SpeakerArrangement = 1 << 0;
    pub const kLeft: SpeakerArrangement = 1 << 0;
    pub const kRight: SpeakerArrangement = 1 << 1;
    pub const kStereo: SpeakerArrangement = kLeft | kRight;
}

// =============================================================================
// Tail Samples
// =============================================================================

pub const kNoTail: uint32 = 0;
pub const kInfiniteTail: uint32 = u32::MAX;

// =============================================================================
// Category Constants
// =============================================================================

pub const kVstAudioEffectClass: &[u8] = b"Audio Module Class\0";
pub const kVstComponentControllerClass: &[u8] = b"Component Controller Class\0";

// =============================================================================
// Subcategory Constants
// =============================================================================

pub mod PlugType {
    pub const kFx: &[u8] = b"Fx\0";
    pub const kInstrument: &[u8] = b"Instrument\0";
    pub const kInstrumentSynth: &[u8] = b"Instrument|Synth\0";
}
