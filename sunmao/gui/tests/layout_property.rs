//! Property tests for the declarative stack layout.
//!
//! The unit tests in `stack.rs` pin specific arrangements. These pin the
//! *invariants* that must hold for any arrangement — the ones a future
//! flex/grow implementation could quietly break.

use proptest::prelude::*;
use sunmao_gui::{Axis, Column, Padding, Rect, Row, Stack, Toggle, Widget};

/// Build a stack of `sizes` children with the given spacing, laid out in
/// `area`.
fn build(axis: Axis, sizes: &[(f32, f32)], gap: f32, padding: f32, area: Rect) -> Stack {
    let mut stack = match axis {
        Axis::Vertical => Column::new(),
        Axis::Horizontal => Row::new(),
    }
    .gap(gap)
    .padding(padding);
    for (width, height) in sizes {
        stack = stack.child(Toggle::new("p").with_bounds(Rect::new(0.0, 0.0, *width, *height)));
    }
    stack.layout(area);
    stack
}

fn axis_strategy() -> impl Strategy<Value = Axis> {
    prop_oneof![Just(Axis::Vertical), Just(Axis::Horizontal)]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Siblings must never overlap along the main axis. Overlapping widgets
    /// make hit-testing ambiguous: two controls would claim the same pixel and
    /// which one responds would depend on iteration order.
    #[test]
    fn siblings_never_overlap_along_the_main_axis(
        axis in axis_strategy(),
        sizes in prop::collection::vec((1.0f32..200.0, 1.0f32..200.0), 1..8),
        gap in 0.0f32..40.0,
        padding in 0.0f32..30.0,
        width in 1.0f32..800.0,
        height in 1.0f32..800.0,
    ) {
        let stack = build(axis, &sizes, gap, padding, Rect::new(0.0, 0.0, width, height));
        let rects: Vec<Rect> = (0..stack.len())
            .map(|i| stack.child_bounds(i).unwrap())
            .collect();
        for pair in rects.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            match axis {
                Axis::Vertical => prop_assert!(
                    b.y >= a.y + a.height - f32::EPSILON,
                    "vertical overlap: {:?} then {:?}", a, b
                ),
                Axis::Horizontal => prop_assert!(
                    b.x >= a.x + a.width - f32::EPSILON,
                    "horizontal overlap: {:?} then {:?}", a, b
                ),
            }
        }
    }

    /// No child may be given a negative size, however small the container or
    /// however large the padding. A negative width reaches the renderer as a
    /// degenerate rectangle and, on some backends, as a wrapped huge unsigned
    /// value.
    #[test]
    fn no_child_ever_receives_a_negative_size(
        axis in axis_strategy(),
        sizes in prop::collection::vec((1.0f32..200.0, 1.0f32..200.0), 1..8),
        gap in 0.0f32..40.0,
        padding in 0.0f32..300.0,
        width in 0.0f32..200.0,
        height in 0.0f32..200.0,
    ) {
        let stack = build(axis, &sizes, gap, padding, Rect::new(0.0, 0.0, width, height));
        for index in 0..stack.len() {
            let rect = stack.child_bounds(index).unwrap();
            prop_assert!(rect.width >= 0.0, "child {index} width {}", rect.width);
            prop_assert!(rect.height >= 0.0, "child {index} height {}", rect.height);
        }
    }

    /// Children start at the padded origin and keep their declared main-axis
    /// size; only the cross axis is allowed to stretch.
    #[test]
    fn the_main_axis_size_is_preserved_and_the_cross_axis_fills(
        axis in axis_strategy(),
        sizes in prop::collection::vec((1.0f32..200.0, 1.0f32..200.0), 1..8),
        gap in 0.0f32..40.0,
        padding in 0.0f32..30.0,
        origin_x in -100.0f32..100.0,
        origin_y in -100.0f32..100.0,
        width in 100.0f32..800.0,
        height in 100.0f32..800.0,
    ) {
        let area = Rect::new(origin_x, origin_y, width, height);
        let stack = build(axis, &sizes, gap, padding, area);
        let expected_cross = match axis {
            Axis::Vertical => (width - padding * 2.0).max(0.0),
            Axis::Horizontal => (height - padding * 2.0).max(0.0),
        };
        for (index, (declared_w, declared_h)) in sizes.iter().enumerate() {
            let rect = stack.child_bounds(index).unwrap();
            match axis {
                Axis::Vertical => {
                    prop_assert!((rect.height - declared_h).abs() < 1.0e-3);
                    prop_assert!((rect.width - expected_cross).abs() < 1.0e-3);
                    prop_assert!((rect.x - (origin_x + padding)).abs() < 1.0e-3);
                }
                Axis::Horizontal => {
                    prop_assert!((rect.width - declared_w).abs() < 1.0e-3);
                    prop_assert!((rect.height - expected_cross).abs() < 1.0e-3);
                    prop_assert!((rect.y - (origin_y + padding)).abs() < 1.0e-3);
                }
            }
        }
        // The first child sits exactly at the padded origin.
        let first = stack.child_bounds(0).unwrap();
        match axis {
            Axis::Vertical => prop_assert!((first.y - (origin_y + padding)).abs() < 1.0e-3),
            Axis::Horizontal => prop_assert!((first.x - (origin_x + padding)).abs() < 1.0e-3),
        }
    }

    /// `content_extent` is what an editor asks the host to resize to, so it has
    /// to agree with where the children actually landed — not merely look
    /// plausible.
    #[test]
    fn content_extent_matches_the_placed_children(
        axis in axis_strategy(),
        sizes in prop::collection::vec((1.0f32..200.0, 1.0f32..200.0), 1..8),
        gap in 0.0f32..40.0,
        padding in 0.0f32..30.0,
    ) {
        let area = Rect::new(0.0, 0.0, 1000.0, 1000.0);
        let stack = build(axis, &sizes, gap, padding, area);
        let last = stack.child_bounds(stack.len() - 1).unwrap();
        let placed_end = match axis {
            Axis::Vertical => last.y + last.height + padding,
            Axis::Horizontal => last.x + last.width + padding,
        };
        prop_assert!(
            (stack.content_extent() - placed_end).abs() < 1.0e-2,
            "content_extent {} but children end at {placed_end}",
            stack.content_extent()
        );
    }

    /// Laying the same stack out twice must not drift. `layout` reads each
    /// child's current bounds to get its main-axis size, so a rule that also
    /// wrote that axis would compound on every relayout — exactly the bug the
    /// DPI-scale base size avoids in `view_baseview`.
    #[test]
    fn relayout_is_idempotent(
        axis in axis_strategy(),
        sizes in prop::collection::vec((1.0f32..200.0, 1.0f32..200.0), 1..8),
        gap in 0.0f32..40.0,
        padding in 0.0f32..30.0,
        width in 100.0f32..800.0,
        height in 100.0f32..800.0,
    ) {
        let area = Rect::new(0.0, 0.0, width, height);
        let mut stack = build(axis, &sizes, gap, padding, area);
        let first: Vec<Rect> = (0..stack.len()).map(|i| stack.child_bounds(i).unwrap()).collect();
        stack.layout(area);
        let second: Vec<Rect> = (0..stack.len()).map(|i| stack.child_bounds(i).unwrap()).collect();
        for (index, (a, b)) in first.iter().zip(second.iter()).enumerate() {
            prop_assert!(
                (a.x - b.x).abs() < 1.0e-3
                    && (a.y - b.y).abs() < 1.0e-3
                    && (a.width - b.width).abs() < 1.0e-3
                    && (a.height - b.height).abs() < 1.0e-3,
                "child {index} drifted: {:?} -> {:?}", a, b
            );
        }
    }

    /// Per-edge padding must offset the origin by the leading edges only.
    #[test]
    fn per_edge_padding_offsets_only_the_leading_edges(
        top in 0.0f32..40.0,
        left in 0.0f32..40.0,
        right in 0.0f32..40.0,
        bottom in 0.0f32..40.0,
    ) {
        let mut column = Column::new()
            .gap(0.0)
            .padding_each(Padding { top, right, bottom, left })
            .child(Toggle::new("p").with_bounds(Rect::new(0.0, 0.0, 10.0, 10.0)));
        column.layout(Rect::new(0.0, 0.0, 500.0, 500.0));
        let rect = column.child_bounds(0).unwrap();
        prop_assert!((rect.x - left).abs() < 1.0e-3);
        prop_assert!((rect.y - top).abs() < 1.0e-3);
        prop_assert!((rect.width - (500.0 - left - right)).abs() < 1.0e-3);
    }
}

