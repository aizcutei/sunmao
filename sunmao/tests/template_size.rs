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
fn the_instrument_template_fits_the_boilerplate_budget() {
    // This one took three tries to get under the budget, and only the third
    // was a real fix. M3's oscillator and envelope took it from 86 to 81
    // lines, which was not close, because the bulk was never DSP: it was
    // parameter `Default` boilerplate, two methods spelling out "this is an
    // instrument", MIDI note bookkeeping, and a per-sample write loop. Those
    // became `#[param(default = ..., range = ...)]`, `IS_INSTRUMENT`,
    // `MonoVoice::play_events`, and `AudioBuffer::fill_mono` — each useful to
    // any plugin, not just to this file's line count.
    let lines = INSTRUMENT.lines().count();
    assert!(
        lines <= BUDGET,
        "instrument template is {lines} lines, budget is {BUDGET}"
    );
}
