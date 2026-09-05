//! Widget trait and base implementations.
//!
//! Widgets are the building blocks of the GUI. They handle events,
//! maintain state, and draw themselves using the GuiContext.

use crate::{Event, GuiContext, Rect};

/// Unique identifier for a widget
pub type WidgetId = u64;

/// Widget state flags
#[derive(Debug, Clone, Copy, Default)]
pub struct WidgetState {
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
    pub disabled: bool,
}

/// Base trait for all widgets.
///
/// Widgets are renderer-agnostic UI components that can draw themselves
/// and respond to user input.
pub trait Widget {
    /// Get the widget's unique identifier
    fn id(&self) -> WidgetId;

    /// Get the widget's bounding rectangle
    fn bounds(&self) -> Rect;

    /// Set the widget's bounding rectangle
    fn set_bounds(&mut self, bounds: Rect);

    /// Get the widget's current state
    fn state(&self) -> WidgetState;

    /// Update keyboard focus state. Custom widgets that expose focus styling
    /// should override this method.
    fn set_focused(&mut self, _focused: bool) {}

    /// Handle an input event
    /// Returns true if the event was consumed
    fn handle_event(&mut self, event: &Event) -> bool;

    /// Draw the widget
    fn draw(&self, ctx: &mut dyn GuiContext);

    /// Called when the widget needs to update its layout
    fn layout(&mut self) {}

    /// Check if a point is inside this widget
    fn hit_test(&self, x: f32, y: f32) -> bool {
        self.bounds().contains(x, y)
    }

    /// View this widget as a parameter control, if it is one.
    ///
    /// Every [`ParameterWidget`] overrides this. It exists so [`ParamBinder`]
    /// can find the controls inside a widget tree without downcasting to a
    /// concrete type, which would force the binder to know every widget that
    /// will ever exist.
    ///
    /// [`ParamBinder`]: crate::ParamBinder
    fn as_parameter(&mut self) -> Option<&mut dyn ParameterWidget> {
        None
    }
}

/// A parameter-bound widget that syncs with plugin parameters.
pub trait ParameterWidget: Widget {
    /// Get the parameter ID this widget is bound to
    fn param_id(&self) -> &str;

    /// Get the current normalized value (0.0 - 1.0)
    fn value(&self) -> f32;

    /// Set the normalized value (0.0 - 1.0) from plugin or host state.
    ///
    /// Programmatic synchronization must not invoke the value-changed callback,
    /// otherwise a host automation update can be echoed back to the host.
    fn set_value(&mut self, value: f32);

    /// Get the display value (formatted string)
    fn display_value(&self) -> String {
        format!("{:.2}", self.value())
    }

    /// Register a callback for value changes caused by user interaction.
    fn on_value_changed(&mut self, _callback: Box<dyn Fn(f32) + Send>) {}
}

/// Container for multiple widgets
pub struct WidgetContainer {
    widgets: Vec<Box<dyn Widget>>,
    focused_id: Option<WidgetId>,
}

impl WidgetContainer {
    pub fn new() -> Self {
        Self {
            widgets: Vec::new(),
            focused_id: None,
        }
    }

    pub fn add<W: Widget + 'static>(&mut self, widget: W) {
        self.widgets.push(Box::new(widget));
    }

    pub fn handle_event(&mut self, event: &Event) -> bool {
        // First, try the focused widget
        if let Some(focused_id) = self.focused_id {
            for widget in &mut self.widgets {
                if widget.id() == focused_id {
                    if widget.handle_event(event) {
                        return true;
                    }
                }
            }
        }

        // Then try all widgets in reverse order (top to bottom)
        for widget in self.widgets.iter_mut().rev() {
            // The focused widget already received the event above. Do not
            // dispatch it a second time when it declines the event; controls
            // are allowed to have side effects even for events they do not
            // ultimately consume.
            if self.focused_id == Some(widget.id()) {
                continue;
            }
            if widget.handle_event(event) {
                return true;
            }
        }

        false
    }

    pub fn draw(&self, ctx: &mut dyn GuiContext) {
        for widget in &self.widgets {
            widget.draw(ctx);
        }
    }

    pub fn layout(&mut self) {
        for widget in &mut self.widgets {
            widget.layout();
        }
    }

    pub fn set_focus(&mut self, id: Option<WidgetId>) {
        // Never retain a focus target that is not part of the container. This
        // keeps keyboard dispatch deterministic when callers use a stale ID.
        let id = id.filter(|requested| self.widgets.iter().any(|widget| widget.id() == *requested));
        if self.focused_id == id {
            return;
        }
        for widget in &mut self.widgets {
            if Some(widget.id()) == self.focused_id {
                widget.set_focused(false);
            }
            if Some(widget.id()) == id {
                widget.set_focused(true);
            }
        }
        self.focused_id = id;
    }

    /// Iterate over all widgets in paint order.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Widget> + '_ {
        self.widgets.iter().map(|widget| widget.as_ref())
    }

    pub fn widget_at(&self, x: f32, y: f32) -> Option<&dyn Widget> {
        for widget in self.widgets.iter().rev() {
            if widget.hit_test(x, y) {
                return Some(widget.as_ref());
            }
        }
        None
    }
}

impl Default for WidgetContainer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Event, GuiContext, MouseButton, NullContext};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct CountingWidget {
        calls: Arc<AtomicUsize>,
        id: WidgetId,
    }

    impl Widget for CountingWidget {
        fn id(&self) -> WidgetId {
            self.id
        }
        fn bounds(&self) -> Rect {
            Rect::new(0.0, 0.0, 100.0, 100.0)
        }
        fn set_bounds(&mut self, _bounds: Rect) {}
        fn state(&self) -> WidgetState {
            WidgetState::default()
        }
        fn handle_event(&mut self, _event: &Event) -> bool {
            self.calls.fetch_add(1, Ordering::Relaxed);
            false
        }
        fn draw(&self, _ctx: &mut dyn GuiContext) {}
    }

    #[test]
    fn focused_widget_is_not_dispatched_twice() {
        let mut container = WidgetContainer::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let widget = CountingWidget {
            calls: Arc::clone(&calls),
            id: 7,
        };
        container.add(widget);
        container.set_focus(Some(7));
        let event = Event::MouseUp {
            x: 1.0,
            y: 1.0,
            button: MouseButton::Left,
            modifiers: Default::default(),
        };
        assert!(!container.handle_event(&event));
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        let mut ctx = NullContext::new(100.0, 100.0);
        container.draw(&mut ctx);
    }

    #[test]
    fn focus_rejects_unknown_widget_ids() {
        let mut container = WidgetContainer::new();
        container.set_focus(Some(99));
        let event = Event::MouseUp {
            x: 1.0,
            y: 1.0,
            button: MouseButton::Left,
            modifiers: Default::default(),
        };
        assert!(!container.handle_event(&event));
    }
}
