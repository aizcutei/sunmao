//! SunMao Gain Plugin with WGPU GUI
//!
//! This example demonstrates a gain effect plugin with a custom GUI
//! using the WGPU renderer backend. The same codebase exports to
//! AU, VST3, and CLAP — no format-specific GUI code needed.

use sunmao::prelude::*;

// ============ Plugin Definition ============

#[derive(Params)]
#[cfg_attr(all(target_os = "macos", feature = "au"), sunmao_au)]
pub struct GainParams {
    #[unit = "LinearGain"]
    pub gain: FloatParam,
}

impl Default for GainParams {
    fn default() -> Self {
        Self {
            gain: FloatParam::new("gain", "Gain", 1.0, 0.0, 2.0),
        }
    }
}

pub struct GainPlugin {
    params: Arc<GainParams>,
}

impl Default for GainPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(GainParams::default()),
        }
    }
}

impl SunmaoPlugin for GainPlugin {
    const NAME: &'static str = "SunMao Gain WGPU";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";

    type Params = GainParams;

    fn params(&self) -> Arc<Self::Params> {
        self.params.clone()
    }

    fn input_channels(&self) -> u32 {
        2
    }
    fn output_channels(&self) -> u32 {
        2
    }
    fn accepts_midi(&self) -> bool {
        false
    }

    fn initialize(&mut self, _sample_rate: f64, _max_frames: u32) {}
    fn reset(&mut self) {}

    fn clap_info() -> ClapInfo {
        ClapInfo {
            id: "com.sunmao.fx.gain.wgpu",
            features: &["audio-effect", "utility", "stereo"],
        }
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        buffer.copy_input_to_output();
        let mut gain = self.params.gain.get();
        let mut changes = events
            .param_changes()
            .filter(|change| change.id == self.params.gain.id)
            .peekable();

        for sample_index in 0..buffer.num_samples() {
            while changes
                .peek()
                .is_some_and(|change| change.offset as usize <= sample_index)
            {
                let change = changes.next().expect("peeked parameter change");
                gain = self.params.gain.min
                    + change.value.clamp(0.0, 1.0) * (self.params.gain.max - self.params.gain.min);
            }
            for channel in 0..buffer.num_output_channels() {
                buffer.output(channel)[sample_index] *= gain;
            }
        }
        ProcessStatus::Normal
    }

    fn view(&self) -> Option<Box<dyn SunmaoView>> {
        let config = BaseviewConfig {
            title: "SunMao Gain".to_string(),
            width: 400,
            height: 120,
            scale_policy: WindowScalePolicy::SystemScaleFactor,
            background: Color::rgb(0.12, 0.12, 0.18),
        };

        let view =
            BaseviewWgpuView::new(config, |context| GainViewState::new(context, 400.0, 120.0));
        Some(Box::new(view))
    }
}

// ============ GUI State ============

struct GainViewState {
    slider: Slider,
    context: Arc<dyn ViewContext>,
    editing: bool,
}

impl GainViewState {
    fn new(context: Arc<dyn ViewContext>, width: f32, height: f32) -> Self {
        let mut slider = Slider::new("gain").with_default(0.5);
        slider.set_bounds(Rect::new(20.0, (height - 24.0) * 0.5, width - 40.0, 24.0));
        Self {
            slider,
            context,
            editing: false,
        }
    }

    fn relayout(&mut self, width: f32, height: f32) {
        self.slider
            .set_bounds(Rect::new(20.0, (height - 24.0) * 0.5, width - 40.0, 24.0));
    }

    fn sync_from_params(&mut self) {
        if let Some(value) = self.context.get_param("gain") {
            self.slider.set_value(value);
        }
    }
}

impl WgpuViewState for GainViewState {
    fn draw(&mut self, ctx: &mut WgpuContext, width: f32, height: f32) {
        self.sync_from_params();
        ctx.fill_rect(
            0.0,
            0.0,
            width,
            height,
            Fill::Solid(Color::rgb(0.14, 0.14, 0.2)),
        );
        self.slider.draw(ctx);
    }

    fn on_mouse_event(&mut self, event: &GuiEvent) -> bool {
        let before = self.slider.value();
        let handled = self.slider.handle_event(event);
        let after = self.slider.value();

        match event {
            GuiEvent::MouseDown {
                button: GuiMouseButton::Left,
                ..
            } if handled => {
                self.editing = true;
                self.context.begin_edit("gain");
            }
            GuiEvent::MouseUp {
                button: GuiMouseButton::Left,
                ..
            } if self.editing => {
                self.editing = false;
                self.context.end_edit("gain");
            }
            _ => {}
        }

        if handled && (after - before).abs() > f32::EPSILON {
            self.context.set_param("gain", after);
        }

        handled
    }

    fn on_resize(&mut self, width: f32, height: f32) {
        self.relayout(width, height);
    }
}

// ============ Unified VST3 + CLAP Export ============
sunmao::sunmao_export!(GainPlugin, gui);

// ============ AU Export (macOS only) ============
#[cfg(all(target_os = "macos", feature = "au"))]
mod au_export {
    use super::*;
    use sunmao_backend_au::{au_params, fourcc, PluginInfo};

    const AU_INFO: PluginInfo = PluginInfo {
        name: "SunMao Gain WGPU",
        component_type: fourcc(b"aufx"),
        component_subtype: fourcc(b"smgw"),
        manufacturer: fourcc(b"SunM"),
        version: 0x00010000,
        flags: 0,
        flags_mask: 0,
        input_channels: 2,
        output_channels: 2,
        supports_midi: false,
    };

    // One macro call — no format-specific GUI code needed!
    // The plugin's `view()` returns a BaseviewWgpuView, and the AU backend
    // bridges it via AuViewAdapter + baseview's native window handles.
    sunmao_backend_au::sunmao_export_au_with_view!(GainPlugin, AU_INFO, au_params::<GainParams>());
}
