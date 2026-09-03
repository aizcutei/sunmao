//! Peak and RMS metering, published lock-free for a GUI to read.
//!
//! A meter is read from a different thread than it is written on: the audio
//! callback measures, the editor draws at whatever rate the window redraws. A
//! mutex between them would let the GUI block the audio thread — a dropout
//! caused by drawing — so publication goes through atomics instead. The reader
//! may see a slightly stale value; for a meter that is invisible, and it is the
//! right trade against a glitch.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::flush_denormal;

/// The published values, shared between the audio thread and the editor.
///
/// `f32`s are stored as their bit patterns because there is no `AtomicF32`.
#[derive(Debug, Default)]
struct MeterState {
    peak: AtomicU32,
    rms: AtomicU32,
}

/// The read side of a meter, for the GUI.
///
/// Cloning gives another reader; all of them see the same published values.
#[derive(Debug, Clone)]
pub struct MeterHandle {
    state: Arc<MeterState>,
}

impl MeterHandle {
    /// Most recent peak level, linear.
    pub fn peak(&self) -> f32 {
        f32::from_bits(self.state.peak.load(Ordering::Acquire))
    }

    /// Most recent RMS level, linear.
    pub fn rms(&self) -> f32 {
        f32::from_bits(self.state.rms.load(Ordering::Acquire))
    }

    /// Most recent peak in decibels; `-inf` for silence.
    pub fn peak_db(&self) -> f32 {
        crate::mixing::gain_to_db(self.peak())
    }

    /// Most recent RMS in decibels; `-inf` for silence.
    pub fn rms_db(&self) -> f32 {
        crate::mixing::gain_to_db(self.rms())
    }
}

/// The write side of a meter, for the audio thread.
///
/// ```
/// # use sunmao_dsp::metering::Meter;
/// let mut meter = Meter::new();
/// let handle = meter.handle();
/// meter.set_sample_rate(48_000.0);
///
/// let block: Vec<f32> = (0..512).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
/// meter.process_block(&block);
///
/// // The GUI side sees it without taking a lock.
/// assert!(handle.peak() > 0.4);
/// ```
#[derive(Debug)]
pub struct Meter {
    state: Arc<MeterState>,
    /// Peak with a decay, so a transient stays visible long enough to read.
    peak: f32,
    peak_decay: f32,
    /// One-pole average of the squared signal: constant memory, no window
    /// buffer to size or allocate.
    mean_square: f32,
    rms_coefficient: f32,
}

impl Default for Meter {
    fn default() -> Self {
        Self::new()
    }
}

impl Meter {
    pub fn new() -> Self {
        let mut meter = Self {
            state: Arc::new(MeterState::default()),
            peak: 0.0,
            peak_decay: 0.0,
            mean_square: 0.0,
            rms_coefficient: 1.0,
        };
        meter.set_sample_rate(48_000.0);
        meter
    }

    /// A reader for the GUI. Call before moving the meter onto the audio side.
    pub fn handle(&self) -> MeterHandle {
        MeterHandle {
            state: Arc::clone(&self.state),
        }
    }

