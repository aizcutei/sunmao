//! Core traits and data structures for SunMao.
//!
//! This crate defines the core abstractions that plugins implement.

pub mod audio;
pub mod events;
pub mod metadata;
pub mod params;
pub mod plugin;
pub mod smoothing;
pub mod view;
pub mod viz;

// Re-exports for convenience
pub use audio::AudioBuffer;
pub use events::{Event, EventQueue, MidiMessage, ParamChange};
pub use metadata::{derive_clap_id, AuInfo, ClapInfo, Vst3Info, Vst3SpeakerLayout};
pub use params::{
    stable_param_id, validate_param_layout, BoolParam, FloatParam, IntParam, ParamDescriptor,
    ParamKind, ParamLayoutError, Params,
};
pub use plugin::{ProcessContext, ProcessStatus, SunmaoPlugin};
pub use view::{
    ParamsViewContext, ParentWindow, StandaloneViewOptions, StandaloneViewResult, SunmaoView,
    ViewContext, ViewHandle, ViewKey, ViewKeyCode,
};
pub use viz::{viz_channel, VizChannel, VizConsumer, VizFrame, VizPublisher};

/// Common imports for SunMao plugins.
pub mod prelude {
    pub use crate::audio::AudioBuffer;
    pub use crate::events::{Event, EventQueue, MidiMessage, ParamChange};
    pub use crate::metadata::{derive_clap_id, AuInfo, ClapInfo, Vst3Info, Vst3SpeakerLayout};
    pub use crate::params::{
        stable_param_id, validate_param_layout, BoolParam, FloatParam, IntParam, ParamDescriptor,
        ParamKind, ParamLayoutError, Params,
    };
    pub use crate::plugin::{
        BusConfig, BusInfo, BusRole, PresetLocation, ProcessContext, ProcessStatus, SunmaoPlugin,
    };
    pub use crate::smoothing::{Smoother, SmoothingStyle};
    pub use crate::view::{
        ParamsViewContext, ParentWindow, StandaloneViewOptions, StandaloneViewResult, SunmaoView,
        ViewContext, ViewHandle,
    };
    pub use std::sync::Arc;
}
