//! A single monophonic voice, wired to the host's note events.
//!
//! This lives in the framework layer rather than in `sunmao_dsp` on purpose:
//! `sunmao_dsp` knows nothing about hosts, events, or plugins, and keeping it
//! that way is what makes it reusable outside SunMao. A voice that reads an
//! [`EventQueue`] is glue between the two, so it belongs here.

use sunmao_core::audio::AudioBuffer;
use sunmao_core::events::{EventQueue, MidiMessage};
use sunmao_dsp::envelopes::Adsr;
use sunmao_dsp::oscillators::{Oscillator, Waveform};

/// One oscillator and one envelope, driven by note on/off.
///
/// Monophonic with **last-note priority**: a new note takes over immediately,
/// and a note-off is honoured only if it names the note that is currently
/// sounding. That second rule is the one that is easy to skip and audible when
/// missing — releasing a key that was already superseded would otherwise cut
/// off the note the player is holding.
///
/// [`MonoVoice::render`] applies notes at their sample offsets, so timing does
/// not get quantised to the block size. Use [`MonoVoice::play_events`] plus
/// your own rendering only when block-accurate timing is genuinely enough.
///
/// ```
/// # use sunmao::prelude::*;
/// let mut voice = MonoVoice::default();
/// voice.prepare(48_000.0);
///
/// let mut events = EventQueue::new();
/// events.push(Event::Midi(MidiMessage::note_on(0, 0, 69, 100)));
/// voice.play_events(&events);
/// assert!(voice.is_active());
///
/// // A held note produces sound; silence before it would be a dead voice.
/// let sounded = (0..64).map(|_| voice.next().abs()).fold(0.0f32, f32::max);
/// assert!(sounded > 0.0);
/// ```
#[derive(Debug, Clone)]
pub struct MonoVoice {
    oscillator: Oscillator,
    envelope: Adsr,
    sample_rate: f64,
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
    sounding: Option<u8>,
}

impl Default for MonoVoice {
    fn default() -> Self {
        Self::new(Waveform::Sine)
    }
}

impl MonoVoice {
    /// A voice with the given waveform and a short, neutral envelope.
    pub fn new(waveform: Waveform) -> Self {
        Self {
            oscillator: Oscillator::new(waveform),
            envelope: Adsr::new(),
            sample_rate: 48_000.0,
            attack: 0.005,
            decay: 0.100,
            sustain: 0.7,
            release: 0.200,
            sounding: None,
        }
    }

    /// Adopts the host's sample rate. Call from `initialize`.
    pub fn prepare(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.apply_envelope();
        if let Some(note) = self.sounding {
            self.oscillator
                .set_frequency(note_frequency_hz(note), sample_rate);
        }
    }

    /// Sets the ADSR times in seconds and the sustain level in `0.0..=1.0`.
    pub fn set_envelope(&mut self, attack: f32, decay: f32, sustain: f32, release: f32) {
        self.attack = attack;
        self.decay = decay;
        self.sustain = sustain;
        self.release = release;
        self.apply_envelope();
    }

    /// Switches the oscillator's waveform, keeping phase and envelope.
    pub fn set_waveform(&mut self, waveform: Waveform) {
        self.oscillator.set_waveform(waveform);
    }

    /// Starts a note at an explicit pitch, for callers not driven by MIDI.
    pub fn note_on_hz(&mut self, frequency_hz: f64) {
        self.sounding = None;
        self.oscillator
            .set_frequency(frequency_hz, self.sample_rate);
        self.envelope.gate_on();
    }

    /// Releases whatever is sounding.
    pub fn note_off(&mut self) {
        self.sounding = None;
        self.envelope.gate_off();
    }

    /// Applies every note on/off in `events` to this voice, ignoring their
    /// sample offsets.
    ///
    /// Non-note events are ignored, so passing the whole queue is safe. Prefer
    /// [`MonoVoice::render`], which honours the offsets.
    pub fn play_events(&mut self, events: &EventQueue) {
        for message in events.midi_events() {
            self.apply(message);
        }
    }

