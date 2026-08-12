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
