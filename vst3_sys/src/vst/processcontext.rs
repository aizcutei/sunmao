//! ProcessContext for tempo/transport

use crate::base::types::*;
use crate::vst::types::*;

// =============================================================================
// Frame Rate
// =============================================================================

/// Frame rate info
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FrameRate {
    pub frames_per_second: uint32,
    pub flags: uint32,
}

pub mod FrameRateFlags {
    use crate::base::types::uint32;
    pub const kPullDownRate: uint32 = 1 << 0;
    pub const kDropRate: uint32 = 1 << 1;
}

// =============================================================================
// Chord
// =============================================================================

/// Chord info
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Chord {
    pub key_note: uint8,
    pub root_note: uint8,
    pub chord_mask: int16,
}

// =============================================================================
// ProcessContext
// =============================================================================

/// Process context states and flags
pub mod ProcessContextFlags {
    use crate::base::types::uint32;
    pub const kPlaying: uint32 = 1 << 1;
    pub const kCycleActive: uint32 = 1 << 2;
    pub const kRecording: uint32 = 1 << 3;
    pub const kSystemTimeValid: uint32 = 1 << 8;
    pub const kContTimeValid: uint32 = 1 << 17;
    pub const kProjectTimeMusicValid: uint32 = 1 << 9;
    pub const kBarPositionValid: uint32 = 1 << 11;
    pub const kCycleValid: uint32 = 1 << 12;
    pub const kTempoValid: uint32 = 1 << 10;
    pub const kTimeSigValid: uint32 = 1 << 13;
    pub const kChordValid: uint32 = 1 << 18;
    pub const kSmpteValid: uint32 = 1 << 14;
    pub const kClockValid: uint32 = 1 << 15;
}

/// Audio processing context (tempo, transport, etc.)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProcessContext {
    pub state: uint32,
    pub sample_rate: f64,
    pub project_time_samples: TSamples,
    pub system_time: int64,
    pub continous_time_samples: TSamples,
    pub project_time_music: TQuarterNotes,
    pub bar_position_music: TQuarterNotes,
    pub cycle_start_music: TQuarterNotes,
    pub cycle_end_music: TQuarterNotes,
    pub tempo: f64,
    pub time_sig_numerator: int32,
    pub time_sig_denominator: int32,
    pub chord: Chord,
    pub smpte_offset_subframes: int32,
    pub frame_rate: FrameRate,
    pub samples_to_next_clock: int32,
}

impl Default for ProcessContext {
    fn default() -> Self {
        Self {
            state: 0,
            sample_rate: 44100.0,
            project_time_samples: 0,
            system_time: 0,
            continous_time_samples: 0,
            project_time_music: 0.0,
            bar_position_music: 0.0,
            cycle_start_music: 0.0,
            cycle_end_music: 0.0,
            tempo: 120.0,
            time_sig_numerator: 4,
            time_sig_denominator: 4,
            chord: Chord::default(),
            smpte_offset_subframes: 0,
            frame_rate: FrameRate::default(),
            samples_to_next_clock: 0,
        }
    }
}