    /// Renders a whole block, applying each note at its own sample offset.
    ///
    /// A note-on the host placed 17 samples into the block starts sounding on
    /// sample 17, not on sample 0. That difference is the whole reason hosts
    /// send offsets: quantising notes to block boundaries adds up to a block
    /// of jitter — over 10 ms at 512 samples and 48 kHz — which is audible as
    /// sloppy timing on anything percussive.
    ///
    /// Events are applied in queue order and the write cursor never moves
    /// backwards, so a host that delivers events out of order gets the later
    /// one applied at the current position rather than a rewind (which would
    /// mean overwriting audio already rendered). Sorting would need an
    /// allocation, and this runs on the audio thread.
    pub fn render(&mut self, buffer: &mut AudioBuffer, events: &EventQueue, gain: f32) {
        let samples = buffer.num_samples();
        let mut cursor = 0usize;
        for message in events.midi_events() {
            let offset = (message.offset as usize).clamp(cursor, samples);
            if offset > cursor {
                let (oscillator, envelope) = (&mut self.oscillator, &mut self.envelope);
                buffer.fill_mono_range(cursor..offset, || {
                    oscillator.next() * envelope.next() * gain
                });
                cursor = offset;
            }
            self.apply(message);
        }
        let (oscillator, envelope) = (&mut self.oscillator, &mut self.envelope);
        buffer.fill_mono_range(cursor..samples, || {
            oscillator.next() * envelope.next() * gain
        });
    }

    fn apply(&mut self, message: &MidiMessage) {
        if message.is_note_on() {
            self.oscillator
                .set_frequency(message.frequency_hz(), self.sample_rate);
            self.envelope.gate_on();
            self.sounding = Some(message.note());
        } else if message.is_note_off() {
            // Release when the note-off names what is sounding, and also when
            // nothing is: that second case is a voice started through
            // `note_on_hz`, which has no note number to name. Ignoring the
            // note-off there would leave a voice that no host event can stop,
            // which is worse than releasing one sample early.
            match self.sounding {
                Some(note) if note != message.note() => {}
                _ => {
                    self.sounding = None;
                    self.envelope.gate_off();
                }
            }
        }
    }

    /// Produces the next sample: oscillator times envelope.
    #[inline]
    pub fn next(&mut self) -> f32 {
        self.oscillator.next() * self.envelope.next()
    }

    /// Whether the envelope is still producing sound, including its release.
    pub fn is_active(&self) -> bool {
        self.envelope.is_active()
    }

    /// Silences the voice and clears its note, without a release tail.
    pub fn reset(&mut self) {
        self.oscillator.reset();
        self.envelope.reset();
        self.sounding = None;
    }

    fn apply_envelope(&mut self) {
        self.envelope.set_params(
            self.attack,
            self.decay,
            self.sustain,
            self.release,
            self.sample_rate,
        );
    }
}

