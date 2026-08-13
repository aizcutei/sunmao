//! SunMao sine synthesizer with an embeddable OpenGL editor.

use std::sync::Arc;

use sunmao_core::prelude::*;
use sunmao_gui::{
    Color, Event as GuiEvent, Fill, GuiContext, MouseButton as GuiMouseButton, ParameterWidget,
    Rect, Slider, Widget,
};
use sunmao_view_baseview::{BaseviewConfig, BaseviewView, ViewState, WindowScalePolicy};

#[path = "../../sunmao_sine_engine.rs"]
mod sine_engine;
use sine_engine::{SineEngine, SineParams};

pub struct SineSynthGl {
    engine: SineEngine,
}

impl Default for SineSynthGl {
    fn default() -> Self {
        Self {
            engine: SineEngine::default(),
        }
    }
}

impl SunmaoPlugin for SineSynthGl {
    const NAME: &'static str = "SunMao Sine Synth GL";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";

    type Params = SineParams;

    fn input_channels(&self) -> u32 {
        0
    }

    fn output_channels(&self) -> u32 {
        2
    }

    fn accepts_midi(&self) -> bool {
        true
    }

    fn params(&self) -> Arc<Self::Params> {
        self.engine.params()
    }

    fn initialize(&mut self, sample_rate: f64, _max_block_size: u32) {
        self.engine.initialize(sample_rate);
    }

    fn reset(&mut self) {
        self.engine.reset();
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        events: &EventQueue,
        context: &ProcessContext,
    ) -> ProcessStatus {
        self.engine.process(buffer, events, context)
    }

    fn view(&self) -> Option<Box<dyn SunmaoView>> {
        let config = BaseviewConfig {
            title: Self::NAME.to_string(),
            width: 400,
            height: 120,
            scale_policy: WindowScalePolicy::SystemScaleFactor,
            background: Color::rgb(0.08, 0.11, 0.14),
        };
        Some(Box::new(BaseviewView::new(config, |context| {
            SineGlViewState::new(context, 400.0, 120.0)
        })))
    }

    fn vst3_info() -> Vst3Info {
        Vst3Info {
            class_id: *b"SunMaoSineGuiGL!",
            categories: &["Instrument", "Synth"],
            ..Default::default()
        }
    }

    fn clap_info() -> ClapInfo {
        ClapInfo {
            id: "com.sunmao.synth.sine.gl",
            features: &["instrument", "synthesizer", "stereo"],
        }
    }
}

struct SineGlViewState {
    slider: Slider,
    context: Arc<dyn ViewContext>,
    editing: bool,
}

impl SineGlViewState {
    fn new(context: Arc<dyn ViewContext>, width: f32, height: f32) -> Self {
        let mut slider = Slider::new("volume").with_default(0.5);
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
        if let Some(value) = self.context.get_param("volume") {
            self.slider.set_value(value);
        }
    }
}

impl ViewState for SineGlViewState {
    fn draw(&mut self, ctx: &mut dyn GuiContext, width: f32, height: f32) {
        self.sync_from_params();
        ctx.fill_rect(
            0.0,
            0.0,
            width,
            height,
            Fill::Solid(Color::rgb(0.08, 0.11, 0.14)),
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
                self.context.begin_edit("volume");
            }
            GuiEvent::MouseUp {
                button: GuiMouseButton::Left,
                ..
            } if self.editing => {
                self.editing = false;
                self.context.end_edit("volume");
            }
            _ => {}
        }

        if handled && (after - before).abs() > f32::EPSILON {
            self.context.set_param("volume", after);
        }
        handled
    }

    fn on_resize(&mut self, width: f32, height: f32) {
        self.relayout(width, height);
    }
}

sunmao::sunmao_export!(SineSynthGl, gui);
