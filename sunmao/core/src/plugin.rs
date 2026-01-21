//! Plugin trait definition.

use std::sync::Arc;
use crate::params::Params;
use crate::audio::AudioBuffer;
use crate::events::EventQueue;
use crate::metadata::{Vst3Info, AuInfo, ClapInfo};
use crate::view::SunmaoView;

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
pub struct ProcessContext {
    /// Current sample rate in Hz.
    pub sample_rate: f64,
    /// Current tempo in BPM (if available).
    pub tempo: Option<f64>,
    /// Whether the transport is playing.
    pub is_playing: bool,
    /// Current position in samples from the start.
    pub sample_pos: i64,
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

    /// The parameter struct type.
    type Params: Params;

    /// Number of input channels (0 for synths).
    fn input_channels(&self) -> u32 { 2 }
    /// Number of output channels.
    fn output_channels(&self) -> u32 { 2 }
    /// Whether this plugin accepts MIDI input.
    fn accepts_midi(&self) -> bool { false }

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
    fn vst3_info() -> Vst3Info { Vst3Info::default() }
    /// Audio Unit-specific metadata.
    fn au_info() -> AuInfo { AuInfo::default() }
    /// CLAP-specific metadata.
    fn clap_info() -> ClapInfo { ClapInfo::default() }
}

