//! CLAP Backend Adapter for SunMao.
//!
//! This crate wraps `SunmaoPlugin` to expose it as a CLAP plugin via `clap_rs`.

use clap_rs::ext::gui::{GuiApi, GuiHandler, GuiResizeHints};
use clap_rs::gui::prepare_view;
use clap_rs::process::ProcessContext;
use clap_rs::{
    clap_sys::process::clap_process_status, events::Event as ClapEvent, AudioPortInfo, HostHandle,
    NotePortInfo, ParameterInfo, Plugin,
};
use raw_window_handle::{AppKitWindowHandle, RawWindowHandle, Win32WindowHandle, XlibWindowHandle};
use std::ffi::c_void;
use std::num::NonZeroIsize;
use std::ptr::NonNull;
use std::sync::Arc;
use sunmao_core::events::MidiMessage;
use sunmao_core::plugin::ProcessContext as SunmaoProcessContext;
use sunmao_core::view::ParamsViewContext;
use sunmao_core::{AudioBuffer, Event as SunmaoEvent, EventQueue, Params, SunmaoPlugin};
use sunmao_core::{ParentWindow, ViewHandle};

pub use clap_rs::{export_clap_plugin, export_clap_plugin_with_gui, PluginInfo};

/// Wrapper for ViewHandle that is Send+Sync (unsafe).
/// GUI handles are only accessed on the main thread.
struct ThreadSafeViewHandle(ViewHandle);
unsafe impl Send for ThreadSafeViewHandle {}
unsafe impl Sync for ThreadSafeViewHandle {}

/// Wrapper that adapts a SunmaoPlugin to clap_rs::Plugin.
pub struct SunmaoClapWrapper<P: SunmaoPlugin> {
    plugin: P,
    params: Arc<P::Params>,
    sample_rate: f64,
    max_frames: u32,
    // Temporary buffers for SunmaoPlugin processing
    input_buffers: Vec<Vec<f32>>,
    output_buffers: Vec<Vec<f32>>,
    // GUI State
    view_handle: Option<ThreadSafeViewHandle>,
    gui_api: Option<GuiApi>,
}

unsafe impl<P: SunmaoPlugin> Send for SunmaoClapWrapper<P> {}
unsafe impl<P: SunmaoPlugin> Sync for SunmaoClapWrapper<P> {}

impl<P: SunmaoPlugin> Plugin for SunmaoClapWrapper<P> {
    type AudioProcessor = ();

    fn new(_host: HostHandle) -> Self {
        let plugin = P::default();
        let params = plugin.params();
        let in_ch = plugin.input_channels() as usize;
        let out_ch = plugin.output_channels() as usize;

        Self {
            plugin,
            params,
            sample_rate: 44100.0,
            max_frames: 4096,
            input_buffers: vec![vec![0.0; 4096]; in_ch.max(2)],
            output_buffers: vec![vec![0.0; 4096]; out_ch.max(2)],
            view_handle: None,
            gui_api: None,
        }
    }

    fn activate(&mut self, sample_rate: f64, _min_frames: u32, max_frames: u32) -> bool {
        self.sample_rate = sample_rate;
        self.max_frames = max_frames;

        // Resize buffers
        for buf in &mut self.input_buffers {
            buf.resize(max_frames as usize, 0.0);
        }
        for buf in &mut self.output_buffers {
            buf.resize(max_frames as usize, 0.0);
        }

        self.plugin.initialize(sample_rate, max_frames);
        true
    }

    fn deactivate(&mut self) {
        self.plugin.reset();
    }

    fn audio_ports_config(&self) -> Vec<AudioPortInfo> {
        let in_ch = self.plugin.input_channels();
        let out_ch = self.plugin.output_channels();

        let mut ports = vec![];
        if in_ch > 0 {
            ports.push(AudioPortInfo {
                id: 0,
                name: "Input".to_string(),
                channel_count: in_ch,
                is_main: true,
                is_input: true,
            });
        }
        if out_ch > 0 {
            ports.push(AudioPortInfo {
                id: 1,
                name: "Output".to_string(),
                channel_count: out_ch,
                is_main: true,
                is_input: false,
            });
        }
        ports
    }

