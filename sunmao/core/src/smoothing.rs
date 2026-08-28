//! Per-sample parameter smoothing.
//!
//! A host can jump a parameter anywhere between two blocks, and applying that
//! jump directly to a gain or a filter coefficient is audible as a click.
//! A [`Smoother`] turns the jump into a ramp.
//!
//! Everything here is fixed-size and allocation-free: [`Smoother::next`] is
//! called once per sample on the audio thread, so it does no work beyond
//! arithmetic and never touches the allocator.
//!
//! # Where smoothing sits relative to automation and modulation
//!
//! Automation is the parameter's *value*, so it is what gets smoothed: feed
//! each automation change to [`Smoother::set_target`] and read
//! [`Smoother::next`] per sample.
//!
//! Modulation is a separate, additive offset that must not enter saved state
//! (see `docs/phase2/semantics.md`). Add it *after* smoothing rather than
//! folding it into the target, so it stays instantaneous and never becomes part
//! of the smoothed value the plugin would persist.

/// How a [`Smoother`] travels from its current value to its target.
///
/// ```
/// # use sunmao_core::smoothing::SmoothingStyle;
/// // A 20 ms ramp is a typical click-free gain change.
/// let style = SmoothingStyle::Linear(0.02);
/// assert_eq!(style.seconds(), 0.02);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SmoothingStyle {
    /// Constant rate: reaches the target after exactly this many seconds.
    Linear(f32),
    /// Exponential approach with this time constant: fast at first, then
    /// slower. Natural for gain and cutoff, and it never overshoots.
    Exponential(f32),
}

impl SmoothingStyle {
    /// The configured duration or time constant, in seconds.
    pub fn seconds(self) -> f32 {
        match self {
            Self::Linear(seconds) | Self::Exponential(seconds) => seconds,
        }
    }
}

/// Below this distance the smoother snaps to its target.
///
/// An exponential approach never mathematically arrives, so without a floor it
/// would smooth forever and drive values into the denormal range, where some
/// CPUs run orders of magnitude slower. This threshold is far below audibility
/// for gains and coefficients alike.
///
/// It is *not* what guarantees termination — see the non-progress check in
/// [`Smoother::next`]. An epsilon alone cannot: the distance at which f32 stops
/// making progress scales with the target's magnitude, so any fixed value is
/// either too small for large targets or audibly coarse for small ones.
const SNAP_EPSILON: f32 = 1.0e-6;

/// Time constants after which an exponential ramp snaps to its target.
///
/// An exponential approach is asymptotic, so "arrival" has to be defined. After
/// this many time constants the remaining distance is `e^-12`, about six parts
/// per million of the original jump — inaudible for gains and coefficients
/// alike. Bounding it this way means smoothing lasts a predictable multiple of
/// the configured time instead of however long the value takes to fall below
/// some absolute threshold, which for a large jump can be tens of seconds.
const EXPONENTIAL_TIME_CONSTANTS: u32 = 12;

/// Ramps a value towards a target, one sample at a time.
///
/// ```
/// # use sunmao_core::smoothing::{Smoother, SmoothingStyle};
/// // 4 samples of linear ramp from 0 to 1.
/// let mut smoother = Smoother::new(SmoothingStyle::Linear(4.0));
/// smoother.set_sample_rate(1.0);
/// smoother.reset(0.0);
/// smoother.set_target(1.0);
///
/// assert!(smoother.is_smoothing());
/// let ramp: Vec<f32> = (0..4).map(|_| smoother.next()).collect();
/// assert_eq!(ramp, vec![0.25, 0.5, 0.75, 1.0]);
///
/// // Arrived: further samples hold the target and cost nothing.
/// assert!(!smoother.is_smoothing());
/// assert_eq!(smoother.next(), 1.0);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Smoother {
    style: SmoothingStyle,
    current: f32,
    target: f32,
    /// Linear: increment per sample. Unused when exponential.
    step: f32,
    /// Exponential: fraction of the remaining distance kept each sample.
    decay: f32,
    /// Linear: samples left before the target is reached exactly.
    remaining: u32,
    /// Samples the configured duration corresponds to at the current rate.
    duration_samples: u32,
}

impl Smoother {
    /// Creates a smoother. Call [`Smoother::set_sample_rate`] before use.
    pub fn new(style: SmoothingStyle) -> Self {
        Self {
            style,
            current: 0.0,
            target: 0.0,
            step: 0.0,
            decay: 0.0,
            remaining: 0,
            duration_samples: 0,
        }
    }

