//! Accessibility tree.
//!
//! A custom-drawn editor is opaque to assistive technology: the platform sees
//! one rectangle. This turns the widget tree into a description a bridge can
//! hand to UI Automation, NSAccessibility or AT-SPI.
//!
//! The tree is built here, renderer- and platform-agnostic, because the shape
//! is the same for all three and it is the part worth testing. The per-platform
//! bridging is not implemented yet — see `docs/phase4/status.md`.

use crate::{Stack, Widget};

/// What kind of control a node describes.
///
/// Deliberately small: these are the roles the widget set actually has, and
/// each maps onto a real control type in all three platform APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessibleRole {
    /// A continuous value, such as a knob or slider.
    Slider,
    /// A two-state switch.
    CheckBox,
    /// A choice from a fixed list.
    ComboBox,
    /// A momentary action.
    Button,
    /// Non-interactive text.
    Label,
    /// A non-interactive display, such as a meter.
    Graphic,
    /// A container that groups other nodes.
    Group,
}

/// One entry in the accessibility tree.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessibleNode {
    pub role: AccessibleRole,
    /// What a screen reader announces. For a bound control this is the
    /// parameter id, which is the only human-meaningful name a widget has
    /// today; a future `Widget::accessible_label` can override it.
    pub label: String,
    /// The spoken value, e.g. "0.75" or "Warm". Empty for non-controls.
    pub value: String,
    /// Normalized position for range controls, so a bridge can report
    /// `RangeValue` without re-deriving it.
    pub normalized: Option<f32>,
    /// Bounds in editor coordinates. A bridge converts to screen coordinates.
    pub bounds: crate::Rect,
    pub focused: bool,
    pub disabled: bool,
    pub children: Vec<AccessibleNode>,
}

impl AccessibleNode {
    /// Depth-first walk including this node.
    pub fn walk(&self) -> Vec<&AccessibleNode> {
        let mut out = vec![self];
        for child in &self.children {
            out.extend(child.walk());
        }
        out
    }

    /// Total node count including this one.
    pub fn len(&self) -> usize {
        self.walk().len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }
}

