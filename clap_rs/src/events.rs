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
    pub port_index: i16,
    pub channel: i16,
    pub key: i16,
    pub note_id: i32,
    pub velocity: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct ParamValueEvent {
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
    pub port_index: u16,
    pub data: [u8; 3],
}

impl Event {
    pub unsafe fn from_raw(header: *const clap_event_header_t) -> Self {
        match unsafe { (*header).type_ } {
            CLAP_EVENT_NOTE_ON => {
                let note = unsafe { &*(header as *const clap_event_note_t) };
                Event::NoteOn(NoteEvent {
                    port_index: note.port_index,
                    channel: note.channel,
                    key: note.key,
                    note_id: note.note_id,
                    velocity: note.velocity,
                })
            }
            CLAP_EVENT_NOTE_OFF => {
                let note = unsafe { &*(header as *const clap_event_note_t) };
                Event::NoteOff(NoteEvent {
                    port_index: note.port_index,
                    channel: note.channel,
                    key: note.key,
                    note_id: note.note_id,
                    velocity: note.velocity,
                })
            }
            CLAP_EVENT_PARAM_VALUE => {
                let param = unsafe { &*(header as *const clap_event_param_value_t) };
                Event::ParamValue(ParamValueEvent {
                    param_id: param.param_id,
                    cookie: param.cookie,
                    note_id: param.note_id,
                    port_index: param.port_index,
                    channel: param.channel,
                    key: param.key,
                    value: param.value,
                })
            }
            CLAP_EVENT_MIDI => {
               let midi = unsafe { &*(header as *const clap_event_midi_t) };
               Event::Midi(MidiEvent {
                   port_index: midi.port_index,
                   data: [midi.data[0], midi.data[1], midi.data[2]],
               })
            }
            _ => Event::Unknown,
        }
    }
}
