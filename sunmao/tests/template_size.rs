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
    // M3's oscillator and envelope brought this from 86 to 81 lines, but the
    // remaining bulk is not DSP: it is the trait ceremony (params `Default`,
    // `input_channels`/`accepts_midi`/`params`/`initialize`, the `process`
    // signature) plus MIDI handling and the per-sample write loop. Reaching 50
    // needs a higher-level voice abstraction, which is a design decision beyond
    // the DSP crate. So the budget is still not asserted here; the size is
    // pinned so it cannot creep, and the gap stays visible.
    let lines = INSTRUMENT.lines().count();
    assert!(
        lines <= 85,
        "instrument template grew to {lines} lines; shrink it or raise the pin deliberately"
    );
    assert!(
        lines > BUDGET,
        "instrument template is now {lines} lines, at or under the {BUDGET} budget — \
         switch this test to assert the budget and update docs/phase3/status.md"
    );
}
