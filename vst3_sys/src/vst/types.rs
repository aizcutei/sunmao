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

pub mod Speakers {
    use super::Speaker;

    pub const kLeft: Speaker = 1 << 0;
    pub const kRight: Speaker = 1 << 1;
    pub const kCenter: Speaker = 1 << 2;
    pub const kLfe: Speaker = 1 << 3;
    pub const kLeftSurround: Speaker = 1 << 4;
    pub const kRightSurround: Speaker = 1 << 5;
    pub const kLeftCenter: Speaker = 1 << 6;
    pub const kRightCenter: Speaker = 1 << 7;
    pub const kCenterSurround: Speaker = 1 << 8;
    pub const kSideLeft: Speaker = 1 << 9;
    pub const kSideRight: Speaker = 1 << 10;
    pub const kTopCenter: Speaker = 1 << 11;
    pub const kTopFrontLeft: Speaker = 1 << 12;
    pub const kTopFrontCenter: Speaker = 1 << 13;
    pub const kTopFrontRight: Speaker = 1 << 14;
    pub const kTopRearLeft: Speaker = 1 << 15;
    pub const kTopRearCenter: Speaker = 1 << 16;
    pub const kTopRearRight: Speaker = 1 << 17;
    pub const kLfe2: Speaker = 1 << 18;
    pub const kMono: Speaker = 1 << 19;
    pub const kTopSideLeft: Speaker = 1 << 24;
    pub const kTopSideRight: Speaker = 1 << 25;
    pub const kLeftCenterSurround: Speaker = 1 << 26;
    pub const kRightCenterSurround: Speaker = 1 << 27;
    pub const kBottomFrontLeft: Speaker = 1 << 28;
    pub const kBottomFrontCenter: Speaker = 1 << 29;
    pub const kBottomFrontRight: Speaker = 1 << 30;
    pub const kProximityLeft: Speaker = 1 << 31;
    pub const kProximityRight: Speaker = 1 << 32;
    pub const kBottomSideLeft: Speaker = 1 << 33;
    pub const kBottomSideRight: Speaker = 1 << 34;
    pub const kBottomRearLeft: Speaker = 1 << 35;
    pub const kBottomRearCenter: Speaker = 1 << 36;
    pub const kBottomRearRight: Speaker = 1 << 37;
    pub const kLeftWide: Speaker = 1 << 59;
    pub const kRightWide: Speaker = 1 << 60;
}

pub mod SpeakerArr {
    use super::SpeakerArrangement;
    use super::Speakers as S;

    pub const kEmpty: SpeakerArrangement = 0;
    pub const kMono: SpeakerArrangement = S::kMono;
    pub const kLeft: SpeakerArrangement = S::kLeft;
    pub const kRight: SpeakerArrangement = S::kRight;
    pub const kStereo: SpeakerArrangement = kLeft | kRight;
    pub const kStereoWide: SpeakerArrangement = S::kLeftWide | S::kRightWide;
    pub const kStereoSurround: SpeakerArrangement = S::kLeftSurround | S::kRightSurround;
    pub const kStereoCenter: SpeakerArrangement = S::kLeftCenter | S::kRightCenter;
    pub const kStereoSide: SpeakerArrangement = S::kSideLeft | S::kSideRight;
    pub const kStereoCLfe: SpeakerArrangement = S::kCenter | S::kLfe;

    pub const k30Cine: SpeakerArrangement = kLeft | kRight | S::kCenter;
    pub const k31Cine: SpeakerArrangement = k30Cine | S::kLfe;
    pub const k30Music: SpeakerArrangement = kLeft | kRight | S::kCenterSurround;
    pub const k31Music: SpeakerArrangement = k30Music | S::kLfe;
    pub const k40Cine: SpeakerArrangement = kLeft | kRight | S::kCenter | S::kCenterSurround;
    pub const k41Cine: SpeakerArrangement = k40Cine | S::kLfe;
    pub const k40Music: SpeakerArrangement = kLeft | kRight | S::kLeftSurround | S::kRightSurround;
    pub const k41Music: SpeakerArrangement = k40Music | S::kLfe;
    pub const k50: SpeakerArrangement =
        kLeft | kRight | S::kCenter | S::kLeftSurround | S::kRightSurround;
    pub const k51: SpeakerArrangement = k50 | S::kLfe;
    pub const k60Cine: SpeakerArrangement = k50 | S::kCenterSurround;
    pub const k61Cine: SpeakerArrangement = k60Cine | S::kLfe;
    pub const k60Music: SpeakerArrangement =
        kLeft | kRight | S::kLeftSurround | S::kRightSurround | S::kSideLeft | S::kSideRight;
    pub const k61Music: SpeakerArrangement = k60Music | S::kLfe;
    pub const k70Cine: SpeakerArrangement = kLeft
        | kRight
        | S::kCenter
        | S::kLeftSurround
        | S::kRightSurround
        | S::kLeftCenter
        | S::kRightCenter;
    pub const k71Cine: SpeakerArrangement = k70Cine | S::kLfe;
    pub const k70Music: SpeakerArrangement = kLeft
        | kRight
        | S::kCenter
        | S::kLeftSurround
        | S::kRightSurround
        | S::kSideLeft
        | S::kSideRight;
    pub const k71Music: SpeakerArrangement = k70Music | S::kLfe;
    pub const k80Cine: SpeakerArrangement = k70Cine | S::kCenterSurround;
    pub const k81Cine: SpeakerArrangement = k80Cine | S::kLfe;
    pub const k80Music: SpeakerArrangement = k70Music | S::kCenterSurround;
    pub const k81Music: SpeakerArrangement = k80Music | S::kLfe;
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

#[cfg(test)]
mod tests {
    use super::{SpeakerArr, Speakers};

    #[test]
    fn speaker_arrangements_match_vst3_sdk_masks() {
        assert_eq!(SpeakerArr::kMono, 0x0008_0000);
        assert_eq!(SpeakerArr::kLeft, 0x0000_0001);
        assert_eq!(SpeakerArr::kRight, 0x0000_0002);
        assert_eq!(SpeakerArr::kStereo, 0x0000_0003);
        assert_eq!(Speakers::kCenterSurround, 0x0000_0100);
        assert_eq!(Speakers::kRightWide, 1 << 60);
        assert_eq!(SpeakerArr::k30Cine, 0x0000_0007);
        assert_eq!(SpeakerArr::k30Music, 0x0000_0103);
        assert_eq!(SpeakerArr::k40Music, 0x0000_0033);
        assert_eq!(SpeakerArr::k51, 0x0000_003f);
        assert_eq!(SpeakerArr::k61Cine, 0x0000_013f);
        assert_eq!(SpeakerArr::k71Cine, 0x0000_00ff);
        assert_eq!(SpeakerArr::k71Music, 0x0000_063f);
        assert_eq!(SpeakerArr::k81Music, 0x0000_073f);
    }
}
