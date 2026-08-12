//! VST3 Sine Synthesizer using vst3_rs wrapper
//!
//! A simple polyphonic sine wave synthesizer demonstrating
//! MIDI input handling with the vst3_rs safety wrapper.

use std::f64::consts::PI;
use vst3_rs::*;

const MAX_VOICES: usize = 8;
const MAX_EVENTS_PER_BLOCK: usize = 4096;

struct Voice {
    active: bool,
    note: i16,
    phase: f64,
    phase_inc: f64,
    velocity: f32,
}

impl Voice {
    fn new() -> Self {
        Self {
            active: false,
            note: 0,
            phase: 0.0,
            phase_inc: 0.0,
            velocity: 0.0,
        }
    }
}

#[derive(Clone, Copy)]
enum PendingNote {
    On {
        offset: u32,
        sequence: u32,
        pitch: i16,
        velocity: f32,
    },
    Off {
        offset: u32,
        sequence: u32,
        pitch: i16,
    },
}

impl PendingNote {
    fn offset(self) -> u32 {
        match self {
            Self::On { offset, .. } | Self::Off { offset, .. } => offset,
        }
    }

    fn sequence(self) -> u32 {
        match self {
            Self::On { sequence, .. } | Self::Off { sequence, .. } => sequence,
        }
    }
}

/// Simple Sine Synthesizer
struct MySine {
    gain: f64,
    sample_rate: f64,
    voices: [Voice; MAX_VOICES],
    pending_notes: Vec<PendingNote>,
    event_overflowed: bool,
    scratch: Vec<f32>,
}

impl Plugin for MySine {
    const MAX_EVENTS_PER_BLOCK: usize = MAX_EVENTS_PER_BLOCK;

    fn info() -> PluginInfo {
        PluginInfo {
            id: "com.sunmao.vst3_rs_syn_sine",
            name: "Vst3 Rs Syn Sine",
            vendor: "aizcutei",
            url: "https://aizcutei.github.io/sunmao",
            email: "info@example.com",
            version: "0.1.0",
            category: "Instrument|Synth",
        }
    }

    fn new(_host: HostHandle) -> Self {
        Self {
            gain: 0.5,
            sample_rate: 44100.0,
            voices: std::array::from_fn(|_| Voice::new()),
            pending_notes: Vec::with_capacity(MAX_EVENTS_PER_BLOCK),
            event_overflowed: false,
            scratch: vec![0.0; 4096],
        }
    }

    fn audio_config() -> AudioConfig {
        AudioConfig::stereo_synth()
    }

    fn activate(&mut self, sample_rate: f64, max_frames: u32) -> bool {
        self.sample_rate = sample_rate;
        self.scratch.resize(max_frames as usize, 0.0);
        true
    }

    fn reset(&mut self) {
        for voice in &mut self.voices {
            voice.active = false;
            voice.phase = 0.0;
        }
        self.pending_notes.clear();
        self.event_overflowed = false;
    }

    fn params() -> Vec<ParamInfo> {
        vec![
            ParamInfo::new(0, "Gain")
                .range(0.0, 1.0)
                .default(0.5)
                .units(""),
        ]
    }

    fn get_param(&self, _id: u32) -> f64 {
        self.gain
    }

    fn set_param(&mut self, _id: u32, value: f64) {
        self.gain = value;
    }

