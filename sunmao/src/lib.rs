//! SunMao: An Audio Plugin Framework in Rust.
//!
//! This is the main entry point crate. It re-exports types from `sunmao_core` and `sunmao_macros`.
//!
//! ## Quick Start
//!
//! ```ignore
//! use sunmao::prelude::*;
//!
//! struct MyPlugin { params: Arc<MyParams> }
//! struct MyParams { gain: FloatParam }
//!
//! impl SunmaoPlugin for MyPlugin {
//!     const NAME: &'static str = "My Plugin";
//!     const VENDOR: &'static str = "My Company";
//!     const URL: &'static str = "https://example.com";
//!     type Params = MyParams;
//!     fn params(&self) -> Arc<Self::Params> { self.params.clone() }
//!     fn process(&mut self, buffer: &mut AudioBuffer, events: &EventQueue, ctx: &ProcessContext) -> ProcessStatus {
//!         ProcessStatus::Normal
//!     }
//! }
//!
//! sunmao_export!(MyPlugin);
//! ```

// Re-export everything from sunmao_core
pub use sunmao_core::*;

// Re-export macros
pub use sunmao_macros::*;

#[cfg(feature = "standalone")]
pub use sunmao_runtime;

/// Common imports for SunMao plugins.
pub mod prelude {
    pub use sunmao_core::plugin::{SunmaoPlugin, ProcessStatus, ProcessContext};
    pub use sunmao_core::params::{Params, FloatParam, IntParam, BoolParam};
    pub use sunmao_core::audio::AudioBuffer;
    pub use sunmao_core::events::{EventQueue, Event, MidiMessage};
    pub use sunmao_core::metadata::{Vst3Info, AuInfo, ClapInfo};
    pub use sunmao_macros::{Params, sunmao_export};
    pub use std::sync::Arc;
}
