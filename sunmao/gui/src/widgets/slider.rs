//! Slider widget - linear control for parameters.

use crate::{
    GuiContext, Widget, ParameterWidget, WidgetId, WidgetState, Rect,
    Event, MouseButton, Color, Fill, Stroke,
};
use super::next_widget_id;

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
        self.default_value = default;
        self.value = default;
        self
    }
    
    fn value_from_position(&self, x: f32, y: f32) -> f32 {
        match self.orientation {
            Orientation::Horizontal => {
                let track_start = self.bounds.x + self.thumb_size / 2.0;
                let track_length = self.bounds.width - self.thumb_size;
                if track_length <= 0.0 { return 0.5; }
                ((x - track_start) / track_length).clamp(0.0, 1.0)
            }
            Orientation::Vertical => {
                let track_start = self.bounds.y + self.thumb_size / 2.0;
                let track_length = self.bounds.height - self.thumb_size;
                if track_length <= 0.0 { return 0.5; }
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
}

impl Widget for Slider {
    fn id(&self) -> WidgetId { self.id }
    fn bounds(&self) -> Rect { self.bounds }
    fn set_bounds(&mut self, bounds: Rect) { self.bounds = bounds; }
    fn state(&self) -> WidgetState { self.state }
    
    fn handle_event(&mut self, event: &Event) -> bool {
        match event {
            Event::MouseMove { x, y, .. } => {
                self.state.hovered = self.bounds.contains(*x, *y);
                
                if self.is_dragging {
                    self.value = self.value_from_position(*x, *y);
                    return true;
                }
                false
            }
            
            Event::MouseDown { x, y, button: MouseButton::Left, modifiers, .. } => {
                if self.bounds.contains(*x, *y) {
                    self.state.pressed = true;
                    self.is_dragging = true;
                    
                    // Ctrl/Cmd+click to reset
                    if modifiers.ctrl || modifiers.meta {
                        self.value = self.default_value;
                    } else {
                        self.value = self.value_from_position(*x, *y);
                    }
                    
                    return true;
                }
                false
            }
            
            Event::MouseUp { button: MouseButton::Left, .. } => {
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
        let track_thickness = 4.0;
        let corner_radius = track_thickness / 2.0;
        
        match self.orientation {
            Orientation::Horizontal => {
                let track_y = self.bounds.center_y() - track_thickness / 2.0;
                
                // Track background
                ctx.fill_rounded_rect(
                    self.bounds.x, track_y,
                    self.bounds.width, track_thickness,
                    corner_radius,
                    Fill::Solid(self.track_color),
                );
                
                // Value fill
                let (thumb_x, _) = self.thumb_position();
                let fill_width = thumb_x - self.bounds.x;
                if fill_width > 0.0 {
                    ctx.fill_rounded_rect(
                        self.bounds.x, track_y,
                        fill_width, track_thickness,
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
                    track_x, self.bounds.y,
                    track_thickness, self.bounds.height,
                    corner_radius,
                    Fill::Solid(self.track_color),
                );
                
                // Value fill (from bottom)
                let (_, thumb_y) = self.thumb_position();
                let fill_height = self.bounds.bottom() - thumb_y;
                if fill_height > 0.0 {
                    ctx.fill_rounded_rect(
                        track_x, thumb_y,
                        track_thickness, fill_height,
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
    fn param_id(&self) -> &str { &self.param_id }
    fn value(&self) -> f32 { self.value }
    fn set_value(&mut self, value: f32) { self.value = value.clamp(0.0, 1.0); }
}
