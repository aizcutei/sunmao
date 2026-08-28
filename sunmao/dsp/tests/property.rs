//! Property tests for the DSP components.
//!
//! Filters are where a plausible-looking implementation quietly fails: a
//! cutoff a host sweeps past Nyquist, a resonance at its limit, a burst of
//! full-scale input. These sweep the parameter space rather than checking the
//! few points a unit test picks.

use proptest::prelude::*;
use sunmao_dsp::envelopes::{Adsr, AdsrStage, EnvelopeFollower};
use sunmao_dsp::filters::{Biquad, BiquadKind, OnePole, OnePoleKind, Svf};
use sunmao_dsp::oscillators::{Oscillator, Waveform};

/// Sample rates a host may actually use.
fn sample_rates() -> impl Strategy<Value = f64> {
    prop::sample::select(vec![
        8_000.0f64, 22_050.0, 44_100.0, 48_000.0, 96_000.0, 192_000.0,
    ])
}

proptest! {
    /// No parameter combination may produce a non-finite sample.
    ///
    /// This is the invariant that matters most: one NaN lodges in the filter
    /// state and every later sample is NaN, so the plugin outputs silence or
    /// noise for the rest of the session.
    #[test]
    fn no_filter_ever_produces_a_non_finite_sample(
        cutoff in -1_000.0f64..200_000.0,
        resonance in -1.0f32..2.0,
        q in -1.0f32..100.0,
        sample_rate in sample_rates(),
        amplitude in 0.0f32..8.0,
    ) {
        let mut svf = Svf::new();
        svf.set_params(cutoff, resonance, sample_rate);
        let mut low = OnePole::new(OnePoleKind::Lowpass);
        low.set_cutoff(cutoff, sample_rate);
        let mut high = OnePole::new(OnePoleKind::Highpass);
        high.set_cutoff(cutoff, sample_rate);
        let mut biquads = [Biquad::new(), Biquad::new(), Biquad::new()];
        for (filter, kind) in biquads.iter_mut().zip([
            BiquadKind::Lowpass,
            BiquadKind::Highpass,
            BiquadKind::Bandpass,
        ]) {
            filter.set_params(kind, cutoff, q, sample_rate);
        }

        // Alternating full-scale input is the worst case for a resonant
        // filter: maximum energy at Nyquist.
        for index in 0..2_048 {
            let input = if index % 2 == 0 { amplitude } else { -amplitude };
            let out = svf.tick(input);
            prop_assert!(out.lowpass.is_finite(), "svf lowpass");
            prop_assert!(out.bandpass.is_finite(), "svf bandpass");
            prop_assert!(out.highpass.is_finite(), "svf highpass");
            prop_assert!(low.process(input).is_finite(), "one-pole lowpass");
            prop_assert!(high.process(input).is_finite(), "one-pole highpass");
            for filter in biquads.iter_mut() {
                prop_assert!(filter.process(input).is_finite(), "biquad");
            }
        }
    }

    /// A filter fed silence must decay to inaudibility and never sit on a
    /// denormal, where arithmetic is far slower on many CPUs.
    ///
    /// Deliberately *not* "reaches exactly zero": a second-order recursion can
    /// hover just above any flush floor for a long time, and demanding exact
    /// zero would only invite tuning the floor upward until the test passes.
    /// What matters is that the residue is inaudible and normal-ranged.
    #[test]
    fn every_filter_settles_below_audibility_and_out_of_the_denormal_range(
        cutoff in 20.0f64..20_000.0,
        resonance in 0.0f32..1.0,
        sample_rate in sample_rates(),
    ) {
        let mut svf = Svf::new();
        svf.set_params(cutoff, resonance, sample_rate);
        let mut one_pole = OnePole::new(OnePoleKind::Lowpass);
        one_pole.set_cutoff(cutoff, sample_rate);
        let mut biquad = Biquad::new();
        biquad.set_params(BiquadKind::Lowpass, cutoff, 0.707, sample_rate);

        for _ in 0..512 {
            svf.tick(1.0);
            one_pole.process(1.0);
            biquad.process(1.0);
        }
        // Long enough for any decay at these cutoffs to reach the flush floor.
        for _ in 0..400_000 {
            svf.tick(0.0);
            one_pole.process(0.0);
            biquad.process(0.0);
        }
        // f32 denormals live below ~1.18e-38; anything below -360 dBFS is
        // inaudible by any measure.
        const INAUDIBLE: f32 = 1.0e-18;
        const SMALLEST_NORMAL: f32 = f32::MIN_POSITIVE;
        for (label, value) in [
            ("svf", svf.tick(0.0).lowpass),
            ("one pole", one_pole.process(0.0)),
            ("biquad", biquad.process(0.0)),
        ] {
            prop_assert!(
                value.abs() < INAUDIBLE,
                "{label} left an audible residue: {value:e}"
            );
            prop_assert!(
                value == 0.0 || value.abs() >= SMALLEST_NORMAL,
                "{label} settled on a denormal: {value:e}"
            );
        }
    }

    /// A lowpass must not amplify DC, whatever its cutoff.
    ///
    /// Unity DC gain is what makes a filter safe to drop into a signal path;
    /// a coefficient slip shows up here as gain that grows with cutoff.
    #[test]
    fn a_lowpass_has_unity_gain_at_dc(
        cutoff in 20.0f64..20_000.0,
        sample_rate in sample_rates(),
    ) {
        let mut one_pole = OnePole::new(OnePoleKind::Lowpass);
        one_pole.set_cutoff(cutoff, sample_rate);
        let mut biquad = Biquad::new();
        biquad.set_params(BiquadKind::Lowpass, cutoff, 0.707, sample_rate);
        let mut svf = Svf::new();
        svf.set_params(cutoff, 0.0, sample_rate);

        let mut one_pole_out = 0.0;
        let mut biquad_out = 0.0;
        let mut svf_out = 0.0;
        for _ in 0..200_000 {
            one_pole_out = one_pole.process(1.0);
            biquad_out = biquad.process(1.0);
            svf_out = svf.tick(1.0).lowpass;
        }
        prop_assert!((one_pole_out - 1.0).abs() < 1.0e-2, "one pole {one_pole_out}");
        prop_assert!((biquad_out - 1.0).abs() < 1.0e-2, "biquad {biquad_out}");
        prop_assert!((svf_out - 1.0).abs() < 1.0e-2, "svf {svf_out}");
    }

    /// Resetting a filter must make it behave as if freshly built, so a host
    /// stopping and restarting transport cannot leak the previous tail.
    #[test]
    fn reset_makes_a_filter_indistinguishable_from_new(
        cutoff in 20.0f64..20_000.0,
        resonance in 0.0f32..1.0,
        sample_rate in sample_rates(),
    ) {
        let mut used = Svf::new();
        used.set_params(cutoff, resonance, sample_rate);
        for index in 0..1_000 {
            used.tick(if index % 3 == 0 { 1.0 } else { -0.5 });
        }
        used.reset();

        let mut fresh = Svf::new();
        fresh.set_params(cutoff, resonance, sample_rate);

        for index in 0..256 {
            let input = (index as f32 * 0.01).sin();
            prop_assert_eq!(used.tick(input).lowpass, fresh.tick(input).lowpass);
        }
    }

    /// No oscillator setting may produce a non-finite or runaway sample.
    ///
    /// PolyBLEP divides by the phase increment, so a frequency of zero or one
    /// pushed past Nyquist is exactly where a naive implementation produces
    /// infinities.
    #[test]
    fn no_oscillator_setting_produces_a_bad_sample(
        frequency in -1_000.0f64..100_000.0,
        pulse_width in -1.0f32..2.0,
        sample_rate in sample_rates(),
        waveform_index in 0usize..3,
    ) {
        let waveform = [Waveform::Sine, Waveform::Saw, Waveform::Pulse][waveform_index];
        let mut osc = Oscillator::new(waveform);
        osc.set_frequency(frequency, sample_rate);
        osc.set_pulse_width(pulse_width);
        for _ in 0..4_096 {
            let value = osc.next();
            prop_assert!(value.is_finite(), "{waveform:?} produced {value}");
            // PolyBLEP overshoots a little; anything beyond this is a bug.
            prop_assert!(value.abs() <= 2.0, "{waveform:?} produced {value}");
        }
    }

    /// An ADSR must stay in `0.0..=1.0` and always terminate after gate-off.
    ///
    /// A voice is freed when the envelope goes idle, so an envelope that never
    /// arrives leaks a voice for the lifetime of the session.
    #[test]
    fn an_adsr_stays_bounded_and_always_finishes(
        attack in 0.0f32..0.5,
        decay in 0.0f32..0.5,
        sustain in -0.5f32..1.5,
        release in 0.0f32..0.5,
        sample_rate in sample_rates(),
    ) {
        let mut env = Adsr::new();
        env.set_params(attack, decay, sustain, release, sample_rate);
        env.gate_on();

        // Hold long enough to pass attack and decay at the longest settings.
        let hold = (sample_rate * 1.5) as usize;
        for _ in 0..hold {
            let value = env.next();
            prop_assert!(value.is_finite(), "produced {value}");
            prop_assert!((0.0..=1.0).contains(&value), "escaped 0..=1: {value}");
        }

        env.gate_off();
        let limit = (sample_rate * 1.5) as usize;
        let mut finished = false;
        for _ in 0..limit {
            let value = env.next();
            prop_assert!((0.0..=1.0).contains(&value), "escaped 0..=1: {value}");
            if !env.is_active() {
                finished = true;
                break;
            }
        }
        prop_assert!(finished, "envelope never went idle, leaking the voice");
        prop_assert_eq!(env.stage(), AdsrStage::Idle);
        prop_assert_eq!(env.level(), 0.0);
    }

    /// A follower's level never exceeds the largest magnitude it has seen, and
    /// never goes negative — it tracks amplitude, not signal.
    #[test]
    fn a_follower_tracks_magnitude_within_the_signal_it_saw(
        attack in 0.0f32..0.1,
        release in 0.0f32..0.5,
        amplitude in 0.0f32..4.0,
        sample_rate in sample_rates(),
    ) {
        let mut follower = EnvelopeFollower::new();
        follower.set_params(attack, release, sample_rate);
        for index in 0..8_192 {
            // Alternating polarity: a follower keyed on the raw signal rather
            // than its magnitude would oscillate instead of settling.
            let input = if index % 2 == 0 { amplitude } else { -amplitude };
            let level = follower.process(input);
            prop_assert!(level.is_finite(), "produced {level}");
            prop_assert!(level >= 0.0, "went negative: {level}");
            prop_assert!(
                level <= amplitude + 1.0e-3,
                "exceeded the input magnitude: {level} > {amplitude}"
            );
        }
    }
}