    fn note_ports_config(&self) -> Vec<NotePortInfo> {
        if self.plugin.accepts_midi() {
            vec![NotePortInfo {
                id: 0,
                name: "MIDI In".to_string(),
                is_input: true,
            }]
        } else {
            vec![]
        }
    }

    fn declare_parameters(&self) -> Vec<ParameterInfo> {
        let ids = P::Params::ids();
        ids.iter()
            .enumerate()
            .map(|(idx, &id)| ParameterInfo {
                id: idx as u32,
                name: id.to_string(),
                module: "".to_string(),
                min_value: 0.0,
                max_value: 1.0,
                default_value: 0.5,
            })
            .collect()
    }

    fn get_parameter(&self, id: u32) -> f64 {
        let ids = P::Params::ids();
        if let Some(&param_id) = ids.get(id as usize) {
            self.params.get_normalized(param_id).unwrap_or(0.0) as f64
        } else {
            0.0
        }
    }

    fn set_parameter(&mut self, id: u32, value: f64) {
        let ids = P::Params::ids();
        if let Some(&param_id) = ids.get(id as usize) {
            self.params.set_normalized(param_id, value as f32);
        }
    }

    fn process(&mut self, mut ctx: ProcessContext) -> clap_process_status {
        let frames = ctx.frames_count as usize;
        let is_synth = self.plugin.input_channels() == 0;

        // Collect MIDI events from CLAP input events
        let mut event_queue = EventQueue::new();
        for event in ctx.events() {
            match event {
                ClapEvent::NoteOn(note) => {
                    let midi = MidiMessage::note_on(
                        0, // offset - CLAP doesn't provide sample offset easily
                        note.channel as u8,
                        note.key as u8,
                        (note.velocity * 127.0) as u8,
                    );
                    event_queue.push(SunmaoEvent::Midi(midi));
                }
                ClapEvent::NoteOff(note) => {
                    let midi = MidiMessage::note_off(
                        0,
                        note.channel as u8,
                        note.key as u8,
                        (note.velocity * 127.0) as u8,
                    );
                    event_queue.push(SunmaoEvent::Midi(midi));
                }
                ClapEvent::Midi(midi) => {
                    let msg = MidiMessage {
                        offset: 0,
                        data: midi.data,
                    };
                    event_queue.push(SunmaoEvent::Midi(msg));
                }
                _ => {}
            }
        }

        // Copy CLAP input to temp buffers
        for (ch, input) in ctx.audio_inputs.iter().enumerate() {
            if ch < self.input_buffers.len() {
                let len = frames.min(input.len()).min(self.input_buffers[ch].len());
                self.input_buffers[ch][..len].copy_from_slice(&input[..len]);
            }
        }

        // Prepare output buffers
        if is_synth {
            // Synth: clear output buffers
            for buf in &mut self.output_buffers {
                for sample in buf.iter_mut().take(frames) {
                    *sample = 0.0;
                }
            }
        } else {
            // Effect: pre-copy input to output
            for ch in 0..self.input_buffers.len().min(self.output_buffers.len()) {
                let len = frames
                    .min(self.input_buffers[ch].len())
                    .min(self.output_buffers[ch].len());
                self.output_buffers[ch][..len].copy_from_slice(&self.input_buffers[ch][..len]);
            }
        }

        // Create AudioBuffer for SunmaoPlugin
        let input_refs: Vec<&[f32]> = self.input_buffers.iter().map(|b| &b[..frames]).collect();
        let mut output_refs: Vec<&mut [f32]> = self
            .output_buffers
            .iter_mut()
            .map(|b| &mut b[..frames])
            .collect();

        let mut audio_buffer = AudioBuffer::new(&input_refs, &mut output_refs, frames);

        // Create process context
        let mut tempo = None;
        let mut is_playing = true;
        let mut sample_pos = 0i64;
        if let Some(transport) = ctx.transport() {
            tempo = transport.tempo();
            is_playing = transport.is_playing();
            if let Some(seconds) = transport.song_pos_seconds() {
                sample_pos = (seconds * self.sample_rate) as i64;
            }
        }
        let sunmao_ctx = SunmaoProcessContext {
            sample_rate: self.sample_rate,
            tempo,
            is_playing,
            sample_pos,
        };

        // Call the actual plugin process
        let _status = self
            .plugin
            .process(&mut audio_buffer, &event_queue, &sunmao_ctx);

        // Copy output back to CLAP buffers
        for (ch, output) in ctx.audio_outputs.iter_mut().enumerate() {
            if ch < self.output_buffers.len() {
                let len = frames.min(output.len()).min(self.output_buffers[ch].len());
                output[..len].copy_from_slice(&self.output_buffers[ch][..len]);
            }
        }

        clap_rs::CLAP_PROCESS_CONTINUE
    }
}