    /// Recomputes the ramp for a new sample rate.
    ///
    /// Call this from `initialize`, not from the audio callback: it is the only
    /// method that divides, and the rate cannot change mid-block anyway.
    /// A non-finite or non-positive rate leaves the smoother in a
    /// pass-through state rather than producing NaNs downstream.
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        let seconds = self.style.seconds();
        if !sample_rate.is_finite() || sample_rate <= 0.0 || !seconds.is_finite() || seconds <= 0.0
        {
            self.duration_samples = 0;
            self.decay = 0.0;
            self.step = 0.0;
            self.remaining = 0;
            return;
        }

        let samples = (seconds as f64 * sample_rate).round();
        self.duration_samples = samples.clamp(0.0, u32::MAX as f64) as u32;
        self.decay = match self.style {
            SmoothingStyle::Linear(_) => 0.0,
            // One time constant per `seconds`, i.e. e^(-1/samples) per sample.
            SmoothingStyle::Exponential(_) => {
                if self.duration_samples == 0 {
                    0.0
                } else {
                    (-1.0f64 / self.duration_samples as f64).exp() as f32
                }
            }
        };
        // Re-arm towards the existing target under the new rate.
        self.set_target(self.target);
    }

    /// Jumps to `value` immediately, cancelling any ramp in progress.
    ///
    /// Use on `reset`/`initialize`, where a ramp from a stale value would be
    /// audible as a slide from the previous session's setting.
    pub fn reset(&mut self, value: f32) {
        let value = sanitize(value, 0.0);
        self.current = value;
        self.target = value;
        self.remaining = 0;
        self.step = 0.0;
    }

    /// Aims at a new target, starting from wherever the ramp currently is.
    ///
    /// Retargeting mid-ramp does not jump: a host sweeping a control produces a
    /// continuous line rather than a staircase.
    pub fn set_target(&mut self, target: f32) {
        let target = sanitize(target, self.target);
        self.target = target;
        let distance = target - self.current;
        if self.duration_samples == 0 || distance.abs() <= SNAP_EPSILON {
            self.current = target;
            self.remaining = 0;
            self.step = 0.0;
            return;
        }
        self.remaining = match self.style {
            SmoothingStyle::Linear(_) => self.duration_samples,
            SmoothingStyle::Exponential(_) => self
                .duration_samples
                .saturating_mul(EXPONENTIAL_TIME_CONSTANTS),
        };
        self.step = distance / self.duration_samples as f32;
    }

    /// Produces the next sample. Allocation-free and branch-light.
    pub fn next(&mut self) -> f32 {
        match self.style {
            SmoothingStyle::Linear(_) => {
                if self.remaining == 0 {
                    return self.current;
                }
                self.remaining -= 1;
                if self.remaining == 0 {
                    // Land exactly on the target rather than accumulating the
                    // step's rounding error over the ramp.
                    self.current = self.target;
                } else {
                    self.current += self.step;
                }
                self.current
            }
            SmoothingStyle::Exponential(_) => {
                if self.remaining == 0 {
                    return self.current;
                }
                self.remaining -= 1;
                let distance = self.target - self.current;
                let next = self.target - distance * self.decay;
                // Three ways to finish, all of which land exactly on the
                // target: the budget ran out, we are already close enough, or
                // f32 can no longer represent progress. The last one matters
                // because `target - small` rounds back to the same value once
                // the distance falls below the target's precision — a fixed
                // point that was observed to stall at a distance of ~1.4e-5
                // for a target of 1.0, well above any sane absolute epsilon.
                if self.remaining == 0 || distance.abs() <= SNAP_EPSILON || next == self.current {
                    self.current = self.target;
                    self.remaining = 0;
                } else {
                    self.current = next;
                }
                self.current
            }
        }
    }

    /// Whether a ramp is still in progress.
    ///
    /// False means the value has arrived *exactly*, so a plugin that stops
    /// calling [`Smoother::next`] on `false` is not left holding a value a
    /// fraction off target forever. The snap epsilon deliberately does not
    /// appear here: it decides when `next` jumps the last step, not whether the
    /// caller may stop.
    pub fn is_smoothing(&self) -> bool {
        self.remaining > 0
    }

    /// The value most recently produced.
    pub fn current(&self) -> f32 {
        self.current
    }

    /// The value being approached.
    pub fn target(&self) -> f32 {
        self.target
    }
}

