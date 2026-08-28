//! Property tests for the Phase 2 contract invariants.
//!
//! These complement the example-based unit tests: they assert what must hold
//! for *every* host input, including the malformed values a misbehaving host
//! can produce.

use proptest::prelude::*;
use sunmao_core::audio::AudioBuffer;
use sunmao_core::events::{Event, EventQueue, MidiMessage, NoteExpression, NoteExpressionKind};
use sunmao_core::plugin::{BusInfo, TailLength};

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
}
