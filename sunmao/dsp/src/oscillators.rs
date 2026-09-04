//! Band-limited oscillators.
//!
//! A naive saw or pulse is a step discontinuity sampled directly, which folds
//! every harmonic above Nyquist back into the audible band as inharmonic
//! aliasing — audibly wrong, and worse the higher the note. These use PolyBLEP
//! (polynomial band-limited step) to round the discontinuity over the two
//! samples around it, which removes most of that aliasing for a couple of
//! multiplies.

/// Which waveform an [`Oscillator`] produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waveform {
    Sine,
    /// Rising sawtooth, -1 to 1.
    Saw,
    /// Rectangular wave; duty cycle set by [`Oscillator::set_pulse_width`].
    Pulse,
}

/// A single-voice oscillator.
///
/// ```
/// # use sunmao_dsp::oscillators::{Oscillator, Waveform};
/// let mut osc = Oscillator::new(Waveform::Sine);
/// osc.set_frequency(1_000.0, 48_000.0);
/// // A sine stays inside unity and comes back around.
/// let peak = (0..48).map(|_| osc.next().abs()).fold(0.0f32, f32::max);
/// assert!(peak <= 1.0);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Oscillator {
    waveform: Waveform,
    /// Phase in `0.0..1.0`.
    phase: f64,
    /// Phase advance per sample, i.e. frequency / sample rate.
    increment: f64,
    pulse_width: f64,
}

impl Default for Oscillator {
    /// A silent sine, so an oscillator can sit in a `#[derive(Default)]`
    /// plugin struct and be configured in `initialize`.
    fn default() -> Self {
        Self::new(Waveform::Sine)
    }
}

impl Oscillator {
    pub fn new(waveform: Waveform) -> Self {
        Self {
            waveform,
            phase: 0.0,
            increment: 0.0,
            pulse_width: 0.5,
        }
    }

    /// Sets the frequency in Hz.
    ///
    /// Clamped below Nyquist: a frequency at or above it has no meaningful
    /// waveform left, and letting the phase increment reach or exceed 0.5 would
    /// break the PolyBLEP correction's assumption that at most one
    /// discontinuity falls near any sample.
    pub fn set_frequency(&mut self, frequency_hz: f64, sample_rate: f64) {
        if !frequency_hz.is_finite() || !sample_rate.is_finite() || sample_rate <= 0.0 {
            self.increment = 0.0;
            return;
        }
        self.increment = (frequency_hz / sample_rate).clamp(0.0, 0.49);
    }

    /// Switches waveform without disturbing phase or frequency.
    ///
    /// Keeping the phase is deliberate: resetting it mid-note would produce a
    /// click exactly when a user is auditioning waveforms.
    ///
    /// ```
    /// # use sunmao_dsp::oscillators::{Oscillator, Waveform};
    /// let mut osc = Oscillator::new(Waveform::Sine);
    /// osc.set_frequency(1_000.0, 48_000.0);
    /// for _ in 0..12 {
    ///     osc.next();
    /// }
    /// let before = osc.next();
    /// osc.set_waveform(Waveform::Saw);
    /// // A saw at the same phase is a different value, but the oscillator did
    /// // not jump back to the start of its cycle.
    /// assert_ne!(before, osc.next());
    /// ```
    pub fn set_waveform(&mut self, waveform: Waveform) {
        self.waveform = waveform;
    }

    /// Sets the pulse duty cycle, clamped away from the degenerate extremes
    /// where the wave would be constant.
    pub fn set_pulse_width(&mut self, width: f32) {
        self.pulse_width = if width.is_finite() {
            f64::from(width).clamp(0.01, 0.99)
        } else {
            0.5
        };
    }

    /// Restarts the phase. Use on note-on so every note starts alike.
    pub fn reset(&mut self) {
        self.phase = 0.0;
    }

    /// Sets the phase directly, in `0.0..1.0`.
    pub fn set_phase(&mut self, phase: f32) {
        self.phase = if phase.is_finite() {
            f64::from(phase).rem_euclid(1.0)
        } else {
            0.0
        };
    }

    /// Produces the next sample.
    pub fn next(&mut self) -> f32 {
        let value = match self.waveform {
            Waveform::Sine => (self.phase * std::f64::consts::TAU).sin(),
            Waveform::Saw => {
                // Naive ramp, then round off the wrap discontinuity.
                let mut value = 2.0 * self.phase - 1.0;
                value -= poly_blep(self.phase, self.increment);
                value
            }
            Waveform::Pulse => {
                let mut value = if self.phase < self.pulse_width {
                    1.0
                } else {
                    -1.0
                };
                // Two discontinuities per cycle, in opposite directions: the
                // wrap at 0 and the edge at the duty point.
                value += poly_blep(self.phase, self.increment);
                value -= poly_blep(
                    (self.phase - self.pulse_width).rem_euclid(1.0),
                    self.increment,
                );
                value
            }
        };

        self.phase += self.increment;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        value as f32
    }
}

