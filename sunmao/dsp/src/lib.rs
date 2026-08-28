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
pub mod oscillators;

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
#[inline]
pub fn flush_denormal(value: f32) -> f32 {
    if value.abs() < 1.0e-20 { 0.0 } else { value }
}

/// [`flush_denormal`] for `f64` state.
#[inline]
pub fn flush_denormal_f64(value: f64) -> f64 {
    if value.abs() < 1.0e-20 { 0.0 } else { value }
}

/// Common imports for using this crate.
pub mod prelude {
    pub use crate::envelopes::{Adsr, AdsrStage, EnvelopeFollower};
    pub use crate::filters::{Biquad, BiquadKind, OnePole, OnePoleKind, Svf, SvfOutput};
    pub use crate::flush_denormal;
    pub use crate::oscillators::{Oscillator, Waveform};
}
