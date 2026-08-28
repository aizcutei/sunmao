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

/// What a host-visible audio bus is used for.
///
/// The formats express this differently — VST3 has an explicit `kAux` bus
/// type, CLAP relies on the `is_main` flag and port order — so the unified API
/// names the role and lets each backend encode it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BusRole {
    /// The plugin's primary audio path.
    #[default]
    Main,
    /// A key/control input that is not part of the main signal path.
    Sidechain,
}

/// One declared audio bus.
///
/// ```
/// # use sunmao_core::plugin::{BusInfo, BusRole};
/// let key = BusInfo::sidechain("Sidechain", 2);
/// assert_eq!(key.role, BusRole::Sidechain);
/// assert_eq!(BusInfo::main("Input", 2).channels, 2);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusInfo {
    /// Host-visible bus name.
    pub name: &'static str,
    /// Channel count carried by this bus.
    pub channels: u32,
    /// What the bus is used for.
    pub role: BusRole,
}

impl BusInfo {
    /// Declares a main-path bus.
    pub const fn main(name: &'static str, channels: u32) -> Self {
        Self {
            name,
            channels,
            role: BusRole::Main,
        }
    }

    /// Declares a sidechain/key bus.
    pub const fn sidechain(name: &'static str, channels: u32) -> Self {
        Self {
            name,
            channels,
            role: BusRole::Sidechain,
        }
    }
}

/// Polyphony information a host can query.
///
/// CLAP exposes this through `clap.voice-info`; VST3 has no equivalent, so a
/// VST3 host simply never sees it.
///
/// ```
/// # use sunmao_core::plugin::VoiceInfo;
/// let info = VoiceInfo { active: 3, capacity: 8, supports_overlapping_notes: false };
/// assert!(info.active <= info.capacity);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceInfo {
    /// Voices currently sounding.
    pub active: u32,
    /// Largest number of simultaneous voices.
    pub capacity: u32,
    /// Whether several notes may sound on the same key at once.
    pub supports_overlapping_notes: bool,
}

/// How long a plugin keeps producing output after its input goes silent.
///
/// Both formats encode "infinite" with a magic number — VST3 uses
/// `kInfiniteTail` and CLAP treats anything at or above `i32::MAX` as
/// unbounded — so the unified API names the concept instead.
///
/// ```
/// # use sunmao_core::plugin::TailLength;
/// // A one-second reverb tail at 48 kHz.
/// let tail = TailLength::Samples(48_000);
/// assert_ne!(tail, TailLength::Infinite);
/// assert_eq!(TailLength::default(), TailLength::None);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TailLength {
    /// The plugin stops producing output as soon as its input does.
    #[default]
    None,
    /// The plugin keeps producing output for this many samples.
    Samples(u32),
    /// The plugin may produce output indefinitely (e.g. a feedback network).
    Infinite,
}

/// Whether the host is rendering in realtime or offline.
///
/// VST3 additionally distinguishes a prefetch mode; it maps to
/// [`RenderMode::Realtime`] because it still runs under realtime constraints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderMode {
    /// Normal realtime playback.
    #[default]
    Realtime,
    /// Offline rendering such as a bounce or export.
    Offline,
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

    /// Declares the plugin's input buses.
    ///
    /// The default derives a single main bus from
    /// [`SunmaoPlugin::input_channels`], so a plugin only overrides this when
    /// it needs a sidechain or several buses. Channel indices handed to
    /// [`AudioBuffer`] are the buses concatenated in declaration order.
    fn input_buses(&self) -> Vec<BusInfo> {
        match self.input_channels() {
            0 => Vec::new(),
            channels => vec![BusInfo::main("Input", channels)],
        }
    }

    /// Declares the plugin's output buses.
    fn output_buses(&self) -> Vec<BusInfo> {
        match self.output_channels() {
            0 => Vec::new(),
            channels => vec![BusInfo::main("Output", channels)],
        }
    }

    /// Called before processing starts. Use for initialization.
    fn initialize(&mut self, _sample_rate: f64, _max_block_size: u32) {}

    /// Called when processing stops.
    fn reset(&mut self) {}

    /// Processing delay in samples, so the host can compensate for it.
    ///
    /// Both formats only re-query this outside the processing state, so it
    /// must stay constant for as long as the plugin is processing; change it
    /// from [`SunmaoPlugin::initialize`] or [`SunmaoPlugin::set_render_mode`].
    fn latency_samples(&self) -> u32 {
        0
    }

    /// How long the plugin keeps producing output after its input stops.
    fn tail(&self) -> TailLength {
        TailLength::None
    }

    /// Polyphony information for hosts that ask for it.
    ///
    /// `None` means "not a voice-based plugin". VST3 hosts never see this.
    fn voice_info(&self) -> Option<VoiceInfo> {
        None
    }

    /// Called when the host switches between realtime and offline rendering.
    ///
    /// The host calls this outside processing, so it is a valid place to
    /// change [`SunmaoPlugin::latency_samples`] (e.g. to enable a longer
    /// lookahead when rendering offline).
    fn set_render_mode(&mut self, _mode: RenderMode) {}

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
