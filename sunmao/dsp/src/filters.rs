//! Filters: one-pole, state-variable, and biquad.
//!
//! All three separate coefficient computation from processing. `set_*` does the
//! trigonometry; `process`/`tick` is arithmetic only.

use crate::{DENORMAL_FLOOR, flush_denormal};

/// Largest fraction of the sample rate a cutoff may reach.
///
/// The bilinear/TPT prewarp uses `tan(pi * f / fs)`, which diverges at Nyquist.
/// Clamping here keeps coefficients finite for any automation a host sends,
/// instead of producing an infinite `g` and a permanently NaN filter state.
const MAX_NORMALIZED_CUTOFF: f64 = 0.49;
/// Smallest fraction, so a cutoff of zero does not collapse the filter.
const MIN_NORMALIZED_CUTOFF: f64 = 1.0e-5;

/// Normalizes and clamps a cutoff into a safe fraction of the sample rate.
fn normalized_cutoff(cutoff_hz: f64, sample_rate: f64) -> f64 {
    if !cutoff_hz.is_finite() || !sample_rate.is_finite() || sample_rate <= 0.0 {
        // No usable rate: sit at the bottom of the range rather than NaN.
        return MIN_NORMALIZED_CUTOFF;
    }
    (cutoff_hz / sample_rate).clamp(MIN_NORMALIZED_CUTOFF, MAX_NORMALIZED_CUTOFF)
}

/// Which response a [`OnePole`] produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnePoleKind {
    Lowpass,
    Highpass,
}

/// A one-pole filter: 6 dB/octave, no resonance, one state variable.
///
/// ```
/// # use sunmao_dsp::filters::{OnePole, OnePoleKind};
/// let mut filter = OnePole::new(OnePoleKind::Lowpass);
/// filter.set_cutoff(1_000.0, 48_000.0);
/// // A lowpass fed a constant settles on that constant.
/// let mut out = 0.0;
/// for _ in 0..10_000 {
///     out = filter.process(1.0);
/// }
/// assert!((out - 1.0).abs() < 1.0e-3);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct OnePole {
    kind: OnePoleKind,
    /// Fraction of the input mixed in each sample.
    coefficient: f32,
    state: f32,
}

impl OnePole {
    pub fn new(kind: OnePoleKind) -> Self {
        Self {
            kind,
            coefficient: 1.0,
            state: 0.0,
        }
    }

    /// Sets the -3 dB cutoff. Safe for any input, including zero sample rate.
    pub fn set_cutoff(&mut self, cutoff_hz: f64, sample_rate: f64) {
        let normalized = normalized_cutoff(cutoff_hz, sample_rate);
        // Standard one-pole coefficient: 1 - e^(-2*pi*f/fs).
        let coefficient = 1.0 - (-std::f64::consts::TAU * normalized).exp();
        self.coefficient = coefficient.clamp(0.0, 1.0) as f32;
    }

    /// Clears the filter's memory.
    pub fn reset(&mut self) {
        self.state = 0.0;
    }

    /// Processes one sample.
    pub fn process(&mut self, input: f32) -> f32 {
        // A non-finite input would lodge in the state permanently; drop it and
        // keep filtering rather than poisoning every later sample.
        let input = if input.is_finite() { input } else { 0.0 };
        self.state = flush_denormal(self.state + self.coefficient * (input - self.state));
        match self.kind {
            OnePoleKind::Lowpass => self.state,
            OnePoleKind::Highpass => input - self.state,
        }
    }
}

/// The three simultaneous outputs of a [`Svf`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SvfOutput {
    pub lowpass: f32,
    pub bandpass: f32,
    pub highpass: f32,
}

/// A state-variable filter in Zavalishin's topology-preserving transform form.
///
/// TPT stays stable and keeps its cutoff accurate when parameters are modulated
/// per sample, which a naive digital SVF does not.
///
/// ```
/// # use sunmao_dsp::filters::Svf;
/// let mut filter = Svf::new();
/// filter.set_params(1_000.0, 0.0, 48_000.0);
/// // DC passes the lowpass and is rejected by the highpass.
/// let mut out = filter.tick(1.0);
/// for _ in 0..10_000 {
///     out = filter.tick(1.0);
/// }
/// assert!((out.lowpass - 1.0).abs() < 1.0e-3);
/// assert!(out.highpass.abs() < 1.0e-3);
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct Svf {
    /// Prewarped cutoff.
    g: f32,
    /// Damping; 2.0 is non-resonant, smaller is more resonant.
    k: f32,
    ic1: f32,
    ic2: f32,
}

