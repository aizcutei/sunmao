//! IAudioProcessor interface

use crate::base::types::*;
use crate::vst::types::*;
use std::ffi::c_void;

// =============================================================================
// Structs
// =============================================================================

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProcessSetup {
    pub process_mode: int32,
    pub symbolic_sample_size: int32,
    pub max_samples_per_block: int32,
    pub sample_rate: SampleRate,
}

#[repr(C)]
pub struct AudioBusBuffers {
    pub num_channels: int32,
    pub silence_flags: uint64,
    pub buffers: *mut *mut c_void,
}

#[repr(C)]
pub struct ProcessData {
    pub process_mode: int32,
    pub symbolic_sample_size: int32,
    pub num_samples: int32,
    pub num_inputs: int32,
    pub num_outputs: int32,
    pub inputs: *mut AudioBusBuffers,
    pub outputs: *mut AudioBusBuffers,
    pub input_parameter_changes: *mut c_void,
    pub output_parameter_changes: *mut c_void,
    pub input_events: *mut c_void,
    pub output_events: *mut c_void,
    pub process_context: *mut c_void,
}

// =============================================================================
// IAudioProcessor VTable
// =============================================================================

/// IAudioProcessor vtable
#[repr(C)]
pub struct IAudioProcessorVtbl {
    pub unknown: IUnknownVtbl,
    pub set_bus_arrangements: unsafe extern "system" fn(
        this: *mut c_void,
        inputs: *mut SpeakerArrangement,
        num_ins: int32,
        outputs: *mut SpeakerArrangement,
        num_outs: int32,
    ) -> tresult,
    pub get_bus_arrangement: unsafe extern "system" fn(
        this: *mut c_void,
        dir: BusDirection,
        index: int32,
        arr: *mut SpeakerArrangement,
    ) -> tresult,
    pub can_process_sample_size:
        unsafe extern "system" fn(this: *mut c_void, symbolic_sample_size: int32) -> tresult,
    pub get_latency_samples: unsafe extern "system" fn(this: *mut c_void) -> uint32,
    pub setup_processing:
        unsafe extern "system" fn(this: *mut c_void, setup: *mut ProcessSetup) -> tresult,
    pub set_processing: unsafe extern "system" fn(this: *mut c_void, state: TBool) -> tresult,
    pub process: unsafe extern "system" fn(this: *mut c_void, data: *mut ProcessData) -> tresult,
    pub get_tail_samples: unsafe extern "system" fn(this: *mut c_void) -> uint32,
}

/// `IProcessContextRequirements`, transcribed from
/// `pluginterfaces/vst/ivstaudioprocessor.h`.
/// Upstream: steinbergmedia/vst3_pluginterfaces at
/// `4f547e8e102b47de4a8b8aaf343c73b700786372`, lines 438–469.
///
/// VST3 3.7 makes this mandatory on the audio processor: it is how a plugin
/// tells the host which `ProcessContext` fields it actually reads, so the host
/// can skip computing the rest. Steinberg's validator reports
/// "Missing mandatory IProcessContextRequirements extension!" without it.
#[repr(C)]
pub struct IProcessContextRequirementsVtbl {
    pub base: IUnknownVtbl,
    pub get_process_context_requirements: unsafe extern "system" fn(this: *mut c_void) -> u32,
}

/// `IProcessContextRequirements::Flags`, transcribed verbatim. The trailing
/// comments are upstream's, naming the `ProcessContext` state flag each one
/// corresponds to.
#[allow(non_upper_case_globals)]
pub mod ProcessContextRequirementsFlags {
    pub const kNeedSystemTime: u32 = 1 << 0; // kSystemTimeValid
    pub const kNeedContinousTimeSamples: u32 = 1 << 1; // kContTimeValid
    pub const kNeedProjectTimeMusic: u32 = 1 << 2; // kProjectTimeMusicValid
    pub const kNeedBarPositionMusic: u32 = 1 << 3; // kBarPositionValid
    pub const kNeedCycleMusic: u32 = 1 << 4; // kCycleValid
    pub const kNeedSamplesToNextClock: u32 = 1 << 5; // kClockValid
    pub const kNeedTempo: u32 = 1 << 6; // kTempoValid
    pub const kNeedTimeSignature: u32 = 1 << 7; // kTimeSigValid
    pub const kNeedChord: u32 = 1 << 8; // kChordValid
    pub const kNeedFrameRate: u32 = 1 << 9; // kSmpteValid
    pub const kNeedTransportState: u32 = 1 << 10; // kPlaying, kCycleActive, kRecording
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_requirements_abi_matches_the_upstream_header() {
        use ProcessContextRequirementsFlags as Need;
        use std::mem::{align_of, offset_of, size_of};
        assert_eq!(offset_of!(IProcessContextRequirementsVtbl, base), 0);
        assert_eq!(
            offset_of!(
                IProcessContextRequirementsVtbl,
                get_process_context_requirements
            ),
            3 * size_of::<*const c_void>()
        );
        assert_eq!(
            size_of::<IProcessContextRequirementsVtbl>(),
            4 * size_of::<*const c_void>()
        );
        assert_eq!(
            align_of::<IProcessContextRequirementsVtbl>(),
            align_of::<*const c_void>()
        );
        assert_eq!(
            [
                Need::kNeedSystemTime,
                Need::kNeedContinousTimeSamples,
                Need::kNeedProjectTimeMusic,
                Need::kNeedBarPositionMusic,
                Need::kNeedCycleMusic,
                Need::kNeedSamplesToNextClock,
                Need::kNeedTempo,
                Need::kNeedTimeSignature,
                Need::kNeedChord,
                Need::kNeedFrameRate,
                Need::kNeedTransportState
            ],
            [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024]
        );
        // Literal bytes from DECLARE_CLASS_IID, using the two upstream UID
        // layouts. This checks the IID as well as the function table.
        let expected: [u8; 16] = if cfg!(target_os = "windows") {
            [
                0x03, 0x43, 0x65, 0x2a, 0x76, 0xef, 0x3d, 0x4e, 0x95, 0xb5, 0xfe, 0x83, 0x73, 0x0e,
                0xf6, 0xd0,
            ]
        } else {
            [
                0x2a, 0x65, 0x43, 0x03, 0xef, 0x76, 0x4e, 0x3d, 0x95, 0xb5, 0xfe, 0x83, 0x73, 0x0e,
                0xf6, 0xd0,
            ]
        };
        assert_eq!(
            crate::vst::iid::IProcessContextRequirements.map(|b| b as u8),
            expected
        );
    }
}
