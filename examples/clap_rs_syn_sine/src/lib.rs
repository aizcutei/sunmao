use clap_rs::events::Event;
use clap_rs::process::ProcessContext;
use clap_rs::{
    AudioPortInfo, AudioProcessor, CLAP_PROCESS_CONTINUE, HostHandle, NotePortInfo, ParameterInfo,
    Plugin, PluginInfo,
};
use std::f64::consts::PI;
use std::ffi::c_char;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

struct Sine {
    _host: HostHandle,
    frequency: Arc<AtomicU64>,
    gain: Arc<AtomicU64>,
}

struct SineProcessor {
    frequency: Arc<AtomicU64>,
    gain: Arc<AtomicU64>,
    phase: f64,
    sample_rate: f64,
    active_voices: i32,
}

impl Plugin for Sine {
    type AudioProcessor = SineProcessor;

    fn new(host: HostHandle) -> Self {
        Self {
            _host: host,
            frequency: Arc::new(AtomicU64::new(440.0f64.to_bits())),
            gain: Arc::new(AtomicU64::new(0.5f64.to_bits())),
        }
    }

    fn activate(
        &mut self,
        sample_rate: f64,
        _min_frames: u32,
        _max_frames: u32,
    ) -> Option<Self::AudioProcessor> {
        Some(SineProcessor {
            frequency: self.frequency.clone(),
            gain: self.gain.clone(),
            phase: 0.0,
            sample_rate,
            active_voices: 0,
        })
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
                is_stepped: false,
            },
            ParameterInfo {
                id: 1,
                name: "Gain".to_string(),
                module: "".to_string(),
                min_value: 0.0,
                max_value: 1.0,
                default_value: 0.5,
                is_stepped: false,
            },
        ]
    }

    fn audio_ports_config(&self) -> Vec<AudioPortInfo> {
        vec![AudioPortInfo {
            id: 0,
            name: "Main Out".to_string(),
            channel_count: 2,
            is_main: true,
            is_input: false,
        }]
    }

    fn note_ports_config(&self) -> Vec<NotePortInfo> {
        vec![NotePortInfo {
            id: 0,
            name: "Notes".to_string(),
            is_input: true,
        }]
    }

    fn get_parameter(&self, id: u32) -> f64 {
        match id {
            0 => f64::from_bits(self.frequency.load(Ordering::Relaxed)),
            1 => f64::from_bits(self.gain.load(Ordering::Relaxed)),
            _ => 0.0,
        }
    }

    fn set_parameter(&mut self, id: u32, value: f64) {
        match id {
            0 => self.frequency.store(value.to_bits(), Ordering::Relaxed),
            1 => self.gain.store(value.to_bits(), Ordering::Relaxed),
            _ => {}
        }
    }
}

impl AudioProcessor for SineProcessor {
    fn process(&mut self, mut ctx: ProcessContext) -> clap_rs::clap_process_status {
        let frames = ctx.frames_count as usize;
        for output in &mut ctx.audio_outputs {
            let len = frames.min(output.len());
            output[..len].fill(0.0);
        }

        let mut events = ctx.events().peekable();
        for i in 0..frames {
            while events
                .peek()
                .is_some_and(|event| event_time(event) as usize <= i)
            {
                self.handle_event(events.next().expect("peeked event"));
            }
            if self.active_voices == 0 {
                continue;
            }
            let frequency = f64::from_bits(self.frequency.load(Ordering::Relaxed));
            let gain = f64::from_bits(self.gain.load(Ordering::Relaxed)) as f32;
            let phase_inc = 2.0 * PI * frequency / self.sample_rate;
            let sample = (self.phase as f32).sin() * gain;
            for output in &mut ctx.audio_outputs {
                if i < output.len() {
                    output[i] = sample;
                }
            }
            self.phase += phase_inc;
            if self.phase > 2.0 * PI {
                self.phase -= 2.0 * PI;
            }
        }

        CLAP_PROCESS_CONTINUE
    }

    fn set_parameter(&mut self, id: u32, value: f64) {
        match id {
            0 => self.frequency.store(value.to_bits(), Ordering::Relaxed),
            1 => self.gain.store(value.to_bits(), Ordering::Relaxed),
            _ => {}
        }
    }
}

