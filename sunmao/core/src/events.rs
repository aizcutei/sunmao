//! Event types and queue.

/// A MIDI message.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MidiMessage {
    /// Sample offset within the current buffer.
    pub offset: u32,
    /// MIDI data bytes.
    pub data: [u8; 3],
}

impl MidiMessage {
    /// Note On message.
    pub fn note_on(offset: u32, channel: u8, note: u8, velocity: u8) -> Self {
        Self {
            offset,
            data: [0x90 | (channel & 0x0F), note & 0x7F, velocity & 0x7F],
        }
    }

    /// Note Off message.
    pub fn note_off(offset: u32, channel: u8, note: u8, velocity: u8) -> Self {
        Self {
            offset,
            data: [0x80 | (channel & 0x0F), note & 0x7F, velocity & 0x7F],
        }
    }

    /// Check if this is a Note On.
    pub fn is_note_on(&self) -> bool {
        (self.data[0] & 0xF0) == 0x90 && self.data[2] > 0
    }

    /// Check if this is a Note Off.
    pub fn is_note_off(&self) -> bool {
        (self.data[0] & 0xF0) == 0x80 || ((self.data[0] & 0xF0) == 0x90 && self.data[2] == 0)
    }

    /// Get the note number.
    pub fn note(&self) -> u8 {
        self.data[1]
    }

    /// Get velocity.
    pub fn velocity(&self) -> u8 {
        self.data[2]
    }

    /// Get the MIDI channel (0-15).
    pub fn channel(&self) -> u8 {
        self.data[0] & 0x0F
    }
}

/// A normalized host automation change for a declared parameter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamChange {
    /// Static parameter ID from [`crate::params::ParamDescriptor`].
    pub id: &'static str,
    /// Normalized parameter value in the 0.0..=1.0 range.
    pub value: f32,
    /// Sample offset within the current buffer.
    pub offset: u32,
}

/// An event that can occur during processing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    /// MIDI message.
    Midi(MidiMessage),
    /// Parameter change from host automation.
    ParamChange {
        id: &'static str,
        value: f32,
        offset: u32,
    },
    /// A temporary, additive parameter offset.
    ///
    /// Unlike [`Event::ParamChange`] a modulation never becomes part of the
    /// plugin's saved state; it applies on top of the current value until the
    /// host sends another one.
    ParamMod {
        id: &'static str,
        amount: f32,
        offset: u32,
    },
    /// Per-note expression for one sounding voice.
    NoteExpression(NoteExpression),
}

/// Which dimension a [`Event::NoteExpression`] addresses.
///
/// CLAP names these directly; VST3 delivers them through
/// `INoteExpressionController` type ids that the backend maps onto the same
/// set. A dimension neither format models stays `Unknown` with its raw id so
/// no host event is silently dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteExpressionKind {
    Volume,
    Pan,
    Tuning,
    Vibrato,
    Expression,
    Brightness,
    Pressure,
    /// A dimension this framework version does not name, with the format's
    /// own identifier preserved.
    Unknown(i32),
}

/// Per-note expression addressed to one sounding voice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteExpression {
    /// Sample offset within the current block.
    pub offset: u32,
    /// Which dimension changed.
    pub kind: NoteExpressionKind,
    /// Host-assigned note identity, or `None` when the host matched by
    /// channel and key instead.
    pub note_id: Option<i32>,
    /// MIDI channel the note sounds on, when the format reports it.
    ///
    /// VST3 identifies the target note by `note_id` alone, so this is `None`
    /// there; CLAP reports channel and key alongside the note id.
    pub channel: Option<u8>,
    /// MIDI key the note sounds on, when the format reports it.
    pub key: Option<u8>,
    /// Dimension value; the range depends on `kind`.
    pub value: f64,
}

impl Event {
    /// Sample offset within the current processing block.
    pub fn offset(&self) -> u32 {
        match self {
            Self::Midi(event) => event.offset,
            Self::ParamChange { offset, .. } => *offset,
            Self::ParamMod { offset, .. } => *offset,
            Self::NoteExpression(expression) => expression.offset,
        }
    }

    /// Return the parameter change carried by this event, if any.
    pub fn as_param_change(&self) -> Option<ParamChange> {
        match self {
            Self::ParamChange { id, value, offset } => Some(ParamChange {
                id,
                value: *value,
                offset: *offset,
            }),
            // A modulation is deliberately not a parameter change: reporting
            // it as one would let it leak into saved state.
            Self::ParamMod { .. } | Self::Midi(_) | Self::NoteExpression(_) => None,
        }
    }
}

/// Queue of events for a processing block.
pub struct EventQueue {
    events: Vec<Event>,
    max_events: usize,
}

