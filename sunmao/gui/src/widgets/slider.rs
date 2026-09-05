//! Slider widget - linear control for parameters.

use super::next_widget_id;
use crate::{
    Color, Event, Fill, GuiContext, MouseButton, ParameterWidget, Rect, Widget, WidgetId,
    WidgetState,
};

/// Slider orientation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

/// A linear slider widget for parameter control.
pub struct Slider {
    id: WidgetId,
    param_id: String,
    bounds: Rect,
    state: WidgetState,
    value: f32,
    default_value: f32,
    orientation: Orientation,
    // Drag state
    is_dragging: bool,
    value_changed: Option<Box<dyn Fn(f32) + Send>>,
    // Visual settings
    pub track_color: Color,
    pub value_color: Color,
    pub thumb_color: Color,
    pub thumb_size: f32,
}

impl Slider {
    pub fn new(param_id: &str) -> Self {
        Self {
            id: next_widget_id(),
            param_id: param_id.to_string(),
            bounds: Rect::new(0.0, 0.0, 120.0, 24.0),
            state: WidgetState::default(),
            value: 0.5,
            default_value: 0.5,
            orientation: Orientation::Horizontal,
            is_dragging: false,
            value_changed: None,
            track_color: Color::rgb(0.3, 0.3, 0.35),
            value_color: Color::ACCENT,
            thumb_color: Color::FOREGROUND,
            thumb_size: 16.0,
        }
    }

    pub fn with_bounds(mut self, bounds: Rect) -> Self {
        self.bounds = bounds;
        self
    }

    pub fn with_orientation(mut self, orientation: Orientation) -> Self {
        self.orientation = orientation;
        self
    }

    pub fn with_default(mut self, default: f32) -> Self {
        let default = if default.is_finite() {
            default.clamp(0.0, 1.0)
        } else {
            0.5
        };
        self.default_value = default;
        self.value = default;
        self
    }

    pub fn set_disabled(&mut self, disabled: bool) {
        self.state.disabled = disabled;
        if disabled {
            self.state.pressed = false;
            self.is_dragging = false;
        }
    }

    pub fn is_disabled(&self) -> bool {
        self.state.disabled
    }

    fn set_value_internal(&mut self, value: f32, notify: bool) {
        let value = if value.is_finite() {
            value.clamp(0.0, 1.0)
        } else {
            self.value
        };
        if (value - self.value).abs() <= f32::EPSILON {
            return;
        }
        self.value = value;
        if notify {
            if let Some(callback) = self.value_changed.as_ref() {
                callback(value);
            }
        }
    }

    fn value_from_position(&self, x: f32, y: f32) -> f32 {
        match self.orientation {
            Orientation::Horizontal => {
                let track_start = self.bounds.x + self.thumb_size / 2.0;
                let track_length = self.bounds.width - self.thumb_size;
                if track_length <= 0.0 {
                    return 0.5;
                }
                ((x - track_start) / track_length).clamp(0.0, 1.0)
            }
            Orientation::Vertical => {
                let track_start = self.bounds.y + self.thumb_size / 2.0;
                let track_length = self.bounds.height - self.thumb_size;
                if track_length <= 0.0 {
                    return 0.5;
                }
                // Vertical: top = max, bottom = min
                1.0 - ((y - track_start) / track_length).clamp(0.0, 1.0)
            }
        }
    }

    fn thumb_position(&self) -> (f32, f32) {
        match self.orientation {
            Orientation::Horizontal => {
                let track_start = self.bounds.x + self.thumb_size / 2.0;
                let track_length = self.bounds.width - self.thumb_size;
                let x = track_start + self.value * track_length;
                let y = self.bounds.center_y();
                (x, y)
            }
            Orientation::Vertical => {
                let track_start = self.bounds.y + self.thumb_size / 2.0;
                let track_length = self.bounds.height - self.thumb_size;
                let x = self.bounds.center_x();
                let y = track_start + (1.0 - self.value) * track_length;
                (x, y)
            }
        }
    }