impl SineProcessor {
    fn handle_event(&mut self, event: Event) {
        match event {
            Event::NoteOn(event) => self.note_on(event.key as u8),
            Event::NoteOff(_) => self.active_voices = 0,
            Event::ParamValue(event) => self.set_parameter(event.param_id, event.value),
            Event::Midi(event) if event.data[0] & 0xf0 == 0x90 && event.data[2] > 0 => {
                self.note_on(event.data[1]);
            }
            Event::Midi(event)
                if event.data[0] & 0xf0 == 0x80
                    || (event.data[0] & 0xf0 == 0x90 && event.data[2] == 0) =>
            {
                self.active_voices = 0;
            }
            _ => {}
        }
    }

    fn note_on(&mut self, key: u8) {
        let frequency = 440.0 * (2.0f64).powf((key as f64 - 69.0) / 12.0);
        self.frequency.store(frequency.to_bits(), Ordering::Relaxed);
        self.active_voices = 1;
    }
}

fn event_time(event: &Event) -> u32 {
    match event {
        Event::NoteOn(event) | Event::NoteOff(event) => event.time,
        Event::ParamValue(event) => event.time,
        Event::Midi(event) => event.time,
        Event::Unknown => 0,
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
    std::ptr::null(),
]);

export_clap_plugin!(
    Sine,
    PluginInfo {
        id: "com.sunmao.clap_rs.syn_sine",
        name: "Clap Rs Syn Sine",
        vendor: "aizcutei",
        url: "https://aizcutei.github.io/sunmao",
        manual_url: "https://aizcutei.github.io/sunmao/manual",
        support_url: "https://aizcutei.github.io/sunmao/support",
        version: "0.1",
        description: "A simple sine synth using clap_rs",
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
        CLAP_CORE_EVENT_SPACE_ID, CLAP_EVENT_NOTE_ON, clap_event_header_t, clap_event_note_t,
        clap_input_events_t,
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
    fn direct_clap_synth_callback_does_not_allocate() {
        unsafe {
            let plugin = PluginEntry::create_plugin::<Sine>(ptr::null(), ptr::null());
            assert!(((*plugin).init.unwrap())(plugin));
            assert!(((*plugin).activate.unwrap())(plugin, 48_000.0, 1, 16));
            assert!(((*plugin).start_processing.unwrap())(plugin));

            let mut output_left = [0.0_f32; 16];
            let mut output_right = [0.0_f32; 16];
            let mut output_channels = [output_left.as_mut_ptr(), output_right.as_mut_ptr()];
            let mut output = clap_audio_buffer_t {
                data32: output_channels.as_mut_ptr(),
                data64: ptr::null_mut(),
                channel_count: 2,
                latency: 0,
                constant_mask: 0,
            };
            let mut note_on = clap_event_note_t {
                header: clap_event_header_t {
                    size: std::mem::size_of::<clap_event_note_t>() as u32,
                    time: 3,
                    space_id: CLAP_CORE_EVENT_SPACE_ID,
                    type_: CLAP_EVENT_NOTE_ON,
                    flags: 0,
                },
                note_id: 1,
                port_index: 0,
                channel: 0,
                key: 69,
                velocity: 1.0,
            };
            let events = clap_input_events_t {
                ctx: &mut note_on as *mut _ as *mut c_void,
                size: Some(event_count),
                get: Some(event_get),
            };
            let process = clap_process_t {
                steady_time: 0,
                frames_count: 16,
                transport: ptr::null(),
                audio_inputs: ptr::null(),
                audio_outputs: &mut output,
                audio_inputs_count: 0,
                audio_outputs_count: 1,
                in_events: &events,
                out_events: ptr::null(),
            };

            let (status, allocator_calls) = realtime_test_support::count_allocator_calls(|| {
                ((*plugin).process.unwrap())(plugin, &process)
            });
            assert_eq!(status, CLAP_PROCESS_CONTINUE);
            assert_eq!(allocator_calls, 0);
            assert_eq!(&output_left[..4], &[0.0; 4]);
            assert_eq!(&output_left, &output_right);
            assert!(output_left[4..].iter().any(|sample| *sample != 0.0));

            ((*plugin).stop_processing.unwrap())(plugin);
            ((*plugin).deactivate.unwrap())(plugin);
            ((*plugin).destroy.unwrap())(plugin);
        }
    }
}
