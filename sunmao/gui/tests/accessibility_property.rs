//! Property tests for the accessibility tree.
//!
//! The unit tests in `accessibility.rs` pin one hand-built editor. These pin
//! the *invariants* that must hold for any editor, because a bridge publishing
//! this tree to UI Automation / NSAccessibility / AT-SPI cannot defend itself
//! against a malformed one — it will hand whatever it gets to a screen reader.

use proptest::prelude::*;
use sunmao_gui::{
    accessibility_tree, AccessibleRole, Column, Dropdown, Knob, Rect, Row, Slider,
    SpectrumAnalyzer, Stack, StaticSpectrum, Toggle,
};

/// The control kinds an editor can be built from.
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

/// Build an editor from `kinds`, laid out in `area`.
fn build(kinds: &[Kind], horizontal: bool, area: Rect) -> Stack {
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
    stack.layout(area);
    stack
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Every child must produce exactly one node, in order. A bridge indexes
    /// into this tree to answer "what is at position N"; a dropped or
    /// reordered node makes a screen reader point at the wrong control.
    #[test]
    fn the_tree_mirrors_the_widget_tree_one_for_one(
        kinds in prop::collection::vec(kind_strategy(), 0..10),
        horizontal in any::<bool>(),
    ) {
        let mut stack = build(&kinds, horizontal, Rect::new(0.0, 0.0, 400.0, 400.0));
        let tree = accessibility_tree(&mut stack, "editor");
        prop_assert_eq!(tree.children.len(), kinds.len());
        prop_assert_eq!(tree.len(), kinds.len() + 1);
        prop_assert_eq!(tree.role, AccessibleRole::Group);

        for (node, kind) in tree.children.iter().zip(&kinds) {
            let expected = match kind {
                Kind::Knob | Kind::Slider => AccessibleRole::Slider,
                Kind::Toggle => AccessibleRole::CheckBox,
                Kind::Dropdown => AccessibleRole::ComboBox,
                Kind::Display => AccessibleRole::Graphic,
            };
            prop_assert_eq!(node.role, expected);
        }
    }

    /// A control must always be announceable: a name to say, a value to say,
    /// and a position a screen reader can report. A node missing any of these
    /// reads as an unlabelled blank to the user.
    #[test]
    fn every_control_is_announceable(
        kinds in prop::collection::vec(kind_strategy(), 1..10),
        horizontal in any::<bool>(),
    ) {
        let mut stack = build(&kinds, horizontal, Rect::new(0.0, 0.0, 400.0, 400.0));
        let tree = accessibility_tree(&mut stack, "editor");
        for node in &tree.children {
            if node.role == AccessibleRole::Graphic {
                // A display is describable but has nothing to announce.
                prop_assert!(node.normalized.is_none());
                continue;
            }
            prop_assert!(!node.label.is_empty(), "unlabelled control");
            prop_assert!(!node.value.is_empty(), "control with nothing to speak");
            let value = node.normalized.expect("a control must report a position");
            prop_assert!(
                value.is_finite() && (0.0..=1.0).contains(&value),
                "normalized value escaped the range: {}",
                value
            );
        }
    }

    /// At most one node may claim focus, and it must be the one the stack
    /// actually focused. Two focused nodes make a screen reader's caret
    /// ambiguous; the wrong one sends it to a different control than the
    /// keyboard is driving.
    #[test]
    fn focus_is_reported_exactly_once_and_matches_the_stack(
        kinds in prop::collection::vec(kind_strategy(), 1..10),
        horizontal in any::<bool>(),
        target in 0usize..12,
    ) {
        let mut stack = build(&kinds, horizontal, Rect::new(0.0, 0.0, 400.0, 400.0));
        stack.set_focus(Some(target));
        let focused = stack.focused();
        let tree = accessibility_tree(&mut stack, "editor");

        let claimed: Vec<usize> = tree
            .children
            .iter()
            .enumerate()
            .filter(|(_, node)| node.focused)
            .map(|(index, _)| index)
            .collect();
        prop_assert!(claimed.len() <= 1, "several nodes claimed focus: {:?}", claimed);
        // An out-of-range target clears focus, so the tree must show none.
        prop_assert_eq!(claimed.first().copied(), focused);
        // The root is a container: it never holds focus itself.
        prop_assert!(!tree.focused);
    }

    /// Bounds must survive the round trip unchanged. A bridge converts these to
    /// screen coordinates for hit-testing; a rectangle that does not match what
    /// was painted puts the screen reader's cursor on empty space.
    #[test]
    fn bounds_match_the_laid_out_widgets(
        kinds in prop::collection::vec(kind_strategy(), 1..10),
        horizontal in any::<bool>(),
        width in 50.0f32..800.0,
        height in 50.0f32..800.0,
    ) {
        let area = Rect::new(0.0, 0.0, width, height);
        let mut stack = build(&kinds, horizontal, area);
        let expected: Vec<Rect> = (0..stack.len())
            .map(|index| stack.child_bounds(index).unwrap())
            .collect();
        let tree = accessibility_tree(&mut stack, "editor");
        for (node, rect) in tree.children.iter().zip(&expected) {
            prop_assert_eq!(node.bounds.x, rect.x);
            prop_assert_eq!(node.bounds.y, rect.y);
            prop_assert_eq!(node.bounds.width, rect.width);
            prop_assert_eq!(node.bounds.height, rect.height);
        }
        prop_assert_eq!(tree.bounds.width, area.width);
    }
}
