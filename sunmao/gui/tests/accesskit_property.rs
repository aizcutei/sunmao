//! Property tests for the AccessKit translation.
//!
//! AccessKit is strict about tree shape — it rejects an update whose node is
//! neither the root nor some node's child, and a duplicated or self-referential
//! id makes a platform bridge walk in circles. None of that shows up in a unit
//! test over one hand-built editor, so these pin the invariants over arbitrary
//! ones.

#![cfg(feature = "accessibility")]

use accesskit::NodeId;
use proptest::prelude::*;
use std::collections::HashSet;
use sunmao_gui::{
    accessibility_tree, accesskit_update, Column, Dropdown, Knob, Rect, Row, Slider,
    SpectrumAnalyzer, Stack, StaticSpectrum, Toggle,
};

#[derive(Debug, Clone, Copy)]
enum Kind {
    Knob,
    Slider,
    Toggle,
    Dropdown,
    Display,
}

fn kind_strategy() -> impl Strategy<Value = Kind> {
    prop_oneof![
        Just(Kind::Knob),
        Just(Kind::Slider),
        Just(Kind::Toggle),
        Just(Kind::Dropdown),
        Just(Kind::Display),
    ]
}

fn build(kinds: &[Kind], horizontal: bool) -> Stack {
    let mut stack = if horizontal {
        Row::new()
    } else {
        Column::new()
    };
    for (index, kind) in kinds.iter().enumerate() {
        let id = format!("p{index}");
        let bounds = Rect::new(0.0, 0.0, 40.0, 20.0);
        stack = match kind {
            Kind::Knob => stack.child(Knob::new(&id).with_bounds(bounds)),
            Kind::Slider => stack.child(Slider::new(&id).with_bounds(bounds)),
            Kind::Toggle => stack.child(Toggle::new(&id).with_bounds(bounds)),
            Kind::Dropdown => {
                stack.child(Dropdown::new(&id, &["Clean", "Warm"]).with_bounds(bounds))
            }
            Kind::Display => stack.child(
                SpectrumAnalyzer::new(Box::new(StaticSpectrum::new(vec![0.5f32; 4])))
                    .with_bounds(bounds),
            ),
        };
    }
    stack.layout(Rect::new(0.0, 0.0, 400.0, 400.0));
    stack
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Every node must be reachable from the root exactly once. AccessKit
    /// rejects an update containing a node that is neither the root nor a
    /// child, and a node claimed by two parents corrupts the platform's walk.
    #[test]
    fn the_update_is_a_tree_rooted_at_the_root(
        kinds in prop::collection::vec(kind_strategy(), 0..10),
        horizontal in any::<bool>(),
        focus in prop::option::of(0usize..12),
    ) {
        let mut stack = build(&kinds, horizontal);
        stack.set_focus(focus);
        let update = accesskit_update(&accessibility_tree(&mut stack, "editor"));

        let ids: Vec<NodeId> = update.nodes.iter().map(|(id, _)| *id).collect();
        let unique: HashSet<u64> = ids.iter().map(|id| id.0).collect();
        prop_assert_eq!(unique.len(), ids.len(), "duplicate node ids");

        let root = update.tree.as_ref().expect("an initial update carries tree info").root;
        prop_assert!(unique.contains(&root.0), "root is not among the nodes");

        // Count how many parents claim each id.
        let mut claimed: Vec<u64> = Vec::new();
        for (id, node) in &update.nodes {
            for child in node.children() {
                prop_assert_ne!(*child, *id, "a node is its own child");
                claimed.push(child.0);
            }
        }
        let claimed_unique: HashSet<u64> = claimed.iter().copied().collect();
        prop_assert_eq!(claimed_unique.len(), claimed.len(), "a node has two parents");

        // Everything except the root is claimed exactly once, and every claimed
        // id actually exists.
        for id in &unique {
            if *id == root.0 {
                prop_assert!(!claimed_unique.contains(id), "the root is someone's child");
            } else {
                prop_assert!(claimed_unique.contains(id), "node {} is unreachable", id);
            }
        }
        for id in &claimed_unique {
            prop_assert!(unique.contains(id), "child {} does not exist", id);
        }
    }

    /// Focus must name a node that exists. AccessKit requires a focus target on
    /// every update, and a dangling one is worse than none: the platform asks
    /// for a node the tree cannot produce.
    #[test]
    fn focus_always_names_a_node_that_exists(
        kinds in prop::collection::vec(kind_strategy(), 0..10),
        horizontal in any::<bool>(),
        focus in prop::option::of(0usize..12),
    ) {
        let mut stack = build(&kinds, horizontal);
        stack.set_focus(focus);
        let tree = accessibility_tree(&mut stack, "editor");
        let expected_focus = tree.walk().iter().position(|node| node.focused);
        let update = accesskit_update(&tree);

        let ids: HashSet<u64> = update.nodes.iter().map(|(id, _)| id.0).collect();
        prop_assert!(ids.contains(&update.focus.0), "focus names a missing node");

        match expected_focus {
            Some(index) => prop_assert_eq!(update.focus.0, index as u64),
            // Nothing focused: fall back to the root rather than an arbitrary node.
            None => prop_assert_eq!(update.focus, update.tree.as_ref().unwrap().root),
        }
    }

    /// The translation must not lose or invent nodes.
    #[test]
    fn node_count_is_preserved(
        kinds in prop::collection::vec(kind_strategy(), 0..10),
        horizontal in any::<bool>(),
    ) {
        let mut stack = build(&kinds, horizontal);
        let tree = accessibility_tree(&mut stack, "editor");
        let update = accesskit_update(&tree);
        prop_assert_eq!(update.nodes.len(), tree.len());
        prop_assert_eq!(update.nodes.len(), kinds.len() + 1);
    }

    /// A numeric value, when present, must be finite and within the range the
    /// same node advertises — a screen reader computes a percentage from these.
    #[test]
    fn numeric_values_stay_inside_the_advertised_range(
        kinds in prop::collection::vec(kind_strategy(), 1..10),
        horizontal in any::<bool>(),
    ) {
        let mut stack = build(&kinds, horizontal);
        let update = accesskit_update(&accessibility_tree(&mut stack, "editor"));
        for (_, node) in &update.nodes {
            let Some(value) = node.numeric_value() else { continue };
            prop_assert!(value.is_finite(), "non-finite numeric value {}", value);
            let min = node.min_numeric_value().expect("a numeric node needs a minimum");
            let max = node.max_numeric_value().expect("a numeric node needs a maximum");
            prop_assert!(min <= value && value <= max, "{} outside {}..={}", value, min, max);
        }
    }
}