/// Replaces a non-finite value with a fallback.
///
/// A host can send NaN, and letting it into the ramp would silently poison
/// every later sample: NaN propagates through both the linear and exponential
/// updates and never recovers.
fn sanitize(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_linear_ramp_lands_exactly_on_its_target() {
        let mut smoother = Smoother::new(SmoothingStyle::Linear(1.0));
        smoother.set_sample_rate(8.0);
        smoother.reset(0.0);
        smoother.set_target(1.0);

        let mut last = 0.0;
        for _ in 0..8 {
            last = smoother.next();
        }
        // Exactly, not approximately: the final sample assigns the target
        // rather than adding one more step.
        assert_eq!(last, 1.0);
        assert!(!smoother.is_smoothing());
    }

    #[test]
    fn an_exponential_ramp_approaches_without_overshooting() {
        let mut smoother = Smoother::new(SmoothingStyle::Exponential(0.01));
        smoother.set_sample_rate(48_000.0);
        smoother.reset(0.0);
        smoother.set_target(1.0);

        let mut previous = 0.0;
        for _ in 0..48_000 {
            let value = smoother.next();
            assert!(value >= previous, "must be monotonic: {value} < {previous}");
            assert!(value <= 1.0, "must never overshoot: {value}");
            previous = value;
        }
        // And it settles rather than smoothing forever.
        assert!(!smoother.is_smoothing());
        assert_eq!(smoother.current(), 1.0);
    }

    #[test]
    fn retargeting_mid_ramp_continues_from_the_current_value() {
        let mut smoother = Smoother::new(SmoothingStyle::Linear(1.0));
        smoother.set_sample_rate(10.0);
        smoother.reset(0.0);
        smoother.set_target(1.0);

        for _ in 0..5 {
            smoother.next();
        }
        let mid = smoother.current();
        assert!(mid > 0.0 && mid < 1.0);

        // A new target must not jump the output.
        smoother.set_target(0.0);
        let next = smoother.next();
        assert!(
            (next - mid).abs() < 0.2,
            "retarget jumped from {mid} to {next}"
        );
    }

    #[test]
    fn a_non_finite_target_is_ignored_rather_than_poisoning_the_ramp() {
        let mut smoother = Smoother::new(SmoothingStyle::Linear(0.01));
        smoother.set_sample_rate(48_000.0);
        smoother.reset(0.5);

        smoother.set_target(f32::NAN);
        assert!(smoother.target().is_finite());
        for _ in 0..16 {
            assert!(smoother.next().is_finite());
        }

        smoother.set_target(f32::INFINITY);
        for _ in 0..16 {
            assert!(smoother.next().is_finite());
        }
    }

    #[test]
    fn an_invalid_sample_rate_degrades_to_pass_through() {
        for rate in [0.0, -48_000.0, f64::NAN, f64::INFINITY] {
            let mut smoother = Smoother::new(SmoothingStyle::Linear(0.01));
            smoother.set_sample_rate(rate);
            smoother.reset(0.0);
            smoother.set_target(1.0);
            // No ramp is possible, so the value must arrive immediately rather
            // than stalling at zero or producing NaN.
            assert_eq!(smoother.next(), 1.0, "rate {rate}");
        }
    }

    #[test]
    fn reset_cancels_a_ramp_instead_of_sliding_from_the_old_value() {
        let mut smoother = Smoother::new(SmoothingStyle::Linear(1.0));
        smoother.set_sample_rate(100.0);
        smoother.reset(0.0);
        smoother.set_target(1.0);
        smoother.next();

        smoother.reset(0.25);
        assert!(!smoother.is_smoothing());
        assert_eq!(smoother.next(), 0.25);
    }

    #[test]
    fn the_smoother_holds_no_heap_state() {
        // `next()` cannot allocate because there is nothing heap-allocated to
        // grow: the whole smoother is scalars, which `Copy` also attests.
        // End-to-end zero-alloc through a real `process` call is asserted in
        // `sunmao_backend_clap`'s `smoothing_in_process_does_not_allocate`.
        fn assert_copy<T: Copy>() {}
        assert_copy::<Smoother>();
        // Small enough to sit inline in a plugin struct without indirection.
        assert!(std::mem::size_of::<Smoother>() <= 64);
    }
}
