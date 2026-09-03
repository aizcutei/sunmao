//! 2x and 4x oversampling for non-linear processing.
//!
//! A waveshaper generates harmonics above the original signal's band. At the
//! base rate those fold back as aliasing; running the non-linearity at a higher
//! rate and filtering before decimating keeps them out.
//!
//! The filters are linear phase, so they impose a fixed delay the host must be
//! told about — [`Oversampler::latency_samples`] reports it, and a plugin
//! forwards that through `SunmaoPlugin::latency_samples`. A plugin that
//! oversamples without reporting latency is silently out of time with the rest
//! of the session.

/// How much to oversample by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OversamplingFactor {
    /// No oversampling: zero latency, the filters are bypassed entirely.
    None,
    X2,
    X4,
}

impl OversamplingFactor {
    pub fn ratio(self) -> usize {
        match self {
            Self::None => 1,
            Self::X2 => 2,
            Self::X4 => 4,
        }
    }

    /// Latency in samples at the *base* rate.
    ///
    /// This lives on the factor rather than only on a prepared [`Oversampler`]
    /// because a host asks for latency at points where the plugin may not have
    /// been prepared yet — VST3 calls `getLatencySamples` before `setActive`.
    /// The delay is fully determined by the factor and the filter length, so a
    /// plugin can answer honestly at any time.
    ///
    /// Each stage contributes the half-band's group delay measured at that
    /// stage's own rate, so a stage running at 4x costs half as much in base
    /// samples as one running at 2x, and both the interpolating and the
    /// decimating stage are traversed:
    ///
    /// - 2x: two stages at 2x — `2 * 16/2` = 16
    /// - 4x: that pair, plus two stages at 4x — `16 + 2 * 16/4` = 24
    pub fn latency_samples(self) -> u32 {
        let centre = HALFBAND_CENTRE as u32;
        match self {
            Self::None => 0,
            Self::X2 => centre,
            Self::X4 => centre + centre / 2,
        }
    }
}

/// Half-band FIR taps: a windowed sinc cutting at a quarter of whatever rate it
/// runs at.
///
/// Half-band means every other tap away from the centre is zero, so the real
/// cost is about half the length. The length is *even*-centred (33 taps, centre
/// at 16) rather than the more usual 31 on purpose: the 4x path runs stages at
/// two different rates, and the total delay is only an exact whole number of
/// base-rate samples if the centre divides by four. With a centre of 15 the true
/// latency would be 22.5 samples, and a host can only be told an integer — so
/// the plugin would sit half a sample out of alignment forever.
const HALFBAND_TAPS: usize = 33;
const HALFBAND_CENTRE: usize = HALFBAND_TAPS / 2;

/// Builds the half-band kernel once, at construction.
fn halfband_kernel() -> [f32; HALFBAND_TAPS] {
    let mut kernel = [0.0f32; HALFBAND_TAPS];
    for (index, tap) in kernel.iter_mut().enumerate() {
        let offset = index as isize - HALFBAND_CENTRE as isize;
        let sinc = if offset == 0 {
            0.5
        } else {
            let x = offset as f64 * std::f64::consts::PI * 0.5;
            (0.5 * x.sin() / x) * 2.0 * 0.5
        };
        // Blackman window, to keep stopband leakage low enough that the
        // aliasing this is meant to prevent does not creep back in.
        let n = index as f64 / (HALFBAND_TAPS - 1) as f64;
        let window = 0.42 - 0.5 * (std::f64::consts::TAU * n).cos()
            + 0.08 * (2.0 * std::f64::consts::TAU * n).cos();
        *tap = (sinc * window) as f32;
    }
    // Normalize to unity DC gain so oversampling does not change level.
    let sum: f32 = kernel.iter().sum();
    if sum.abs() > f32::EPSILON {
        for tap in kernel.iter_mut() {
            *tap /= sum;
        }
    }
    kernel
}

