//! SunMao Tempo Delay — Phase 2 acceptance fixture.
//!
//! M0 skeleton: a plain feedback delay with a millisecond time parameter,
//! built only on the Phase 1 contract. M1 replaces the free-running time with
//! a tempo-synced division consumed from the transport-aware
//! `ProcessContext`; M2 adds latency and tail reporting.

use sunmao::prelude::*;

/// Upper bound for the delay line, allocated once off the audio thread.
const MAX_DELAY_SECONDS: f64 = 2.0;

/// Tempo delay parameters.
#[derive(Params)]
pub struct TempoDelayParams {
    /// Delay time in milliseconds, used when sync is off or the host provides
    /// no tempo.
    pub time_ms: FloatParam,
    /// Whether the delay time follows the host tempo.
    pub sync: BoolParam,
    /// Tempo division as a number of quarter notes (1 = quarter, 2 = half).
    pub division: FloatParam,
    /// Feedback amount fed back into the delay line.
    pub feedback: FloatParam,
    /// Dry/wet mix.
    pub mix: FloatParam,
}

impl Default for TempoDelayParams {
    fn default() -> Self {
        Self {
            time_ms: FloatParam::new("time_ms", "Time", 250.0, 1.0, 2000.0),
            sync: BoolParam::new("sync", "Tempo Sync", true),
            division: FloatParam::new("division", "Division", 1.0, 0.25, 4.0),
            feedback: FloatParam::new("feedback", "Feedback", 0.3, 0.0, 0.95),
            mix: FloatParam::new("mix", "Mix", 0.5, 0.0, 1.0),
        }
    }
}

/// Lookahead used to align the wet path, in milliseconds. Offline rendering
/// doubles it because there is no realtime deadline to respect.
const LOOKAHEAD_MS: f64 = 5.0;

/// The tempo delay plugin.
pub struct TempoDelayPlugin {
    params: Arc<TempoDelayParams>,
    delay_lines: [Vec<f32>; 2],
    write_pos: usize,
    sample_rate: f64,
    render_mode: RenderMode,
}

impl Default for TempoDelayPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(TempoDelayParams::default()),
            delay_lines: [Vec::new(), Vec::new()],
            write_pos: 0,
            sample_rate: 44_100.0,
            render_mode: RenderMode::Realtime,
        }
    }
}

impl TempoDelayPlugin {
    fn lookahead_seconds(&self) -> f64 {
        let scale = match self.render_mode {
            RenderMode::Realtime => 1.0,
            RenderMode::Offline => 2.0,
        };
        LOOKAHEAD_MS / 1000.0 * scale
    }
}

impl SunmaoPlugin for TempoDelayPlugin {
    const NAME: &'static str = "SunMao Tempo Delay";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";

    type Params = TempoDelayParams;

    fn params(&self) -> Arc<Self::Params> {
        self.params.clone()
    }

    fn initialize(&mut self, sample_rate: f64, _max_block_size: u32) {
        self.sample_rate = sample_rate;
        let capacity = (MAX_DELAY_SECONDS * sample_rate).ceil() as usize + 1;
        for line in &mut self.delay_lines {
            line.clear();
            line.resize(capacity, 0.0);
        }
        self.write_pos = 0;
    }

    fn reset(&mut self) {
        for line in &mut self.delay_lines {
            line.fill(0.0);
        }
        self.write_pos = 0;
    }

    fn latency_samples(&self) -> u32 {
        (self.lookahead_seconds() * self.sample_rate).round() as u32
    }

    fn tail(&self) -> TailLength {
        // With feedback the delay line never fully decays in finite time, so
        // the tail is only bounded when feedback is off.
        if self.params.feedback.get() > 0.0 {
            TailLength::Infinite
        } else {
            TailLength::Samples((MAX_DELAY_SECONDS * self.sample_rate).ceil() as u32)
        }
    }

