//! Dropdown widget — a discrete-choice parameter control.

use super::next_widget_id;
use crate::{
    Event, Fill, GuiContext, MouseButton, ParameterWidget, Rect, TextAlign, Theme, Widget,
    WidgetId, WidgetState,
};

/// Height of one row in the open list, in logical pixels.
const OPTION_HEIGHT: f32 = 22.0;

/// A closed-by-default list of named options bound to a stepped parameter.
///
/// The value is normalized like every other [`ParameterWidget`]: option `i` of
/// `n` maps to `i / (n - 1)`, which is how both VST3 and CLAP carry a stepped
/// parameter across the host boundary. A single-option dropdown is pinned at
/// `0.0` rather than dividing by zero.
pub struct Dropdown {
    id: WidgetId,
    param_id: String,
    bounds: Rect,
    state: WidgetState,
    options: Vec<String>,
    selected: usize,
    open: bool,
    value_changed: Option<Box<dyn Fn(f32) + Send>>,
    pub theme: Theme,
}

impl Dropdown {
    /// An empty dropdown is legal but inert; it renders its plate and ignores
    /// clicks, which is better than panicking on a plugin that builds its
    /// option list at runtime.
    pub fn new(param_id: &str, options: &[&str]) -> Self {
        Self {
            id: next_widget_id(),
            param_id: param_id.to_string(),
            bounds: Rect::new(0.0, 0.0, 120.0, 24.0),
            state: WidgetState::default(),
            options: options.iter().map(|option| option.to_string()).collect(),
            selected: 0,
            open: false,
            value_changed: None,
            theme: Theme::dark(),
        }
    }

    pub fn with_bounds(mut self, bounds: Rect) -> Self {
        self.bounds = bounds;
        self
    }

