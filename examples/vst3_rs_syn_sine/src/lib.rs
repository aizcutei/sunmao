//! VST3 Sine Synthesizer using vst3_rs wrapper
//!
//! A simple polyphonic sine wave synthesizer demonstrating
//! MIDI input handling with the vst3_rs safety wrapper.

use std::f64::consts::PI;
use vst3_rs::*;

const MAX_VOICES: usize = 8;

struct Voice {
    active: bool,
    note: i16,
    phase: f64,
    phase_inc: f64,
    velocity: f32,
}

impl Voice {
    fn new() -> Self {
        Self { active: false, note: 0, phase: 0.0, phase_inc: 0.0, velocity: 0.0 }
    }
}

/// Simple Sine Synthesizer
struct MySine {
    gain: f64,
    sample_rate: f64,
    voices: [Voice; MAX_VOICES],
}

impl Plugin for MySine {
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
        }
    }
    
    fn audio_config() -> AudioConfig {
        AudioConfig::stereo_synth()
    }
    
    fn activate(&mut self, sample_rate: f64, _max_frames: u32) -> bool {
        self.sample_rate = sample_rate;
        true
    }
    
    fn reset(&mut self) {
        for voice in &mut self.voices {
            voice.active = false;
            voice.phase = 0.0;
        }
    }
    
    fn params() -> Vec<ParamInfo> {
        vec![
            ParamInfo::new(0, "Gain")
                .range(0.0, 1.0)
                .default(0.5)
                .units("")
        ]
    }
    
    fn get_param(&self, _id: u32) -> f64 {
        self.gain
    }
    
    fn set_param(&mut self, _id: u32, value: f64) {
        self.gain = value;
    }
    
    fn process(&mut self, ctx: &mut ProcessContext) {
        let num_samples = ctx.num_samples;
        let gain = self.gain as f32;
        
        // Generate audio
        let num_channels = ctx.num_outputs();
        
        // Collect samples
        let mut samples: Vec<f32> = vec![0.0; num_samples];
        
        for voice in &mut self.voices {
            if voice.active {
                for i in 0..num_samples {
                    let sample = (voice.phase * 2.0 * PI).sin() as f32 * voice.velocity * gain;
                    samples[i] += sample;
                    voice.phase += voice.phase_inc;
                    if voice.phase >= 1.0 { voice.phase -= 1.0; }
                }
            }
        }
        
        // Write to outputs
        for ch in 0..num_channels {
            let output = ctx.output_mut(ch);
            for (i, s) in samples.iter().enumerate() {
                if i < output.len() {
                    output[i] = *s;
                }
            }
        }
    }
    
    fn note_on(&mut self, _channel: i16, pitch: i16, velocity: f32) {
        for voice in &mut self.voices {
            if !voice.active {
                voice.active = true;
                voice.note = pitch;
                voice.velocity = velocity;
                voice.phase = 0.0;
                voice.phase_inc = Self::midi_note_to_freq(pitch) / self.sample_rate;
                break;
            }
        }
    }
    
    fn note_off(&mut self, _channel: i16, pitch: i16, _velocity: f32) {
        for voice in &mut self.voices {
            if voice.active && voice.note == pitch {
                voice.active = false;
                break;
            }
        }
    }
}

impl MySine {
    fn midi_note_to_freq(note: i16) -> f64 {
        440.0 * 2.0_f64.powf((note as f64 - 69.0) / 12.0)
    }
}

export_vst3_plugin!(MySine);
