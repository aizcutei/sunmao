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

/// Format adapters used by the unified export macro.
pub use sunmao_backend_clap as backend_clap;
pub use sunmao_backend_vst3 as backend_vst3;

#[cfg(feature = "standalone")]
pub use sunmao_runtime;

/// Common imports for SunMao plugins.
pub mod prelude {
    pub use std::sync::Arc;
    pub use sunmao_core::audio::AudioBuffer;
    pub use sunmao_core::events::{Event, EventQueue, MidiMessage, ParamChange};
    pub use sunmao_core::metadata::{AuInfo, ClapInfo, Vst3Info, Vst3SpeakerLayout};
    pub use sunmao_core::params::{
        stable_param_id, BoolParam, FloatParam, IntParam, ParamDescriptor, ParamKind, Params,
    };
    pub use sunmao_core::plugin::{ProcessContext, ProcessStatus, SunmaoPlugin};
    pub use sunmao_macros::{sunmao_export, Params};
}