    fn process(&mut self, ctx: &mut ProcessContext) -> ProcessResult {
        if self.event_overflowed {
            self.pending_notes.clear();
            self.event_overflowed = false;
            return Err(ProcessError::OutOfMemory);
        }
        let num_samples = ctx.num_samples;
        let gain = self.gain as f32;

        let num_channels = ctx.num_outputs();
        self.scratch[..num_samples].fill(0.0);
        let mut pending_notes = std::mem::take(&mut self.pending_notes);
        pending_notes.sort_unstable_by_key(|event| (event.offset(), event.sequence()));
        let mut event_index = 0;

        for sample_index in 0..num_samples {
            while event_index < pending_notes.len()
                && pending_notes[event_index].offset() as usize <= sample_index
            {
                match pending_notes[event_index] {
                    PendingNote::On {
                        pitch, velocity, ..
                    } => {
                        if let Some(voice) = self.voices.iter_mut().find(|voice| !voice.active) {
                            voice.active = true;
                            voice.note = pitch;
                            voice.velocity = velocity;
                            voice.phase = 0.0;
                            voice.phase_inc = Self::midi_note_to_freq(pitch) / self.sample_rate;
                        }
                    }
                    PendingNote::Off { pitch, .. } => {
                        if let Some(voice) = self
                            .voices
                            .iter_mut()
                            .find(|voice| voice.active && voice.note == pitch)
                        {
                            voice.active = false;
                        }
                    }
                }
                event_index += 1;
            }

            for voice in self.voices.iter_mut().filter(|voice| voice.active) {
                let sample = (voice.phase * 2.0 * PI).sin() as f32 * voice.velocity * gain;
                self.scratch[sample_index] += sample;
                voice.phase += voice.phase_inc;
                if voice.phase >= 1.0 {
                    voice.phase -= 1.0;
                }
            }
        }
        pending_notes.clear();
        self.pending_notes = pending_notes;

        for ch in 0..num_channels {
            let output = ctx.output_mut(ch);
            for (i, s) in self.scratch[..num_samples].iter().enumerate() {
                if i < output.len() {
                    output[i] = *s;
                }
            }
        }
        Ok(())
    }

    fn note_on(&mut self, sample_offset: u32, _channel: i16, pitch: i16, velocity: f32) {
        if self.pending_notes.len() >= MAX_EVENTS_PER_BLOCK {
            self.event_overflowed = true;
            return;
        }
        let sequence = self.pending_notes.len() as u32;
        self.pending_notes.push(PendingNote::On {
            offset: sample_offset,
            sequence,
            pitch,
            velocity,
        });
    }

    fn note_off(&mut self, sample_offset: u32, _channel: i16, pitch: i16, _velocity: f32) {
        if self.pending_notes.len() >= MAX_EVENTS_PER_BLOCK {
            self.event_overflowed = true;
            return;
        }
        let sequence = self.pending_notes.len() as u32;
        self.pending_notes.push(PendingNote::Off {
            offset: sample_offset,
            sequence,
            pitch,
        });
    }
}

impl MySine {
    fn midi_note_to_freq(note: i16) -> f64 {
        440.0 * 2.0_f64.powf((note as f64 - 69.0) / 12.0)
    }
}

export_vst3_plugin!(MySine);

#[cfg(test)]
use vst3_rs::vst3_sys as vst3_test_api;

#[cfg(test)]
#[path = "../../realtime_test_support.rs"]
mod realtime_test_support;

#[cfg(test)]
#[path = "../../vst3_callback_test_support.rs"]
mod vst3_callback_test_support;

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::c_void;
    use vst3_callback_test_support::{ActiveProcessor, TestEventList};
    use vst3_test_api::*;

    #[test]
    fn direct_vst3_synth_callback_does_not_allocate() {
        unsafe {
            let mut processor = ActiveProcessor::new(__vst3_rs_impl::GetPluginFactory(), 16);
            let mut output_left = [0.0_f32; 16];
            let mut output_right = [0.0_f32; 16];
            let mut output_channels = [
                output_left.as_mut_ptr() as *mut c_void,
                output_right.as_mut_ptr() as *mut c_void,
            ];
            let mut output = AudioBusBuffers {
                num_channels: 2,
                silence_flags: 0,
                buffers: output_channels.as_mut_ptr(),
            };
            let mut events = TestEventList::note_on(3, 69);
            let mut data = ProcessData {
                process_mode: 0,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                num_samples: 16,
                num_inputs: 0,
                num_outputs: 1,
                inputs: std::ptr::null_mut(),
                outputs: &mut output,
                input_parameter_changes: std::ptr::null_mut(),
                output_parameter_changes: std::ptr::null_mut(),
                input_events: events.as_raw(),
                output_events: std::ptr::null_mut(),
                process_context: std::ptr::null_mut(),
            };

            let (status, allocator_calls) =
                realtime_test_support::count_allocator_calls(|| processor.process(&mut data));
            assert_eq!(status, kResultOk);
            assert_eq!(allocator_calls, 0);
            assert_eq!(&output_left[..4], &[0.0; 4]);
            assert_eq!(&output_left, &output_right);
            assert!(output_left[4..].iter().any(|sample| *sample != 0.0));
        }
    }
}
