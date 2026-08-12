//! Audio Unit Backend Adapter for SunMao.
//!
//! This crate wraps `SunmaoPlugin` to expose it as an Audio Unit plugin via `au_rs`.
//! **macOS only**.

#![cfg(target_os = "macos")]

use au_rs::{BufferList, Plugin};
use sunmao_core::events::MidiMessage;
use sunmao_core::plugin::ProcessContext as SunmaoProcessContext;
use sunmao_core::{AudioBuffer, Event as SunmaoEvent, EventQueue, Params, SunmaoPlugin};

pub use au_rs::{
    export_au_plugin, fourcc, AudioComponentDescription, AudioComponentPlugInInterface,
    AudioUnitCocoaViewInfo, ParameterInfo, ParameterUnit, PluginInfo, PluginWrapper,
};
pub use au_rs::{
    get_parameter_local, gl_get_proc_address, set_parameter_local, NSPoint, NSRect, NSSize,
};
pub use au_sys;

/// AU parameter list provider for auto-registration.
pub trait SunmaoAuParamList {
    fn au_params() -> &'static [ParameterInfo];
}

/// Helper to fetch AU params for a Params type.
pub fn au_params<P: SunmaoAuParamList>() -> &'static [ParameterInfo] {
    P::au_params()
}

/// Wrapper that adapts a SunmaoPlugin to au_rs::Plugin.
pub struct SunmaoAuWrapper<P: SunmaoPlugin> {
    plugin: P,
    params: Arc<P::Params>,
    sample_rate: f64,
    max_frames: u32,
    is_synth: bool,
    // Temporary buffers for plugin processing
    input_buffers: Vec<Vec<f32>>,
    output_buffers: Vec<Vec<f32>>,
    // MIDI event queue
    pending_midi: Vec<MidiMessage>,
}

unsafe impl<P: SunmaoPlugin> Send for SunmaoAuWrapper<P> {}
unsafe impl<P: SunmaoPlugin> Sync for SunmaoAuWrapper<P> {}

impl<P: SunmaoPlugin> Plugin for SunmaoAuWrapper<P> {
    fn init(sample_rate: f64, max_frames: u32) -> Self {
        let mut plugin = P::default();
        plugin.initialize(sample_rate, max_frames);
        let params = plugin.params();
        let is_synth = plugin.input_channels() == 0;
        let in_ch = plugin.input_channels() as usize;
        let out_ch = plugin.output_channels() as usize;

        Self {
            plugin,
            params,
            sample_rate,
            max_frames,
            is_synth,
            input_buffers: vec![vec![0.0; max_frames as usize]; in_ch.max(2)],
            output_buffers: vec![vec![0.0; max_frames as usize]; out_ch.max(2)],
            pending_midi: Vec::with_capacity(128),
        }
    }

    fn reset(&mut self) {
        self.plugin.reset();
        self.pending_midi.clear();
    }

    fn process(
        &mut self,
        inputs: Option<BufferList<'_>>,
        outputs: &mut BufferList<'_>,
        frames: usize,
    ) {
        // Copy AU input to temp buffers
        if let Some(mut inputs) = inputs {
            let num_in_ch = inputs.len();
            for ch in 0..num_in_ch.min(self.input_buffers.len()) {
                let src = unsafe { inputs.channel_mut(ch) };
                let dst = &mut self.input_buffers[ch];
                let len = frames.min(src.len()).min(dst.len());
                dst[..len].copy_from_slice(&src[..len]);
            }
        }

        // Prepare output buffers
        if self.is_synth {
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
        let transport = au_rs::current_transport();
        let ctx = SunmaoProcessContext {
            sample_rate: self.sample_rate,
            tempo: transport.tempo,
            is_playing: transport.is_playing.unwrap_or(true),
            sample_pos: transport.sample_pos.unwrap_or(0),
        };

        // Create event queue with pending MIDI
        let mut events = EventQueue::new();
        for midi in self.pending_midi.drain(..) {
            events.push(SunmaoEvent::Midi(midi));
        }

        // Call the actual plugin process
        let _status = self.plugin.process(&mut audio_buffer, &events, &ctx);

        // Copy output back to AU buffers
        let num_out_ch = outputs.len();
        for ch in 0..num_out_ch.min(self.output_buffers.len()) {
            let src = &self.output_buffers[ch];
            let dst = unsafe { outputs.channel_mut(ch) };
            let len = frames.min(src.len()).min(dst.len());
            dst[..len].copy_from_slice(&src[..len]);
        }
    }

    fn parameters(&self) -> &'static [ParameterInfo] {
        // Parameters are provided via export_au_plugin! macro
        &[]
    }

