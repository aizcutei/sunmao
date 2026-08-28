//! Starting point for a new SunMao instrument: copy this crate and rename the
//! types. Zero `input_channels` plus `accepts_midi` is what makes a host treat
//! this as an instrument; ids derive from `VENDOR::NAME`.

use sunmao::prelude::*;
use sunmao_dsp::prelude::*;

#[derive(Params)]
pub struct TemplateInstrumentParams {
    #[unit = "LinearGain"]
    pub level: FloatParam,
}

impl Default for TemplateInstrumentParams {
    fn default() -> Self {
        Self {
            level: FloatParam::new("level", "Level", 0.5, 0.0, 1.0),
        }
    }
}

#[derive(Default)]
pub struct TemplateInstrument {
    params: Arc<TemplateInstrumentParams>,
    sample_rate: f64,
    osc: Oscillator,
    env: Adsr,
}

impl SunmaoPlugin for TemplateInstrument {
    const NAME: &'static str = "SunMao Template Instrument";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";
    type Params = TemplateInstrumentParams;

    fn input_channels(&self) -> u32 {
        0
    }

    fn accepts_midi(&self) -> bool {
        true
    }

    fn params(&self) -> Arc<Self::Params> {
        self.params.clone()
    }

    fn initialize(&mut self, sample_rate: f64, _max_block_size: u32) {
        self.sample_rate = sample_rate;
        self.env.set_params(0.005, 0.100, 0.7, 0.200, sample_rate);
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        for event in events.iter() {
            if let Event::Midi(message) = event {
                if message.is_note_on() {
                    let hz = 440.0 * 2.0_f64.powf((f64::from(message.note()) - 69.0) / 12.0);
                    self.osc.set_frequency(hz, self.sample_rate);
                    self.env.gate_on();
                } else if message.is_note_off() {
                    self.env.gate_off();
                }
            }
        }
        let level = self.params.level.get();
        for index in 0..buffer.num_samples() {
            let sample = self.osc.next() * self.env.next() * level;
            for channel in 0..buffer.num_output_channels() {
                buffer.output(channel)[index] = sample;
            }
        }
        ProcessStatus::Normal
    }
}

sunmao::sunmao_export!(TemplateInstrument);
