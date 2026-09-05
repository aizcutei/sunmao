//! Declarative stacks: [`Column`] and [`Row`].
//!
//! These own their children and compute every child's bounds from one call to
//! [`Stack::layout`], replacing the hand-written `set_bounds` arithmetic each
//! editor used to carry. The sizing rule is deliberately small enough to keep
//! in your head:
//!
//! - **Main axis** (down for a column, across for a row): each child keeps the
//!   size it was built with. Children are placed in order, separated by `gap`.
//! - **Cross axis**: each child stretches to fill the padded width (column) or
//!   height (row).
//!
//! There is no flex/grow yet — a fixed rule that always does the same thing is
//! easier to build on than a half-implemented flexbox.

use crate::{Event, GuiContext, Padding, Rect, Theme, Widget};

/// Which way a [`Stack`] lays its children out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Top to bottom.
    Vertical,
    /// Left to right.
    Horizontal,
}

/// A container that owns its children and assigns their bounds.
///
/// Use [`Column`] or [`Row`] rather than constructing this directly.
///
/// ```
/// # use sunmao_gui::{Column, Rect, Toggle, Widget};
/// let mut column = Column::new()
///     .gap(10.0)
///     .padding(8.0)
///     .child(Toggle::new("a").with_bounds(Rect::new(0.0, 0.0, 40.0, 20.0)))
///     .child(Toggle::new("b").with_bounds(Rect::new(0.0, 0.0, 40.0, 20.0)));
/// column.layout(Rect::new(0.0, 0.0, 100.0, 100.0));
/// // First child sits at the padded origin; the second is one gap below it.
/// assert_eq!(column.child_bounds(0).unwrap().y, 8.0);
/// assert_eq!(column.child_bounds(1).unwrap().y, 8.0 + 20.0 + 10.0);
/// // Both stretch across the padded width.
/// assert_eq!(column.child_bounds(0).unwrap().width, 100.0 - 16.0);
/// ```
pub struct Stack {
    axis: Axis,
    gap: f32,
    padding: Padding,
    children: Vec<Box<dyn Widget + Send>>,
    bounds: Rect,
    /// Index of the keyboard-focused child, if any.
    focused: Option<usize>,
}

impl Stack {
    pub fn new(axis: Axis) -> Self {
        Self {
            axis,
            gap: Theme::dark().spacing,
            padding: Padding::all(0.0),
            children: Vec::new(),
            bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
            focused: None,
        }
    }

