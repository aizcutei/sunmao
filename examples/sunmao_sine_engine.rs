//! Shared DSP engine for the GUI sine-synth examples.
//!
//! The engine deliberately has no format or windowing dependencies. Each GUI
//! example wraps it in a `SunmaoPlugin` implementation and supplies one view
//! backend, while the audio/MIDI behavior stays identical across all builds.

use sunmao::prelude::*;

#[derive(Params)]
pub struct SineParams {
    /// Master volume (0.0 to 1.0).
    pub volume: FloatParam,
}

impl Default for SineParams {
    fn default() -> Self {
        Self {
            volume: FloatParam::new("volume", "Volume", 0.5, 0.0, 1.0),
        }
    }
}

struct Voice {
    note: u8,
    velocity: f32,
    phase: f64,
    active: bool,
}

impl Voice {
    fn new() -> Self {
        Self {
            note: 0,
            velocity: 0.0,
            phase: 0.0,
            active: false,
        }
    }
}

/// Format-independent polyphonic sine oscillator.
pub struct SineEngine {
    params: Arc<SineParams>,
    voices: [Voice; 8],
    sample_rate: f64,
}

impl Default for SineEngine {
    fn default() -> Self {
        Self {
            params: Arc::new(SineParams::default()),
            voices: std::array::from_fn(|_| Voice::new()),
            sample_rate: 44100.0,
        }
    }
}

impl SineEngine {
    pub fn params(&self) -> Arc<SineParams> {
        Arc::clone(&self.params)
    }

    pub fn initialize(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    pub fn reset(&mut self) {
        for voice in &mut self.voices {
            voice.active = false;
            voice.velocity = 0.0;
            voice.phase = 0.0;
        }
    }

    pub fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        let mut volume = self.params.volume.get();
        let num_samples = buffer.num_samples();
        buffer.clear();
        let mut events = events.timed_events().peekable();

        for sample_index in 0..num_samples {
            while events
                .peek()
                .is_some_and(|event| event.offset() as usize <= sample_index)
            {
                match events.next().expect("peeked event") {
                    Event::Midi(event) if event.is_note_on() => {
                        if let Some(voice) = self.voices.iter_mut().find(|voice| !voice.active) {
                            voice.note = event.note();
                            voice.velocity = event.velocity() as f32 / 127.0;
                            voice.phase = 0.0;
                            voice.active = true;
                        }
                    }
                    Event::Midi(event) if event.is_note_off() => {
                        if let Some(voice) = self
                            .voices
                            .iter_mut()
                            .find(|voice| voice.active && voice.note == event.note())
                        {
                            voice.active = false;
                        }
                    }
                    Event::ParamChange { id, value, .. } if id == self.params.volume.id => {
                        volume = self.params.volume.min
                            + value.clamp(0.0, 1.0)
                                * (self.params.volume.max - self.params.volume.min);
                    }
                    _ => {}
                }
            }

            for voice in self.voices.iter_mut().filter(|voice| voice.active) {
                let freq = 440.0 * 2.0_f64.powf((voice.note as f64 - 69.0) / 12.0);
                let phase_inc = freq / self.sample_rate;
                let sample = (voice.phase * std::f64::consts::TAU).sin() as f32;
                let out = sample * voice.velocity * volume;

                if buffer.num_output_channels() > 0 {
                    buffer.output(0)[sample_index] += out;
                }
                if buffer.num_output_channels() > 1 {
                    buffer.output(1)[sample_index] += out;
                }

                voice.phase += phase_inc;
                if voice.phase >= 1.0 {
                    voice.phase -= 1.0;
                }
            }
        }

        ProcessStatus::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(samples: &[f32]) -> f32 {
        samples
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()))
    }

    #[test]
    fn reset_silences_active_voices() {
        let mut engine = SineEngine::default();
        engine.initialize(48_000.0);
        let inputs: [&[f32]; 0] = [];
        let mut left = [0.0; 64];
        let mut right = [0.0; 64];
        let mut events = EventQueue::new();
        events.push(Event::Midi(MidiMessage::note_on(0, 0, 60, 127)));
        {
            let mut outputs: [&mut [f32]; 2] = [&mut left, &mut right];
            let mut buffer = AudioBuffer::new(&inputs, &mut outputs, 64);
            engine.process(
                &mut buffer,
                &events,
                &ProcessContext {
                    sample_rate: 48_000.0,
                    tempo: None,
                    is_playing: true,
                    sample_pos: 0,
                    ..Default::default()
                },
            );
        }
        assert!(peak(&left) > 1.0e-6);

        engine.reset();
        left.fill(0.0);
        right.fill(0.0);
        let empty = EventQueue::new();
        {
            let mut outputs: [&mut [f32]; 2] = [&mut left, &mut right];
            let mut buffer = AudioBuffer::new(&inputs, &mut outputs, 64);
            engine.process(
                &mut buffer,
                &empty,
                &ProcessContext {
                    sample_rate: 48_000.0,
                    tempo: None,
                    is_playing: false,
                    sample_pos: 64,
                    ..Default::default()
                },
            );
        }
        assert_eq!(peak(&left), 0.0);
        assert_eq!(peak(&right), 0.0);
    }
}
