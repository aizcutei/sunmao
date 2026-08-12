use clap_rs::events::Event;
use clap_rs::process::ProcessContext;
use clap_rs::{
    AudioPortInfo, AudioProcessor, CLAP_PROCESS_CONTINUE, HostHandle, ParameterInfo, Plugin,
    PluginInfo,
};
use std::ffi::c_char;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

struct Gain {
    _host: HostHandle,
    gain: Arc<AtomicU64>,
}

struct GainProcessor {
    gain: Arc<AtomicU64>,
}

impl Plugin for Gain {
    type AudioProcessor = GainProcessor;

    fn new(host: HostHandle) -> Self {
        Self {
            _host: host,
            gain: Arc::new(AtomicU64::new(1.0f64.to_bits())),
        }
    }

    fn init(&mut self) -> bool {
        true
    }

    fn activate(
        &mut self,
        _sample_rate: f64,
        _min_frames: u32,
        _max_frames: u32,
    ) -> Option<Self::AudioProcessor> {
        Some(GainProcessor {
            gain: self.gain.clone(),
        })
    }

    fn declare_parameters(&self) -> Vec<ParameterInfo> {
        vec![ParameterInfo {
            id: 0,
            name: "Gain".to_string(),
            module: "".to_string(),
            min_value: 0.0,
            max_value: 2.0,
            default_value: 1.0,
            is_stepped: false,
        }]
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
        if id == 0 {
            f64::from_bits(self.gain.load(Ordering::Relaxed))
        } else {
            0.0
        }
    }

    fn set_parameter(&mut self, id: u32, value: f64) {
        if id == 0 {
            self.gain.store(value.to_bits(), Ordering::Relaxed);
        }
    }
}

impl AudioProcessor for GainProcessor {
    fn process(&mut self, mut ctx: ProcessContext) -> clap_rs::clap_process_status {
        let frames = ctx.frames_count as usize;
        let mut gain = f64::from_bits(self.gain.load(Ordering::Relaxed)) as f32;
        let mut cursor = 0;

        for event in ctx.events() {
            let Event::ParamValue(event) = event else {
                continue;
            };
            if event.param_id != 0 || event.time as usize >= frames {
                continue;
            }
            let event_time = (event.time as usize).max(cursor);
            process_gain_range(&mut ctx, cursor, event_time, gain);
            gain = event.value as f32;
            self.set_parameter(event.param_id, event.value);
            cursor = event_time;
        }
        process_gain_range(&mut ctx, cursor, frames, gain);

        CLAP_PROCESS_CONTINUE
    }

    fn set_parameter(&mut self, id: u32, value: f64) {
        if id == 0 {
            self.gain.store(value.to_bits(), Ordering::Relaxed);
        }
    }
}

fn process_gain_range(ctx: &mut ProcessContext<'_>, start: usize, end: usize, gain: f32) {
    for (input, output) in ctx.audio_inputs.iter().zip(ctx.audio_outputs.iter_mut()) {
        let end = end.min(input.len()).min(output.len());
        let start = start.min(end);
        for i in start..end {
            output[i] = input[i] * gain;
        }
    }
}

use clap_rs::export_clap_plugin;

struct SyncFeatures([*const c_char; 3]);
unsafe impl Sync for SyncFeatures {}

static FEATURES: SyncFeatures = SyncFeatures([
    b"audio-effect\0".as_ptr() as *const c_char,
    b"stereo\0".as_ptr() as *const c_char,
    std::ptr::null(),
]);

export_clap_plugin!(
    Gain,
    PluginInfo {
        id: "com.sunmao.clap_rs.fx_gain",
        name: "Clap Rs Fx Gain",
        vendor: "aizcutei",
        url: "https://aizcutei.github.io/sunmao",
        manual_url: "https://aizcutei.github.io/sunmao/manual",
        support_url: "https://aizcutei.github.io/sunmao/support",
        version: "0.1",
        description: "A simple gain plugin using clap_rs",
    },
    FEATURES.0
);

