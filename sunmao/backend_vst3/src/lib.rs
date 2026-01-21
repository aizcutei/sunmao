//! VST3 Backend Adapter for SunMao.
//!
//! This crate wraps `SunmaoPlugin` to expose it as a VST3 plugin via `vst3_rs`.

use raw_window_handle::RawWindowHandle;
use std::any::TypeId;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use sunmao_core::events::MidiMessage;
use sunmao_core::plugin::{ProcessContext as SunmaoProcessContext, ProcessStatus};
use sunmao_core::view::ViewContext;
use sunmao_core::{
    AudioBuffer, Event, EventQueue, Params, ParentWindow, SunmaoPlugin, SunmaoView, ViewHandle,
};
use vst3_rs::gui::prepare_view;
use vst3_rs::gui::GuiSize;
use vst3_rs::{AudioConfig, GuiPlugin, HostHandle, ParamInfo, Plugin, PluginInfo, ProcessContext};

pub use vst3_rs::{export_vst3_plugin, export_vst3_plugin_with_gui};

/// Wrapper for ViewHandle that is Send+Sync (unsafe).
/// GUI handles are only accessed on the main thread in VST3.
struct ThreadSafeViewHandle(ViewHandle);
unsafe impl Send for ThreadSafeViewHandle {}
unsafe impl Sync for ThreadSafeViewHandle {}

/// Wrapper that adapts a SunmaoPlugin to vst3_rs::Plugin.
pub struct SunmaoVst3Wrapper<P: SunmaoPlugin> {
    plugin: P,
    params: Arc<P::Params>,
    sample_rate: f64,
    // Temporary buffers for deinterleaving
    input_buffers: Vec<Vec<f32>>,
    output_buffers: Vec<Vec<f32>>,
    // MIDI event queue for synths
    pending_midi: Vec<MidiMessage>,
    // GUI View Handle
    view_handle: Option<ThreadSafeViewHandle>,
    // Shared parameter store (shared with GUI/controller)
    shared_params: Arc<SharedParamStore>,
}

struct SharedParamStore {
    values: Vec<AtomicU32>,
}

impl SharedParamStore {
    fn new(values: Vec<f32>) -> Self {
        Self {
            values: values
                .into_iter()
                .map(|v| AtomicU32::new(v.to_bits()))
                .collect(),
        }
    }

    fn get(&self, index: usize) -> f32 {
        self.values
            .get(index)
            .map(|v| f32::from_bits(v.load(Ordering::Relaxed)))
            .unwrap_or(0.0)
    }

    fn set(&self, index: usize, value: f32) {
        if let Some(slot) = self.values.get(index) {
            slot.store(value.to_bits(), Ordering::Relaxed);
        }
    }
}

static SHARED_PARAMS: OnceLock<Mutex<HashMap<TypeId, Arc<SharedParamStore>>>> = OnceLock::new();

fn get_shared_params<P: SunmaoPlugin>(params: &Arc<P::Params>) -> Arc<SharedParamStore> {
    let map = SHARED_PARAMS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = map.lock().expect("shared params lock poisoned");
    map.entry(TypeId::of::<P>())
        .or_insert_with(|| {
            let values = P::Params::ids()
                .iter()
                .map(|id| params.get_normalized(id).unwrap_or(0.0))
                .collect();
            Arc::new(SharedParamStore::new(values))
        })
        .clone()
}

struct Vst3ParamsViewContext<P: Params> {
    params: Arc<P>,
    shared: Arc<SharedParamStore>,
    id_to_index: HashMap<&'static str, usize>,
}

impl<P: Params> Vst3ParamsViewContext<P> {
    fn new(params: Arc<P>, shared: Arc<SharedParamStore>) -> Self {
        let id_to_index = P::ids()
            .iter()
            .enumerate()
            .map(|(idx, &id)| (id, idx))
            .collect();
        Self {
            params,
            shared,
            id_to_index,
        }
    }
}