    /// Sets the sample rate, recomputing the ballistics.
    ///
    /// The time constants are fixed at values that read well: a peak that falls
    /// at roughly 20 dB per second, and a 100 ms RMS time constant. 100 ms is a
    /// time *constant*, not a window — a one-pole is within a percent of its
    /// target after about three of them, so this settles in the ~300 ms a VU
    /// meter is expected to take. Using 300 ms here instead would look right on
    /// paper and take a sluggish full second to reach level.
    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        // Per-sample multiplier giving about -20 dB/s.
        self.peak_decay = 10.0f64.powf(-20.0 / 20.0 / sample_rate) as f32;
        let time_constant_samples = 0.1 * sample_rate;
        self.rms_coefficient = (1.0 - (-1.0 / time_constant_samples).exp()) as f32;
    }

    /// Clears the meter and publishes silence.
    pub fn reset(&mut self) {
        self.peak = 0.0;
        self.mean_square = 0.0;
        self.publish();
    }

    /// Measures one block and publishes the result.
    ///
    /// Allocation-free, and it does not modify the audio.
    pub fn process_block(&mut self, block: &[f32]) {
        for sample in block {
            let value = if sample.is_finite() { *sample } else { 0.0 };
            let magnitude = value.abs();

            // Peak rises instantly and falls slowly: a click that lasts one
            // sample still has to be visible on screen.
            self.peak *= self.peak_decay;
            if magnitude > self.peak {
                self.peak = magnitude;
            }

            self.mean_square += self.rms_coefficient * (value * value - self.mean_square);
        }
        self.peak = flush_denormal(self.peak);
        self.mean_square = flush_denormal(self.mean_square);
        self.publish();
    }

    /// Current peak, without reading it back through the handle.
    pub fn peak(&self) -> f32 {
        self.peak
    }

    /// Current RMS, without reading it back through the handle.
    pub fn rms(&self) -> f32 {
        self.mean_square.max(0.0).sqrt()
    }

    /// One publication per block rather than per sample: the GUI cannot use
    /// per-sample resolution, and the stores are not free.
    fn publish(&self) {
        self.state
            .peak
            .store(self.peak.to_bits(), Ordering::Release);
        self.state
            .rms
            .store(self.rms().to_bits(), Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(count: usize, amplitude: f32) -> Vec<f32> {
        (0..count)
            .map(|i| (i as f32 * 0.05).sin() * amplitude)
            .collect()
    }

    #[test]
    fn the_handle_sees_what_the_audio_side_measured() {
        let mut meter = Meter::new();
        let handle = meter.handle();
        assert_eq!(handle.peak(), 0.0);

        // Half a second: several RMS time constants, so the average has
        // actually reached level rather than still climbing towards it.
        meter.process_block(&sine(24_000, 0.8));
        assert!(
            (handle.peak() - 0.8).abs() < 0.05,
            "peak was {}",
            handle.peak()
        );
        // RMS of a sine is its amplitude over root two.
        let expected_rms = 0.8 / std::f32::consts::SQRT_2;
        assert!(
            (handle.rms() - expected_rms).abs() < 0.05,
            "rms was {}, expected {expected_rms}",
            handle.rms()
        );
    }

    #[test]
    fn rms_sits_below_peak_for_anything_that_is_not_a_square_wave() {
        let mut meter = Meter::new();
        meter.process_block(&sine(48_000, 1.0));
        assert!(
            meter.rms() < meter.peak(),
            "rms {} should be under peak {}",
            meter.rms(),
            meter.peak()
        );
    }

    #[test]
    fn a_single_sample_transient_stays_visible() {
        // The point of peak decay: an instantaneous click must not be gone by
        // the time the editor next redraws.
        let mut meter = Meter::new();
        meter.set_sample_rate(48_000.0);
        let mut block = vec![0.0f32; 512];
        block[0] = 1.0;
        meter.process_block(&block);
        assert!(meter.peak() > 0.9, "transient decayed too fast");

        // A frame later (about 16 ms) it is still readable.
        meter.process_block(&vec![0.0f32; 768]);
        assert!(
            meter.peak() > 0.5,
            "peak fell to {} within a frame",
            meter.peak()
        );
    }

    #[test]
    fn the_meter_falls_back_to_silence_and_does_not_leave_a_denormal() {
        let mut meter = Meter::new();
        meter.set_sample_rate(48_000.0);
        meter.process_block(&sine(4_800, 1.0));
        assert!(meter.peak() > 0.5);

        fn feed_silence(meter: &mut Meter, seconds: f64) {
            for _ in 0..(seconds * 48_000.0 / 512.0) as usize {
                meter.process_block(&[0.0f32; 512]);
            }
        }

        // The decay is a rate, not a cliff: after a couple of seconds the
        // reading is inaudible, but a -20 dB/s ramp is still legitimately above
        // zero. Asserting exact silence here would be asserting a decay far
        // faster than the one that makes a transient readable in the first
        // place.
        feed_silence(&mut meter, 3.0);
        let handle = meter.handle();
        // 3 s at -20 dB/s is -60 dB down from a full-scale peak.
        assert!(
            handle.peak_db() < -50.0,
            "peak still audible after 3 s: {} dB",
            handle.peak_db()
        );
        assert!(
            handle.rms_db() < -50.0,
            "rms still audible after 3 s: {} dB",
            handle.rms_db()
        );

        // Left alone long enough, it must reach exactly zero rather than
        // creeping down through the denormal range forever — that is what the
        // flush is for, and it is the part that costs CPU if it is missing.
        feed_silence(&mut meter, 30.0);
        assert_eq!(meter.peak(), 0.0, "peak did not flush to zero");
        assert_eq!(meter.rms(), 0.0, "rms did not flush to zero");
        assert_eq!(handle.peak_db(), f32::NEG_INFINITY);
    }

    #[test]
    fn metering_does_not_modify_the_audio() {
        // A meter is an observer; a fixture that measures must sound identical
        // to one that does not.
        let mut meter = Meter::new();
        let block = sine(256, 0.5);
        let original = block.clone();
        meter.process_block(&block);
        assert_eq!(block, original);
    }

    #[test]
    fn non_finite_input_does_not_poison_the_meter() {
        let mut meter = Meter::new();
        meter.process_block(&[f32::NAN, f32::INFINITY, 0.5, -0.5]);
        assert!(meter.peak().is_finite() && meter.rms().is_finite());
        assert!(meter.peak() <= 1.0, "peak was {}", meter.peak());
    }

    #[test]
    fn reset_publishes_silence_to_the_reader() {
        let mut meter = Meter::new();
        let handle = meter.handle();
        meter.process_block(&sine(4_800, 1.0));
        assert!(handle.peak() > 0.5);
        meter.reset();
        assert_eq!(handle.peak(), 0.0);
        assert_eq!(handle.rms(), 0.0);
    }

    #[test]
    fn a_reader_on_another_thread_sees_updates_without_locking() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let mut meter = Meter::new();
        let handle = meter.handle();
        let done = Arc::new(AtomicBool::new(false));
        let reader_done = Arc::clone(&done);

        let reader = std::thread::spawn(move || {
            let mut seen_signal = false;
            while !reader_done.load(Ordering::Acquire) {
                if handle.peak() > 0.1 {
                    seen_signal = true;
                }
                std::hint::spin_loop();
            }
            seen_signal || handle.peak() > 0.1
        });

        for _ in 0..200 {
            meter.process_block(&sine(512, 0.9));
        }
        done.store(true, Ordering::Release);
        assert!(reader.join().unwrap(), "reader never observed the signal");
    }
}