    /// Keyboard nudge for a focused control.
    ///
    /// Arrow keys move by 1%, Page keys by 10%, Home/End jump to the ends.
    /// A modifier-held arrow is left alone so host shortcuts still work.
    /// Returns whether the key was consumed.
    fn handle_key(&mut self, event: &Event) -> bool {
        let Event::KeyDown { key, modifiers } = event else {
            return false;
        };
        if !self.state.focused || self.state.disabled {
            return false;
        }
        if modifiers.ctrl || modifiers.alt || modifiers.meta {
            return false;
        }
        // Shift is the conventional fine-adjust modifier.
        let step = if modifiers.shift { 0.001 } else { 0.01 };
        let target = match key {
            crate::KeyCode::Right | crate::KeyCode::Up => self.value + step,
            crate::KeyCode::Left | crate::KeyCode::Down => self.value - step,
            crate::KeyCode::PageUp => self.value + step * 10.0,
            crate::KeyCode::PageDown => self.value - step * 10.0,
            crate::KeyCode::Home => 0.0,
            crate::KeyCode::End => 1.0,
            _ => return false,
        };
        self.set_value_internal(target.clamp(0.0, 1.0), true);
        true
    }
}

impl Widget for Slider {
    fn id(&self) -> WidgetId {
        self.id
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }
    fn set_focused(&mut self, focused: bool) {
        self.state.focused = focused;
    }
    fn state(&self) -> WidgetState {
        self.state
    }

    fn handle_event(&mut self, event: &Event) -> bool {
        if self.handle_key(event) {
            return true;
        }
        if self.state.disabled {
            self.state.hovered = false;
            self.state.pressed = false;
            self.is_dragging = false;
            return false;
        }
        match event {
            Event::MouseMove { x, y, .. } => {
                self.state.hovered = self.bounds.contains(*x, *y);

                if self.is_dragging {
                    let value = self.value_from_position(*x, *y);
                    self.set_value_internal(value, true);
                    return true;
                }
                false
            }

            Event::MouseDown {
                x,
                y,
                button: MouseButton::Left,
                modifiers,
                ..
            } => {
                if self.bounds.contains(*x, *y) {
                    self.state.pressed = true;
                    self.is_dragging = true;

                    // Ctrl/Cmd+click to reset
                    if modifiers.ctrl || modifiers.meta {
                        self.set_value_internal(self.default_value, true);
                    } else {
                        let value = self.value_from_position(*x, *y);
                        self.set_value_internal(value, true);
                    }

                    return true;
                }
                false
            }

            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => {
                if self.is_dragging {
                    self.is_dragging = false;
                    self.state.pressed = false;
                    return true;
                }
                false
            }

            Event::Scroll { x, y, delta_y, .. } => {
                if self.bounds.contains(*x, *y) {
                    if !delta_y.is_finite() {
                        return false;
                    }
                    let delta = *delta_y * 0.01;
                    self.set_value_internal(self.value + delta, true);
                    return true;
                }
                false
            }

            _ => false,
        }
    }

    fn as_parameter(&mut self) -> Option<&mut dyn ParameterWidget> {
        Some(self)
    }

