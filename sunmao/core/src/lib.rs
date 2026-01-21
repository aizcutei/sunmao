//! Core traits and data structures for SunMao.
//!
//! This crate defines the core abstractions that plugins implement.

use std::sync::Arc;

pub mod plugin;
pub mod params;
pub mod audio;
pub mod events;
pub mod metadata;
pub mod view;

// Re-exports for convenience
pub use plugin::{SunmaoPlugin, ProcessStatus, ProcessContext};
pub use params::{Params, FloatParam, IntParam, BoolParam};
pub use audio::AudioBuffer;
pub use events::{EventQueue, Event, MidiMessage};
pub use metadata::{Vst3Info, AuInfo, ClapInfo};
pub use view::{SunmaoView, ParentWindow, ViewContext, ViewHandle};

/// Common imports for SunMao plugins.
pub mod prelude {
    pub use crate::plugin::{SunmaoPlugin, ProcessStatus, ProcessContext};
    pub use crate::params::{Params, FloatParam, IntParam, BoolParam};
    pub use crate::audio::AudioBuffer;
    pub use crate::events::{EventQueue, Event, MidiMessage};
    pub use crate::metadata::{Vst3Info, AuInfo, ClapInfo};
    pub use crate::view::{SunmaoView, ParentWindow, ViewContext, ViewHandle};
    pub use std::sync::Arc;
}

