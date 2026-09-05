//! Toggle widget — a boolean parameter control.

use super::next_widget_id;
use crate::{
    Event, Fill, GuiContext, MouseButton, ParameterWidget, Rect, TextAlign, Theme, Widget,
    WidgetId, WidgetState,
};

/// A two-state switch bound to a boolean parameter.
///
/// Like every [`ParameterWidget`], the value is carried as a normalized `f32`:
/// `>= 0.5` is on. That keeps one representation across the host boundary,
/// where VST3 and CLAP both express booleans as normalized floats.
pub struct Toggle {
    id: WidgetId,
    param_id: String,
    bounds: Rect,
    state: WidgetState,
    value: f32,
    default_value: f32,
    label: Option<String>,
    value_changed: Option<Box<dyn Fn(f32) + Send>>,
    pub theme: Theme,
}

impl Toggle {
    pub fn new(param_id: &str) -> Self {
        Self {
            id: next_widget_id(),
            param_id: param_id.to_string(),
            bounds: Rect::new(0.0, 0.0, 48.0, 24.0),
            state: WidgetState::default(),
            value: 0.0,
            default_value: 0.0,
            label: None,
            value_changed: None,
            theme: Theme::dark(),
        }
    }

    pub fn with_bounds(mut self, bounds: Rect) -> Self {
        self.bounds = bounds;
        self
    }

    pub fn with_default(mut self, on: bool) -> Self {
        self.default_value = if on { 1.0 } else { 0.0 };
        self.value = self.default_value;
        self
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.label = Some(label.to_string());
        self
    }

    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn is_on(&self) -> bool {
        self.value >= 0.5
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

    fn set_value_internal(&mut self, value: f32, notify: bool) {
        let value = if value.is_finite() {
            value.clamp(0.0, 1.0)
        } else {
            self.value
        };
        // Snap to the two states this control can actually represent, so a
        // host sending 0.4 does not leave the widget in a third state its
        // painter cannot draw.
        let value = if value >= 0.5 { 1.0 } else { 0.0 };
        if value == self.value {
            return;
        }
        self.value = value;
        if notify {
            if let Some(callback) = self.value_changed.as_ref() {
                callback(value);
            }
        }
    }
}

impl Widget for Toggle {
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

    fn set_focused(&mut self, focused: bool) {
        self.state.focused = focused;
    }

    fn handle_event(&mut self, event: &Event) -> bool {
        if self.state.disabled {
            return false;
        }
        match event {
            Event::MouseMove { x, y, .. } => {
                self.state.hovered = self.hit_test(*x, *y);
                false
            }
            Event::MouseDown {
                x,
                y,
                button: MouseButton::Left,
                ..
            } => {
                if !self.hit_test(*x, *y) {
                    return false;
                }
                self.state.pressed = true;
                true
            }
            Event::MouseUp {
                x,
                y,
                button: MouseButton::Left,
                ..
            } => {
                if !self.state.pressed {
                    return false;
                }
                self.state.pressed = false;
                // Flip only when the release lands on the control, matching
                // every platform's button semantics: dragging off cancels.
                if self.hit_test(*x, *y) {
                    let flipped = if self.is_on() { 0.0 } else { 1.0 };
                    self.set_value_internal(flipped, true);
                }
                true
            }
            _ => false,
        }
    }

    fn as_parameter(&mut self) -> Option<&mut dyn ParameterWidget> {
        Some(self)
    }

    fn draw(&self, ctx: &mut dyn GuiContext) {
        let b = self.bounds;
        let theme = &self.theme;
        ctx.fill_rect(b.x, b.y, b.width, b.height, Fill::Solid(theme.surface));

        // The knob travels to the right half when on; colour carries the state
        // too, so a monochrome capture can still tell them apart.
        let inset = 2.0;
        let travel = (b.width - b.height).max(0.0);
        let knob_x = if self.is_on() { b.x + travel } else { b.x };
        let fill = if self.state.disabled {
            theme.muted
        } else if self.is_on() {
            if self.state.hovered {
                theme.accent_hover
            } else {
                theme.accent
            }
        } else {
            theme.track
        };
        ctx.fill_rect(
            knob_x + inset,
            b.y + inset,
            (b.height - inset * 2.0).max(1.0),
            (b.height - inset * 2.0).max(1.0),
            Fill::Solid(fill),
        );

        if let Some(label) = self.label.as_deref() {
            let colour = if self.state.disabled {
                theme.muted
            } else {
                theme.foreground
            };
            ctx.draw_text(
                label,
                b.x + b.width + 6.0,
                b.y + b.height * 0.5,
                12.0,
                colour,
                TextAlign::Left,
            );
        }
    }
}

impl ParameterWidget for Toggle {
    fn param_id(&self) -> &str {
        &self.param_id
    }

