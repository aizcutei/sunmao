//! VST3 Sine Synthesizer Example using vst3_sys
//!
//! A simple sine wave synthesizer plugin.

#![allow(unsafe_op_in_unsafe_fn)]

use std::f64::consts::PI;
use std::ffi::c_void;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use vst3_sys::*;

// =============================================================================
// Plugin UIDs
// =============================================================================

const CID_PROCESSOR: TUID = uid!(0x87654321, 0xAAAABBBB, 0xCCCCDDDD, 0xEEEE1111);
const CID_CONTROLLER: TUID = uid!(0x87654321, 0xAAAABBBB, 0xCCCCDDDD, 0xEEEE2222);

// =============================================================================
// Parameter IDs
// =============================================================================

const PARAM_GAIN: ParamID = 0;

const SHARED_GAIN_CONNECTION_IID: TUID = uid!(0x53554E4D, 0x414F5359, 0x53474149, 0x4E000001);
const CONNECTION_ROLE_PROCESSOR: u32 = 1;
const CONNECTION_ROLE_CONTROLLER: u32 = 2;

const STATE_MAGIC: [u8; 8] = *b"SMV3SYN\0";
const STATE_VERSION: u32 = 1;
const STATE_ENTRY_COUNT: u32 = 1;
const STATE_LEN: usize = 28;

// =============================================================================
// Voice State
// =============================================================================

const MAX_VOICES: usize = 8;
const MAX_NOTE_EVENTS: usize = 256;
const MAX_PARAM_EVENTS: usize = 256;

struct SharedGain {
    state: AtomicU64,
}

impl SharedGain {
    fn new(value: f32) -> Self {
        Self {
            state: AtomicU64::new(Self::pack(value, 0)),
        }
    }

    fn pack(value: f32, generation: u32) -> u64 {
        ((generation as u64) << 32) | value.to_bits() as u64
    }

    fn value(state: u64) -> f32 {
        f32::from_bits(state as u32)
    }

    fn generation(state: u64) -> u32 {
        (state >> 32) as u32
    }

    fn snapshot(&self) -> u64 {
        self.state.load(Ordering::Acquire)
    }

    fn load(&self) -> f32 {
        Self::value(self.snapshot())
    }