    fn draw(&self, ctx: &mut dyn GuiContext) {
        let track_thickness = 4.0;
        let corner_radius = track_thickness / 2.0;

        match self.orientation {
            Orientation::Horizontal => {
                let track_y = self.bounds.center_y() - track_thickness / 2.0;

                // Track background
                ctx.fill_rounded_rect(
                    self.bounds.x,
                    track_y,
                    self.bounds.width,
                    track_thickness,
                    corner_radius,
                    Fill::Solid(self.track_color),
                );

                // Value fill
                let (thumb_x, _) = self.thumb_position();
                let fill_width = thumb_x - self.bounds.x;
                if fill_width > 0.0 {
                    ctx.fill_rounded_rect(
                        self.bounds.x,
                        track_y,
                        fill_width,
                        track_thickness,
                        corner_radius,
                        Fill::Solid(self.value_color),
                    );
                }

                // Thumb
                let (tx, ty) = self.thumb_position();
                let thumb_radius = self.thumb_size / 2.0;
                ctx.fill_circle(tx, ty, thumb_radius, Fill::Solid(self.thumb_color));

                // Thumb highlight on hover/press
                if self.state.hovered || self.state.pressed {
                    let highlight = if self.state.pressed {
                        Color::rgba(1.0, 1.0, 1.0, 0.2)
                    } else {
                        Color::rgba(1.0, 1.0, 1.0, 0.1)
                    };
                    ctx.fill_circle(tx, ty, thumb_radius + 2.0, Fill::Solid(highlight));
                }
            }

            Orientation::Vertical => {
                let track_x = self.bounds.center_x() - track_thickness / 2.0;

                // Track background
                ctx.fill_rounded_rect(
                    track_x,
                    self.bounds.y,
                    track_thickness,
                    self.bounds.height,
                    corner_radius,
                    Fill::Solid(self.track_color),
                );

                // Value fill (from bottom)
                let (_, thumb_y) = self.thumb_position();
                let fill_height = self.bounds.bottom() - thumb_y;
                if fill_height > 0.0 {
                    ctx.fill_rounded_rect(
                        track_x,
                        thumb_y,
                        track_thickness,
                        fill_height,
                        corner_radius,
                        Fill::Solid(self.value_color),
                    );
                }

                // Thumb
                let (tx, ty) = self.thumb_position();
                let thumb_radius = self.thumb_size / 2.0;
                ctx.fill_circle(tx, ty, thumb_radius, Fill::Solid(self.thumb_color));
            }
        }
    }
}

impl ParameterWidget for Slider {
    fn param_id(&self) -> &str {
        &self.param_id
    }
    fn value(&self) -> f32 {
        self.value
    }
    fn set_value(&mut self, value: f32) {
        self.set_value_internal(value, false);
    }

    fn on_value_changed(&mut self, callback: Box<dyn Fn(f32) + Send>) {
        self.value_changed = Some(callback);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Event, Modifiers};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn slider_reports_interactive_value_changes_once() {
        let mut slider = Slider::new("gain").with_bounds(Rect::new(0.0, 0.0, 100.0, 20.0));
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&calls);
        slider.on_value_changed(Box::new(move |_value| {
            observed.fetch_add(1, Ordering::Relaxed);
        }));

        slider.handle_event(&Event::MouseDown {
            x: 50.0,
            y: 10.0,
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
        });
        slider.handle_event(&Event::MouseMove {
            x: 75.0,
            y: 10.0,
            modifiers: Modifiers::none(),
        });
        slider.handle_event(&Event::MouseUp {
            x: 75.0,
            y: 10.0,
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
        });

        assert!((slider.value() - 0.8).abs() < 0.02);
        assert!(calls.load(Ordering::Relaxed) >= 1);
        let calls_before_sync = calls.load(Ordering::Relaxed);
        slider.set_value(0.25);
        assert_eq!(calls.load(Ordering::Relaxed), calls_before_sync);
        slider.set_value(f32::NAN);
        assert!(slider.value().is_finite());
    }

    #[test]
    fn disabled_slider_does_not_consume_or_mutate_input() {
        let mut slider = Slider::new("gain");
        slider.state.disabled = true;
        let before = slider.value();
        let consumed = slider.handle_event(&Event::MouseDown {
            x: 10.0,
            y: 10.0,
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
        });
        assert!(!consumed);
        assert_eq!(slider.value(), before);
    }

    /// A host automation update must not be reported back to the host as a user
    /// edit: that closes a feedback loop where the host's own write bounces
    /// back, and with a smoothed parameter the two can chase each other.
    #[test]
    fn host_sync_never_echoes_back_to_the_host() {
        let mut control = Slider::new("gain").with_bounds(Rect::new(0.0, 0.0, 120.0, 24.0));
        let count = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&count);
        control.on_value_changed(Box::new(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
        }));
        control.set_value(0.75);
        assert_eq!(control.value(), 0.75);
        assert_eq!(
            count.load(Ordering::SeqCst),
            0,
            "a host-driven update was echoed back as an edit"
        );
    }
}
