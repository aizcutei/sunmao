pub mod clap_host;
pub mod scanner;
pub mod vst3_host;

#[cfg(target_os = "macos")]
pub mod au_host;

use crate::gui_window::PluginGuiWindow;
use std::path::Path;

/// Returns whether a file can be a platform VST3 module inside a bundle.
///
/// Windows VST3 modules conventionally use a `.vst3` suffix even though they
/// are loadable DLLs. Accepting only `.dll` makes correctly packaged Windows
/// bundles invisible to the scanner and host. Standard macOS bundles use an
/// extensionless executable under `Contents/MacOS`.
pub(crate) fn is_vst3_module(path: &Path) -> bool {
    match path.extension().and_then(|value| value.to_str()) {
        None => path.file_name().is_some(),
        Some(extension) => ["dylib", "so", "dll", "vst3"]
            .iter()
            .any(|expected| extension.eq_ignore_ascii_case(expected)),
    }
}

#[cfg(target_os = "linux")]
const PLUGIN_DLOPEN_FLAGS: libc::c_int = libc::RTLD_LAZY | libc::RTLD_LOCAL | libc::RTLD_NODELETE;

/// Load a plugin module while preserving the platform's expected module lifetime.
///
/// WebKitGTK and similar GUI stacks keep process-global state that is
/// not safe to tear down through `dlclose` and initialize again in the same
/// host. Real plugin hosts commonly keep modules mapped until process exit, so
/// mirror that behavior while retaining normal `Library` ownership elsewhere.
pub(crate) unsafe fn load_plugin_library(
    path: &Path,
) -> Result<libloading::Library, libloading::Error> {
    #[cfg(target_os = "linux")]
    {
        let library =
            unsafe { libloading::os::unix::Library::open(Some(path), PLUGIN_DLOPEN_FLAGS)? };
        Ok(library.into())
    }

    #[cfg(not(target_os = "linux"))]
    {
        unsafe { libloading::Library::new(path) }
    }
}

/// Information about a discovered plugin.
#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub id: String,
    pub path: String,
    pub format: PluginFormat,
    /// Factory class index for formats that expose multiple plugin classes.
    pub class_index: u32,
    pub input_channels: u32,
    pub output_channels: u32,
    pub is_synth: bool,
}

/// A timestamped event delivered to a plugin during one process block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HostEvent {
    NoteOn {
        sample_offset: u32,
        channel: u8,
        pitch: u8,
        velocity: f32,
    },
    NoteOff {
        sample_offset: u32,
        channel: u8,
        pitch: u8,
        velocity: f32,
    },
    ParamValue {
        sample_offset: u32,
        id: u32,
        value: f64,
    },
}

impl HostEvent {
    pub fn sample_offset(self) -> u32 {
        match self {
            Self::NoteOn { sample_offset, .. } | Self::NoteOff { sample_offset, .. } => {
                sample_offset
            }
            Self::ParamValue { sample_offset, .. } => sample_offset,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginFormat {
    CLAP,
    VST3,
    AU,
}

/// Host-observed parameter callbacks emitted by a plugin GUI gesture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuiGestureEvidence {
    pub begin_count: usize,
    pub value_count: usize,
    pub end_count: usize,
    pub last_param_id: u32,
    pub last_value: f64,
    pub completed_count: usize,
    pub last_completed_param_id: u32,
    pub last_completed_value: f64,
}

impl std::fmt::Display for PluginFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginFormat::CLAP => write!(f, "CLAP"),
            PluginFormat::VST3 => write!(f, "VST3"),
            PluginFormat::AU => write!(f, "AU"),
        }
    }
}

/// Result of a test run.
#[derive(Debug)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub message: String,
}

impl TestResult {
    pub fn pass(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: true,
            message: String::new(),
        }
    }
    pub fn fail(name: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: false,
            message: msg.into(),
        }
    }
}

/// Unified trait for host-side plugin instances.
pub trait HostPlugin: Send {
    /// Get plugin info.
    fn info(&self) -> &PluginInfo;

    /// Initialize the plugin (call once after load).
    fn initialize(&mut self, sample_rate: f64, max_frames: u32) -> Result<(), String>;

    /// Get the number of parameters.
    fn param_count(&self) -> u32;

    /// Get parameter info by index.
    fn param_info(&self, index: u32) -> Option<ParamInfo>;

