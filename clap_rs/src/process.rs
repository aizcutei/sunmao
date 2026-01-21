use clap_sys::process::clap_process_t;
use clap_sys::events::{
    clap_event_transport_t, clap_input_events_t, clap_output_events_t,
    CLAP_TRANSPORT_HAS_BEATS_TIMELINE, CLAP_TRANSPORT_HAS_SECONDS_TIMELINE,
    CLAP_TRANSPORT_HAS_TEMPO, CLAP_TRANSPORT_IS_PLAYING,
};
use clap_sys::fixedpoint::{clap_sectime, CLAP_BEATTIME_FACTOR, CLAP_SECTIME_FACTOR};
use crate::events::Event;
use std::slice;
use std::marker::PhantomData;

pub struct ProcessContext<'a> {
    pub frames_count: u32,
    pub audio_inputs: Vec<&'a [f32]>,
    pub audio_outputs: Vec<&'a mut [f32]>,
    input_events: *const clap_input_events_t,
    output_events: *const clap_output_events_t,
    transport: *const clap_event_transport_t,
}

impl<'a> ProcessContext<'a> {
    pub unsafe fn from_raw(process: *const clap_process_t) -> Self {
        let process = unsafe { &*process };
        let frames_count = process.frames_count;

        let mut audio_inputs = Vec::new();
        if !process.audio_inputs.is_null() && process.audio_inputs_count > 0 {
             let input = unsafe { &*process.audio_inputs }; // Assume 1st input bus for now
             if !input.data32.is_null() {
                 let channels = unsafe { slice::from_raw_parts(input.data32, input.channel_count as usize) };
                 for ch_ptr in channels {
                     if !ch_ptr.is_null() {
                         audio_inputs.push(unsafe { slice::from_raw_parts(*ch_ptr, frames_count as usize) });
                     }
                 }
             }
        }

        let mut audio_outputs = Vec::new();
        if !process.audio_outputs.is_null() && process.audio_outputs_count > 0 {
             let output = unsafe { &*process.audio_outputs }; // Assume 1st output bus
             if !output.data32.is_null() {
                 let channels = unsafe { slice::from_raw_parts(output.data32, output.channel_count as usize) };
                 for ch_ptr in channels {
                     if !ch_ptr.is_null() {
                         audio_outputs.push(unsafe { slice::from_raw_parts_mut(*ch_ptr, frames_count as usize) });
                     }
                 }
             }
        }

        ProcessContext {
            frames_count,
            audio_inputs,
            audio_outputs,
            input_events: process.in_events,
            output_events: process.out_events,
            transport: process.transport,
        }
    }

    pub fn transport(&self) -> Option<Transport> {
        if self.transport.is_null() {
            None
        } else {
            Some(Transport {
                raw: unsafe { *self.transport },
            })
        }
    }

    pub fn events(&self) -> InputEventIterator<'a> {
        let count = if !self.input_events.is_null() {
            unsafe {
                let size_fn = (*self.input_events).size.unwrap();
                size_fn(self.input_events)
            }
        } else {
            0
        };

        InputEventIterator {
            events: self.input_events,
            index: 0,
            count,
            _marker: PhantomData,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Transport {
    raw: clap_event_transport_t,
}

impl Transport {
    pub fn tempo(&self) -> Option<f64> {
        if (self.raw.flags & CLAP_TRANSPORT_HAS_TEMPO) != 0 {
            Some(self.raw.tempo)
        } else {
            None
        }
    }

    pub fn is_playing(&self) -> bool {
        (self.raw.flags & CLAP_TRANSPORT_IS_PLAYING) != 0
    }

    pub fn song_pos_seconds(&self) -> Option<f64> {
        if (self.raw.flags & CLAP_TRANSPORT_HAS_SECONDS_TIMELINE) != 0 {
            Some(fixed_to_f64(self.raw.song_pos_seconds, CLAP_SECTIME_FACTOR))
        } else {
            None
        }
    }

    pub fn song_pos_beats(&self) -> Option<f64> {
        if (self.raw.flags & CLAP_TRANSPORT_HAS_BEATS_TIMELINE) != 0 {
            Some(fixed_to_f64(self.raw.song_pos_beats, CLAP_BEATTIME_FACTOR))
        } else {
            None
        }
    }
}

fn fixed_to_f64(value: clap_sectime, factor: i64) -> f64 {
    value as f64 / factor as f64
}

pub struct InputEventIterator<'a> {
    events: *const clap_input_events_t,
    index: u32,
    count: u32,
    _marker: PhantomData<&'a ()>,
}

impl<'a> Iterator for InputEventIterator<'a> {
    type Item = Event;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.count || self.events.is_null() {
            return None;
        }

        unsafe {
            let get_fn = (*self.events).get.unwrap();
            let event_header = get_fn(self.events, self.index);
            self.index += 1;
            
            if event_header.is_null() {
                return None;
            }
            
            Some(Event::from_raw(event_header))
        }
    }
}