    fn set_render_mode(&mut self, mode: RenderMode) {
        self.render_mode = mode;
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        events: &EventQueue,
        context: &ProcessContext,
    ) -> ProcessStatus {
        buffer.copy_input_to_output();

        // Skeleton semantics: parameter changes apply at block rate (last one
        // wins). The milestone DSP replaces this with sample-offset handling.
        let mut time_ms = self.params.time_ms.get();
        let mut feedback = self.params.feedback.get();
        let mut mix = self.params.mix.get();
        for change in events.param_changes() {
            let value = change.value.clamp(0.0, 1.0);
            if change.id == self.params.time_ms.id {
                time_ms = self.params.time_ms.min
                    + value * (self.params.time_ms.max - self.params.time_ms.min);
            } else if change.id == self.params.feedback.id {
                feedback = self.params.feedback.min
                    + value * (self.params.feedback.max - self.params.feedback.min);
            } else if change.id == self.params.mix.id {
                mix = self.params.mix.min + value * (self.params.mix.max - self.params.mix.min);
            }
        }

        let len = self.delay_lines[0].len();
        if len == 0 {
            return ProcessStatus::Normal;
        }

        // Tempo sync is best-effort: a host that reports no tempo (or a
        // degenerate one) falls back to the millisecond time rather than
        // producing a silent or zero-length delay.
        let synced_seconds = self
            .params
            .sync
            .get()
            .then(|| context.tempo)
            .flatten()
            .filter(|tempo| *tempo > 0.0)
            .map(|tempo| f64::from(self.params.division.get()) * 60.0 / tempo);
        let delay_seconds = synced_seconds.unwrap_or_else(|| f64::from(time_ms) / 1000.0);
        let delay_samples = ((delay_seconds * self.sample_rate) as usize).clamp(1, len - 1);

        let channels = buffer.num_output_channels().min(2);
        for sample_index in 0..buffer.num_samples() {
            let read_pos = (self.write_pos + len - delay_samples) % len;
            for (channel, line) in self.delay_lines.iter_mut().enumerate().take(channels) {
                let dry = buffer.output(channel)[sample_index];
                let delayed = line[read_pos];
                line[self.write_pos] = dry + delayed * feedback;
                buffer.output(channel)[sample_index] = dry * (1.0 - mix) + delayed * mix;
            }
            self.write_pos = (self.write_pos + 1) % len;
        }

        ProcessStatus::Normal
    }

    fn vst3_info() -> Vst3Info {
        Vst3Info {
            class_id: *b"SunMaoFxTmpoDly!",
            categories: &["Fx", "Delay"],
            ..Default::default()
        }
    }

