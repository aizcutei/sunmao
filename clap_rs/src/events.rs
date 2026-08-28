use clap_sys::events::*;

#[derive(Debug, Clone, Copy)]
pub enum Event {
    NoteOn(NoteEvent),
    NoteOff(NoteEvent),
    ParamValue(ParamValueEvent),
    /// A temporary, additive parameter offset that must not be written back
    /// into the plugin's saved state.
    ParamMod(ParamModEvent),
    NoteExpression(NoteExpressionEvent),
    Midi(MidiEvent),
    Unknown,
}

/// Per-note expression dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteExpressionKind {
    Volume,
    Pan,
    Tuning,
    Vibrato,
    Expression,
    Brightness,
    Pressure,
}

impl NoteExpressionKind {
    /// Maps a raw `clap_note_expression`; unknown dimensions stay `None` so
    /// the caller can decide how to report them rather than mistaking one
    /// dimension for another.
    pub fn from_raw(value: clap_note_expression) -> Option<Self> {
        match value {
            CLAP_NOTE_EXPRESSION_VOLUME => Some(Self::Volume),
            CLAP_NOTE_EXPRESSION_PAN => Some(Self::Pan),
            CLAP_NOTE_EXPRESSION_TUNING => Some(Self::Tuning),
            CLAP_NOTE_EXPRESSION_VIBRATO => Some(Self::Vibrato),
            CLAP_NOTE_EXPRESSION_EXPRESSION => Some(Self::Expression),
            CLAP_NOTE_EXPRESSION_BRIGHTNESS => Some(Self::Brightness),
            CLAP_NOTE_EXPRESSION_PRESSURE => Some(Self::Pressure),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NoteExpressionEvent {
    pub time: u32,
    /// `None` for a dimension this binding does not model.
    pub kind: Option<NoteExpressionKind>,
    pub raw_kind: clap_note_expression,
    pub note_id: i32,
    pub port_index: i16,
    pub channel: i16,
    pub key: i16,
    pub value: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct ParamModEvent {
    pub time: u32,
    pub param_id: u32,
    pub cookie: *mut std::ffi::c_void,
    pub note_id: i32,
    pub port_index: i16,
    pub channel: i16,
    pub key: i16,
    /// Additive offset applied on top of the parameter's current value.
    pub amount: f64,
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
            CLAP_EVENT_PARAM_MOD
                if header_ref.size >= std::mem::size_of::<clap_event_param_mod_t>() as u32 =>
            {
                let param = unsafe { &*(header as *const clap_event_param_mod_t) };
                Event::ParamMod(ParamModEvent {
                    time: param.header.time,
                    param_id: param.param_id,
                    cookie: param.cookie,
                    note_id: param.note_id,
                    port_index: param.port_index,
                    channel: param.channel,
                    key: param.key,
                    amount: param.amount,
                })
            }
            CLAP_EVENT_NOTE_EXPRESSION
                if header_ref.size
                    >= std::mem::size_of::<clap_event_note_expression_t>() as u32 =>
            {
                let expression = unsafe { &*(header as *const clap_event_note_expression_t) };
                Event::NoteExpression(NoteExpressionEvent {
                    time: expression.header.time,
                    kind: NoteExpressionKind::from_raw(expression.expression_id),
                    raw_kind: expression.expression_id,
                    note_id: expression.note_id,
                    port_index: expression.port_index,
                    channel: expression.channel,
                    key: expression.key,
                    value: expression.value,
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

    fn expression_header(size: u32) -> clap_event_header_t {
        clap_event_header_t {
            size,
            time: 7,
            space_id: CLAP_CORE_EVENT_SPACE_ID,
            type_: CLAP_EVENT_NOTE_EXPRESSION,
            flags: 0,
        }
    }

    #[test]
    fn note_expression_events_are_decoded_rather_than_dropped() {
        let raw = clap_event_note_expression_t {
            header: expression_header(std::mem::size_of::<clap_event_note_expression_t>() as u32),
            expression_id: CLAP_NOTE_EXPRESSION_PRESSURE,
            note_id: 11,
            port_index: 0,
            channel: 3,
            key: 64,
            value: 0.75,
        };

        let event = unsafe { Event::from_raw(&raw.header) };
        let Event::NoteExpression(expression) = event else {
            panic!("expected a note expression event, got {event:?}");
        };
        assert_eq!(expression.kind, Some(NoteExpressionKind::Pressure));
        assert_eq!(expression.note_id, 11);
        assert_eq!(expression.channel, 3);
        assert_eq!(expression.key, 64);
        assert_eq!(expression.value, 0.75);
        assert_eq!(expression.time, 7);
    }

    #[test]
    fn an_unknown_expression_dimension_keeps_its_raw_id() {
        // The event must still reach the plugin: dropping it would silently
        // lose host automation for a dimension this binding predates.
        let raw = clap_event_note_expression_t {
            header: expression_header(std::mem::size_of::<clap_event_note_expression_t>() as u32),
            expression_id: 4242,
            note_id: 1,
            port_index: 0,
            channel: 0,
            key: 60,
            value: 0.5,
        };

        let Event::NoteExpression(expression) = (unsafe { Event::from_raw(&raw.header) }) else {
            panic!("an unknown dimension must not be dropped");
        };
        assert_eq!(expression.kind, None);
        assert_eq!(expression.raw_kind, 4242);
    }

    #[test]
    fn param_mod_events_are_decoded_rather_than_dropped() {
        let raw = clap_event_param_mod_t {
            header: clap_event_header_t {
                size: std::mem::size_of::<clap_event_param_mod_t>() as u32,
                time: 3,
                space_id: CLAP_CORE_EVENT_SPACE_ID,
                type_: CLAP_EVENT_PARAM_MOD,
                flags: 0,
            },
            param_id: 9,
            cookie: std::ptr::null_mut(),
            note_id: -1,
            port_index: 0,
            channel: -1,
            key: -1,
            amount: -0.25,
        };

        let Event::ParamMod(param) = (unsafe { Event::from_raw(&raw.header) }) else {
            panic!("expected a param mod event");
        };
        assert_eq!(param.param_id, 9);
        assert_eq!(param.amount, -0.25);
        assert_eq!(param.time, 3);
    }

    #[test]
    fn short_expression_and_mod_events_are_rejected() {
        for (type_, required_size) in [
            (
                CLAP_EVENT_NOTE_EXPRESSION,
                std::mem::size_of::<clap_event_note_expression_t>(),
            ),
            (
                CLAP_EVENT_PARAM_MOD,
                std::mem::size_of::<clap_event_param_mod_t>(),
            ),
        ] {
            let header = clap_event_header_t {
                size: required_size as u32 - 1,
                time: 0,
                space_id: CLAP_CORE_EVENT_SPACE_ID,
                type_,
                flags: 0,
            };
            assert!(matches!(
                unsafe { Event::from_raw(&header) },
                Event::Unknown
            ));
        }
    }
}
