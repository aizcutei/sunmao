//! Core widget implementations.
//!
//! This module provides the basic widgets for building plugin GUIs:
//! - Knob: Rotary control for parameters
//! - Slider: Linear control for parameters
//! - Button: Toggle or momentary button
//! - Toggle: Boolean switch
//! - Dropdown: Discrete choice
//! - SpectrumAnalyzer: audio->GUI bar display
//! - Label: Text display

mod button;
mod dropdown;
mod knob;
mod label;
mod slider;
mod spectrum;
mod toggle;

pub use button::{Button, ButtonType};
pub use dropdown::Dropdown;
pub use knob::Knob;
pub use label::Label;
pub use slider::{Orientation, Slider};
pub use spectrum::{SpectrumAnalyzer, SpectrumSource, StaticSpectrum, MAX_SPECTRUM_BARS};
pub use toggle::Toggle;

use std::sync::atomic::{AtomicU64, Ordering};

/// Global widget ID counter
static NEXT_WIDGET_ID: AtomicU64 = AtomicU64::new(1);

/// Generate a unique widget ID
pub fn next_widget_id() -> u64 {
    NEXT_WIDGET_ID.fetch_add(1, Ordering::Relaxed)
}