    /// Space between adjacent children. Negative values are treated as zero:
    /// overlapping siblings would make hit-testing ambiguous.
    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = if gap.is_finite() { gap.max(0.0) } else { 0.0 };
        self
    }

    /// Uniform padding inside the container.
    pub fn padding(mut self, padding: f32) -> Self {
        let padding = if padding.is_finite() {
            padding.max(0.0)
        } else {
            0.0
        };
        self.padding = Padding::all(padding);
        self
    }

    /// Per-edge padding.
    pub fn padding_each(mut self, padding: Padding) -> Self {
        self.padding = padding;
        self
    }

    pub fn child<W: Widget + Send + 'static>(mut self, child: W) -> Self {
        self.children.push(Box::new(child));
        self
    }

    pub fn len(&self) -> usize {
        self.children.len()
    }

    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    pub fn child_bounds(&self, index: usize) -> Option<Rect> {
        self.children.get(index).map(|child| child.bounds())
    }

    pub fn child_at(&self, index: usize) -> Option<&(dyn Widget + Send)> {
        self.children.get(index).map(|child| child.as_ref())
    }

    pub fn child_at_mut(&mut self, index: usize) -> Option<&mut (dyn Widget + Send + 'static)> {
        self.children.get_mut(index).map(|child| child.as_mut())
    }

    pub fn iter(&self) -> impl Iterator<Item = &(dyn Widget + Send)> + '_ {
        self.children.iter().map(|child| child.as_ref())
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut (dyn Widget + Send + 'static)> + '_ {
        self.children.iter_mut().map(|child| child.as_mut())
    }

    /// Total extent along the main axis, including padding and gaps. Useful for
    /// asking the host to resize the editor to fit its content.
    pub fn content_extent(&self) -> f32 {
        let (lead, trail) = match self.axis {
            Axis::Vertical => (self.padding.top, self.padding.bottom),
            Axis::Horizontal => (self.padding.left, self.padding.right),
        };
        let sizes: f32 = self
            .children
            .iter()
            .map(|child| match self.axis {
                Axis::Vertical => child.bounds().height,
                Axis::Horizontal => child.bounds().width,
            })
            .sum();
        let gaps = self.gap * (self.children.len().saturating_sub(1)) as f32;
        lead + sizes + gaps + trail
    }

    /// Assign bounds to every child inside `area`.
    ///
    /// Children keep their main-axis size and stretch across the cross axis.
    /// A container smaller than its padding yields zero-width/height children
    /// rather than negative ones.
    pub fn layout(&mut self, area: Rect) {
        self.bounds = area;
        let inner_x = area.x + self.padding.left;
        let inner_y = area.y + self.padding.top;
        let inner_width = (area.width - self.padding.left - self.padding.right).max(0.0);
        let inner_height = (area.height - self.padding.top - self.padding.bottom).max(0.0);

        let mut cursor = match self.axis {
            Axis::Vertical => inner_y,
            Axis::Horizontal => inner_x,
        };
        for child in &mut self.children {
            let current = child.bounds();
            let rect = match self.axis {
                Axis::Vertical => Rect::new(inner_x, cursor, inner_width, current.height),
                Axis::Horizontal => Rect::new(cursor, inner_y, current.width, inner_height),
            };
            child.set_bounds(rect);
            child.layout();
            cursor += match self.axis {
                Axis::Vertical => current.height,
                Axis::Horizontal => current.width,
            } + self.gap;
        }
    }

    /// Draw every child in insertion order, so later children paint on top.
    pub fn draw(&self, ctx: &mut dyn GuiContext) {
        for child in &self.children {
            child.draw(ctx);
        }
    }

    /// Index of the focused child.
    pub fn focused(&self) -> Option<usize> {
        self.focused
    }

    /// Move keyboard focus. An out-of-range index clears focus rather than
    /// leaving a stale one, which would send keys to a child that no longer
    /// exists.
    pub fn set_focus(&mut self, index: Option<usize>) {
        let index = index.filter(|i| *i < self.children.len());
        if index == self.focused {
            return;
        }
        if let Some(previous) = self.focused.and_then(|i| self.children.get_mut(i)) {
            previous.set_focused(false);
        }
        if let Some(next) = index.and_then(|i| self.children.get_mut(i)) {
            next.set_focused(true);
        }
        self.focused = index;
    }

    /// Focus the next child, wrapping. Called for Tab.
    pub fn focus_next(&mut self) {
        if self.children.is_empty() {
            return;
        }
        let next = match self.focused {
            Some(current) => (current + 1) % self.children.len(),
            None => 0,
        };
        self.set_focus(Some(next));
    }

    /// Focus the previous child, wrapping. Called for Shift-Tab.
    pub fn focus_prev(&mut self) {
        if self.children.is_empty() {
            return;
        }
        let previous = match self.focused {
            Some(0) | None => self.children.len() - 1,
            Some(current) => current - 1,
        };
        self.set_focus(Some(previous));
    }

    /// Dispatch an event.
    ///
    /// Keyboard and text events go **only** to the focused child: broadcasting
    /// them would let one keystroke move several controls at once. Mouse events
    /// go to children in reverse paint order, so the child drawn on top gets
    /// first refusal, and a press moves focus to whatever accepted it.
    pub fn handle_event(&mut self, event: &Event) -> bool {
        match event {
            Event::KeyDown {
                key: crate::KeyCode::Tab,
                modifiers,
            } => {
                if modifiers.shift {
                    self.focus_prev();
                } else {
                    self.focus_next();
                }
                return true;
            }
            Event::KeyDown { .. } | Event::KeyUp { .. } | Event::TextInput { .. } => {
                let Some(index) = self.focused else {
                    return false;
                };
                return self
                    .children
                    .get_mut(index)
                    .is_some_and(|child| child.handle_event(event));
            }
            _ => {}
        }

        let mut consumed_by = None;
        for (index, child) in self.children.iter_mut().enumerate().rev() {
            if child.handle_event(event) {
                consumed_by = Some(index);
                break;
            }
        }
        if let Some(index) = consumed_by {
            if matches!(event, Event::MouseDown { .. }) {
                self.set_focus(Some(index));
            }
            return true;
        }
        false
    }
}

