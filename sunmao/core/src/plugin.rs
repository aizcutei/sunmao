//! Plugin trait definition.

use crate::audio::AudioBuffer;
use crate::events::EventQueue;
use crate::metadata::{AuInfo, ClapInfo, Vst3Info};
use crate::params::Params;
use crate::view::SunmaoView;
use std::sync::Arc;

/// Processing status returned by the plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessStatus {
    /// Normal processing, plugin is active.
    Normal,
    /// Plugin wants to signal it has finished (e.g., tail finished).
    Tail(u32),
    /// Plugin encountered an error.
    Error,
}

/// Context information provided during processing.
///
/// Every musical field is optional because both VST3 and CLAP let the host
/// declare which parts of its timeline are valid; a `None` means "the host did
/// not provide this", never "zero". Beat positions are expressed in quarter
/// notes, which is the native unit of both formats.
///
/// ```
/// # use sunmao_core::plugin::ProcessContext;
/// let context = ProcessContext {
///     sample_rate: 48_000.0,
///     tempo: Some(120.0),
///     time_signature: Some((3, 4)),
///     ..Default::default()
/// };
/// // One bar of 3/4 at 120 BPM lasts three quarter notes.
/// let beats_per_bar = context.time_signature.map(|(num, _)| f64::from(num));
/// assert_eq!(beats_per_bar, Some(3.0));
/// assert_eq!(context.bar_number, None);
/// ```
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProcessContext {
    /// Current sample rate in Hz.
    pub sample_rate: f64,
    /// Current tempo in BPM (if available).
    pub tempo: Option<f64>,
    /// Whether the transport is playing.
    pub is_playing: bool,
    /// Whether the host is recording.
    pub is_recording: bool,
    /// Whether the host's loop/cycle region is active.
    pub is_loop_active: bool,
    /// Current position in samples from the start.
    pub sample_pos: i64,
    /// Time signature as `(numerator, denominator)`.
    pub time_signature: Option<(u16, u16)>,
    /// Song position in quarter notes.
    pub song_pos_beats: Option<f64>,
    /// Song position in seconds.
    pub song_pos_seconds: Option<f64>,
    /// Musical start of the current bar, in quarter notes.
    pub bar_start_beats: Option<f64>,
    /// Index of the current bar. CLAP-only; VST3 hosts report `None`.
    pub bar_number: Option<i32>,
    /// Active loop region as `(start, end)` in quarter notes.
    pub loop_beats: Option<(f64, f64)>,
}

/// The core plugin trait that developers implement.
///
/// This trait defines the interface between your plugin logic and the host.
pub trait SunmaoPlugin: Default + Send + 'static {
    /// Human-readable plugin name.
    const NAME: &'static str;
    /// Vendor/company name.
    const VENDOR: &'static str;
    /// Plugin URL.
    const URL: &'static str;
    /// Plugin version string.
    const VERSION: &'static str = "1.0.0";
    /// Maximum number of host events accepted in one processing block.
    ///
    /// Format adapters allocate this scratch during activation and report a
    /// processing error instead of growing it on the audio thread.
    const MAX_EVENTS_PER_BLOCK: usize = 4096;

    /// The parameter struct type.
    type Params: Params;

    /// Number of input channels (0 for synths).
    fn input_channels(&self) -> u32 {
        2
    }
    /// Number of output channels.
    fn output_channels(&self) -> u32 {
        2
    }
    /// Whether this plugin accepts MIDI input.
    fn accepts_midi(&self) -> bool {
        false
    }

    /// Get the plugin's parameters.
    fn params(&self) -> Arc<Self::Params>;

    /// Called before processing starts. Use for initialization.
    fn initialize(&mut self, _sample_rate: f64, _max_block_size: u32) {}

    /// Called when processing stops.
    fn reset(&mut self) {}

    /// Main audio processing callback.
    ///
    /// Process the audio in `buffer` and handle events from `events`.
    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        events: &EventQueue,
        context: &ProcessContext,
    ) -> ProcessStatus;

    // --- GUI Support ---

    /// Create the plugin's editor view.
    ///
    /// Return `Some(view)` if this plugin has a custom GUI.
    /// Return `None` to use the host's generic parameter UI.
    fn view(&self) -> Option<Box<dyn SunmaoView>> {
        None
    }

    // --- Format-specific metadata (with defaults) ---

    /// VST3-specific metadata.
    fn vst3_info() -> Vst3Info {
        Vst3Info::default()
    }
    /// Audio Unit-specific metadata.
    fn au_info() -> AuInfo {
        AuInfo::default()
    }
    /// CLAP-specific metadata.
    fn clap_info() -> ClapInfo {
        ClapInfo::default()
    }
}
