//! Starting point for a new SunMao instrument: copy this crate and rename the
//! types. `input_channels` of zero plus `accepts_midi` is what makes a host
//! treat this as an instrument; ids are derived from `VENDOR::NAME`.

use sunmao::prelude::*;

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
    phase: f64,
    note: Option<u8>,
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
                    self.note = Some(message.note());
                } else if message.is_note_off() && self.note == Some(message.note()) {
                    self.note = None;
                }
            }
        }
        let level = self.params.level.get();
        let increment = self
            .note
            .map(|note| 440.0 * 2.0_f64.powf((f64::from(note) - 69.0) / 12.0) / self.sample_rate)
            .unwrap_or(0.0);
        for index in 0..buffer.num_samples() {
            let sample = if self.note.is_some() {
                (self.phase * std::f64::consts::TAU).sin() as f32 * level
            } else {
                0.0
            };
            self.phase = (self.phase + increment).fract();
            for channel in 0..buffer.num_output_channels() {
                buffer.output(channel)[index] = sample;
            }
        }
        ProcessStatus::Normal
    }
}

sunmao::sunmao_export!(TemplateInstrument);
