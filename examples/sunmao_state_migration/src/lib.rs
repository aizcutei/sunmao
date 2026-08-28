//! SunMao State Migration Fixture — Phase 2 acceptance fixture.
//!
//! A v2 tone-control effect that must still load state written by its v1
//! build. v1 stored a linear `level`; v2 keeps it and adds `trim_db`, which
//! did not exist then and therefore loads at its default.

use sunmao::prelude::*;

/// Version 2 parameters. `level` existed in v1; `trim_db` is new.
#[derive(Params)]
pub struct StateMigrationParams {
    /// Output level (linear). Present since v1.
    pub level: FloatParam,
    /// Extra trim in dB. Added in v2, so a v1 state leaves it at 0.
    pub trim_db: FloatParam,
}

/// Default trim for a state written before `trim_db` existed.
pub const V1_TRIM_DEFAULT_DB: f32 = 0.0;

impl Default for StateMigrationParams {
    fn default() -> Self {
        Self {
            level: FloatParam::new("level", "Level", 1.0, 0.0, 2.0),
            trim_db: FloatParam::new("trim_db", "Trim", V1_TRIM_DEFAULT_DB, -24.0, 24.0),
        }
    }
}

/// The state migration fixture plugin.
pub struct StateMigrationPlugin {
    params: Arc<StateMigrationParams>,
    /// Version of the last state migrated into this instance, for tests.
    migrated_from: Option<u32>,
    /// Name of the last factory preset applied, for tests.
    loaded_preset: Option<&'static str>,
}

impl Default for StateMigrationPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(StateMigrationParams::default()),
            migrated_from: None,
            loaded_preset: None,
        }
    }
}

impl SunmaoPlugin for StateMigrationPlugin {
    const NAME: &'static str = "SunMao State Migration";
    const VENDOR: &'static str = "SunMao";
    const URL: &'static str = "https://aizcutei.github.io/sunmao";

    type Params = StateMigrationParams;

    const STATE_VERSION: u32 = 2;

    fn params(&self) -> Arc<Self::Params> {
        self.params.clone()
    }

    const SUPPORTS_PRESET_LOAD: bool = true;

    /// Applies a factory preset by name.
    ///
    /// Presets are just parameter state, so this fixture keeps them in the
    /// plugin rather than reading files: it exercises the host-facing contract
    /// (a named preset is applied, an unknown one is refused) without making
    /// the test depend on the filesystem.
    fn load_preset(&mut self, location: PresetLocation<'_>) -> bool {
        let key = match location {
            // A file preset is refused: this fixture ships no preset files, and
            // claiming success would tell the host a preset was applied when
            // nothing changed.
            PresetLocation::File { .. } => return false,
            PresetLocation::Internal { key } => key,
        };
        match key {
            Some("init") => {
                self.params.level.set(1.0);
                self.params.trim_db.set(V1_TRIM_DEFAULT_DB);
                self.loaded_preset = Some("init");
                true
            }
            Some("loud") => {
                self.params.level.set(2.0);
                self.params.trim_db.set(6.0);
                self.loaded_preset = Some("loud");
                true
            }
            _ => false,
        }
    }

    fn migrate_state(&mut self, from_version: u32) {
        self.migrated_from = Some(from_version);
        if from_version < 2 {
            // v1 had no trim; state written then must sound exactly as it did.
            self.params.trim_db.set(V1_TRIM_DEFAULT_DB);
        }
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

    #[test]
    fn a_v1_state_leaves_the_new_parameter_at_its_documented_default() {
        let mut plugin = StateMigrationPlugin::default();
        // A v1 host state only carried `level`; whatever `trim_db` happened to
        // hold beforehand must not survive as if the old host had set it.
        plugin.params.level.set(0.25);
        plugin.params.trim_db.set(9.0);

        plugin.migrate_state(1);

        assert_eq!(
            plugin.params.trim_db.get(),
            V1_TRIM_DEFAULT_DB,
            "a v1 state predates trim_db, so it must load at the default"
        );
        assert_eq!(
            plugin.params.level.get(),
            0.25,
            "a parameter that existed in v1 must survive migration"
        );
        assert_eq!(plugin.migrated_from, Some(1));
    }

    #[test]
    fn the_plugin_declares_the_current_state_version() {
        assert_eq!(StateMigrationPlugin::STATE_VERSION, 2);
    }

    #[test]
    fn a_named_factory_preset_is_applied() {
        let mut plugin = StateMigrationPlugin::default();
        assert!(plugin.load_preset(PresetLocation::Internal { key: Some("loud") }));
        assert_eq!(plugin.loaded_preset, Some("loud"));
        assert_eq!(plugin.params.level.get(), 2.0);
        assert_eq!(plugin.params.trim_db.get(), 6.0);

        assert!(plugin.load_preset(PresetLocation::Internal { key: Some("init") }));
        assert_eq!(plugin.params.level.get(), 1.0);
        assert_eq!(plugin.params.trim_db.get(), V1_TRIM_DEFAULT_DB);
    }

    #[test]
    fn an_unknown_or_file_preset_is_refused_rather_than_faked() {
        let mut plugin = StateMigrationPlugin::default();
        let before = plugin.params.level.get();

        // A host must be told the load failed, not left believing it worked.
        assert!(!plugin.load_preset(PresetLocation::Internal { key: Some("nope") }));
        assert!(!plugin.load_preset(PresetLocation::Internal { key: None }));
        assert!(!plugin.load_preset(PresetLocation::File {
            path: "/presets/whatever.clap-preset",
            key: None,
        }));

        assert_eq!(plugin.loaded_preset, None);
        assert_eq!(
            plugin.params.level.get(),
            before,
            "a refusal changes nothing"
        );
    }
}

// ============ Unified VST3 + CLAP Export ============
sunmao::sunmao_export!(StateMigrationPlugin);