#[cfg(test)]
#[path = "../../realtime_test_support.rs"]
mod realtime_test_support;

#[cfg(test)]
mod tests {
    use super::*;
    use clap_rs::PluginEntry;
    use clap_rs::clap_sys::audio_buffer::clap_audio_buffer_t;
    use clap_rs::clap_sys::events::{
        CLAP_CORE_EVENT_SPACE_ID, CLAP_EVENT_PARAM_VALUE, clap_event_header_t,
        clap_event_param_value_t, clap_input_events_t,
    };
    use clap_rs::clap_sys::process::clap_process_t;
    use std::ffi::c_void;
    use std::ptr;

    unsafe extern "C" fn event_count(_list: *const clap_input_events_t) -> u32 {
        1
    }

    unsafe extern "C" fn event_get(
        list: *const clap_input_events_t,
        index: u32,
    ) -> *const clap_event_header_t {
        if index == 0 {
            unsafe { (*list).ctx as *const clap_event_header_t }
        } else {
            ptr::null()
        }
    }

    #[test]
    fn direct_clap_effect_callback_does_not_allocate() {
        unsafe {
            let plugin = PluginEntry::create_plugin::<Gain>(ptr::null(), ptr::null());
            assert!(((*plugin).init.unwrap())(plugin));
            assert!(((*plugin).activate.unwrap())(plugin, 48_000.0, 1, 16));
            assert!(((*plugin).start_processing.unwrap())(plugin));

            let input_left = [1.0_f32; 16];
            let input_right = [0.5_f32; 16];
            let mut output_left = [0.0_f32; 16];
            let mut output_right = [0.0_f32; 16];
            let mut input_channels = [
                input_left.as_ptr() as *mut f32,
                input_right.as_ptr() as *mut f32,
            ];
            let mut output_channels = [output_left.as_mut_ptr(), output_right.as_mut_ptr()];
            let input = clap_audio_buffer_t {
                data32: input_channels.as_mut_ptr(),
                data64: ptr::null_mut(),
                channel_count: 2,
                latency: 0,
                constant_mask: 0,
            };
            let mut output = clap_audio_buffer_t {
                data32: output_channels.as_mut_ptr(),
                data64: ptr::null_mut(),
                channel_count: 2,
                latency: 0,
                constant_mask: 0,
            };
            let mut gain_event = clap_event_param_value_t {
                header: clap_event_header_t {
                    size: std::mem::size_of::<clap_event_param_value_t>() as u32,
                    time: 0,
                    space_id: CLAP_CORE_EVENT_SPACE_ID,
                    type_: CLAP_EVENT_PARAM_VALUE,
                    flags: 0,
                },
                param_id: 0,
                cookie: ptr::null_mut(),
                note_id: -1,
                port_index: -1,
                channel: -1,
                key: -1,
                value: 0.25,
            };
            let events = clap_input_events_t {
                ctx: &mut gain_event as *mut _ as *mut c_void,
                size: Some(event_count),
                get: Some(event_get),
            };
            let process = clap_process_t {
                steady_time: 0,
                frames_count: 16,
                transport: ptr::null(),
                audio_inputs: &input,
                audio_outputs: &mut output,
                audio_inputs_count: 1,
                audio_outputs_count: 1,
                in_events: &events,
                out_events: ptr::null(),
            };

            let (status, allocator_calls) = realtime_test_support::count_allocator_calls(|| {
                ((*plugin).process.unwrap())(plugin, &process)
            });
            assert_eq!(status, CLAP_PROCESS_CONTINUE);
            assert_eq!(allocator_calls, 0);
            assert_eq!(output_left, [0.25; 16]);
            assert_eq!(output_right, [0.125; 16]);

            ((*plugin).stop_processing.unwrap())(plugin);
            ((*plugin).deactivate.unwrap())(plugin);
            ((*plugin).destroy.unwrap())(plugin);
        }
    }
}
