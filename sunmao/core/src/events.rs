//! Event types and queue.

/// A MIDI message.
#[derive(Debug, Clone, Copy)]
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
}

/// An event that can occur during processing.
#[derive(Debug, Clone)]
pub enum Event {
    /// MIDI message.
    Midi(MidiMessage),
    /// Parameter change from host automation.
    ParamChange { id: String, value: f32, offset: u32 },
}

/// Queue of events for a processing block.
pub struct EventQueue {
    events: Vec<Event>,
}

impl EventQueue {
    /// Create an empty event queue.
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Add an event.
    pub fn push(&mut self, event: Event) {
        self.events.push(event);
    }

    /// Iterate over events.
    pub fn iter(&self) -> impl Iterator<Item = &Event> {
        self.events.iter()
    }

    /// Get MIDI events only.
    pub fn midi_events(&self) -> impl Iterator<Item = &MidiMessage> {
        self.events.iter().filter_map(|e| match e {
            Event::Midi(m) => Some(m),
            _ => None,
        })
    }

    /// Clear all events.
    pub fn clear(&mut self) {
        self.events.clear();
    }
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new()
    }
}