// ---------------------------------------------------------------------------
// Text layout (Phase 4 M3)
// ---------------------------------------------------------------------------

use sunmao_gui::{Font, TtfFont};

fn ubuntu_font() -> Font {
    Font::new(Box::new(
        TtfFont::from_bytes(epaint_default_fonts::UBUNTU_LIGHT).expect("bundled font parses"),
    ))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Wrapped text must never place a glyph past the limit it was wrapped to.
    /// Overflow here is invisible in a screenshot but paints outside the
    /// control, over whatever is next to it.
    #[test]
    fn wrapped_glyphs_stay_within_the_limit(
        words in prop::collection::vec("[a-z]{1,9}", 1..12),
        size in 8.0f32..28.0,
        limit in 40.0f32..300.0,
    ) {
        let font = ubuntu_font();
        let text = words.join(" ");
        let placed = font.layout(&text, size, Some(limit));
        for glyph in &placed {
            let advance = font.measure(&glyph.ch.to_string(), size).width;
            prop_assert!(
                glyph.x + advance <= limit + 1.0e-2 || glyph.x == 0.0,
                "glyph {:?} at x={} + {} exceeds limit {limit}",
                glyph.ch, glyph.x, advance
            );
        }
    }

    /// Every non-newline character must be placed exactly once, in order.
    /// Wrapping is allowed to move glyphs; it is never allowed to drop or
    /// duplicate them.
    #[test]
    fn layout_places_every_character_exactly_once(
        text in "[a-z ]{0,60}",
        size in 8.0f32..28.0,
        limit in prop::option::of(30.0f32..250.0),
    ) {
        let font = ubuntu_font();
        let placed = font.layout(&text, size, limit);
        let expected: Vec<char> = text.chars().filter(|ch| *ch != '\n').collect();
        let actual: Vec<char> = placed.iter().map(|glyph| glyph.ch).collect();
        prop_assert_eq!(actual, expected);
    }

    /// Lines advance monotonically and each line restarts at or after x=0;
    /// baselines increase with the line index so lines never stack on top of
    /// one another.
    #[test]
    fn lines_advance_monotonically(
        words in prop::collection::vec("[a-z]{1,7}", 1..14),
        size in 8.0f32..24.0,
        limit in 30.0f32..160.0,
    ) {
        let font = ubuntu_font();
        let placed = font.layout(&words.join(" "), size, Some(limit));
        for pair in placed.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            prop_assert!(b.line >= a.line, "line index went backwards");
            if b.line == a.line {
                prop_assert!(b.x >= a.x, "x went backwards within a line");
            } else {
                prop_assert!(b.baseline > a.baseline, "a new line did not move down");
                prop_assert!(b.x >= 0.0);
            }
        }
    }

    /// Measuring a string equals the sum of measuring its characters. Callers
    /// centre labels with this, so a discrepancy shows up as text that drifts
    /// off-centre as it gets longer.
    #[test]
    fn measuring_is_additive_over_characters(
        text in "[a-zA-Z0-9 ]{0,40}",
        size in 6.0f32..40.0,
    ) {
        let font = ubuntu_font();
        let whole = font.measure(&text, size).width;
        let summed: f32 = text
            .chars()
            .map(|ch| font.measure(&ch.to_string(), size).width)
            .sum();
        prop_assert!(
            (whole - summed).abs() < 1.0e-2,
            "whole {whole} vs summed {summed}"
        );
    }
}
