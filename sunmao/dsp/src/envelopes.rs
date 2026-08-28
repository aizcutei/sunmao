//! Envelopes: an ADSR generator and an amplitude follower.

use crate::flush_denormal;

/// Which segment an [`Adsr`] is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdsrStage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// A linear ADSR envelope.
///
/// Linear rather than exponential segments, because a linear attack reaches its
/// peak exactly and predictably; an exponential one only approaches it, which
/// is the same asymptote problem the parameter smoother had to bound.
///
/// ```
/// # use sunmao_dsp::envelopes::{Adsr, AdsrStage};
/// let mut env = Adsr::new();
/// env.set_params(0.001, 0.001, 0.5, 0.001, 48_000.0);
/// env.gate_on();
/// // Runs up to the peak, then settles on the sustain level.
/// for _ in 0..1_000 {
///     env.next();
/// }
/// assert_eq!(env.stage(), AdsrStage::Sustain);
/// assert!((env.next() - 0.5).abs() < 1.0e-6);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Adsr {
    stage: AdsrStage,
    level: f32,
    sustain: f32,
    attack_step: f32,
    decay_step: f32,
    release_step: f32,
}

impl Default for Adsr {
    fn default() -> Self {
        Self::new()
    }
}

impl Adsr {
    pub fn new() -> Self {
        Self {
            stage: AdsrStage::Idle,
            level: 0.0,
            sustain: 1.0,
            attack_step: 1.0,
            decay_step: 1.0,
            release_step: 1.0,
        }
    }

    /// Sets segment times in seconds and the sustain level in `0.0..=1.0`.
    ///
    /// A zero or negative time becomes an immediate transition rather than a
    /// division by zero, so a host automating a time to its minimum gets a
    /// click, not a NaN.
    pub fn set_params(
        &mut self,
        attack_seconds: f32,
        decay_seconds: f32,
        sustain: f32,
        release_seconds: f32,
        sample_rate: f64,
    ) {
        self.sustain = if sustain.is_finite() {
            sustain.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.attack_step = step_for(attack_seconds, sample_rate);
        self.decay_step = step_for(decay_seconds, sample_rate);
        self.release_step = step_for(release_seconds, sample_rate);
    }

    /// Starts (or restarts) the envelope.
    ///
    /// Retriggering continues from the current level instead of jumping to
    /// zero, so a fast repeated note does not click.
    pub fn gate_on(&mut self) {
        self.stage = AdsrStage::Attack;
    }

    /// Begins the release segment. Ignored when already idle.
    pub fn gate_off(&mut self) {
        if self.stage != AdsrStage::Idle {
            self.stage = AdsrStage::Release;
        }
    }

    /// Silences the envelope immediately.
    pub fn reset(&mut self) {
        self.stage = AdsrStage::Idle;
        self.level = 0.0;
    }

    /// Whether the envelope is producing anything.
    ///
    /// A voice can be freed when this turns false — the check a synth needs to
    /// avoid running silent voices forever.
    pub fn is_active(&self) -> bool {
        self.stage != AdsrStage::Idle
    }

    pub fn stage(&self) -> AdsrStage {
        self.stage
    }

    pub fn level(&self) -> f32 {
        self.level
    }

    /// Produces the next envelope value.
    pub fn next(&mut self) -> f32 {
        match self.stage {
            AdsrStage::Idle => self.level = 0.0,
            AdsrStage::Attack => {
                self.level += self.attack_step;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = AdsrStage::Decay;
                }
            }
            AdsrStage::Decay => {
                self.level -= self.decay_step;
                if self.level <= self.sustain {
                    self.level = self.sustain;
                    self.stage = AdsrStage::Sustain;
                }
            }
            AdsrStage::Sustain => self.level = self.sustain,
            AdsrStage::Release => {
                self.level -= self.release_step;
                if self.level <= 0.0 {
                    self.level = 0.0;
                    self.stage = AdsrStage::Idle;
                }
            }
        }
        self.level
    }
}

/// Per-sample increment covering the full range in `seconds`.
fn step_for(seconds: f32, sample_rate: f64) -> f32 {
    if !seconds.is_finite() || seconds <= 0.0 || !sample_rate.is_finite() || sample_rate <= 0.0 {
        // Immediate: one sample covers the whole range.
        return 1.0;
    }
    let samples = f64::from(seconds) * sample_rate;
    if samples < 1.0 {
        1.0
    } else {
        (1.0 / samples) as f32
    }
}

/// Tracks the amplitude of a signal, with separate attack and release rates.
///
/// Useful for a compressor's detector or a meter's ballistics.
///
/// ```
/// # use sunmao_dsp::envelopes::EnvelopeFollower;
/// let mut follower = EnvelopeFollower::new();
/// follower.set_params(0.001, 0.100, 48_000.0);
/// for _ in 0..4_800 {
///     follower.process(1.0);
/// }
/// // Tracks up to the signal's magnitude.
/// assert!((follower.level() - 1.0).abs() < 0.05);
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct EnvelopeFollower {
    level: f32,
    attack_coefficient: f32,
    release_coefficient: f32,
}

impl EnvelopeFollower {
    pub fn new() -> Self {
        Self {
            level: 0.0,
            attack_coefficient: 1.0,
            release_coefficient: 1.0,
        }
    }

