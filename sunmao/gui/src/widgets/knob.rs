//! Knob widget - rotary control for parameters.

use super::next_widget_id;
use crate::{
    Color, Event, Fill, GuiContext, MouseButton, ParameterWidget, Rect, Stroke, Widget, WidgetId,
    WidgetState,
};
use std::f32::consts::PI;

/// A rotary knob widget for parameter control.
///
/// The knob displays a circular dial that can be dragged to change values.
/// Supports vertical drag for value changes (like most DAW knobs).
pub struct Knob {
    id: WidgetId,
    param_id: String,
    bounds: Rect,
    state: WidgetState,
    value: f32,
    default_value: f32,
    // Drag state
    drag_start_value: f32,
    drag_start_y: f32,
    is_dragging: bool,
    // Visual settings
    pub track_color: Color,
    pub value_color: Color,
    pub pointer_color: Color,
    pub background_color: Color,
    // Behavior
    pub sensitivity: f32, // Pixels per full range
}

impl Knob {
    pub fn new(param_id: &str) -> Self {
        Self {
            id: next_widget_id(),
            param_id: param_id.to_string(),
            bounds: Rect::new(0.0, 0.0, 48.0, 48.0),
            state: WidgetState::default(),
            value: 0.5,
            default_value: 0.5,
            drag_start_value: 0.0,
            drag_start_y: 0.0,
            is_dragging: false,
            track_color: Color::rgb(0.3, 0.3, 0.35),
            value_color: Color::ACCENT,
            pointer_color: Color::FOREGROUND,
            background_color: Color::rgb(0.2, 0.2, 0.22),
            sensitivity: 200.0,
        }
    }

    pub fn with_bounds(mut self, bounds: Rect) -> Self {
        self.bounds = bounds;
        self
    }

    pub fn with_default(mut self, default: f32) -> Self {
        self.default_value = default;
        self.value = default;
        self
    }

    fn angle_for_value(&self, value: f32) -> f32 {
        // -135° to +135° range (270° total)
        let start_angle = -PI * 0.75;
        let range = PI * 1.5;
        start_angle + value * range
    }
}

impl Widget for Knob {
    fn id(&self) -> WidgetId {
        self.id
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }
    fn state(&self) -> WidgetState {
        self.state
    }

    fn handle_event(&mut self, event: &Event) -> bool {
        match event {
            Event::MouseMove { x, y, .. } => {
                let was_hovered = self.state.hovered;
                self.state.hovered = self.bounds.contains(*x, *y);

                if self.is_dragging {
                    let delta = (self.drag_start_y - *y) / self.sensitivity;
                    self.value = (self.drag_start_value + delta).clamp(0.0, 1.0);
                    return true;
                }

                was_hovered != self.state.hovered
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
                    self.drag_start_y = *y;
                    self.drag_start_value = self.value;

                    // Double-click or Cmd/Ctrl+click to reset
                    if modifiers.ctrl || modifiers.meta {
                        self.value = self.default_value;
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
                    let delta = *delta_y * 0.01;
                    self.value = (self.value + delta).clamp(0.0, 1.0);
                    return true;
                }
                false
            }

            _ => false,
        }
    }

    fn draw(&self, ctx: &mut dyn GuiContext) {
        let cx = self.bounds.center_x();
        let cy = self.bounds.center_y();
        let radius = self.bounds.width.min(self.bounds.height) / 2.0 - 4.0;
        let track_width = 3.0;

        // Background circle
        ctx.fill_circle(cx, cy, radius, Fill::Solid(self.background_color));

        // Track (full arc)
        let start_angle = -PI * 0.75;
        let end_angle = PI * 0.75;
        ctx.stroke_arc(
            cx,
            cy,
            radius - track_width,
            start_angle,
            end_angle,
            Stroke::new(self.track_color, track_width),
        );

        // Value arc
        let value_angle = self.angle_for_value(self.value);
        if self.value > 0.0 {
            ctx.stroke_arc(
                cx,
                cy,
                radius - track_width,
                start_angle,
                value_angle,
                Stroke::new(self.value_color, track_width),
            );
        }

        // Pointer line
        let pointer_inner = radius * 0.3;
        let pointer_outer = radius * 0.7;
        let angle = self.angle_for_value(self.value);
        let cos_a = angle.cos();
        let sin_a = angle.sin();
        ctx.stroke_line(
            cx + cos_a * pointer_inner,
            cy + sin_a * pointer_inner,
            cx + cos_a * pointer_outer,
            cy + sin_a * pointer_outer,
            Stroke::new(self.pointer_color, 2.0),
        );

        // Hover/pressed highlight
        if self.state.hovered || self.state.pressed {
            let highlight_color = if self.state.pressed {
                Color::rgba(1.0, 1.0, 1.0, 0.15)
            } else {
                Color::rgba(1.0, 1.0, 1.0, 0.08)
            };
            ctx.fill_circle(cx, cy, radius, Fill::Solid(highlight_color));
        }
    }
}

impl ParameterWidget for Knob {
    fn param_id(&self) -> &str {
        &self.param_id
    }
    fn value(&self) -> f32 {
        self.value
    }
    fn set_value(&mut self, value: f32) {
        self.value = value.clamp(0.0, 1.0);
    }
}
