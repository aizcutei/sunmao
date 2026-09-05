//! Translate the [`AccessibleNode`] tree into AccessKit.
//!
//! AccessKit owns the three native bridges SunMao would otherwise have to write
//! and maintain — UI Automation on Windows, NSAccessibility on macOS, AT-SPI on
//! Linux. This module is the whole of SunMao's side: a pure data mapping, with
//! no platform code, so it is testable on any host.
//!
//! Behind the `accessibility` feature; see the feature's comment in Cargo.toml
//! for why it is off by default.

use crate::{AccessibleNode, AccessibleRole};
use accesskit::{Node, NodeId, Rect, Role, TreeId, TreeInfo, TreeUpdate};

/// Node id of the tree's root. Children are numbered from 1 in walk order.
pub const ROOT_ID: NodeId = NodeId(0);

/// Map a SunMao role onto AccessKit's vocabulary.
///
/// The mapping is deliberately conservative: each of these AccessKit roles has
/// a well-defined counterpart on all three platforms, so a screen reader
/// announces the same kind of control everywhere.
fn accesskit_role(role: AccessibleRole) -> Role {
    match role {
        AccessibleRole::Slider => Role::Slider,
        AccessibleRole::CheckBox => Role::CheckBox,
        AccessibleRole::ComboBox => Role::ComboBox,
        AccessibleRole::Button => Role::Button,
        AccessibleRole::Label => Role::Label,
        // AccessKit has no "meter"; `Image` is the closest thing that reads as
        // "present, describable, not interactive", which is what a spectrum
        // display is.
        AccessibleRole::Graphic => Role::Image,
        AccessibleRole::Group => Role::Group,
    }
}

/// Convert one node, without its children.
fn accesskit_node(source: &AccessibleNode) -> Node {
    let mut node = Node::new(accesskit_role(source.role));
    if !source.label.is_empty() {
        node.set_label(source.label.clone());
    }
    if !source.value.is_empty() {
        node.set_value(source.value.clone());
    }
    if let Some(value) = source.normalized {
        // Only a finite value is meaningful to a screen reader; a NaN would be
        // announced as garbage.
        if value.is_finite() {
            node.set_numeric_value(value as f64);
            node.set_min_numeric_value(0.0);
            node.set_max_numeric_value(1.0);
        }
    }
    let bounds = source.bounds;
    if bounds.width > 0.0 && bounds.height > 0.0 {
        node.set_bounds(Rect {
            x0: bounds.x as f64,
            y0: bounds.y as f64,
            x1: (bounds.x + bounds.width) as f64,
            y1: (bounds.y + bounds.height) as f64,
        });
    }
    if source.disabled {
        node.set_disabled();
    }
    node
}

/// Flatten `tree` into AccessKit nodes, assigning ids in depth-first order.
fn flatten(source: &AccessibleNode, next_id: &mut u64, out: &mut Vec<(NodeId, Node)>) -> NodeId {
    let id = NodeId(*next_id);
    *next_id += 1;
    // Reserve this node's slot before recursing, so a child can never be
    // assigned its parent's id.
    out.push((id, Node::new(Role::Unknown)));
    let index = out.len() - 1;

    let children: Vec<NodeId> = source
        .children
        .iter()
        .map(|child| flatten(child, next_id, out))
        .collect();

    let mut node = accesskit_node(source);
    if !children.is_empty() {
        node.set_children(children);
    }
    out[index] = (id, node);
    id
}

