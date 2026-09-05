//! SunMao GUI Abstraction Layer
//!
//! This crate provides a renderer-agnostic GUI framework for SunMao plugins.
//! It defines traits and components that can be implemented by different
//! rendering backends (OpenGL, WGPU, WebView, etc.).
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │             Plugin / Standalone              │
//! ├─────────────────────────────────────────────┤
//! │              sunmao_gui (this)              │
//! │   - Widget trait, ParameterWidget trait     │
//! │   - Layout system, Event handling           │
//! │   - Core widgets (Knob, Slider, Button)     │
//! ├─────────────────────────────────────────────┤
//! │           Renderer Backends                  │
//! │   - sunmao_gui::gl (OpenGL)                 │
//! │   - sunmao_gui::wgpu (optional)             │
//! │   - sunmao_gui::webview (optional)          │
//! └─────────────────────────────────────────────┘
//! ```

mod accessibility;
mod binding;
mod clipboard;
mod context;
mod event;
mod layout;
mod stack;
mod text;
#[cfg(feature = "text")]
mod text_ttf;
mod theme;
mod widget;
mod widgets;

pub use accessibility::*;
pub use binding::*;
pub use clipboard::*;
pub use context::*;
pub use event::*;
pub use layout::*;
pub use stack::*;
pub use text::*;
#[cfg(feature = "text")]
pub use text_ttf::*;
pub use theme::*;
pub use widget::*;
pub use widgets::*;

#[cfg(feature = "gl")]
pub mod gl;

#[cfg(feature = "wgpu")]
pub mod wgpu;

#[cfg(feature = "webview")]
pub mod webview;