impl EventQueue {
    /// Create an empty event queue.
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            max_events: usize::MAX,
        }
    }

    /// Create an empty event queue with a fixed, activation-owned capacity.
    ///
    /// Once full, [`Self::push`] and [`Self::push_param_change`] return `false`
    /// without allocating or modifying the queue.
    pub fn with_capacity(capacity: usize) -> Self {
        Self::try_with_capacity(capacity)
            .expect("event queue capacity is too large for the available address space")
    }

    /// Fallibly create an empty event queue with a fixed, activation-owned
    /// capacity.
    ///
    /// Format adapters call this while they still own the plugin instance so
    /// an invalid or unreasonably large
    /// [`crate::plugin::SunmaoPlugin::MAX_EVENTS_PER_BLOCK`]
    /// cannot panic across a host ABI boundary. The returned queue never grows
    /// during processing; callers should propagate the error and leave the
    /// plugin inactive when allocation fails.
    pub fn try_with_capacity(capacity: usize) -> Result<Self, std::collections::TryReserveError> {
        let mut events = Vec::new();
        events.try_reserve_exact(capacity)?;
        Ok(Self {
            events,
            max_events: capacity,
        })
    }

    /// Reserve event scratch outside the processing callback.
    pub fn reserve(&mut self, additional: usize) {
        self.events.reserve(additional);
        self.max_events = self.max_events.saturating_add(additional);
    }

    /// Add an event, returning `false` when a fixed-capacity queue is full.
    pub fn push(&mut self, event: Event) -> bool {
        if self.events.len() >= self.max_events {
            return false;
        }
        self.events.push(event);
        true
    }

    /// Add a parameter change, returning `false` when the queue is full.
    pub fn push_param_change(&mut self, change: ParamChange) -> bool {
        if self.events.len() >= self.max_events {
            return false;
        }
        self.events.push(Event::ParamChange {
            id: change.id,
            value: change.value,
            offset: change.offset,
        });
        true
    }

    /// Iterate over all events in their original insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &Event> {
        self.events.iter()
    }

    /// Iterate over all events by value.
    pub fn timed_events(&self) -> impl Iterator<Item = Event> + '_ {
        self.events.iter().copied()
    }

    /// Get MIDI events only.
    pub fn midi_events(&self) -> impl Iterator<Item = &MidiMessage> {
        self.events.iter().filter_map(|event| match event {
            Event::Midi(message) => Some(message),
            _ => None,
        })
    }

    /// Get parameter automation events in their original host order.
    pub fn param_changes(&self) -> impl Iterator<Item = ParamChange> + '_ {
        self.events.iter().filter_map(Event::as_param_change)
    }

    /// Get parameter modulations in their original host order.
    ///
    /// These are separate from [`EventQueue::param_changes`] because a
    /// modulation must not be written back into the plugin's state.
    pub fn param_mods(&self) -> impl Iterator<Item = (&'static str, f32, u32)> + '_ {
        self.events.iter().filter_map(|event| match event {
            Event::ParamMod { id, amount, offset } => Some((*id, *amount, *offset)),
            _ => None,
        })
    }

    /// Get per-note expression events in their original host order.
    pub fn note_expressions(&self) -> impl Iterator<Item = &NoteExpression> {
        self.events.iter().filter_map(|event| match event {
            Event::NoteExpression(expression) => Some(expression),
            _ => None,
        })
    }

    /// Clear all events.
    pub fn clear(&mut self) {
        self.events.clear();
    }

    #[cfg(test)]
    fn capacity(&self) -> usize {
        self.events.capacity()
    }

    /// Maximum number of events accepted without changing the queue contract.
    pub fn max_events(&self) -> usize {
        self.max_events
    }
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_offsets_and_parameter_iteration_preserve_order() {
        let mut events = EventQueue::with_capacity(3);
        events.push(Event::Midi(MidiMessage::note_on(2, 0, 60, 100)));
        events.push_param_change(ParamChange {
            id: "gain",
            value: 0.25,
            offset: 5,
        });
        events.push_param_change(ParamChange {
            id: "gain",
            value: 0.75,
            offset: 5,
        });

        assert_eq!(
            events.iter().map(Event::offset).collect::<Vec<_>>(),
            [2, 5, 5]
        );
        assert_eq!(
            events.param_changes().collect::<Vec<_>>(),
            [
                ParamChange {
                    id: "gain",
                    value: 0.25,
                    offset: 5,
                },
                ParamChange {
                    id: "gain",
                    value: 0.75,
                    offset: 5,
                },
            ]
        );
        let capacity = events.capacity();
        events.clear();
        assert_eq!(events.capacity(), capacity);
    }

    #[test]
    fn fixed_capacity_overflow_does_not_grow_or_modify_the_queue() {
        let mut events = EventQueue::with_capacity(2);
        assert!(events.push(Event::Midi(MidiMessage::note_on(0, 0, 60, 100))));
        assert!(events.push_param_change(ParamChange {
            id: "gain",
            value: 0.5,
            offset: 1,
        }));
        let capacity = events.capacity();

        assert!(!events.push(Event::Midi(MidiMessage::note_off(2, 0, 60, 0))));
        assert!(!events.push_param_change(ParamChange {
            id: "gain",
            value: 0.75,
            offset: 2,
        }));
        assert_eq!(events.iter().count(), 2);
        assert_eq!(events.capacity(), capacity);
        assert_eq!(events.max_events(), 2);
    }

    #[test]
    fn try_with_capacity_rejects_capacity_overflow_without_panicking() {
        let result = EventQueue::try_with_capacity(usize::MAX);
        assert!(result.is_err());
    }
}