impl<P: Params> ViewContext for Vst3ParamsViewContext<P> {
    fn get_param(&self, id: &str) -> Option<f32> {
        self.params.get_normalized(id)
    }

    fn set_param(&self, id: &str, value: f32) {
        self.params.set_normalized(id, value);
        if let Some(&index) = self.id_to_index.get(id) {
            self.shared.set(index, value);
        }
    }

    fn begin_edit(&self, _id: &str) {}

    fn end_edit(&self, _id: &str) {}

    fn request_resize(&self, _width: u32, _height: u32) -> bool {
        false
    }
}

unsafe impl<P: SunmaoPlugin> Send for SunmaoVst3Wrapper<P> {}
unsafe impl<P: SunmaoPlugin> Sync for SunmaoVst3Wrapper<P> {}

impl<P: SunmaoPlugin> Plugin for SunmaoVst3Wrapper<P> {
    fn info() -> PluginInfo {
        let vst3_info = P::vst3_info();

        PluginInfo {
            id: P::NAME,
            name: P::NAME,
            vendor: P::VENDOR,
            url: P::URL,
            email: "",
            version: P::VERSION,
            category: if P::default().input_channels() == 0 {
                "Instrument|Synth"
            } else {
                "Fx"
            },
        }
    }

    fn new(_host: HostHandle) -> Self {
        let plugin = P::default();
        let params = plugin.params();
        let shared_params = get_shared_params::<P>(&params);
        Self {
            plugin,
            params,
            sample_rate: 44100.0,
            input_buffers: vec![vec![0.0; 4096]; 2],
            output_buffers: vec![vec![0.0; 4096]; 2],
            pending_midi: Vec::with_capacity(128),
            view_handle: None,
            shared_params,
        }
    }

    fn activate(&mut self, sample_rate: f64, max_frames: u32) -> bool {
        self.sample_rate = sample_rate;
        let max = max_frames as usize;
        for buf in &mut self.input_buffers {
            buf.resize(max, 0.0);
        }
        for buf in &mut self.output_buffers {
            buf.resize(max, 0.0);
        }
        self.plugin.initialize(sample_rate, max_frames);
        true
    }

    fn deactivate(&mut self) {
        self.plugin.reset();
        self.pending_midi.clear();
    }

    fn audio_config() -> AudioConfig {
        let plugin = P::default();
        if plugin.input_channels() == 0 {
            AudioConfig::stereo_synth()
        } else {
            AudioConfig::stereo_effect()
        }
    }

    fn params() -> Vec<ParamInfo> {
        let ids = P::Params::ids();
        ids.iter()
            .enumerate()
            .map(|(idx, &id)| ParamInfo::new(idx as u32, id).range(0.0, 1.0).default(0.5))
            .collect()
    }

    fn get_param(&self, id: u32) -> f64 {
        let ids = P::Params::ids();
        let index = id as usize;
        if index < ids.len() {
            self.shared_params.get(index) as f64
        } else {
            0.0
        }
    }

    fn set_param(&mut self, id: u32, value: f64) {
        let ids = P::Params::ids();
        let index = id as usize;
        if let Some(&param_id) = ids.get(index) {
            let v = value as f32;
            self.params.set_normalized(param_id, v);
            self.shared_params.set(index, v);
        }
    }

    fn note_on(&mut self, channel: i16, pitch: i16, velocity: f32) {
        // Queue MIDI note on event for processing
        let midi = MidiMessage::note_on(0, channel as u8, pitch as u8, (velocity * 127.0) as u8);
        self.pending_midi.push(midi);
    }

    fn note_off(&mut self, channel: i16, pitch: i16, velocity: f32) {
        // Queue MIDI note off event for processing
        let midi = MidiMessage::note_off(0, channel as u8, pitch as u8, (velocity * 127.0) as u8);
        self.pending_midi.push(midi);
    }

