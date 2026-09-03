//! Gain and dry/wet utilities.
//!
//! Small, but worth centralising: every plugin needs them, and the two easy
//! mistakes — treating decibels as linear, and losing level in the middle of a
//! crossfade — are the kind that only show up when someone listens.

/// Converts decibels to a linear gain multiplier.
///
/// ```
/// # use sunmao_dsp::mixing::db_to_gain;
/// assert!((db_to_gain(0.0) - 1.0).abs() < 1.0e-6);
/// assert!((db_to_gain(-6.0) - 0.501_187).abs() < 1.0e-4);
/// ```
#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    if !db.is_finite() {
        // -inf dB is a legitimate way to spell silence; +inf is not a
        // legitimate way to spell anything.
        return if db == f32::NEG_INFINITY { 0.0 } else { 1.0 };
    }
    // Below this, the difference from silence is not representable in any
    // realistic signal path, and the exponential is wasted work.
    if db <= -120.0 {
        return 0.0;
    }
    10.0f32.powf(db / 20.0)
}

/// Converts a linear gain multiplier to decibels.
///
/// Silence maps to `-inf`, which is what a meter should display rather than a
/// NaN from `log10(0)`.
#[inline]
pub fn gain_to_db(gain: f32) -> f32 {
    if !gain.is_finite() {
        return f32::NEG_INFINITY;
    }
    let magnitude = gain.abs();
    if magnitude <= 0.0 {
        return f32::NEG_INFINITY;
    }
    20.0 * magnitude.log10()
}

/// Applies a constant gain to a block.
#[inline]
pub fn apply_gain(block: &mut [f32], gain: f32) {
    if gain == 1.0 {
        return;
    }
    for sample in block.iter_mut() {
        *sample *= gain;
    }
}

/// How a [`DryWet`] crossfades between the two signals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixLaw {
    /// Coefficients sum to one. Correct when dry and wet are correlated — a
    /// filter, an EQ, anything that passes the same signal through.
    Linear,
    /// Coefficients' *squares* sum to one. Correct when dry and wet are
    /// uncorrelated — a reverb, a delay, a pitch shift — where linear mixing
    /// dips about 3 dB in the middle.
    EqualPower,
}

/// Blends a dry and a wet signal.
///
/// ```
/// # use sunmao_dsp::mixing::{DryWet, MixLaw};
/// let mixer = DryWet::new(MixLaw::Linear, 0.25);
/// // A quarter wet: mostly the original.
/// assert!((mixer.mix(1.0, 5.0) - 2.0).abs() < 1.0e-6);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct DryWet {
    law: MixLaw,
    dry_gain: f32,
    wet_gain: f32,
}

impl Default for DryWet {
    /// Fully wet with the linear law: the common default for an insert effect.
    fn default() -> Self {
        Self::new(MixLaw::Linear, 1.0)
    }
}

impl DryWet {
    pub fn new(law: MixLaw, amount: f32) -> Self {
        let mut mixer = Self {
            law,
            dry_gain: 0.0,
            wet_gain: 1.0,
        };
        mixer.set_amount(amount);
        mixer
    }

    /// Sets the wet proportion, `0.0` (dry) to `1.0` (wet).
    pub fn set_amount(&mut self, amount: f32) {
        let amount = if amount.is_finite() {
            amount.clamp(0.0, 1.0)
        } else {
            1.0
        };
        match self.law {
            MixLaw::Linear => {
                self.wet_gain = amount;
                self.dry_gain = 1.0 - amount;
            }
            MixLaw::EqualPower => {
                // Quarter-turn of a sine/cosine pair: sin^2 + cos^2 == 1, so
                // total power is constant across the sweep.
                // `cos(FRAC_PI_2)` in f32 is -4.4e-8, not zero: a fully wet
                // mix would otherwise carry a polarity-inverted trace of the
                // dry signal.
                let angle = amount * std::f32::consts::FRAC_PI_2;
                self.wet_gain = angle.sin().clamp(0.0, 1.0);
                self.dry_gain = angle.cos().clamp(0.0, 1.0);
            }
        }
    }

    pub fn law(&self) -> MixLaw {
        self.law
    }

    pub fn dry_gain(&self) -> f32 {
        self.dry_gain
    }

    pub fn wet_gain(&self) -> f32 {
        self.wet_gain
    }

    /// Blends one sample pair.
    #[inline]
    pub fn mix(&self, dry: f32, wet: f32) -> f32 {
        dry * self.dry_gain + wet * self.wet_gain
    }

