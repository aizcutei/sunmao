//! Lock-free peak/RMS display backed by the DSP meter.
use sunmao_dsp::metering::MeterHandle;
use sunmao_gui::SpectrumSource;

/// Supplies peak and RMS bars, in that order, on a -60..0 dBFS scale.
///
/// The DSP meter owns ballistics; use `with_falloff(1.0)` on the display to
/// avoid applying a second decay. Reads allocate nothing and take no locks.
/// Peak and RMS are independent atomic readings, not a coherent snapshot.
///
/// ```
/// use sunmao::prelude::*;
/// let mut audio = Meter::new();
/// let mut display = SpectrumAnalyzer::new(Box::new(MeterSource::new(audio.handle())))
///     .with_falloff(1.0);
/// audio.process_block(&[0.5; 512]);
/// display.refresh();
/// ```
pub struct MeterSource {
    handle: MeterHandle,
}

impl MeterSource {
    pub fn new(handle: MeterHandle) -> Self {
        Self { handle }
    }
}

fn level(value: f32) -> f32 {
    if value.is_nan() || value <= 0.0 {
        0.0
    } else if value >= 1.0 {
        1.0
    } else {
        ((20.0 * value.log10() + 60.0) / 60.0).clamp(0.0, 1.0)
    }
}

impl SpectrumSource for MeterSource {
    fn fill(&mut self, out: &mut [f32]) -> usize {
        let values = [level(self.handle.peak()), level(self.handle.rms())];
        let count = out.len().min(2);
        out[..count].copy_from_slice(&values[..count]);
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use sunmao_dsp::metering::Meter;

    #[test]
    fn audio_publication_and_reset_reach_the_display() {
        let mut meter = Meter::new();
        let mut source = MeterSource::new(meter.handle());
        let mut bars = [0.0; 3];
        meter.process_block(&[0.5; 24000]);
        assert_eq!(source.fill(&mut bars), 2);
        assert!((bars[0] - 0.89965665).abs() < 0.001);
        assert!((bars[1] - bars[0]).abs() < 0.001);
        meter.reset();
        source.fill(&mut bars);
        assert_eq!(bars, [0.0; 3]);
        assert_eq!(source.fill(&mut []), 0);
        assert_eq!(source.fill(&mut [0.0]), 1);
    }

    proptest! {
        #[test]
        fn arbitrary_levels_are_bounded_and_monotone(a in any::<f32>(), b in any::<f32>()) {
            let x = level(a);
            prop_assert!(x.is_finite() && (0.0..=1.0).contains(&x));
            if !a.is_nan() && !b.is_nan() && a <= b {
                prop_assert!(x <= level(b));
            }
        }
    }
}