    fn process(&mut self, ctx: &mut ProcessContext) {
        self.sync_params_from_shared();
        let num_samples = ctx.num_samples;
        let num_in = ctx.num_inputs();
        let num_out = ctx.num_outputs();

        // Copy input data to our buffers
        for ch in 0..num_in.min(self.input_buffers.len()) {
            let src = ctx.input(ch);
            let dst = &mut self.input_buffers[ch];
            let len = num_samples.min(src.len()).min(dst.len());
            dst[..len].copy_from_slice(&src[..len]);
        }

        // For effects: pre-copy input to output. For synths: clear output.
        // Use the actual plugin instance, not P::default() which creates a new one!
        let is_synth = self.plugin.input_channels() == 0;
        if is_synth {
            // Synth: start with silence
            for buf in &mut self.output_buffers {
                for sample in buf.iter_mut().take(num_samples) {
                    *sample = 0.0;
                }
            }
        } else {
            // Effect: pre-copy input to output
            for ch in 0..num_in.min(self.output_buffers.len()) {
                let len = num_samples
                    .min(self.input_buffers[ch].len())
                    .min(self.output_buffers[ch].len());
                self.output_buffers[ch][..len].copy_from_slice(&self.input_buffers[ch][..len]);
            }
        }

        // Create AudioBuffer for sunmao plugin
        let inputs: Vec<&[f32]> = self
            .input_buffers
            .iter()
            .map(|b| &b[..num_samples])
            .collect();
        let mut outputs: Vec<&mut [f32]> = self
            .output_buffers
            .iter_mut()
            .map(|b| &mut b[..num_samples])
            .collect();

        let mut audio_buffer = AudioBuffer::new(&inputs, &mut outputs, num_samples);

        // Create process context
        let sunmao_ctx = SunmaoProcessContext {
            sample_rate: self.sample_rate,
            tempo: ctx.tempo(),
            is_playing: ctx.is_playing(),
            sample_pos: ctx.sample_pos(),
        };

        // Create event queue with pending MIDI events
        let mut events = EventQueue::new();
        for midi in self.pending_midi.drain(..) {
            events.push(Event::Midi(midi));
        }

        // Call the actual plugin process
        let _status = self.plugin.process(&mut audio_buffer, &events, &sunmao_ctx);

        // Copy output data back to vst3 context
        for ch in 0..num_out.min(self.output_buffers.len()) {
            let src = &self.output_buffers[ch];
            let dst = ctx.output_mut(ch);
            let len = num_samples.min(src.len()).min(dst.len());
            dst[..len].copy_from_slice(&src[..len]);
        }
    }
}

impl<P: SunmaoPlugin> SunmaoVst3Wrapper<P> {
    fn sync_params_from_shared(&self) {
        let ids = P::Params::ids();
        for (idx, &id) in ids.iter().enumerate() {
            let v = self.shared_params.get(idx);
            self.params.set_normalized(id, v);
        }
    }
}

impl<P: SunmaoPlugin> GuiPlugin for SunmaoVst3Wrapper<P> {
    fn gui_size() -> GuiSize {
        let plugin = P::default();
        if let Some(view) = plugin.view() {
            let (w, h) = view.size();
            GuiSize::new(w, h)
        } else {
            GuiSize::new(0, 0)
        }
    }

    fn gui_create(&mut self, parent: RawWindowHandle) -> bool {
        let mut parent = parent;
        if prepare_view(&mut parent).is_err() {
            return false;
        }
        if let Some(view) = self.plugin.view() {
            let parent_window = match parent {
                RawWindowHandle::AppKit(h) => {
                    let ptr = h.ns_view.as_ptr();
                    ParentWindow::AppKit(ptr as *mut c_void)
                }
                RawWindowHandle::Win32(h) => {
                    let ptr = h.hwnd.get();
                    ParentWindow::Win32(ptr as *mut c_void)
                }
                RawWindowHandle::Xcb(h) => ParentWindow::X11(h.window.get()),
                _ => return false,
            };

            let context = Arc::new(Vst3ParamsViewContext::new(
                self.params.clone(),
                self.shared_params.clone(),
            ));
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

    fn gui_destroy(&mut self) {
        self.view_handle = None;
    }
}
