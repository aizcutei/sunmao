//! SunMao SVF Filter — Phase 3 acceptance fixture.
//!
//! Now built on the `sunmao_dsp::filters::Svf` component. The inline TPT
//! implementation this fixture started with has been removed; every test below
//! is unchanged from the skeleton, which is what makes the swap evidence that
//! the component reproduces the hand-written DSP rather than merely compiling.

use sunmao::prelude::*;
use sunmao_dsp::filters::Svf;

/// Filter mode selected by the `mode` parameter.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FilterMode {
    Lowpass,
    Bandpass,
    Highpass,
}

impl FilterMode {
    fn from_index(index: i32) -> Self {
        match index {
            1 => Self::Bandpass,
            2 => Self::Highpass,
            _ => Self::Lowpass,
        }
    }
}

/// SVF parameters.
#[derive(Params)]
pub struct SvfParams {
    /// Cutoff frequency in Hz.
    pub cutoff: FloatParam,
    /// Resonance amount; 0.0 maps to a Butterworth-ish damping, 1.0 to a
    /// strongly resonant but still bounded filter.
    pub resonance: FloatParam,
    /// Filter mode: 0 lowpass, 1 bandpass, 2 highpass.
    pub mode: IntParam,
}

impl Default for SvfParams {
    fn default() -> Self {
        Self {
            cutoff: FloatParam::new("cutoff", "Cutoff", 1000.0, 20.0, 20000.0),
            resonance: FloatParam::new("resonance", "Resonance", 0.0, 0.0, 1.0),
            mode: IntParam::new("mode", "Mode", 0, 0, 2),
        }
    }
}

/// The SVF filter plugin.
pub struct SvfPlugin {
    params: Arc<SvfParams>,
    sample_rate: f64,
    states: [Svf; 2],
}

impl Default for SvfPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(SvfParams::default()),
            sample_rate: 44_100.0,
            states: [Svf::new(); 2],
        }
    }
}

impl SunmaoPlugin for SvfPlugin {
    const NAME: &'static str = "SunMao SVF Filter";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";

    type Params = SvfParams;

    fn params(&self) -> Arc<Self::Params> {
        self.params.clone()
    }

    fn initialize(&mut self, sample_rate: f64, _max_block_size: u32) {
        self.sample_rate = sample_rate;
    }

    fn reset(&mut self) {
        for state in &mut self.states {
            state.reset();
        }
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        buffer.copy_input_to_output();

        // Skeleton semantics: parameter changes apply at block rate (last one
        // wins). M2's smoothers take over per-sample coefficient updates.
        let mut cutoff = self.params.cutoff.get();
        let mut resonance = self.params.resonance.get();
        for change in events.param_changes() {
            let value = change.value.clamp(0.0, 1.0);
            if change.id == self.params.cutoff.id {
                cutoff = self.params.cutoff.min
                    + value * (self.params.cutoff.max - self.params.cutoff.min);
            } else if change.id == self.params.resonance.id {
                resonance = self.params.resonance.min
                    + value * (self.params.resonance.max - self.params.resonance.min);
            }
        }
        let mode = FilterMode::from_index(self.params.mode.get());

        // The component owns cutoff prewarping, the Nyquist clamp, and the
        // resonance-to-damping mapping the inline version used to do here.
        for state in &mut self.states {
            state.set_params(f64::from(cutoff), resonance, self.sample_rate);
        }

        let channels = buffer.num_output_channels().min(2);
        for sample_index in 0..buffer.num_samples() {
            for (channel, state) in self.states.iter_mut().enumerate().take(channels) {
                let input = buffer.output(channel)[sample_index];
                let out = state.tick(input);
                let (lp, bp, hp) = (out.lowpass, out.bandpass, out.highpass);
                buffer.output(channel)[sample_index] = match mode {
                    FilterMode::Lowpass => lp,
                    FilterMode::Bandpass => bp,
                    FilterMode::Highpass => hp,
                };
            }
        }

        ProcessStatus::Normal
    }

    fn vst3_info() -> Vst3Info {
        Vst3Info {
            class_id: *b"SunMaoFxSvfFilt!",
            categories: &["Fx", "Filter"],
            ..Default::default()
        }
    }

