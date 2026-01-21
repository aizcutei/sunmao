use clap_rs::{Plugin, PluginInfo, ParameterInfo, AudioPortInfo, NotePortInfo, HostHandle, CLAP_PROCESS_CONTINUE};
use clap_rs::process::ProcessContext;
use clap_rs::events::Event;
use std::f64::consts::PI;
use std::ffi::c_char;

struct Sine {
    _host: HostHandle,
    frequency: f64,
    phase: f64,
    sample_rate: f64,
    gain: f64,
    active_voices: i32,
}

impl Plugin for Sine {
    type AudioProcessor = ();

    fn new(host: HostHandle) -> Self {
        Self {
            _host: host,
            frequency: 440.0,
            phase: 0.0,
            sample_rate: 44100.0,
            gain: 0.5,
            active_voices: 0,
        }
    }

    fn activate(&mut self, sample_rate: f64, _min_frames: u32, _max_frames: u32) -> bool {
        self.sample_rate = sample_rate;
        true
    }

    fn declare_parameters(&self) -> Vec<ParameterInfo> {
        vec![
            ParameterInfo {
                id: 0,
                name: "Frequency".to_string(),
                module: "".to_string(),
                min_value: 20.0,
                max_value: 2000.0,
                default_value: 440.0,
            },
            ParameterInfo {
                id: 1,
                name: "Gain".to_string(),
                module: "".to_string(),
                min_value: 0.0,
                max_value: 1.0,
                default_value: 0.5,
            }
        ]
    }
    
    fn audio_ports_config(&self) -> Vec<AudioPortInfo> {
        vec![
            AudioPortInfo {
                id: 0,
                name: "Main Out".to_string(),
                channel_count: 2,
                is_main: true,
                is_input: false,
            },
        ]
    }
    
    fn note_ports_config(&self) -> Vec<NotePortInfo> {
        vec![
            NotePortInfo {
                id: 0,
                name: "Notes".to_string(),
                is_input: true,
            },
        ]
    }

    fn get_parameter(&self, id: u32) -> f64 {
        match id {
            0 => self.frequency,
            1 => self.gain,
            _ => 0.0
        }
    }

    fn set_parameter(&mut self, id: u32, value: f64) {
        match id {
            0 => self.frequency = value,
            1 => self.gain = value,
            _ => {}
        }
    }

    fn process(&mut self, mut ctx: ProcessContext) -> clap_rs::clap_process_status {
        let frames = ctx.frames_count as usize;
        
        let mut buffer = vec![0.0f32; frames];
        
        let events: Vec<Event> = ctx.events().collect();

        for event in events {
            match event {
                Event::NoteOn(e) => {
                    self.frequency = 440.0 * (2.0f64).powf((e.key as f64 - 69.0) / 12.0);
                    self.active_voices = 1;
                },
                Event::NoteOff(_) => {
                    self.active_voices = 0;
                },
                Event::ParamValue(p) => {
                    self.set_parameter(p.param_id, p.value);
                },
                _ => {}
            }
        }

        let phase_inc = 2.0 * PI * self.frequency / self.sample_rate;
        for i in 0..frames {
            if self.active_voices > 0 {
                buffer[i] = (self.phase as f32).sin() * (self.gain as f32);
                self.phase += phase_inc;
                if self.phase > 2.0 * PI {
                    self.phase -= 2.0 * PI;
                }
            } else {
                buffer[i] = 0.0;
            }
        }

        for output in ctx.audio_outputs.iter_mut() {
             let len = output.len().min(frames);
             for i in 0..len {
                 output[i] = buffer[i];
             }
        }
        
        CLAP_PROCESS_CONTINUE
    }
}

use clap_rs::export_clap_plugin;

// Features: instrument, synthesizer, stereo
struct SyncFeatures([*const c_char; 4]);
unsafe impl Sync for SyncFeatures {}

static FEATURES: SyncFeatures = SyncFeatures([
    b"instrument\0".as_ptr() as *const c_char,
    b"synthesizer\0".as_ptr() as *const c_char,
    b"stereo\0".as_ptr() as *const c_char,
    std::ptr::null()
]);

export_clap_plugin!(Sine, PluginInfo {
    id: "com.sunmao.clap_rs.syn_sine\0",
    name: "Clap Rs Syn Sine\0",
    vendor: "aizcutei\0",
    url: "https://aizcutei.github.io/sunmao\0",
    manual_url: "https://aizcutei.github.io/sunmao/manual\0",
    support_url: "https://aizcutei.github.io/sunmao/support\0",
    version: "0.1\0",
    description: "A simple sine synth using clap_rs\0",
}, FEATURES.0);