/// PolyBLEP residual for a unit upward step at phase 0.
///
/// Returns the correction to subtract from a naive waveform near its
/// discontinuity. Away from the discontinuity it is exactly zero, so the cost
/// is one comparison for most samples.
fn poly_blep(phase: f64, increment: f64) -> f64 {
    if increment <= 0.0 {
        return 0.0;
    }
    if phase < increment {
        // Just after the step.
        let t = phase / increment;
        return t + t - t * t - 1.0;
    }
    if phase > 1.0 - increment {
        // Just before the next one.
        let t = (phase - 1.0) / increment;
        return t * t + t + t + 1.0;
    }
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Largest jump between consecutive samples — a proxy for how much
    /// high-frequency energy a waveform contains.
    fn max_step(samples: &[f32]) -> f32 {
        samples
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0.0, f32::max)
    }

    fn collect(osc: &mut Oscillator, count: usize) -> Vec<f32> {
        (0..count).map(|_| osc.next()).collect()
    }

    /// The same saw without any band-limiting, for comparison.
    fn naive_saw(frequency: f64, sample_rate: f64, count: usize) -> Vec<f32> {
        let increment = frequency / sample_rate;
        let mut phase = 0.0f64;
        (0..count)
            .map(|_| {
                let value = (2.0 * phase - 1.0) as f32;
                phase = (phase + increment).fract();
                value
            })
            .collect()
    }

    #[test]
    fn every_waveform_stays_within_a_sane_amplitude() {
        for waveform in [Waveform::Sine, Waveform::Saw, Waveform::Pulse] {
            let mut osc = Oscillator::new(waveform);
            osc.set_frequency(440.0, 48_000.0);
            let samples = collect(&mut osc, 48_000);
            let peak = samples.iter().map(|s| s.abs()).fold(0.0, f32::max);
            // PolyBLEP overshoots slightly at the discontinuity by design.
            assert!(peak <= 1.2, "{waveform:?} peaked at {peak}");
            assert!(samples.iter().all(|s| s.is_finite()), "{waveform:?}");
        }
    }

    #[test]
    fn the_sine_completes_the_expected_number_of_cycles() {
        let mut osc = Oscillator::new(Waveform::Sine);
        osc.set_frequency(1_000.0, 48_000.0);
        let samples = collect(&mut osc, 48_000);
        // One second at 1 kHz: 1000 positive-going zero crossings.
        let crossings = samples
            .windows(2)
            .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
            .count();
        assert!(
            (999..=1001).contains(&crossings),
            "expected ~1000 cycles, counted {crossings}"
        );
    }

    #[test]
    fn band_limiting_removes_high_frequency_energy_a_naive_saw_would_alias() {
        // At a high fundamental a naive saw's wrap is a full-scale jump between
        // adjacent samples, which is exactly the aliasing PolyBLEP suppresses.
        let mut osc = Oscillator::new(Waveform::Saw);
        osc.set_frequency(8_000.0, 48_000.0);
        let limited = collect(&mut osc, 4_800);
        let naive = naive_saw(8_000.0, 48_000.0, 4_800);

        let limited_step = max_step(&limited);
        let naive_step = max_step(&naive);
        assert!(
            limited_step < naive_step * 0.75,
            "band-limited saw is not smoother: {limited_step} vs naive {naive_step}"
        );
    }

    #[test]
    fn the_pulse_width_changes_the_duty_cycle() {
        let mut osc = Oscillator::new(Waveform::Pulse);
        osc.set_frequency(100.0, 48_000.0);
        osc.set_pulse_width(0.25);
        let samples = collect(&mut osc, 48_000);
        let positive = samples.iter().filter(|s| **s > 0.0).count() as f64;
        let duty = positive / samples.len() as f64;
        assert!(
            (duty - 0.25).abs() < 0.02,
            "duty cycle was {duty}, expected ~0.25"
        );
    }

    #[test]
    fn extreme_frequencies_stay_finite() {
        for frequency in [0.0, -100.0, 24_000.0, 48_000.0, 1.0e9, f64::INFINITY] {
            for waveform in [Waveform::Sine, Waveform::Saw, Waveform::Pulse] {
                let mut osc = Oscillator::new(waveform);
                osc.set_frequency(frequency, 48_000.0);
                for _ in 0..1_024 {
                    let value = osc.next();
                    assert!(value.is_finite(), "{waveform:?} at {frequency}");
                    assert!(value.abs() <= 2.0, "{waveform:?} at {frequency}: {value}");
                }
            }
        }
    }

    #[test]
    fn a_degenerate_pulse_width_does_not_silence_the_oscillator() {
        // Clamping keeps a width of 0 or 1 from producing a constant.
        for width in [0.0, 1.0, f32::NAN] {
            let mut osc = Oscillator::new(Waveform::Pulse);
            osc.set_frequency(220.0, 48_000.0);
            osc.set_pulse_width(width);
            let samples = collect(&mut osc, 4_800);
            assert!(
                samples.iter().any(|s| *s > 0.0) && samples.iter().any(|s| *s < 0.0),
                "width {width} produced a constant"
            );
        }
    }

    #[test]
    fn reset_restarts_the_waveform_identically() {
        let mut osc = Oscillator::new(Waveform::Saw);
        osc.set_frequency(330.0, 48_000.0);
        let first = collect(&mut osc, 256);
        osc.reset();
        let second = collect(&mut osc, 256);
        assert_eq!(first, second);
    }
}