    /// Get parameter value by id.
    fn param_get(&self, id: u32) -> Option<f64>;

    /// Set parameter value by id.
    fn param_set(&mut self, id: u32, value: f64) -> Result<(), String>;

    /// Process interleaved audio using the channel counts reported by [`Self::info`].
    fn process(&mut self, input: &[f32], output: &mut [f32]) -> Result<(), String>;

    /// Process interleaved audio with timestamped note and parameter events.
    ///
    /// Formats without event support keep the existing audio-only behavior and
    /// report a clear error only when events are actually supplied.
    fn process_with_events(
        &mut self,
        input: &[f32],
        output: &mut [f32],
        events: &[HostEvent],
    ) -> Result<(), String> {
        if events.is_empty() {
            self.process(input, output)
        } else {
            Err(format!(
                "{} host does not support input events",
                self.info().format
            ))
        }
    }

    /// Reset the plugin state.
    fn reset(&mut self) -> Result<(), String>;

    /// Save plugin state to bytes.
    fn save_state(&mut self) -> Result<Vec<u8>, String>;

    /// Load plugin state from bytes.
    fn load_state(&mut self, data: &[u8]) -> Result<(), String>;

    /// Shutdown and release the plugin.
    fn shutdown(&mut self);

    /// Service deferred host callbacks such as CLAP parameter flush requests.
    fn service_host_requests(&mut self) -> Result<(), String> {
        Ok(())
    }

    /// Snapshot parameter gesture callbacks received by the host.
    fn gui_gesture_evidence(&self) -> Option<GuiGestureEvidence> {
        None
    }

    /// Open the plugin's native GUI in a window.
    fn open_gui(&mut self, _window: &PluginGuiWindow) -> Result<(), String> {
        Err(format!("{} does not support GUI", self.info().format))
    }

    /// Negotiate and apply a host-driven editor resize.
    fn resize_gui(&mut self, _width: u32, _height: u32) -> Result<(u32, u32), String> {
        Err(format!(
            "{} does not support host-driven GUI resizing",
            self.info().format
        ))
    }

    /// Close the plugin's native GUI.
    fn close_gui(&mut self) {}
}

pub(crate) fn process_frame_count(
    input_len: usize,
    input_channels: usize,
    output_len: usize,
    output_channels: usize,
    max_frames: usize,
) -> Result<usize, String> {
    if output_channels == 0 {
        return Err("plugin has no audio output channels".into());
    }
    if output_len % output_channels != 0 {
        return Err(format!(
            "output buffer has {} samples, not divisible by {} channels",
            output_len, output_channels
        ));
    }

    let frames = output_len / output_channels;
    if frames > i32::MAX as usize {
        return Err(format!("process block has too many frames: {}", frames));
    }
    if frames > max_frames {
        return Err(format!(
            "process block has {} frames, exceeding maximum {}",
            frames, max_frames
        ));
    }

    if input_channels == 0 {
        if input_len != 0 {
            return Err(format!(
                "plugin has no audio inputs but received {} samples",
                input_len
            ));
        }
    } else {
        if input_len % input_channels != 0 {
            return Err(format!(
                "input buffer has {} samples, not divisible by {} channels",
                input_len, input_channels
            ));
        }
        let input_frames = input_len / input_channels;
        if input_frames != frames {
            return Err(format!(
                "input/output frame mismatch: {} input, {} output",
                input_frames, frames
            ));
        }
    }

    Ok(frames)
}

