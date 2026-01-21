//! AU RS Fx Gain with Metal-backed GUI
//! 
//! A gain effect with a slider rendered using safe CALayer API on MTKView.
//! This example demonstrates how to create GUI plugins without any unsafe code.

use au_rs::{
    BufferList, ParameterInfo, ParameterUnit, Plugin, PluginInfo, export_au_plugin,
    for_each_channel, fourcc,
};

const PARAM_GAIN: u32 = 0;

const PARAMETERS: [ParameterInfo; 1] = [ParameterInfo {
    id: PARAM_GAIN,
    name: "Gain",
    min: 0.0,
    max: 2.0,
    default: 1.0,
    unit: ParameterUnit::LinearGain,
}];

pub struct GainEffectWgpu {
    gain: f32,
}

impl Plugin for GainEffectWgpu {
    fn init(_sample_rate: f64, _max_frames: u32) -> Self {
        Self { gain: 1.0 }
    }

    fn process(
        &mut self,
        inputs: Option<BufferList<'_>>,
        outputs: &mut BufferList<'_>,
        frames: usize,
    ) {
        if inputs.is_none() {
            for ch in 0..outputs.len() {
                let out = unsafe { outputs.channel_mut(ch) };
                let out_len = frames.min(out.len());
                for sample in out[..out_len].iter_mut() {
                    *sample *= self.gain;
                }
            }
            return;
        }
        for_each_channel(inputs, outputs, frames, |input, output| {
            for (idx, out_sample) in output.iter_mut().enumerate() {
                let sample = input.and_then(|buf| buf.get(idx)).copied().unwrap_or(0.0);
                *out_sample = sample * self.gain;
            }
        });
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
            self.gain = value.clamp(0.0, 2.0);
        }
    }
}

#[cfg(target_os = "macos")]
mod gui {
    use std::ffi::c_void;

    use au_rs::{
        NSPoint, NSRect, NSSize,
        gui::{GuiConfig, GuiHandler, set_needs_display, view_bounds},
        gui::layer::{Layer, TransactionGuard, get_view_layer},
        get_parameter_local, set_parameter_local,
    };
    use objc::runtime::Object;

    use crate::PARAM_GAIN;

    // Slider layout constants
    const SLIDER_MARGIN: f64 = 20.0;
    const SLIDER_HEIGHT: f64 = 20.0;
    const KNOB_WIDTH: f64 = 12.0;

    // Colors
    const BG_COLOR: (f64, f64, f64, f64) = (0.12, 0.12, 0.16, 1.0);
    const TRACK_COLOR: (f64, f64, f64, f64) = (0.25, 0.25, 0.3, 1.0);
    const KNOB_COLOR: (f64, f64, f64, f64) = (0.1, 0.6, 0.9, 1.0);

    pub struct WgpuGui {
        track_layer: Layer,
        knob_layer: Layer,
        gain: f32,
        dragging: bool,
        width: f64,
        height: f64,
    }

    impl Default for WgpuGui {
        fn default() -> Self {
            Self {
                track_layer: Layer::from_ptr(std::ptr::null_mut()),
                knob_layer: Layer::from_ptr(std::ptr::null_mut()),
                gain: 1.0,
                dragging: false,
                width: 400.0,
                height: 100.0,
            }
        }
    }

    impl WgpuGui {
        fn set_gain(&mut self, view: *mut Object, au: *mut c_void, gain: f32) {
            self.gain = gain.clamp(0.0, 2.0);
            if !au.is_null() {
                let _ = set_parameter_local(au, PARAM_GAIN, self.gain);
            }
            self.update_knob_position();
            set_needs_display(view);
        }

        fn gain_from_x(&self, x: f64) -> f32 {
            let slider_width = (self.width - 2.0 * SLIDER_MARGIN - KNOB_WIDTH).max(1.0);
            let local_x = (x - SLIDER_MARGIN).clamp(0.0, slider_width);
            ((local_x / slider_width * 2.0) as f32).clamp(0.0, 2.0)
        }

        fn update_knob_position(&self) {
            if self.knob_layer.is_null() {
                return;
            }
            
            let slider_width = self.width - 2.0 * SLIDER_MARGIN - KNOB_WIDTH;
            let normalized = (self.gain / 2.0).clamp(0.0, 1.0) as f64;
            let knob_x = SLIDER_MARGIN + normalized * slider_width;
            let knob_height = SLIDER_HEIGHT * 2.0;
            let knob_y = (self.height - knob_height) / 2.0;
            
            // Use TransactionGuard to disable animations (RAII pattern)
            let _guard = TransactionGuard::begin_no_animation();
            self.knob_layer.set_frame(knob_x, knob_y, KNOB_WIDTH, knob_height);
        }
    }