    fn store(&self, value: f32) {
        let mut current = self.snapshot();
        loop {
            let next = Self::pack(value, Self::generation(current).wrapping_add(1));
            match self.state.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return,
                Err(actual) => current = actual,
            }
        }
    }

    fn store_if_unchanged(&self, snapshot: u64, value: f32) -> bool {
        let next = Self::pack(value, Self::generation(snapshot).wrapping_add(1));
        self.state
            .compare_exchange(snapshot, next, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

struct Voice {
    active: bool,
    note_id: i32,
    channel: i16,
    pitch: i16,
    phase: f64,
    phase_inc: f64,
    velocity: f32,
}

impl Voice {
    fn new() -> Self {
        Self {
            active: false,
            note_id: -1,
            channel: 0,
            pitch: 0,
            phase: 0.0,
            phase_inc: 0.0,
            velocity: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PendingNoteKind {
    On,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PendingNote {
    sample_offset: usize,
    kind: PendingNoteKind,
    note_id: i32,
    channel: i16,
    pitch: i16,
    velocity: f32,
}

impl PendingNote {
    const EMPTY: Self = Self {
        sample_offset: 0,
        kind: PendingNoteKind::Off,
        note_id: -1,
        channel: 0,
        pitch: 0,
        velocity: 0.0,
    };
}

fn sort_pending_notes(notes: &mut [PendingNote]) {
    for index in 1..notes.len() {
        let note = notes[index];
        let mut insert_at = index;
        while insert_at > 0 && notes[insert_at - 1].sample_offset > note.sample_offset {
            notes[insert_at] = notes[insert_at - 1];
            insert_at -= 1;
        }
        notes[insert_at] = note;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PendingParam {
    sample_offset: usize,
    value: f32,
}

impl PendingParam {
    const EMPTY: Self = Self {
        sample_offset: 0,
        value: 0.0,
    };
}

fn sort_pending_params(params: &mut [PendingParam]) {
    for index in 1..params.len() {
        let param = params[index];
        let mut insert_at = index;
        while insert_at > 0 && params[insert_at - 1].sample_offset > param.sample_offset {
            params[insert_at] = params[insert_at - 1];
            insert_at -= 1;
        }
        params[insert_at] = param;
    }
}

fn encode_gain_state(gain: f64) -> Option<[u8; STATE_LEN]> {
    if !gain.is_finite() || !(0.0..=1.0).contains(&gain) {
        return None;
    }

    let mut bytes = [0u8; STATE_LEN];
    bytes[..8].copy_from_slice(&STATE_MAGIC);
    bytes[8..12].copy_from_slice(&STATE_VERSION.to_le_bytes());
    bytes[12..16].copy_from_slice(&STATE_ENTRY_COUNT.to_le_bytes());
    bytes[16..20].copy_from_slice(&PARAM_GAIN.to_le_bytes());
    bytes[20..28].copy_from_slice(&gain.to_le_bytes());
    Some(bytes)
}

fn decode_gain_state(bytes: &[u8; STATE_LEN]) -> Option<f64> {
    if bytes[..8] != STATE_MAGIC {
        return None;
    }
    if u32::from_le_bytes(bytes[8..12].try_into().ok()?) != STATE_VERSION
        || u32::from_le_bytes(bytes[12..16].try_into().ok()?) != STATE_ENTRY_COUNT
        || ParamID::from_le_bytes(bytes[16..20].try_into().ok()?) != PARAM_GAIN
    {
        return None;
    }

    let gain = f64::from_le_bytes(bytes[20..28].try_into().ok()?);
    (gain.is_finite() && (0.0..=1.0).contains(&gain)).then_some(gain)
}

unsafe fn stream_write_all(stream: *mut c_void, mut bytes: &[u8]) -> bool {
    if stream.is_null() {
        return false;
    }
    let vtbl = *(stream as *const *const IBStreamVtbl);
    if vtbl.is_null() {
        return false;
    }

    while !bytes.is_empty() {
        let requested = bytes.len().min(int32::MAX as usize) as int32;
        let mut written = 0;
        let result = ((*vtbl).write)(
            stream,
            bytes.as_ptr() as *mut c_void,
            requested,
            &mut written,
        );
        if result != kResultOk || written <= 0 || written > requested {
            return false;
        }
        bytes = &bytes[written as usize..];
    }
    true
}

unsafe fn stream_read_exact(stream: *mut c_void, mut bytes: &mut [u8]) -> bool {
    if stream.is_null() {
        return false;
    }
    let vtbl = *(stream as *const *const IBStreamVtbl);
    if vtbl.is_null() {
        return false;
    }

    while !bytes.is_empty() {
        let requested = bytes.len().min(int32::MAX as usize) as int32;
        let mut read = 0;
        let result = ((*vtbl).read)(
            stream,
            bytes.as_mut_ptr() as *mut c_void,
            requested,
            &mut read,
        );
        if result != kResultOk || read <= 0 || read > requested {
            return false;
        }
        bytes = &mut bytes[read as usize..];
    }
    true
}

unsafe fn save_gain_state(stream: *mut c_void, gain: f64) -> tresult {
    if stream.is_null() {
        return kInvalidArgument;
    }
    let Some(bytes) = encode_gain_state(gain) else {
        return kResultFalse;
    };
    if stream_write_all(stream, &bytes) {
        kResultOk
    } else {
        kResultFalse
    }
}

unsafe fn load_gain_state(stream: *mut c_void) -> Result<f64, tresult> {
    if stream.is_null() {
        return Err(kInvalidArgument);
    }
    let mut bytes = [0u8; STATE_LEN];
    if !stream_read_exact(stream, &mut bytes) {
        return Err(kResultFalse);
    }
    decode_gain_state(&bytes).ok_or(kResultFalse)
}

fn midi_note_to_freq(note: i16) -> f64 {
    440.0 * 2.0_f64.powf((note as f64 - 69.0) / 12.0)
}

// =============================================================================
// Processor Implementation - Dual vtables for IComponent + IAudioProcessor
// =============================================================================

#[repr(C)]
struct SineProcessorObj {
    vtbl_component: *const ComponentVtbl,
    vtbl_audio: *const AudioProcessorVtbl,
    vtbl_connection: *const ConnectionPointVtbl,
    ref_count: AtomicI32,
    shared_gain: Arc<SharedGain>,
    gain: f32,
    sample_rate: f64,
    active: bool,
    processing: bool,
    voices: [Voice; MAX_VOICES],
    pending_notes: [PendingNote; MAX_NOTE_EVENTS],
    pending_params: [PendingParam; MAX_PARAM_EVENTS],
}

#[repr(C)]
struct ComponentVtbl {
    query_interface:
        unsafe extern "system" fn(*mut c_void, *const TUID, *mut *mut c_void) -> tresult,
    add_ref: unsafe extern "system" fn(*mut c_void) -> uint32,
    release: unsafe extern "system" fn(*mut c_void) -> uint32,
    initialize: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
    terminate: unsafe extern "system" fn(*mut c_void) -> tresult,
    get_controller_class_id: unsafe extern "system" fn(*mut c_void, *mut TUID) -> tresult,
    set_io_mode: unsafe extern "system" fn(*mut c_void, IoMode) -> tresult,
    get_bus_count: unsafe extern "system" fn(*mut c_void, MediaType, BusDirection) -> int32,
    get_bus_info: unsafe extern "system" fn(
        *mut c_void,
        MediaType,
        BusDirection,
        int32,
        *mut BusInfo,
    ) -> tresult,
    get_routing_info:
        unsafe extern "system" fn(*mut c_void, *mut RoutingInfo, *mut RoutingInfo) -> tresult,
    activate_bus:
        unsafe extern "system" fn(*mut c_void, MediaType, BusDirection, int32, TBool) -> tresult,
    set_active: unsafe extern "system" fn(*mut c_void, TBool) -> tresult,
    set_state: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
    get_state: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
}

#[repr(C)]
struct AudioProcessorVtbl {
    query_interface:
        unsafe extern "system" fn(*mut c_void, *const TUID, *mut *mut c_void) -> tresult,
    add_ref: unsafe extern "system" fn(*mut c_void) -> uint32,
    release: unsafe extern "system" fn(*mut c_void) -> uint32,
    set_bus_arrangements: unsafe extern "system" fn(
        *mut c_void,
        *mut SpeakerArrangement,
        int32,
        *mut SpeakerArrangement,
        int32,
    ) -> tresult,
    get_bus_arrangement: unsafe extern "system" fn(
        *mut c_void,
        BusDirection,
        int32,
        *mut SpeakerArrangement,
    ) -> tresult,
    can_process_sample_size: unsafe extern "system" fn(*mut c_void, int32) -> tresult,
    get_latency_samples: unsafe extern "system" fn(*mut c_void) -> uint32,
    setup_processing: unsafe extern "system" fn(*mut c_void, *mut ProcessSetup) -> tresult,
    set_processing: unsafe extern "system" fn(*mut c_void, TBool) -> tresult,
    process: unsafe extern "system" fn(*mut c_void, *mut ProcessData) -> tresult,
    get_tail_samples: unsafe extern "system" fn(*mut c_void) -> uint32,
}

#[repr(C)]
struct ConnectionPointVtbl {
    query_interface:
        unsafe extern "system" fn(*mut c_void, *const TUID, *mut *mut c_void) -> tresult,
    add_ref: unsafe extern "system" fn(*mut c_void) -> uint32,
    release: unsafe extern "system" fn(*mut c_void) -> uint32,
    connect: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
    disconnect: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
    notify: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
    get_role: unsafe extern "system" fn(*mut c_void) -> u32,
    get_shared_gain: unsafe extern "system" fn(*mut c_void) -> *const SharedGain,
    adopt_shared_gain: unsafe extern "system" fn(*mut c_void, *const SharedGain) -> tresult,
}

static COMPONENT_VTBL: ComponentVtbl = ComponentVtbl {
    query_interface: component_query_interface,
    add_ref: component_add_ref,
    release: component_release,
    initialize: processor_initialize,
    terminate: processor_terminate,
    get_controller_class_id: processor_get_controller_class_id,
    set_io_mode: processor_set_io_mode,
    get_bus_count: processor_get_bus_count,
    get_bus_info: processor_get_bus_info,
    get_routing_info: processor_get_routing_info,
    activate_bus: processor_activate_bus,
    set_active: processor_set_active,
    set_state: processor_set_state,
    get_state: processor_get_state,
};

static AUDIO_VTBL: AudioProcessorVtbl = AudioProcessorVtbl {
    query_interface: audio_query_interface,
    add_ref: audio_add_ref,
    release: audio_release,
    set_bus_arrangements: audio_set_bus_arrangements,
    get_bus_arrangement: audio_get_bus_arrangement,
    can_process_sample_size: audio_can_process_sample_size,
    get_latency_samples: audio_get_latency_samples,
    setup_processing: audio_setup_processing,
    set_processing: audio_set_processing,
    process: audio_process,
    get_tail_samples: audio_get_tail_samples,
};

static PROCESSOR_CONNECTION_VTBL: ConnectionPointVtbl = ConnectionPointVtbl {
    query_interface: processor_connection_query_interface,
    add_ref: processor_connection_add_ref,
    release: processor_connection_release,
    connect: connection_connect,
    disconnect: connection_disconnect,
    notify: connection_notify,
    get_role: processor_connection_get_role,
    get_shared_gain: processor_connection_get_shared_gain,
    adopt_shared_gain: processor_connection_adopt_shared_gain,
};

impl SineProcessorObj {
    fn new() -> *mut Self {
        let obj = Box::new(SineProcessorObj {
            vtbl_component: &COMPONENT_VTBL,
            vtbl_audio: &AUDIO_VTBL,
            vtbl_connection: &PROCESSOR_CONNECTION_VTBL,
            ref_count: AtomicI32::new(1),
            shared_gain: Arc::new(SharedGain::new(0.5)),
            gain: 0.5,
            sample_rate: 44100.0,
            active: false,
            processing: false,
            voices: std::array::from_fn(|_| Voice::new()),
            pending_notes: [PendingNote::EMPTY; MAX_NOTE_EVENTS],
            pending_params: [PendingParam::EMPTY; MAX_PARAM_EVENTS],
        });
        Box::into_raw(obj)
    }

    unsafe fn from_component(this: *mut c_void) -> *mut Self {
        this as *mut Self
    }

    unsafe fn from_audio(this: *mut c_void) -> *mut Self {
        (this as *mut u8).sub(std::mem::offset_of!(Self, vtbl_audio)) as *mut Self
    }

    unsafe fn from_connection(this: *mut c_void) -> *mut Self {
        (this as *mut u8).sub(std::mem::offset_of!(Self, vtbl_connection)) as *mut Self
    }
}

// Component interface functions
unsafe extern "system" fn component_query_interface(
    this: *mut c_void,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    let iid = &*iid;
    let base = SineProcessorObj::from_component(this);

    if iid_equal(iid, &iid::IUnknown)
        || iid_equal(iid, &base_iid::IPluginBase)
        || iid_equal(iid, &vst_iid::IComponent)
    {
        component_add_ref(this);
        *obj = this;
        return kResultOk;
    }

    if iid_equal(iid, &vst_iid::IAudioProcessor) {
        component_add_ref(this);
        *obj = &(*base).vtbl_audio as *const _ as *mut c_void;
        return kResultOk;
    }

    if iid_equal(iid, &vst_iid::IConnectionPoint) {
        component_add_ref(this);
        *obj = &(*base).vtbl_connection as *const _ as *mut c_void;
        return kResultOk;
    }

    *obj = std::ptr::null_mut();
    kNoInterface
}

unsafe extern "system" fn component_add_ref(this: *mut c_void) -> uint32 {
    let obj = SineProcessorObj::from_component(this);
    (*obj).ref_count.fetch_add(1, Ordering::SeqCst) as uint32 + 1
}

unsafe extern "system" fn component_release(this: *mut c_void) -> uint32 {
    let obj = SineProcessorObj::from_component(this);
    let count = (*obj).ref_count.fetch_sub(1, Ordering::SeqCst) - 1;
    if count == 0 {
        let _ = Box::from_raw(obj);
    }
    count as uint32
}

// Audio interface functions
unsafe extern "system" fn audio_query_interface(
    this: *mut c_void,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    let iid = &*iid;
    let base = SineProcessorObj::from_audio(this);

    if iid_equal(iid, &vst_iid::IAudioProcessor) {
        audio_add_ref(this);
        *obj = this;
        return kResultOk;
    }

    if iid_equal(iid, &iid::IUnknown)
        || iid_equal(iid, &base_iid::IPluginBase)
        || iid_equal(iid, &vst_iid::IComponent)
    {
        audio_add_ref(this);
        *obj = base as *mut c_void;
        return kResultOk;
    }

    if iid_equal(iid, &vst_iid::IConnectionPoint) {
        audio_add_ref(this);
        *obj = &(*base).vtbl_connection as *const _ as *mut c_void;
        return kResultOk;
    }

    *obj = std::ptr::null_mut();
    kNoInterface
}

unsafe extern "system" fn audio_add_ref(this: *mut c_void) -> uint32 {
    let obj = SineProcessorObj::from_audio(this);
    (*obj).ref_count.fetch_add(1, Ordering::SeqCst) as uint32 + 1
}

unsafe extern "system" fn audio_release(this: *mut c_void) -> uint32 {
    let obj = SineProcessorObj::from_audio(this);
    let count = (*obj).ref_count.fetch_sub(1, Ordering::SeqCst) - 1;
    if count == 0 {
        let _ = Box::from_raw(obj);
    }
    count as uint32
}

unsafe extern "system" fn processor_connection_query_interface(
    this: *mut c_void,
    requested_iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    if requested_iid.is_null() || obj.is_null() {
        return kInvalidArgument;
    }
    if iid_equal(&*requested_iid, &SHARED_GAIN_CONNECTION_IID)
        || iid_equal(&*requested_iid, &vst_iid::IConnectionPoint)
    {
        processor_connection_add_ref(this);
        *obj = this;
        return kResultOk;
    }
    component_query_interface(
        SineProcessorObj::from_connection(this) as *mut c_void,
        requested_iid,
        obj,
    )
}

unsafe extern "system" fn processor_connection_add_ref(this: *mut c_void) -> uint32 {
    component_add_ref(SineProcessorObj::from_connection(this) as *mut c_void)
}

unsafe extern "system" fn processor_connection_release(this: *mut c_void) -> uint32 {
    component_release(SineProcessorObj::from_connection(this) as *mut c_void)
}

unsafe extern "system" fn processor_connection_get_role(_this: *mut c_void) -> u32 {
    CONNECTION_ROLE_PROCESSOR
}

unsafe extern "system" fn processor_connection_get_shared_gain(
    this: *mut c_void,
) -> *const SharedGain {
    let obj = SineProcessorObj::from_connection(this);
    Arc::as_ptr(&(*obj).shared_gain)
}

unsafe extern "system" fn processor_connection_adopt_shared_gain(
    _this: *mut c_void,
    _shared_gain: *const SharedGain,
) -> tresult {
    kInvalidArgument
}

unsafe extern "system" fn connection_connect(this: *mut c_void, other: *mut c_void) -> tresult {
    if this.is_null() || other.is_null() {
        return kInvalidArgument;
    }
    let this_vtbl = *(this as *const *const ConnectionPointVtbl);
    if this_vtbl.is_null() {
        return kInvalidArgument;
    }
    let other_unknown = *(other as *const *const IUnknownVtbl);
    if other_unknown.is_null() {
        return kInvalidArgument;
    }

    let mut peer = std::ptr::null_mut();
    let query_result =
        ((*other_unknown).query_interface)(other, &SHARED_GAIN_CONNECTION_IID, &mut peer);
    if query_result != kResultOk || peer.is_null() {
        return kNoInterface;
    }

    let peer_vtbl = *(peer as *const *const ConnectionPointVtbl);
    if peer_vtbl.is_null() {
        ((*other_unknown).release)(other);
        return kInvalidArgument;
    }
    let this_role = ((*this_vtbl).get_role)(this);
    let peer_role = ((*peer_vtbl).get_role)(peer);
    let result = match (this_role, peer_role) {
        (CONNECTION_ROLE_PROCESSOR, CONNECTION_ROLE_CONTROLLER) => {
            let shared_gain = ((*this_vtbl).get_shared_gain)(this);
            ((*peer_vtbl).adopt_shared_gain)(peer, shared_gain)
        }
        (CONNECTION_ROLE_CONTROLLER, CONNECTION_ROLE_PROCESSOR) => {
            let shared_gain = ((*peer_vtbl).get_shared_gain)(peer);
            ((*this_vtbl).adopt_shared_gain)(this, shared_gain)
        }
        _ => kInvalidArgument,
    };

    ((*peer_vtbl).release)(peer);
    result
}

unsafe extern "system" fn connection_disconnect(
    _this: *mut c_void,
    _other: *mut c_void,
) -> tresult {
    kResultOk
}

unsafe extern "system" fn connection_notify(_this: *mut c_void, _message: *mut c_void) -> tresult {
    kResultOk
}

// IComponent methods
unsafe extern "system" fn processor_initialize(
    _this: *mut c_void,
    _context: *mut c_void,
) -> tresult {
    kResultOk
}
unsafe extern "system" fn processor_terminate(_this: *mut c_void) -> tresult {
    kResultOk
}
unsafe extern "system" fn processor_get_controller_class_id(
    _this: *mut c_void,
    class_id: *mut TUID,
) -> tresult {
    *class_id = CID_CONTROLLER;
    kResultOk
}
unsafe extern "system" fn processor_set_io_mode(_this: *mut c_void, _mode: IoMode) -> tresult {
    kResultOk
}

unsafe extern "system" fn processor_get_bus_count(
    _this: *mut c_void,
    media_type: MediaType,
    dir: BusDirection,
) -> int32 {
    match (media_type, dir) {
        (m, d) if m == MediaTypes::kAudio && d == BusDirections::kOutput => 1,
        (m, d) if m == MediaTypes::kEvent && d == BusDirections::kInput => 1,
        _ => 0,
    }
}

unsafe extern "system" fn processor_get_bus_info(
    _this: *mut c_void,
    media_type: MediaType,
    dir: BusDirection,
    index: int32,
    bus: *mut BusInfo,
) -> tresult {
    if index != 0 {
        return kInvalidArgument;
    }
    let bus = &mut *bus;

    if media_type == MediaTypes::kAudio && dir == BusDirections::kOutput {
        bus.media_type = MediaTypes::kAudio;
        bus.direction = BusDirections::kOutput;
        bus.channel_count = 2;
        bus.bus_type = BusTypes::kMain;
        bus.flags = BusFlags::kDefaultActive;
        str16cpy_safe(&mut bus.name, "Output");
        return kResultOk;
    }

    if media_type == MediaTypes::kEvent && dir == BusDirections::kInput {
        bus.media_type = MediaTypes::kEvent;
        bus.direction = BusDirections::kInput;
        bus.channel_count = 1;
        bus.bus_type = BusTypes::kMain;
        bus.flags = BusFlags::kDefaultActive;
        str16cpy_safe(&mut bus.name, "MIDI In");
        return kResultOk;
    }

    kInvalidArgument
}

unsafe extern "system" fn processor_get_routing_info(
    _this: *mut c_void,
    _in_info: *mut RoutingInfo,
    _out_info: *mut RoutingInfo,
) -> tresult {
    kNotImplemented
}
unsafe extern "system" fn processor_activate_bus(
    _this: *mut c_void,
    _media_type: MediaType,
    _dir: BusDirection,
    _index: int32,
    _state: TBool,
) -> tresult {
    kResultOk
}
unsafe extern "system" fn processor_set_active(_this: *mut c_void, _state: TBool) -> tresult {
    kResultOk
}
unsafe extern "system" fn processor_set_state(this: *mut c_void, state: *mut c_void) -> tresult {
    let gain = match load_gain_state(state) {
        Ok(gain) => gain,
        Err(result) => return result,
    };
    let obj = SineProcessorObj::from_component(this);
    (*obj).gain = gain as f32;
    (*obj).shared_gain.store(gain as f32);
    kResultOk
}
unsafe extern "system" fn processor_get_state(this: *mut c_void, state: *mut c_void) -> tresult {
    let obj = SineProcessorObj::from_component(this);
    save_gain_state(state, (*obj).shared_gain.load() as f64)
}

// IAudioProcessor methods
unsafe extern "system" fn audio_set_bus_arrangements(
    _this: *mut c_void,
    _inputs: *mut SpeakerArrangement,
    _num_ins: int32,
    _outputs: *mut SpeakerArrangement,
    _num_outs: int32,
) -> tresult {
    kResultOk
}
unsafe extern "system" fn audio_get_bus_arrangement(
    _this: *mut c_void,
    _dir: BusDirection,
    _index: int32,
    arr: *mut SpeakerArrangement,
) -> tresult {
    *arr = SpeakerArr::kStereo;
    kResultOk
}
unsafe extern "system" fn audio_can_process_sample_size(
    _this: *mut c_void,
    symbolic_sample_size: int32,
) -> tresult {
    if symbolic_sample_size == SymbolicSampleSizes::kSample32 {
        kResultOk
    } else {
        kResultFalse
    }
}
unsafe extern "system" fn audio_get_latency_samples(_this: *mut c_void) -> uint32 {
    0
}
unsafe extern "system" fn audio_setup_processing(
    this: *mut c_void,
    setup: *mut ProcessSetup,
) -> tresult {
    let obj = SineProcessorObj::from_audio(this);
    (*obj).sample_rate = (*setup).sample_rate;
    kResultOk
}
unsafe extern "system" fn audio_set_processing(this: *mut c_void, state: TBool) -> tresult {
    let obj = SineProcessorObj::from_audio(this);
    (*obj).processing = state != 0;
    kResultOk
}

unsafe extern "system" fn audio_process(this: *mut c_void, data: *mut ProcessData) -> tresult {
    if data.is_null() {
        return kInvalidArgument;
    }
    let obj = SineProcessorObj::from_audio(this);
    let data = &*data;

    if data.num_samples < 0 || data.num_outputs < 0 {
        return kInvalidArgument;
    }
    if data.symbolic_sample_size != SymbolicSampleSizes::kSample32 {
        return kResultFalse;
    }

    let num_samples = data.num_samples as usize;
    let shared_snapshot = (*obj).shared_gain.snapshot();
    let mut gain = SharedGain::value(shared_snapshot);
    (*obj).gain = gain;

    let mut param_count = 0usize;
    if !data.input_parameter_changes.is_null() {
        let param_changes = data.input_parameter_changes;
        let vtbl = *(param_changes as *const *const IParameterChangesVtbl);
        if vtbl.is_null() {
            return kInvalidArgument;
        }
        let num_params = ((*vtbl).get_parameter_count)(param_changes);
        if num_params < 0 {
            return kInvalidArgument;
        }

        for queue_index in 0..num_params {
            let queue = ((*vtbl).get_parameter_data)(param_changes, queue_index);
            if queue.is_null() {
                continue;
            }
            let queue_vtbl = *(queue as *const *const IParamValueQueueVtbl);
            if queue_vtbl.is_null() {
                return kInvalidArgument;
            }
            if ((*queue_vtbl).get_parameter_id)(queue) != PARAM_GAIN {
                continue;
            }

            let num_points = ((*queue_vtbl).get_point_count)(queue);
            if num_points < 0 {
                return kInvalidArgument;
            }
            for point_index in 0..num_points {
                let mut sample_offset = 0;
                let mut value = 0.0;
                if ((*queue_vtbl).get_point)(queue, point_index, &mut sample_offset, &mut value)
                    != kResultOk
                    || !value.is_finite()
                {
                    continue;
                }
                if param_count == MAX_PARAM_EVENTS {
                    return kOutOfMemory;
                }
                (*obj).pending_params[param_count] = PendingParam {
                    sample_offset: if num_samples == 0 {
                        0
                    } else {
                        sample_offset.clamp(0, data.num_samples - 1) as usize
                    },
                    value: value.clamp(0.0, 1.0) as f32,
                };
                param_count += 1;
            }
        }
    }
    sort_pending_params(&mut (&mut (*obj).pending_params)[..param_count]);

    if num_samples == 0 {
        for pending in &(&(*obj).pending_params)[..param_count] {
            gain = pending.value;
        }
        (*obj).gain = gain;
        if param_count > 0 {
            (*obj).shared_gain.store_if_unchanged(shared_snapshot, gain);
        }
        return kResultOk;
    }

    let mut note_count = 0usize;
    if !data.input_events.is_null() {
        let events = data.input_events;
        let vtbl = *(events as *const *const IEventListVtbl);
        if vtbl.is_null() {
            return kInvalidArgument;
        }
        let count = ((*vtbl).get_event_count)(events);

        for i in 0..count {
            let mut event = std::mem::zeroed::<Event>();
            if ((*vtbl).get_event)(events, i, &mut event) == kResultOk {
                let pending = match event.type_ {
                    t if t == EventTypes::kNoteOnEvent => {
                        let note = event.event.note_on;
                        Some(PendingNote {
                            sample_offset: event.sample_offset.clamp(0, data.num_samples - 1)
                                as usize,
                            kind: PendingNoteKind::On,
                            note_id: note.note_id,
                            channel: note.channel,
                            pitch: note.pitch,
                            velocity: note.velocity.clamp(0.0, 1.0),
                        })
                    }
                    t if t == EventTypes::kNoteOffEvent => {
                        let note = event.event.note_off;
                        Some(PendingNote {
                            sample_offset: event.sample_offset.clamp(0, data.num_samples - 1)
                                as usize,
                            kind: PendingNoteKind::Off,
                            note_id: note.note_id,
                            channel: note.channel,
                            pitch: note.pitch,
                            velocity: note.velocity.clamp(0.0, 1.0),
                        })
                    }
                    _ => None,
                };
                if let Some(pending) = pending {
                    if note_count == MAX_NOTE_EVENTS {
                        return kOutOfMemory;
                    }
                    (*obj).pending_notes[note_count] = pending;
                    note_count += 1;
                }
            }
        }
    }
    sort_pending_notes(&mut (&mut (*obj).pending_notes)[..note_count]);

    let (channel_count, output_buffers) = if data.num_outputs > 0 {
        if data.outputs.is_null() {
            return kInvalidArgument;
        }
        let outputs = &mut *data.outputs;
        if outputs.num_channels < 0 || (outputs.num_channels > 0 && outputs.buffers.is_null()) {
            return kInvalidArgument;
        }
        let channel_count = outputs.num_channels as usize;
        for channel in 0..channel_count {
            let output = *(outputs.buffers as *const *mut f32).add(channel);
            if !output.is_null() {
                std::ptr::write_bytes(output, 0, num_samples);
            }
        }
        (channel_count, outputs.buffers)
    } else {
        (0, std::ptr::null_mut())
    };

    let mut note_index = 0usize;
    let mut param_index = 0usize;
    for sample_index in 0..num_samples {
        while param_index < param_count
            && (*obj).pending_params[param_index].sample_offset <= sample_index
        {
            gain = (*obj).pending_params[param_index].value;
            param_index += 1;
        }

        while note_index < note_count
            && (*obj).pending_notes[note_index].sample_offset <= sample_index
        {
            let pending = (*obj).pending_notes[note_index];
            match pending.kind {
                PendingNoteKind::On => {
                    if let Some(voice) = (*obj).voices.iter_mut().find(|voice| !voice.active) {
                        voice.active = true;
                        voice.note_id = pending.note_id;
                        voice.channel = pending.channel;
                        voice.pitch = pending.pitch;
                        voice.velocity = pending.velocity;
                        voice.phase = 0.0;
                        voice.phase_inc = midi_note_to_freq(pending.pitch) / (*obj).sample_rate;
                    }
                }
                PendingNoteKind::Off => {
                    if let Some(voice) = (*obj).voices.iter_mut().find(|voice| {
                        voice.active
                            && if pending.note_id >= 0 {
                                voice.note_id == pending.note_id
                            } else {
                                voice.channel == pending.channel && voice.pitch == pending.pitch
                            }
                    }) {
                        voice.active = false;
                    }
                }
            }
            note_index += 1;
        }

        let mut sample = 0.0f32;
        for voice in (*obj).voices.iter_mut().filter(|voice| voice.active) {
            sample += (voice.phase * 2.0 * PI).sin() as f32 * voice.velocity * gain;
            voice.phase += voice.phase_inc;
            if voice.phase >= 1.0 {
                voice.phase -= 1.0;
            }
        }
        for channel in 0..channel_count {
            let output = *(output_buffers as *const *mut f32).add(channel);
            if !output.is_null() {
                *output.add(sample_index) = sample;
            }
        }
    }

    (*obj).gain = gain;
    if param_count > 0 {
        (*obj).shared_gain.store_if_unchanged(shared_snapshot, gain);
    }

    kResultOk
}

unsafe extern "system" fn audio_get_tail_samples(_this: *mut c_void) -> uint32 {
    kNoTail
}

// =============================================================================
// Controller Implementation
// =============================================================================

#[repr(C)]
struct SineControllerObj {
    vtbl: *const ControllerVtbl,
    vtbl_connection: *const ConnectionPointVtbl,
    ref_count: AtomicI32,
    shared_gain: Arc<SharedGain>,
}

#[repr(C)]
struct ControllerVtbl {
    query_interface:
        unsafe extern "system" fn(*mut c_void, *const TUID, *mut *mut c_void) -> tresult,
    add_ref: unsafe extern "system" fn(*mut c_void) -> uint32,
    release: unsafe extern "system" fn(*mut c_void) -> uint32,
    initialize: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
    terminate: unsafe extern "system" fn(*mut c_void) -> tresult,
    set_component_state: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
    set_state: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
    get_state: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
    get_parameter_count: unsafe extern "system" fn(*mut c_void) -> int32,
    get_parameter_info:
        unsafe extern "system" fn(*mut c_void, int32, *mut ParameterInfo) -> tresult,
    get_param_string_by_value:
        unsafe extern "system" fn(*mut c_void, ParamID, ParamValue, *mut String128) -> tresult,
    get_param_value_by_string:
        unsafe extern "system" fn(*mut c_void, ParamID, *const TChar, *mut ParamValue) -> tresult,
    normalized_param_to_plain:
        unsafe extern "system" fn(*mut c_void, ParamID, ParamValue) -> ParamValue,
    plain_param_to_normalized:
        unsafe extern "system" fn(*mut c_void, ParamID, ParamValue) -> ParamValue,
    get_param_normalized: unsafe extern "system" fn(*mut c_void, ParamID) -> ParamValue,
    set_param_normalized: unsafe extern "system" fn(*mut c_void, ParamID, ParamValue) -> tresult,
    set_component_handler: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
    create_view: unsafe extern "system" fn(*mut c_void, FIDString) -> *mut c_void,
}

static CONTROLLER_VTBL: ControllerVtbl = ControllerVtbl {
    query_interface: controller_query_interface,
    add_ref: controller_add_ref,
    release: controller_release,
    initialize: controller_initialize,
    terminate: controller_terminate,
    set_component_state: controller_set_component_state,
    set_state: controller_set_state,
    get_state: controller_get_state,
    get_parameter_count: controller_get_parameter_count,
    get_parameter_info: controller_get_parameter_info,
    get_param_string_by_value: controller_get_param_string_by_value,
    get_param_value_by_string: controller_get_param_value_by_string,
    normalized_param_to_plain: controller_normalized_param_to_plain,
    plain_param_to_normalized: controller_plain_param_to_normalized,
    get_param_normalized: controller_get_param_normalized,
    set_param_normalized: controller_set_param_normalized,
    set_component_handler: controller_set_component_handler,
    create_view: controller_create_view,
};

static CONTROLLER_CONNECTION_VTBL: ConnectionPointVtbl = ConnectionPointVtbl {
    query_interface: controller_connection_query_interface,
    add_ref: controller_connection_add_ref,
    release: controller_connection_release,
    connect: connection_connect,
    disconnect: connection_disconnect,
    notify: connection_notify,
    get_role: controller_connection_get_role,
    get_shared_gain: controller_connection_get_shared_gain,
    adopt_shared_gain: controller_connection_adopt_shared_gain,
};

impl SineControllerObj {
    fn new() -> *mut Self {
        Box::into_raw(Box::new(SineControllerObj {
            vtbl: &CONTROLLER_VTBL,
            vtbl_connection: &CONTROLLER_CONNECTION_VTBL,
            ref_count: AtomicI32::new(1),
            shared_gain: Arc::new(SharedGain::new(0.5)),
        }))
    }

    unsafe fn from_connection(this: *mut c_void) -> *mut Self {
        (this as *mut u8).sub(std::mem::offset_of!(Self, vtbl_connection)) as *mut Self
    }
}

unsafe extern "system" fn controller_query_interface(
    this: *mut c_void,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    let iid = &*iid;
    if iid_equal(iid, &iid::IUnknown)
        || iid_equal(iid, &base_iid::IPluginBase)
        || iid_equal(iid, &vst_iid::IEditController)
    {
        controller_add_ref(this);
        *obj = this;
        return kResultOk;
    }
    if iid_equal(iid, &vst_iid::IConnectionPoint) {
        controller_add_ref(this);
        *obj = &(*(this as *mut SineControllerObj)).vtbl_connection as *const _ as *mut c_void;
        return kResultOk;
    }
    *obj = std::ptr::null_mut();
    kNoInterface
}

unsafe extern "system" fn controller_add_ref(this: *mut c_void) -> uint32 {
    (*(this as *mut SineControllerObj))
        .ref_count
        .fetch_add(1, Ordering::SeqCst) as uint32
        + 1
}

unsafe extern "system" fn controller_release(this: *mut c_void) -> uint32 {
    let obj = this as *mut SineControllerObj;
    let count = (*obj).ref_count.fetch_sub(1, Ordering::SeqCst) - 1;
    if count == 0 {
        let _ = Box::from_raw(obj);
    }
    count as uint32
}

unsafe extern "system" fn controller_connection_query_interface(
    this: *mut c_void,
    requested_iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    if requested_iid.is_null() || obj.is_null() {
        return kInvalidArgument;
    }
    if iid_equal(&*requested_iid, &SHARED_GAIN_CONNECTION_IID)
        || iid_equal(&*requested_iid, &vst_iid::IConnectionPoint)
    {
        controller_connection_add_ref(this);
        *obj = this;
        return kResultOk;
    }
    controller_query_interface(
        SineControllerObj::from_connection(this) as *mut c_void,
        requested_iid,
        obj,
    )
}

unsafe extern "system" fn controller_connection_add_ref(this: *mut c_void) -> uint32 {
    controller_add_ref(SineControllerObj::from_connection(this) as *mut c_void)
}

unsafe extern "system" fn controller_connection_release(this: *mut c_void) -> uint32 {
    controller_release(SineControllerObj::from_connection(this) as *mut c_void)
}

unsafe extern "system" fn controller_connection_get_role(_this: *mut c_void) -> u32 {
    CONNECTION_ROLE_CONTROLLER
}

unsafe extern "system" fn controller_connection_get_shared_gain(
    this: *mut c_void,
) -> *const SharedGain {
    let obj = SineControllerObj::from_connection(this);
    Arc::as_ptr(&(*obj).shared_gain)
}

unsafe extern "system" fn controller_connection_adopt_shared_gain(
    this: *mut c_void,
    shared_gain: *const SharedGain,
) -> tresult {
    if shared_gain.is_null() {
        return kInvalidArgument;
    }
    let obj = SineControllerObj::from_connection(this);
    if Arc::as_ptr(&(*obj).shared_gain) == shared_gain {
        return kResultOk;
    }
    Arc::increment_strong_count(shared_gain);
    (*obj).shared_gain = Arc::from_raw(shared_gain);
    kResultOk
}

unsafe extern "system" fn controller_initialize(
    _this: *mut c_void,
    _context: *mut c_void,
) -> tresult {
    kResultOk
}
unsafe extern "system" fn controller_terminate(_this: *mut c_void) -> tresult {
    kResultOk
}
unsafe extern "system" fn controller_set_component_state(
    this: *mut c_void,
    state: *mut c_void,
) -> tresult {
    let gain = match load_gain_state(state) {
        Ok(gain) => gain,
        Err(result) => return result,
    };
    (*(this as *mut SineControllerObj))
        .shared_gain
        .store(gain as f32);
    kResultOk
}
unsafe extern "system" fn controller_set_state(this: *mut c_void, state: *mut c_void) -> tresult {
    controller_set_component_state(this, state)
}
unsafe extern "system" fn controller_get_state(this: *mut c_void, state: *mut c_void) -> tresult {
    save_gain_state(
        state,
        (*(this as *mut SineControllerObj)).shared_gain.load() as f64,
    )
}
unsafe extern "system" fn controller_get_parameter_count(_this: *mut c_void) -> int32 {
    1
}

unsafe extern "system" fn controller_get_parameter_info(
    _this: *mut c_void,
    param_index: int32,
    info: *mut ParameterInfo,
) -> tresult {
    if param_index != 0 {
        return kInvalidArgument;
    }
    let info = &mut *info;
    info.id = PARAM_GAIN;
    str16cpy_safe(&mut info.title, "Gain");
    str16cpy_safe(&mut info.short_title, "Gain");
    str16cpy_safe(&mut info.units, "%");
    info.step_count = 0;
    info.default_normalized_value = 0.5;
    info.unit_id = 0;
    info.flags = ParameterFlags::kCanAutomate;
    kResultOk
}

unsafe extern "system" fn controller_get_param_string_by_value(
    _this: *mut c_void,
    id: ParamID,
    value: ParamValue,
    string: *mut String128,
) -> tresult {
    if id == PARAM_GAIN {
        str16cpy_safe(&mut *string, &format!("{}%", (value * 100.0) as i32));
        return kResultOk;
    }
    kInvalidArgument
}

unsafe extern "system" fn controller_get_param_value_by_string(
    _this: *mut c_void,
    _id: ParamID,
    _string: *const TChar,
    _value: *mut ParamValue,
) -> tresult {
    kNotImplemented
}
unsafe extern "system" fn controller_normalized_param_to_plain(
    _this: *mut c_void,
    _id: ParamID,
    value: ParamValue,
) -> ParamValue {
    value * 100.0
}
unsafe extern "system" fn controller_plain_param_to_normalized(
    _this: *mut c_void,
    _id: ParamID,
    value: ParamValue,
) -> ParamValue {
    value / 100.0
}

unsafe extern "system" fn controller_get_param_normalized(
    this: *mut c_void,
    id: ParamID,
) -> ParamValue {
    if id == PARAM_GAIN {
        (*(this as *mut SineControllerObj)).shared_gain.load() as f64
    } else {
        0.0
    }
}

unsafe extern "system" fn controller_set_param_normalized(
    this: *mut c_void,
    id: ParamID,
    value: ParamValue,
) -> tresult {
    if id != PARAM_GAIN || !value.is_finite() {
        return kInvalidArgument;
    }
    (*(this as *mut SineControllerObj))
        .shared_gain
        .store(value.clamp(0.0, 1.0) as f32);
    kResultOk
}

unsafe extern "system" fn controller_set_component_handler(
    _this: *mut c_void,
    _handler: *mut c_void,
) -> tresult {
    kResultOk
}
unsafe extern "system" fn controller_create_view(
    _this: *mut c_void,
    _name: FIDString,
) -> *mut c_void {
    std::ptr::null_mut()
}

// =============================================================================
// Plugin Factory
// =============================================================================

#[repr(C)]
struct PluginFactoryObj {
    vtbl: *const PluginFactoryVtbl,
    ref_count: AtomicI32,
}

#[repr(C)]
struct PluginFactoryVtbl {
    query_interface:
        unsafe extern "system" fn(*mut c_void, *const TUID, *mut *mut c_void) -> tresult,
    add_ref: unsafe extern "system" fn(*mut c_void) -> uint32,
    release: unsafe extern "system" fn(*mut c_void) -> uint32,
    get_factory_info: unsafe extern "system" fn(*mut c_void, *mut PFactoryInfoData) -> tresult,
    count_classes: unsafe extern "system" fn(*mut c_void) -> int32,
    get_class_info: unsafe extern "system" fn(*mut c_void, int32, *mut PClassInfoData) -> tresult,
    create_instance:
        unsafe extern "system" fn(*mut c_void, FIDString, FIDString, *mut *mut c_void) -> tresult,
    get_class_info2: unsafe extern "system" fn(*mut c_void, int32, *mut PClassInfo2Data) -> tresult,
    get_class_info_unicode:
        unsafe extern "system" fn(*mut c_void, int32, *mut PClassInfoWData) -> tresult,
    set_host_context: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
}

static FACTORY_VTBL: PluginFactoryVtbl = PluginFactoryVtbl {
    query_interface: factory_query_interface,
    add_ref: factory_add_ref,
    release: factory_release,
    get_factory_info: factory_get_factory_info,
    count_classes: factory_count_classes,
    get_class_info: factory_get_class_info,
    create_instance: factory_create_instance,
    get_class_info2: factory_get_class_info2,
    get_class_info_unicode: factory_get_class_info_unicode,
    set_host_context: factory_set_host_context,
};

struct SendSyncPtr(*mut PluginFactoryObj);
unsafe impl Send for SendSyncPtr {}
unsafe impl Sync for SendSyncPtr {}
static FACTORY: OnceLock<SendSyncPtr> = OnceLock::new();

unsafe extern "system" fn factory_query_interface(
    this: *mut c_void,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    let iid = &*iid;
    if iid_equal(iid, &iid::IUnknown)
        || iid_equal(iid, &base_iid::IPluginFactory)
        || iid_equal(iid, &base_iid::IPluginFactory2)
        || iid_equal(iid, &base_iid::IPluginFactory3)
    {
        factory_add_ref(this);
        *obj = this;
        return kResultOk;
    }
    *obj = std::ptr::null_mut();
    kNoInterface
}

unsafe extern "system" fn factory_add_ref(this: *mut c_void) -> uint32 {
    (*(this as *mut PluginFactoryObj))
        .ref_count
        .fetch_add(1, Ordering::SeqCst) as uint32
        + 1
}

unsafe extern "system" fn factory_release(this: *mut c_void) -> uint32 {
    let count = (*(this as *mut PluginFactoryObj))
        .ref_count
        .fetch_sub(1, Ordering::SeqCst)
        - 1;
    // The singleton keeps its initial reference for the module lifetime.
    count as uint32
}

unsafe extern "system" fn factory_get_factory_info(
    _this: *mut c_void,
    info: *mut PFactoryInfoData,
) -> tresult {
    let info = &mut *info;
    strcpy_safe(&mut info.vendor, b"aizcutei\0");
    strcpy_safe(&mut info.url, b"https://aizcutei.github.io/sunmao\0");
    strcpy_safe(&mut info.email, b"info@example.com\0");
    info.flags = PFactoryInfo::Flags::kUnicode;
    kResultOk
}

unsafe extern "system" fn factory_count_classes(_this: *mut c_void) -> int32 {
    2
}

unsafe extern "system" fn factory_get_class_info(
    _this: *mut c_void,
    index: int32,
    info: *mut PClassInfoData,
) -> tresult {
    let info = &mut *info;
    match index {
        0 => {
            info.cid = CID_PROCESSOR;
            info.cardinality = PClassInfo::kManyInstances;
            strcpy_safe(&mut info.category, kVstAudioEffectClass);
            strcpy_safe(&mut info.name, b"Vst3 Sys Syn Sine\0");
        }
        1 => {
            info.cid = CID_CONTROLLER;
            info.cardinality = PClassInfo::kManyInstances;
            strcpy_safe(&mut info.category, kVstComponentControllerClass);
            strcpy_safe(&mut info.name, b"Vst3 Sys Syn Sine Controller\0");
        }
        _ => return kInvalidArgument,
    }
    kResultOk
}

unsafe extern "system" fn factory_get_class_info2(
    _this: *mut c_void,
    index: int32,
    info: *mut PClassInfo2Data,
) -> tresult {
    let info = &mut *info;
    match index {
        0 => {
            info.cid = CID_PROCESSOR;
            info.cardinality = PClassInfo::kManyInstances;
            strcpy_safe(&mut info.category, kVstAudioEffectClass);
            strcpy_safe(&mut info.name, b"Vst3 Sys Syn Sine\0");
            info.class_flags = ComponentFlags::kSimpleModeSupported;
            strcpy_safe(&mut info.sub_categories, PlugType::kInstrumentSynth);
            strcpy_safe(&mut info.vendor, b"aizcutei\0");
            strcpy_safe(&mut info.version, b"0.1.0\0");
            strcpy_safe(&mut info.sdk_version, b"VST 3.8.0\0");
        }
        1 => {
            info.cid = CID_CONTROLLER;
            info.cardinality = PClassInfo::kManyInstances;
            strcpy_safe(&mut info.category, kVstComponentControllerClass);
            strcpy_safe(&mut info.name, b"Vst3 Sys Syn Sine Controller\0");
            info.class_flags = 0;
            strcpy_safe(&mut info.sub_categories, b"\0");
            strcpy_safe(&mut info.vendor, b"aizcutei\0");
            strcpy_safe(&mut info.version, b"0.1.0\0");
            strcpy_safe(&mut info.sdk_version, b"VST 3.8.0\0");
        }
        _ => return kInvalidArgument,
    }
    kResultOk
}

unsafe extern "system" fn factory_create_instance(
    _this: *mut c_void,
    cid: FIDString,
    requested_iid: FIDString,
    obj: *mut *mut c_void,
) -> tresult {
    if obj.is_null() {
        return kInvalidArgument;
    }
    *obj = std::ptr::null_mut();
    if cid.is_null() || requested_iid.is_null() {
        return kInvalidArgument;
    }

    let cid_bytes = std::slice::from_raw_parts(cid as *const i8, 16);
    let mut cid_arr: TUID = [0; 16];
    cid_arr.copy_from_slice(cid_bytes);

    let instance = if iid_equal(&cid_arr, &CID_PROCESSOR) {
        SineProcessorObj::new() as *mut c_void
    } else if iid_equal(&cid_arr, &CID_CONTROLLER) {
        SineControllerObj::new() as *mut c_void
    } else {
        return kNoInterface;
    };

    let unknown = *(instance as *const *const IUnknownVtbl);
    let result = ((*unknown).query_interface)(instance, requested_iid as *const TUID, obj);
    ((*unknown).release)(instance);
    if result == kResultOk && !(*obj).is_null() {
        kResultOk
    } else {
        *obj = std::ptr::null_mut();
        if result == kResultOk {
            kNoInterface
        } else {
            result
        }
    }
}

unsafe extern "system" fn factory_get_class_info_unicode(
    _this: *mut c_void,
    index: int32,
    info: *mut PClassInfoWData,
) -> tresult {
    let info = &mut *info;
    match index {
        0 => {
            info.cid = CID_PROCESSOR;
            info.cardinality = PClassInfo::kManyInstances;
            strcpy_safe(&mut info.category, kVstAudioEffectClass);
            str16cpy(&mut info.name, "Vst3 Sys Syn Sine");
            info.class_flags = ComponentFlags::kSimpleModeSupported;
            strcpy_safe(&mut info.sub_categories, PlugType::kInstrumentSynth);
            str16cpy(&mut info.vendor, "aizcutei");
            str16cpy(&mut info.version, "0.1.0");
            str16cpy(&mut info.sdk_version, "VST 3.8.0");
        }
        1 => {
            info.cid = CID_CONTROLLER;
            info.cardinality = PClassInfo::kManyInstances;
            strcpy_safe(&mut info.category, kVstComponentControllerClass);
            str16cpy(&mut info.name, "Vst3 Sys Syn Sine Controller");
            info.class_flags = 0;
            strcpy_safe(&mut info.sub_categories, b"\0");
            str16cpy(&mut info.vendor, "aizcutei");
            str16cpy(&mut info.version, "0.1.0");
            str16cpy(&mut info.sdk_version, "VST 3.8.0");
        }
        _ => return kInvalidArgument,
    }
    kResultOk
}

unsafe extern "system" fn factory_set_host_context(
    _this: *mut c_void,
    _context: *mut c_void,
) -> tresult {
    kResultOk
}

// =============================================================================
// Entry Points
// =============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn GetPluginFactory() -> *mut c_void {
    let factory = FACTORY
        .get_or_init(|| {
            SendSyncPtr(Box::into_raw(Box::new(PluginFactoryObj {
                vtbl: &FACTORY_VTBL,
                ref_count: AtomicI32::new(1),
            })))
        })
        .0 as *mut c_void;
    unsafe { factory_add_ref(factory) };
    factory
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn bundleEntry(_bundle: *mut c_void) -> bool {
    true
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn bundleExit() -> bool {
    true
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn ModuleEntry(_: *mut c_void) -> bool {
    true
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn ModuleExit() -> bool {
    true
}

#[cfg(target_os = "windows")]
#[unsafe(no_mangle)]
pub extern "C" fn InitDll() -> bool {
    true
}

#[cfg(target_os = "windows")]
#[unsafe(no_mangle)]
pub extern "C" fn ExitDll() -> bool {
    true
}

#[cfg(test)]
#[path = "../../realtime_test_support.rs"]
mod realtime_test_support;

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(C)]
    struct TestEventList {
        vtbl: *const IEventListVtbl,
        event: Event,
    }

    unsafe extern "system" fn event_query_interface(
        _this: *mut c_void,
        _iid: *const TUID,
        obj: *mut *mut c_void,
    ) -> tresult {
        unsafe { *obj = std::ptr::null_mut() };
        kNoInterface
    }

    unsafe extern "system" fn event_add_ref(_this: *mut c_void) -> uint32 {
        1
    }

    unsafe extern "system" fn event_release(_this: *mut c_void) -> uint32 {
        1
    }

    unsafe extern "system" fn event_count(_this: *mut c_void) -> int32 {
        1
    }

    unsafe extern "system" fn event_get(
        this: *mut c_void,
        index: int32,
        event: *mut Event,
    ) -> tresult {
        if index != 0 || event.is_null() {
            return kInvalidArgument;
        }
        unsafe { *event = (*(this as *const TestEventList)).event };
        kResultOk
    }

    unsafe extern "system" fn event_add(_this: *mut c_void, _event: *mut Event) -> tresult {
        kNotImplemented
    }

    static EVENT_LIST_VTBL: IEventListVtbl = IEventListVtbl {
        unknown: IUnknownVtbl {
            query_interface: event_query_interface,
            add_ref: event_add_ref,
            release: event_release,
        },
        get_event_count: event_count,
        get_event: event_get,
        add_event: event_add,
    };

    #[test]
    fn raw_vst3_synth_callback_does_not_allocate() {
        unsafe {
            let processor = SineProcessorObj::new();
            let audio = &mut (*processor).vtbl_audio as *mut _ as *mut c_void;
            (*processor).sample_rate = 48_000.0;

            let mut output_left = [0.0_f32; 16];
            let mut output_right = [0.0_f32; 16];
            let mut output_channels = [
                output_left.as_mut_ptr() as *mut c_void,
                output_right.as_mut_ptr() as *mut c_void,
            ];
            let mut output = AudioBusBuffers {
                num_channels: 2,
                silence_flags: 0,
                buffers: output_channels.as_mut_ptr(),
            };
            let mut events = TestEventList {
                vtbl: &EVENT_LIST_VTBL,
                event: Event {
                    bus_index: 0,
                    sample_offset: 3,
                    ppq_position: 0.0,
                    flags: 0,
                    type_: EventTypes::kNoteOnEvent,
                    event: EventData {
                        note_on: NoteOnEvent {
                            channel: 0,
                            pitch: 69,
                            tuning: 0.0,
                            velocity: 1.0,
                            length: 0,
                            note_id: 1,
                        },
                    },
                },
            };
            let mut data = ProcessData {
                process_mode: 0,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                num_samples: 16,
                num_inputs: 0,
                num_outputs: 1,
                inputs: std::ptr::null_mut(),
                outputs: &mut output,
                input_parameter_changes: std::ptr::null_mut(),
                output_parameter_changes: std::ptr::null_mut(),
                input_events: &mut events as *mut _ as *mut c_void,
                output_events: std::ptr::null_mut(),
                process_context: std::ptr::null_mut(),
            };

            let (status, allocator_calls) =
                realtime_test_support::count_allocator_calls(|| audio_process(audio, &mut data));
            assert_eq!(status, kResultOk);
            assert_eq!(allocator_calls, 0);
            assert_eq!(&output_left[..4], &[0.0; 4]);
            assert_eq!(&output_left, &output_right);
            assert!(output_left[4..].iter().any(|sample| *sample != 0.0));

            component_release(processor as *mut c_void);
        }
    }

    #[test]
    fn gain_state_is_versioned_keyed_and_validated() {
        let bytes = encode_gain_state(0.375).expect("valid gain state");
        assert_eq!(&bytes[..8], &STATE_MAGIC);
        assert_eq!(decode_gain_state(&bytes), Some(0.375));

        let mut wrong_version = bytes;
        wrong_version[8..12].copy_from_slice(&2u32.to_le_bytes());
        assert_eq!(decode_gain_state(&wrong_version), None);

        let mut wrong_id = bytes;
        wrong_id[16..20].copy_from_slice(&99u32.to_le_bytes());
        assert_eq!(decode_gain_state(&wrong_id), None);

        let mut non_finite = bytes;
        non_finite[20..28].copy_from_slice(&f64::NAN.to_le_bytes());
        assert_eq!(decode_gain_state(&non_finite), None);
    }

    #[test]
    fn pending_note_sort_is_stable_for_equal_offsets() {
        let mut notes = [
            PendingNote {
                sample_offset: 17,
                kind: PendingNoteKind::On,
                pitch: 60,
                ..PendingNote::EMPTY
            },
            PendingNote {
                sample_offset: 3,
                kind: PendingNoteKind::On,
                pitch: 61,
                ..PendingNote::EMPTY
            },
            PendingNote {
                sample_offset: 17,
                kind: PendingNoteKind::Off,
                pitch: 62,
                ..PendingNote::EMPTY
            },
        ];

        sort_pending_notes(&mut notes);
        assert_eq!(notes[0].pitch, 61);
        assert_eq!(notes[1].pitch, 60);
        assert_eq!(notes[2].pitch, 62);
    }

    #[test]
    fn pending_param_sort_is_stable_for_same_offset_last_wins() {
        let mut params = [
            PendingParam {
                sample_offset: 31,
                value: 1.0,
            },
            PendingParam {
                sample_offset: 17,
                value: 0.75,
            },
            PendingParam {
                sample_offset: 31,
                value: 0.5,
            },
        ];

        sort_pending_params(&mut params);
        assert_eq!(params[0].sample_offset, 17);
        assert_eq!(params[0].value, 0.75);
        assert_eq!(params[1].value, 1.0);
        assert_eq!(params[2].value, 0.5);
    }

    #[test]
    fn automation_publication_preserves_concurrent_controller_edit() {
        let shared = SharedGain::new(0.25);
        let stale_snapshot = shared.snapshot();
        shared.store(0.8);

        assert!(!shared.store_if_unchanged(stale_snapshot, 0.5));
        assert_eq!(shared.load(), 0.8);

        let current_snapshot = shared.snapshot();
        assert!(shared.store_if_unchanged(current_snapshot, 0.5));
        assert_eq!(shared.load(), 0.5);
    }

    #[test]
    fn standard_connection_points_link_only_their_processor_controller_pair() {
        unsafe {
            let processor = SineProcessorObj::new();
            let controller = SineControllerObj::new();
            let unrelated_processor = SineProcessorObj::new();
            let mut processor_connection = std::ptr::null_mut();
            let mut controller_connection = std::ptr::null_mut();

            assert_eq!(
                component_query_interface(
                    processor as *mut c_void,
                    &vst_iid::IConnectionPoint,
                    &mut processor_connection,
                ),
                kResultOk
            );
            assert_eq!(
                controller_query_interface(
                    controller as *mut c_void,
                    &vst_iid::IConnectionPoint,
                    &mut controller_connection,
                ),
                kResultOk
            );

            let processor_vtbl = *(processor_connection as *const *const IConnectionPointVtbl);
            let controller_vtbl = *(controller_connection as *const *const IConnectionPointVtbl);
            assert_eq!(
                ((*processor_vtbl).connect)(processor_connection, controller_connection),
                kResultOk
            );
            assert_eq!(
                ((*controller_vtbl).connect)(controller_connection, processor_connection),
                kResultOk
            );

            assert_eq!(
                controller_set_param_normalized(controller as *mut c_void, PARAM_GAIN, 0.75),
                kResultOk
            );
            assert_eq!((*processor).shared_gain.load(), 0.75);
            assert_eq!((*unrelated_processor).shared_gain.load(), 0.5);

            ((*processor_vtbl).unknown.release)(processor_connection);
            ((*controller_vtbl).unknown.release)(controller_connection);
            component_release(processor as *mut c_void);
            controller_release(controller as *mut c_void);
            component_release(unrelated_processor as *mut c_void);
        }
    }
}
