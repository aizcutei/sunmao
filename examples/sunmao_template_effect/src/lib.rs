//! Starting point for a new SunMao effect: copy this crate, rename the types.
//! Ids derive from `VENDOR::NAME`; override `vst3_info`/`clap_info` only for
//! categories/features. Hold a `Smoother` to ramp a parameter per sample.

use sunmao::prelude::*;

#[derive(Params)]
pub struct TemplateEffectParams {
    #[unit = "LinearGain"]
    pub gain: FloatParam,
}

impl Default for TemplateEffectParams {
    fn default() -> Self {
        Self {
            gain: FloatParam::new("gain", "Gain", 1.0, 0.0, 2.0),
        }
    }
}

#[derive(Default)]
pub struct TemplateEffect {
    params: Arc<TemplateEffectParams>,
}

impl SunmaoPlugin for TemplateEffect {
    const NAME: &'static str = "SunMao Template Effect";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";
    type Params = TemplateEffectParams;

    fn params(&self) -> Arc<Self::Params> {
        self.params.clone()
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        _events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        buffer.copy_input_to_output();
        let gain = self.params.gain.get();
        for channel in 0..buffer.num_output_channels() {
            buffer.output(channel).iter_mut().for_each(|s| *s *= gain);
        }
        ProcessStatus::Normal
    }
}
sunmao::sunmao_export!(TemplateEffect);