/// One half-band FIR with its own delay line.
#[derive(Clone, Copy)]
struct Halfband {
    kernel: [f32; HALFBAND_TAPS],
    history: [f32; HALFBAND_TAPS],
    position: usize,
}

impl Halfband {
    fn new() -> Self {
        Self {
            kernel: halfband_kernel(),
            history: [0.0; HALFBAND_TAPS],
            position: 0,
        }
    }

    fn reset(&mut self) {
        self.history = [0.0; HALFBAND_TAPS];
        self.position = 0;
    }

    /// Pushes one sample and returns the filtered result.
    fn process(&mut self, input: f32) -> f32 {
        self.history[self.position] = input;
        let mut sum = 0.0f32;
        let mut index = self.position;
        for tap in self.kernel.iter() {
            sum += tap * self.history[index];
            index = if index == 0 {
                HALFBAND_TAPS - 1
            } else {
                index - 1
            };
        }
        self.position = (self.position + 1) % HALFBAND_TAPS;
        sum
    }
}

/// Runs a closure at a higher sample rate.
///
/// ```
/// # use sunmao_dsp::oversampling::{Oversampler, OversamplingFactor};
/// let mut os = Oversampler::new();
/// os.prepare(OversamplingFactor::X2, 512);
/// // Latency must be reported to the host; it is not zero.
/// assert!(os.latency_samples() > 0);
///
/// let mut block = vec![0.0f32; 64];
/// os.process(&mut block, |upsampled| {
///     // Runs at twice the rate, so twice as many samples.
///     assert_eq!(upsampled.len(), 128);
///     for sample in upsampled.iter_mut() {
///         *sample = sample.tanh();
///     }
/// });
/// ```
pub struct Oversampler {
    factor: OversamplingFactor,
    /// One stage per doubling, each running at its own rate. 4x is *not* a
    /// single stuff-by-four: both half-bands would then cut at the same
    /// frequency, and the images between the base Nyquist and that cutoff would
    /// survive to fold back down on decimation.
    up: [Halfband; 2],
    down: [Halfband; 2],
    /// Allocated in `prepare`, never in `process`. The 4x path needs the
    /// intermediate rate as well as the final one.
    scratch_2x: Vec<f32>,
    scratch_4x: Vec<f32>,
    max_block: usize,
}

impl Default for Oversampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Oversampler {
    pub fn new() -> Self {
        Self {
            factor: OversamplingFactor::None,
            up: [Halfband::new(); 2],
            down: [Halfband::new(); 2],
            scratch_2x: Vec::new(),
            scratch_4x: Vec::new(),
            max_block: 0,
        }
    }

    /// Allocates for the given factor and maximum block size.
    ///
    /// Call from `initialize`, never from the audio callback — this is the only
    /// method here that allocates.
    pub fn prepare(&mut self, factor: OversamplingFactor, max_block: usize) {
        self.factor = factor;
        self.max_block = max_block;
        self.scratch_2x = vec![0.0; max_block * 2];
        self.scratch_4x = vec![
            0.0;
            if factor == OversamplingFactor::X4 {
                max_block * 4
            } else {
                0
            }
        ];
        self.reset();
    }

    /// Clears the filter delay lines.
    pub fn reset(&mut self) {
        for stage in self.up.iter_mut().chain(self.down.iter_mut()) {
            stage.reset();
        }
        for sample in self.scratch_2x.iter_mut().chain(self.scratch_4x.iter_mut()) {
            *sample = 0.0;
        }
    }

    pub fn factor(&self) -> OversamplingFactor {
        self.factor
    }

    /// Latency in samples at the *base* rate, for the host.
    ///
    /// See [`OversamplingFactor::latency_samples`], which a plugin can call
    /// before the oversampler has been prepared.
    pub fn latency_samples(&self) -> u32 {
        self.factor.latency_samples()
    }