impl Svf {
    pub fn new() -> Self {
        Self {
            g: 0.0,
            k: 2.0,
            ic1: 0.0,
            ic2: 0.0,
        }
    }

    /// Sets cutoff in Hz and resonance in `0.0..=1.0`.
    ///
    /// Resonance maps 0 to a non-resonant response and 1 to a pronounced but
    /// still bounded peak; the filter never self-oscillates, so a host sweeping
    /// resonance to its maximum cannot make it blow up.
    pub fn set_params(&mut self, cutoff_hz: f64, resonance: f32, sample_rate: f64) {
        let normalized = normalized_cutoff(cutoff_hz, sample_rate);
        self.g = (std::f64::consts::PI * normalized).tan() as f32;
        let resonance = if resonance.is_finite() {
            resonance.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.k = 2.0 - 1.9 * resonance;
    }

    /// Clears the filter's memory.
    pub fn reset(&mut self) {
        self.ic1 = 0.0;
        self.ic2 = 0.0;
    }

    /// Processes one sample, producing all three responses.
    pub fn tick(&mut self, input: f32) -> SvfOutput {
        let input = if input.is_finite() { input } else { 0.0 };
        let a1 = 1.0 / (1.0 + self.g * (self.g + self.k));
        let a2 = self.g * a1;
        let a3 = self.g * a2;
        let v3 = input - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        let ic1 = 2.0 * v1 - self.ic1;
        let ic2 = 2.0 * v2 - self.ic2;
        // The two integrator states have to be flushed as a *pair*, not one at
        // a time. They differ hugely in scale at low cutoff — |ic1| is about
        // `g` times |ic2|, so at 20 Hz / 96 kHz the bandpass state is ~1500x
        // smaller — and ic1 is the only fast decay path ic2 has: with ic1
        // pinned to zero, ic2 crawls down at the O(g²) rate `2*a3` instead of
        // the O(g*k) rate the filter is designed to decay at. Flushing them
        // independently therefore zeroes ic1 first and *slows the decay down*:
        // at 20 Hz, resonance 0.2, 96 kHz the filter took 6.8M samples (71
        // seconds) to reach zero instead of the 43k samples (0.45 s) its own
        // time constant implies. Flushing jointly keeps the pair coupled all
        // the way down, and costs the same one comparison per state.
        if ic1.abs() < DENORMAL_FLOOR && ic2.abs() < DENORMAL_FLOOR {
            self.ic1 = 0.0;
            self.ic2 = 0.0;
        } else {
            self.ic1 = ic1;
            self.ic2 = ic2;
        }
        SvfOutput {
            lowpass: v2,
            bandpass: v1,
            highpass: input - self.k * v1 - v2,
        }
    }
}

/// Which response a [`Biquad`] is configured for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BiquadKind {
    Lowpass,
    Highpass,
    Bandpass,
}

/// A biquad in transposed direct form II, with `f64` coefficients and state.
///
/// The precision is not a luxury. With `f32` coefficients the direct-form
/// recursion loses DC accuracy at low normalized cutoffs, because `a1` and `a2`
/// approach -2 and 1 and `1 + a1 + a2` becomes catastrophic cancellation. A
/// 20 Hz lowpass at 96 kHz was measured with a DC gain of **1.142** — a 14%
/// error at an entirely ordinary setting. `f64` state costs a handful of extra
/// multiplies per sample and removes the trap; [`Svf`] and [`OnePole`] are
/// accurate in `f32` because neither form has that cancellation.
///
/// ```
/// # use sunmao_dsp::filters::{Biquad, BiquadKind};
/// let mut filter = Biquad::new();
/// filter.set_params(BiquadKind::Lowpass, 1_000.0, 0.707, 48_000.0);
/// let mut out = 0.0;
/// for _ in 0..10_000 {
///     out = filter.process(1.0);
/// }
/// // Unity gain at DC for a lowpass.
/// assert!((out - 1.0).abs() < 1.0e-3);
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    s1: f64,
    s2: f64,
}