    /// Sets attack and release time constants in seconds.
    pub fn set_params(&mut self, attack_seconds: f32, release_seconds: f32, sample_rate: f64) {
        self.attack_coefficient = coefficient_for(attack_seconds, sample_rate);
        self.release_coefficient = coefficient_for(release_seconds, sample_rate);
    }

    pub fn reset(&mut self) {
        self.level = 0.0;
    }

    pub fn level(&self) -> f32 {
        self.level
    }

    /// Feeds one sample and returns the tracked level.
    pub fn process(&mut self, input: f32) -> f32 {
        let magnitude = if input.is_finite() { input.abs() } else { 0.0 };
        // Rising and falling use different rates: a detector must catch a
        // transient quickly but let go slowly, or it modulates the signal.
        let coefficient = if magnitude > self.level {
            self.attack_coefficient
        } else {
            self.release_coefficient
        };
        self.level = flush_denormal(self.level + coefficient * (magnitude - self.level));
        self.level
    }
}

/// One-pole coefficient for a time constant.
fn coefficient_for(seconds: f32, sample_rate: f64) -> f32 {
    if !seconds.is_finite() || seconds <= 0.0 || !sample_rate.is_finite() || sample_rate <= 0.0 {
        return 1.0;
    }
    let samples = f64::from(seconds) * sample_rate;
    if samples < 1.0 {
        return 1.0;
    }
    (1.0 - (-1.0 / samples).exp()) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_envelope_walks_through_its_stages_and_ends_idle() {
        let mut env = Adsr::new();
        env.set_params(0.01, 0.01, 0.5, 0.01, 48_000.0);
        env.gate_on();
        assert_eq!(env.stage(), AdsrStage::Attack);

        let mut peak = 0.0f32;
        for _ in 0..480 {
            peak = peak.max(env.next());
        }
        assert!(
            (peak - 1.0).abs() < 1.0e-3,
            "attack did not reach 1: {peak}"
        );

        for _ in 0..480 {
            env.next();
        }
        assert_eq!(env.stage(), AdsrStage::Sustain);
        assert!((env.level() - 0.5).abs() < 1.0e-3);

        env.gate_off();
        for _ in 0..480 {
            env.next();
        }
        assert_eq!(env.stage(), AdsrStage::Idle);
        assert_eq!(env.level(), 0.0);
        assert!(!env.is_active());
    }

    #[test]
    fn attack_time_is_roughly_what_was_asked_for() {
        let mut env = Adsr::new();
        env.set_params(0.1, 0.001, 1.0, 0.001, 48_000.0);
        env.gate_on();
        let mut samples = 0;
        while env.stage() == AdsrStage::Attack && samples < 48_000 {
            env.next();
            samples += 1;
        }
        // 100 ms at 48 kHz is 4800 samples.
        assert!(
            (4_700..=4_900).contains(&samples),
            "attack took {samples} samples"
        );
    }

    #[test]
    fn retriggering_continues_from_the_current_level() {
        // Jumping to zero on retrigger is an audible click on fast repeats.
        let mut env = Adsr::new();
        env.set_params(0.05, 0.05, 0.5, 0.05, 48_000.0);
        env.gate_on();
        for _ in 0..1_000 {
            env.next();
        }
        env.gate_off();
        for _ in 0..100 {
            env.next();
        }
        let before = env.level();
        assert!(before > 0.0);

        env.gate_on();
        let after = env.next();
        assert!(
            (after - before).abs() < 0.01,
            "retrigger jumped from {before} to {after}"
        );
    }

    #[test]
    fn zero_times_are_immediate_rather_than_undefined() {
        let mut env = Adsr::new();
        env.set_params(0.0, 0.0, 0.5, 0.0, 48_000.0);
        env.gate_on();
        assert!(env.next().is_finite());
        for _ in 0..8 {
            assert!(env.next().is_finite());
        }
        assert!((env.level() - 0.5).abs() < 1.0e-6);
        env.gate_off();
        env.next();
        assert_eq!(env.stage(), AdsrStage::Idle);
    }

    #[test]
    fn non_finite_parameters_do_not_break_the_envelope() {
        let mut env = Adsr::new();
        env.set_params(f32::NAN, f32::INFINITY, f32::NAN, -1.0, 48_000.0);
        env.gate_on();
        for _ in 0..256 {
            assert!(env.next().is_finite());
        }
    }

    #[test]
    fn the_follower_attacks_faster_than_it_releases() {
        let mut follower = EnvelopeFollower::new();
        follower.set_params(0.001, 0.100, 48_000.0);

        // Rise.
        let mut rise_samples = 0;
        while follower.level() < 0.9 && rise_samples < 48_000 {
            follower.process(1.0);
            rise_samples += 1;
        }
        // Fall.
        let mut fall_samples = 0;
        while follower.level() > 0.1 && fall_samples < 480_000 {
            follower.process(0.0);
            fall_samples += 1;
        }
        assert!(
            fall_samples > rise_samples * 5,
            "release ({fall_samples}) should be much slower than attack ({rise_samples})"
        );
    }

    #[test]
    fn the_follower_ignores_polarity_and_non_finite_input() {
        let mut follower = EnvelopeFollower::new();
        follower.set_params(0.0005, 0.0005, 48_000.0);
        for _ in 0..4_800 {
            follower.process(-1.0);
        }
        assert!(
            (follower.level() - 1.0).abs() < 0.05,
            "magnitude not tracked: {}",
            follower.level()
        );
        follower.process(f32::NAN);
        assert!(follower.level().is_finite());
    }
}
