use std::f32::consts::TAU;

use au_sys::{
    AuComponentDescriptor, AuPlugin, BufferList, ParameterInfo, ParameterUnit, export_au_component,
    fourcc,
};

const PARAM_GAIN: u32 = 0;

const PARAMETERS: [ParameterInfo; 1] = [ParameterInfo {
    id: PARAM_GAIN,
    name: "Gain",
    min: 0.0,
    max: 1.0,
    default: 0.2,
    unit: ParameterUnit::LinearGain,
}];

pub struct BasicSynth {
    sample_rate: f32,
    phase: f32,
    freq: f32,
    gain: f32,
    gate: bool,
    next_note_id: u32,
    active_note_id: u32,
}

impl AuPlugin for BasicSynth {
    fn new(sample_rate: f64, _max_frames: u32) -> Self {
        Self {
            sample_rate: sample_rate as f32,
            phase: 0.0,
            freq: 440.0,
            gain: PARAMETERS[0].default,
            gate: false,
            next_note_id: 1,
            active_note_id: 0,
        }
    }

    fn reset(&mut self) {
        self.phase = 0.0;
    }

    fn process(
        &mut self,
        _inputs: Option<BufferList<'_>>,
        outputs: &mut BufferList<'_>,
        frames: usize,
    ) {
        let channels = outputs.len();
        for i in 0..frames {
            let sample = if self.gate {
                (self.phase).sin() * self.gain
            } else {
                0.0
            };
            self.phase += TAU * self.freq / self.sample_rate;
            if self.phase >= TAU {
                self.phase -= TAU;
            }
            for ch in 0..channels {
                let out = unsafe { outputs.channel_mut(ch) };
                if i < out.len() {
                    out[i] = sample;
                }
            }
        }
    }

    fn parameters(&self) -> &'static [ParameterInfo] {
        &PARAMETERS
    }

    fn get_parameter(&self, id: u32) -> f32 {
        match id {
            PARAM_GAIN => self.gain,
            _ => 0.0,
        }
    }

    fn set_parameter(&mut self, id: u32, value: f32) {
        if id == PARAM_GAIN {
            self.gain = value.clamp(0.0, 1.0);
        }
    }

    fn handle_midi_event(&mut self, status: u8, data1: u8, data2: u8, _offset: u32) {
        let command = status & 0xF0;
        match command {
            0x90 => {
                if data2 == 0 {
                    self.gate = false;
                } else {
                    self.freq = midi_note_to_freq(data1 as f32);
                    self.gate = true;
                }
            }
            0x80 => {
                self.gate = false;
            }
            _ => {}
        }
    }

    fn start_note(&mut self, pitch: f32, _velocity: f32, _offset: u32) -> u32 {
        self.freq = midi_note_to_freq(pitch);
        self.gate = true;
        let note_id = self.next_note_id;
        self.next_note_id = self.next_note_id.wrapping_add(1);
        self.active_note_id = note_id;
        note_id
    }

    fn stop_note(&mut self, note_id: u32, _offset: u32) {
        if note_id == self.active_note_id {
            self.gate = false;
        }
    }
}

fn midi_note_to_freq(note: f32) -> f32 {
    440.0 * 2.0_f32.powf((note - 69.0) / 12.0)
}

export_au_component!(
    RustAUFactory,
    BasicSynth,
    AuComponentDescriptor {
        name: "Au Sys Syn Sine",
        component_type: fourcc(b"aumu"),
        component_subtype: fourcc(b"ssyn"),
        manufacturer: fourcc(b"RUST"),
        version: 0x0001_0000,
        flags: 0,
        flags_mask: 0,
        input_channels: 0,
        output_channels: 2,
        supports_midi: true,
        parameters: &PARAMETERS,
        cocoa_view_info: None,
        cocoa_view_class: None,
        cocoa_view_bundle_id: None,
        cocoa_view_init: None,
    }
);
