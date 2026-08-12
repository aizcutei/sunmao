//! Label widget - text display.

use super::next_widget_id;
use crate::{Color, Event, GuiContext, Rect, TextAlign, Widget, WidgetId, WidgetState};

/// A text label widget.
pub struct Label {
    id: WidgetId,
    bounds: Rect,
    text: String,
    // Visual settings
    pub color: Color,
    pub font_size: f32,
    pub alignment: TextAlign,
}

impl Label {
    pub fn new(text: &str) -> Self {
        Self {
            id: next_widget_id(),
            bounds: Rect::new(0.0, 0.0, 100.0, 20.0),
            text: text.to_string(),
            color: Color::FOREGROUND,
            font_size: 13.0,
            alignment: TextAlign::Left,
        }
    }

    pub fn with_bounds(mut self, bounds: Rect) -> Self {
        self.bounds = bounds;
        self
    }

    pub fn with_alignment(mut self, alignment: TextAlign) -> Self {
        self.alignment = alignment;
        self
    }

    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_string();
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

impl Widget for Label {
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
        WidgetState::default()
    }

    fn handle_event(&mut self, _event: &Event) -> bool {
        // Labels don't handle events
        false
    }

    fn draw(&self, ctx: &mut dyn GuiContext) {
        let x = match self.alignment {
            TextAlign::Left => self.bounds.x,
            TextAlign::Center => self.bounds.center_x(),
            TextAlign::Right => self.bounds.right(),
        };

        let y = self.bounds.center_y() + self.font_size * 0.35;

        ctx.draw_text(&self.text, x, y, self.font_size, self.color, self.alignment);
    }
}