    fn get_parameter(&self, id: u32) -> f32 {
        let ids = P::Params::ids();
        if let Some(&param_id) = ids.get(id as usize) {
            self.params.get_normalized(param_id).unwrap_or(0.0)
        } else {
            0.0
        }
    }

    fn set_parameter(&mut self, id: u32, value: f32) {
        let ids = P::Params::ids();
        if let Some(&param_id) = ids.get(id as usize) {
            self.params.set_normalized(param_id, value);
        }
    }

    fn handle_midi_event(&mut self, status: u8, data1: u8, data2: u8, offset: u32) {
        // Queue raw MIDI events
        let midi = MidiMessage {
            offset,
            data: [status, data1, data2],
        };
        self.pending_midi.push(midi);
    }

    fn start_note(&mut self, pitch: f32, velocity: f32, offset: u32) -> u32 {
        // Convert AU note to MIDI note on
        let note = pitch as u8;
        let vel = (velocity * 127.0) as u8;
        let midi = MidiMessage::note_on(offset, 0, note, vel);
        self.pending_midi.push(midi);
        note as u32
    }

    fn stop_note(&mut self, note_id: u32, offset: u32) {
        // Convert AU note to MIDI note off
        let note = note_id as u8;
        let midi = MidiMessage::note_off(offset, 0, note, 0);
        self.pending_midi.push(midi);
    }
}

/// AU GUI configuration for Sunmao plugins.
///
/// Defaults to a normalized 0.0-1.0 range for the first parameter.
/// Plugins can override this by implementing `SunmaoAuGuiParams`.
pub trait SunmaoAuGuiParams: SunmaoPlugin {
    /// The value range for the primary parameter exposed by the AU GUI.
    fn au_gui_range() -> (f32, f32) {
        (0.0, 1.0)
    }
}

/// AU GUI re-exports for building custom AU GUIs in examples.
pub mod gui {
    pub use au_rs::gui::layer::{get_view_layer, Layer, TransactionGuard};
    pub use au_rs::gui::webview::{self, MessageCallback};
    pub use au_rs::gui::{
        flush_context, make_current_context, open_gl_context, set_best_resolution,
        set_needs_display, set_pixel_format, update_open_gl_view, view_backing_bounds, view_bounds,
        CocoaObject, GuiConfig, GuiHandler,
    };
}

// ============ AU View Adapter (SunmaoView → GuiHandler bridge) ============

use std::sync::{Arc, OnceLock};
use sunmao_core::view::ParamsViewContext;
use sunmao_core::{SunmaoView, ViewContext};

/// Storage for the SunmaoView, ViewContext, and preferred size before the AU GUI is created.
///
/// These are populated by `setup_au_gui` before the AU host queries the GUI.
/// The adapter's `init()` reads from these to create the baseview window.
static AU_VIEW_STORAGE: OnceLock<(Box<dyn SunmaoView>, Arc<dyn ViewContext>, (u32, u32))> =
    OnceLock::new();

/// Adapter that bridges AU's `GuiHandler` trait to `SunmaoView` via baseview.
///
/// This allows plugins implementing `SunmaoPlugin::view()` to get AU GUI support
/// without writing a separate `GuiHandler` implementation.
///
/// Baseview creates its own child NSView + GL context inside the AU-provided NSView,
/// and handles rendering (via CFRunLoopTimer at ~66fps) and input events (via AppKit
/// event chain) automatically.
pub struct AuViewAdapter {
    window: Option<sunmao_core::ViewHandle>,
    width: f32,
    height: f32,
}

impl Default for AuViewAdapter {
    fn default() -> Self {
        Self {
            window: None,
            width: 400.0,
            height: 300.0,
        }
    }
}

