use clap_sys::events::*;

#[derive(Debug, Clone, Copy)]
pub enum Event {
    NoteOn(NoteEvent),
    NoteOff(NoteEvent),
    ParamValue(ParamValueEvent),
    Midi(MidiEvent),
    Unknown,
}

#[derive(Debug, Clone, Copy)]
pub struct NoteEvent {
    pub time: u32,
    pub port_index: i16,
    pub channel: i16,
    pub key: i16,
    pub note_id: i32,
    pub velocity: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct ParamValueEvent {
    pub time: u32,
    pub param_id: u32,
    pub cookie: *mut std::ffi::c_void,
    pub note_id: i32,
    pub port_index: i16,
    pub channel: i16,
    pub key: i16,
    pub value: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct MidiEvent {
    pub time: u32,
    pub port_index: u16,
    pub data: [u8; 3],
}

impl Event {
    pub unsafe fn from_raw(header: *const clap_event_header_t) -> Self {
        let header_ref = unsafe { &*header };
        if header_ref.space_id != CLAP_CORE_EVENT_SPACE_ID {
            return Event::Unknown;
        }

        match header_ref.type_ {
            CLAP_EVENT_NOTE_ON
                if header_ref.size >= std::mem::size_of::<clap_event_note_t>() as u32 =>
            {
                let note = unsafe { &*(header as *const clap_event_note_t) };
                Event::NoteOn(NoteEvent {
                    time: note.header.time,
                    port_index: note.port_index,
                    channel: note.channel,
                    key: note.key,
                    note_id: note.note_id,
                    velocity: note.velocity,
                })
            }
            CLAP_EVENT_NOTE_OFF
                if header_ref.size >= std::mem::size_of::<clap_event_note_t>() as u32 =>
            {
                let note = unsafe { &*(header as *const clap_event_note_t) };
                Event::NoteOff(NoteEvent {
                    time: note.header.time,
                    port_index: note.port_index,
                    channel: note.channel,
                    key: note.key,
                    note_id: note.note_id,
                    velocity: note.velocity,
                })
            }
            CLAP_EVENT_PARAM_VALUE
                if header_ref.size >= std::mem::size_of::<clap_event_param_value_t>() as u32 =>
            {
                let param = unsafe { &*(header as *const clap_event_param_value_t) };
                Event::ParamValue(ParamValueEvent {
                    time: param.header.time,
                    param_id: param.param_id,
                    cookie: param.cookie,
                    note_id: param.note_id,
                    port_index: param.port_index,
                    channel: param.channel,
                    key: param.key,
                    value: param.value,
                })
            }
            CLAP_EVENT_MIDI
                if header_ref.size >= std::mem::size_of::<clap_event_midi_t>() as u32 =>
            {
                let midi = unsafe { &*(header as *const clap_event_midi_t) };
                Event::Midi(MidiEvent {
                    time: midi.header.time,
                    port_index: midi.port_index,
                    data: [midi.data[0], midi.data[1], midi.data[2]],
                })
            }
            _ => Event::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header<T>(event: &T) -> *const clap_event_header_t {
        event as *const T as *const clap_event_header_t
    }

    #[test]
    fn decoded_note_and_midi_events_preserve_sample_time() {
        let note = clap_event_note_t {
            header: clap_event_header_t {
                size: std::mem::size_of::<clap_event_note_t>() as u32,
                time: 37,
                space_id: CLAP_CORE_EVENT_SPACE_ID,
                type_: CLAP_EVENT_NOTE_ON,
                flags: 0,
            },
            note_id: 1,
            port_index: 0,
            channel: 2,
            key: 64,
            velocity: 0.75,
        };
        let midi = clap_event_midi_t {
            header: clap_event_header_t {
                size: std::mem::size_of::<clap_event_midi_t>() as u32,
                time: 91,
                space_id: CLAP_CORE_EVENT_SPACE_ID,
                type_: CLAP_EVENT_MIDI,
                flags: 0,
            },
            port_index: 0,
            data: [0x90, 60, 100],
        };

        match unsafe { Event::from_raw(header(&note)) } {
            Event::NoteOn(event) => assert_eq!(event.time, 37),
            event => panic!("unexpected event: {event:?}"),
        }
        match unsafe { Event::from_raw(header(&midi)) } {
            Event::Midi(event) => assert_eq!(event.time, 91),
            event => panic!("unexpected event: {event:?}"),
        }
    }

    #[test]
    fn custom_event_space_is_not_decoded_as_a_core_event() {
        let event = clap_event_header_t {
            size: u32::MAX,
            time: 0,
            space_id: CLAP_CORE_EVENT_SPACE_ID + 1,
            type_: CLAP_EVENT_NOTE_ON,
            flags: 0,
        };

        assert!(matches!(unsafe { Event::from_raw(&event) }, Event::Unknown));
    }

    #[test]
    fn undersized_core_events_are_unknown() {
        let cases = [
            (CLAP_EVENT_NOTE_ON, std::mem::size_of::<clap_event_note_t>()),
            (
                CLAP_EVENT_NOTE_OFF,
                std::mem::size_of::<clap_event_note_t>(),
            ),
            (
                CLAP_EVENT_PARAM_VALUE,
                std::mem::size_of::<clap_event_param_value_t>(),
            ),
            (CLAP_EVENT_MIDI, std::mem::size_of::<clap_event_midi_t>()),
        ];

        for (type_, required_size) in cases {
            let event = clap_event_header_t {
                size: required_size as u32 - 1,
                time: 0,
                space_id: CLAP_CORE_EVENT_SPACE_ID,
                type_,
                flags: 0,
            };

            assert!(matches!(unsafe { Event::from_raw(&event) }, Event::Unknown));
        }
    }

    #[test]
    fn unsupported_core_event_type_remains_unknown() {
        let event = clap_event_header_t {
            size: std::mem::size_of::<clap_event_header_t>() as u32,
            time: 0,
            space_id: CLAP_CORE_EVENT_SPACE_ID,
            type_: CLAP_EVENT_NOTE_CHOKE,
            flags: 0,
        };

        assert!(matches!(unsafe { Event::from_raw(&event) }, Event::Unknown));
    }
}