    /// Runs `body` over an upsampled copy of `block`, then decimates back.
    ///
    /// Allocation-free: the scratch buffers come from `prepare`. A block longer
    /// than the prepared maximum is processed without oversampling rather than
    /// reallocating on the audio thread.
    pub fn process(&mut self, block: &mut [f32], body: impl FnOnce(&mut [f32])) {
        if self.factor == OversamplingFactor::None || block.len() > self.max_block {
            body(block);
            return;
        }

        // Destructured so the filters and the buffers are separate borrows.
        let Self {
            factor,
            up,
            down,
            scratch_2x,
            scratch_4x,
            ..
        } = self;
        let length = block.len();
        let half = &mut scratch_2x[..length * 2];

        interpolate(block, half, &mut up[0]);
        if *factor == OversamplingFactor::X4 {
            let full = &mut scratch_4x[..length * 4];
            interpolate(half, full, &mut up[1]);
            body(full);
            decimate(full, half, &mut down[1]);
        } else {
            body(half);
        }
        decimate(half, block, &mut down[0]);
    }
}

/// Keeps a sample inside the range the filter arithmetic can survive.
///
/// Two ways a block reaches here unusable: a non-finite sample from upstream,
/// and a finite-but-enormous one. The second is the subtler of the two —
/// interpolation multiplies by two to make up for the inserted zeros, so a
/// sample near `f32::MAX` overflows to infinity, and the FIR then sums `inf`
/// against `-inf` and produces NaN. The caller's non-linearity never even sees
/// the sample; the upsampler destroyed it first.
///
/// The bound is about +600 dBFS: unreachable by any real signal path, so this
/// is not a hidden limiter, but low enough that neither the doubling nor the
/// 33-tap sum can overflow.
#[inline]
fn sanitize(sample: f32) -> f32 {
    const CEILING: f32 = 1.0e30;
    if sample.is_finite() {
        sample.clamp(-CEILING, CEILING)
    } else {
        0.0
    }
}

/// Doubles the rate: zero-stuff, then filter away the image the stuffing
/// created. The factor of two makes up the level the inserted zeros cost.
fn interpolate(input: &[f32], output: &mut [f32], filter: &mut Halfband) {
    debug_assert_eq!(output.len(), input.len() * 2);
    for (index, slot) in output.iter_mut().enumerate() {
        let stuffed = if index % 2 == 0 {
            sanitize(input[index / 2]) * 2.0
        } else {
            0.0
        };
        *slot = filter.process(stuffed);
    }
}

