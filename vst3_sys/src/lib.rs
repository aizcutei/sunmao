//! Pure Rust bindings for VST3 plugin creation.
//!
//! This crate provides low-level VST3 interface definitions without linking to
//! the original C++ SDK.
//!
//! # Module Structure
//! - `base` - Fundamental types, IUnknown, IPluginBase, IPluginFactory, IBStream
//! - `vst` - Audio/event interfaces: IComponent, IAudioProcessor, IEditController
//! - `gui` - View interfaces: IPlugView, IPlugFrame

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

pub mod base;
pub mod gui;
pub mod vst;

// Re-export commonly used items at crate root
pub use base::ibstream::*;
pub use base::iid as base_iid;
pub use base::ipluginbase::*;
pub use base::types::*;

pub use vst::iaudioprocessor::*;
pub use vst::icomponent::*;
pub use vst::ieditcontroller::*;
pub use vst::ievents::*;
pub use vst::iid as vst_iid;
pub use vst::iparameters::*;
pub use vst::ivstmessage::*;
pub use vst::processcontext::*;
pub use vst::types::*;

pub use gui::iid as gui_iid;
pub use gui::iplugview::*;

// Unified IID module for convenience
pub mod iid {
    pub use crate::base::iid::*;
    pub use crate::gui::iid::*;
    pub use crate::vst::iid::*;
}