    fn clap_info() -> ClapInfo {
        ClapInfo {
            id: "com.sunmao.fx.svf",
            features: &["audio-effect", "filter", "stereo"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process_block(plugin: &mut SvfPlugin, input: &[f32]) -> Vec<f32> {
        let input_right = input.to_vec();
        let inputs: [&[f32]; 2] = [input, &input_right];
        let mut left = vec![0.0; input.len()];
        let mut right = vec![0.0; input.len()];
        let num_samples = input.len();
        let mut outputs: [&mut [f32]; 2] = [&mut left, &mut right];
        let mut buffer = AudioBuffer::new(&inputs, &mut outputs, num_samples);
        let events = EventQueue::new();
        let status = plugin.process(&mut buffer, &events, &ProcessContext::default());
        assert_eq!(status, ProcessStatus::Normal);
        left
    }

    /// Deterministic pseudo-noise in -1.0..1.0 (xorshift, no dependencies).
    fn noise(len: usize) -> Vec<f32> {
        let mut state = 0x2545F491u32;
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state as f32 / u32::MAX as f32) * 2.0 - 1.0
            })
            .collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn the_lowpass_passes_dc_and_the_highpass_blocks_it() {
        let dc = vec![1.0f32; 4096];

        let mut lowpass = SvfPlugin::default();
        lowpass.initialize(48_000.0, 64);
        lowpass.params.mode.set(0);
        let lp_out = process_block(&mut lowpass, &dc);
        assert!(
            (lp_out[4000] - 1.0).abs() < 1e-3,
            "lowpass must settle to the DC value, got {}",
            lp_out[4000]
        );

        let mut highpass = SvfPlugin::default();
        highpass.initialize(48_000.0, 64);
        highpass.params.mode.set(2);
        let hp_out = process_block(&mut highpass, &dc);
        assert!(
            hp_out[4000].abs() < 1e-3,
            "highpass must reject DC, got {}",
            hp_out[4000]
        );
    }

    #[test]
    fn the_bandpass_rejects_both_dc_and_nyquist() {
        let mut plugin = SvfPlugin::default();
        plugin.initialize(48_000.0, 64);
        plugin.params.mode.set(1);
        plugin.params.cutoff.set(1000.0);

        let dc = vec![1.0f32; 4096];
        let dc_out = process_block(&mut plugin, &dc);
        assert!(dc_out[4000].abs() < 1e-3, "bandpass must reject DC");

        plugin.reset();
        let nyquist: Vec<f32> = (0..4096)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        let ny_out = process_block(&mut plugin, &nyquist);
        assert!(
            rms(&ny_out[2048..]) < 0.05,
            "bandpass must attenuate Nyquist, rms {}",
            rms(&ny_out[2048..])
        );
    }

    #[test]
    fn a_low_cutoff_attenuates_noise_harder_than_an_open_one() {
        let input = noise(8192);

        let mut open = SvfPlugin::default();
        open.initialize(48_000.0, 64);
        open.params.cutoff.set(20_000.0);
        let open_rms = rms(&process_block(&mut open, &input)[4096..]);

        let mut closed = SvfPlugin::default();
        closed.initialize(48_000.0, 64);
        closed.params.cutoff.set(100.0);
        let closed_rms = rms(&process_block(&mut closed, &input)[4096..]);

        assert!(
            closed_rms < open_rms * 0.3,
            "closing the filter must remove broadband energy: {closed_rms} vs {open_rms}"
        );
    }

    #[test]
    fn full_resonance_on_noise_stays_finite_and_bounded() {
        let mut plugin = SvfPlugin::default();
        plugin.initialize(48_000.0, 64);
        plugin.params.resonance.set(1.0);
        plugin.params.cutoff.set(2000.0);

        let output = process_block(&mut plugin, &noise(48_000));
        assert!(
            output.iter().all(|s| s.is_finite()),
            "the filter must never emit NaN/inf"
        );
        let peak = output.iter().fold(0.0f32, |acc, s| acc.max(s.abs()));
        assert!(
            peak < 100.0,
            "even at full resonance the filter must not blow up, peak {peak}"
        );
    }

    #[test]
    fn extreme_cutoffs_do_not_produce_nan() {
        for cutoff in [20.0f32, 20_000.0] {
            let mut plugin = SvfPlugin::default();
            plugin.initialize(48_000.0, 64);
            plugin.params.cutoff.set(cutoff);
            plugin.params.resonance.set(1.0);
            let output = process_block(&mut plugin, &noise(4096));
            assert!(
                output.iter().all(|s| s.is_finite()),
                "cutoff {cutoff} produced a non-finite sample"
            );
        }
    }

    #[test]
    fn reset_clears_the_filter_memory() {
        let mut plugin = SvfPlugin::default();
        plugin.initialize(48_000.0, 64);
        plugin.params.cutoff.set(100.0);

        let dc = vec![1.0f32; 1024];
        process_block(&mut plugin, &dc);
        plugin.reset();

        let silence = vec![0.0f32; 64];
        let output = process_block(&mut plugin, &silence);
        assert!(
            output.iter().all(|s| *s == 0.0),
            "filter memory must not survive reset, got {output:?}"
        );
    }
}

// ============ Unified VST3 + CLAP Export ============
sunmao::sunmao_export!(SvfPlugin);