/// 12-tone equal temperament with A4 = 440 Hz, for a bare note number.
fn note_frequency_hz(note: u8) -> f64 {
    440.0 * 2.0_f64.powf((f64::from(note) - 69.0) / 12.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use sunmao_core::events::{Event, MidiMessage};

    /// Counts allocator traffic on this thread while a scope is armed, so the
    /// audio-path claim can be measured rather than asserted in a comment.
    /// Same shape as the backends' allocation matrix.
    struct TestAllocator;

    thread_local! {
        static ALLOCATOR_CALL_COUNT: Cell<isize> = const { Cell::new(-1) };
    }

    unsafe impl GlobalAlloc for TestAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            record_allocator_call();
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            record_allocator_call();
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            record_allocator_call();
            unsafe { System.alloc_zeroed(layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            record_allocator_call();
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    fn record_allocator_call() {
        let _ = ALLOCATOR_CALL_COUNT.try_with(|count| {
            let current = count.get();
            if current >= 0 {
                count.set(current + 1);
            }
        });
    }

    #[global_allocator]
    static TEST_ALLOCATOR: TestAllocator = TestAllocator;

    struct AllocationScope;

    impl Drop for AllocationScope {
        fn drop(&mut self) {
            ALLOCATOR_CALL_COUNT.with(|count| count.set(-1));
        }
    }

    fn count_allocator_calls<R>(callback: impl FnOnce() -> R) -> (R, usize) {
        ALLOCATOR_CALL_COUNT.with(|count| {
            assert_eq!(count.get(), -1);
            count.set(0);
        });
        let scope = AllocationScope;
        let result = callback();
        let calls = ALLOCATOR_CALL_COUNT.with(|count| count.get() as usize);
        drop(scope);
        (result, calls)
    }

    fn queue(messages: &[MidiMessage]) -> EventQueue {
        let mut events = EventQueue::new();
        for message in messages {
            events.push(Event::Midi(*message));
        }
        events
    }

    fn peak(voice: &mut MonoVoice, samples: usize) -> f32 {
        (0..samples)
            .map(|_| voice.next().abs())
            .fold(0.0f32, f32::max)
    }

    #[test]
    fn a_note_on_starts_sound_and_a_matching_note_off_releases_it() {
        let mut voice = MonoVoice::default();
        voice.prepare(48_000.0);
        voice.play_events(&queue(&[MidiMessage::note_on(0, 0, 60, 100)]));
        assert!(voice.is_active());
        assert!(peak(&mut voice, 480) > 0.0);

        voice.play_events(&queue(&[MidiMessage::note_off(0, 0, 60, 0)]));
        // The release is 200 ms; well past it the voice is silent and idle.
        let tail = peak(&mut voice, 48_000);
        assert!(!voice.is_active(), "still active after its release");
        assert!(tail > 0.0, "the release should have been audible");
        assert_eq!(peak(&mut voice, 480), 0.0, "kept sounding after release");
    }

    #[test]
    fn a_note_off_for_a_superseded_note_is_ignored() {
        // Play 60, then 64 while 60 is still held, then release 60. A player
        // hears the note they are still holding; cutting it would be a bug.
        let mut voice = MonoVoice::default();
        voice.prepare(48_000.0);
        voice.play_events(&queue(&[
            MidiMessage::note_on(0, 0, 60, 100),
            MidiMessage::note_on(0, 0, 64, 100),
            MidiMessage::note_off(0, 0, 60, 0),
        ]));
        assert!(voice.is_active());
        assert!(peak(&mut voice, 48_000) > 0.0);
        assert!(
            voice.is_active(),
            "the held note was cut by a stale note-off"
        );
    }

    #[test]
    fn a_note_on_with_zero_velocity_releases_like_a_note_off() {
        // Running-status keyboards send note-on/velocity 0 instead of note-off.
        let mut voice = MonoVoice::default();
        voice.prepare(48_000.0);
        voice.play_events(&queue(&[MidiMessage::note_on(0, 0, 60, 100)]));
        voice.play_events(&queue(&[MidiMessage::note_on(0, 0, 60, 0)]));
        assert_eq!(peak(&mut voice, 96_000), 0.0);
        assert!(!voice.is_active());
    }

    #[test]
    fn preparing_after_a_note_keeps_its_pitch() {
        // A host may change sample rate while a note is held. The pitch has to
        // follow the new rate, not stay at the old rate's phase increment.
        let mut voice = MonoVoice::default();
        voice.prepare(48_000.0);
        voice.play_events(&queue(&[MidiMessage::note_on(0, 0, 69, 100)]));
        let mut reference = voice.clone();
        voice.prepare(96_000.0);

        // Count zero crossings over one second at each rate: same pitch means
        // the same count, give or take the boundary.
        let crossings = |voice: &mut MonoVoice, samples: usize| {
            let mut previous = voice.next();
            let mut count = 0i32;
            for _ in 1..samples {
                let current = voice.next();
                if previous <= 0.0 && current > 0.0 {
                    count += 1;
                }
                previous = current;
            }
            count
        };
        let at_48k = crossings(&mut reference, 48_000);
        let at_96k = crossings(&mut voice, 96_000);
        assert!(
            (at_48k - at_96k).abs() <= 1,
            "pitch changed with the sample rate: {at_48k} vs {at_96k} cycles"
        );
    }

    #[test]
    fn reset_silences_the_voice_immediately() {
        let mut voice = MonoVoice::default();
        voice.prepare(48_000.0);
        voice.play_events(&queue(&[MidiMessage::note_on(0, 0, 60, 100)]));
        peak(&mut voice, 4_800);
        voice.reset();
        assert_eq!(peak(&mut voice, 480), 0.0);
        assert!(!voice.is_active());
    }

    #[test]
    fn a_manually_started_note_can_still_be_stopped_by_the_host() {
        // `note_on_hz` has no note number, so a note-off cannot name it. If
        // that meant "ignore", a voice started this way would sound forever.
        let mut voice = MonoVoice::default();
        voice.prepare(48_000.0);
        voice.note_on_hz(440.0);
        assert!(peak(&mut voice, 480) > 0.0);

        voice.play_events(&queue(&[MidiMessage::note_off(0, 0, 60, 0)]));
        peak(&mut voice, 48_000);
        assert!(!voice.is_active(), "a manual note could not be stopped");
    }

    #[test]
    fn render_starts_a_note_at_its_sample_offset() {
        // This is the host-visible contract the unit-test runner asserts on
        // every synth: silence before the note's offset, sound from it on.
        let mut voice = MonoVoice::default();
        voice.prepare(48_000.0);
        let mut left = [0.0f32; 128];
        let mut right = [0.0f32; 128];
        let mut outputs: Vec<&mut [f32]> = vec![&mut left, &mut right];
        let mut buffer = AudioBuffer::new(&[], &mut outputs, 128);

        voice.render(
            &mut buffer,
            &queue(&[MidiMessage::note_on(17, 0, 60, 100)]),
            1.0,
        );
        drop(buffer);

        let before = left[..17].iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
        let after = left[17..].iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
        assert_eq!(before, 0.0, "sound leaked in before the note's offset");
        assert!(after > 0.0, "the note did not start at its offset");
        assert_eq!(left, right, "channels diverged");
    }

    #[test]
    fn render_applies_an_out_of_order_event_without_rewinding() {
        // A misbehaving host can send a note-off with a lower offset than the
        // note-on before it. Rendering must not walk backwards over samples it
        // already wrote.
        let mut voice = MonoVoice::default();
        voice.prepare(48_000.0);
        let mut channel = [f32::NAN; 64];
        let mut outputs: Vec<&mut [f32]> = vec![&mut channel];
        let mut buffer = AudioBuffer::new(&[], &mut outputs, 64);

        voice.render(
            &mut buffer,
            &queue(&[
                MidiMessage::note_on(40, 0, 60, 100),
                MidiMessage::note_off(5, 0, 60, 0),
                MidiMessage::note_on(999, 0, 62, 100),
            ]),
            1.0,
        );
        drop(buffer);

        assert!(
            channel.iter().all(|sample| sample.is_finite()),
            "some samples were never written: {channel:?}"
        );
    }

    #[test]
    fn rendering_a_block_does_not_allocate() {
        // `render` runs in the audio callback, where an allocation can block
        // on a lock inside the allocator and produce a dropout. Nothing in the
        // path should need the heap: the voice is fixed size and writes into
        // the host's buffer.
        let mut voice = MonoVoice::default();
        voice.prepare(48_000.0);
        let events = queue(&[
            MidiMessage::note_on(17, 0, 60, 100),
            MidiMessage::note_off(200, 0, 60, 0),
            MidiMessage::note_on(240, 0, 67, 90),
        ]);
        let mut left = [0.0f32; 256];
        let mut right = [0.0f32; 256];
        let mut outputs: Vec<&mut [f32]> = vec![&mut left, &mut right];
        let mut buffer = AudioBuffer::new(&[], &mut outputs, 256);

        let (_, calls) = count_allocator_calls(|| {
            voice.render(&mut buffer, &events, 0.5);
            voice.play_events(&events);
            for _ in 0..256 {
                voice.next();
            }
        });
        assert_eq!(calls, 0, "the audio path allocated {calls} time(s)");
    }

    #[test]
    fn a_non_note_event_is_ignored() {
        let mut voice = MonoVoice::default();
        voice.prepare(48_000.0);
        let mut events = EventQueue::new();
        events.push(Event::ParamMod {
            id: "gain",
            amount: 0.5,
            offset: 0,
        });
        voice.play_events(&events);
        assert!(!voice.is_active());
    }
}
