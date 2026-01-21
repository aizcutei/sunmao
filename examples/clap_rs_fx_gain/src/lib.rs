use clap_rs::{Plugin, PluginInfo, ParameterInfo, AudioPortInfo, NotePortInfo, HostHandle, CLAP_PROCESS_CONTINUE};
use clap_rs::process::ProcessContext;
use std::ffi::c_char;

struct Gain {
    _host: HostHandle,
    gain: f64,
}

impl Plugin for Gain {
    type AudioProcessor = ();

    fn new(host: HostHandle) -> Self {
        Self {
            _host: host,
            gain: 1.0,
        }
    }

    fn init(&mut self) -> bool {
        true
    }

    fn declare_parameters(&self) -> Vec<ParameterInfo> {
        vec![
            ParameterInfo {
                id: 0,
                name: "Gain".to_string(),
                module: "".to_string(),
                min_value: 0.0,
                max_value: 2.0,
                default_value: 1.0,
            }
        ]
    }
    
    fn audio_ports_config(&self) -> Vec<AudioPortInfo> {
        vec![
            AudioPortInfo {
                id: 0,
                name: "Main In".to_string(),
                channel_count: 2,
                is_main: true,
                is_input: true,
            },
            AudioPortInfo {
                id: 1,
                name: "Main Out".to_string(),
                channel_count: 2,
                is_main: true,
                is_input: false,
            },
        ]
    }

    fn get_parameter(&self, id: u32) -> f64 {
        if id == 0 { self.gain } else { 0.0 }
    }

    fn set_parameter(&mut self, id: u32, value: f64) {
        if id == 0 {
            self.gain = value;
        }
    }

    fn process(&mut self, mut ctx: ProcessContext) -> clap_rs::clap_process_status {
        let frames = ctx.frames_count as usize;
        
        // Simple stereo processing (or multi-channel)
        for (input, output) in ctx.audio_inputs.iter().zip(ctx.audio_outputs.iter_mut()) {
             let len = input.len().min(output.len()).min(frames);
             for i in 0..len {
                 output[i] = input[i] * (self.gain as f32);
             }
        }
        
        CLAP_PROCESS_CONTINUE
    }
}

use clap_rs::export_clap_plugin;

struct SyncFeatures([*const c_char; 3]);
unsafe impl Sync for SyncFeatures {}

static FEATURES: SyncFeatures = SyncFeatures([
    b"audio-effect\0".as_ptr() as *const c_char,
    b"stereo\0".as_ptr() as *const c_char,
    std::ptr::null()
]);

export_clap_plugin!(Gain, PluginInfo {
    id: "com.sunmao.clap_rs.fx_gain\0",
    name: "Clap Rs Fx Gain\0",
    vendor: "aizcutei\0",
    url: "https://aizcutei.github.io/sunmao\0",
    manual_url: "https://aizcutei.github.io/sunmao/manual\0",
    support_url: "https://aizcutei.github.io/sunmao/support\0",
    version: "0.1\0",
    description: "A simple gain plugin using clap_rs\0",
}, FEATURES.0);
