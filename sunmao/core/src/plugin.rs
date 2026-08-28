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

/// One complete, host-selectable bus layout.
///
/// A plugin that can run in more than one channel layout — mono and stereo,
/// say — lists each one as a `BusConfig`. The two formats negotiate layouts in
/// opposite directions, and this type is what lets one declaration serve both:
///
/// - CLAP enumerates configurations and the host *selects* one by id
///   (`clap.audio-ports-config`), so the list is handed over as-is.
/// - VST3 has the host *propose* a speaker arrangement per bus
///   (`setBusArrangements`), so the backend searches this list for a matching
///   entry and accepts only if it finds one.
///
/// Every config must declare the same number of buses in each direction; only
/// channel counts may differ. A host that changes the *number* of buses is not
/// something either format expresses through these calls.
///
/// ```
/// # use sunmao_core::plugin::{BusConfig, BusInfo};
/// let mono = BusConfig::new("Mono", vec![BusInfo::main("Input", 1)], vec![BusInfo::main("Output", 1)]);
/// let stereo = BusConfig::new("Stereo", vec![BusInfo::main("Input", 2)], vec![BusInfo::main("Output", 2)]);
/// assert_eq!(mono.input_channel_counts(), vec![1]);
/// assert_eq!(stereo.input_channel_counts(), vec![2]);
/// // The two configs agree on bus counts, as every config must.
/// assert_eq!(mono.inputs.len(), stereo.inputs.len());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusConfig {
    /// Host-visible name for this layout, e.g. "Stereo".
    pub name: &'static str,
    /// Input buses in declaration order.
    pub inputs: Vec<BusInfo>,
    /// Output buses in declaration order.
    pub outputs: Vec<BusInfo>,
}

impl BusConfig {
    /// Declares a selectable layout.
    pub fn new(name: &'static str, inputs: Vec<BusInfo>, outputs: Vec<BusInfo>) -> Self {
        Self {
            name,
            inputs,
            outputs,
        }
    }

    /// Channel count of each input bus, in declaration order.
    pub fn input_channel_counts(&self) -> Vec<u32> {
        self.inputs.iter().map(|bus| bus.channels).collect()
    }

    /// Channel count of each output bus, in declaration order.
    pub fn output_channel_counts(&self) -> Vec<u32> {
        self.outputs.iter().map(|bus| bus.channels).collect()
    }

    /// Whether this layout is exactly the per-bus channel counts a host asked
    /// for. This is the VST3 `setBusArrangements` matching rule.
    pub fn matches(&self, input_channels: &[u32], output_channels: &[u32]) -> bool {
        self.input_channel_counts() == input_channels
            && self.output_channel_counts() == output_channels
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
    /// Version of this plugin's parameter state layout.
    ///
    /// Bump it whenever the *meaning* of an existing parameter changes.
    /// Adding or removing parameters does not require a bump: state entries
    /// are matched by parameter id, so a state written by an older build
    /// restores the parameters it knew and leaves new ones at their defaults.
    const STATE_VERSION: u32 = 1;

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

    /// Called when the host activates or deactivates one declared audio bus.
    ///
    /// `bus_index` indexes [`SunmaoPlugin::input_buses`] or
    /// [`SunmaoPlugin::output_buses`], the same numbering
    /// [`AudioBuffer::input_bus`] uses. Both backends validate the index
    /// against those declarations before calling, so it always names a real
    /// bus. Returning `false` rejects the request.
    ///
    /// Both backends only call this while the plugin is inactive, so it is a
    /// valid place to reconfigure processing state. A deactivated input bus
    /// still occupies its buffer slot rather than disappearing, so a plugin
    /// that ignores this callback keeps working; use it to skip work — and to
    /// stop reading a key bus the host has switched off.
    ///
    /// Format notes: VST3 delivers this through `IComponent::activateBus`,
    /// CLAP through `clap.audio-ports-activation/2`. See
    /// `docs/phase2/semantics.md` for the exact differences.
    ///
    /// ```
    /// # use sunmao_core::plugin::{BusInfo, BusRole};
    /// let buses = vec![BusInfo::main("Input", 2), BusInfo::sidechain("Sidechain", 2)];
    /// // `bus_index` indexes the declaration list, so bus 1 is the sidechain.
    /// assert_eq!(buses[1].role, BusRole::Sidechain);
    /// ```
    fn set_bus_active(&mut self, _is_input: bool, _bus_index: u32, _active: bool) -> bool {
        true
    }

    /// Alternative bus layouts this plugin can run in.
    ///
    /// The default is empty, meaning the plugin runs only in the single layout
    /// its [`SunmaoPlugin::input_buses`]/[`SunmaoPlugin::output_buses`]
    /// declare — a host cannot renegotiate it. Returning two or more configs
    /// opts into negotiation. The entry named by
    /// [`SunmaoPlugin::current_bus_config`] is the one in force, and before the
    /// host selects anything it must agree with what
    /// `input_buses()`/`output_buses()` report; the list order itself carries
    /// no meaning beyond being the ids hosts use.
    ///
    /// A plugin that overrides this must also honour
    /// [`SunmaoPlugin::select_bus_config`] by making `input_buses()` and
    /// `output_buses()` reflect the selected config — those two are what the
    /// backends use to lay out the audio buffer.
    ///
    /// ```
    /// # use sunmao_core::plugin::{BusConfig, BusInfo};
    /// let configs = vec![
    ///     BusConfig::new("Mono", vec![BusInfo::main("In", 1)], vec![BusInfo::main("Out", 1)]),
    ///     BusConfig::new("Stereo", vec![BusInfo::main("In", 2)], vec![BusInfo::main("Out", 2)]),
    /// ];
    /// // A VST3 host proposing stereo in / stereo out matches entry 1.
    /// assert!(configs[1].matches(&[2], &[2]));
    /// assert!(!configs[0].matches(&[2], &[2]));
    /// ```
    fn bus_configs(&self) -> Vec<BusConfig> {
        Vec::new()
    }

    /// The index into [`SunmaoPlugin::bus_configs`] the plugin is currently in.
    ///
    /// Defaults to 0, the declared default layout.
    fn current_bus_config(&self) -> usize {
        0
    }

    /// Called when the host selects one of [`SunmaoPlugin::bus_configs`].
    ///
    /// `index` is always in range — both backends validate it against the
    /// declared list first. Returning `false` rejects the layout and leaves the
    /// previous one in force; the backends report that refusal to the host
    /// rather than pretending the switch happened.
    ///
    /// After accepting, `input_buses()`/`output_buses()` must report the new
    /// layout. Both formats only negotiate while the plugin is inactive, so
    /// reallocating here is safe.
    ///
    /// Format notes: CLAP hosts call this directly through
    /// `clap.audio-ports-config`'s `select`. VST3 hosts instead propose speaker
    /// arrangements through `setBusArrangements`, and the backend translates
    /// that into the matching index — so a VST3 host can only reach layouts
    /// listed here. See `docs/phase2/semantics.md`.
    fn select_bus_config(&mut self, _index: usize) -> bool {
        false
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

    /// Called after a state written by an older build has been applied.
    ///
    /// `from_version` is that build's [`SunmaoPlugin::STATE_VERSION`], always
    /// lower than this build's. Use it to reinterpret values whose meaning
    /// changed; parameters that simply did not exist yet already hold their
    /// defaults and need no action.
    fn migrate_state(&mut self, _from_version: u32) {}

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
