//! Button widget - toggle or momentary button.

use super::next_widget_id;
use crate::{
    Color, Event, Fill, GuiContext, MouseButton, Rect, Stroke, TextAlign, Widget, WidgetId,
    WidgetState,
};

/// Button behavior type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonType {
    /// Click to toggle on/off
    Toggle,
    /// Pressed while held, released when let go
    Momentary,
}

/// A button widget.
pub struct Button {
    id: WidgetId,
    bounds: Rect,
    state: WidgetState,
    label: String,
    is_on: bool,
    button_type: ButtonType,
    clicked: Option<Box<dyn Fn(bool) + Send>>,
    // Visual settings
    pub off_color: Color,
    pub on_color: Color,
    pub text_color: Color,
    pub corner_radius: f32,
    pub font_size: f32,
}

impl Button {
    pub fn new(label: &str) -> Self {
        Self {
            id: next_widget_id(),
            bounds: Rect::new(0.0, 0.0, 80.0, 28.0),
            state: WidgetState::default(),
            label: label.to_string(),
            is_on: false,
            button_type: ButtonType::Momentary,
            clicked: None,
            off_color: Color::rgb(0.25, 0.25, 0.28),
            on_color: Color::ACCENT,
            text_color: Color::FOREGROUND,
            corner_radius: 4.0,
            font_size: 13.0,
        }
    }

    pub fn toggle(label: &str) -> Self {
        Self {
            button_type: ButtonType::Toggle,
            ..Self::new(label)
        }
    }

    pub fn with_bounds(mut self, bounds: Rect) -> Self {
        self.bounds = bounds;
        self
    }

    pub fn is_on(&self) -> bool {
        self.is_on
    }

    pub fn set_on(&mut self, on: bool) {
        self.is_on = on;
    }

    pub fn set_disabled(&mut self, disabled: bool) {
        self.state.disabled = disabled;
        if disabled {
            self.state.pressed = false;
        }
    }

    pub fn is_disabled(&self) -> bool {
        self.state.disabled
    }

    /// Register a callback invoked after the button's logical state changes.
    /// The callback receives the new `is_on` value.
    pub fn on_click(&mut self, callback: Box<dyn Fn(bool) + Send>) {
        self.clicked = Some(callback);
    }

    fn notify_click(&self) {
        if let Some(callback) = self.clicked.as_ref() {
            callback(self.is_on);
        }
    }
}

impl Widget for Button {
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
        if self.state.disabled {
            self.state.hovered = false;
            self.state.pressed = false;
            return false;
        }
        match event {
            Event::MouseMove { x, y, .. } => {
                let was_hovered = self.state.hovered;
                self.state.hovered = self.bounds.contains(*x, *y);
                was_hovered != self.state.hovered
            }

            Event::MouseDown {
                x,
                y,
                button: MouseButton::Left,
                ..
            } => {
                if self.bounds.contains(*x, *y) {
                    self.state.pressed = true;

                    match self.button_type {
                        ButtonType::Toggle => {
                            self.is_on = !self.is_on;
                        }
                        ButtonType::Momentary => {
                            self.is_on = true;
                        }
                    }
                    self.notify_click();

                    return true;
                }
                false
            }

            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => {
                if self.state.pressed {
                    self.state.pressed = false;

                    if self.button_type == ButtonType::Momentary {
                        self.is_on = false;
                        self.notify_click();
                    }

                    return true;
                }
                false
            }

            _ => false,
        }
    }

    fn draw(&self, ctx: &mut dyn GuiContext) {
        let bg_color = if self.is_on || self.state.pressed {
            self.on_color
        } else if self.state.hovered {
            Color::rgb(0.3, 0.3, 0.33)
        } else {
            self.off_color
        };

        // Background
        ctx.fill_rounded_rect(
            self.bounds.x,
            self.bounds.y,
            self.bounds.width,
            self.bounds.height,
            self.corner_radius,
            Fill::Solid(bg_color),
        );

        // Border
        ctx.stroke_rounded_rect(
            self.bounds.x,
            self.bounds.y,
            self.bounds.width,
            self.bounds.height,
            self.corner_radius,
            Stroke::new(Color::rgb(0.4, 0.4, 0.45), 1.0),
        );

        // Label
        ctx.draw_text(
            &self.label,
            self.bounds.center_x(),
            self.bounds.center_y() + self.font_size * 0.35, // Approximate vertical centering
            self.font_size,
            self.text_color,
            TextAlign::Center,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Modifiers, NullContext};
    use std::sync::{Arc, Mutex};

    #[test]
    fn toggle_button_notifies_and_disabled_button_ignores_input() {
        let mut button = Button::toggle("Bypass");
        let states = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&states);
        button.on_click(Box::new(move |state| observed.lock().unwrap().push(state)));
        let down = Event::MouseDown {
            x: 10.0,
            y: 10.0,
            button: MouseButton::Left,
            modifiers: Modifiers::none(),
        };
        assert!(button.handle_event(&down));
        assert!(button.is_on());
        assert_eq!(*states.lock().unwrap(), vec![true]);

        button.state.disabled = true;
        assert!(!button.handle_event(&down));
        assert!(button.is_on());

        let mut ctx = NullContext::new(100.0, 100.0);
        button.draw(&mut ctx);
    }
}