impl au_rs::gui::GuiHandler for AuViewAdapter {
    fn init(
        &mut self,
        view: *mut au_rs::gui::CocoaObject,
        size: au_rs::NSSize,
        _audio_unit: *mut std::ffi::c_void,
    ) {
        sunmao_log(&format!(
            "AuViewAdapter::init called, view={:?}, size={}x{}",
            view, size.width, size.height
        ));
        self.width = size.width.max(1.0) as f32;
        self.height = size.height.max(1.0) as f32;

        let Some((sunmao_view, context, _size)) = AU_VIEW_STORAGE.get() else {
            sunmao_log("AuViewAdapter::init: AU_VIEW_STORAGE not initialized!");
            return;
        };
        sunmao_log("AuViewAdapter::init: storage found, opening baseview");

        // Let the host control the AU view size — don't call setFrameSize.
        // Baseview creates a child NSView inside the AU view that fills it.

        let parent = sunmao_core::ParentWindow::AppKit(view as *mut std::ffi::c_void);
        match sunmao_view.open(parent, context.clone()) {
            Some(handle) => {
                sunmao_log(&format!(
                    "AuViewAdapter::init: baseview opened, handle ptr={:?}",
                    &handle as *const _
                ));
                self.window = Some(handle);
            }
            None => {
                sunmao_log("AuViewAdapter::init: baseview.open() returned None!");
            }
        }
    }

    fn reshape(&mut self, _view: *mut au_rs::gui::CocoaObject, _audio_unit: *mut std::ffi::c_void) {
        // Forward resize to baseview so it updates its child NSView and GL context.
        // Note: baseview's WindowHandle doesn't expose a resize method directly,
        // but the viewDidChangeBackingProperties handler in baseview picks up size changes.
        // We store the latest size for potential future use.
        let bounds = au_rs::gui::view_bounds(_view);
        self.width = bounds.size.width.max(1.0) as f32;
        self.height = bounds.size.height.max(1.0) as f32;
    }

    // draw(), mouse_down(), mouse_dragged(), mouse_up(), key_down() are all no-ops.
    // Baseview handles rendering via its own CFRunLoopTimer (~66fps) and receives
    // mouse/keyboard events through the AppKit event chain (its NSView is a subview
    // of the AU-provided NSView).
}

/// Get the preferred size stored by `setup_au_gui`.
/// Returns `None` if `setup_au_gui` hasn't been called.
pub fn au_gui_preferred_size() -> Option<au_rs::NSSize> {
    AU_VIEW_STORAGE.get().map(|(_, _, (w, h))| au_rs::NSSize {
        width: *w as f64,
        height: *h as f64,
    })
}

/// Initialize AU view storage. Returns preferred size from `SunmaoView::size()`.
///
/// This stores the `SunmaoView` and `ViewContext` in a static so that
/// `AuViewAdapter::init()` can retrieve them when the AU host creates the view.
/// Must be called before the AU host queries `kAudioUnitProperty_CocoaUI`.
pub fn sunmao_log(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/sunmao_au_debug.log")
    {
        let _ = writeln!(f, "[{}] {}", std::process::id(), msg);
    }
}

pub fn setup_au_gui<P: SunmaoPlugin>() -> Option<(u32, u32)> {
    sunmao_log("setup_au_gui: called");
    let plugin = P::default();
    let view = plugin.view();
    sunmao_log(&format!(
        "setup_au_gui: plugin.view() returned Some={}",
        view.is_some()
    ));
    let view = view?;
    let size = view.size();
    sunmao_log(&format!("setup_au_gui: view size={}x{}", size.0, size.1));
    let params = plugin.params();
    let context: Arc<dyn ViewContext> = Arc::new(ParamsViewContext::new(params));

    let result = AU_VIEW_STORAGE.set((view, context, size));
    sunmao_log(&format!(
        "setup_au_gui: storage set, was_first={}",
        result.is_ok()
    ));

    // Register the AuViewAdapter GUI with the AU runtime.
    // This must happen here so that cocoa_view_info() returns our factory class.
    let gui_config = au_rs::gui::GuiConfig {
        factory_class: "SunmaoAUCocoaViewFactoryAuto",
        view_class: "SunmaoAUCocoaViewAuto",
        view_superclass: "NSView",
        description: "SunMao AU Auto GUI",
        preferred_size: Some(au_rs::NSSize {
            width: size.0 as f64,
            height: size.1 as f64,
        }),
    };
    au_rs::gui::register_gui::<AuViewAdapter>(gui_config);
    sunmao_log("setup_au_gui: register_gui called");

    Some(size)
}

