//! Core widget implementations.
//!
//! This module provides the basic widgets for building plugin GUIs:
//! - Knob: Rotary control for parameters
//! - Slider: Linear control for parameters
//! - Button: Toggle or momentary button
//! - Label: Text display

mod button;
mod knob;
mod label;
mod slider;

pub use button::{Button, ButtonType};
pub use knob::Knob;
pub use label::Label;
pub use slider::{Orientation, Slider};

use std::sync::atomic::{AtomicU64, Ordering};

/// Global widget ID counter
static NEXT_WIDGET_ID: AtomicU64 = AtomicU64::new(1);

/// Generate a unique widget ID
pub fn next_widget_id() -> u64 {
    NEXT_WIDGET_ID.fetch_add(1, Ordering::Relaxed)
}