    pub fn with_selected(mut self, index: usize) -> Self {
        self.selected = self.clamp_index(index);
        self
    }

    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    pub fn options(&self) -> &[String] {
        &self.options
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// Text of the current option, or `""` when there are none.
    pub fn selected_label(&self) -> &str {
        self.options
            .get(self.selected)
            .map(String::as_str)
            .unwrap_or("")
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn set_disabled(&mut self, disabled: bool) {
        self.state.disabled = disabled;
        if disabled {
            self.open = false;
            self.state.pressed = false;
        }
    }

    pub fn is_disabled(&self) -> bool {
        self.state.disabled
    }

    fn clamp_index(&self, index: usize) -> usize {
        index.min(self.options.len().saturating_sub(1))
    }

    /// Normalized value for an option index.
    fn value_for_index(&self, index: usize) -> f32 {
        let last = self.options.len().saturating_sub(1);
        if last == 0 {
            return 0.0;
        }
        index as f32 / last as f32
    }

    /// Option index for a normalized value. Rounds to the nearest step so a
    /// host's floating-point round-trip cannot land between options.
    ///
    /// `None` for a non-finite value: the caller must leave the selection
    /// alone rather than fall back to index 0, which would silently jump the
    /// control to the first option.
    fn index_for_value(&self, value: f32) -> Option<usize> {
        if !value.is_finite() {
            return None;
        }
        let last = self.options.len().saturating_sub(1);
        if last == 0 {
            return Some(0);
        }
        let scaled = (value.clamp(0.0, 1.0) * last as f32).round();
        Some((scaled as usize).min(last))
    }

    /// Bounds of the open list, directly below the closed control.
    fn list_bounds(&self) -> Rect {
        Rect::new(
            self.bounds.x,
            self.bounds.y + self.bounds.height,
            self.bounds.width,
            OPTION_HEIGHT * self.options.len() as f32,
        )
    }

    fn select_internal(&mut self, index: usize, notify: bool) {
        if self.options.is_empty() {
            return;
        }
        let index = self.clamp_index(index);
        if index == self.selected {
            return;
        }
        self.selected = index;
        if notify {
            if let Some(callback) = self.value_changed.as_ref() {
                callback(self.value_for_index(index));
            }
        }
    }

    /// Arrows move the selection; Space/Enter opens or closes the list;
    /// Escape closes it without changing anything.
    fn handle_key(&mut self, event: &Event) -> bool {
        let Event::KeyDown { key, modifiers } = event else {
            return false;
        };
        if !self.state.focused || self.state.disabled || self.options.is_empty() {
            return false;
        }
        if modifiers.ctrl || modifiers.alt || modifiers.meta {
            return false;
        }
        match key {
            crate::KeyCode::Down | crate::KeyCode::Right => {
                let next = (self.selected + 1).min(self.options.len() - 1);
                self.select_internal(next, true);
                true
            }
            crate::KeyCode::Up | crate::KeyCode::Left => {
                let previous = self.selected.saturating_sub(1);
                self.select_internal(previous, true);
                true
            }
            crate::KeyCode::Home => {
                self.select_internal(0, true);
                true
            }
            crate::KeyCode::End => {
                self.select_internal(self.options.len() - 1, true);
                true
            }
            crate::KeyCode::Space | crate::KeyCode::Enter => {
                self.open = !self.open;
                true
            }
            crate::KeyCode::Escape => {
                self.open = false;
                true
            }
            _ => false,
        }
    }
}

impl Widget for Dropdown {
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
        if !focused {
            // Losing focus closes the list, otherwise it would hang open over
            // whatever the user clicked next.
            self.open = false;
        }
    }

    /// While open the dropdown claims clicks anywhere, because a popup that
    /// let clicks through to the controls beneath it would edit them by
    /// accident.
    fn hit_test(&self, x: f32, y: f32) -> bool {
        self.bounds.contains(x, y) || (self.open && self.list_bounds().contains(x, y))
    }

    fn handle_event(&mut self, event: &Event) -> bool {
        if self.handle_key(event) {
            return true;
        }
        if self.state.disabled || self.options.is_empty() {
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
                if self.open {
                    let list = self.list_bounds();
                    if list.contains(*x, *y) {
                        let row = ((*y - list.y) / OPTION_HEIGHT).floor();
                        if row >= 0.0 {
                            self.select_internal(row as usize, true);
                        }
                        self.open = false;
                        return true;
                    }
                    // A click anywhere else dismisses the list without
                    // changing the value.
                    self.open = false;
                    return self.bounds.contains(*x, *y);
                }
                if self.bounds.contains(*x, *y) {
                    self.open = true;
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
        let b = self.bounds;
        let theme = &self.theme;
        let plate = if self.state.disabled {
            theme.track
        } else if self.state.hovered {
            theme.accent_hover
        } else {
            theme.surface
        };
        ctx.fill_rect(b.x, b.y, b.width, b.height, Fill::Solid(plate));

        let text_colour = if self.state.disabled {
            theme.muted
        } else {
            theme.foreground
        };
        ctx.draw_text(
            self.selected_label(),
            b.x + 6.0,
            b.y + b.height * 0.5,
            12.0,
            text_colour,
            TextAlign::Left,
        );

        if !self.open {
            return;
        }
        let list = self.list_bounds();
        ctx.fill_rect(
            list.x,
            list.y,
            list.width,
            list.height,
            Fill::Solid(theme.surface),
        );
        for (index, option) in self.options.iter().enumerate() {
            let row_y = list.y + OPTION_HEIGHT * index as f32;
            if index == self.selected {
                ctx.fill_rect(
                    list.x,
                    row_y,
                    list.width,
                    OPTION_HEIGHT,
                    Fill::Solid(theme.accent),
                );
            }
            ctx.draw_text(
                option,
                list.x + 6.0,
                row_y + OPTION_HEIGHT * 0.5,
                12.0,
                theme.foreground,
                TextAlign::Left,
            );
        }
    }
}

impl ParameterWidget for Dropdown {
    fn param_id(&self) -> &str {
        &self.param_id
    }

    fn value(&self) -> f32 {
        self.value_for_index(self.selected)
    }

    fn set_value(&mut self, value: f32) {
        let Some(index) = self.index_for_value(value) else {
            return;
        };
        // Host-driven sync must not echo back to the host.
        self.select_internal(index, false);
    }

    fn display_value(&self) -> String {
        self.selected_label().to_string()
    }

    fn accessible_role(&self) -> crate::AccessibleRole {
        crate::AccessibleRole::ComboBox
    }

    /// Accepts an option label, so a value copied out of one dropdown pastes
    /// back meaningfully. A bare number is still accepted as the normalized
    /// form.
    fn set_from_text(&mut self, text: &str) -> bool {
        let trimmed = text.trim();
        if let Some(index) = self
            .options
            .iter()
            .position(|option| option.eq_ignore_ascii_case(trimmed))
        {
            self.select_internal(index, false);
            return true;
        }
        match trimmed.parse::<f32>() {
            Ok(value) if value.is_finite() => {
                if let Some(index) = self.index_for_value(value) {
                    self.select_internal(index, false);
                    return true;
                }
                false
            }
            _ => false,
        }
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
    use std::sync::{Arc, Mutex};

    const MODES: [&str; 4] = ["Clean", "Warm", "Bright", "Crush"];

    fn click(dropdown: &mut Dropdown, x: f32, y: f32) -> bool {
        dropdown.handle_event(&Event::MouseDown {
            x,
            y,
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
        })
    }

    fn dropdown() -> Dropdown {
        Dropdown::new("mode", &MODES).with_bounds(Rect::new(0.0, 0.0, 120.0, 24.0))
    }

    #[test]
    fn opening_then_picking_a_row_selects_that_option() {
        let mut d = dropdown();
        assert!(!d.is_open());
        assert!(click(&mut d, 10.0, 10.0));
        assert!(d.is_open());

        // Third row (index 2) of the list below the control.
        let y = d.bounds.height + OPTION_HEIGHT * 2.0 + 1.0;
        assert!(click(&mut d, 10.0, y));
        assert!(!d.is_open(), "picking an option left the list open");
        assert_eq!(d.selected_index(), 2);
        assert_eq!(d.selected_label(), "Bright");
    }

    #[test]
    fn a_click_outside_dismisses_without_changing_the_value() {
        let mut d = dropdown();
        click(&mut d, 10.0, 10.0);
        assert!(d.is_open());
        click(&mut d, 900.0, 900.0);
        assert!(!d.is_open());
        assert_eq!(d.selected_index(), 0, "dismissing changed the selection");
    }

    #[test]
    fn an_open_list_swallows_clicks_meant_for_widgets_beneath_it() {
        let mut d = dropdown();
        click(&mut d, 10.0, 10.0);
        let inside_list = d.bounds.height + 1.0;
        assert!(
            d.hit_test(10.0, inside_list),
            "the open list does not claim its own area"
        );
        assert!(!d.hit_test(10.0, 9_000.0));
    }

    #[test]
    fn normalized_values_round_trip_through_every_option() {
        let mut d = dropdown();
        for index in 0..MODES.len() {
            d.select_internal(index, false);
            let value = d.value();
            let mut other = dropdown();
            other.set_value(value);
            assert_eq!(
                other.selected_index(),
                index,
                "option {index} did not survive the normalized round trip"
            );
        }
    }

    #[test]
    fn a_value_between_steps_rounds_to_the_nearest_option() {
        let mut d = dropdown();
        // Steps are at 0, 1/3, 2/3, 1. A value just off a step must not fall
        // to a neighbour.
        d.set_value(0.34);
        assert_eq!(d.selected_index(), 1);
        d.set_value(0.66);
        assert_eq!(d.selected_index(), 2);
        d.set_value(f32::NAN);
        assert_eq!(d.selected_index(), 2, "NaN moved the selection");
        d.set_value(5.0);
        assert_eq!(d.selected_index(), 3);
    }

    #[test]
    fn host_sync_never_echoes_back_to_the_host() {
        let mut d = dropdown();
        let count = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&count);
        d.on_value_changed(Box::new(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
        }));
        d.set_value(1.0);
        assert_eq!(d.selected_index(), 3);
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_user_pick_reports_the_normalized_value_once() {
        let mut d = dropdown();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        d.on_value_changed(Box::new(move |value| {
            sink.lock().unwrap().push(value);
        }));
        click(&mut d, 10.0, 10.0);
        let y = d.bounds.height + OPTION_HEIGHT * 3.0 + 1.0;
        click(&mut d, 10.0, y);
        assert_eq!(*seen.lock().unwrap(), vec![1.0]);
    }

    #[test]
    fn arrows_walk_the_options_and_clamp_at_the_ends() {
        let mut d = dropdown();
        let press = |code| Event::KeyDown {
            key: code,
            modifiers: Modifiers::default(),
        };
        // Unfocused: ignored.
        assert!(!d.handle_event(&press(crate::KeyCode::Down)));
        d.set_focused(true);

        assert!(d.handle_event(&press(crate::KeyCode::Down)));
        assert_eq!(d.selected_label(), "Warm");
        assert!(d.handle_event(&press(crate::KeyCode::Up)));
        assert_eq!(d.selected_label(), "Clean");
        // Already at the first option: stays put rather than wrapping, so a
        // held arrow key cannot silently jump to the far end.
        d.handle_event(&press(crate::KeyCode::Up));
        assert_eq!(d.selected_index(), 0);

        assert!(d.handle_event(&press(crate::KeyCode::End)));
        assert_eq!(d.selected_label(), "Crush");
        d.handle_event(&press(crate::KeyCode::Down));
        assert_eq!(d.selected_index(), 3);
        assert!(d.handle_event(&press(crate::KeyCode::Home)));
        assert_eq!(d.selected_index(), 0);
    }

    #[test]
    fn space_opens_the_list_and_escape_closes_it_without_changing_the_value() {
        let mut d = dropdown();
        d.set_focused(true);
        let press = |code| Event::KeyDown {
            key: code,
            modifiers: Modifiers::default(),
        };
        assert!(d.handle_event(&press(crate::KeyCode::Space)));
        assert!(d.is_open());
        assert!(d.handle_event(&press(crate::KeyCode::Escape)));
        assert!(!d.is_open());
        assert_eq!(d.selected_index(), 0, "escape changed the selection");
    }

    #[test]
    fn a_keyboard_pick_reports_the_normalized_value() {
        let mut d = dropdown();
        d.set_focused(true);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        d.on_value_changed(Box::new(move |value| sink.lock().unwrap().push(value)));
        d.handle_event(&Event::KeyDown {
            key: crate::KeyCode::End,
            modifiers: Modifiers::default(),
        });
        assert_eq!(*seen.lock().unwrap(), vec![1.0]);
    }
    #[test]
    fn an_empty_or_single_option_dropdown_is_inert_rather_than_panicking() {
        let mut empty = Dropdown::new("mode", &[]).with_bounds(Rect::new(0.0, 0.0, 60.0, 20.0));
        assert!(!click(&mut empty, 5.0, 5.0));
        assert_eq!(empty.selected_label(), "");
        assert_eq!(empty.value(), 0.0);
        let mut ctx = NullContext::new(100.0, 100.0);
        empty.draw(&mut ctx);

        let mut single =
            Dropdown::new("mode", &["Only"]).with_bounds(Rect::new(0.0, 0.0, 60.0, 20.0));
        single.set_value(1.0);
        assert_eq!(single.selected_index(), 0);
        assert_eq!(single.value(), 0.0, "a lone option must not divide by zero");
    }

    #[test]
    fn losing_focus_closes_the_list() {
        let mut d = dropdown();
        click(&mut d, 10.0, 10.0);
        assert!(d.is_open());
        d.set_focused(false);
        assert!(!d.is_open());
    }

    #[test]
    fn a_disabled_dropdown_ignores_input() {
        let mut d = dropdown();
        d.set_disabled(true);
        assert!(!click(&mut d, 10.0, 10.0));
        assert!(!d.is_open());
    }
}
