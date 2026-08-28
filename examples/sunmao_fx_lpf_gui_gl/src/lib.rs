//! SunMao Lowpass Filter Plugin with OpenGL GUI
//!
//! This example demonstrates a simple lowpass effect with
//! frequency and Q controls, plus a custom OpenGL GUI.

use std::f32::consts::PI;
use sunmao::prelude::*;

const FREQ_MIN: f32 = 20.0;
const FREQ_MAX: f32 = 20000.0;
const Q_MIN: f32 = 0.1;
const Q_MAX: f32 = 10.0;

fn normalize(value: f32, min: f32, max: f32) -> f32 {
    ((value - min) / (max - min)).clamp(0.0, 1.0)
}

fn denormalize(norm: f32, min: f32, max: f32) -> f32 {
    min + norm.clamp(0.0, 1.0) * (max - min)
}

#[derive(Clone, Copy)]
struct BiquadCoeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Default for BiquadCoeffs {
    fn default() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }
    }
}

struct Biquad {
    coeffs: BiquadCoeffs,
    z1: [f32; 2],
    z2: [f32; 2],
    sample_rate: f32,
    freq: f32,
    q: f32,
}

impl Default for Biquad {
    fn default() -> Self {
        Self {
            coeffs: BiquadCoeffs::default(),
            z1: [0.0; 2],
            z2: [0.0; 2],
            sample_rate: 44100.0,
            freq: 1000.0,
            q: 0.707,
        }
    }
}

impl Biquad {
    fn reset(&mut self) {
        self.z1 = [0.0; 2];
        self.z2 = [0.0; 2];
    }

    fn update(&mut self, sample_rate: f32, freq: f32, q: f32) {
        if sample_rate <= 0.0 {
            return;
        }

        let max_freq = (sample_rate * 0.45).min(FREQ_MAX);
        let freq = freq.clamp(FREQ_MIN, max_freq);
        let q = q.clamp(Q_MIN, Q_MAX);

        let needs_update = (sample_rate - self.sample_rate).abs() > f32::EPSILON
            || (freq - self.freq).abs() > 1e-3
            || (q - self.q).abs() > 1e-3;

        if !needs_update {
            return;
        }

        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = (1.0 - cos_w0) * 0.5;
        let b1 = 1.0 - cos_w0;
        let b2 = (1.0 - cos_w0) * 0.5;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        self.coeffs = BiquadCoeffs {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        };

        self.sample_rate = sample_rate;
        self.freq = freq;
        self.q = q;
    }

    fn process_sample(&mut self, channel: usize, sample: f32) -> f32 {
        let output = self.coeffs.b0 * sample + self.z1[channel];
        self.z1[channel] = self.coeffs.b1 * sample - self.coeffs.a1 * output + self.z2[channel];
        self.z2[channel] = self.coeffs.b2 * sample - self.coeffs.a2 * output;
        output
    }
}

// ============ Plugin Definition ============

#[derive(Params)]
#[cfg_attr(all(target_os = "macos", feature = "au"), sunmao_au)]
pub struct LpfParams {
    pub cutoff: FloatParam,
    pub q: FloatParam,
}

impl Default for LpfParams {
    fn default() -> Self {
        Self {
            cutoff: FloatParam::new("cutoff", "Cutoff", 1000.0, FREQ_MIN, FREQ_MAX),
            q: FloatParam::new("q", "Q", 0.707, Q_MIN, Q_MAX),
        }
    }
}

pub struct LpfPlugin {
    params: Arc<LpfParams>,
    filter: Biquad,
    sample_rate: f32,
}

impl Default for LpfPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(LpfParams::default()),
            filter: Biquad::default(),
            sample_rate: 44100.0,
        }
    }
}

impl SunmaoPlugin for LpfPlugin {
    const NAME: &'static str = "SunMao LPF GL";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";

    type Params = LpfParams;

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

    fn clap_info() -> ClapInfo {
        ClapInfo {
            id: "com.sunmao.fx.lpf.gl",
            features: &["audio-effect", "filter"],
        }
    }

    fn initialize(&mut self, sample_rate: f64, _max_frames: u32) {
        self.sample_rate = sample_rate as f32;
        self.filter.update(
            self.sample_rate,
            self.params.cutoff.get(),
            self.params.q.get(),
        );
    }

    fn reset(&mut self) {
        self.filter.reset();
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        let mut cutoff = self.params.cutoff.get();
        let mut q = self.params.q.get();
        self.filter.update(self.sample_rate, cutoff, q);
        buffer.copy_input_to_output();
        let channels = buffer
            .num_input_channels()
            .min(buffer.num_output_channels())
            .min(self.filter.z1.len());
        let mut changes = events
            .param_changes()
            .filter(|change| change.id == self.params.cutoff.id || change.id == self.params.q.id)
            .peekable();

        for sample_index in 0..buffer.num_samples() {
            let mut coefficients_changed = false;
            while changes
                .peek()
                .is_some_and(|change| change.offset as usize <= sample_index)
            {
                let change = changes.next().expect("peeked parameter change");
                if change.id == self.params.cutoff.id {
                    cutoff =
                        denormalize(change.value, self.params.cutoff.min, self.params.cutoff.max);
                    coefficients_changed = true;
                } else if change.id == self.params.q.id {
                    q = denormalize(change.value, self.params.q.min, self.params.q.max);
                    coefficients_changed = true;
                }
            }
            if coefficients_changed {
                self.filter.update(self.sample_rate, cutoff, q);
            }

            for channel in 0..channels {
                let input = buffer.output(channel)[sample_index];
                buffer.output(channel)[sample_index] = self.filter.process_sample(channel, input);
            }
            for channel in channels..buffer.num_output_channels() {
                buffer.output(channel)[sample_index] = 0.0;
            }
        }
        ProcessStatus::Normal
    }

