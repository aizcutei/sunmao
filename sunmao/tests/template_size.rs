//! Keeps the plugin templates honest about their size.
//!
//! The point of a template is that starting a plugin costs almost no
//! boilerplate. That claim rots silently unless something measures it, so the
//! budget is asserted here rather than stated in a doc.

const EFFECT: &str = include_str!("../../examples/sunmao_template_effect/src/lib.rs");
const INSTRUMENT: &str = include_str!("../../examples/sunmao_template_instrument/src/lib.rs");

/// The Phase 3 M2 target for new-plugin boilerplate.
const BUDGET: usize = 50;

#[test]
fn the_effect_template_fits_the_boilerplate_budget() {
    let lines = EFFECT.lines().count();
    assert!(
        lines <= BUDGET,
        "effect template is {lines} lines, budget is {BUDGET}"
    );
}

#[test]
fn the_instrument_template_does_not_grow_past_its_recorded_size() {
    // A working instrument needs voice handling that an effect does not, and
    // until `sunmao/dsp` ships an oscillator and envelope (Phase 3 M3) that
    // code has to live in the template itself. So this pins the current size
    // instead of asserting the 50-line budget: it cannot silently grow, and the
    // gap is visible. Revisit once M3 lands.
    let lines = INSTRUMENT.lines().count();
    assert!(
        lines <= 90,
        "instrument template grew to {lines} lines; shrink it or raise the pin deliberately"
    );
    assert!(
        lines > BUDGET,
        "instrument template is now {lines} lines, at or under the {BUDGET} budget — \
         switch this test to assert the budget and update docs/phase3/status.md"
    );
}