/// Build a complete AccessKit update from a SunMao accessibility tree.
///
/// `focus` follows the tree: whichever node reports `focused` becomes
/// AccessKit's focus, and the root is used when nothing is focused — AccessKit
/// requires a focus target, and pointing at the root reads as "the editor
/// itself" rather than at an arbitrary control.
///
/// ```
/// # use sunmao_gui::{accessibility_tree, accesskit_update, Column, Rect, Toggle};
/// let mut editor = Column::new()
///     .child(Toggle::new("bypass").with_bounds(Rect::new(0.0, 0.0, 40.0, 20.0)));
/// editor.layout(Rect::new(0.0, 0.0, 100.0, 100.0));
/// let update = accesskit_update(&accessibility_tree(&mut editor, "My Plugin"));
/// // Root plus one control.
/// assert_eq!(update.nodes.len(), 2);
/// assert_eq!(update.nodes[1].1.role(), accesskit::Role::CheckBox);
/// ```
pub fn accesskit_update(tree: &AccessibleNode) -> TreeUpdate {
    let mut nodes = Vec::new();
    let mut next_id = 0u64;
    let root = flatten(tree, &mut next_id, &mut nodes);

    // Depth-first ids mean `walk()` and `nodes` are in the same order, so the
    // focused node's id is its position in the walk.
    let focus = tree
        .walk()
        .iter()
        .position(|node| node.focused)
        .map(|index| NodeId(index as u64))
        .unwrap_or(root);

    // Name the toolkit: assistive technology and bug reports both benefit from
    // knowing which framework produced the tree.
    let mut info = TreeInfo::new(root);
    info.toolkit_name = Some("SunMao".to_string());
    info.toolkit_version = Some(env!("CARGO_PKG_VERSION").to_string());

    TreeUpdate {
        nodes,
        tree: Some(info),
        // The editor is the whole tree; SunMao publishes no subtrees.
        tree_id: TreeId::ROOT,
        focus,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        accessibility_tree, Column, Dropdown, Knob, Rect as GuiRect, SpectrumAnalyzer, Stack,
        StaticSpectrum, Toggle, Widget,
    };

    fn editor() -> Stack {
        let mut stack = Column::new()
            .child(Knob::new("gain").with_bounds(GuiRect::new(0.0, 0.0, 40.0, 40.0)))
            .child(
                Dropdown::new("mode", &["Clean", "Warm"])
                    .with_bounds(GuiRect::new(0.0, 0.0, 80.0, 20.0)),
            )
            .child(Toggle::new("bypass").with_bounds(GuiRect::new(0.0, 0.0, 40.0, 20.0)))
            .child(
                SpectrumAnalyzer::new(Box::new(StaticSpectrum::new(vec![0.5f32; 4])))
                    .with_bounds(GuiRect::new(0.0, 0.0, 100.0, 30.0)),
            );
        stack.layout(GuiRect::new(0.0, 0.0, 200.0, 200.0));
        stack
    }

    #[test]
    fn every_node_survives_the_translation_with_its_role() {
        let mut stack = editor();
        let tree = accessibility_tree(&mut stack, "SunMao Widgets");
        let update = accesskit_update(&tree);

        assert_eq!(update.nodes.len(), tree.len(), "a node was lost");
        let roles: Vec<Role> = update.nodes.iter().map(|(_, node)| node.role()).collect();
        assert_eq!(
            roles,
            vec![
                Role::Group,
                Role::Slider,
                Role::ComboBox,
                Role::CheckBox,
                Role::Image,
            ]
        );
    }

    /// Ids must be unique and a parent must list exactly its own children.
    /// A duplicated or crossed id makes the platform walk the tree in circles.
    #[test]
    fn ids_are_unique_and_children_are_wired_to_the_right_parent() {
        let mut stack = editor();
        let update = accesskit_update(&accessibility_tree(&mut stack, "editor"));

        let ids: Vec<NodeId> = update.nodes.iter().map(|(id, _)| *id).collect();
        let mut sorted = ids.clone();
        sorted.sort_by_key(|id| id.0);
        sorted.dedup_by_key(|id| id.0);
        assert_eq!(sorted.len(), ids.len(), "duplicate node ids");

        let (root_id, root) = &update.nodes[0];
        assert_eq!(*root_id, ROOT_ID);
        assert_eq!(root.children().len(), 4);
        // Children are the four ids following the root, in order.
        assert_eq!(
            root.children().to_vec(),
            vec![NodeId(1), NodeId(2), NodeId(3), NodeId(4)]
        );
        // No child claims children of its own.
        for (_, node) in &update.nodes[1..] {
            assert!(node.children().is_empty());
        }
    }

    #[test]
    fn a_control_carries_a_label_a_value_and_a_bounded_numeric_range() {
        let mut stack = editor();
        stack
            .child_at_mut(0)
            .unwrap()
            .as_parameter()
            .unwrap()
            .set_value(0.75);
        let update = accesskit_update(&accessibility_tree(&mut stack, "editor"));

        let (_, knob) = &update.nodes[1];
        assert_eq!(knob.label().as_deref(), Some("gain"));
        assert_eq!(knob.value().as_deref(), Some("0.75"));
        assert_eq!(knob.numeric_value(), Some(0.75));
        // Without a range a screen reader cannot say "75 percent".
        assert_eq!(knob.min_numeric_value(), Some(0.0));
        assert_eq!(knob.max_numeric_value(), Some(1.0));
        assert!(knob.bounds().is_some());
    }

    #[test]
    fn focus_points_at_the_focused_control_and_falls_back_to_the_root() {
        let mut stack = editor();
        let update = accesskit_update(&accessibility_tree(&mut stack, "editor"));
        assert_eq!(update.focus, ROOT_ID, "unfocused editor did not fall back");

        stack.set_focus(Some(2));
        let update = accesskit_update(&accessibility_tree(&mut stack, "editor"));
        // Child index 2 is the third child, whose depth-first id is 3.
        assert_eq!(update.focus, NodeId(3));
        let (_, focused) = &update.nodes[3];
        assert_eq!(focused.role(), Role::CheckBox);
    }

    /// A NaN would be read out as garbage, so it must not reach the platform.
    #[test]
    fn a_non_finite_value_is_left_off_rather_than_announced() {
        let node = AccessibleNode {
            role: AccessibleRole::Slider,
            label: "gain".into(),
            value: "NaN".into(),
            normalized: Some(f32::NAN),
            bounds: crate::Rect::new(0.0, 0.0, 10.0, 10.0),
            focused: false,
            disabled: false,
            children: Vec::new(),
        };
        let update = accesskit_update(&node);
        assert_eq!(update.nodes[0].1.numeric_value(), None);
    }

    #[test]
    fn an_empty_editor_still_produces_a_root_that_can_take_focus() {
        let mut stack = Column::new();
        stack.layout(GuiRect::new(0.0, 0.0, 10.0, 10.0));
        let update = accesskit_update(&accessibility_tree(&mut stack, "empty"));
        assert_eq!(update.nodes.len(), 1);
        assert_eq!(update.focus, ROOT_ID);
        assert_eq!(update.nodes[0].1.role(), Role::Group);
    }

    #[test]
    fn a_disabled_control_is_reported_as_disabled() {
        let node = AccessibleNode {
            role: AccessibleRole::Button,
            label: "go".into(),
            value: String::new(),
            normalized: None,
            bounds: crate::Rect::new(0.0, 0.0, 10.0, 10.0),
            focused: false,
            disabled: true,
            children: Vec::new(),
        };
        let update = accesskit_update(&node);
        assert!(update.nodes[0].1.is_disabled());
    }
}