/// A top-to-bottom [`Stack`].
pub struct Column;

impl Column {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Stack {
        Stack::new(Axis::Vertical)
    }
}

/// A left-to-right [`Stack`].
pub struct Row;

impl Row {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Stack {
        Stack::new(Axis::Horizontal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NullContext, Toggle};

    fn toggle(width: f32, height: f32) -> Toggle {
        Toggle::new("p").with_bounds(Rect::new(0.0, 0.0, width, height))
    }

    #[test]
    fn a_column_stacks_downward_with_gaps_and_padding() {
        let mut column = Column::new()
            .gap(10.0)
            .padding(8.0)
            .child(toggle(40.0, 20.0))
            .child(toggle(40.0, 30.0));
        column.layout(Rect::new(5.0, 7.0, 100.0, 200.0));

        let first = column.child_bounds(0).unwrap();
        let second = column.child_bounds(1).unwrap();
        assert_eq!((first.x, first.y), (13.0, 15.0));
        assert_eq!((second.x, second.y), (13.0, 15.0 + 20.0 + 10.0));
        // Cross axis stretches to the padded width.
        assert_eq!(first.width, 100.0 - 16.0);
        assert_eq!(second.width, 100.0 - 16.0);
        // Main axis keeps each child's own size.
        assert_eq!(first.height, 20.0);
        assert_eq!(second.height, 30.0);
    }

    #[test]
    fn a_row_stacks_rightward_and_stretches_height() {
        let mut row = Row::new()
            .gap(4.0)
            .padding(2.0)
            .child(toggle(20.0, 10.0))
            .child(toggle(30.0, 10.0));
        row.layout(Rect::new(0.0, 0.0, 200.0, 50.0));

        let first = row.child_bounds(0).unwrap();
        let second = row.child_bounds(1).unwrap();
        assert_eq!(first.x, 2.0);
        assert_eq!(second.x, 2.0 + 20.0 + 4.0);
        assert_eq!(first.height, 50.0 - 4.0);
        assert_eq!(first.width, 20.0);
        assert_eq!(second.width, 30.0);
    }

    #[test]
    fn a_container_smaller_than_its_padding_yields_no_negative_sizes() {
        let mut column = Column::new().padding(50.0).child(toggle(40.0, 20.0));
        column.layout(Rect::new(0.0, 0.0, 10.0, 10.0));
        let child = column.child_bounds(0).unwrap();
        assert!(child.width >= 0.0, "negative width {}", child.width);
    }

    #[test]
    fn nonsense_gap_and_padding_are_neutralised() {
        let column = Column::new().gap(f32::NAN).padding(-10.0);
        assert_eq!(column.gap, 0.0);
        assert_eq!(column.padding, Padding::all(0.0));
        let column = Column::new().gap(-5.0);
        assert_eq!(column.gap, 0.0, "a negative gap would overlap siblings");
    }

    #[test]
    fn content_extent_accounts_for_padding_and_gaps() {
        let column = Column::new()
            .gap(10.0)
            .padding(8.0)
            .child(toggle(40.0, 20.0))
            .child(toggle(40.0, 30.0));
        // 8 + 20 + 10 + 30 + 8
        assert_eq!(column.content_extent(), 76.0);
        assert_eq!(Column::new().content_extent(), 0.0);
    }