pub(crate) fn validate_host_events(events: &[HostEvent], frames: usize) -> Result<(), String> {
    let mut previous_offset = 0;
    for (index, event) in events.iter().copied().enumerate() {
        let sample_offset = event.sample_offset();
        match event {
            HostEvent::NoteOn {
                channel,
                pitch,
                velocity,
                ..
            }
            | HostEvent::NoteOff {
                channel,
                pitch,
                velocity,
                ..
            } => {
                if channel > 15 {
                    return Err(format!(
                        "event {} has invalid MIDI channel {}",
                        index, channel
                    ));
                }
                if pitch > 127 {
                    return Err(format!("event {} has invalid MIDI pitch {}", index, pitch));
                }
                if !velocity.is_finite() || !(0.0..=1.0).contains(&velocity) {
                    return Err(format!("event {} has invalid velocity {}", index, velocity));
                }
            }
            HostEvent::ParamValue { id, value, .. } => {
                if id == u32::MAX {
                    return Err(format!("event {} has an invalid parameter ID", index));
                }
                if !value.is_finite() {
                    return Err(format!(
                        "event {} has a non-finite parameter value {}",
                        index, value
                    ));
                }
            }
        }

        if sample_offset as usize >= frames {
            return Err(format!(
                "event {} offset {} is outside {}-frame block",
                index, sample_offset, frames
            ));
        }
        if index > 0 && sample_offset < previous_offset {
            return Err("input events must be ordered by sample offset".into());
        }
        previous_offset = sample_offset;
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ParamInfo {
    pub id: u32,
    pub name: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    pub is_stepped: bool,
    pub can_automate: bool,
}

pub struct ParameterSnapshot {
    id: u32,
    name: String,
    value: f64,
    alternate: f64,
}

pub fn capture_parameter_snapshot(
    plugin: &dyn HostPlugin,
) -> Result<Vec<ParameterSnapshot>, String> {
    let mut snapshot = Vec::with_capacity(plugin.param_count() as usize);
    for index in 0..plugin.param_count() {
        let info = plugin
            .param_info(index)
            .ok_or_else(|| format!("missing parameter info at index {}", index))?;
        let value = plugin
            .param_get(info.id)
            .ok_or_else(|| format!("failed to read parameter {}", info.name))?;
        if !value.is_finite() || !info.min.is_finite() || !info.max.is_finite() {
            return Err(format!(
                "parameter {} has non-finite metadata/value",
                info.name
            ));
        }
        let alternate = if values_match(value, info.min) {
            info.max
        } else {
            info.min
        };
        snapshot.push(ParameterSnapshot {
            id: info.id,
            name: info.name,
            value,
            alternate,
        });
    }
    Ok(snapshot)
}

pub fn overwrite_parameter_snapshot(
    plugin: &mut dyn HostPlugin,
    snapshot: &[ParameterSnapshot],
) -> Result<(), String> {
    for parameter in snapshot {
        if values_match(parameter.value, parameter.alternate) {
            continue;
        }
        plugin.param_set(parameter.id, parameter.alternate)?;
        let actual = plugin
            .param_get(parameter.id)
            .ok_or_else(|| format!("failed to re-read parameter {}", parameter.name))?;
        if !values_match(actual, parameter.alternate) {
            return Err(format!(
                "parameter {} did not change: expected {}, got {}",
                parameter.name, parameter.alternate, actual
            ));
        }
    }
    Ok(())
}

pub fn verify_parameter_snapshot(
    plugin: &dyn HostPlugin,
    snapshot: &[ParameterSnapshot],
) -> Result<(), String> {
    for parameter in snapshot {
        let actual = plugin
            .param_get(parameter.id)
            .ok_or_else(|| format!("failed to read restored parameter {}", parameter.name))?;
        if !values_match(actual, parameter.value) {
            return Err(format!(
                "parameter {} was not restored: expected {}, got {}",
                parameter.name, parameter.value, actual
            ));
        }
    }
    Ok(())
}

fn values_match(left: f64, right: f64) -> bool {
    let scale = left.abs().max(right.abs()).max(1.0);
    (left - right).abs() <= 1.0e-6 * scale
}

#[cfg(test)]
mod tests {
    use super::{
        is_vst3_module, process_frame_count, validate_host_events, HostEvent, HostPlugin,
        ParamInfo, PluginFormat, PluginInfo,
    };
    use std::path::Path;

    struct AudioOnlyHost {
        info: PluginInfo,
        process_calls: usize,
    }

    impl AudioOnlyHost {
        fn new() -> Self {
            Self {
                info: PluginInfo {
                    name: "audio-only".into(),
                    vendor: String::new(),
                    version: String::new(),
                    id: "audio-only".into(),
                    path: String::new(),
                    format: PluginFormat::AU,
                    class_index: 0,
                    input_channels: 2,
                    output_channels: 2,
                    is_synth: false,
                },
                process_calls: 0,
            }
        }
    }

    impl HostPlugin for AudioOnlyHost {
        fn info(&self) -> &PluginInfo {
            &self.info
        }

        fn initialize(&mut self, _sample_rate: f64, _max_frames: u32) -> Result<(), String> {
            Ok(())
        }

        fn param_count(&self) -> u32 {
            0
        }

        fn param_info(&self, _index: u32) -> Option<ParamInfo> {
            None
        }

        fn param_get(&self, _id: u32) -> Option<f64> {
            None
        }

        fn param_set(&mut self, _id: u32, _value: f64) -> Result<(), String> {
            Ok(())
        }

        fn process(&mut self, _input: &[f32], _output: &mut [f32]) -> Result<(), String> {
            self.process_calls += 1;
            Ok(())
        }

        fn reset(&mut self) -> Result<(), String> {
            Ok(())
        }

        fn save_state(&mut self) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }

        fn load_state(&mut self, _data: &[u8]) -> Result<(), String> {
            Ok(())
        }

        fn shutdown(&mut self) {}
    }

    #[test]
    fn recognizes_native_and_windows_vst3_module_suffixes() {
        assert!(is_vst3_module(Path::new("Plugin.dylib")));
        assert!(is_vst3_module(Path::new("Plugin.so")));
        assert!(is_vst3_module(Path::new("Plugin.dll")));
        assert!(is_vst3_module(Path::new("Plugin.vst3")));
        assert!(is_vst3_module(Path::new("Plugin.VST3")));
        assert!(is_vst3_module(Path::new("Plugin")));
        assert!(!is_vst3_module(Path::new("Info.plist")));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn native_plugin_modules_are_not_unloaded_mid_process() {
        assert_ne!(super::PLUGIN_DLOPEN_FLAGS & libc::RTLD_NODELETE, 0);
        assert_eq!(super::PLUGIN_DLOPEN_FLAGS & libc::RTLD_GLOBAL, 0);
    }

    #[test]
    fn frame_count_accepts_zero_input_synth_buffers() {
        assert_eq!(process_frame_count(0, 0, 1024, 2, 512), Ok(512));
    }

    #[test]
    fn frame_count_rejects_mismatched_and_oversized_buffers() {
        assert!(process_frame_count(1024, 2, 511, 2, 512).is_err());
        assert!(process_frame_count(1022, 2, 1024, 2, 512).is_err());
        assert!(process_frame_count(0, 0, 1026, 2, 512).is_err());
    }

    #[test]
    fn host_event_validation_preserves_ordered_sample_offsets() {
        let events = [
            HostEvent::NoteOn {
                sample_offset: 17,
                channel: 0,
                pitch: 60,
                velocity: 0.8,
            },
            HostEvent::NoteOff {
                sample_offset: 31,
                channel: 0,
                pitch: 60,
                velocity: 0.0,
            },
        ];
        assert_eq!(events[0].sample_offset(), 17);
        assert!(validate_host_events(&events, 64).is_ok());
        assert!(validate_host_events(&events, 31).is_err());
        assert!(validate_host_events(&[events[1], events[0]], 64).is_err());
    }

    #[test]
    fn host_event_validation_accepts_parameter_points_and_rejects_invalid_values() {
        let events = [
            HostEvent::ParamValue {
                sample_offset: 17,
                id: 42,
                value: 0.25,
            },
            HostEvent::ParamValue {
                sample_offset: 31,
                id: 42,
                value: 0.75,
            },
        ];
        assert!(validate_host_events(&events, 64).is_ok());
        assert!(validate_host_events(&events, 31).is_err());
        assert!(validate_host_events(&[events[1], events[0]], 64).is_err());
        assert!(validate_host_events(
            &[HostEvent::ParamValue {
                sample_offset: 0,
                id: u32::MAX,
                value: 0.5,
            }],
            1,
        )
        .is_err());
        assert!(validate_host_events(
            &[HostEvent::ParamValue {
                sample_offset: 0,
                id: 42,
                value: f64::NAN,
            }],
            1,
        )
        .is_err());
    }

    #[test]
    fn audio_only_hosts_keep_default_event_compatibility() {
        let mut host = AudioOnlyHost::new();
        assert!(host.process_with_events(&[], &mut [], &[]).is_ok());
        assert_eq!(host.process_calls, 1);

        let event = HostEvent::NoteOn {
            sample_offset: 0,
            channel: 0,
            pitch: 60,
            velocity: 1.0,
        };
        let error = host
            .process_with_events(&[], &mut [], &[event])
            .unwrap_err();
        assert!(error.contains("does not support input events"));
        assert_eq!(host.process_calls, 1);
    }
}