/// Export helper macro for AU with default WGPU GUI.
#[macro_export]
macro_rules! sunmao_export_au {
    ($factory_fn:ident, $plugin:ty, $info:expr, $parameters:expr $(,)?) => {
        $crate::export_au_plugin!(
            $factory_fn,
            $crate::SunmaoAuWrapper<$plugin>,
            $info,
            $parameters,
            gui: None
        );
    };
    ($factory_fn:ident, $plugin:ty, $info:expr, $parameters:expr, gui: None $(,)?) => {
        $crate::export_au_plugin!(
            $factory_fn,
            $crate::SunmaoAuWrapper<$plugin>,
            $info,
            $parameters,
            gui: None
        );
    };
    ($factory_fn:ident, $plugin:ty, $info:expr, $parameters:expr, gui: { handler: $handler:ty, config: $config:expr } $(,)?) => {
        $crate::export_au_plugin!(
            $factory_fn,
            $crate::SunmaoAuWrapper<$plugin>,
            $info,
            $parameters,
            gui: { handler: $handler, config: $config }
        );
    };
}

/// Export AU plugin with GUI automatically derived from `SunmaoPlugin::view()`.
///
/// This is the unified AU export macro — no separate `GuiHandler` implementation needed.
/// The plugin's `view()` method is called to get a `SunmaoView`, which is then bridged
/// to AU via `AuViewAdapter` and baseview.
///
/// The factory function is always named `RustAUFactory` to match the standard
/// AU bundle convention used by the packager tools.
///
/// Usage:
/// ```ignore
/// sunmao_export_au_with_view!(MyPlugin, AU_INFO, au_params::<MyParams>());
/// ```
#[macro_export]
macro_rules! sunmao_export_au_with_view {
    ($plugin:ty, $info:expr, $parameters:expr $(,)?) => {
        // GUI setup function: stores SunmaoView + ViewContext, registers with AU.
        // Called lazily when host queries kAudioUnitProperty_CocoaUI.
        fn __sunmao_au_cocoa_view_info() -> $crate::AudioUnitCocoaViewInfo {
            $crate::sunmao_log("__sunmao_au_cocoa_view_info: called");
            $crate::setup_au_gui::<$plugin>();
            $crate::sunmao_log("__sunmao_au_cocoa_view_info: calling cocoa_view_info_for_plugin");

            // Pass the RustAUFactory function pointer so cocoa_view_info can use dladdr
            // to find the correct .component bundle URL. Without this, NSBundle bundleForClass:
            // returns the main bundle for dynamically registered ObjC classes, causing AU hosts
            // like Logic Pro to fail to load the CocoaUI factory class.
            let factory_name = std::ffi::CString::new("RustAUFactory").unwrap();
            let factory_ptr = unsafe { $crate::au_sys::libc::dlsym($crate::au_sys::libc::RTLD_DEFAULT, factory_name.as_ptr()) };
            if factory_ptr.is_null() {
                $crate::sunmao_log("__sunmao_au_cocoa_view_info: RustAUFactory not found via dlsym, using fallback");
                $crate::au_sys::cocoa_view_info()
            } else {
                $crate::au_sys::cocoa_view_info_for_plugin(factory_ptr as *const std::ffi::c_void)
            }
        }

        // Export the AU component with GUI support.
        // Factory function is always RustAUFactory (matches Info.plist).
        $crate::au_sys::export_au_component!(
            RustAUFactory,
            $crate::PluginWrapper<$crate::SunmaoAuWrapper<$plugin>>,
            $info.descriptor($parameters, Some(__sunmao_au_cocoa_view_info))
        );
    };
}
