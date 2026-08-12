//! Core traits and data structures for SunMao.
//!
//! This crate defines the core abstractions that plugins implement.

pub mod audio;
pub mod events;
pub mod metadata;
pub mod params;
pub mod plugin;
pub mod view;

// Re-exports for convenience
pub use audio::AudioBuffer;
pub use events::{Event, EventQueue, MidiMessage, ParamChange};
pub use metadata::{AuInfo, ClapInfo, Vst3Info, Vst3SpeakerLayout};
pub use params::{
    stable_param_id, BoolParam, FloatParam, IntParam, ParamDescriptor, ParamKind, Params,
};
pub use plugin::{ProcessContext, ProcessStatus, SunmaoPlugin};
pub use view::{ParentWindow, SunmaoView, ViewContext, ViewHandle};

/// Common imports for SunMao plugins.
pub mod prelude {
    pub use crate::audio::AudioBuffer;
    pub use crate::events::{Event, EventQueue, MidiMessage, ParamChange};
    pub use crate::metadata::{AuInfo, ClapInfo, Vst3Info, Vst3SpeakerLayout};
    pub use crate::params::{
        stable_param_id, BoolParam, FloatParam, IntParam, ParamDescriptor, ParamKind, Params,
    };
    pub use crate::plugin::{ProcessContext, ProcessStatus, SunmaoPlugin};
    pub use crate::view::{ParentWindow, SunmaoView, ViewContext, ViewHandle};
    pub use std::sync::Arc;
}