    /// Blends a whole block in place, `wet` overwriting nothing.
    ///
    /// `output` holds the wet signal on entry and the mix on exit; `dry` is the
    /// untouched original. Panics if the lengths differ, which is a caller bug
    /// rather than a runtime condition.
    pub fn mix_block(&self, output: &mut [f32], dry: &[f32]) {
        assert_eq!(
            output.len(),
            dry.len(),
            "dry and wet blocks must be the same length"
        );
        for (slot, dry_sample) in output.iter_mut().zip(dry.iter()) {
            *slot = *dry_sample * self.dry_gain + *slot * self.wet_gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decibels_round_trip_through_linear_gain() {
        for db in [-60.0f32, -24.0, -6.0, 0.0, 6.0, 12.0] {
            let round_tripped = gain_to_db(db_to_gain(db));
            assert!(
                (round_tripped - db).abs() < 1.0e-3,
                "{db} dB came back as {round_tripped}"
            );
        }
    }

    #[test]
    fn silence_and_non_finite_values_have_defined_conversions() {
        assert_eq!(db_to_gain(f32::NEG_INFINITY), 0.0);
        assert_eq!(db_to_gain(-200.0), 0.0);
        assert!(db_to_gain(f32::NAN).is_finite());
        assert_eq!(gain_to_db(0.0), f32::NEG_INFINITY);
        assert_eq!(gain_to_db(f32::NAN), f32::NEG_INFINITY);
        // Polarity is not level.
        assert_eq!(gain_to_db(-1.0), gain_to_db(1.0));
    }

    #[test]
    fn the_extremes_of_the_mix_are_exactly_dry_and_exactly_wet() {
        for law in [MixLaw::Linear, MixLaw::EqualPower] {
            let dry_only = DryWet::new(law, 0.0);
            assert!((dry_only.mix(1.0, 9.0) - 1.0).abs() < 1.0e-6, "{law:?}");
            let wet_only = DryWet::new(law, 1.0);
            assert!((wet_only.mix(9.0, 1.0) - 1.0).abs() < 1.0e-6, "{law:?}");
        }
    }

    #[test]
    fn the_equal_power_law_holds_level_where_the_linear_one_dips() {
        // The reason both laws exist: for uncorrelated signals, power adds, so
        // linear coefficients of 0.5 each give 0.707 total instead of 1.0.
        let linear = DryWet::new(MixLaw::Linear, 0.5);
        let equal_power = DryWet::new(MixLaw::EqualPower, 0.5);

        let power = |mixer: &DryWet| mixer.dry_gain().powi(2) + mixer.wet_gain().powi(2);
        assert!(
            (power(&equal_power) - 1.0).abs() < 1.0e-5,
            "equal power did not hold power: {}",
            power(&equal_power)
        );
        assert!(
            power(&linear) < 0.6,
            "linear should lose power in the middle: {}",
            power(&linear)
        );
    }

    #[test]
    fn out_of_range_amounts_are_clamped_rather_than_extrapolated() {
        let mut mixer = DryWet::new(MixLaw::Linear, 0.5);
        mixer.set_amount(4.0);
        assert!((mixer.wet_gain() - 1.0).abs() < 1.0e-6);
        mixer.set_amount(-4.0);
        assert!((mixer.dry_gain() - 1.0).abs() < 1.0e-6);
        mixer.set_amount(f32::NAN);
        assert!(mixer.dry_gain().is_finite() && mixer.wet_gain().is_finite());
    }

    #[test]
    fn a_block_mix_matches_the_per_sample_one() {
        let mixer = DryWet::new(MixLaw::EqualPower, 0.3);
        let dry: Vec<f32> = (0..64).map(|i| (i as f32 * 0.1).sin()).collect();
        let wet: Vec<f32> = (0..64).map(|i| (i as f32 * 0.3).cos()).collect();

        let mut block = wet.clone();
        mixer.mix_block(&mut block, &dry);
        for index in 0..64 {
            let expected = mixer.mix(dry[index], wet[index]);
            assert!((block[index] - expected).abs() < 1.0e-6);
        }
    }

    #[test]
    fn applying_unity_gain_leaves_the_block_untouched() {
        let mut block: Vec<f32> = (0..32).map(|i| i as f32).collect();
        let original = block.clone();
        apply_gain(&mut block, 1.0);
        assert_eq!(block, original);
        apply_gain(&mut block, 0.5);
        assert!((block[8] - 4.0).abs() < 1.0e-6);
    }
}
