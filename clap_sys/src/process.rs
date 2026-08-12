use crate::audio_buffer::clap_audio_buffer_t;
use crate::events::{clap_event_transport_t, clap_input_events_t, clap_output_events_t};

pub const CLAP_PROCESS_ERROR: i32 = 0;
pub const CLAP_PROCESS_CONTINUE: i32 = 1;
pub const CLAP_PROCESS_CONTINUE_IF_NOT_QUIET: i32 = 2;
pub const CLAP_PROCESS_TAIL: i32 = 3;
pub const CLAP_PROCESS_SLEEP: i32 = 4;

pub type clap_process_status = i32;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct clap_process_t {
    pub steady_time: i64,
    pub frames_count: u32,
    pub transport: *const clap_event_transport_t,
    pub audio_inputs: *const clap_audio_buffer_t,
    pub audio_outputs: *mut clap_audio_buffer_t,
    pub audio_inputs_count: u32,
    pub audio_outputs_count: u32,
    pub in_events: *const clap_input_events_t,
    pub out_events: *const clap_output_events_t,
}