/// Halves the rate: filter first, *then* drop every other sample. Filtering
/// after decimating would be too late — the fold has already happened.
///
/// The input is sanitized here as well, because between interpolation and this
/// point the caller's closure has run and may have produced anything.
fn decimate(input: &[f32], output: &mut [f32], filter: &mut Halfband) {
    debug_assert_eq!(input.len(), output.len() * 2);
    for (index, sample) in input.iter().enumerate() {
        let filtered = filter.process(sanitize(*sample));
        if index % 2 == 0 {
            output[index / 2] = filtered;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_oversampling_is_a_transparent_passthrough_with_no_latency() {
        let mut os = Oversampler::new();
        os.prepare(OversamplingFactor::None, 64);
        assert_eq!(os.latency_samples(), 0);

        let mut block: Vec<f32> = (0..64).map(|i| (i as f32 * 0.1).sin()).collect();
        let original = block.clone();
        os.process(&mut block, |samples| {
            // Same length: no rate change at all.
            assert_eq!(samples.len(), 64);
        });
        assert_eq!(block, original);
    }

    #[test]
    fn the_callback_sees_the_higher_rate() {
        for (factor, ratio) in [(OversamplingFactor::X2, 2), (OversamplingFactor::X4, 4)] {
            let mut os = Oversampler::new();
            os.prepare(factor, 128);
            let mut block = vec![0.0f32; 128];
            let mut seen = 0;
            os.process(&mut block, |samples| seen = samples.len());
            assert_eq!(seen, 128 * ratio, "{factor:?}");
        }
    }

    #[test]
    fn latency_can_be_answered_before_the_oversampler_is_prepared() {
        // A host asks for latency before it activates the plugin, so the
        // factor has to be able to answer on its own.
        for factor in [
            OversamplingFactor::None,
            OversamplingFactor::X2,
            OversamplingFactor::X4,
        ] {
            let mut os = Oversampler::new();
            os.prepare(factor, 64);
            assert_eq!(factor.latency_samples(), os.latency_samples(), "{factor:?}");
        }
    }

    #[test]
    fn oversampling_reports_a_non_zero_latency_that_grows_with_the_factor() {
        let mut two = Oversampler::new();
        two.prepare(OversamplingFactor::X2, 64);
        let mut four = Oversampler::new();
        four.prepare(OversamplingFactor::X4, 64);
        assert!(two.latency_samples() > 0);
        assert!(
            four.latency_samples() > two.latency_samples(),
            "4x cascades another stage, so it must cost more delay"
        );
    }

    /// Locates the peak of the impulse response, i.e. the actual group delay.
    fn measured_latency(factor: OversamplingFactor) -> usize {
        let mut os = Oversampler::new();
        let block = 256;
        os.prepare(factor, block);
        let mut signal = vec![0.0f32; block * 4];
        signal[0] = 1.0;

        let mut output = Vec::new();
        for chunk in signal.chunks_mut(block) {
            os.process(chunk, |_| {});
            output.extend_from_slice(chunk);
        }
        output
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap())
            .map(|(index, _)| index)
            .unwrap_or(0)
    }

    #[test]
    fn the_reported_latency_matches_the_measured_group_delay() {
        // The reported number is what the host uses to align the track, so it
        // has to match reality rather than merely be non-zero.
        for factor in [OversamplingFactor::X2, OversamplingFactor::X4] {
            let mut os = Oversampler::new();
            os.prepare(factor, 256);
            let reported = os.latency_samples() as usize;
            let measured = measured_latency(factor);
            let difference = reported.abs_diff(measured);
            assert!(
                difference <= 1,
                "{factor:?}: reported {reported}, measured {measured}"
            );
        }
    }

    #[test]
    fn oversampling_preserves_level_for_a_linear_body() {
        // Up then down with no non-linearity must return the signal, delayed.
        for factor in [OversamplingFactor::X2, OversamplingFactor::X4] {
            let mut os = Oversampler::new();
            let block = 512;
            os.prepare(factor, block);
            let mut signal: Vec<f32> = (0..block * 4)
                .map(|i| (i as f64 * 0.01).sin() as f32 * 0.5)
                .collect();
            for chunk in signal.chunks_mut(block) {
                os.process(chunk, |_| {});
            }
            // Skip the ramp-in, then compare peaks.
            let peak = signal[block..].iter().map(|s| s.abs()).fold(0.0, f32::max);
            assert!(
                (peak - 0.5).abs() < 0.05,
                "{factor:?} changed the level: peak {peak}"
            );
        }
    }

    #[test]
    fn a_block_larger_than_prepared_is_handled_without_allocating() {
        // Reallocating on the audio thread would be worse than not
        // oversampling, so an oversized block falls back to passthrough.
        let mut os = Oversampler::new();
        os.prepare(OversamplingFactor::X2, 64);
        let mut block = vec![0.5f32; 256];
        let mut seen = 0;
        os.process(&mut block, |samples| seen = samples.len());
        assert_eq!(seen, 256, "should have run at the base rate");
        assert!(block.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn extreme_input_does_not_come_back_as_nan() {
        // Interpolation doubles the signal to make up for the inserted zeros,
        // so a sample near f32::MAX overflows to infinity and the FIR turns
        // inf - inf into NaN — for input that was perfectly finite going in.
        for factor in [OversamplingFactor::X2, OversamplingFactor::X4] {
            let mut os = Oversampler::new();
            os.prepare(factor, 256);
            let mut block: Vec<f32> = [f32::MAX, -f32::MAX, 1.0e30, f32::NAN, f32::INFINITY, 0.0]
                .iter()
                .copied()
                .cycle()
                .take(256)
                .collect();
            os.process(&mut block, |upsampled| {
                assert!(
                    upsampled.iter().all(|s| s.is_finite()),
                    "{factor:?}: the closure was handed a non-finite sample"
                );
            });
            assert!(
                block.iter().all(|s| s.is_finite()),
                "{factor:?} produced a non-finite output"
            );
        }
    }

    #[test]
    fn a_nonfinite_burst_does_not_linger_in_the_filter_state() {
        // The filters are FIR, so a bad sample has bounded reach — but only if
        // it never enters the delay line as NaN in the first place.
        let mut os = Oversampler::new();
        os.prepare(OversamplingFactor::X4, 64);
        let mut poison = vec![f32::NAN; 64];
        os.process(&mut poison, |_| {});

        let mut block = vec![0.5f32; 64];
        for _ in 0..4 {
            block.fill(0.5);
            os.process(&mut block, |_| {});
        }
        assert!(
            block.iter().all(|s| s.is_finite()),
            "the oversampler stayed poisoned: {block:?}"
        );
        let mean = block.iter().sum::<f32>() / block.len() as f32;
        assert!(
            (mean - 0.5).abs() < 0.01,
            "signal did not recover after the burst: mean {mean}"
        );
    }

    #[test]
    fn oversampling_suppresses_the_aliasing_a_hard_nonlinearity_creates() {
        // A hard clip on a high sine generates harmonics above Nyquist. At the
        // base rate they fold down as inharmonic tones; oversampled, they are
        // filtered before they can.
        let sample_rate = 48_000.0f64;
        let frequency = 7_000.0f64;
        let block = 1_024;

        let render = |factor: OversamplingFactor| -> Vec<f32> {
            let mut os = Oversampler::new();
            os.prepare(factor, block);
            let mut phase = 0.0f64;
            let mut output = Vec::new();
            for _ in 0..8 {
                let mut chunk: Vec<f32> = (0..block)
                    .map(|_| {
                        let sample = (phase * std::f64::consts::TAU).sin() as f32;
                        phase = (phase + frequency / sample_rate).fract();
                        sample
                    })
                    .collect();
                os.process(&mut chunk, |samples| {
                    for sample in samples.iter_mut() {
                        *sample = sample.clamp(-0.3, 0.3);
                    }
                });
                output.extend_from_slice(&chunk);
            }
            output
        };

        // Energy at a frequency that is purely an aliasing artefact: the third
        // harmonic of 7 kHz is 21 kHz, which folds to 27 kHz -> 21 kHz... the
        // fifth (35 kHz) folds to 13 kHz, which is where we listen.
        let goertzel = |samples: &[f32], target: f64| -> f32 {
            let omega = std::f64::consts::TAU * target / sample_rate;
            let coefficient = 2.0 * omega.cos();
            let (mut s0, mut s1, mut s2) = (0.0f64, 0.0f64, 0.0f64);
            for sample in samples.iter().skip(2_048) {
                s0 = f64::from(*sample) + coefficient * s1 - s2;
                s2 = s1;
                s1 = s0;
            }
            ((s1 * s1 + s2 * s2 - coefficient * s1 * s2).abs().sqrt()) as f32
        };

        let base = render(OversamplingFactor::None);
        let oversampled = render(OversamplingFactor::X4);
        let alias_frequency = 13_000.0;
        let base_alias = goertzel(&base, alias_frequency);
        let oversampled_alias = goertzel(&oversampled, alias_frequency);

        assert!(
            oversampled_alias < base_alias * 0.5,
            "4x did not suppress the alias: {oversampled_alias} vs base {base_alias}"
        );
    }
}