    fn clap_info() -> ClapInfo {
        ClapInfo {
            id: "com.sunmao.fx.tempo_delay",
            features: &["audio-effect", "delay", "stereo"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process_block(
        plugin: &mut TempoDelayPlugin,
        input: &[f32],
        output: &mut [f32],
    ) -> ProcessStatus {
        process_block_with_tempo(plugin, input, output, None)
    }

    fn process_block_with_tempo(
        plugin: &mut TempoDelayPlugin,
        input: &[f32],
        output: &mut [f32],
        tempo: Option<f64>,
    ) -> ProcessStatus {
        let input_right = input.to_vec();
        let inputs: [&[f32]; 2] = [input, &input_right];
        let mut output_right = vec![0.0; output.len()];
        let num_samples = output.len();
        let mut outputs: [&mut [f32]; 2] = [output, &mut output_right];
        let mut buffer = AudioBuffer::new(&inputs, &mut outputs, num_samples);
        let events = EventQueue::new();
        plugin.process(
            &mut buffer,
            &events,
            &ProcessContext {
                sample_rate: 1000.0,
                tempo,
                is_playing: true,
                sample_pos: 0,
                ..Default::default()
            },
        )
    }

    /// Configures a fully wet, non-feeding delay so a single impulse marks the
    /// delay length exactly.
    fn impulse_probe(plugin: &mut TempoDelayPlugin) {
        plugin.initialize(1000.0, 32);
        plugin.params.mix.set(1.0);
        plugin.params.feedback.set(0.0);
    }

    /// Returns the sample index where the delayed impulse lands.
    fn echo_position(output: &[f32]) -> Option<usize> {
        output
            .iter()
            .position(|sample| (*sample - 1.0).abs() < 1e-6)
    }

    #[test]
    fn offline_rendering_doubles_the_reported_lookahead() {
        let mut plugin = TempoDelayPlugin::default();
        plugin.initialize(48_000.0, 64);
        let realtime = plugin.latency_samples();
        assert_eq!(realtime, 240, "5 ms at 48 kHz");

        plugin.set_render_mode(RenderMode::Offline);
        assert_eq!(plugin.latency_samples(), realtime * 2);

        plugin.set_render_mode(RenderMode::Realtime);
        assert_eq!(plugin.latency_samples(), realtime);
    }

    #[test]
    fn feedback_makes_the_tail_unbounded() {
        let mut plugin = TempoDelayPlugin::default();
        plugin.initialize(48_000.0, 64);

        plugin.params.feedback.set(0.0);
        assert_eq!(
            plugin.tail(),
            TailLength::Samples((MAX_DELAY_SECONDS * 48_000.0) as u32)
        );

        plugin.params.feedback.set(0.5);
        assert_eq!(plugin.tail(), TailLength::Infinite);
    }

    #[test]
    fn tempo_sync_derives_the_delay_from_the_host_tempo() {
        let mut plugin = TempoDelayPlugin::default();
        impulse_probe(&mut plugin);
        plugin.params.sync.set(true);
        plugin.params.division.set(1.0);
        // The fixture runs at 1 kHz, so an exaggerated tempo keeps one quarter
        // note inside a short test block: 60 / 12000 s = 5 samples.
        plugin.params.time_ms.set(20.0); // deliberately different from the sync time

        let mut input = [0.0f32; 16];
        input[0] = 1.0;
        let mut output = [0.0f32; 16];
        process_block_with_tempo(&mut plugin, &input, &mut output, Some(12_000.0));

        assert_eq!(
            echo_position(&output),
            Some(5),
            "a quarter note at 12000 BPM is 5 samples, got {output:?}"
        );
    }

    #[test]
    fn the_division_scales_the_synced_delay() {
        let mut plugin = TempoDelayPlugin::default();
        impulse_probe(&mut plugin);
        plugin.params.sync.set(true);
        plugin.params.division.set(2.0); // half note

        let mut input = [0.0f32; 16];
        input[0] = 1.0;
        let mut output = [0.0f32; 16];
        process_block_with_tempo(&mut plugin, &input, &mut output, Some(12_000.0));

        assert_eq!(echo_position(&output), Some(10), "got {output:?}");
    }

    #[test]
    fn a_host_without_tempo_falls_back_to_the_millisecond_time() {
        let mut plugin = TempoDelayPlugin::default();
        impulse_probe(&mut plugin);
        plugin.params.sync.set(true);
        plugin.params.division.set(1.0);
        plugin.params.time_ms.set(7.0); // 7 samples at 1 kHz

        let mut input = [0.0f32; 16];
        input[0] = 1.0;
        let mut output = [0.0f32; 16];
        process_block_with_tempo(&mut plugin, &input, &mut output, None);

        assert_eq!(echo_position(&output), Some(7), "got {output:?}");
    }

    #[test]
    fn a_degenerate_tempo_falls_back_instead_of_collapsing_the_delay() {
        let mut plugin = TempoDelayPlugin::default();
        impulse_probe(&mut plugin);
        plugin.params.sync.set(true);
        plugin.params.time_ms.set(7.0);

        let mut input = [0.0f32; 16];
        input[0] = 1.0;
        let mut output = [0.0f32; 16];
        process_block_with_tempo(&mut plugin, &input, &mut output, Some(0.0));

        assert_eq!(echo_position(&output), Some(7), "got {output:?}");
    }

    #[test]
    fn sync_off_ignores_the_host_tempo() {
        let mut plugin = TempoDelayPlugin::default();
        impulse_probe(&mut plugin);
        plugin.params.sync.set(false);
        plugin.params.division.set(1.0);
        plugin.params.time_ms.set(9.0);

        let mut input = [0.0f32; 16];
        input[0] = 1.0;
        let mut output = [0.0f32; 16];
        process_block_with_tempo(&mut plugin, &input, &mut output, Some(12_000.0));

        assert_eq!(echo_position(&output), Some(9), "got {output:?}");
    }

    #[test]
    fn an_impulse_reappears_after_the_configured_delay() {
        let mut plugin = TempoDelayPlugin::default();
        plugin.initialize(1000.0, 16);
        plugin.params.time_ms.set(4.0); // 4 samples at 1 kHz
        plugin.params.mix.set(1.0);
        plugin.params.feedback.set(0.0);

        let mut input = [0.0f32; 12];
        input[0] = 1.0;
        let mut output = [0.0f32; 12];
        let status = process_block(&mut plugin, &input, &mut output);

        assert_eq!(status, ProcessStatus::Normal);
        assert_eq!(output[0], 0.0, "fully wet output starts silent");
        assert!(
            (output[4] - 1.0).abs() < 1e-6,
            "impulse should reappear at sample 4, got {output:?}"
        );
    }

    #[test]
    fn reset_clears_the_delay_line() {
        let mut plugin = TempoDelayPlugin::default();
        plugin.initialize(1000.0, 16);
        plugin.params.time_ms.set(4.0);
        plugin.params.mix.set(1.0);
        plugin.params.feedback.set(0.5);

        let mut input = [0.0f32; 8];
        input[0] = 1.0;
        let mut output = [0.0f32; 8];
        process_block(&mut plugin, &input, &mut output);
        plugin.reset();

        let silent = [0.0f32; 8];
        let mut after_reset = [0.0f32; 8];
        process_block(&mut plugin, &silent, &mut after_reset);
        assert!(
            after_reset.iter().all(|sample| *sample == 0.0),
            "delay memory must not survive reset, got {after_reset:?}"
        );
    }
}

// ============ Unified VST3 + CLAP Export ============
sunmao::sunmao_export!(TempoDelayPlugin);