impl<P: SunmaoPlugin> GuiHandler for SunmaoClapWrapper<P> {
    fn is_api_supported(&self, api: GuiApi, is_floating: bool) -> bool {
        if is_floating {
            return false;
        }
        match api {
            GuiApi::Cocoa | GuiApi::Win32 | GuiApi::X11 => true,
            _ => false,
        }
    }

    fn preferred_api(&self) -> Option<(GuiApi, bool)> {
        #[cfg(target_os = "macos")]
        {
            Some((GuiApi::Cocoa, false))
        }
        #[cfg(target_os = "windows")]
        {
            Some((GuiApi::Win32, false))
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            Some((GuiApi::X11, false))
        }
    }

    fn gui_create(&mut self, api: GuiApi, is_floating: bool) -> bool {
        if is_floating || !self.is_api_supported(api, is_floating) {
            return false;
        }
        if self.plugin.view().is_none() {
            return false;
        }
        self.gui_api = Some(api);
        true
    }

    fn gui_destroy(&mut self) {
        self.view_handle = None;
        self.gui_api = None;
    }

    fn gui_get_size(&self) -> Option<(u32, u32)> {
        // Instantiate plugin just to get size
        let plugin = P::default();
        if let Some(view) = plugin.view() {
            Some(view.size())
        } else {
            None
        }
    }

