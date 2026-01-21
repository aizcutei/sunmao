//! vst3_rs - Safe Rust Wrapper for VST3 Plugin Development
//!
//! This crate provides a safe abstraction over vst3_sys, allowing plugin
//! developers to focus on plugin logic rather than COM vtable complexity.

pub mod plugin;
pub mod params;
pub mod process;
pub mod wrapper;
pub mod gui;

pub use plugin::{Plugin, PluginInfo, HostHandle, AudioConfig, PortType};
pub use params::ParamInfo;
pub use process::ProcessContext;
pub use gui::GuiPlugin;

/// Re-export vst3_sys for macro compatibility
/// Users don't need to add vst3_sys as a dependency
#[doc(hidden)]
pub use vst3_sys;