/// Build the accessibility tree for a [`Stack`].
///
/// Every child becomes a node. Controls report the role they declare via
/// [`ParameterWidget::accessible_role`], their spoken value and their
/// normalized position; anything else is a [`AccessibleRole::Graphic`], which
/// is how a meter should read — present and describable, but not interactive.
///
/// ```
/// # use sunmao_gui::{accessibility_tree, AccessibleRole, Column, Rect, Toggle};
/// let mut editor = Column::new()
///     .child(Toggle::new("bypass").with_bounds(Rect::new(0.0, 0.0, 40.0, 20.0)));
/// editor.layout(Rect::new(0.0, 0.0, 100.0, 100.0));
/// let tree = accessibility_tree(&mut editor, "My Plugin");
/// assert_eq!(tree.children[0].role, AccessibleRole::CheckBox);
/// assert_eq!(tree.children[0].label, "bypass");
/// assert_eq!(tree.children[0].value, "Off");
/// ```
///
/// [`ParameterWidget::accessible_role`]: crate::ParameterWidget::accessible_role
pub fn accessibility_tree(stack: &mut Stack, label: &str) -> AccessibleNode {
    let bounds = stack.bounds();
    let focused = stack.focused();
    let mut children = Vec::with_capacity(stack.len());
    for index in 0..stack.len() {
        let Some(child) = stack.child_at_mut(index) else {
            continue;
        };
        let child_bounds = child.bounds();
        let state = child.state();
        // Focus is read from the stack for *every* child, including displays.
        // `Stack::set_focus` only bounds-checks the index, so a meter can hold
        // focus; a tree that reported `false` there would tell a screen reader
        // nothing is focused while the keyboard is in fact sitting on it.
        let focused = focused == Some(index);
        let node = match child.as_parameter() {
            Some(param) => AccessibleNode {
                role: param.accessible_role(),
                label: param.param_id().to_string(),
                value: param.display_value(),
                normalized: Some(param.value()),
                bounds: child_bounds,
                focused,
                disabled: state.disabled,
                children: Vec::new(),
            },
            None => AccessibleNode {
                role: AccessibleRole::Graphic,
                label: String::new(),
                value: String::new(),
                normalized: None,
                bounds: child_bounds,
                focused,
                disabled: state.disabled,
                children: Vec::new(),
            },
        };
        children.push(node);
    }
    AccessibleNode {
        role: AccessibleRole::Group,
        label: label.to_string(),
        value: String::new(),
        normalized: None,
        bounds,
        focused: false,
        disabled: false,
        children,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Column, Dropdown, Knob, Rect, SpectrumAnalyzer, StaticSpectrum, Toggle};

    fn editor() -> Stack {
        let mut stack = Column::new()
            .child(Knob::new("gain").with_bounds(Rect::new(0.0, 0.0, 40.0, 40.0)))
            .child(
                Dropdown::new("mode", &["Clean", "Warm"])
                    .with_bounds(Rect::new(0.0, 0.0, 80.0, 20.0)),
            )
            .child(Toggle::new("bypass").with_bounds(Rect::new(0.0, 0.0, 40.0, 20.0)))
            .child(
                SpectrumAnalyzer::new(Box::new(StaticSpectrum::new(vec![0.5f32; 4])))
                    .with_bounds(Rect::new(0.0, 0.0, 100.0, 30.0)),
            );
        stack.layout(Rect::new(0.0, 0.0, 200.0, 200.0));
        stack
    }

    /// Without this an editor is one opaque rectangle to a screen reader.
    #[test]
    fn every_control_appears_with_a_role_a_platform_can_map() {
        let mut stack = editor();
        let tree = accessibility_tree(&mut stack, "SunMao Widgets");
        assert_eq!(tree.role, AccessibleRole::Group);
        assert_eq!(tree.label, "SunMao Widgets");
        assert_eq!(tree.children.len(), 4);
        assert_eq!(tree.len(), 5, "the walk missed a node");

        let roles: Vec<AccessibleRole> = tree.children.iter().map(|node| node.role).collect();
        assert_eq!(
            roles,
            vec![
                AccessibleRole::Slider,
                AccessibleRole::ComboBox,
                AccessibleRole::CheckBox,
                // A meter is describable but not interactive.
                AccessibleRole::Graphic,
            ]
        );
    }

    #[test]
    fn controls_report_a_spoken_value_and_a_normalized_position() {
        let mut stack = editor();
        stack
            .child_at_mut(0)
            .unwrap()
            .as_parameter()
            .unwrap()
            .set_value(0.75);
        let tree = accessibility_tree(&mut stack, "editor");

        let knob = &tree.children[0];
        assert_eq!(knob.label, "gain");
        assert_eq!(knob.value, "0.75");
        assert_eq!(knob.normalized, Some(0.75));

        // A dropdown speaks its option, not a float: "Clean" is what a user
        // needs to hear, while `normalized` carries the machine-readable form.
        let dropdown = &tree.children[1];
        assert_eq!(dropdown.value, "Clean");
        assert_eq!(dropdown.normalized, Some(0.0));

        // A display has no value to speak.
        assert_eq!(tree.children[3].value, "");
        assert_eq!(tree.children[3].normalized, None);
    }

    #[test]
    fn focus_and_disabled_state_reach_the_tree() {
        let mut stack = editor();
        stack.set_focus(Some(2));
        let tree = accessibility_tree(&mut stack, "editor");
        assert!(tree.children[2].focused, "focus did not reach the tree");
        assert_eq!(
            tree.children.iter().filter(|node| node.focused).count(),
            1,
            "more than one node claimed focus"
        );
        assert!(tree.children.iter().all(|node| !node.disabled));
    }

    #[test]
    fn bounds_are_reported_so_a_bridge_can_hit_test() {
        let mut stack = editor();
        let tree = accessibility_tree(&mut stack, "editor");
        for node in &tree.children {
            assert!(
                node.bounds.width > 0.0 && node.bounds.height > 0.0,
                "a node has no area: {:?}",
                node.bounds
            );
        }
        // Children sit inside their group.
        for node in &tree.children {
            assert!(node.bounds.y >= tree.bounds.y);
        }
    }

    /// The reason roles are declared rather than sniffed from the displayed
    /// text: these options *are* numbers, and calling this a slider would tell
    /// a screen-reader user something false about how to operate it.
    #[test]
    fn a_dropdown_of_numeric_options_is_still_a_dropdown() {
        let mut stack = Column::new().child(
            Dropdown::new("ratio", &["1", "2", "4"]).with_bounds(Rect::new(0.0, 0.0, 60.0, 20.0)),
        );
        stack.layout(Rect::new(0.0, 0.0, 100.0, 100.0));
        let tree = accessibility_tree(&mut stack, "editor");
        assert_eq!(tree.children[0].role, AccessibleRole::ComboBox);
        assert_eq!(tree.children[0].value, "1");
    }

    #[test]
    fn an_empty_editor_still_produces_a_root() {
        let mut stack = Column::new();
        stack.layout(Rect::new(0.0, 0.0, 10.0, 10.0));
        let tree = accessibility_tree(&mut stack, "empty");
        assert_eq!(tree.children.len(), 0);
        assert_eq!(tree.len(), 1);
        assert!(!tree.is_empty(), "the root itself is always a node");
    }
}
