use clap_sys::audio_buffer::clap_audio_buffer_t;
use clap_sys::events::{
    clap_event_note_t, clap_event_param_value_t, clap_input_events_t, CLAP_EVENT_NOTE_CHOKE, CLAP_EVENT_NOTE_END,
    CLAP_EVENT_NOTE_OFF, CLAP_EVENT_NOTE_ON, CLAP_EVENT_PARAM_VALUE,
};
use clap_sys::ext::audio_ports::{clap_audio_port_info_t, clap_plugin_audio_ports_t, CLAP_AUDIO_PORT_IS_MAIN, CLAP_PORT_STEREO};
use clap_sys::ext::note_ports::{clap_note_port_info_t, clap_plugin_note_ports_t, CLAP_NOTE_DIALECT_CLAP, CLAP_NOTE_DIALECT_MIDI, CLAP_NOTE_DIALECT_MIDI2, CLAP_NOTE_DIALECT_MIDI_MPE};
use clap_sys::ext::params::{clap_param_info_t, clap_plugin_params_t, CLAP_PARAM_IS_AUTOMATABLE};
use clap_sys::factory::plugin_factory::{clap_plugin_factory_t, CLAP_PLUGIN_FACTORY_ID};
use clap_sys::host::clap_host_t;
use clap_sys::id::{clap_id, CLAP_INVALID_ID};
use clap_sys::plugin::{clap_plugin_descriptor_t, clap_plugin_t};
use clap_sys::process::{clap_process_t, clap_process_status, CLAP_PROCESS_CONTINUE};
use clap_sys::string_sizes::{CLAP_NAME_SIZE, CLAP_PATH_SIZE};
use clap_sys::version::{clap_version_is_compatible, CLAP_VERSION};
use clap_sys::plugin_features::{CLAP_PLUGIN_FEATURE_INSTRUMENT, CLAP_PLUGIN_FEATURE_STEREO, CLAP_PLUGIN_FEATURE_SYNTHESIZER};
use clap_sys::ext::audio_ports::CLAP_EXT_AUDIO_PORTS;
use clap_sys::ext::note_ports::CLAP_EXT_NOTE_PORTS;
use clap_sys::ext::params::CLAP_EXT_PARAMS;
use clap_sys::ext::state::{clap_plugin_state_t, CLAP_EXT_STATE};
use clap_sys::stream::{clap_istream_t, clap_ostream_t};
use std::ffi::{c_char, CStr};
use std::ptr;

const PARAM_GAIN: clap_id = 0;

struct SineSynth {
    host: *const clap_host_t,
    sample_rate: f64,
    phase: f64,
    freq: f64,
    gain: f64,
    gate: bool,
}

struct SyncDescriptor(clap_plugin_descriptor_t);
unsafe impl Sync for SyncDescriptor {}

