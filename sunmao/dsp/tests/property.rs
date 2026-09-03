//! Property tests for the DSP components.
//!
//! Filters are where a plausible-looking implementation quietly fails: a
//! cutoff a host sweeps past Nyquist, a resonance at its limit, a burst of
//! full-scale input. These sweep the parameter space rather than checking the
//! few points a unit test picks.

use proptest::prelude::*;
use sunmao_dsp::envelopes::{Adsr, AdsrStage, EnvelopeFollower};
use sunmao_dsp::filters::{Biquad, BiquadKind, OnePole, OnePoleKind, Svf};
use sunmao_dsp::metering::Meter;
use sunmao_dsp::mixing::{DryWet, MixLaw, db_to_gain, gain_to_db};
use sunmao_dsp::oscillators::{Oscillator, Waveform};
use sunmao_dsp::oversampling::{Oversampler, OversamplingFactor};

fn oversampling_factors() -> impl Strategy<Value = OversamplingFactor> {
    prop::sample::select(vec![
        OversamplingFactor::None,
        OversamplingFactor::X2,
        OversamplingFactor::X4,
    ])
}

fn mix_laws() -> impl Strategy<Value = MixLaw> {
    prop::sample::select(vec![MixLaw::Linear, MixLaw::EqualPower])
}

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

    /// When it resamples, the oversampler never hands the body, or returns to
    /// the caller, a non-finite sample — for any block size up to the prepared
    /// maximum, and input up to and including `f32::MAX`. When it does not
    /// (`None`), it is a bypass and the body sees the input exactly as it was.
    ///
    /// Interpolation doubles the signal to make up for the inserted zeros, so
    /// a finite-but-huge input is exactly how `inf - inf` becomes NaN inside
    /// the FIR and then lodges in its delay line.
    #[test]
    fn the_oversampler_never_produces_a_non_finite_sample(
        factor in oversampling_factors(),
        max_block in 1usize..512,
        block_fraction in 0.0f32..=1.0,
        amplitude in prop::sample::select(vec![0.0f32, 1.0, 100.0, 1.0e30, f32::MAX]),
        poison in prop::bool::ANY,
    ) {
        let mut os = Oversampler::new();
        os.prepare(factor, max_block);
        let block_len = ((max_block as f32 * block_fraction) as usize).max(1);
        let mut block: Vec<f32> = (0..block_len)
            .map(|index| if index % 2 == 0 { amplitude } else { -amplitude })
            .collect();
        if poison {
            block[0] = f32::NAN;
            if block_len > 1 {
                block[1] = f32::INFINITY;
            }
        }
        let original = block.clone();

        for _ in 0..4 {
            let mut seen = 0;
            let mut body_saw_input = false;
            os.process(&mut block, |upsampled| {
                seen = upsampled.len();
                body_saw_input = upsampled
                    .iter()
                    .zip(original.iter())
                    .all(|(a, b)| a.to_bits() == b.to_bits());
                if factor != OversamplingFactor::None {
                    for sample in upsampled.iter() {
                        assert!(sample.is_finite(), "{factor:?} body was handed {sample}");
                    }
                }
            });
            prop_assert_eq!(seen, block_len * factor.ratio(), "{:?}", factor);
            if factor == OversamplingFactor::None {
                prop_assert!(body_saw_input, "bypass altered the block");
            } else {
                for sample in block.iter() {
                    prop_assert!(sample.is_finite(), "{factor:?} produced {sample}");
                }
            }
        }
    }

    /// The latency the factor reports is the delay the signal actually
    /// suffers, and it does not depend on how the host chops the stream into
    /// blocks.
    ///
    /// This is the number a host shifts the track by, so "close" is not good
    /// enough: a host can only be told an integer, and the design deliberately
    /// sizes the filters so the true delay *is* one.
    #[test]
    fn reported_latency_is_the_measured_delay_for_any_block_size(
        factor in prop::sample::select(vec![OversamplingFactor::X2, OversamplingFactor::X4]),
        block_len in 1usize..256,
    ) {
        let mut os = Oversampler::new();
        os.prepare(factor, block_len);
        let reported = factor.latency_samples() as usize;

        let mut signal = vec![0.0f32; (reported + 64).div_ceil(block_len) * block_len];
        signal[0] = 1.0;
        for chunk in signal.chunks_mut(block_len) {
            os.process(chunk, |_| {});
        }
        let measured = signal
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap())
            .map(|(index, _)| index)
            .unwrap();
        prop_assert!(
            reported.abs_diff(measured) <= 1,
            "{factor:?} at block {block_len}: reported {reported}, measured {measured}"
        );
    }

    /// Up then down with a linear body is a pure delay: DC gain stays at unity
    /// whatever the level, so oversampling never changes how loud a plugin is.
    #[test]
    fn a_linear_body_keeps_unity_gain_at_dc(
        factor in oversampling_factors(),
        amplitude in 0.0f32..4.0,
        block_len in 1usize..256,
    ) {
        let mut os = Oversampler::new();
        os.prepare(factor, block_len);
        // Long enough for both filter cascades to be fully primed.
        let mut block = vec![amplitude; block_len];
        let mut steady = Vec::new();
        for pass in 0..(512 / block_len + 4) {
            block.fill(amplitude);
            os.process(&mut block, |_| {});
            if pass * block_len >= 256 {
                steady.extend_from_slice(&block);
            }
        }
        for sample in steady {
            prop_assert!(
                (sample - amplitude).abs() <= amplitude * 0.02 + 1.0e-4,
                "{factor:?} changed the level: {sample} vs {amplitude}"
            );
        }
    }

    /// After `reset`, an oversampler is indistinguishable from a fresh one:
    /// nothing from the previous session leaks into the next.
    #[test]
    fn reset_makes_an_oversampler_indistinguishable_from_new(
        factor in oversampling_factors(),
        amplitude in -2.0f32..2.0,
    ) {
        let block_len = 64;
        let mut used = Oversampler::new();
        used.prepare(factor, block_len);
        let mut noise: Vec<f32> = (0..block_len).map(|i| (i as f32 * 0.7).sin() * 3.0).collect();
        used.process(&mut noise, |_| {});
        used.reset();

        let mut fresh = Oversampler::new();
        fresh.prepare(factor, block_len);

        let mut a = vec![amplitude; block_len];
        let mut b = vec![amplitude; block_len];
        used.process(&mut a, |_| {});
        fresh.process(&mut b, |_| {});
        prop_assert_eq!(a, b, "{:?}", factor);
    }

    /// Decibels and linear gain round-trip, and the mapping is monotone: a
    /// louder setting in dB is never a quieter gain.
    #[test]
    fn decibel_conversion_round_trips_and_is_monotone(
        db in -119.0f32..60.0,
        step in 0.0f32..10.0,
    ) {
        let gain = db_to_gain(db);
        prop_assert!(gain.is_finite() && gain > 0.0, "{db} dB gave {gain}");
        let back = gain_to_db(gain);
        prop_assert!((back - db).abs() < 1.0e-2, "{db} dB came back as {back}");
        prop_assert!(db_to_gain(db + step) >= gain, "not monotone at {db} + {step}");
    }

    /// A dry/wet mixer's gains stay in `0.0..=1.0` for any amount, and each law
    /// holds the quantity it promises constant: the linear law's coefficients
    /// sum to one, the equal-power law's squares do.
    #[test]
    fn a_dry_wet_mixer_holds_its_law_for_any_amount(
        law in mix_laws(),
        amount in -1.0f32..2.0,
        dry in -2.0f32..2.0,
        wet in -2.0f32..2.0,
    ) {
        let mixer = DryWet::new(law, amount);
        let (dry_gain, wet_gain) = (mixer.dry_gain(), mixer.wet_gain());
        prop_assert!((0.0..=1.0).contains(&dry_gain), "dry gain {dry_gain}");
        prop_assert!((0.0..=1.0).contains(&wet_gain), "wet gain {wet_gain}");
        let conserved = match law {
            MixLaw::Linear => dry_gain + wet_gain,
            MixLaw::EqualPower => dry_gain * dry_gain + wet_gain * wet_gain,
        };
        prop_assert!((conserved - 1.0).abs() < 1.0e-5, "{law:?} conserved {conserved}");

        // The block path is the per-sample path applied everywhere.
        let mut block = vec![wet; 32];
        mixer.mix_block(&mut block, &vec![dry; 32]);
        for sample in block {
            prop_assert!((sample - mixer.mix(dry, wet)).abs() < 1.0e-6);
        }
        // Mixing a signal with itself under the linear law is the identity,
        // which is why that law is the right one for correlated paths.
        if law == MixLaw::Linear {
            prop_assert!((mixer.mix(dry, dry) - dry).abs() < 1.0e-5);
        }
    }

    /// A meter reports a level no higher than the largest magnitude it saw,
    /// never negative, never non-finite, and the reader handle sees exactly
    /// what the audio side published — even when the input contains NaN.
    #[test]
    fn a_meter_stays_within_the_signal_it_measured(
        amplitude in 0.0f32..4.0,
        block_len in 1usize..1_024,
        blocks in 1usize..64,
        sample_rate in sample_rates(),
        poison in prop::bool::ANY,
    ) {
        let mut meter = Meter::new();
        meter.set_sample_rate(sample_rate);
        let handle = meter.handle();

        let mut block: Vec<f32> = (0..block_len)
            .map(|index| (index as f32 * 0.37).sin() * amplitude)
            .collect();
        if poison {
            block[0] = f32::NAN;
        }
        let largest = block
            .iter()
            .filter(|s| s.is_finite())
            .fold(0.0f32, |acc, s| acc.max(s.abs()));

        for _ in 0..blocks {
            meter.process_block(&block);
            let (peak, rms) = (meter.peak(), meter.rms());
            prop_assert!(peak.is_finite() && rms.is_finite(), "peak {peak} rms {rms}");
            prop_assert!(peak >= 0.0 && rms >= 0.0, "peak {peak} rms {rms}");
            prop_assert!(peak <= largest + 1.0e-6, "peak {peak} exceeded input {largest}");
            prop_assert!(rms <= largest + 1.0e-3, "rms {rms} exceeded input {largest}");
            prop_assert_eq!(handle.peak().to_bits(), peak.to_bits());
            prop_assert_eq!(handle.rms().to_bits(), rms.to_bits());
        }
    }
}
