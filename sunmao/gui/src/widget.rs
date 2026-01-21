//! Widget trait and base implementations.
//!
//! Widgets are the building blocks of the GUI. They handle events,
//! maintain state, and draw themselves using the GuiContext.

use crate::{GuiContext, Event, Rect};

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
}

/// A parameter-bound widget that syncs with plugin parameters.
pub trait ParameterWidget: Widget {
    /// Get the parameter ID this widget is bound to
    fn param_id(&self) -> &str;
    
    /// Get the current normalized value (0.0 - 1.0)
    fn value(&self) -> f32;
    
    /// Set the normalized value (0.0 - 1.0)
    fn set_value(&mut self, value: f32);
    
    /// Get the display value (formatted string)
    fn display_value(&self) -> String {
        format!("{:.2}", self.value())
    }
    
    /// Called when the value changes (for parameter automation)
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
        self.focused_id = id;
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