impl Biquad {
    pub fn new() -> Self {
        // Identity until configured, so an unconfigured filter passes audio
        // through rather than silencing it.
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            s1: 0.0,
            s2: 0.0,
        }
    }

    /// Sets the response, cutoff in Hz, and Q.
    ///
    /// Q is clamped to a sane band: a non-positive Q makes the standard RBJ
    /// coefficients divide by zero, and an enormous Q produces a filter so
    /// resonant that f32 cannot keep it stable.
    pub fn set_params(&mut self, kind: BiquadKind, cutoff_hz: f64, q: f32, sample_rate: f64) {
        let normalized = normalized_cutoff(cutoff_hz, sample_rate);
        let q = if q.is_finite() {
            q.clamp(0.05, 40.0) as f64
        } else {
            0.707
        };

        // RBJ cookbook forms.
        let omega = std::f64::consts::TAU * normalized;
        let (sin_omega, cos_omega) = omega.sin_cos();
        let alpha = sin_omega / (2.0 * q);

        let (b0, b1, b2, a0, a1, a2) = match kind {
            BiquadKind::Lowpass => {
                let b1 = 1.0 - cos_omega;
                (
                    b1 / 2.0,
                    b1,
                    b1 / 2.0,
                    1.0 + alpha,
                    -2.0 * cos_omega,
                    1.0 - alpha,
                )
            }
            BiquadKind::Highpass => {
                let b0 = (1.0 + cos_omega) / 2.0;
                (
                    b0,
                    -(1.0 + cos_omega),
                    b0,
                    1.0 + alpha,
                    -2.0 * cos_omega,
                    1.0 - alpha,
                )
            }
            BiquadKind::Bandpass => (
                alpha,
                0.0,
                -alpha,
                1.0 + alpha,
                -2.0 * cos_omega,
                1.0 - alpha,
            ),
        };

        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    /// Clears the filter's memory.
    pub fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }

    /// Processes one sample.
    pub fn process(&mut self, input: f32) -> f32 {
        let input = if input.is_finite() {
            f64::from(input)
        } else {
            0.0
        };
        let output = self.b0 * input + self.s1;
        let s1 = self.b1 * input - self.a1 * output + self.s2;
        let s2 = self.b2 * input - self.a2 * output;
        // Flushed as a pair, for the same reason as [`Svf`] — and here the
        // independent version does worse than stall, it *pumps*. At a low
        // cutoff `a1` approaches -2 and `a2` approaches 1, so the decay comes
        // out of the near-cancellation between `-a1 * output` and `s2`. Zero
        // `s2` on its own and what is left is `s1 * -a1`, a gain of nearly two:
        // the pair then sits in a limit cycle just above the floor, each state
        // taking turns being zeroed and pumped back up. A 121 Hz lowpass at
        // 96 kHz was measured parked at 6.2e-20 indefinitely. Zeroing only
        // when both states are inside the floor keeps the cancellation intact.
        if s1.abs() < f64::from(DENORMAL_FLOOR) && s2.abs() < f64::from(DENORMAL_FLOOR) {
            self.s1 = 0.0;
            self.s2 = 0.0;
        } else {
            self.s1 = s1;
            self.s2 = s2;
        }
        output as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Peak magnitude of a filter's steady-state response to a sine.
    fn response(process: &mut impl FnMut(f32) -> f32, frequency: f64, sample_rate: f64) -> f32 {
        let increment = frequency / sample_rate;
        let mut phase = 0.0f64;
        // Settle first, then measure, so the transient is not counted.
        for _ in 0..20_000 {
            let sample = (phase * std::f64::consts::TAU).sin() as f32;
            phase = (phase + increment).fract();
            process(sample);
        }
        let mut peak = 0.0f32;
        for _ in 0..20_000 {
            let sample = (phase * std::f64::consts::TAU).sin() as f32;
            phase = (phase + increment).fract();
            peak = peak.max(process(sample).abs());
        }
        peak
    }

    #[test]
    fn a_one_pole_lowpass_passes_dc_and_rejects_high_frequencies() {
        let mut filter = OnePole::new(OnePoleKind::Lowpass);
        filter.set_cutoff(500.0, 48_000.0);
        let low = response(&mut |x| filter.process(x), 50.0, 48_000.0);
        filter.reset();
        let high = response(&mut |x| filter.process(x), 12_000.0, 48_000.0);
        assert!(low > 0.9, "passband attenuated: {low}");
        assert!(high < 0.1, "stopband not attenuated: {high}");
    }

    #[test]
    fn a_one_pole_highpass_is_the_complement() {
        let mut filter = OnePole::new(OnePoleKind::Highpass);
        filter.set_cutoff(500.0, 48_000.0);
        let low = response(&mut |x| filter.process(x), 50.0, 48_000.0);
        filter.reset();
        let high = response(&mut |x| filter.process(x), 12_000.0, 48_000.0);
        assert!(low < 0.2, "DC leaked through: {low}");
        assert!(high > 0.9, "passband attenuated: {high}");
    }

    #[test]
    fn the_svf_separates_its_three_outputs() {
        let mut filter = Svf::new();
        filter.set_params(1_000.0, 0.0, 48_000.0);

        let low = response(&mut |x| filter.tick(x).lowpass, 60.0, 48_000.0);
        filter.reset();
        let high_through_lp = response(&mut |x| filter.tick(x).lowpass, 15_000.0, 48_000.0);
        filter.reset();
        let high = response(&mut |x| filter.tick(x).highpass, 15_000.0, 48_000.0);
        filter.reset();
        let centre = response(&mut |x| filter.tick(x).bandpass, 1_000.0, 48_000.0);
        filter.reset();
        let off_centre = response(&mut |x| filter.tick(x).bandpass, 60.0, 48_000.0);

        assert!(low > 0.9, "lowpass passband: {low}");
        assert!(high_through_lp < 0.1, "lowpass stopband: {high_through_lp}");
        assert!(high > 0.9, "highpass passband: {high}");
        assert!(centre > off_centre * 4.0, "bandpass not selective");
    }

    #[test]
    fn svf_resonance_peaks_without_blowing_up() {
        // The documented promise: resonance 1.0 is pronounced but bounded.
        let mut filter = Svf::new();
        filter.set_params(1_000.0, 1.0, 48_000.0);
        let peak = response(&mut |x| filter.tick(x).bandpass, 1_000.0, 48_000.0);
        assert!(peak > 1.0, "resonance produced no lift: {peak}");
        assert!(
            peak.is_finite() && peak < 100.0,
            "resonance ran away: {peak}"
        );
    }

    #[test]
    fn a_cutoff_above_nyquist_stays_stable() {
        // A host can automate cutoff anywhere; `tan` diverges at Nyquist.
        for filter_cutoff in [24_000.0, 48_000.0, 1.0e9, f64::INFINITY] {
            let mut svf = Svf::new();
            svf.set_params(filter_cutoff, 1.0, 48_000.0);
            let mut biquad = Biquad::new();
            biquad.set_params(BiquadKind::Lowpass, filter_cutoff, 0.707, 48_000.0);
            let mut one_pole = OnePole::new(OnePoleKind::Lowpass);
            one_pole.set_cutoff(filter_cutoff, 48_000.0);

            for index in 0..4_096 {
                let input = if index % 2 == 0 { 1.0 } else { -1.0 };
                let svf_out = svf.tick(input);
                assert!(svf_out.lowpass.is_finite(), "svf at {filter_cutoff}");
                assert!(
                    biquad.process(input).is_finite(),
                    "biquad at {filter_cutoff}"
                );
                assert!(
                    one_pole.process(input).is_finite(),
                    "one pole at {filter_cutoff}"
                );
            }
        }
    }

    #[test]
    fn state_flushes_to_zero_when_the_input_goes_silent() {
        // Without flushing, the decay tail lives in denormals, where some CPUs
        // are dramatically slower — a plugin that costs more when quiet.
        let mut filter = Svf::new();
        filter.set_params(2_000.0, 0.0, 48_000.0);
        for _ in 0..1_000 {
            filter.tick(1.0);
        }
        for _ in 0..200_000 {
            filter.tick(0.0);
        }
        assert_eq!(filter.ic1, 0.0, "bandpass state did not flush");
        assert_eq!(filter.ic2, 0.0, "lowpass state did not flush");
    }

    #[test]
    fn a_non_finite_input_does_not_poison_the_filter() {
        let mut filter = Svf::new();
        filter.set_params(1_000.0, 0.5, 48_000.0);
        filter.tick(f32::NAN);
        filter.tick(f32::INFINITY);
        for _ in 0..64 {
            let out = filter.tick(0.5);
            assert!(out.lowpass.is_finite() && out.highpass.is_finite());
        }
    }

    #[test]
    fn an_unconfigured_biquad_passes_audio_through() {
        let mut filter = Biquad::new();
        for input in [-1.0, 0.0, 0.25, 1.0] {
            assert_eq!(filter.process(input), input);
        }
    }

    #[test]
    fn a_biquad_bandpass_is_selective() {
        let mut filter = Biquad::new();
        filter.set_params(BiquadKind::Bandpass, 1_000.0, 4.0, 48_000.0);
        let centre = response(&mut |x| filter.process(x), 1_000.0, 48_000.0);
        filter.reset();
        let low = response(&mut |x| filter.process(x), 100.0, 48_000.0);
        filter.reset();
        let high = response(&mut |x| filter.process(x), 10_000.0, 48_000.0);
        assert!(centre > low * 4.0 && centre > high * 4.0, "not selective");
    }
}