    impl GuiHandler for WgpuGui {
        fn init(&mut self, view: *mut Object, size: NSSize, audio_unit: *mut c_void) {
            self.width = size.width;
            self.height = size.height;

            // Get initial gain from audio unit
            if !audio_unit.is_null() {
                if let Ok(value) = get_parameter_local(audio_unit, PARAM_GAIN) {
                    self.gain = value.clamp(0.0, 2.0);
                }
            }

            // Get the root layer from the view
            let root_layer = get_view_layer(view);
            if root_layer.is_null() {
                return;
            }
            
            // Set background color
            root_layer.set_background_color(BG_COLOR.0, BG_COLOR.1, BG_COLOR.2, BG_COLOR.3);
            
            // Create and configure track layer
            let track_layer = Layer::new();
            let track_width = self.width - 2.0 * SLIDER_MARGIN;
            let track_y = (self.height - SLIDER_HEIGHT) / 2.0;
            track_layer.set_frame(SLIDER_MARGIN, track_y, track_width, SLIDER_HEIGHT);
            track_layer.set_background_color(TRACK_COLOR.0, TRACK_COLOR.1, TRACK_COLOR.2, TRACK_COLOR.3);
            root_layer.add_sublayer(&track_layer);
            self.track_layer = track_layer;
            
            // Create and configure knob layer
            let knob_layer = Layer::new();
            let knob_height = SLIDER_HEIGHT * 2.0;
            let slider_width = track_width - KNOB_WIDTH;
            let normalized = (self.gain / 2.0).clamp(0.0, 1.0) as f64;
            let knob_x = SLIDER_MARGIN + normalized * slider_width;
            let knob_y = (self.height - knob_height) / 2.0;
            knob_layer.set_frame(knob_x, knob_y, KNOB_WIDTH, knob_height);
            knob_layer.set_background_color(KNOB_COLOR.0, KNOB_COLOR.1, KNOB_COLOR.2, KNOB_COLOR.3);
            knob_layer.set_corner_radius(4.0);
            root_layer.add_sublayer(&knob_layer);
            self.knob_layer = knob_layer;

            set_needs_display(view);
        }

        fn draw(&mut self, _view: *mut Object, _audio_unit: *mut c_void, _rect: NSRect) {
            // CALayer handles rendering, no manual drawing needed
        }

        fn reshape(&mut self, view: *mut Object, _audio_unit: *mut c_void) {
            let bounds = view_bounds(view);
            self.width = bounds.size.width.max(1.0);
            self.height = bounds.size.height.max(1.0);
            
            // Update track layer size
            if !self.track_layer.is_null() {
                let track_width = self.width - 2.0 * SLIDER_MARGIN;
                let track_y = (self.height - SLIDER_HEIGHT) / 2.0;
                
                let _guard = TransactionGuard::begin_no_animation();
                self.track_layer.set_frame(SLIDER_MARGIN, track_y, track_width, SLIDER_HEIGHT);
            }
            
            self.update_knob_position();
            set_needs_display(view);
        }

        fn mouse_down(
            &mut self,
            view: *mut Object,
            audio_unit: *mut c_void,
            point: NSPoint,
            _flags: u64,
        ) {
            self.dragging = true;
            let gain = self.gain_from_x(point.x);
            self.set_gain(view, audio_unit, gain);
        }

        fn mouse_dragged(
            &mut self,
            view: *mut Object,
            audio_unit: *mut c_void,
            point: NSPoint,
            _flags: u64,
        ) {
            if self.dragging {
                let gain = self.gain_from_x(point.x);
                self.set_gain(view, audio_unit, gain);
            }
        }

        fn mouse_up(
            &mut self,
            view: *mut Object,
            _audio_unit: *mut c_void,
            _point: NSPoint,
            _flags: u64,
        ) {
            self.dragging = false;
            set_needs_display(view);
        }

        fn key_down(
            &mut self,
            view: *mut Object,
            audio_unit: *mut c_void,
            key_code: u16,
            _flags: u64,
        ) {
            // ESC key resets to 1.0
            if key_code == 53 {
                self.set_gain(view, audio_unit, 1.0);
            }
        }
    }

    pub const CONFIG: GuiConfig = GuiConfig {
        factory_class: "RustAUCocoaViewFactoryWgpu",
        view_class: "RustAUCocoaViewWgpu",
        view_superclass: "MTKView",
        description: "Rust AU Metal",
    };
}

#[cfg(target_os = "macos")]
export_au_plugin!(
    RustAUFactory,
    GainEffectWgpu,
    PluginInfo {
        name: "Au Rs Fx Gain Gui Wgpu",
        component_type: fourcc(b"aufx"),
        component_subtype: fourcc(b"rgwg"),
        manufacturer: fourcc(b"RUST"),
        version: 0x0001_0000,
        flags: 0,
        flags_mask: 0,
        input_channels: 2,
        output_channels: 2,
        supports_midi: false,
    },
    &PARAMETERS,
    gui: { handler: gui::WgpuGui, config: gui::CONFIG }
);

#[cfg(not(target_os = "macos"))]
export_au_plugin!(
    RustAUFactory,
    GainEffectWgpu,
    PluginInfo {
        name: "Rust Gain (WGPU)",
        component_type: fourcc(b"aufx"),
        component_subtype: fourcc(b"rgwg"),
        manufacturer: fourcc(b"RUST"),
        version: 0x0001_0000,
        flags: 0,
        flags_mask: 0,
        input_channels: 2,
        output_channels: 2,
        supports_midi: false,
    },
    &PARAMETERS,
    gui: None
);