    fn key(code: crate::KeyCode) -> crate::Event {
        crate::Event::KeyDown {
            key: code,
            modifiers: crate::Modifiers::default(),
        }
    }

    fn shift_key(code: crate::KeyCode) -> crate::Event {
        crate::Event::KeyDown {
            key: code,
            modifiers: crate::Modifiers {
                shift: true,
                ..Default::default()
            },
        }
    }

    #[test]
    fn tab_walks_focus_forward_and_shift_tab_walks_it_back() {
        let mut column = Column::new()
            .child(toggle(40.0, 20.0))
            .child(toggle(40.0, 20.0))
            .child(toggle(40.0, 20.0));
        column.layout(Rect::new(0.0, 0.0, 100.0, 200.0));
        assert_eq!(column.focused(), None);

        column.handle_event(&key(crate::KeyCode::Tab));
        assert_eq!(column.focused(), Some(0));
        column.handle_event(&key(crate::KeyCode::Tab));
        assert_eq!(column.focused(), Some(1));
        // Forward wraps.
        column.handle_event(&key(crate::KeyCode::Tab));
        column.handle_event(&key(crate::KeyCode::Tab));
        assert_eq!(column.focused(), Some(0));
        // Backward wraps too.
        column.handle_event(&shift_key(crate::KeyCode::Tab));
        assert_eq!(column.focused(), Some(2));
    }

    /// A keystroke must reach exactly one control. Broadcasting it would let a
    /// single arrow press move every knob in the editor at once.
    #[test]
    fn keys_reach_only_the_focused_child() {
        let mut column = Column::new()
            .child(Toggle::new("a").with_bounds(Rect::new(0.0, 0.0, 40.0, 20.0)))
            .child(Toggle::new("b").with_bounds(Rect::new(0.0, 0.0, 40.0, 20.0)));
        column.layout(Rect::new(0.0, 0.0, 100.0, 100.0));
        column.set_focus(Some(1));

        assert!(column.handle_event(&key(crate::KeyCode::Space)));
        let values: Vec<f32> = (0..2)
            .map(|i| {
                column
                    .child_at_mut(i)
                    .unwrap()
                    .as_parameter()
                    .unwrap()
                    .value()
            })
            .collect();
        assert_eq!(
            values,
            vec![0.0, 1.0],
            "the keystroke hit the wrong control"
        );
    }

    #[test]
    fn keys_with_no_focus_are_ignored_rather_than_broadcast() {
        let mut column = Column::new().child(toggle(40.0, 20.0));
        column.layout(Rect::new(0.0, 0.0, 100.0, 100.0));
        assert!(!column.handle_event(&key(crate::KeyCode::Space)));
    }

    #[test]
    fn a_mouse_press_moves_focus_to_what_it_hit() {
        let mut column = Column::new()
            .gap(0.0)
            .child(toggle(40.0, 20.0))
            .child(toggle(40.0, 20.0));
        column.layout(Rect::new(0.0, 0.0, 100.0, 100.0));
        let press = crate::Event::MouseDown {
            x: 5.0,
            y: 25.0,
            button: crate::MouseButton::Left,
            modifiers: crate::Modifiers::default(),
        };
        assert!(column.handle_event(&press));
        assert_eq!(column.focused(), Some(1));
    }

    #[test]
    fn focus_rejects_an_index_past_the_end() {
        let mut column = Column::new().child(toggle(40.0, 20.0));
        column.set_focus(Some(99));
        assert_eq!(column.focused(), None, "a stale index kept focus");
    }
    #[test]
    fn events_reach_the_topmost_child_first() {
        // Two children at the same spot: the later one paints on top and must
        // win the event.
        let mut column = Column::new().gap(0.0).child(toggle(40.0, 20.0));
        column.layout(Rect::new(0.0, 0.0, 40.0, 40.0));
        let mut ctx = NullContext::new(40.0, 40.0);
        column.draw(&mut ctx);
        assert_eq!(column.len(), 1);
        assert!(!column.is_empty());
    }
}
