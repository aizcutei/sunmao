//! Property tests for the Phase 2 contract invariants.
//!
//! These complement the example-based unit tests: they assert what must hold
//! for *every* host input, including the malformed values a misbehaving host
//! can produce.

use proptest::prelude::*;
use sunmao_core::audio::AudioBuffer;
use sunmao_core::events::{Event, EventQueue, MidiMessage, NoteExpression, NoteExpressionKind};
use sunmao_core::plugin::{BusConfig, BusInfo, TailLength};
use sunmao_core::smoothing::{Smoother, SmoothingStyle};

/// Builds the bus bounds a format adapter hands to `AudioBuffer`.
fn bounds_for(channels: &[u32]) -> Vec<usize> {
    let mut bounds = Vec::with_capacity(channels.len() + 1);
    let mut offset = 0usize;
    bounds.push(offset);
    for count in channels {
        offset += *count as usize;
        bounds.push(offset);
    }
    bounds
}

proptest! {
    /// Reading any bus of any declared layout must never panic, and a bus the
    /// host did not fully connect must read as absent rather than borrowing
    /// another bus's channels.
    #[test]
    fn bus_views_never_panic_or_overlap(
        bus_channels in prop::collection::vec(0u32..4, 0..5),
        connected_channels in 0usize..8,
        probe_bus in 0usize..8,
        probe_channel in 0usize..8,
    ) {
        let bounds = bounds_for(&bus_channels);
        let channel_data = vec![vec![0.0f32; 4]; connected_channels];
        let inputs: Vec<&[f32]> = channel_data.iter().map(Vec::as_slice).collect();
        let mut output = vec![0.0f32; 4];
        let mut outputs: [&mut [f32]; 1] = [&mut output];
        let buffer = AudioBuffer::new(&inputs, &mut outputs, 4)
            .with_input_bus_bounds(&bounds);

        // Must not panic for any bus/channel index.
        let slice = buffer.input_bus(probe_bus, probe_channel);

        match buffer.input_bus_channels(probe_bus) {
            Some(range) => {
                // A reported range always lies inside the connected channels.
                prop_assert!(range.end <= connected_channels);
                prop_assert!(range.start < range.end);
                if probe_channel < range.len() {
                    prop_assert_eq!(slice.len(), 4);
                } else {
                    prop_assert!(slice.is_empty());
                }
            }
            None => prop_assert!(slice.is_empty()),
        }
    }

    /// A modulation must never be visible as a parameter change, otherwise it
    /// would leak into the plugin's saved state.
    #[test]
    fn modulations_never_appear_as_automation(
        amounts in prop::collection::vec(-10.0f32..10.0, 0..8),
        values in prop::collection::vec(0.0f32..1.0, 0..8),
    ) {
        let mut queue = EventQueue::new();
        for amount in &amounts {
            queue.push(Event::ParamMod { id: "gain", amount: *amount, offset: 0 });
        }
        for value in &values {
            queue.push(Event::ParamChange { id: "gain", value: *value, offset: 0 });
        }

        prop_assert_eq!(queue.param_changes().count(), values.len());
        prop_assert_eq!(queue.param_mods().count(), amounts.len());
    }

    /// Every event kind reports its offset without panicking, whatever the
    /// host put in it.
    #[test]
    fn every_event_reports_an_offset(
        offset in any::<u32>(),
        key in any::<u8>(),
        channel in any::<u8>(),
        raw_kind in any::<i32>(),
        value in any::<f64>(),
    ) {
        let events = [
            Event::Midi(MidiMessage::note_on(offset, channel & 0x0F, key & 0x7F, 100)),
            Event::ParamChange { id: "gain", value: 0.5, offset },
            Event::ParamMod { id: "gain", amount: 0.5, offset },
            Event::NoteExpression(NoteExpression {
                offset,
                kind: NoteExpressionKind::Unknown(raw_kind),
                note_id: None,
                channel: Some(channel & 0x0F),
                key: Some(key & 0x7F),
                value,
            }),
        ];

        for event in &events {
            prop_assert_eq!(event.offset(), offset);
        }
    }

    /// A declared bus set always yields a monotonically increasing bound table
    /// whose last entry is the flattened channel count the adapters allocate.
    #[test]
    fn bus_bounds_match_the_flattened_channel_count(
        channels in prop::collection::vec(0u32..8, 1..6),
    ) {
        let buses: Vec<BusInfo> = channels
            .iter()
            .map(|count| BusInfo::main("Bus", *count))
            .collect();
        let bounds = bounds_for(&channels);
        let total: u32 = buses.iter().map(|bus| bus.channels).sum();

        prop_assert_eq!(*bounds.last().unwrap(), total as usize);
        prop_assert!(bounds.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    /// A finite tail is never conflated with an unbounded one.
    #[test]
    fn a_finite_tail_is_never_infinite(samples in any::<u32>()) {
        prop_assert_ne!(TailLength::Samples(samples), TailLength::Infinite);
        prop_assert_ne!(TailLength::Samples(samples), TailLength::None);
    }

    /// The two formats must be able to reach exactly the same set of layouts.
    ///
    /// A CLAP host selects a published config by index; a VST3 host proposes
    /// channel counts and the backend looks up the matching index. Those two
    /// paths must agree: the VST3 lookup may only ever land on a config whose
    /// channel counts are exactly what was proposed, and it must find one
    /// whenever such a config exists. Otherwise a layout would be reachable in
    /// one format but not the other, or VST3 would accept a proposal and then
    /// run a different layout than the host asked for.
    #[test]
    fn a_layout_is_reachable_from_both_formats_or_neither(
        config_shapes in prop::collection::vec(
            (prop::collection::vec(0u32..4, 0..3), prop::collection::vec(0u32..4, 0..3)),
            0..5,
        ),
        proposed_inputs in prop::collection::vec(0u32..4, 0..3),
        proposed_outputs in prop::collection::vec(0u32..4, 0..3),
    ) {
        let configs: Vec<BusConfig> = config_shapes
            .iter()
            .map(|(inputs, outputs)| {
                BusConfig::new(
                    "Layout",
                    inputs.iter().map(|c| BusInfo::main("In", *c)).collect(),
                    outputs.iter().map(|c| BusInfo::main("Out", *c)).collect(),
                )
            })
            .collect();

        // The VST3 backend's rule.
        let found = configs
            .iter()
            .position(|config| config.matches(&proposed_inputs, &proposed_outputs));

        // The ground truth, independent of `matches`.
        let expected = config_shapes
            .iter()
            .position(|(inputs, outputs)| {
                inputs.as_slice() == proposed_inputs.as_slice()
                    && outputs.as_slice() == proposed_outputs.as_slice()
            });
        prop_assert_eq!(found, expected);

        // Whatever it landed on really is the proposed layout, so a CLAP host
        // selecting that same index gets what the VST3 host asked for.
        if let Some(index) = found {
            prop_assert_eq!(configs[index].input_channel_counts(), proposed_inputs.clone());
            prop_assert_eq!(configs[index].output_channel_counts(), proposed_outputs.clone());
        } else {
            prop_assert!(
                !configs
                    .iter()
                    .any(|c| c.matches(&proposed_inputs, &proposed_outputs)),
                "no config may match once the lookup reported none"
            );
        }
    }

    /// A ramp must always arrive, at any magnitude and any duration.
    ///
    /// This is the invariant an epsilon cannot provide: the distance at which
    /// f32 stops making progress scales with the target, so a fixed threshold
    /// leaves large targets smoothing forever. Sweeping magnitudes from
    /// subnormal-ish to audio-rate frequencies is what makes that visible.
    #[test]
    fn a_smoother_always_reaches_its_target(
        start in -20_000.0f32..20_000.0,
        target in -20_000.0f32..20_000.0,
        seconds in 0.0001f32..1.0,
        sample_rate in prop::sample::select(vec![8_000.0f64, 44_100.0, 48_000.0, 192_000.0]),
        exponential in any::<bool>(),
    ) {
        let style = if exponential {
            SmoothingStyle::Exponential(seconds)
        } else {
            SmoothingStyle::Linear(seconds)
        };
        let mut smoother = Smoother::new(style);
        smoother.set_sample_rate(sample_rate);
        smoother.reset(start);
        smoother.set_target(target);

        // An exponential ramp is bounded at 12 time constants and the longest
        // configured time here is one second, so 15 seconds of samples is more
        // than arrival can legitimately need for either style.
        let limit = (sample_rate * 15.0) as usize;
        let mut arrived = false;
        for _ in 0..limit {
            let value = smoother.next();
            prop_assert!(value.is_finite(), "produced {value}");
            if !smoother.is_smoothing() {
                arrived = true;
                break;
            }
        }
        prop_assert!(
            arrived,
            "never arrived: start={start} target={target} seconds={seconds} exp={exponential} \
             stalled at {} with distance {:e}",
            smoother.current(),
            (target - smoother.current()).abs()
        );
        prop_assert_eq!(smoother.current(), smoother.target());
    }

    /// A ramp never leaves the interval between where it started and its
    /// target, so smoothing a gain cannot momentarily exceed either end.
    #[test]
    fn a_smoother_never_leaves_the_interval_it_travels(
        start in -10.0f32..10.0,
        target in -10.0f32..10.0,
        seconds in 0.001f32..0.05,
        exponential in any::<bool>(),
    ) {
        let style = if exponential {
            SmoothingStyle::Exponential(seconds)
        } else {
            SmoothingStyle::Linear(seconds)
        };
        let mut smoother = Smoother::new(style);
        smoother.set_sample_rate(48_000.0);
        smoother.reset(start);
        smoother.set_target(target);

        let low = start.min(target);
        let high = start.max(target);
        for _ in 0..48_000 {
            let value = smoother.next();
            // A small tolerance for the exact landing arithmetic only.
            prop_assert!(
                value >= low - 1.0e-3 && value <= high + 1.0e-3,
                "{value} escaped [{low}, {high}]"
            );
            if !smoother.is_smoothing() {
                break;
            }
        }
    }
}
