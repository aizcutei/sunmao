//! Property tests for `MonoVoice`.
//!
//! The unit tests in the module cover specific note patterns. These cover the
//! ones a host can produce that nobody thinks to write down: offsets past the
//! end of the block, offsets that go backwards, several notes landing on the
//! same sample, a block with no events at all.

use proptest::prelude::*;
use sunmao::prelude::*;

/// A note event as a host would place it in a block.
fn messages() -> impl Strategy<Value = Vec<MidiMessage>> {
    prop::collection::vec((any::<bool>(), 0u32..600, 0u8..128, 0u8..128), 0..8).prop_map(|raw| {
        raw.into_iter()
            .map(|(is_on, offset, note, velocity)| {
                if is_on {
                    MidiMessage::note_on(offset, 0, note, velocity)
                } else {
                    MidiMessage::note_off(offset, 0, note, velocity)
                }
            })
            .collect()
    })
}

fn queue(messages: &[MidiMessage]) -> EventQueue {
    let mut events = EventQueue::new();
    for message in messages {
        events.push(Event::Midi(*message));
    }
    events
}

proptest! {
    /// Every sample of the block must be written exactly once, whatever the
    /// host sends. The buffer starts as NaN, so a sample the voice skipped is
    /// visible as a NaN left behind — which in a real host is whatever the
    /// previous plugin or an uninitialised buffer left there.
    #[test]
    fn render_writes_every_sample_of_the_block(
        notes in messages(),
        frames in 1usize..300,
        channels in 1usize..3,
        gain in -4.0f32..4.0,
        sample_rate in prop::sample::select(vec![8_000.0f64, 44_100.0, 48_000.0, 192_000.0]),
    ) {
        let mut voice = MonoVoice::default();
        voice.prepare(sample_rate);

        let mut storage = vec![vec![f32::NAN; frames]; channels];
        {
            let mut outputs: Vec<&mut [f32]> = storage
                .iter_mut()
                .map(|channel| channel.as_mut_slice())
                .collect();
            let mut buffer = AudioBuffer::new(&[], &mut outputs, frames);
            voice.render(&mut buffer, &queue(&notes), gain);
        }

        for channel in &storage {
            prop_assert!(
                channel.iter().all(|sample| sample.is_finite()),
                "left an unwritten or non-finite sample: {channel:?}"
            );
        }
        // Every channel gets the same signal: this is a mono voice.
        for channel in &storage[1..] {
            prop_assert_eq!(channel, &storage[0]);
        }
    }

    /// A voice cannot be louder than the gain it was given. The oscillator and
    /// the envelope are each bounded by one, so the product is too — with
    /// headroom for the band-limited waveforms' overshoot at a discontinuity.
    #[test]
    fn a_voice_never_exceeds_the_gain_it_was_given(
        notes in messages(),
        gain in 0.0f32..4.0,
    ) {
        let mut voice = MonoVoice::default();
        voice.prepare(48_000.0);
        let mut storage = vec![0.0f32; 512];
        {
            let mut outputs: Vec<&mut [f32]> = vec![storage.as_mut_slice()];
            let mut buffer = AudioBuffer::new(&[], &mut outputs, 512);
            voice.render(&mut buffer, &queue(&notes), gain);
        }

        let peak = storage.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
        prop_assert!(peak <= gain * 1.5 + 1.0e-6, "peaked at {peak} for gain {gain}");
    }

    /// With no events, splitting the block cannot change anything: rendering
    /// must produce exactly what pulling samples one at a time produces.
    #[test]
    fn an_empty_block_renders_the_same_samples_it_would_produce_one_by_one(
        frames in 1usize..300,
        gain in -2.0f32..2.0,
    ) {
        let mut rendered = MonoVoice::default();
        rendered.prepare(48_000.0);
        rendered.note_on_hz(440.0);
        let mut sampled = rendered.clone();

        let mut storage = vec![0.0f32; frames];
        {
            let mut outputs: Vec<&mut [f32]> = vec![storage.as_mut_slice()];
            let mut buffer = AudioBuffer::new(&[], &mut outputs, frames);
            rendered.render(&mut buffer, &EventQueue::new(), gain);
        }

        for (index, value) in storage.iter().enumerate() {
            let expected = sampled.next() * gain;
            prop_assert_eq!(
                value.to_bits(),
                expected.to_bits(),
                "sample {} diverged: {} vs {}",
                index,
                value,
                expected
            );
        }
    }
}