    fn view(&self) -> Option<Box<dyn SunmaoView>> {
        let config = BaseviewConfig {
            title: "SunMao LPF".to_string(),
            width: 420,
            height: 160,
            scale_policy: WindowScalePolicy::SystemScaleFactor,
            background: Color::rgb(0.12, 0.12, 0.18),
        };

        let view = BaseviewView::new(config, |context| LpfViewState::new(context, 420.0, 160.0));
        Some(Box::new(view))
    }
}

struct LpfViewState {
    freq_slider: Slider,
    q_slider: Slider,
    context: Arc<dyn ViewContext>,
    editing_freq: bool,
    editing_q: bool,
}

impl LpfViewState {
    fn new(context: Arc<dyn ViewContext>, width: f32, height: f32) -> Self {
        let freq_default = normalize(1000.0, FREQ_MIN, FREQ_MAX);
        let q_default = normalize(0.707, Q_MIN, Q_MAX);
        let freq_slider = Slider::new("cutoff").with_default(freq_default);
        let q_slider = Slider::new("q").with_default(q_default);

        let mut state = Self {
            freq_slider,
            q_slider,
            context,
            editing_freq: false,
            editing_q: false,
        };
        state.relayout(width, height);
        state
    }

    fn relayout(&mut self, width: f32, height: f32) {
        let slider_height = 24.0;
        let margin = 20.0;
        let width = (width - margin * 2.0).max(1.0);

        let top_y = (height * 0.33 - slider_height * 0.5).max(4.0);
        let bottom_y = (height * 0.66 - slider_height * 0.5).max(4.0);

        self.freq_slider
            .set_bounds(Rect::new(margin, top_y, width, slider_height));
        self.q_slider
            .set_bounds(Rect::new(margin, bottom_y, width, slider_height));
    }

    fn sync_from_params(&mut self) {
        if let Some(value) = self.context.get_param("cutoff") {
            self.freq_slider.set_value(value);
        }
        if let Some(value) = self.context.get_param("q") {
            self.q_slider.set_value(value);
        }
    }

    fn update_editing(
        context: &Arc<dyn ViewContext>,
        id: &str,
        handled: bool,
        editing: &mut bool,
        event: &GuiEvent,
    ) {
        match event {
            GuiEvent::MouseDown {
                button: GuiMouseButton::Left,
                ..
            } if handled => {
                *editing = true;
                context.begin_edit(id);
            }
            GuiEvent::MouseUp {
                button: GuiMouseButton::Left,
                ..
            } if *editing => {
                *editing = false;
                context.end_edit(id);
            }
            _ => {}
        }
    }
}

impl ViewState for LpfViewState {
    fn draw(&mut self, ctx: &mut dyn GuiContext, width: f32, height: f32) {
        self.sync_from_params();
        ctx.fill_rect(
            0.0,
            0.0,
            width,
            height,
            Fill::Solid(Color::rgb(0.14, 0.14, 0.2)),
        );
        self.freq_slider.draw(ctx);
        self.q_slider.draw(ctx);
    }

    fn on_mouse_event(&mut self, event: &GuiEvent) -> bool {
        let freq_before = self.freq_slider.value();
        let freq_handled = self.freq_slider.handle_event(event);
        let freq_after = self.freq_slider.value();

        if freq_handled && (freq_after - freq_before).abs() > f32::EPSILON {
            self.context.set_param("cutoff", freq_after);
        }

        Self::update_editing(
            &self.context,
            "cutoff",
            freq_handled,
            &mut self.editing_freq,
            event,
        );

        if freq_handled {
            return true;
        }

        let q_before = self.q_slider.value();
        let q_handled = self.q_slider.handle_event(event);
        let q_after = self.q_slider.value();

        if q_handled && (q_after - q_before).abs() > f32::EPSILON {
            self.context.set_param("q", q_after);
        }

        Self::update_editing(&self.context, "q", q_handled, &mut self.editing_q, event);

        q_handled
    }

    fn on_resize(&mut self, width: f32, height: f32) {
        self.relayout(width, height);
    }
}

// ============ Unified VST3 + CLAP Export ============
sunmao::sunmao_export!(LpfPlugin, gui);

// ============ AU Export (macOS only) ============
#[cfg(all(target_os = "macos", feature = "au"))]
mod au_export {
    use super::*;
    use sunmao_backend_au::{au_params, fourcc, PluginInfo};

    const AU_INFO: PluginInfo = PluginInfo {
        name: "SunMao LPF GL",
        component_type: fourcc(b"aufx"),
        component_subtype: fourcc(b"slpf"),
        manufacturer: fourcc(b"SunM"),
        version: 0x00010000,
        flags: 0,
        flags_mask: 0,
        input_channels: 2,
        output_channels: 2,
        supports_midi: false,
    };

    sunmao_backend_au::sunmao_export_au_with_view!(LpfPlugin, AU_INFO, au_params::<LpfParams>());
}