    fn value(&self) -> f32 {
        self.value
    }

    fn set_value(&mut self, value: f32) {
        // Host-driven sync must not echo back to the host.
        self.set_value_internal(value, false);
    }

    fn display_value(&self) -> String {
        if self.is_on() { "On" } else { "Off" }.to_string()
    }

    fn on_value_changed(&mut self, callback: Box<dyn Fn(f32) + Send>) {
        self.value_changed = Some(callback);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Modifiers, NullContext};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn press(toggle: &mut Toggle, x: f32, y: f32) {
        toggle.handle_event(&Event::MouseDown {
            x,
            y,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
        });
    }

    fn release(toggle: &mut Toggle, x: f32, y: f32) {
        toggle.handle_event(&Event::MouseUp {
            x,
            y,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
        });
    }

    fn toggle_at_origin() -> Toggle {
        Toggle::new("bypass").with_bounds(Rect::new(0.0, 0.0, 48.0, 24.0))
    }

    #[test]
    fn a_click_flips_the_state_and_notifies_once() {
        let mut toggle = toggle_at_origin();
        let count = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&count);
        toggle.on_value_changed(Box::new(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
        }));

        assert!(!toggle.is_on());
        press(&mut toggle, 10.0, 10.0);
        release(&mut toggle, 10.0, 10.0);
        assert!(toggle.is_on());
        assert_eq!(count.load(Ordering::SeqCst), 1);

        press(&mut toggle, 10.0, 10.0);
        release(&mut toggle, 10.0, 10.0);
        assert!(!toggle.is_on());
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn dragging_off_the_control_cancels_the_click() {
        let mut toggle = toggle_at_origin();
        press(&mut toggle, 10.0, 10.0);
        release(&mut toggle, 500.0, 500.0);
        assert!(
            !toggle.is_on(),
            "release outside the bounds still flipped it"
        );
    }

    #[test]
    fn host_sync_never_echoes_back_to_the_host() {
        let mut toggle = toggle_at_origin();
        let count = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&count);
        toggle.on_value_changed(Box::new(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
        }));
        toggle.set_value(1.0);
        assert!(toggle.is_on());
        assert_eq!(
            count.load(Ordering::SeqCst),
            0,
            "a host-driven update was echoed back as an edit"
        );
    }

    #[test]
    fn intermediate_and_invalid_values_snap_to_a_drawable_state() {
        let mut toggle = toggle_at_origin();
        toggle.set_value(0.4);
        assert_eq!(toggle.value(), 0.0);
        toggle.set_value(0.6);
        assert_eq!(toggle.value(), 1.0);
        toggle.set_value(f32::NAN);
        assert_eq!(toggle.value(), 1.0, "NaN moved the control");
        toggle.set_value(-5.0);
        assert_eq!(toggle.value(), 0.0);
    }

    #[test]
    fn a_disabled_toggle_ignores_input_but_still_draws() {
        let mut toggle = toggle_at_origin();
        toggle.set_disabled(true);
        press(&mut toggle, 10.0, 10.0);
        release(&mut toggle, 10.0, 10.0);
        assert!(!toggle.is_on());
        let mut ctx = NullContext::new(100.0, 100.0);
        toggle.draw(&mut ctx);
    }

    #[test]
    fn display_value_reads_as_a_state_not_a_number() {
        let mut toggle = toggle_at_origin();
        assert_eq!(toggle.display_value(), "Off");
        toggle.set_value(1.0);
        assert_eq!(toggle.display_value(), "On");
    }
}
