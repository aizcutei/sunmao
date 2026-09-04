//! Starting point for a new SunMao instrument: copy this crate and rename the
//! types. `IS_INSTRUMENT` is what makes a host treat this as an instrument
//! rather than an effect; ids derive from `VENDOR::NAME`.

use sunmao::prelude::*;

#[derive(Params)]
pub struct TemplateInstrumentParams {
    #[param(name = "Level", default = 0.5, range = 0.0..=1.0, unit = "LinearGain")]
    pub level: FloatParam,
}

#[derive(Default)]
pub struct TemplateInstrument {
    params: Arc<TemplateInstrumentParams>,
    voice: MonoVoice,
}

impl SunmaoPlugin for TemplateInstrument {
    const NAME: &'static str = "SunMao Template Instrument";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";
    const IS_INSTRUMENT: bool = true;
    type Params = TemplateInstrumentParams;

    fn params(&self) -> Arc<Self::Params> {
        self.params.clone()
    }

    fn initialize(&mut self, sample_rate: f64, _max_block_size: u32) {
        self.voice.prepare(sample_rate);
    }

    fn reset(&mut self) {
        self.voice.reset();
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        // `render` applies each note at its own sample offset.
        self.voice.render(buffer, events, self.params.level.get());
        ProcessStatus::Normal
    }
}
sunmao::sunmao_export!(TemplateInstrument);
