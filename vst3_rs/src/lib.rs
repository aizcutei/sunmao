//! vst3_rs - Safe Rust Wrapper for VST3 Plugin Development
//!
//! This crate provides a safe abstraction over vst3_sys, allowing plugin
//! developers to focus on plugin logic rather than COM vtable complexity.

pub mod gui;
pub mod params;
pub mod plugin;
pub mod process;
mod state;
pub mod wrapper;

pub use gui::GuiPlugin;
pub use params::ParamInfo;
pub use plugin::{
    AudioConfig, HostHandle, ParameterBridge, Plugin, PluginInfo, PortConfig, PortType, RenderMode,
    class_id_from_str,
};
pub use process::{ParamChange, ProcessContext, ProcessError, ProcessResult};

/// Re-export vst3_sys for macro compatibility
/// Users don't need to add vst3_sys as a dependency
#[doc(hidden)]
pub use vst3_sys;