    fn gui_set_parent(&mut self, window: *mut c_void) -> bool {
        if let Some(view) = self.plugin.view() {
            let mut raw_handle = match self.gui_api {
                Some(GuiApi::Cocoa) => {
                    let Some(ns_view) = NonNull::new(window) else {
                        return false;
                    };
                    RawWindowHandle::AppKit(AppKitWindowHandle::new(ns_view))
                }
                Some(GuiApi::Win32) => {
                    let Some(hwnd) = NonZeroIsize::new(window as isize) else {
                        return false;
                    };
                    RawWindowHandle::Win32(Win32WindowHandle::new(hwnd))
                }
                Some(GuiApi::X11) => {
                    if window.is_null() {
                        return false;
                    }
                    RawWindowHandle::Xlib(XlibWindowHandle::new(window as u64))
                }
                _ => return false,
            };

            if prepare_view(&mut raw_handle).is_err() {
                return false;
            }

            let parent_window = match self.gui_api {
                Some(GuiApi::Cocoa) => ParentWindow::AppKit(window),
                Some(GuiApi::Win32) => ParentWindow::Win32(window),
                Some(GuiApi::X11) => ParentWindow::X11(window as u32),
                _ => return false,
            };

            let context = Arc::new(ParamsViewContext::new(self.params.clone()));
            if let Some(handle) = view.open(parent_window, context) {
                self.view_handle = Some(ThreadSafeViewHandle(handle));
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    fn gui_show(&mut self) -> bool {
        self.view_handle.is_some()
    }

    fn gui_hide(&mut self) -> bool {
        true // Assuming hiding usually works if handle exists?
             // Or should we implement hide logic in ViewHandle?
             // baseview doesn't expose hide easily on handle without dropping it.
             // But usually hide means window is unmapped by parent.
    }
}

/// Entry type alias for sunmao_export! macro
pub type ClapEntry = clap_rs::clap_sys::entry::clap_plugin_entry_t;

/// Thread-safe wrapper for CLAP feature lists.
/// CLAP feature identifiers.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ClapFeature {
    Instrument,
    AudioEffect,
    NoteEffect,
    NoteDetector,
    Analyzer,
    Synthesizer,
    Sampler,
    Drum,
    DrumMachine,
    Filter,
    Phaser,
    Equalizer,
    DeEsser,
    PhaseVocoder,
    Granular,
    FrequencyShifter,
    PitchShifter,
    Distortion,
    TransientShaper,
    Compressor,
    Expander,
    Gate,
    Limiter,
    Flanger,
    Chorus,
    Delay,
    Reverb,
    Tremolo,
    Glitch,
    Utility,
    Mono,
    Stereo,
    Surround,
    Ambisonic,
    PitchCorrection,
    Restoration,
    MultiEffects,
    Mixing,
    Mastering,
}

impl ClapFeature {
    pub const fn as_ptr(self) -> *const std::ffi::c_char {
        match self {
            ClapFeature::Instrument => b"instrument\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::AudioEffect => b"audio-effect\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::NoteEffect => b"note-effect\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::NoteDetector => b"note-detector\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Analyzer => b"analyzer\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Synthesizer => b"synthesizer\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Sampler => b"sampler\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Drum => b"drum\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::DrumMachine => b"drum-machine\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Filter => b"filter\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Phaser => b"phaser\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Equalizer => b"equalizer\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::DeEsser => b"de-esser\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::PhaseVocoder => b"phase-vocoder\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Granular => b"granular\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::FrequencyShifter => {
                b"frequency-shifter\0".as_ptr() as *const std::ffi::c_char
            }
            ClapFeature::PitchShifter => b"pitch-shifter\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Distortion => b"distortion\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::TransientShaper => {
                b"transient-shaper\0".as_ptr() as *const std::ffi::c_char
            }
            ClapFeature::Compressor => b"compressor\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Expander => b"expander\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Gate => b"gate\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Limiter => b"limiter\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Flanger => b"flanger\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Chorus => b"chorus\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Delay => b"delay\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Reverb => b"reverb\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Tremolo => b"tremolo\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Glitch => b"glitch\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Utility => b"utility\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::PitchCorrection => {
                b"pitch-correction\0".as_ptr() as *const std::ffi::c_char
            }
            ClapFeature::Restoration => b"restoration\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::MultiEffects => b"multi-effects\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Mixing => b"mixing\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Mastering => b"mastering\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Mono => b"mono\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Stereo => b"stereo\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Surround => b"surround\0".as_ptr() as *const std::ffi::c_char,
            ClapFeature::Ambisonic => b"ambisonic\0".as_ptr() as *const std::ffi::c_char,
        }
    }
}

/// Thread-safe wrapper for CLAP feature lists (null-terminated).
pub struct ClapFeatures(&'static [*const std::ffi::c_char]);
unsafe impl Sync for ClapFeatures {}
unsafe impl Send for ClapFeatures {}

impl ClapFeatures {
    pub const fn new(features: &'static [*const std::ffi::c_char]) -> Self {
        Self(features)
    }

    pub const fn as_ptr(&self) -> *const *const std::ffi::c_char {
        self.0.as_ptr()
    }
}
