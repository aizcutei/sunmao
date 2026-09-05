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

pub mod voice;

// Re-export everything from sunmao_core
pub use sunmao_core::*;

/// The DSP component library, re-exported so plugins depend on one crate.
pub use sunmao_dsp as dsp;

// Re-export macros
pub use sunmao_macros::*;

/// Implementation crates used by proc-macro expansions. This module is
/// intentionally hidden from normal API documentation; it lets a plugin
/// depend only on the `sunmao` facade while `#[derive(Params)]` still refers
/// to the canonical core types.
#[doc(hidden)]
pub mod __private {
    pub use sunmao_core;
}

/// Format adapters used by the unified export macro.
pub use sunmao_backend_clap as backend_clap;
pub use sunmao_backend_vst3 as backend_vst3;

#[cfg(feature = "standalone")]
pub use sunmao_runtime;
#[cfg(feature = "standalone")]
pub use sunmao_runtime as runtime;

/// Generate the complete `main` function for a standalone executable target.
#[macro_export]
macro_rules! sunmao_standalone {
    ($plugin_type:ty) => {
        fn main() {
            if let Err(error) = $crate::runtime::run_standalone_entry::<$plugin_type>() {
                eprintln!("SunMao standalone failed: {error:#}");
                std::process::exit(1);
            }
        }
    };
}

/// Renderer-agnostic GUI primitives, enabled with the `gui` feature.
#[cfg(feature = "gui")]
pub use sunmao_gui as gui;

/// Bridges the host-facing `ViewContext` to the GUI layer's `ParamHost`.
#[cfg(feature = "gui")]
mod binding;
#[cfg(feature = "gui")]
pub use binding::ViewContextHost;

/// Baseview-backed editor views. Enable one of `gui-gl`, `gui-wgpu`, or
/// `gui-webview` to expose the corresponding renderer types through the
/// facade.
#[cfg(any(feature = "gui-gl", feature = "gui-wgpu", feature = "gui-webview"))]
pub use sunmao_view_baseview as view_baseview;

/// Common imports for SunMao plugins.
pub mod prelude {
    pub use std::sync::Arc;
    pub use sunmao_core::audio::AudioBuffer;
    pub use sunmao_core::events::{
        Event, EventQueue, MidiMessage, NoteExpression, NoteExpressionKind, ParamChange,
    };
    pub use sunmao_core::metadata::{AuInfo, ClapInfo, Vst3Info, Vst3SpeakerLayout};
    pub use sunmao_core::params::{
        stable_param_id, validate_param_layout, BoolParam, FloatParam, IntParam, ParamDescriptor,
        ParamKind, ParamLayoutError, Params,
    };
    pub use sunmao_core::plugin::{
        BusConfig, BusInfo, BusRole, PresetLocation, ProcessContext, ProcessStatus, RenderMode,
        SunmaoPlugin, TailLength, VoiceInfo,
    };
    pub use sunmao_core::smoothing::{Smoother, SmoothingStyle};
    pub use sunmao_core::view::{
        ParamsViewContext, ParentWindow, StandaloneViewOptions, StandaloneViewResult, SunmaoView,
        ViewContext, ViewHandle,
    };
    pub use sunmao_macros::{sunmao_export, Params};

    /// The DSP building blocks, so a plugin needs one import rather than two.
    /// None of these names collide with the framework's.
    pub use sunmao_dsp::prelude::*;

    pub use crate::voice::MonoVoice;

    #[cfg(feature = "standalone")]
    pub use sunmao_runtime::{
        InputMode, RuntimeConfig, StandaloneProcessor, StandaloneSmokeReport,
    };

    #[cfg(feature = "gui")]
    pub use crate::binding::ViewContextHost;
    pub use sunmao_core::viz::{viz_channel, VizConsumer, VizFrame, VizPublisher};
    #[cfg(feature = "accessibility")]
    pub use sunmao_gui::accesskit_update;
    #[cfg(feature = "gui")]
    pub use sunmao_gui::{accessibility_tree, AccessibleNode, AccessibleRole};
    #[cfg(feature = "gui")]
    pub use sunmao_gui::{
        Alignment, Axis, Button, ButtonType, Color, Column, Direction, Dropdown, Event as GuiEvent,
        Fill, FontStyle, GuiContext, KeyCode, Knob, Label, Layout, Modifiers, MouseButton,
        MouseButton as GuiMouseButton, NullContext, Orientation, ParamBinder, ParamHost,
        ParameterWidget, Point, Rect, Row, Size, Slider, Stack, Stroke, TextAlign, TextMetrics,
        TextVAlign, Theme, Toggle, TtfFont, Widget, WidgetContainer,
    };
    #[cfg(feature = "gui")]
    pub use sunmao_gui::{
        Clipboard, Font, GlyphBitmap, GlyphMetrics, GlyphSource, LineMetrics, MemoryClipboard,
        SpectrumAnalyzer, SpectrumSource, StaticSpectrum, MAX_SPECTRUM_BARS,
    };

    #[cfg(feature = "gui-wgpu")]
    pub use sunmao_gui::wgpu::WgpuContext;
    #[cfg(any(feature = "gui-gl", feature = "gui-wgpu", feature = "gui-webview"))]
    pub use sunmao_view_baseview::{BaseviewConfig, WindowScalePolicy};
    #[cfg(feature = "gui-gl")]
    pub use sunmao_view_baseview::{BaseviewView, ViewState};
    #[cfg(feature = "gui-webview")]
    pub use sunmao_view_baseview::{BaseviewWebView, WebViewState};
    #[cfg(feature = "gui-wgpu")]
    pub use sunmao_view_baseview::{BaseviewWgpuView, WgpuViewState};
}
