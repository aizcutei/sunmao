//! Allocation-free DSP building blocks for SunMao plugins.
//!
//! Every component here is designed for the audio callback: fixed size, `Copy`
//! or plain-old-data, and free of allocation, locking, and panics on the
//! processing path. Coefficient updates are separated from per-sample
//! processing so the expensive trigonometry happens when a parameter changes
//! rather than once per sample.
//!
//! Components clamp their own parameters instead of trusting the caller. A host
//! can automate a cutoff to anything, and a filter that produces NaN once
//! poisons its state forever — so out-of-range input is bounded at the edge
//! rather than propagated.

pub mod envelopes;
pub mod filters;
pub mod metering;
pub mod mixing;
pub mod oscillators;
pub mod oversampling;

/// The magnitude at and below which decayed state is snapped to zero.
///
/// Exposed as a constant because it is part of this crate's numeric contract
/// (`docs/phase3/compatibility.md` §1.3): components' settling time depends on
/// it, so it is pinned rather than sprinkled through the implementations. Use
/// it when writing a component that has to agree with the ones here.
///
/// A component with more than one state variable must test its states
/// *together* — zeroing one while the others are still live breaks whatever
/// coupling makes them decay:
///
/// ```
/// # use sunmao_dsp::DENORMAL_FLOOR;
/// let (mut a, mut b) = (3.0e-21f32, 5.0e-19f32);
/// if a.abs() < DENORMAL_FLOOR && b.abs() < DENORMAL_FLOOR {
///     a = 0.0;
///     b = 0.0;
/// }
/// // `a` alone is under the floor, but the pair is not, so neither is snapped.
/// assert_eq!((a, b), (3.0e-21, 5.0e-19));
/// ```
pub const DENORMAL_FLOOR: f32 = 1.0e-20;

/// Snaps values that have decayed to inaudibility down to zero.
///
/// A filter fed silence decays asymptotically, and on many CPUs arithmetic on
/// denormals is dramatically slower than on normal floats — a plugin can start
/// costing far more when its input goes quiet, which is exactly backwards.
/// Flushing costs one comparison per state variable.
///
/// The threshold sits well above f32's denormal range (~1.18e-38) rather than
/// just above it. Merely avoiding denormals would let a resonant filter at a
/// low cutoff spend hundreds of thousands of samples creeping through the
/// normal-but-pointless range first; a low-cutoff SVF was measured still at
/// 1.1e-28 after 400k samples of silence. At 1e-20 — about -400 dBFS, far
/// below anything representable in a real signal path — the state reaches zero
/// promptly instead.
///
/// Use this only for a **single** state variable. Applying it independently to
/// each state of a coupled recursion is worse than not flushing at all: see
/// [`DENORMAL_FLOOR`] for the pairing rule, and [`filters::Svf`] and
/// [`filters::Biquad`] for what it cost when they got it wrong.
#[inline]
pub fn flush_denormal(value: f32) -> f32 {
    if value.abs() < DENORMAL_FLOOR {
        0.0
    } else {
        value
    }
}

/// [`flush_denormal`] for `f64` state.
#[inline]
pub fn flush_denormal_f64(value: f64) -> f64 {
    if value.abs() < f64::from(DENORMAL_FLOOR) {
        0.0
    } else {
        value
    }
}

/// Common imports for using this crate.
pub mod prelude {
    pub use crate::envelopes::{Adsr, AdsrStage, EnvelopeFollower};
    pub use crate::filters::{Biquad, BiquadKind, OnePole, OnePoleKind, Svf, SvfOutput};
    pub use crate::metering::{Meter, MeterHandle};
    pub use crate::mixing::{DryWet, MixLaw, apply_gain, db_to_gain, gain_to_db};
    pub use crate::oscillators::{Oscillator, Waveform};
    pub use crate::oversampling::{Oversampler, OversamplingFactor};
    pub use crate::{DENORMAL_FLOOR, flush_denormal};
}
