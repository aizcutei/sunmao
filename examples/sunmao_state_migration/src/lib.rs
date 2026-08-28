//! SunMao State Migration Fixture — Phase 2 acceptance fixture.
//!
//! M0 skeleton: a v1 tone-control effect with the plain Phase 1 parameter
//! state. M5 evolves the parameter set to v2 and exercises the versioned
//! state migration path (v1 blobs injected into a v2 plugin must load with
//! documented defaults for the new fields).

use sunmao::prelude::*;

/// Version 1 parameters. M5 adds a v2 field and a migration from this layout.
#[derive(Params)]
pub struct StateMigrationParams {
    /// Output level (linear).
    pub level: FloatParam,
}

impl Default for StateMigrationParams {
    fn default() -> Self {
        Self {
            level: FloatParam::new("level", "Level", 1.0, 0.0, 2.0),
        }
    }
}

/// The state migration fixture plugin.
pub struct StateMigrationPlugin {
    params: Arc<StateMigrationParams>,
}

impl Default for StateMigrationPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(StateMigrationParams::default()),
        }
    }
}

impl SunmaoPlugin for StateMigrationPlugin {
    const NAME: &'static str = "SunMao State Migration";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";

    type Params = StateMigrationParams;

    fn params(&self) -> Arc<Self::Params> {
        self.params.clone()
    }

    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        events: &EventQueue,
        _context: &ProcessContext,
    ) -> ProcessStatus {
        buffer.copy_input_to_output();

        let mut level = self.params.level.get();
        for change in events.param_changes() {
            if change.id == self.params.level.id {
                level = self.params.level.min
                    + change.value.clamp(0.0, 1.0)
                        * (self.params.level.max - self.params.level.min);
            }
        }

        for channel in 0..buffer.num_output_channels() {
            for sample in buffer.output(channel) {
                *sample *= level;
            }
        }
        ProcessStatus::Normal
    }

    fn vst3_info() -> Vst3Info {
        Vst3Info {
            class_id: *b"SunMaoFxStateMg!",
            categories: &["Fx", "Tools"],
            ..Default::default()
        }
    }

    fn clap_info() -> ClapInfo {
        ClapInfo {
            id: "com.sunmao.fx.state_migration",
            features: &["audio-effect", "utility", "stereo"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_level_parameter_scales_the_output() {
        let mut plugin = StateMigrationPlugin::default();
        plugin.params.level.set(0.5);

        let input_left = [1.0f32; 4];
        let input_right = [1.0f32; 4];
        let inputs: [&[f32]; 2] = [&input_left, &input_right];
        let mut output_left = [0.0f32; 4];
        let mut output_right = [0.0f32; 4];
        let mut outputs: [&mut [f32]; 2] = [&mut output_left, &mut output_right];
        let mut buffer = AudioBuffer::new(&inputs, &mut outputs, 4);
        let events = EventQueue::new();

        let status = plugin.process(
            &mut buffer,
            &events,
            &ProcessContext {
                sample_rate: 48_000.0,
                tempo: None,
                is_playing: true,
                sample_pos: 0,
                ..Default::default()
            },
        );

        assert_eq!(status, ProcessStatus::Normal);
        assert_eq!(output_left, [0.5; 4]);
    }
}

// ============ Unified VST3 + CLAP Export ============
sunmao::sunmao_export!(StateMigrationPlugin);