struct SyncFeatures(&'static [*const c_char]);
unsafe impl Sync for SyncFeatures {}

static FEATURES: SyncFeatures = SyncFeatures(&[
    CLAP_PLUGIN_FEATURE_INSTRUMENT.as_ptr() as *const c_char,
    CLAP_PLUGIN_FEATURE_SYNTHESIZER.as_ptr() as *const c_char,
    CLAP_PLUGIN_FEATURE_STEREO.as_ptr() as *const c_char,
    ptr::null(),
]);

static DESCRIPTOR: SyncDescriptor = SyncDescriptor(clap_plugin_descriptor_t {
    clap_version: CLAP_VERSION,
    id: b"com.sunmao.clap_sys.syn_sine\0".as_ptr() as *const c_char,
    name: b"Clap Sys Syn Sine\0".as_ptr() as *const c_char,
    vendor: b"aizcutei\0".as_ptr() as *const c_char,
    url: b"https://aizcutei.github.io/sunmao\0".as_ptr() as *const c_char,
    manual_url: b"https://aizcutei.github.io/sunmao/manual\0".as_ptr() as *const c_char,
    support_url: b"https://aizcutei.github.io/sunmao/support\0".as_ptr() as *const c_char,
    version: b"0.1\0".as_ptr() as *const c_char,
    description: b"Simple sine synth\0".as_ptr() as *const c_char,
    features: FEATURES.0.as_ptr(),
});

unsafe extern "C" fn plugin_init(_plugin: *const clap_plugin_t) -> bool {
    true
}

unsafe extern "C" fn plugin_destroy(plugin: *const clap_plugin_t) {
    unsafe {
        let instance = (*plugin).plugin_data as *mut SineSynth;
        if !instance.is_null() {
            let _ = Box::from_raw(instance);
        }
        let _ = Box::from_raw(plugin as *mut clap_plugin_t);
    }
}

unsafe extern "C" fn plugin_activate(plugin: *const clap_plugin_t, sample_rate: f64, _min_frames_count: u32, _max_frames_count: u32) -> bool {
    let synth = &mut *((*plugin).plugin_data as *mut SineSynth);
    synth.sample_rate = sample_rate.max(1.0);
    true
}

unsafe extern "C" fn plugin_deactivate(_plugin: *const clap_plugin_t) {}

unsafe extern "C" fn plugin_start_processing(_plugin: *const clap_plugin_t) -> bool {
    true
}

unsafe extern "C" fn plugin_stop_processing(_plugin: *const clap_plugin_t) {}

unsafe extern "C" fn plugin_reset(plugin: *const clap_plugin_t) {
    let synth = &mut *((*plugin).plugin_data as *mut SineSynth);
    synth.phase = 0.0;
}

fn write_cstr_to_array(dst: &mut [c_char], bytes: &[u8]) {
    dst.fill(0);
    let max = dst.len().saturating_sub(1);
    let len = bytes.len().saturating_sub(1).min(max);
    for i in 0..len {
        dst[i] = bytes[i] as c_char;
    }
}

fn midi_note_to_freq(note: i16) -> f64 {
    440.0 * (2.0_f64).powf((note as f64 - 69.0) / 12.0)
}

unsafe fn handle_event(synth: &mut SineSynth, header: *const clap_sys::events::clap_event_header_t) {
    if header.is_null() {
        return;
    }
    if (*header).space_id != clap_sys::events::CLAP_CORE_EVENT_SPACE_ID {
        return;
    }
    match (*header).type_ {
        CLAP_EVENT_NOTE_ON => {
            let event = &*(header as *const clap_event_note_t);
            synth.freq = midi_note_to_freq(event.key);
            synth.gate = event.velocity > 0.0;
        }
        CLAP_EVENT_NOTE_OFF | CLAP_EVENT_NOTE_END | CLAP_EVENT_NOTE_CHOKE => {
            synth.gate = false;
        }
        CLAP_EVENT_PARAM_VALUE => {
            let event = &*(header as *const clap_event_param_value_t);
            if event.param_id == PARAM_GAIN {
                synth.gain = event.value.clamp(0.0, 1.0);
            }
        }
        _ => {}
    }
}

unsafe fn process_audio_f32(synth: &mut SineSynth, output: &mut clap_audio_buffer_t, frames: usize) {
    if output.data32.is_null() {
        return;
    }
    let out_channels = std::slice::from_raw_parts_mut(output.data32, output.channel_count as usize);
    let channels = out_channels.len();
    let sample_rate = synth.sample_rate;
    for i in 0..frames {
        let sample = if synth.gate {
            let value = (synth.phase).sin() * synth.gain;
            synth.phase += std::f64::consts::TAU * synth.freq / sample_rate;
            if synth.phase >= std::f64::consts::TAU {
                synth.phase -= std::f64::consts::TAU;
            }
            value as f32
        } else {
            0.0
        };
        for ch in 0..channels {
            let out_ptr = out_channels[ch];
            if out_ptr.is_null() {
                continue;
            }
            let out_buf = std::slice::from_raw_parts_mut(out_ptr, frames);
            out_buf[i] = sample;
        }
    }
}

unsafe fn process_audio_f64(synth: &mut SineSynth, output: &mut clap_audio_buffer_t, frames: usize) {
    if output.data64.is_null() {
        return;
    }
    let out_channels = std::slice::from_raw_parts_mut(output.data64, output.channel_count as usize);
    let channels = out_channels.len();
    let sample_rate = synth.sample_rate;
    for i in 0..frames {
        let sample = if synth.gate {
            let value = (synth.phase).sin() * synth.gain;
            synth.phase += std::f64::consts::TAU * synth.freq / sample_rate;
            if synth.phase >= std::f64::consts::TAU {
                synth.phase -= std::f64::consts::TAU;
            }
            value
        } else {
            0.0
        };
        for ch in 0..channels {
            let out_ptr = out_channels[ch];
            if out_ptr.is_null() {
                continue;
            }
            let out_buf = std::slice::from_raw_parts_mut(out_ptr, frames);
            out_buf[i] = sample;
        }
    }
}

unsafe extern "C" fn plugin_process(plugin: *const clap_plugin_t, process: *const clap_process_t) -> clap_process_status {
    let synth = &mut *((*plugin).plugin_data as *mut SineSynth);
    let process = &*process;

    if process.audio_outputs.is_null() || process.audio_outputs_count == 0 {
        return CLAP_PROCESS_CONTINUE;
    }
    let frames = process.frames_count as usize;
    let output = &mut *process.audio_outputs;

    let in_events = process.in_events;
    let mut ev_index = 0;
    let mut next_ev_frame = frames as u32;
    let (size_fn, get_fn) = if in_events.is_null() {
        (None, None)
    } else {
        ((*in_events).size, (*in_events).get)
    };
    let total_events = if let Some(size_fn) = size_fn {
        size_fn(in_events)
    } else {
        0
    };
    if total_events > 0 {
        if let Some(get_fn) = get_fn {
            let header = get_fn(in_events, 0);
            if !header.is_null() {
                next_ev_frame = (*header).time.min(frames as u32);
            }
        }
    }

    let mut frame = 0usize;
    while frame < frames {
        while ev_index < total_events {
            let header = get_fn.unwrap()(in_events, ev_index);
            if header.is_null() {
                ev_index += 1;
                continue;
            }
            if (*header).time != frame as u32 {
                next_ev_frame = (*header).time.min(frames as u32);
                break;
            }
            handle_event(synth, header);
            ev_index += 1;
            if ev_index == total_events {
                next_ev_frame = frames as u32;
            }
        }

        let end = next_ev_frame.min(frames as u32) as usize;
        if !output.data32.is_null() {
            for i in frame..end {
                let sample = if synth.gate {
                    let value = (synth.phase).sin() * synth.gain;
                    synth.phase += std::f64::consts::TAU * synth.freq / synth.sample_rate;
                    if synth.phase >= std::f64::consts::TAU {
                        synth.phase -= std::f64::consts::TAU;
                    }
                    value as f32
                } else {
                    0.0
                };
                let out_channels = std::slice::from_raw_parts_mut(output.data32, output.channel_count as usize);
                for ch in 0..out_channels.len() {
                    let out_ptr = out_channels[ch];
                    if out_ptr.is_null() {
                        continue;
                    }
                    let out_buf = std::slice::from_raw_parts_mut(out_ptr, frames);
                    out_buf[i] = sample;
                }
            }
        } else if !output.data64.is_null() {
            for i in frame..end {
                let sample = if synth.gate {
                    let value = (synth.phase).sin() * synth.gain;
                    synth.phase += std::f64::consts::TAU * synth.freq / synth.sample_rate;
                    if synth.phase >= std::f64::consts::TAU {
                        synth.phase -= std::f64::consts::TAU;
                    }
                    value
                } else {
                    0.0
                };
                let out_channels = std::slice::from_raw_parts_mut(output.data64, output.channel_count as usize);
                for ch in 0..out_channels.len() {
                    let out_ptr = out_channels[ch];
                    if out_ptr.is_null() {
                        continue;
                    }
                    let out_buf = std::slice::from_raw_parts_mut(out_ptr, frames);
                    out_buf[i] = sample;
                }
            }
        }

        frame = end;
    }

    CLAP_PROCESS_CONTINUE
}

unsafe extern "C" fn plugin_get_extension(_plugin: *const clap_plugin_t, id: *const c_char) -> *const std::ffi::c_void {
    if id.is_null() {
        return ptr::null();
    }
    let id_cstr = CStr::from_ptr(id);
    if id_cstr.to_bytes_with_nul() == CLAP_EXT_AUDIO_PORTS.as_bytes() {
        return &AUDIO_PORTS as *const _ as *const std::ffi::c_void;
    }
    if id_cstr.to_bytes_with_nul() == CLAP_EXT_NOTE_PORTS.as_bytes() {
        return &NOTE_PORTS as *const _ as *const std::ffi::c_void;
    }
    if id_cstr.to_bytes_with_nul() == CLAP_EXT_PARAMS.as_bytes() {
        return &PARAMS as *const _ as *const std::ffi::c_void;
    }
    if id_cstr.to_bytes_with_nul() == CLAP_EXT_STATE.as_bytes() {
        return &STATE as *const _ as *const std::ffi::c_void;
    }
    ptr::null()
}

unsafe extern "C" fn plugin_on_main_thread(_plugin: *const clap_plugin_t) {}

unsafe extern "C" fn audio_ports_count(_plugin: *const clap_plugin_t, is_input: bool) -> u32 {
    if is_input { 0 } else { 1 }
}

unsafe extern "C" fn audio_ports_get(_plugin: *const clap_plugin_t, index: u32, is_input: bool, info: *mut clap_audio_port_info_t) -> bool {
    if is_input || index != 0 || info.is_null() {
        return false;
    }
    let info = &mut *info;
    info.id = 0;
    write_cstr_to_array(&mut info.name, b"Main Out\0");
    info.flags = CLAP_AUDIO_PORT_IS_MAIN;
    info.channel_count = 2;
    info.port_type = CLAP_PORT_STEREO.as_ptr() as *const c_char;
    info.in_place_pair = CLAP_INVALID_ID;
    true
}

static AUDIO_PORTS: clap_plugin_audio_ports_t = clap_plugin_audio_ports_t {
    count: Some(audio_ports_count),
    get: Some(audio_ports_get),
};

unsafe extern "C" fn note_ports_count(_plugin: *const clap_plugin_t, is_input: bool) -> u32 {
    if is_input { 1 } else { 0 }
}

unsafe extern "C" fn note_ports_get(_plugin: *const clap_plugin_t, index: u32, is_input: bool, info: *mut clap_note_port_info_t) -> bool {
    if !is_input || index != 0 || info.is_null() {
        return false;
    }
    let info = &mut *info;
    info.id = 0;
    info.supported_dialects = CLAP_NOTE_DIALECT_CLAP | CLAP_NOTE_DIALECT_MIDI | CLAP_NOTE_DIALECT_MIDI_MPE | CLAP_NOTE_DIALECT_MIDI2;
    info.preferred_dialect = CLAP_NOTE_DIALECT_CLAP;
    write_cstr_to_array(&mut info.name, b"Notes\0");
    true
}

static NOTE_PORTS: clap_plugin_note_ports_t = clap_plugin_note_ports_t {
    count: Some(note_ports_count),
    get: Some(note_ports_get),
};

unsafe extern "C" fn params_count(_plugin: *const clap_plugin_t) -> u32 {
    1
}

unsafe extern "C" fn params_get_info(_plugin: *const clap_plugin_t, param_index: u32, param_info: *mut clap_param_info_t) -> bool {
    if param_index != 0 || param_info.is_null() {
        return false;
    }
    let info = &mut *param_info;
    info.id = PARAM_GAIN;
    info.flags = CLAP_PARAM_IS_AUTOMATABLE;
    info.cookie = ptr::null_mut();
    write_cstr_to_array(&mut info.name, b"Gain\0");
    info.module = [0; CLAP_PATH_SIZE];
    info.min_value = 0.0;
    info.max_value = 1.0;
    info.default_value = 0.2;
    true
}

unsafe extern "C" fn params_get_value(plugin: *const clap_plugin_t, param_id: clap_id, out_value: *mut f64) -> bool {
    if out_value.is_null() || param_id != PARAM_GAIN {
        return false;
    }
    let synth = &*((*plugin).plugin_data as *const SineSynth);
    *out_value = synth.gain;
    true
}

unsafe extern "C" fn params_value_to_text(_plugin: *const clap_plugin_t, param_id: clap_id, value: f64, out_buffer: *mut c_char, out_buffer_capacity: u32) -> bool {
    if param_id != PARAM_GAIN || out_buffer.is_null() || out_buffer_capacity == 0 {
        return false;
    }
    let text = format!("{:.3}", value);
    let bytes = text.as_bytes();
    let capacity = out_buffer_capacity as usize;
    let len = bytes.len().min(capacity.saturating_sub(1));
    let dst = std::slice::from_raw_parts_mut(out_buffer, capacity);
    dst.fill(0);
    for i in 0..len {
        dst[i] = bytes[i] as c_char;
    }
    true
}

unsafe extern "C" fn params_text_to_value(_plugin: *const clap_plugin_t, param_id: clap_id, param_value_text: *const c_char, out_value: *mut f64) -> bool {
    if param_id != PARAM_GAIN || param_value_text.is_null() || out_value.is_null() {
        return false;
    }
    let text = CStr::from_ptr(param_value_text);
    if let Ok(value_str) = text.to_str() {
        if let Ok(parsed) = value_str.parse::<f64>() {
            *out_value = parsed.clamp(0.0, 1.0);
            return true;
        }
    }
    false
}

unsafe extern "C" fn params_flush(plugin: *const clap_plugin_t, input: *const clap_input_events_t, _output: *const clap_sys::events::clap_output_events_t) {
    if input.is_null() {
        return;
    }
    let synth = &mut *((*plugin).plugin_data as *mut SineSynth);
    let size_fn = (*input).size;
    let get_fn = (*input).get;
    if size_fn.is_none() || get_fn.is_none() {
        return;
    }
    let size = size_fn.unwrap()(input);
    for index in 0..size {
        let header = get_fn.unwrap()(input, index);
        if header.is_null() {
            continue;
        }
        handle_event(synth, header);
    }
}

static PARAMS: clap_plugin_params_t = clap_plugin_params_t {
    count: Some(params_count),
    get_info: Some(params_get_info),
    get_value: Some(params_get_value),
    value_to_text: Some(params_value_to_text),
    text_to_value: Some(params_text_to_value),
    flush: Some(params_flush),
};

fn stream_write_all(stream: *const clap_ostream_t, mut buffer: *const u8, mut size: usize) -> bool {
    if stream.is_null() {
        return false;
    }
    let write_fn = unsafe { (*stream).write };
    let Some(write_fn) = write_fn else { return false };
    while size > 0 {
        let written = unsafe { write_fn(stream, buffer as *const _, size as u64) };
        if written <= 0 {
            return false;
        }
        let written = written as usize;
        buffer = unsafe { buffer.add(written) };
        size -= written;
    }
    true
}

fn stream_read_exact(stream: *const clap_istream_t, mut buffer: *mut u8, mut size: usize) -> bool {
    if stream.is_null() {
        return false;
    }
    let read_fn = unsafe { (*stream).read };
    let Some(read_fn) = read_fn else { return false };
    while size > 0 {
        let read = unsafe { read_fn(stream, buffer as *mut _, size as u64) };
        if read <= 0 {
            return false;
        }
        let read = read as usize;
        buffer = unsafe { buffer.add(read) };
        size -= read;
    }
    true
}

unsafe extern "C" fn state_save(plugin: *const clap_plugin_t, stream: *const clap_ostream_t) -> bool {
    if plugin.is_null() {
        return false;
    }
    let synth = &*((*plugin).plugin_data as *const SineSynth);
    let mut buffer = [0u8; 24];
    buffer[0..8].copy_from_slice(&synth.gain.to_le_bytes());
    buffer[8..16].copy_from_slice(&synth.freq.to_le_bytes());
    buffer[16..24].copy_from_slice(&synth.phase.to_le_bytes());
    if !stream_write_all(stream, buffer.as_ptr(), buffer.len()) {
        return false;
    }
    let gate = if synth.gate { 1u8 } else { 0u8 };
    stream_write_all(stream, &gate as *const u8, 1)
}

unsafe extern "C" fn state_load(plugin: *const clap_plugin_t, stream: *const clap_istream_t) -> bool {
    if plugin.is_null() {
        return false;
    }
    let mut buffer = [0u8; 24];
    if !stream_read_exact(stream, buffer.as_mut_ptr(), buffer.len()) {
        return false;
    }
    let mut gate_byte = [0u8; 1];
    if !stream_read_exact(stream, gate_byte.as_mut_ptr(), 1) {
        return false;
    }
    let synth = &mut *((*plugin).plugin_data as *mut SineSynth);
    synth.gain = f64::from_le_bytes(buffer[0..8].try_into().unwrap()).clamp(0.0, 1.0);
    synth.freq = f64::from_le_bytes(buffer[8..16].try_into().unwrap()).max(1.0);
    synth.phase = f64::from_le_bytes(buffer[16..24].try_into().unwrap());
    synth.gate = gate_byte[0] != 0;
    true
}

static STATE: clap_plugin_state_t = clap_plugin_state_t {
    save: Some(state_save),
    load: Some(state_load),
};

unsafe extern "C" fn get_plugin_count(_factory: *const clap_plugin_factory_t) -> u32 {
    1
}

unsafe extern "C" fn get_plugin_descriptor(_factory: *const clap_plugin_factory_t, index: u32) -> *const clap_plugin_descriptor_t {
    if index == 0 {
        &DESCRIPTOR.0
    } else {
        ptr::null()
    }
}

unsafe extern "C" fn create_plugin(_factory: *const clap_plugin_factory_t, host: *const clap_host_t, plugin_id: *const c_char) -> *const clap_plugin_t {
    if host.is_null() || plugin_id.is_null() {
        return ptr::null();
    }
    if !clap_version_is_compatible((*host).clap_version) {
        return ptr::null();
    }
    let id_cstr = CStr::from_ptr(plugin_id);
    let target_id = CStr::from_ptr(DESCRIPTOR.0.id);
    if id_cstr != target_id {
        return ptr::null();
    }
    let instance = Box::new(SineSynth {
        host,
        sample_rate: 44100.0,
        phase: 0.0,
        freq: 440.0,
        gain: 0.2,
        gate: false,
    });
    let plugin = Box::new(clap_plugin_t {
        desc: &DESCRIPTOR.0,
        plugin_data: Box::into_raw(instance) as *mut _,
        init: Some(plugin_init),
        destroy: Some(plugin_destroy),
        activate: Some(plugin_activate),
        deactivate: Some(plugin_deactivate),
        start_processing: Some(plugin_start_processing),
        stop_processing: Some(plugin_stop_processing),
        reset: Some(plugin_reset),
        process: Some(plugin_process),
        get_extension: Some(plugin_get_extension),
        on_main_thread: Some(plugin_on_main_thread),
    });
    Box::into_raw(plugin)
}

static PLUGIN_FACTORY: clap_plugin_factory_t = clap_plugin_factory_t {
    get_plugin_count: Some(get_plugin_count),
    get_plugin_descriptor: Some(get_plugin_descriptor),
    create_plugin: Some(create_plugin),
};

unsafe extern "C" fn entry_init(_plugin_path: *const c_char) -> bool {
    true
}

unsafe extern "C" fn entry_deinit() {}

unsafe extern "C" fn entry_get_factory(factory_id: *const c_char) -> *const std::ffi::c_void {
    if factory_id.is_null() {
        return ptr::null();
    }
    let id_cstr = CStr::from_ptr(factory_id);
    let factory_id_cstr = CStr::from_ptr(CLAP_PLUGIN_FACTORY_ID.as_ptr() as *const c_char);
    if id_cstr == factory_id_cstr {
        &PLUGIN_FACTORY as *const _ as *const std::ffi::c_void
    } else {
        ptr::null()
    }
}

#[unsafe(no_mangle)]
pub static clap_entry: clap_sys::entry::clap_plugin_entry_t = clap_sys::entry::clap_plugin_entry_t {
    clap_version: CLAP_VERSION,
    init: Some(entry_init),
    deinit: Some(entry_deinit),
    get_factory: Some(entry_get_factory),
};
