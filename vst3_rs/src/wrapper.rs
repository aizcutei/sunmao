//! Internal wrapper module for COM vtable generation
//!
//! This module contains the unsafe FFI glue that converts Plugin trait
//! implementations into VST3 COM interfaces.

use crate::plugin::RenderMode;
use crate::process::{MAX_PROCESS_EVENTS, MAX_PROCESS_FRAMES};
use crate::state::{load_parameter_state, save_parameter_state};
use crate::{HostHandle, ParamInfo, ParameterBridge, Plugin, ProcessContext, ProcessError};
use std::ffi::c_void;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};
use vst3_sys::*;

/// Bound host-controlled parameter queue traversal on the realtime callback.
///
/// VST3 reports the number of changed parameter queues as an `int32`. A host
/// can therefore otherwise make a malformed callback spend a very long time
/// repeatedly calling `getParameterData`, even when the plugin has only a
/// handful of parameters. The limit is deliberately independent from a
/// plugin's event capacity because zero-point/unknown queues do not consume
/// that capacity but still cost callback time.
const MAX_PARAMETER_QUEUES: usize = 4096;

/// Keep Rust panics on the Rust side of a VST3 COM ABI callback.
///
/// VST3 hosts call these functions through raw vtables.  Letting a panic
/// unwind through that boundary is never valid, so callbacks translate a
/// panic into the error value appropriate for their return type.
#[inline]
fn ffi_guard<T>(fallback: T, callback: impl FnOnce() -> T) -> T {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(callback)).unwrap_or(fallback)
}

const PARAMETER_CONNECTION_IID: TUID =
    vst3_sys::uid!(0x53554E4D, 0x414F5042, 0x52494447, 0x45000001);
const CONNECTION_ROLE_PROCESSOR: u32 = 1;
const CONNECTION_ROLE_CONTROLLER: u32 = 2;

#[doc(hidden)]
pub unsafe fn factory_query_interface(
    instance: *mut c_void,
    requested_iid: *const TUID,
    object: *mut *mut c_void,
) -> tresult {
    ffi_guard(kInternalError, || unsafe {
        factory_query_interface_unchecked(instance, requested_iid, object)
    })
}

unsafe fn factory_query_interface_unchecked(
    instance: *mut c_void,
    requested_iid: *const TUID,
    object: *mut *mut c_void,
) -> tresult {
    if instance.is_null() || requested_iid.is_null() || object.is_null() {
        return kInvalidArgument;
    }

    unsafe { *object = std::ptr::null_mut() };
    let unknown = unsafe { *(instance as *const *const IUnknownVtbl) };
    if unknown.is_null() {
        return kNoInterface;
    }

    let result = unsafe { ((*unknown).query_interface)(instance, requested_iid, object) };
    unsafe { ((*unknown).release)(instance) };
    if result == kResultOk && unsafe { !(*object).is_null() } {
        kResultOk
    } else {
        unsafe { *object = std::ptr::null_mut() };
        if result == kResultOk {
            kNoInterface
        } else {
            result
        }
    }
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
    get_parameter_bridge: unsafe extern "system" fn(*mut c_void) -> *const ParameterBridge,
    adopt_parameter_bridge:
        unsafe extern "system" fn(*mut c_void, *const ParameterBridge) -> tresult,
}

unsafe fn connect_parameter_bridges(this: *mut c_void, other: *mut c_void) -> tresult {
    if this.is_null() || other.is_null() {
        return kInvalidArgument;
    }

    let other_unknown = *(other as *const *const IUnknownVtbl);
    if other_unknown.is_null() {
        return kNoInterface;
    }
    let mut peer = std::ptr::null_mut();
    let result = ((*other_unknown).query_interface)(other, &PARAMETER_CONNECTION_IID, &mut peer);
    if result != kResultOk || peer.is_null() {
        return kNoInterface;
    }

    let this_vtbl = *(this as *const *const ConnectionPointVtbl);
    let peer_vtbl = *(peer as *const *const ConnectionPointVtbl);
    if this_vtbl.is_null() || peer_vtbl.is_null() {
        if !peer_vtbl.is_null() {
            ((*peer_vtbl).release)(peer);
        }
        return kNoInterface;
    }
    let this_role = ((*this_vtbl).get_role)(this);
    let peer_role = ((*peer_vtbl).get_role)(peer);

    let result = match (this_role, peer_role) {
        (CONNECTION_ROLE_CONTROLLER, CONNECTION_ROLE_PROCESSOR) => {
            let bridge = ((*peer_vtbl).get_parameter_bridge)(peer);
            ((*this_vtbl).adopt_parameter_bridge)(this, bridge)
        }
        (CONNECTION_ROLE_PROCESSOR, CONNECTION_ROLE_CONTROLLER) => {
            let bridge = ((*this_vtbl).get_parameter_bridge)(this);
            ((*peer_vtbl).adopt_parameter_bridge)(peer, bridge)
        }
        _ => kInvalidArgument,
    };

    ((*peer_vtbl).release)(peer);
    result
}

unsafe fn disconnect_parameter_bridges(this: *mut c_void, other: *mut c_void) -> tresult {
    if this.is_null() || other.is_null() {
        return kInvalidArgument;
    }

    let other_unknown = *(other as *const *const IUnknownVtbl);
    if other_unknown.is_null() {
        return kNoInterface;
    }
    let mut peer = std::ptr::null_mut();
    let result = ((*other_unknown).query_interface)(other, &PARAMETER_CONNECTION_IID, &mut peer);
    if result != kResultOk || peer.is_null() {
        return kNoInterface;
    }

    let this_vtbl = *(this as *const *const ConnectionPointVtbl);
    let peer_vtbl = *(peer as *const *const ConnectionPointVtbl);
    if this_vtbl.is_null() || peer_vtbl.is_null() {
        if !peer_vtbl.is_null() {
            ((*peer_vtbl).release)(peer);
        }
        return kNoInterface;
    }
    let this_role = ((*this_vtbl).get_role)(this);
    let peer_role = ((*peer_vtbl).get_role)(peer);

    let (controller, processor) = match (this_role, peer_role) {
        (CONNECTION_ROLE_CONTROLLER, CONNECTION_ROLE_PROCESSOR) => (this, peer),
        (CONNECTION_ROLE_PROCESSOR, CONNECTION_ROLE_CONTROLLER) => (peer, this),
        _ => {
            ((*peer_vtbl).release)(peer);
            return kInvalidArgument;
        }
    };
    let controller_vtbl = *(controller as *const *const ConnectionPointVtbl);
    let processor_vtbl = *(processor as *const *const ConnectionPointVtbl);
    let controller_bridge = ((*controller_vtbl).get_parameter_bridge)(controller);
    let processor_bridge = ((*processor_vtbl).get_parameter_bridge)(processor);
    let result = if controller_bridge.is_null() || processor_bridge.is_null() {
        kInvalidArgument
    } else if (*controller_bridge).disconnect_from(&*processor_bridge) {
        kResultOk
    } else {
        kResultFalse
    };

    ((*peer_vtbl).release)(peer);
    result
}

fn default_speaker_arrangement(channels: u32) -> Option<SpeakerArrangement> {
    match channels {
        0 => Some(SpeakerArr::kEmpty),
        1 => Some(SpeakerArr::kMono),
        2 => Some(SpeakerArr::kStereo),
        _ => None,
    }
}

fn speaker_arrangement(port: &crate::PortConfig) -> Option<SpeakerArrangement> {
    let arrangement = port
        .speaker_arrangement
        .or_else(|| default_speaker_arrangement(port.channels))?;
    (arrangement.count_ones() == port.channels).then_some(arrangement)
}

fn event_sample_offset(offset: int32, num_samples: int32) -> u32 {
    if num_samples <= 0 {
        0
    } else {
        offset.clamp(0, num_samples - 1) as u32
    }
}

#[inline]
fn sanitize_normalized(value: ParamValue, fallback: ParamValue) -> ParamValue {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        fallback
    }
}

#[inline]
fn sanitize_plain(value: ParamValue, fallback: ParamValue) -> ParamValue {
    if value.is_finite() { value } else { fallback }
}

#[inline]
fn safe_plain_value(param: &ParamInfo, normalized: ParamValue) -> ParamValue {
    let plain = param.to_plain(normalized);
    if plain.is_finite() {
        plain
    } else if param.min.is_finite() {
        param.min
    } else {
        0.0
    }
}

fn valid_note_event(event: &Event, accepts_midi: bool) -> bool {
    if !accepts_midi || event.bus_index != 0 {
        return false;
    }

    match event.type_ {
        EventTypes::kNoteOnEvent => {
            let note = unsafe { event.event.note_on };
            (0..=15).contains(&note.channel)
                && (0..=127).contains(&note.pitch)
                && note.velocity.is_finite()
                && (0.0..=1.0).contains(&note.velocity)
        }
        EventTypes::kNoteOffEvent => {
            let note = unsafe { event.event.note_off };
            (0..=15).contains(&note.channel)
                && (0..=127).contains(&note.pitch)
                && note.velocity.is_finite()
                && (0.0..=1.0).contains(&note.velocity)
        }
        _ => true,
    }
}

/// Internal processor wrapper
#[repr(C)]
pub struct ProcessorWrapper<P: Plugin> {
    vtbl_component: *const ComponentVtbl,
    vtbl_audio: *const AudioProcessorVtbl,
    vtbl_connection: *const ConnectionPointVtbl,
    ref_count: AtomicI32,
    controller_cid: TUID,
    plugin: Option<P>,
    initialized: bool,
    active: bool,
    processing: bool,
    accepts_midi: bool,
    sample_rate: f64,
    max_frames: u32,
    process_ctx: Option<ProcessContext>,
    params: Vec<ParamInfo>,
    final_parameter_values: Vec<Option<f64>>,
    parameter_bridge: Arc<ParameterBridge>,
    parameter_generation: u64,
    // Keep the immutable layout here so process() can bound and validate host
    // bus data without rebuilding AudioConfig on the realtime thread.
    input_bus_channels: Box<[u32]>,
    output_bus_channels: Box<[u32]>,
    _component_vtbl_storage: Box<ComponentVtbl>,
    _audio_vtbl_storage: Box<AudioProcessorVtbl>,
    _connection_vtbl_storage: Box<ConnectionPointVtbl>,
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

impl<P: Plugin> ProcessorWrapper<P> {
    pub fn new(controller_cid: TUID) -> *mut Self {
        ffi_guard(std::ptr::null_mut(), || Self::new_unchecked(controller_cid))
    }

    fn new_unchecked(controller_cid: TUID) -> *mut Self {
        let component_vtbl_storage = Box::new(Self::make_component_vtbl());
        let audio_vtbl_storage = Box::new(Self::make_audio_vtbl());
        let connection_vtbl_storage = Box::new(Self::make_connection_vtbl());
        let vtbl_component = &*component_vtbl_storage;
        let vtbl_audio = &*audio_vtbl_storage;
        let vtbl_connection = &*connection_vtbl_storage;

        let params = P::params();
        let audio_config = P::audio_config();
        let input_bus_channels = audio_config
            .inputs
            .iter()
            .map(|port| port.channels)
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let output_bus_channels = audio_config
            .outputs
            .iter()
            .map(|port| port.channels)
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let parameter_bridge = Arc::new(ParameterBridge::new(&params));
        let host = HostHandle::new(parameter_bridge.clone());
        let plugin = P::new(host);

        let wrapper = Box::new(Self {
            vtbl_component,
            vtbl_audio,
            vtbl_connection,
            ref_count: AtomicI32::new(1),
            controller_cid,
            plugin: Some(plugin),
            initialized: false,
            active: false,
            processing: false,
            accepts_midi: audio_config.accepts_midi,
            sample_rate: 44100.0,
            max_frames: 1024,
            process_ctx: None,
            final_parameter_values: vec![None; params.len()],
            params,
            parameter_bridge,
            parameter_generation: 0,
            input_bus_channels,
            output_bus_channels,
            _component_vtbl_storage: component_vtbl_storage,
            _audio_vtbl_storage: audio_vtbl_storage,
            _connection_vtbl_storage: connection_vtbl_storage,
        });
        Box::into_raw(wrapper)
    }

    fn make_component_vtbl() -> ComponentVtbl {
        ComponentVtbl {
            query_interface: Self::component_query_interface,
            add_ref: Self::component_add_ref,
            release: Self::component_release,
            initialize: Self::processor_initialize,
            terminate: Self::processor_terminate,
            get_controller_class_id: Self::processor_get_controller_class_id,
            set_io_mode: Self::processor_set_io_mode,
            get_bus_count: Self::processor_get_bus_count,
            get_bus_info: Self::processor_get_bus_info,
            get_routing_info: Self::processor_get_routing_info,
            activate_bus: Self::processor_activate_bus,
            set_active: Self::processor_set_active,
            set_state: Self::processor_set_state,
            get_state: Self::processor_get_state,
        }
    }

    fn make_audio_vtbl() -> AudioProcessorVtbl {
        AudioProcessorVtbl {
            query_interface: Self::audio_query_interface,
            add_ref: Self::audio_add_ref,
            release: Self::audio_release,
            set_bus_arrangements: Self::audio_set_bus_arrangements,
            get_bus_arrangement: Self::audio_get_bus_arrangement,
            can_process_sample_size: Self::audio_can_process_sample_size,
            get_latency_samples: Self::audio_get_latency_samples,
            setup_processing: Self::audio_setup_processing,
            set_processing: Self::audio_set_processing,
            process: Self::audio_process,
            get_tail_samples: Self::audio_get_tail_samples,
        }
    }

    fn make_connection_vtbl() -> ConnectionPointVtbl {
        ConnectionPointVtbl {
            query_interface: Self::connection_query_interface,
            add_ref: Self::connection_add_ref,
            release: Self::connection_release,
            connect: Self::connection_connect,
            disconnect: Self::connection_disconnect,
            notify: Self::connection_notify,
            get_role: Self::connection_get_role,
            get_parameter_bridge: Self::connection_get_parameter_bridge,
            adopt_parameter_bridge: Self::connection_adopt_parameter_bridge,
        }
    }

    unsafe fn from_component(this: *mut c_void) -> *mut Self {
        this as *mut Self
    }
    unsafe fn from_audio(this: *mut c_void) -> *mut Self {
        (this as *mut u8).sub(std::mem::size_of::<*const c_void>()) as *mut Self
    }
    unsafe fn from_connection(this: *mut c_void) -> *mut Self {
        (this as *mut u8).sub(2 * std::mem::size_of::<*const c_void>()) as *mut Self
    }

    // Component interface
    unsafe extern "system" fn component_query_interface(
        this: *mut c_void,
        iid: *const TUID,
        obj: *mut *mut c_void,
    ) -> tresult {
        if this.is_null() || iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        *obj = std::ptr::null_mut();
        let iid = &*iid;
        let base = Self::from_component(this);
        if iid_equal(iid, &iid::IUnknown)
            || iid_equal(iid, &base_iid::IPluginBase)
            || iid_equal(iid, &vst_iid::IComponent)
        {
            Self::component_add_ref(this);
            *obj = this;
            return kResultOk;
        }
        if iid_equal(iid, &vst_iid::IAudioProcessor) {
            Self::component_add_ref(this);
            *obj = &(*base).vtbl_audio as *const _ as *mut c_void;
            return kResultOk;
        }
        if iid_equal(iid, &vst_iid::IConnectionPoint) {
            Self::component_add_ref(this);
            *obj = &(*base).vtbl_connection as *const _ as *mut c_void;
            return kResultOk;
        }
        *obj = std::ptr::null_mut();
        kNoInterface
    }

    unsafe extern "system" fn component_add_ref(this: *mut c_void) -> uint32 {
        if this.is_null() {
            return 0;
        }
        let obj = Self::from_component(this);
        (*obj).ref_count.fetch_add(1, Ordering::SeqCst) as uint32 + 1
    }

    unsafe extern "system" fn component_release(this: *mut c_void) -> uint32 {
        ffi_guard(0, || unsafe { Self::component_release_unchecked(this) })
    }

    unsafe fn component_release_unchecked(this: *mut c_void) -> uint32 {
        if this.is_null() {
            return 0;
        }
        let obj = Self::from_component(this);
        let count = (*obj).ref_count.fetch_sub(1, Ordering::SeqCst) - 1;
        if count == 0 {
            let _ = Box::from_raw(obj);
        }
        count as uint32
    }

    // Audio interface
    unsafe extern "system" fn audio_query_interface(
        this: *mut c_void,
        iid: *const TUID,
        obj: *mut *mut c_void,
    ) -> tresult {
        if this.is_null() || iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        *obj = std::ptr::null_mut();
        let iid = &*iid;
        let base = Self::from_audio(this);
        if iid_equal(iid, &vst_iid::IAudioProcessor) {
            Self::audio_add_ref(this);
            *obj = this;
            return kResultOk;
        }
        if iid_equal(iid, &iid::IUnknown)
            || iid_equal(iid, &base_iid::IPluginBase)
            || iid_equal(iid, &vst_iid::IComponent)
        {
            Self::audio_add_ref(this);
            *obj = base as *mut c_void;
            return kResultOk;
        }
        if iid_equal(iid, &vst_iid::IConnectionPoint) {
            Self::audio_add_ref(this);
            *obj = &(*base).vtbl_connection as *const _ as *mut c_void;
            return kResultOk;
        }
        *obj = std::ptr::null_mut();
        kNoInterface
    }

    unsafe extern "system" fn audio_add_ref(this: *mut c_void) -> uint32 {
        if this.is_null() {
            return 0;
        }
        let obj = Self::from_audio(this);
        (*obj).ref_count.fetch_add(1, Ordering::SeqCst) as uint32 + 1
    }

    unsafe extern "system" fn audio_release(this: *mut c_void) -> uint32 {
        ffi_guard(0, || unsafe { Self::audio_release_unchecked(this) })
    }

    unsafe fn audio_release_unchecked(this: *mut c_void) -> uint32 {
        if this.is_null() {
            return 0;
        }
        let obj = Self::from_audio(this);
        let count = (*obj).ref_count.fetch_sub(1, Ordering::SeqCst) - 1;
        if count == 0 {
            let _ = Box::from_raw(obj);
        }
        count as uint32
    }

    unsafe extern "system" fn connection_query_interface(
        this: *mut c_void,
        requested_iid: *const TUID,
        obj: *mut *mut c_void,
    ) -> tresult {
        if this.is_null() || requested_iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        *obj = std::ptr::null_mut();
        let base = Self::from_connection(this);
        if iid_equal(&*requested_iid, &PARAMETER_CONNECTION_IID)
            || iid_equal(&*requested_iid, &vst_iid::IConnectionPoint)
        {
            Self::connection_add_ref(this);
            *obj = this;
            return kResultOk;
        }
        Self::component_query_interface(base as *mut c_void, requested_iid, obj)
    }

    unsafe extern "system" fn connection_add_ref(this: *mut c_void) -> uint32 {
        if this.is_null() {
            return 0;
        }
        let base = Self::from_connection(this);
        Self::component_add_ref(base as *mut c_void)
    }

    unsafe extern "system" fn connection_release(this: *mut c_void) -> uint32 {
        if this.is_null() {
            return 0;
        }
        let base = Self::from_connection(this);
        Self::component_release(base as *mut c_void)
    }

    unsafe extern "system" fn connection_connect(this: *mut c_void, other: *mut c_void) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            connect_parameter_bridges(this, other)
        })
    }

    unsafe extern "system" fn connection_disconnect(
        this: *mut c_void,
        other: *mut c_void,
    ) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            disconnect_parameter_bridges(this, other)
        })
    }

    unsafe extern "system" fn connection_notify(
        _this: *mut c_void,
        _message: *mut c_void,
    ) -> tresult {
        kResultOk
    }

    unsafe extern "system" fn connection_get_role(_this: *mut c_void) -> u32 {
        CONNECTION_ROLE_PROCESSOR
    }

    unsafe extern "system" fn connection_get_parameter_bridge(
        this: *mut c_void,
    ) -> *const ParameterBridge {
        if this.is_null() {
            return std::ptr::null();
        }
        let base = Self::from_connection(this);
        Arc::as_ptr(&(*base).parameter_bridge)
    }

    unsafe extern "system" fn connection_adopt_parameter_bridge(
        _this: *mut c_void,
        _bridge: *const ParameterBridge,
    ) -> tresult {
        kInvalidArgument
    }

    // IComponent methods
    unsafe extern "system" fn processor_initialize(
        this: *mut c_void,
        context: *mut c_void,
    ) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            Self::processor_initialize_unchecked(this, context)
        })
    }

    unsafe fn processor_initialize_unchecked(this: *mut c_void, _context: *mut c_void) -> tresult {
        if this.is_null() {
            return kInvalidArgument;
        }
        let obj = Self::from_component(this);
        if (*obj).initialized {
            return kResultOk;
        }
        if let Some(plugin) = (*obj).plugin.as_mut() {
            if plugin.init() {
                (*obj).initialized = true;
                kResultOk
            } else {
                kResultFalse
            }
        } else {
            kResultFalse
        }
    }

    unsafe extern "system" fn processor_terminate(this: *mut c_void) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            Self::processor_terminate_unchecked(this)
        })
    }

    unsafe fn processor_terminate_unchecked(this: *mut c_void) -> tresult {
        if this.is_null() {
            return kInvalidArgument;
        }
        let obj = Self::from_component(this);
        (*obj).initialized = false;
        (*obj).processing = false;
        (*obj).process_ctx = None;
        if (*obj).active {
            // Publish the inactive state before entering user code. If the
            // plugin panics, the guarded ABI callback still leaves the
            // wrapper in a state that can be safely terminated again.
            (*obj).active = false;
            if let Some(plugin) = (*obj).plugin.as_mut() {
                plugin.deactivate();
            }
        }
        kResultOk
    }

    unsafe extern "system" fn processor_get_controller_class_id(
        this: *mut c_void,
        class_id: *mut TUID,
    ) -> tresult {
        if this.is_null() || class_id.is_null() {
            return kInvalidArgument;
        }
        let obj = Self::from_component(this);
        *class_id = (*obj).controller_cid;
        kResultOk
    }

    unsafe extern "system" fn processor_set_io_mode(_this: *mut c_void, _mode: IoMode) -> tresult {
        kResultOk
    }

    unsafe extern "system" fn processor_get_bus_count(
        this: *mut c_void,
        media_type: MediaType,
        dir: BusDirection,
    ) -> int32 {
        ffi_guard(0, || unsafe {
            Self::processor_get_bus_count_unchecked(this, media_type, dir)
        })
    }

    unsafe fn processor_get_bus_count_unchecked(
        this: *mut c_void,
        media_type: MediaType,
        dir: BusDirection,
    ) -> int32 {
        if this.is_null() {
            return 0;
        }
        let config = P::audio_config();
        if media_type == MediaTypes::kAudio {
            if dir == BusDirections::kInput {
                config.inputs.len() as int32
            } else if dir == BusDirections::kOutput {
                config.outputs.len() as int32
            } else {
                0
            }
        } else if media_type == MediaTypes::kEvent
            && config.accepts_midi
            && dir == BusDirections::kInput
        {
            1
        } else {
            0
        }
    }

    unsafe extern "system" fn processor_get_bus_info(
        this: *mut c_void,
        media_type: MediaType,
        dir: BusDirection,
        index: int32,
        bus: *mut BusInfo,
    ) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            Self::processor_get_bus_info_unchecked(this, media_type, dir, index, bus)
        })
    }

    unsafe fn processor_get_bus_info_unchecked(
        this: *mut c_void,
        media_type: MediaType,
        dir: BusDirection,
        index: int32,
        bus: *mut BusInfo,
    ) -> tresult {
        if this.is_null() || bus.is_null() || index < 0 {
            return kInvalidArgument;
        }
        let config = P::audio_config();
        let bus = &mut *bus;

        if media_type == MediaTypes::kAudio {
            let ports = if dir == BusDirections::kInput {
                &config.inputs
            } else if dir == BusDirections::kOutput {
                &config.outputs
            } else {
                return kInvalidArgument;
            };
            if let Some(port) = ports.get(index as usize) {
                bus.media_type = MediaTypes::kAudio;
                bus.direction = dir;
                let Ok(channel_count) = int32::try_from(port.channels) else {
                    return kInvalidArgument;
                };
                bus.channel_count = channel_count;
                bus.bus_type = match port.port_type {
                    crate::PortType::Main => BusTypes::kMain,
                    crate::PortType::Aux => BusTypes::kAux,
                };
                bus.flags = BusFlags::kDefaultActive;
                str16cpy_safe(&mut bus.name, port.name);
                return kResultOk;
            }
        } else if media_type == MediaTypes::kEvent
            && config.accepts_midi
            && dir == BusDirections::kInput
            && index == 0
        {
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
        this: *mut c_void,
        media_type: MediaType,
        dir: BusDirection,
        index: int32,
        state: TBool,
    ) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            Self::processor_activate_bus_unchecked(this, media_type, dir, index, state)
        })
    }

    unsafe fn processor_activate_bus_unchecked(
        this: *mut c_void,
        media_type: MediaType,
        dir: BusDirection,
        index: int32,
        state: TBool,
    ) -> tresult {
        if this.is_null() || index < 0 {
            return kInvalidArgument;
        }
        let config = P::audio_config();
        if media_type == MediaTypes::kAudio {
            let (is_input, bus_count) = if dir == BusDirections::kInput {
                (true, config.inputs.len())
            } else if dir == BusDirections::kOutput {
                (false, config.outputs.len())
            } else {
                return kInvalidArgument;
            };
            if index as usize >= bus_count {
                return kInvalidArgument;
            }
            let obj = Self::from_component(this);
            if !(*obj).initialized {
                return kNotInitialized;
            }
            let Some(plugin) = (*obj).plugin.as_mut() else {
                return kNotInitialized;
            };
            return if plugin.activate_bus(is_input, index as u32, state != 0) {
                kResultOk
            } else {
                kResultFalse
            };
        }
        // The single event bus has no plugin-side state to toggle; accept the
        // request so hosts that deactivate MIDI while idle keep working.
        if media_type == MediaTypes::kEvent
            && config.accepts_midi
            && dir == BusDirections::kInput
            && index == 0
        {
            return kResultOk;
        }
        kInvalidArgument
    }

    unsafe extern "system" fn processor_set_active(this: *mut c_void, state: TBool) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            Self::processor_set_active_unchecked(this, state)
        })
    }

    unsafe fn processor_set_active_unchecked(this: *mut c_void, state: TBool) -> tresult {
        if this.is_null() {
            return kInvalidArgument;
        }
        let obj = Self::from_component(this);
        if !(*obj).initialized {
            return kNotInitialized;
        }
        if let Some(plugin) = (*obj).plugin.as_mut() {
            if state != 0 {
                if (*obj).active {
                    return kResultOk;
                }
                if (*obj).process_ctx.is_none() {
                    return kNotInitialized;
                }
                if plugin.activate((*obj).sample_rate, (*obj).max_frames) {
                    (*obj).active = true;
                    kResultOk
                } else {
                    kResultFalse
                }
            } else {
                if !(*obj).active {
                    return kResultOk;
                }
                (*obj).active = false;
                (*obj).processing = false;
                plugin.deactivate();
                kResultOk
            }
        } else {
            kResultFalse
        }
    }

    unsafe extern "system" fn processor_set_state(
        this: *mut c_void,
        state: *mut c_void,
    ) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            Self::processor_set_state_unchecked(this, state)
        })
    }

    unsafe fn processor_set_state_unchecked(this: *mut c_void, state: *mut c_void) -> tresult {
        if this.is_null() {
            return kInvalidArgument;
        }
        let obj = Self::from_component(this);
        load_parameter_state(state, &(*obj).params, |id, value| {
            (*obj).parameter_bridge.set(id, value);
        })
    }
    unsafe extern "system" fn processor_get_state(
        this: *mut c_void,
        state: *mut c_void,
    ) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            Self::processor_get_state_unchecked(this, state)
        })
    }

    unsafe fn processor_get_state_unchecked(this: *mut c_void, state: *mut c_void) -> tresult {
        if this.is_null() {
            return kInvalidArgument;
        }
        let obj = Self::from_component(this);
        save_parameter_state(state, &(*obj).params, |id| (*obj).parameter_bridge.get(id))
    }

    // IAudioProcessor methods
    unsafe extern "system" fn audio_set_bus_arrangements(
        this: *mut c_void,
        inputs: *mut SpeakerArrangement,
        num_ins: int32,
        outputs: *mut SpeakerArrangement,
        num_outs: int32,
    ) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            Self::audio_set_bus_arrangements_unchecked(this, inputs, num_ins, outputs, num_outs)
        })
    }

    unsafe fn audio_set_bus_arrangements_unchecked(
        this: *mut c_void,
        inputs: *mut SpeakerArrangement,
        num_ins: int32,
        outputs: *mut SpeakerArrangement,
        num_outs: int32,
    ) -> tresult {
        if this.is_null() {
            return kInvalidArgument;
        }
        let config = P::audio_config();
        if num_ins < 0
            || num_outs < 0
            || num_ins as usize != config.inputs.len()
            || num_outs as usize != config.outputs.len()
            || (num_ins > 0 && inputs.is_null())
            || (num_outs > 0 && outputs.is_null())
        {
            return kResultFalse;
        }

        let input_arrangements: &[SpeakerArrangement] = if num_ins == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(inputs, num_ins as usize)
        };
        let output_arrangements: &[SpeakerArrangement] = if num_outs == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(outputs, num_outs as usize)
        };
        let inputs_match = config
            .inputs
            .iter()
            .zip(input_arrangements)
            .all(|(port, &actual)| {
                speaker_arrangement(port).is_some_and(|expected| expected == actual)
            });
        let outputs_match =
            config
                .outputs
                .iter()
                .zip(output_arrangements)
                .all(|(port, &actual)| {
                    speaker_arrangement(port).is_some_and(|expected| expected == actual)
                });
        if inputs_match && outputs_match {
            kResultOk
        } else {
            kResultFalse
        }
    }

    unsafe extern "system" fn audio_get_bus_arrangement(
        this: *mut c_void,
        dir: BusDirection,
        index: int32,
        arr: *mut SpeakerArrangement,
    ) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            Self::audio_get_bus_arrangement_unchecked(this, dir, index, arr)
        })
    }

    unsafe fn audio_get_bus_arrangement_unchecked(
        this: *mut c_void,
        dir: BusDirection,
        index: int32,
        arr: *mut SpeakerArrangement,
    ) -> tresult {
        if this.is_null() || arr.is_null() || index < 0 {
            return kInvalidArgument;
        }
        let config = P::audio_config();
        let ports = if dir == BusDirections::kInput {
            &config.inputs
        } else if dir == BusDirections::kOutput {
            &config.outputs
        } else {
            return kInvalidArgument;
        };
        let Some(port) = ports.get(index as usize) else {
            return kInvalidArgument;
        };
        let Some(arrangement) = speaker_arrangement(port) else {
            return kResultFalse;
        };
        *arr = arrangement;
        kResultOk
    }

    unsafe extern "system" fn audio_can_process_sample_size(
        this: *mut c_void,
        symbolic_sample_size: int32,
    ) -> tresult {
        if this.is_null() {
            return kInvalidArgument;
        }
        if symbolic_sample_size == SymbolicSampleSizes::kSample32 {
            kResultOk
        } else {
            kResultFalse
        }
    }

    unsafe extern "system" fn audio_get_latency_samples(this: *mut c_void) -> uint32 {
        ffi_guard(0, || unsafe {
            Self::audio_get_latency_samples_unchecked(this)
        })
    }

    unsafe fn audio_get_latency_samples_unchecked(this: *mut c_void) -> uint32 {
        if this.is_null() {
            return 0;
        }
        let obj = Self::from_audio(this);
        (*obj).plugin.as_ref().map(|p| p.latency()).unwrap_or(0)
    }

    unsafe extern "system" fn audio_setup_processing(
        this: *mut c_void,
        setup: *mut ProcessSetup,
    ) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            Self::audio_setup_processing_unchecked(this, setup)
        })
    }

    unsafe fn audio_setup_processing_unchecked(
        this: *mut c_void,
        setup: *mut ProcessSetup,
    ) -> tresult {
        if this.is_null() || setup.is_null() {
            return kInvalidArgument;
        }
        let setup = &*setup;
        if setup.symbolic_sample_size != SymbolicSampleSizes::kSample32 {
            return kResultFalse;
        }
        if setup.max_samples_per_block < 0
            || !setup.sample_rate.is_finite()
            || setup.sample_rate <= 0.0
        {
            return kInvalidArgument;
        }
        if setup.max_samples_per_block as u32 > MAX_PROCESS_FRAMES {
            return kOutOfMemory;
        }
        if P::MAX_EVENTS_PER_BLOCK > MAX_PROCESS_EVENTS {
            return kOutOfMemory;
        }

        let obj = Self::from_audio(this);
        if (*obj).active {
            // VST3 does not permit changing the processing setup while the
            // component is active. The host must deactivate before renegotiating.
            return kResultFalse;
        }
        let sample_rate = setup.sample_rate;
        let max_frames = setup.max_samples_per_block as u32;

        // Create process context
        let config = P::audio_config();
        let Some(num_in) = config.inputs.iter().try_fold(0usize, |total, port| {
            total.checked_add(port.channels as usize)
        }) else {
            return kInvalidArgument;
        };
        let Some(num_out) = config.outputs.iter().try_fold(0usize, |total, port| {
            total.checked_add(port.channels as usize)
        }) else {
            return kInvalidArgument;
        };
        let Some(process_ctx) = ProcessContext::try_new(
            max_frames as usize,
            sample_rate,
            num_in,
            num_out,
            P::MAX_EVENTS_PER_BLOCK,
        ) else {
            return kOutOfMemory;
        };
        (*obj).sample_rate = sample_rate;
        (*obj).max_frames = max_frames;
        (*obj).process_ctx = Some(process_ctx);
        // The component is inactive here, so the plugin may still change the
        // latency it reports in response to the negotiated render mode.
        if let Some(plugin) = (*obj).plugin.as_mut() {
            plugin.set_render_mode(RenderMode::from_process_mode(setup.process_mode));
        }

        kResultOk
    }

    unsafe extern "system" fn audio_set_processing(this: *mut c_void, state: TBool) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            Self::audio_set_processing_unchecked(this, state)
        })
    }

    unsafe fn audio_set_processing_unchecked(this: *mut c_void, state: TBool) -> tresult {
        if this.is_null() {
            return kInvalidArgument;
        }
        let obj = Self::from_audio(this);
        if state != 0 {
            if !(*obj).initialized || !(*obj).active || (*obj).process_ctx.is_none() {
                return kNotInitialized;
            }
            (*obj).processing = true;
        } else if (*obj).processing {
            (*obj).processing = false;
            if let Some(plugin) = (*obj).plugin.as_mut() {
                plugin.reset();
            }
        }
        kResultOk
    }

    unsafe extern "system" fn audio_process(this: *mut c_void, data: *mut ProcessData) -> tresult {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            Self::audio_process_unchecked(this, data)
        })) {
            Ok(result) => result,
            Err(_) => {
                // A process panic leaves the user processor in an unknown
                // state. Stop accepting further realtime blocks until the
                // host performs the normal stop/deactivate transition. This
                // also makes the callback's failure recoverable instead of
                // repeatedly invoking a poisoned processor.
                if !this.is_null() {
                    let obj = unsafe { Self::from_audio(this) };
                    unsafe {
                        (*obj).processing = false;
                    }
                }
                kInternalError
            }
        }
    }

    unsafe fn audio_process_unchecked(this: *mut c_void, data: *mut ProcessData) -> tresult {
        if this.is_null() || data.is_null() {
            return kInvalidArgument;
        }
        let obj = Self::from_audio(this);
        if !(*obj).initialized || (*obj).process_ctx.is_none() {
            return kNotInitialized;
        }
        if !(*obj).active || !(*obj).processing {
            return kResultFalse;
        }
        let data_ref = &*data;
        if data_ref.num_samples < 0 || data_ref.num_inputs < 0 || data_ref.num_outputs < 0 {
            return kInvalidArgument;
        }
        if data_ref.num_samples as u32 > (*obj).max_frames {
            return kInvalidArgument;
        }
        if data_ref.symbolic_sample_size != SymbolicSampleSizes::kSample32 {
            return kResultFalse;
        }

        let num_inputs = data_ref.num_inputs as usize;
        let num_outputs = data_ref.num_outputs as usize;
        let input_bus_channels = &(*obj).input_bus_channels;
        let output_bus_channels = &(*obj).output_bus_channels;
        if num_inputs > input_bus_channels.len()
            || num_outputs > output_bus_channels.len()
            || (num_inputs > 0 && data_ref.inputs.is_null())
            || (num_outputs > 0 && data_ref.outputs.is_null())
        {
            return kInvalidArgument;
        }
        let inputs: &[AudioBusBuffers] = if num_inputs == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(data_ref.inputs, num_inputs)
        };
        let outputs: &[AudioBusBuffers] = if num_outputs == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(data_ref.outputs, num_outputs)
        };
        for (bus, &expected_channels) in inputs.iter().zip(input_bus_channels.iter()) {
            let channels_match = bus.num_channels as u32 == expected_channels
                || (data_ref.num_samples == 0 && bus.num_channels == 0);
            if bus.num_channels < 0
                || !channels_match
                || (bus.num_channels > 0 && bus.buffers.is_null())
            {
                return kInvalidArgument;
            }
        }
        for (bus, &expected_channels) in outputs.iter().zip(output_bus_channels.iter()) {
            let channels_match = bus.num_channels as u32 == expected_channels
                || (data_ref.num_samples == 0 && bus.num_channels == 0);
            if bus.num_channels < 0
                || !channels_match
                || (bus.num_channels > 0 && bus.buffers.is_null())
            {
                return kInvalidArgument;
            }
        }

        let shared_generation = (*obj).parameter_bridge.generation();
        let plugin = match (*obj).plugin.as_mut() {
            Some(p) => p,
            None => return kNotInitialized,
        };

        if shared_generation != (*obj).parameter_generation {
            for param in &(*obj).params {
                plugin.set_param(param.id, (*obj).parameter_bridge.get(param.id));
            }
            (*obj).parameter_generation = shared_generation;
        }

        (*obj).final_parameter_values.fill(None);
        let mut accepted_event_count = 0usize;
        if let Some(ctx) = (*obj).process_ctx.as_mut() {
            ctx.clear_param_changes();
        }

        // Read every point from every per-parameter queue. VST3 groups points
        // by parameter, so the context is stably merged by sample offset below.
        if !data_ref.input_parameter_changes.is_null() {
            let param_changes = data_ref.input_parameter_changes;
            let vtbl = *(param_changes as *const *const IParameterChangesVtbl);
            if vtbl.is_null() {
                return kInvalidArgument;
            }
            let num_params = ((*vtbl).get_parameter_count)(param_changes);
            if num_params < 0 {
                return kInvalidArgument;
            }
            let num_params = num_params as usize;
            if num_params > MAX_PARAMETER_QUEUES {
                return kOutOfMemory;
            }
            for queue_index in 0..num_params {
                let queue_index = queue_index as int32;
                let queue = ((*vtbl).get_parameter_data)(param_changes, queue_index);
                if queue.is_null() {
                    continue;
                }
                let queue_vtbl = *(queue as *const *const IParamValueQueueVtbl);
                if queue_vtbl.is_null() {
                    return kInvalidArgument;
                }
                let param_id = ((*queue_vtbl).get_parameter_id)(queue);
                let Some(param_index) = (*obj)
                    .params
                    .iter()
                    .position(|parameter| parameter.id == param_id)
                else {
                    continue;
                };
                let num_points = ((*queue_vtbl).get_point_count)(queue);
                if num_points < 0 {
                    return kInvalidArgument;
                }
                let num_points = num_points as usize;
                // Check the whole known queue before entering the point loop.
                // This keeps a malicious point count from turning into an
                // unbounded series of ABI calls when the plugin event budget
                // is large (or intentionally unlimited).
                if num_points > P::MAX_EVENTS_PER_BLOCK.saturating_sub(accepted_event_count) {
                    return kOutOfMemory;
                }
                for point_index in 0..num_points {
                    let point_index = point_index as int32;
                    let mut offset: int32 = 0;
                    let mut value: ParamValue = 0.0;
                    if ((*queue_vtbl).get_point)(queue, point_index, &mut offset, &mut value)
                        == kResultOk
                        && value.is_finite()
                    {
                        if accepted_event_count >= P::MAX_EVENTS_PER_BLOCK {
                            return kOutOfMemory;
                        }
                        let value = value.clamp(0.0, 1.0);
                        if data_ref.num_samples > 0 {
                            let sample_offset = event_sample_offset(offset, data_ref.num_samples);
                            if let Some(ctx) = (*obj).process_ctx.as_mut() {
                                if !ctx.try_add_param_change(sample_offset, param_id, value) {
                                    return kOutOfMemory;
                                }
                            }
                        }
                        accepted_event_count += 1;
                        (&mut (*obj).final_parameter_values)[param_index] = Some(value);
                    }
                }
            }
        }
        if let Some(ctx) = (*obj).process_ctx.as_mut() {
            ctx.sort_param_changes_by_offset();
            if data_ref.num_samples > 0 {
                (*obj).final_parameter_values.fill(None);
                for change in ctx.param_changes() {
                    if let Some(param_index) = (&(*obj).params)
                        .iter()
                        .position(|parameter| parameter.id == change.id)
                    {
                        (&mut (*obj).final_parameter_values)[param_index] = Some(change.value);
                    }
                }
            }
        }

        if data_ref.num_samples == 0 {
            for (parameter, value) in (*obj)
                .params
                .iter()
                .zip((*obj).final_parameter_values.iter().copied())
            {
                if let Some(value) = value {
                    plugin.set_param(parameter.id, value);
                    (*obj).parameter_bridge.set(parameter.id, value);
                }
            }
            return kResultOk;
        }

        // Process MIDI events
        if !data_ref.input_events.is_null() {
            let events = data_ref.input_events;
            let vtbl = *(events as *const *const IEventListVtbl);
            if vtbl.is_null() {
                return kInvalidArgument;
            }
            let num_events = ((*vtbl).get_event_count)(events);
            if num_events < 0 {
                return kInvalidArgument;
            }
            if num_events as usize > P::MAX_EVENTS_PER_BLOCK.saturating_sub(accepted_event_count) {
                return kOutOfMemory;
            }
            for i in 0..num_events {
                let mut event = Event::default();
                if ((*vtbl).get_event)(events, i, &mut event) == kResultOk {
                    if !valid_note_event(&event, (*obj).accepts_midi) {
                        return kInvalidArgument;
                    }
                    let sample_offset =
                        event_sample_offset(event.sample_offset, data_ref.num_samples);
                    match event.type_ {
                        EventTypes::kNoteOnEvent => {
                            let note = event.event.note_on;
                            plugin.note_on(sample_offset, note.channel, note.pitch, note.velocity);
                        }
                        EventTypes::kNoteOffEvent => {
                            let note = event.event.note_off;
                            plugin.note_off(sample_offset, note.channel, note.pitch, note.velocity);
                        }
                        EventTypes::kNoteExpressionValueEvent => {
                            let expression = event.event.note_expression_value;
                            if expression.value.is_finite() {
                                plugin.note_expression(
                                    sample_offset,
                                    expression.type_id,
                                    expression.note_id,
                                    expression.value,
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Process audio using the ProcessContext
        if let Some(ctx) = (*obj).process_ctx.as_mut() {
            // Update context
            ctx.num_samples = data_ref.num_samples as usize;
            ctx.update_transport_from_raw(data_ref.process_context as *const c_void);

            // Flatten every VST3 bus into the format-neutral channel list.
            ctx.clear_inputs(ctx.num_samples);
            if !inputs.is_empty() {
                let mut channel_offset = 0;
                for input in inputs {
                    let num_channels = input.num_channels as usize;
                    ctx.copy_from_raw_inputs_at(
                        input.buffers as *const *const f32,
                        num_channels,
                        ctx.num_samples,
                        channel_offset,
                        input.silence_flags,
                    );
                    channel_offset = channel_offset.saturating_add(num_channels);
                }
            }

            // Call plugin process
            if let Err(error) = plugin.process(ctx) {
                return match error {
                    ProcessError::OutOfMemory => kOutOfMemory,
                    ProcessError::InvalidArgument => kInvalidArgument,
                    ProcessError::Internal => kInternalError,
                };
            }

            // Copy outputs
            if !outputs.is_empty() {
                let mut channel_offset = 0;
                for output in outputs {
                    let num_channels = output.num_channels as usize;
                    ctx.copy_to_raw_outputs_at(
                        output.buffers as *const *mut f32,
                        num_channels,
                        ctx.num_samples,
                        channel_offset,
                    );
                    channel_offset = channel_offset.saturating_add(num_channels);
                }
            }
        }

        for (parameter, value) in (*obj)
            .params
            .iter()
            .zip((*obj).final_parameter_values.iter().copied())
        {
            if let Some(value) = value {
                plugin.set_param(parameter.id, value);
                (*obj).parameter_bridge.set(parameter.id, value);
            }
        }

        kResultOk
    }

    unsafe extern "system" fn audio_get_tail_samples(this: *mut c_void) -> uint32 {
        ffi_guard(kNoTail, || unsafe {
            Self::audio_get_tail_samples_unchecked(this)
        })
    }

    unsafe fn audio_get_tail_samples_unchecked(this: *mut c_void) -> uint32 {
        if this.is_null() {
            return kNoTail;
        }
        let obj = Self::from_audio(this);
        (*obj).plugin.as_ref().map(|p| p.tail()).unwrap_or(kNoTail)
    }
}

impl<P: Plugin> Drop for ProcessorWrapper<P> {
    fn drop(&mut self) {
        if self.active {
            self.active = false;
            self.processing = false;
            if let Some(plugin) = self.plugin.as_mut() {
                // Dropping a COM object is itself reached through a raw ABI
                // callback. Do not let a user panic escape from `Drop`.
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    plugin.deactivate();
                }));
            }
        }
    }
}

/// Internal controller wrapper
#[repr(C)]
pub struct ControllerWrapper<P: Plugin> {
    vtbl: *const ControllerVtbl,
    vtbl_connection: *const ConnectionPointVtbl,
    ref_count: AtomicI32,
    params: Vec<ParamInfo>,
    parameter_bridge: Arc<ParameterBridge>,
    host: HostHandle,
    _marker: PhantomData<P>,
    _vtbl_storage: Box<ControllerVtbl>,
    _connection_vtbl_storage: Box<ConnectionPointVtbl>,
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

impl<P: Plugin> ControllerWrapper<P> {
    pub fn new() -> *mut Self {
        ffi_guard(std::ptr::null_mut(), || Self::new_unchecked())
    }

    fn new_unchecked() -> *mut Self {
        let params = P::params();
        let parameter_bridge = Arc::new(ParameterBridge::new(&params));
        let host = HostHandle::new(parameter_bridge.clone());

        let vtbl_storage = Box::new(Self::make_vtbl());
        let connection_vtbl_storage = Box::new(Self::make_connection_vtbl());
        let vtbl = &*vtbl_storage;
        let vtbl_connection = &*connection_vtbl_storage;

        let wrapper = Box::new(Self {
            vtbl,
            vtbl_connection,
            ref_count: AtomicI32::new(1),
            params,
            parameter_bridge,
            host,
            _marker: PhantomData,
            _vtbl_storage: vtbl_storage,
            _connection_vtbl_storage: connection_vtbl_storage,
        });
        Box::into_raw(wrapper)
    }

    fn make_vtbl() -> ControllerVtbl {
        ControllerVtbl {
            query_interface: Self::query_interface,
            add_ref: Self::add_ref,
            release: Self::release,
            initialize: Self::initialize,
            terminate: Self::terminate,
            set_component_state: Self::set_component_state,
            set_state: Self::set_state,
            get_state: Self::get_state,
            get_parameter_count: Self::get_parameter_count,
            get_parameter_info: Self::get_parameter_info,
            get_param_string_by_value: Self::get_param_string_by_value,
            get_param_value_by_string: Self::get_param_value_by_string,
            normalized_param_to_plain: Self::normalized_param_to_plain,
            plain_param_to_normalized: Self::plain_param_to_normalized,
            get_param_normalized: Self::get_param_normalized,
            set_param_normalized: Self::set_param_normalized,
            set_component_handler: Self::set_component_handler,
            create_view: Self::create_view,
        }
    }

    fn make_connection_vtbl() -> ConnectionPointVtbl {
        ConnectionPointVtbl {
            query_interface: Self::connection_query_interface,
            add_ref: Self::connection_add_ref,
            release: Self::connection_release,
            connect: Self::connection_connect,
            disconnect: Self::connection_disconnect,
            notify: Self::connection_notify,
            get_role: Self::connection_get_role,
            get_parameter_bridge: Self::connection_get_parameter_bridge,
            adopt_parameter_bridge: Self::connection_adopt_parameter_bridge,
        }
    }

    unsafe fn from_connection(this: *mut c_void) -> *mut Self {
        (this as *mut u8).sub(std::mem::size_of::<*const c_void>()) as *mut Self
    }

    unsafe extern "system" fn query_interface(
        this: *mut c_void,
        iid: *const TUID,
        obj: *mut *mut c_void,
    ) -> tresult {
        if this.is_null() || iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        *obj = std::ptr::null_mut();
        let iid = &*iid;
        if iid_equal(iid, &iid::IUnknown)
            || iid_equal(iid, &base_iid::IPluginBase)
            || iid_equal(iid, &vst_iid::IEditController)
        {
            Self::add_ref(this);
            *obj = this;
            return kResultOk;
        }
        if iid_equal(iid, &vst_iid::IConnectionPoint) {
            Self::add_ref(this);
            *obj = &(*(this as *mut Self)).vtbl_connection as *const _ as *mut c_void;
            return kResultOk;
        }
        *obj = std::ptr::null_mut();
        kNoInterface
    }

    unsafe extern "system" fn add_ref(this: *mut c_void) -> uint32 {
        if this.is_null() {
            return 0;
        }
        (*(this as *mut Self))
            .ref_count
            .fetch_add(1, Ordering::SeqCst) as uint32
            + 1
    }

    unsafe extern "system" fn release(this: *mut c_void) -> uint32 {
        ffi_guard(0, || unsafe { Self::release_unchecked(this) })
    }

    unsafe fn release_unchecked(this: *mut c_void) -> uint32 {
        if this.is_null() {
            return 0;
        }
        let obj = this as *mut Self;
        let count = (*obj).ref_count.fetch_sub(1, Ordering::SeqCst) - 1;
        if count == 0 {
            let _ = Box::from_raw(obj);
        }
        count as uint32
    }

    unsafe extern "system" fn connection_query_interface(
        this: *mut c_void,
        requested_iid: *const TUID,
        obj: *mut *mut c_void,
    ) -> tresult {
        if this.is_null() || requested_iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        *obj = std::ptr::null_mut();
        let base = Self::from_connection(this);
        if iid_equal(&*requested_iid, &PARAMETER_CONNECTION_IID)
            || iid_equal(&*requested_iid, &vst_iid::IConnectionPoint)
        {
            Self::connection_add_ref(this);
            *obj = this;
            return kResultOk;
        }
        Self::query_interface(base as *mut c_void, requested_iid, obj)
    }

    unsafe extern "system" fn connection_add_ref(this: *mut c_void) -> uint32 {
        if this.is_null() {
            return 0;
        }
        Self::add_ref(Self::from_connection(this) as *mut c_void)
    }

    unsafe extern "system" fn connection_release(this: *mut c_void) -> uint32 {
        if this.is_null() {
            return 0;
        }
        Self::release(Self::from_connection(this) as *mut c_void)
    }

    unsafe extern "system" fn connection_connect(this: *mut c_void, other: *mut c_void) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            connect_parameter_bridges(this, other)
        })
    }

    unsafe extern "system" fn connection_disconnect(
        this: *mut c_void,
        other: *mut c_void,
    ) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            disconnect_parameter_bridges(this, other)
        })
    }

    unsafe extern "system" fn connection_notify(
        _this: *mut c_void,
        _message: *mut c_void,
    ) -> tresult {
        kResultOk
    }

    unsafe extern "system" fn connection_get_role(_this: *mut c_void) -> u32 {
        CONNECTION_ROLE_CONTROLLER
    }

    unsafe extern "system" fn connection_get_parameter_bridge(
        this: *mut c_void,
    ) -> *const ParameterBridge {
        if this.is_null() {
            return std::ptr::null();
        }
        let base = Self::from_connection(this);
        Arc::as_ptr(&(*base).parameter_bridge)
    }

    unsafe extern "system" fn connection_adopt_parameter_bridge(
        this: *mut c_void,
        bridge: *const ParameterBridge,
    ) -> tresult {
        if this.is_null() || bridge.is_null() {
            return kInvalidArgument;
        }
        let base = Self::from_connection(this);
        if (*base).parameter_bridge.link_to(&*bridge) {
            kResultOk
        } else {
            kInvalidArgument
        }
    }

    unsafe extern "system" fn initialize(this: *mut c_void, _context: *mut c_void) -> tresult {
        if this.is_null() {
            kInvalidArgument
        } else {
            kResultOk
        }
    }
    unsafe extern "system" fn terminate(this: *mut c_void) -> tresult {
        if this.is_null() {
            kInvalidArgument
        } else {
            kResultOk
        }
    }
    unsafe extern "system" fn set_component_state(
        this: *mut c_void,
        state: *mut c_void,
    ) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            if this.is_null() {
                return kInvalidArgument;
            }
            let obj = this as *mut Self;
            load_parameter_state(state, &(*obj).params, |id, value| {
                (*obj).parameter_bridge.set(id, value);
            })
        })
    }
    unsafe extern "system" fn set_state(this: *mut c_void, state: *mut c_void) -> tresult {
        Self::set_component_state(this, state)
    }
    unsafe extern "system" fn get_state(this: *mut c_void, state: *mut c_void) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            if this.is_null() {
                return kInvalidArgument;
            }
            let obj = this as *mut Self;
            save_parameter_state(state, &(*obj).params, |id| (*obj).parameter_bridge.get(id))
        })
    }

    unsafe extern "system" fn get_parameter_count(this: *mut c_void) -> int32 {
        if this.is_null() {
            return 0;
        }
        (*(this as *mut Self)).params.len() as int32
    }

    unsafe extern "system" fn get_parameter_info(
        this: *mut c_void,
        param_index: int32,
        info: *mut ParameterInfo,
    ) -> tresult {
        if this.is_null() || info.is_null() || param_index < 0 {
            return kInvalidArgument;
        }
        let obj = this as *mut Self;
        if let Some(param) = (&(*obj).params).get(param_index as usize) {
            let info = &mut *info;
            info.id = param.id;
            str16cpy_safe(&mut info.title, param.name);
            str16cpy_safe(&mut info.short_title, param.short_name);
            str16cpy_safe(&mut info.units, param.units);
            info.step_count = param.step_count;
            info.default_normalized_value = param.default;
            info.unit_id = 0;
            info.flags = param.flags.0 as int32
                | if param.step_count > 0 {
                    ParameterFlags::kIsList
                } else {
                    0
                };
            return kResultOk;
        }
        kInvalidArgument
    }

    unsafe extern "system" fn get_param_string_by_value(
        this: *mut c_void,
        id: ParamID,
        value: ParamValue,
        string: *mut String128,
    ) -> tresult {
        if this.is_null() || string.is_null() {
            return kInvalidArgument;
        }
        let obj = this as *mut Self;
        if let Some(param) = (&(*obj).params).iter().find(|p| p.id == id) {
            let normalized = sanitize_normalized(value, (*obj).parameter_bridge.get(id));
            let plain = safe_plain_value(param, normalized);
            let s = format!("{:.2}{}", plain, param.units);
            str16cpy_safe(&mut *string, &s);
            return kResultOk;
        }
        kInvalidArgument
    }

    unsafe extern "system" fn get_param_value_by_string(
        _this: *mut c_void,
        _id: ParamID,
        _string: *const TChar,
        _value: *mut ParamValue,
    ) -> tresult {
        kNotImplemented
    }

    unsafe extern "system" fn normalized_param_to_plain(
        this: *mut c_void,
        id: ParamID,
        value: ParamValue,
    ) -> ParamValue {
        if this.is_null() {
            return value;
        }
        let obj = this as *mut Self;
        if let Some(param) = (&(*obj).params).iter().find(|p| p.id == id) {
            let normalized = sanitize_normalized(value, (*obj).parameter_bridge.get(id));
            safe_plain_value(param, normalized)
        } else {
            sanitize_normalized(value, 0.0)
        }
    }

    unsafe extern "system" fn plain_param_to_normalized(
        this: *mut c_void,
        id: ParamID,
        value: ParamValue,
    ) -> ParamValue {
        if this.is_null() {
            return value;
        }
        let obj = this as *mut Self;
        if let Some(param) = (&(*obj).params).iter().find(|p| p.id == id) {
            let current_normalized = (*obj).parameter_bridge.get(id);
            let current_plain = safe_plain_value(param, current_normalized);
            let plain = sanitize_plain(value, current_plain);
            let normalized = param.to_normalized(plain);
            sanitize_normalized(normalized, current_normalized)
        } else {
            sanitize_normalized(value, 0.0)
        }
    }

    unsafe extern "system" fn get_param_normalized(this: *mut c_void, id: ParamID) -> ParamValue {
        if this.is_null() {
            return 0.0;
        }
        let obj = this as *mut Self;
        (*obj).parameter_bridge.get(id)
    }

    unsafe extern "system" fn set_param_normalized(
        this: *mut c_void,
        id: ParamID,
        value: ParamValue,
    ) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            if this.is_null() {
                return kInvalidArgument;
            }
            let obj = this as *mut Self;
            if (&(*obj).params).iter().any(|param| param.id == id) {
                let value = sanitize_normalized(value, (*obj).parameter_bridge.get(id));
                (*obj).parameter_bridge.set(id, value);
                return kResultOk;
            }
            kInvalidArgument
        })
    }

    unsafe extern "system" fn set_component_handler(
        this: *mut c_void,
        handler: *mut c_void,
    ) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            if this.is_null() {
                return kInvalidArgument;
            }
            let obj = this as *mut Self;
            if (*obj).host.set_component_handler(handler) {
                kResultOk
            } else {
                kInvalidArgument
            }
        })
    }
    unsafe extern "system" fn create_view(_this: *mut c_void, _name: FIDString) -> *mut c_void {
        std::ptr::null_mut()
    }
}

// =============================================================================
// PlugViewWrapper - IPlugView implementation for GUI plugins
// =============================================================================

use crate::gui::{GuiPlugin, GuiSize};
use raw_window_handle::RawWindowHandle;

#[repr(C)]
struct PlugViewVtblLocal {
    query_interface:
        unsafe extern "system" fn(*mut c_void, *const TUID, *mut *mut c_void) -> tresult,
    add_ref: unsafe extern "system" fn(*mut c_void) -> uint32,
    release: unsafe extern "system" fn(*mut c_void) -> uint32,
    is_platform_type_supported: unsafe extern "system" fn(*mut c_void, FIDString) -> tresult,
    attached: unsafe extern "system" fn(*mut c_void, *mut c_void, FIDString) -> tresult,
    removed: unsafe extern "system" fn(*mut c_void) -> tresult,
    on_wheel: unsafe extern "system" fn(*mut c_void, f32) -> tresult,
    on_key_down: unsafe extern "system" fn(*mut c_void, char16, int16, int16) -> tresult,
    on_key_up: unsafe extern "system" fn(*mut c_void, char16, int16, int16) -> tresult,
    get_size: unsafe extern "system" fn(*mut c_void, *mut ViewRect) -> tresult,
    on_size: unsafe extern "system" fn(*mut c_void, *mut ViewRect) -> tresult,
    on_focus: unsafe extern "system" fn(*mut c_void, TBool) -> tresult,
    set_frame: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
    can_resize: unsafe extern "system" fn(*mut c_void) -> tresult,
    check_size_constraint: unsafe extern "system" fn(*mut c_void, *mut ViewRect) -> tresult,
}

/// Internal IPlugView wrapper for GUI plugins
#[repr(C)]
pub struct PlugViewWrapper<P: GuiPlugin> {
    vtbl: *const PlugViewVtblLocal,
    ref_count: AtomicI32,
    plugin: *mut P,
    size: GuiSize,
    host: HostHandle,
    owner: *mut GuiControllerWrapper<P>,
    _vtbl_storage: Box<PlugViewVtblLocal>,
}

impl<P: GuiPlugin> PlugViewWrapper<P> {
    pub fn new(owner: *mut GuiControllerWrapper<P>, plugin: *mut P, host: HostHandle) -> *mut Self {
        ffi_guard(std::ptr::null_mut(), || {
            Self::new_unchecked(owner, plugin, host)
        })
    }

    fn new_unchecked(
        owner: *mut GuiControllerWrapper<P>,
        plugin: *mut P,
        host: HostHandle,
    ) -> *mut Self {
        let vtbl_storage = Box::new(Self::make_vtbl());
        let vtbl = &*vtbl_storage;
        let size = P::gui_size();

        let wrapper = Box::new(Self {
            vtbl,
            ref_count: AtomicI32::new(1),
            plugin,
            size,
            host,
            owner,
            _vtbl_storage: vtbl_storage,
        });
        if !owner.is_null() {
            unsafe { GuiControllerWrapper::<P>::add_ref(owner.cast::<c_void>()) };
        }
        Box::into_raw(wrapper)
    }

    fn make_vtbl() -> PlugViewVtblLocal {
        PlugViewVtblLocal {
            query_interface: Self::query_interface,
            add_ref: Self::add_ref,
            release: Self::release,
            is_platform_type_supported: Self::is_platform_type_supported,
            attached: Self::attached,
            removed: Self::removed,
            on_wheel: Self::on_wheel,
            on_key_down: Self::on_key_down,
            on_key_up: Self::on_key_up,
            get_size: Self::get_size,
            on_size: Self::on_size,
            on_focus: Self::on_focus,
            set_frame: Self::set_frame,
            can_resize: Self::can_resize,
            check_size_constraint: Self::check_size_constraint,
        }
    }

    unsafe extern "system" fn query_interface(
        this: *mut c_void,
        iid: *const TUID,
        obj: *mut *mut c_void,
    ) -> tresult {
        if this.is_null() || iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        *obj = std::ptr::null_mut();
        let iid = &*iid;
        if iid_equal(iid, &iid::IUnknown) || iid_equal(iid, &gui_iid::IPlugView) {
            Self::add_ref(this);
            *obj = this;
            return kResultOk;
        }
        *obj = std::ptr::null_mut();
        kNoInterface
    }

    unsafe extern "system" fn add_ref(this: *mut c_void) -> uint32 {
        if this.is_null() {
            return 0;
        }
        (*(this as *mut Self))
            .ref_count
            .fetch_add(1, Ordering::SeqCst) as uint32
            + 1
    }

    unsafe extern "system" fn release(this: *mut c_void) -> uint32 {
        ffi_guard(0, || unsafe { Self::release_unchecked(this) })
    }

    unsafe fn release_unchecked(this: *mut c_void) -> uint32 {
        if this.is_null() {
            return 0;
        }
        let obj = this as *mut Self;
        let count = (*obj).ref_count.fetch_sub(1, Ordering::SeqCst) - 1;
        if count == 0 {
            let _ = Box::from_raw(obj);
        }
        count as uint32
    }

    unsafe extern "system" fn is_platform_type_supported(
        this: *mut c_void,
        type_: FIDString,
    ) -> tresult {
        if this.is_null() || type_.is_null() {
            return kResultFalse;
        }
        let type_str = std::ffi::CStr::from_ptr(type_);
        if let Ok(s) = type_str.to_str() {
            if ffi_guard(false, || P::is_platform_supported(s)) {
                kResultOk
            } else {
                kResultFalse
            }
        } else {
            kResultFalse
        }
    }

    unsafe extern "system" fn attached(
        this: *mut c_void,
        parent: *mut c_void,
        type_: FIDString,
    ) -> tresult {
        if this.is_null() {
            return kInvalidArgument;
        }
        let obj = this as *mut Self;
        if parent.is_null() || (*obj).plugin.is_null() {
            return kInvalidArgument;
        }

        // Convert platform type and parent to RawWindowHandle
        let type_str = if !type_.is_null() {
            std::ffi::CStr::from_ptr(type_).to_str().unwrap_or("")
        } else {
            ""
        };

        let handle = match type_str {
            "NSView" => {
                #[cfg(target_os = "macos")]
                {
                    use raw_window_handle::AppKitWindowHandle;
                    let Some(ns_view) = std::ptr::NonNull::new(parent) else {
                        return kInvalidArgument;
                    };
                    let h = AppKitWindowHandle::new(ns_view);
                    RawWindowHandle::AppKit(h)
                }
                #[cfg(not(target_os = "macos"))]
                {
                    return kNotImplemented;
                }
            }
            "HWND" => {
                #[cfg(target_os = "windows")]
                {
                    use raw_window_handle::Win32WindowHandle;
                    use std::num::NonZeroIsize;
                    let Some(hwnd) = NonZeroIsize::new(parent as isize) else {
                        return kInvalidArgument;
                    };
                    let h = Win32WindowHandle::new(hwnd);
                    RawWindowHandle::Win32(h)
                }
                #[cfg(not(target_os = "windows"))]
                {
                    return kNotImplemented;
                }
            }
            "X11EmbedWindowID" => {
                #[cfg(target_os = "linux")]
                {
                    use raw_window_handle::XlibWindowHandle;
                    let h = XlibWindowHandle::new(parent as u64);
                    RawWindowHandle::Xlib(h)
                }
                #[cfg(not(target_os = "linux"))]
                {
                    return kNotImplemented;
                }
            }
            _ => return kNotImplemented,
        };

        if ffi_guard(false, || (*(*obj).plugin).gui_create(handle)) {
            kResultOk
        } else {
            kResultFalse
        }
    }

    unsafe extern "system" fn removed(this: *mut c_void) -> tresult {
        if this.is_null() {
            return kInvalidArgument;
        }
        let obj = this as *mut Self;
        if (*obj).plugin.is_null() {
            return kInvalidArgument;
        }
        ffi_guard(kInternalError, || {
            (*(*obj).plugin).gui_destroy();
            kResultOk
        })
    }

    unsafe extern "system" fn on_wheel(_this: *mut c_void, _distance: f32) -> tresult {
        kResultFalse
    }
    unsafe extern "system" fn on_key_down(
        _this: *mut c_void,
        _key: char16,
        _key_code: int16,
        _modifiers: int16,
    ) -> tresult {
        kResultFalse
    }
    unsafe extern "system" fn on_key_up(
        _this: *mut c_void,
        _key: char16,
        _key_code: int16,
        _modifiers: int16,
    ) -> tresult {
        kResultFalse
    }

    unsafe extern "system" fn get_size(this: *mut c_void, size: *mut ViewRect) -> tresult {
        if this.is_null() || size.is_null() {
            return kInvalidArgument;
        }
        let obj = this as *mut Self;
        *size = ViewRect::new(0, 0, (*obj).size.width as i32, (*obj).size.height as i32);
        kResultOk
    }

    unsafe extern "system" fn on_size(this: *mut c_void, new_size: *mut ViewRect) -> tresult {
        if this.is_null() || new_size.is_null() {
            return kInvalidArgument;
        }
        let obj = this as *mut Self;
        if (*obj).plugin.is_null() {
            return kInvalidArgument;
        }
        let rect = &*new_size;
        if rect.width() <= 0 || rect.height() <= 0 {
            return kInvalidArgument;
        }
        (*obj).size = GuiSize::new(rect.width() as u32, rect.height() as u32);
        ffi_guard(kInternalError, || {
            (*(*obj).plugin).gui_resize((*obj).size);
            kResultOk
        })
    }

    unsafe extern "system" fn on_focus(_this: *mut c_void, _state: TBool) -> tresult {
        kResultOk
    }
    unsafe extern "system" fn set_frame(this: *mut c_void, frame: *mut c_void) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            if this.is_null() {
                return kInvalidArgument;
            }
            let obj = this as *mut Self;
            if (*obj).host.set_plug_frame(frame, this) {
                kResultOk
            } else {
                kInvalidArgument
            }
        })
    }
    unsafe extern "system" fn can_resize(this: *mut c_void) -> tresult {
        if this.is_null() {
            return kInvalidArgument;
        }
        let obj = this as *mut Self;
        if !(*obj).plugin.is_null() && ffi_guard(false, || (*(*obj).plugin).gui_can_resize()) {
            kResultTrue
        } else {
            kResultFalse
        }
    }
    unsafe extern "system" fn check_size_constraint(
        this: *mut c_void,
        rect: *mut ViewRect,
    ) -> tresult {
        if this.is_null() || rect.is_null() {
            return kInvalidArgument;
        }
        let obj = this as *mut Self;
        if !(*obj).plugin.is_null()
            && ffi_guard(false, || (*(*obj).plugin).gui_can_resize())
            && (*rect).width() > 0
            && (*rect).height() > 0
        {
            kResultOk
        } else {
            kResultFalse
        }
    }
}

impl<P: GuiPlugin> Drop for PlugViewWrapper<P> {
    fn drop(&mut self) {
        let view = self as *mut Self as *mut c_void;
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.host.clear_plug_frame(view);
        }));
        if !self.owner.is_null() {
            let owner = self.owner;
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                GuiControllerWrapper::<P>::release(owner.cast::<c_void>())
            }));
            self.owner = std::ptr::null_mut();
        }
    }
}

// =============================================================================
// GuiControllerWrapper - Controller that creates IPlugView for GUI plugins
// =============================================================================

/// Controller wrapper for GUI plugins that can create IPlugView
#[repr(C)]
pub struct GuiControllerWrapper<P: GuiPlugin> {
    vtbl: *const ControllerVtbl,
    vtbl_connection: *const ConnectionPointVtbl,
    ref_count: AtomicI32,
    params: Vec<ParamInfo>,
    parameter_bridge: Arc<ParameterBridge>,
    host: HostHandle,
    plugin: Option<P>,
    _vtbl_storage: Box<ControllerVtbl>,
    _connection_vtbl_storage: Box<ConnectionPointVtbl>,
}

impl<P: GuiPlugin> GuiControllerWrapper<P> {
    pub fn new() -> *mut Self {
        ffi_guard(std::ptr::null_mut(), || Self::new_unchecked())
    }

    fn new_unchecked() -> *mut Self {
        let params = P::params();
        let parameter_bridge = Arc::new(ParameterBridge::new(&params));

        let vtbl_storage = Box::new(Self::make_vtbl());
        let connection_vtbl_storage = Box::new(Self::make_connection_vtbl());
        let vtbl = &*vtbl_storage;
        let vtbl_connection = &*connection_vtbl_storage;
        let host = HostHandle::new(parameter_bridge.clone());
        let plugin = P::new(host.clone());

        let wrapper = Box::new(Self {
            vtbl,
            vtbl_connection,
            ref_count: AtomicI32::new(1),
            params,
            parameter_bridge,
            host,
            plugin: Some(plugin),
            _vtbl_storage: vtbl_storage,
            _connection_vtbl_storage: connection_vtbl_storage,
        });
        Box::into_raw(wrapper)
    }

    fn make_vtbl() -> ControllerVtbl {
        ControllerVtbl {
            query_interface: Self::query_interface,
            add_ref: Self::add_ref,
            release: Self::release,
            initialize: Self::initialize,
            terminate: Self::terminate,
            set_component_state: Self::set_component_state,
            set_state: Self::set_state,
            get_state: Self::get_state,
            get_parameter_count: Self::get_parameter_count,
            get_parameter_info: Self::get_parameter_info,
            get_param_string_by_value: Self::get_param_string_by_value,
            get_param_value_by_string: Self::get_param_value_by_string,
            normalized_param_to_plain: Self::normalized_param_to_plain,
            plain_param_to_normalized: Self::plain_param_to_normalized,
            get_param_normalized: Self::get_param_normalized,
            set_param_normalized: Self::set_param_normalized,
            set_component_handler: Self::set_component_handler,
            create_view: Self::create_view,
        }
    }

    fn make_connection_vtbl() -> ConnectionPointVtbl {
        ConnectionPointVtbl {
            query_interface: Self::connection_query_interface,
            add_ref: Self::connection_add_ref,
            release: Self::connection_release,
            connect: Self::connection_connect,
            disconnect: Self::connection_disconnect,
            notify: Self::connection_notify,
            get_role: Self::connection_get_role,
            get_parameter_bridge: Self::connection_get_parameter_bridge,
            adopt_parameter_bridge: Self::connection_adopt_parameter_bridge,
        }
    }

    unsafe fn from_connection(this: *mut c_void) -> *mut Self {
        (this as *mut u8).sub(std::mem::size_of::<*const c_void>()) as *mut Self
    }

    unsafe extern "system" fn query_interface(
        this: *mut c_void,
        iid: *const TUID,
        obj: *mut *mut c_void,
    ) -> tresult {
        if this.is_null() || iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        *obj = std::ptr::null_mut();
        let iid = &*iid;
        if iid_equal(iid, &iid::IUnknown)
            || iid_equal(iid, &base_iid::IPluginBase)
            || iid_equal(iid, &vst_iid::IEditController)
        {
            Self::add_ref(this);
            *obj = this;
            return kResultOk;
        }
        if iid_equal(iid, &vst_iid::IConnectionPoint) {
            Self::add_ref(this);
            *obj = &(*(this as *mut Self)).vtbl_connection as *const _ as *mut c_void;
            return kResultOk;
        }
        *obj = std::ptr::null_mut();
        kNoInterface
    }

    unsafe extern "system" fn add_ref(this: *mut c_void) -> uint32 {
        if this.is_null() {
            return 0;
        }
        (*(this as *mut Self))
            .ref_count
            .fetch_add(1, Ordering::SeqCst) as uint32
            + 1
    }

    unsafe extern "system" fn release(this: *mut c_void) -> uint32 {
        ffi_guard(0, || unsafe { Self::release_unchecked(this) })
    }

    unsafe fn release_unchecked(this: *mut c_void) -> uint32 {
        if this.is_null() {
            return 0;
        }
        let obj = this as *mut Self;
        let count = (*obj).ref_count.fetch_sub(1, Ordering::SeqCst) - 1;
        if count == 0 {
            let _ = Box::from_raw(obj);
        }
        count as uint32
    }

    unsafe extern "system" fn connection_query_interface(
        this: *mut c_void,
        requested_iid: *const TUID,
        obj: *mut *mut c_void,
    ) -> tresult {
        if this.is_null() || requested_iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        *obj = std::ptr::null_mut();
        let base = Self::from_connection(this);
        if iid_equal(&*requested_iid, &PARAMETER_CONNECTION_IID)
            || iid_equal(&*requested_iid, &vst_iid::IConnectionPoint)
        {
            Self::connection_add_ref(this);
            *obj = this;
            return kResultOk;
        }
        Self::query_interface(base as *mut c_void, requested_iid, obj)
    }

    unsafe extern "system" fn connection_add_ref(this: *mut c_void) -> uint32 {
        if this.is_null() {
            return 0;
        }
        Self::add_ref(Self::from_connection(this) as *mut c_void)
    }

    unsafe extern "system" fn connection_release(this: *mut c_void) -> uint32 {
        if this.is_null() {
            return 0;
        }
        Self::release(Self::from_connection(this) as *mut c_void)
    }

    unsafe extern "system" fn connection_connect(this: *mut c_void, other: *mut c_void) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            connect_parameter_bridges(this, other)
        })
    }

    unsafe extern "system" fn connection_disconnect(
        this: *mut c_void,
        other: *mut c_void,
    ) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            disconnect_parameter_bridges(this, other)
        })
    }

    unsafe extern "system" fn connection_notify(
        _this: *mut c_void,
        _message: *mut c_void,
    ) -> tresult {
        kResultOk
    }

    unsafe extern "system" fn connection_get_role(_this: *mut c_void) -> u32 {
        CONNECTION_ROLE_CONTROLLER
    }

    unsafe extern "system" fn connection_get_parameter_bridge(
        this: *mut c_void,
    ) -> *const ParameterBridge {
        if this.is_null() {
            return std::ptr::null();
        }
        let base = Self::from_connection(this);
        Arc::as_ptr(&(*base).parameter_bridge)
    }

    unsafe extern "system" fn connection_adopt_parameter_bridge(
        this: *mut c_void,
        bridge: *const ParameterBridge,
    ) -> tresult {
        if this.is_null() || bridge.is_null() {
            return kInvalidArgument;
        }
        let base = Self::from_connection(this);
        if (*base).parameter_bridge.link_to(&*bridge) {
            kResultOk
        } else {
            kInvalidArgument
        }
    }

    unsafe extern "system" fn initialize(this: *mut c_void, _context: *mut c_void) -> tresult {
        if this.is_null() {
            kInvalidArgument
        } else {
            kResultOk
        }
    }
    unsafe extern "system" fn terminate(this: *mut c_void) -> tresult {
        if this.is_null() {
            kInvalidArgument
        } else {
            kResultOk
        }
    }
    unsafe extern "system" fn set_component_state(
        this: *mut c_void,
        state: *mut c_void,
    ) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            if this.is_null() {
                return kInvalidArgument;
            }
            let obj = this as *mut Self;
            load_parameter_state(state, &(*obj).params, |id, value| {
                let value = sanitize_normalized(value, (*obj).parameter_bridge.get(id));
                if let Some(plugin) = (*obj).plugin.as_mut() {
                    plugin.set_param(id, value);
                }
                // Publish only after the user callback returns normally.
                (*obj).parameter_bridge.set(id, value);
            })
        })
    }
    unsafe extern "system" fn set_state(this: *mut c_void, state: *mut c_void) -> tresult {
        Self::set_component_state(this, state)
    }
    unsafe extern "system" fn get_state(this: *mut c_void, state: *mut c_void) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            if this.is_null() {
                return kInvalidArgument;
            }
            let obj = this as *mut Self;
            save_parameter_state(state, &(*obj).params, |id| (*obj).parameter_bridge.get(id))
        })
    }

    unsafe extern "system" fn get_parameter_count(this: *mut c_void) -> int32 {
        if this.is_null() {
            return 0;
        }
        (*(this as *mut Self)).params.len() as int32
    }

    unsafe extern "system" fn get_parameter_info(
        this: *mut c_void,
        param_index: int32,
        info: *mut ParameterInfo,
    ) -> tresult {
        if this.is_null() || info.is_null() || param_index < 0 {
            return kInvalidArgument;
        }
        let obj = this as *mut Self;
        if let Some(param) = (&(*obj).params).get(param_index as usize) {
            let info = &mut *info;
            info.id = param.id;
            str16cpy_safe(&mut info.title, param.name);
            str16cpy_safe(&mut info.short_title, param.short_name);
            str16cpy_safe(&mut info.units, param.units);
            info.step_count = param.step_count;
            info.default_normalized_value = param.default;
            info.unit_id = 0;
            info.flags = param.flags.0 as int32
                | if param.step_count > 0 {
                    ParameterFlags::kIsList
                } else {
                    0
                };
            return kResultOk;
        }
        kInvalidArgument
    }

    unsafe extern "system" fn get_param_string_by_value(
        this: *mut c_void,
        id: ParamID,
        value: ParamValue,
        string: *mut String128,
    ) -> tresult {
        if this.is_null() || string.is_null() {
            return kInvalidArgument;
        }
        let obj = this as *mut Self;
        if let Some(param) = (&(*obj).params).iter().find(|p| p.id == id) {
            let normalized = sanitize_normalized(value, (*obj).parameter_bridge.get(id));
            let plain = safe_plain_value(param, normalized);
            let s = format!("{:.2}{}", plain, param.units);
            str16cpy_safe(&mut *string, &s);
            return kResultOk;
        }
        kInvalidArgument
    }

    unsafe extern "system" fn get_param_value_by_string(
        _this: *mut c_void,
        _id: ParamID,
        _string: *const TChar,
        _value: *mut ParamValue,
    ) -> tresult {
        kNotImplemented
    }

    unsafe extern "system" fn normalized_param_to_plain(
        this: *mut c_void,
        id: ParamID,
        value: ParamValue,
    ) -> ParamValue {
        if this.is_null() {
            return value;
        }
        let obj = this as *mut Self;
        if let Some(param) = (&(*obj).params).iter().find(|p| p.id == id) {
            let normalized = sanitize_normalized(value, (*obj).parameter_bridge.get(id));
            safe_plain_value(param, normalized)
        } else {
            sanitize_normalized(value, 0.0)
        }
    }

    unsafe extern "system" fn plain_param_to_normalized(
        this: *mut c_void,
        id: ParamID,
        value: ParamValue,
    ) -> ParamValue {
        if this.is_null() {
            return value;
        }
        let obj = this as *mut Self;
        if let Some(param) = (&(*obj).params).iter().find(|p| p.id == id) {
            let current_normalized = (*obj).parameter_bridge.get(id);
            let current_plain = safe_plain_value(param, current_normalized);
            let plain = sanitize_plain(value, current_plain);
            let normalized = param.to_normalized(plain);
            sanitize_normalized(normalized, current_normalized)
        } else {
            sanitize_normalized(value, 0.0)
        }
    }

    unsafe extern "system" fn get_param_normalized(this: *mut c_void, id: ParamID) -> ParamValue {
        if this.is_null() {
            return 0.0;
        }
        let obj = this as *mut Self;
        (*obj).parameter_bridge.get(id)
    }

    unsafe extern "system" fn set_param_normalized(
        this: *mut c_void,
        id: ParamID,
        value: ParamValue,
    ) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            if this.is_null() {
                return kInvalidArgument;
            }
            let obj = this as *mut Self;
            if (&(*obj).params).iter().any(|param| param.id == id) {
                let value = sanitize_normalized(value, (*obj).parameter_bridge.get(id));
                if let Some(plugin) = (*obj).plugin.as_mut() {
                    plugin.set_param(id as u32, value);
                }
                // Keep the controller/processor bridge consistent with the
                // user object when the callback succeeds.
                (*obj).parameter_bridge.set(id, value);
                return kResultOk;
            }
            kInvalidArgument
        })
    }

    unsafe extern "system" fn set_component_handler(
        this: *mut c_void,
        handler: *mut c_void,
    ) -> tresult {
        ffi_guard(kInternalError, || unsafe {
            if this.is_null() {
                return kInvalidArgument;
            }
            let obj = this as *mut Self;
            if (*obj).host.set_component_handler(handler) {
                kResultOk
            } else {
                kInvalidArgument
            }
        })
    }

    unsafe extern "system" fn create_view(this: *mut c_void, name: FIDString) -> *mut c_void {
        if this.is_null() || name.is_null() {
            return std::ptr::null_mut();
        }
        let name_str = std::ffi::CStr::from_ptr(name);
        if name_str.to_bytes() != b"editor" {
            return std::ptr::null_mut();
        }

        let obj = this as *mut Self;
        if let Some(plugin) = (*obj).plugin.as_mut() {
            // Create PlugViewWrapper with pointer to plugin
            let plugin_ptr = plugin as *mut P;
            PlugViewWrapper::new(obj, plugin_ptr, (*obj).host.clone()) as *mut c_void
        } else {
            std::ptr::null_mut()
        }
    }
}

/// Export macro for VST3 plugins
#[macro_export]
macro_rules! export_vst3_plugin {
    ($plugin_type:ty) => {
        mod __vst3_rs_impl {
            use super::*;
            use std::ffi::c_void;
            use std::sync::OnceLock;
            use std::sync::atomic::{AtomicI32, Ordering};
            use $crate::vst3_sys::*;

            fn processor_cid() -> TUID {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    <$plugin_type as $crate::Plugin>::class_id()
                }))
                .unwrap_or([0; 16])
            }

            fn controller_cid() -> TUID {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    <$plugin_type as $crate::Plugin>::controller_class_id()
                }))
                .unwrap_or([0; 16])
            }

            fn plugin_info() -> $crate::PluginInfo {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    <$plugin_type as $crate::Plugin>::info()
                }))
                .unwrap_or_default()
            }

            #[repr(C)]
            struct PluginFactoryObj {
                vtbl: *const FactoryVtbl,
                ref_count: AtomicI32,
            }

            #[repr(C)]
            struct FactoryVtbl {
                query_interface: unsafe extern "system" fn(
                    *mut c_void,
                    *const TUID,
                    *mut *mut c_void,
                ) -> tresult,
                add_ref: unsafe extern "system" fn(*mut c_void) -> uint32,
                release: unsafe extern "system" fn(*mut c_void) -> uint32,
                get_factory_info:
                    unsafe extern "system" fn(*mut c_void, *mut PFactoryInfoData) -> tresult,
                count_classes: unsafe extern "system" fn(*mut c_void) -> int32,
                get_class_info:
                    unsafe extern "system" fn(*mut c_void, int32, *mut PClassInfoData) -> tresult,
                create_instance: unsafe extern "system" fn(
                    *mut c_void,
                    FIDString,
                    FIDString,
                    *mut *mut c_void,
                ) -> tresult,
                get_class_info2:
                    unsafe extern "system" fn(*mut c_void, int32, *mut PClassInfo2Data) -> tresult,
                get_class_info_unicode:
                    unsafe extern "system" fn(*mut c_void, int32, *mut PClassInfoWData) -> tresult,
                set_host_context: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
            }

            static FACTORY_VTBL: FactoryVtbl = FactoryVtbl {
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
                if this.is_null() || iid.is_null() || obj.is_null() {
                    return kInvalidArgument;
                }
                *obj = std::ptr::null_mut();
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
                if this.is_null() {
                    return 0;
                }
                (*(this as *mut PluginFactoryObj))
                    .ref_count
                    .fetch_add(1, Ordering::SeqCst) as uint32
                    + 1
            }

            unsafe extern "system" fn factory_release(this: *mut c_void) -> uint32 {
                if this.is_null() {
                    return 0;
                }
                ((*(this as *mut PluginFactoryObj))
                    .ref_count
                    .fetch_sub(1, Ordering::SeqCst)
                    - 1) as uint32
            }

            unsafe extern "system" fn factory_get_factory_info(
                _this: *mut c_void,
                info: *mut PFactoryInfoData,
            ) -> tresult {
                if info.is_null() {
                    return kInvalidArgument;
                }
                let plugin_info = plugin_info();
                let info = &mut *info;
                strcpy_safe(&mut info.vendor, plugin_info.vendor.as_bytes());
                strcpy_safe(&mut info.url, plugin_info.url.as_bytes());
                strcpy_safe(&mut info.email, plugin_info.email.as_bytes());
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
                if info.is_null() {
                    return kInvalidArgument;
                }
                let plugin_info = plugin_info();
                let info = &mut *info;
                match index {
                    0 => {
                        info.cid = processor_cid();
                        info.cardinality = PClassInfo::kManyInstances;
                        strcpy_safe(&mut info.category, kVstAudioEffectClass);
                        strcpy_safe(&mut info.name, plugin_info.name.as_bytes());
                    }
                    1 => {
                        info.cid = controller_cid();
                        info.cardinality = PClassInfo::kManyInstances;
                        strcpy_safe(&mut info.category, kVstComponentControllerClass);
                        let name = format!("{} Controller", plugin_info.name);
                        strcpy_safe(&mut info.name, name.as_bytes());
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
                if info.is_null() {
                    return kInvalidArgument;
                }
                let plugin_info = plugin_info();
                let info = &mut *info;
                match index {
                    0 => {
                        info.cid = processor_cid();
                        info.cardinality = PClassInfo::kManyInstances;
                        strcpy_safe(&mut info.category, kVstAudioEffectClass);
                        strcpy_safe(&mut info.name, plugin_info.name.as_bytes());
                        info.class_flags = ComponentFlags::kSimpleModeSupported;
                        strcpy_safe(&mut info.sub_categories, plugin_info.category.as_bytes());
                        strcpy_safe(&mut info.vendor, plugin_info.vendor.as_bytes());
                        strcpy_safe(&mut info.version, plugin_info.version.as_bytes());
                        strcpy_safe(&mut info.sdk_version, b"VST 3.8.0\0");
                    }
                    1 => {
                        info.cid = controller_cid();
                        info.cardinality = PClassInfo::kManyInstances;
                        strcpy_safe(&mut info.category, kVstComponentControllerClass);
                        let name = format!("{} Controller", plugin_info.name);
                        strcpy_safe(&mut info.name, name.as_bytes());
                        info.class_flags = 0;
                        strcpy_safe(&mut info.sub_categories, b"\0");
                        strcpy_safe(&mut info.vendor, plugin_info.vendor.as_bytes());
                        strcpy_safe(&mut info.version, plugin_info.version.as_bytes());
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
                if cid.is_null() || requested_iid.is_null() || obj.is_null() {
                    return kInvalidArgument;
                }
                *obj = std::ptr::null_mut();
                let cid_bytes = std::slice::from_raw_parts(cid as *const i8, 16);
                let mut cid_arr: TUID = [0; 16];
                cid_arr.copy_from_slice(cid_bytes);

                let instance = if iid_equal(&cid_arr, &processor_cid()) {
                    $crate::wrapper::ProcessorWrapper::<$plugin_type>::new(controller_cid())
                        as *mut c_void
                } else if iid_equal(&cid_arr, &controller_cid()) {
                    $crate::wrapper::ControllerWrapper::<$plugin_type>::new() as *mut c_void
                } else {
                    *obj = std::ptr::null_mut();
                    return kNoInterface;
                };
                $crate::wrapper::factory_query_interface(
                    instance,
                    requested_iid as *const TUID,
                    obj,
                )
            }

            unsafe extern "system" fn factory_get_class_info_unicode(
                _this: *mut c_void,
                index: int32,
                info: *mut PClassInfoWData,
            ) -> tresult {
                if info.is_null() {
                    return kInvalidArgument;
                }
                let plugin_info = plugin_info();
                let info = &mut *info;
                match index {
                    0 => {
                        info.cid = processor_cid();
                        info.cardinality = PClassInfo::kManyInstances;
                        strcpy_safe(&mut info.category, kVstAudioEffectClass);
                        str16cpy(&mut info.name, plugin_info.name);
                        info.class_flags = ComponentFlags::kSimpleModeSupported;
                        strcpy_safe(&mut info.sub_categories, plugin_info.category.as_bytes());
                        str16cpy(&mut info.vendor, plugin_info.vendor);
                        str16cpy(&mut info.version, plugin_info.version);
                        str16cpy(&mut info.sdk_version, "VST 3.8.0");
                    }
                    1 => {
                        info.cid = controller_cid();
                        info.cardinality = PClassInfo::kManyInstances;
                        strcpy_safe(&mut info.category, kVstComponentControllerClass);
                        let name = format!("{} Controller", plugin_info.name);
                        str16cpy(&mut info.name, &name);
                        info.class_flags = 0;
                        strcpy_safe(&mut info.sub_categories, b"\0");
                        str16cpy(&mut info.vendor, plugin_info.vendor);
                        str16cpy(&mut info.version, plugin_info.version);
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

            #[unsafe(no_mangle)]
            pub extern "C" fn GetPluginFactory() -> *mut c_void {
                let factory = FACTORY.get_or_init(|| {
                    SendSyncPtr(Box::into_raw(Box::new(PluginFactoryObj {
                        vtbl: &FACTORY_VTBL,
                        ref_count: AtomicI32::new(0),
                    })))
                });
                unsafe { factory_add_ref(factory.0 as *mut c_void) };
                factory.0 as *mut c_void
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
        }
    };
}

/// Export macro for VST3 plugins with GUI support
#[macro_export]
macro_rules! export_vst3_plugin_with_gui {
    ($plugin_type:ty) => {
        mod __vst3_rs_impl {
            use super::*;
            use std::ffi::c_void;
            use std::sync::OnceLock;
            use std::sync::atomic::{AtomicI32, Ordering};
            use $crate::vst3_sys::*;

            fn processor_cid() -> TUID {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    <$plugin_type as $crate::Plugin>::class_id()
                }))
                .unwrap_or([0; 16])
            }

            fn controller_cid() -> TUID {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    <$plugin_type as $crate::Plugin>::controller_class_id()
                }))
                .unwrap_or([0; 16])
            }

            fn plugin_info() -> $crate::PluginInfo {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    <$plugin_type as $crate::Plugin>::info()
                }))
                .unwrap_or_default()
            }

            #[repr(C)]
            struct PluginFactoryObj {
                vtbl: *const FactoryVtbl,
                ref_count: AtomicI32,
            }

            #[repr(C)]
            struct FactoryVtbl {
                query_interface: unsafe extern "system" fn(
                    *mut c_void,
                    *const TUID,
                    *mut *mut c_void,
                ) -> tresult,
                add_ref: unsafe extern "system" fn(*mut c_void) -> uint32,
                release: unsafe extern "system" fn(*mut c_void) -> uint32,
                get_factory_info:
                    unsafe extern "system" fn(*mut c_void, *mut PFactoryInfoData) -> tresult,
                count_classes: unsafe extern "system" fn(*mut c_void) -> int32,
                get_class_info:
                    unsafe extern "system" fn(*mut c_void, int32, *mut PClassInfoData) -> tresult,
                create_instance: unsafe extern "system" fn(
                    *mut c_void,
                    FIDString,
                    FIDString,
                    *mut *mut c_void,
                ) -> tresult,
                get_class_info2:
                    unsafe extern "system" fn(*mut c_void, int32, *mut PClassInfo2Data) -> tresult,
                get_class_info_unicode:
                    unsafe extern "system" fn(*mut c_void, int32, *mut PClassInfoWData) -> tresult,
                set_host_context: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
            }

            static FACTORY_VTBL: FactoryVtbl = FactoryVtbl {
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
                if this.is_null() || iid.is_null() || obj.is_null() {
                    return kInvalidArgument;
                }
                *obj = std::ptr::null_mut();
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
                if this.is_null() {
                    return 0;
                }
                (*(this as *mut PluginFactoryObj))
                    .ref_count
                    .fetch_add(1, Ordering::SeqCst) as uint32
                    + 1
            }

            unsafe extern "system" fn factory_release(this: *mut c_void) -> uint32 {
                if this.is_null() {
                    return 0;
                }
                ((*(this as *mut PluginFactoryObj))
                    .ref_count
                    .fetch_sub(1, Ordering::SeqCst)
                    - 1) as uint32
            }

            unsafe extern "system" fn factory_get_factory_info(
                _this: *mut c_void,
                info: *mut PFactoryInfoData,
            ) -> tresult {
                if info.is_null() {
                    return kInvalidArgument;
                }
                let plugin_info = plugin_info();
                let info = &mut *info;
                strcpy_safe(&mut info.vendor, plugin_info.vendor.as_bytes());
                strcpy_safe(&mut info.url, plugin_info.url.as_bytes());
                strcpy_safe(&mut info.email, plugin_info.email.as_bytes());
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
                if info.is_null() {
                    return kInvalidArgument;
                }
                let plugin_info = plugin_info();
                let info = &mut *info;
                match index {
                    0 => {
                        info.cid = processor_cid();
                        info.cardinality = PClassInfo::kManyInstances;
                        strcpy_safe(&mut info.category, kVstAudioEffectClass);
                        strcpy_safe(&mut info.name, plugin_info.name.as_bytes());
                    }
                    1 => {
                        info.cid = controller_cid();
                        info.cardinality = PClassInfo::kManyInstances;
                        strcpy_safe(&mut info.category, kVstComponentControllerClass);
                        let name = format!("{} Controller", plugin_info.name);
                        strcpy_safe(&mut info.name, name.as_bytes());
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
                if info.is_null() {
                    return kInvalidArgument;
                }
                let plugin_info = plugin_info();
                let info = &mut *info;
                match index {
                    0 => {
                        info.cid = processor_cid();
                        info.cardinality = PClassInfo::kManyInstances;
                        strcpy_safe(&mut info.category, kVstAudioEffectClass);
                        strcpy_safe(&mut info.name, plugin_info.name.as_bytes());
                        info.class_flags = ComponentFlags::kSimpleModeSupported;
                        strcpy_safe(&mut info.sub_categories, plugin_info.category.as_bytes());
                        strcpy_safe(&mut info.vendor, plugin_info.vendor.as_bytes());
                        strcpy_safe(&mut info.version, plugin_info.version.as_bytes());
                        strcpy_safe(&mut info.sdk_version, b"VST 3.8.0\0");
                    }
                    1 => {
                        info.cid = controller_cid();
                        info.cardinality = PClassInfo::kManyInstances;
                        strcpy_safe(&mut info.category, kVstComponentControllerClass);
                        let name = format!("{} Controller", plugin_info.name);
                        strcpy_safe(&mut info.name, name.as_bytes());
                        info.class_flags = 0;
                        strcpy_safe(&mut info.sub_categories, b"\0");
                        strcpy_safe(&mut info.vendor, plugin_info.vendor.as_bytes());
                        strcpy_safe(&mut info.version, plugin_info.version.as_bytes());
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
                if cid.is_null() || requested_iid.is_null() || obj.is_null() {
                    return kInvalidArgument;
                }
                *obj = std::ptr::null_mut();
                let cid_bytes = std::slice::from_raw_parts(cid as *const i8, 16);
                let mut cid_arr: TUID = [0; 16];
                cid_arr.copy_from_slice(cid_bytes);

                let instance = if iid_equal(&cid_arr, &processor_cid()) {
                    $crate::wrapper::ProcessorWrapper::<$plugin_type>::new(controller_cid())
                        as *mut c_void
                } else if iid_equal(&cid_arr, &controller_cid()) {
                    $crate::wrapper::GuiControllerWrapper::<$plugin_type>::new() as *mut c_void
                } else {
                    *obj = std::ptr::null_mut();
                    return kNoInterface;
                };
                $crate::wrapper::factory_query_interface(
                    instance,
                    requested_iid as *const TUID,
                    obj,
                )
            }

            unsafe extern "system" fn factory_get_class_info_unicode(
                _this: *mut c_void,
                index: int32,
                info: *mut PClassInfoWData,
            ) -> tresult {
                if info.is_null() {
                    return kInvalidArgument;
                }
                let plugin_info = plugin_info();
                let info = &mut *info;
                match index {
                    0 => {
                        info.cid = processor_cid();
                        info.cardinality = PClassInfo::kManyInstances;
                        strcpy_safe(&mut info.category, kVstAudioEffectClass);
                        str16cpy(&mut info.name, plugin_info.name);
                        info.class_flags = ComponentFlags::kSimpleModeSupported;
                        strcpy_safe(&mut info.sub_categories, plugin_info.category.as_bytes());
                        str16cpy(&mut info.vendor, plugin_info.vendor);
                        str16cpy(&mut info.version, plugin_info.version);
                        str16cpy(&mut info.sdk_version, "VST 3.8.0");
                    }
                    1 => {
                        info.cid = controller_cid();
                        info.cardinality = PClassInfo::kManyInstances;
                        strcpy_safe(&mut info.category, kVstComponentControllerClass);
                        let name = format!("{} Controller", plugin_info.name);
                        str16cpy(&mut info.name, &name);
                        info.class_flags = 0;
                        strcpy_safe(&mut info.sub_categories, b"\0");
                        str16cpy(&mut info.vendor, plugin_info.vendor);
                        str16cpy(&mut info.version, plugin_info.version);
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

            #[unsafe(no_mangle)]
            pub extern "C" fn GetPluginFactory() -> *mut c_void {
                let factory = FACTORY.get_or_init(|| {
                    SendSyncPtr(Box::into_raw(Box::new(PluginFactoryObj {
                        vtbl: &FACTORY_VTBL,
                        ref_count: AtomicI32::new(0),
                    })))
                });
                unsafe { factory_add_ref(factory.0 as *mut c_void) };
                factory.0 as *mut c_void
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
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AudioConfig, PluginInfo, ProcessResult};
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64};

    struct TestAllocator;

    thread_local! {
        static ALLOCATOR_CALL_COUNT: Cell<isize> = const { Cell::new(-1) };
    }

    fn record_allocator_call() {
        let _ = ALLOCATOR_CALL_COUNT.try_with(|count| {
            let current = count.get();
            if current >= 0 {
                count.set(current + 1);
            }
        });
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
        let allocator_calls = ALLOCATOR_CALL_COUNT.with(|count| count.get() as usize);
        drop(scope);
        (result, allocator_calls)
    }

    unsafe fn begin_test_processing<P: Plugin>(
        processor: *mut ProcessorWrapper<P>,
        audio: *mut c_void,
    ) {
        let component = processor.cast::<c_void>();
        assert_eq!(
            ProcessorWrapper::<P>::processor_initialize(component, std::ptr::null_mut()),
            kResultOk
        );
        assert_eq!(
            ProcessorWrapper::<P>::processor_set_active(component, 1),
            kResultOk
        );
        assert_eq!(
            ProcessorWrapper::<P>::audio_set_processing(audio, 1),
            kResultOk
        );
    }

    #[repr(C)]
    struct TestComponentHandler {
        vtbl: *const IComponentHandlerVtbl,
        refs: AtomicU32,
        begin_id: AtomicU32,
        perform_id: AtomicU32,
        perform_value: AtomicU64,
        end_id: AtomicU32,
    }

    unsafe extern "system" fn host_query_interface(
        _this: *mut c_void,
        _iid: *const TUID,
        object: *mut *mut c_void,
    ) -> tresult {
        if !object.is_null() {
            unsafe { *object = std::ptr::null_mut() };
        }
        kNoInterface
    }

    unsafe extern "system" fn component_handler_add_ref(this: *mut c_void) -> uint32 {
        unsafe {
            (*(this as *mut TestComponentHandler))
                .refs
                .fetch_add(1, Ordering::SeqCst)
                + 1
        }
    }

    unsafe extern "system" fn component_handler_release(this: *mut c_void) -> uint32 {
        unsafe {
            (*(this as *mut TestComponentHandler))
                .refs
                .fetch_sub(1, Ordering::SeqCst)
                - 1
        }
    }

    unsafe extern "system" fn host_begin_edit(this: *mut c_void, id: ParamID) -> tresult {
        unsafe {
            (*(this as *mut TestComponentHandler))
                .begin_id
                .store(id, Ordering::SeqCst)
        };
        kResultOk
    }

    unsafe extern "system" fn host_perform_edit(
        this: *mut c_void,
        id: ParamID,
        value: ParamValue,
    ) -> tresult {
        let handler = unsafe { &*(this as *mut TestComponentHandler) };
        handler.perform_id.store(id, Ordering::SeqCst);
        handler
            .perform_value
            .store(value.to_bits(), Ordering::SeqCst);
        kResultOk
    }

    unsafe extern "system" fn host_end_edit(this: *mut c_void, id: ParamID) -> tresult {
        unsafe {
            (*(this as *mut TestComponentHandler))
                .end_id
                .store(id, Ordering::SeqCst)
        };
        kResultOk
    }

    unsafe extern "system" fn host_restart_component(_this: *mut c_void, _flags: int32) -> tresult {
        kResultOk
    }

    static TEST_COMPONENT_HANDLER_VTBL: IComponentHandlerVtbl = IComponentHandlerVtbl {
        unknown: IUnknownVtbl {
            query_interface: host_query_interface,
            add_ref: component_handler_add_ref,
            release: component_handler_release,
        },
        begin_edit: host_begin_edit,
        perform_edit: host_perform_edit,
        end_edit: host_end_edit,
        restart_component: host_restart_component,
    };

    #[repr(C)]
    struct TestPlugFrame {
        vtbl: *const IPlugFrameVtbl,
        refs: AtomicU32,
        view: AtomicPtr<c_void>,
        width: AtomicU32,
        height: AtomicU32,
    }

    unsafe extern "system" fn plug_frame_add_ref(this: *mut c_void) -> uint32 {
        unsafe {
            (*(this as *mut TestPlugFrame))
                .refs
                .fetch_add(1, Ordering::SeqCst)
                + 1
        }
    }

    unsafe extern "system" fn plug_frame_release(this: *mut c_void) -> uint32 {
        unsafe {
            (*(this as *mut TestPlugFrame))
                .refs
                .fetch_sub(1, Ordering::SeqCst)
                - 1
        }
    }

    unsafe extern "system" fn host_resize_view(
        this: *mut c_void,
        view: *mut c_void,
        size: *mut ViewRect,
    ) -> tresult {
        if size.is_null() {
            return kInvalidArgument;
        }
        let frame = unsafe { &*(this as *mut TestPlugFrame) };
        let size = unsafe { &*size };
        frame.view.store(view, Ordering::SeqCst);
        frame.width.store(size.width() as u32, Ordering::SeqCst);
        frame.height.store(size.height() as u32, Ordering::SeqCst);
        kResultOk
    }

    static TEST_PLUG_FRAME_VTBL: IPlugFrameVtbl = IPlugFrameVtbl {
        unknown: IUnknownVtbl {
            query_interface: host_query_interface,
            add_ref: plug_frame_add_ref,
            release: plug_frame_release,
        },
        resize_view: host_resize_view,
    };

    static FACTORY_PLUGIN_DROPS: AtomicU32 = AtomicU32::new(0);

    struct FactoryDropTestPlugin;

    impl Drop for FactoryDropTestPlugin {
        fn drop(&mut self) {
            FACTORY_PLUGIN_DROPS.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl Plugin for FactoryDropTestPlugin {
        fn info() -> crate::PluginInfo {
            crate::PluginInfo::default()
        }

        fn new(_host: HostHandle) -> Self {
            Self
        }

        fn get_param(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_param(&mut self, _id: u32, _value: f64) {}

        fn process(&mut self, _ctx: &mut ProcessContext) -> crate::ProcessResult {
            Ok(())
        }
    }

    struct HostGuiTestPlugin;

    impl Plugin for HostGuiTestPlugin {
        fn info() -> crate::PluginInfo {
            crate::PluginInfo::default()
        }

        fn new(_host: HostHandle) -> Self {
            Self
        }

        fn params() -> Vec<ParamInfo> {
            vec![ParamInfo::new(17, "Gain")]
        }

        fn get_param(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_param(&mut self, _id: u32, _value: f64) {}

        fn process(&mut self, _ctx: &mut ProcessContext) -> crate::ProcessResult {
            Ok(())
        }
    }

    impl GuiPlugin for HostGuiTestPlugin {
        fn gui_size() -> GuiSize {
            GuiSize::new(320, 200)
        }

        fn gui_create(&mut self, _parent: RawWindowHandle) -> bool {
            true
        }

        fn gui_destroy(&mut self) {}
    }

    mod exported_factory {
        use super::*;

        crate::export_vst3_plugin!(super::FactoryDropTestPlugin);

        pub fn get() -> *mut c_void {
            __vst3_rs_impl::GetPluginFactory()
        }
    }

    struct BridgeTestPlugin {
        parameters: Arc<ParameterBridge>,
    }

    impl Plugin for BridgeTestPlugin {
        fn info() -> crate::PluginInfo {
            crate::PluginInfo::default()
        }

        fn new(host: HostHandle) -> Self {
            Self {
                parameters: host.parameter_bridge(),
            }
        }

        fn params() -> Vec<ParamInfo> {
            vec![ParamInfo::new(17, "Gain").default(0.25)]
        }

        fn get_param(&self, id: u32) -> f64 {
            self.parameters.get(id)
        }

        fn set_param(&mut self, id: u32, value: f64) {
            self.parameters.set(id, value);
        }

        fn process(&mut self, _ctx: &mut ProcessContext) -> crate::ProcessResult {
            Ok(())
        }
    }

    static LAST_NOTE_OFFSET: AtomicU32 = AtomicU32::new(u32::MAX);
    static ZERO_FLUSH_VALUE: AtomicU64 = AtomicU64::new(0);
    static ZERO_FLUSH_PROCESS_CALLS: AtomicU32 = AtomicU32::new(0);
    static TIMED_PARAMETER_CHANGES: Mutex<Vec<crate::ParamChange>> = Mutex::new(Vec::new());
    static TIMED_PARAMETER_PARAMS_CALLS: AtomicU32 = AtomicU32::new(0);
    static TIMED_PARAMETER_PROCESS_CALLS: AtomicU32 = AtomicU32::new(0);
    static TIMED_PARAMETER_VALUE_DURING_PROCESS: AtomicU64 = AtomicU64::new(0);
    static TIMED_PARAMETER_FINAL_A: AtomicU64 = AtomicU64::new(0);
    static TIMED_PARAMETER_FINAL_B: AtomicU64 = AtomicU64::new(0);
    static OVERFLOW_PROCESS_CALLS: AtomicU32 = AtomicU32::new(0);
    static OVERFLOW_TEST_LOCK: Mutex<()> = Mutex::new(());

    struct MidiOffsetTestPlugin;

    impl Plugin for MidiOffsetTestPlugin {
        fn info() -> crate::PluginInfo {
            crate::PluginInfo::default()
        }

        fn new(_host: HostHandle) -> Self {
            Self
        }

        fn audio_config() -> crate::AudioConfig {
            crate::AudioConfig {
                inputs: Vec::new(),
                outputs: Vec::new(),
                accepts_midi: true,
            }
        }

        fn get_param(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_param(&mut self, _id: u32, _value: f64) {}

        fn note_on(&mut self, sample_offset: u32, _channel: i16, _pitch: i16, _velocity: f32) {
            LAST_NOTE_OFFSET.store(sample_offset, Ordering::SeqCst);
        }

        fn process(&mut self, _ctx: &mut ProcessContext) -> crate::ProcessResult {
            Ok(())
        }
    }

    struct ZeroFlushTestPlugin;

    impl Plugin for ZeroFlushTestPlugin {
        fn info() -> crate::PluginInfo {
            crate::PluginInfo::default()
        }

        fn new(_host: HostHandle) -> Self {
            Self
        }

        fn params() -> Vec<ParamInfo> {
            vec![ParamInfo::new(31, "Flush")]
        }

        fn get_param(&self, _id: u32) -> f64 {
            f64::from_bits(ZERO_FLUSH_VALUE.load(Ordering::SeqCst))
        }

        fn set_param(&mut self, _id: u32, value: f64) {
            ZERO_FLUSH_VALUE.store(value.to_bits(), Ordering::SeqCst);
        }

        fn process(&mut self, _ctx: &mut ProcessContext) -> crate::ProcessResult {
            ZERO_FLUSH_PROCESS_CALLS.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct TimedParameterTestPlugin;

    impl Plugin for TimedParameterTestPlugin {
        fn info() -> crate::PluginInfo {
            crate::PluginInfo::default()
        }

        fn new(_host: HostHandle) -> Self {
            Self
        }

        fn params() -> Vec<ParamInfo> {
            TIMED_PARAMETER_PARAMS_CALLS.fetch_add(1, Ordering::SeqCst);
            vec![
                ParamInfo::new(41, "Automated A"),
                ParamInfo::new(42, "Automated B"),
            ]
        }

        fn get_param(&self, id: u32) -> f64 {
            let bits = match id {
                41 => TIMED_PARAMETER_FINAL_A.load(Ordering::SeqCst),
                42 => TIMED_PARAMETER_FINAL_B.load(Ordering::SeqCst),
                _ => return 0.0,
            };
            f64::from_bits(bits)
        }

        fn set_param(&mut self, id: u32, value: f64) {
            match id {
                41 => TIMED_PARAMETER_FINAL_A.store(value.to_bits(), Ordering::SeqCst),
                42 => TIMED_PARAMETER_FINAL_B.store(value.to_bits(), Ordering::SeqCst),
                _ => {}
            }
        }

        fn process(&mut self, ctx: &mut ProcessContext) -> crate::ProcessResult {
            TIMED_PARAMETER_PROCESS_CALLS.fetch_add(1, Ordering::SeqCst);
            TIMED_PARAMETER_VALUE_DURING_PROCESS.store(
                TIMED_PARAMETER_FINAL_A.load(Ordering::SeqCst),
                Ordering::SeqCst,
            );
            *TIMED_PARAMETER_CHANGES.lock().unwrap() = ctx.param_changes().to_vec();
            Ok(())
        }
    }

    struct OverflowTestPlugin;

    impl Plugin for OverflowTestPlugin {
        const MAX_EVENTS_PER_BLOCK: usize = 2;

        fn info() -> crate::PluginInfo {
            crate::PluginInfo::default()
        }

        fn new(_host: HostHandle) -> Self {
            Self
        }

        fn audio_config() -> crate::AudioConfig {
            crate::AudioConfig {
                inputs: Vec::new(),
                outputs: Vec::new(),
                accepts_midi: true,
            }
        }

        fn params() -> Vec<ParamInfo> {
            vec![ParamInfo::new(77, "Bounded")]
        }

        fn get_param(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_param(&mut self, _id: u32, _value: f64) {}

        fn process(&mut self, _ctx: &mut ProcessContext) -> crate::ProcessResult {
            OVERFLOW_PROCESS_CALLS.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct MultiBusTestPlugin;

    impl Plugin for MultiBusTestPlugin {
        fn info() -> crate::PluginInfo {
            crate::PluginInfo::default()
        }

        fn new(_host: HostHandle) -> Self {
            Self
        }

        fn audio_config() -> crate::AudioConfig {
            crate::AudioConfig {
                inputs: vec![
                    crate::PortConfig::mono("Input A"),
                    crate::PortConfig {
                        port_type: crate::PortType::Aux,
                        ..crate::PortConfig::mono("Input B")
                    },
                ],
                outputs: vec![
                    crate::PortConfig::mono("Output A"),
                    crate::PortConfig {
                        port_type: crate::PortType::Aux,
                        ..crate::PortConfig::mono("Output B")
                    },
                ],
                accepts_midi: false,
            }
        }

        fn get_param(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_param(&mut self, _id: u32, _value: f64) {}

        fn process(&mut self, ctx: &mut ProcessContext) -> crate::ProcessResult {
            for sample in 0..ctx.num_samples {
                let a = ctx.input(0)[sample];
                let b = ctx.input(1)[sample];
                ctx.output_mut(0)[sample] = a * 2.0;
                ctx.output_mut(1)[sample] = b * 3.0;
            }
            Ok(())
        }
    }

    struct SurroundLayoutTestPlugin;

    impl Plugin for SurroundLayoutTestPlugin {
        fn info() -> crate::PluginInfo {
            crate::PluginInfo::default()
        }

        fn new(_host: HostHandle) -> Self {
            Self
        }

        fn audio_config() -> crate::AudioConfig {
            crate::AudioConfig {
                inputs: vec![crate::PortConfig {
                    name: "5.1 Input",
                    channels: 6,
                    port_type: crate::PortType::Main,
                    speaker_arrangement: Some(SpeakerArr::k51),
                }],
                outputs: vec![crate::PortConfig {
                    name: "7.1 Music Output",
                    channels: 8,
                    port_type: crate::PortType::Main,
                    speaker_arrangement: Some(SpeakerArr::k71Music),
                }],
                accepts_midi: false,
            }
        }

        fn get_param(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_param(&mut self, _id: u32, _value: f64) {}

        fn process(&mut self, _ctx: &mut ProcessContext) -> crate::ProcessResult {
            Ok(())
        }
    }

    #[repr(C)]
    struct TestEventList {
        vtbl: *const IEventListVtbl,
        event: Event,
    }

    #[repr(C)]
    struct TestParamQueue {
        vtbl: *const IParamValueQueueVtbl,
        id: ParamID,
        points: Vec<(int32, ParamValue)>,
    }

    #[repr(C)]
    struct TestParameterChanges {
        vtbl: *const IParameterChangesVtbl,
        queues: Vec<*mut c_void>,
    }

    unsafe extern "system" fn test_event_query_interface(
        _this: *mut c_void,
        _iid: *const TUID,
        object: *mut *mut c_void,
    ) -> tresult {
        if !object.is_null() {
            unsafe { *object = std::ptr::null_mut() };
        }
        kNoInterface
    }

    unsafe extern "system" fn test_event_add_ref(_this: *mut c_void) -> uint32 {
        1
    }

    unsafe extern "system" fn test_event_release(_this: *mut c_void) -> uint32 {
        1
    }

    unsafe extern "system" fn test_event_count(_this: *mut c_void) -> int32 {
        1
    }

    unsafe extern "system" fn test_event_get(
        this: *mut c_void,
        index: int32,
        event: *mut Event,
    ) -> tresult {
        if this.is_null() || event.is_null() || index != 0 {
            return kInvalidArgument;
        }
        unsafe { *event = (*(this as *const TestEventList)).event };
        kResultOk
    }

    unsafe extern "system" fn test_event_add(_this: *mut c_void, _event: *mut Event) -> tresult {
        kNotImplemented
    }

    unsafe extern "system" fn test_param_id(this: *mut c_void) -> ParamID {
        unsafe { (*(this as *const TestParamQueue)).id }
    }

    unsafe extern "system" fn test_param_point_count(this: *mut c_void) -> int32 {
        unsafe { (*(this as *const TestParamQueue)).points.len() as int32 }
    }

    unsafe extern "system" fn huge_param_point_count(_this: *mut c_void) -> int32 {
        int32::MAX
    }

    unsafe extern "system" fn test_param_get_point(
        this: *mut c_void,
        index: int32,
        offset: *mut int32,
        value: *mut ParamValue,
    ) -> tresult {
        if index < 0 || offset.is_null() || value.is_null() {
            return kInvalidArgument;
        }
        let queue = unsafe { &*(this as *const TestParamQueue) };
        let Some((point_offset, point_value)) = queue.points.get(index as usize).copied() else {
            return kInvalidArgument;
        };
        unsafe {
            *offset = point_offset;
            *value = point_value;
        }
        kResultOk
    }

    unsafe extern "system" fn test_param_add_point(
        _this: *mut c_void,
        _offset: int32,
        _value: ParamValue,
        _index: *mut int32,
    ) -> tresult {
        kNotImplemented
    }

    unsafe extern "system" fn test_parameter_count(this: *mut c_void) -> int32 {
        unsafe { (*(this as *const TestParameterChanges)).queues.len() as int32 }
    }

    unsafe extern "system" fn huge_parameter_count(_this: *mut c_void) -> int32 {
        int32::MAX
    }

    unsafe extern "system" fn test_parameter_data(this: *mut c_void, index: int32) -> *mut c_void {
        if index < 0 {
            return std::ptr::null_mut();
        }
        unsafe {
            (&(*(this as *const TestParameterChanges)).queues)
                .get(index as usize)
                .copied()
                .unwrap_or(std::ptr::null_mut())
        }
    }

    unsafe extern "system" fn test_add_parameter_data(
        _this: *mut c_void,
        _id: *const ParamID,
        _index: *mut int32,
    ) -> *mut c_void {
        std::ptr::null_mut()
    }

    static TEST_EVENT_LIST_VTBL: IEventListVtbl = IEventListVtbl {
        unknown: IUnknownVtbl {
            query_interface: test_event_query_interface,
            add_ref: test_event_add_ref,
            release: test_event_release,
        },
        get_event_count: test_event_count,
        get_event: test_event_get,
        add_event: test_event_add,
    };

    static TEST_PARAM_QUEUE_VTBL: IParamValueQueueVtbl = IParamValueQueueVtbl {
        unknown: IUnknownVtbl {
            query_interface: test_event_query_interface,
            add_ref: test_event_add_ref,
            release: test_event_release,
        },
        get_parameter_id: test_param_id,
        get_point_count: test_param_point_count,
        get_point: test_param_get_point,
        add_point: test_param_add_point,
    };

    static HUGE_PARAM_QUEUE_VTBL: IParamValueQueueVtbl = IParamValueQueueVtbl {
        unknown: IUnknownVtbl {
            query_interface: test_event_query_interface,
            add_ref: test_event_add_ref,
            release: test_event_release,
        },
        get_parameter_id: test_param_id,
        get_point_count: huge_param_point_count,
        get_point: test_param_get_point,
        add_point: test_param_add_point,
    };

    static TEST_PARAMETER_CHANGES_VTBL: IParameterChangesVtbl = IParameterChangesVtbl {
        unknown: IUnknownVtbl {
            query_interface: test_event_query_interface,
            add_ref: test_event_add_ref,
            release: test_event_release,
        },
        get_parameter_count: test_parameter_count,
        get_parameter_data: test_parameter_data,
        add_parameter_data: test_add_parameter_data,
    };

    static HUGE_PARAMETER_CHANGES_VTBL: IParameterChangesVtbl = IParameterChangesVtbl {
        unknown: IUnknownVtbl {
            query_interface: test_event_query_interface,
            add_ref: test_event_add_ref,
            release: test_event_release,
        },
        get_parameter_count: huge_parameter_count,
        get_parameter_data: test_parameter_data,
        add_parameter_data: test_add_parameter_data,
    };

    unsafe fn connection_from_processor<P: Plugin>(
        processor: *mut ProcessorWrapper<P>,
    ) -> *mut c_void {
        let mut connection = std::ptr::null_mut();
        let component = processor as *mut c_void;
        let vtbl = *(component as *const *const ComponentVtbl);
        assert_eq!(
            ((*vtbl).query_interface)(component, &vst_iid::IConnectionPoint, &mut connection),
            kResultOk
        );
        connection
    }

    unsafe fn connection_from_controller<P: Plugin>(
        controller: *mut ControllerWrapper<P>,
    ) -> *mut c_void {
        let mut connection = std::ptr::null_mut();
        let controller_ptr = controller as *mut c_void;
        let vtbl = *(controller_ptr as *const *const ControllerVtbl);
        assert_eq!(
            ((*vtbl).query_interface)(controller_ptr, &vst_iid::IConnectionPoint, &mut connection,),
            kResultOk
        );
        connection
    }

    unsafe fn connection_from_gui_controller<P: GuiPlugin>(
        controller: *mut GuiControllerWrapper<P>,
    ) -> *mut c_void {
        let mut connection = std::ptr::null_mut();
        let controller_ptr = controller as *mut c_void;
        let vtbl = *(controller_ptr as *const *const ControllerVtbl);
        assert_eq!(
            ((*vtbl).query_interface)(controller_ptr, &vst_iid::IConnectionPoint, &mut connection,),
            kResultOk
        );
        connection
    }

    #[test]
    fn malformed_vst3_note_fields_are_rejected_before_plugin_dispatch() {
        let mut event = Event {
            bus_index: 0,
            sample_offset: 0,
            ppq_position: 0.0,
            flags: 0,
            type_: EventTypes::kNoteOnEvent,
            event: EventData {
                note_on: NoteOnEvent {
                    channel: 0,
                    pitch: 60,
                    tuning: 0.0,
                    velocity: 0.5,
                    length: 0,
                    note_id: -1,
                },
            },
        };
        assert!(valid_note_event(&event, true));

        unsafe {
            event.event.note_on.channel = -1;
        }
        assert!(!valid_note_event(&event, true));

        unsafe {
            event.event.note_on.channel = 0;
            event.event.note_on.velocity = f32::NAN;
        }
        assert!(!valid_note_event(&event, true));

        unsafe {
            event.event.note_on.velocity = 0.5;
            event.bus_index = 1;
        }
        assert!(!valid_note_event(&event, true));
        assert!(!valid_note_event(&event, false));
    }

    #[test]
    fn factory_returns_the_requested_adjusted_interface_with_one_reference() {
        unsafe {
            let processor = ProcessorWrapper::<BridgeTestPlugin>::new([0; 16]);
            let expected_audio = std::ptr::addr_of_mut!((*processor).vtbl_audio).cast::<c_void>();
            let mut object = std::ptr::null_mut();

            assert_eq!(
                factory_query_interface(
                    processor.cast::<c_void>(),
                    &vst_iid::IAudioProcessor,
                    &mut object,
                ),
                kResultOk
            );
            assert_eq!(object, expected_audio);
            assert_ne!(object, processor.cast::<c_void>());
            assert_eq!((*processor).ref_count.load(Ordering::SeqCst), 1);

            let audio_vtbl = *(object as *const *const AudioProcessorVtbl);
            assert_eq!(((*audio_vtbl).release)(object), 0);
        }
    }

    #[test]
    fn factory_rejects_unsupported_interfaces_and_drops_the_instance() {
        const UNSUPPORTED_IID: TUID = vst3_sys::uid!(1, 2, 3, 4);

        unsafe {
            FACTORY_PLUGIN_DROPS.store(0, Ordering::SeqCst);
            let processor = ProcessorWrapper::<FactoryDropTestPlugin>::new([0; 16]);
            let mut object = std::ptr::dangling_mut::<c_void>();

            assert_eq!(
                factory_query_interface(processor.cast::<c_void>(), &UNSUPPORTED_IID, &mut object,),
                kNoInterface
            );
            assert!(object.is_null());
            assert_eq!(FACTORY_PLUGIN_DROPS.load(Ordering::SeqCst), 1);
        }
    }

    #[test]
    fn exported_factory_acquires_and_releases_each_caller_reference() {
        unsafe {
            let first = exported_factory::get();
            assert!(!first.is_null());
            let vtbl = *(first as *const *const IUnknownVtbl);
            let mut queried = std::ptr::null_mut();

            assert_eq!(
                ((*vtbl).query_interface)(first, &base_iid::IPluginFactory3, &mut queried),
                kResultOk
            );
            assert_eq!(queried, first);
            assert_eq!(((*vtbl).release)(queried), 1);
            assert_eq!(((*vtbl).release)(first), 0);

            let second = exported_factory::get();
            assert_eq!(second, first);
            assert_eq!(((*vtbl).release)(second), 0);
        }
    }

    #[test]
    fn processor_interfaces_share_one_canonical_iunknown_identity() {
        unsafe {
            let processor = ProcessorWrapper::<BridgeTestPlugin>::new([0; 16]);
            let component = processor.cast::<c_void>();
            let component_vtbl = *(component as *const *const ComponentVtbl);
            let audio = std::ptr::addr_of_mut!((*processor).vtbl_audio).cast::<c_void>();
            let audio_vtbl = *(audio as *const *const AudioProcessorVtbl);
            let mut component_unknown = std::ptr::null_mut();
            let mut audio_unknown = std::ptr::null_mut();

            assert_eq!(
                ((*component_vtbl).query_interface)(
                    component,
                    &iid::IUnknown,
                    &mut component_unknown,
                ),
                kResultOk
            );
            assert_eq!(
                ((*audio_vtbl).query_interface)(audio, &iid::IUnknown, &mut audio_unknown),
                kResultOk
            );
            assert_eq!(component_unknown, component);
            assert_eq!(audio_unknown, component_unknown);
            assert_eq!((*processor).ref_count.load(Ordering::SeqCst), 3);

            assert_eq!(((*component_vtbl).release)(component_unknown), 2);
            assert_eq!(((*component_vtbl).release)(audio_unknown), 1);
            assert_eq!(((*component_vtbl).release)(component), 0);
        }
    }

    #[test]
    fn connection_points_pair_each_controller_with_its_processor() {
        unsafe {
            let processor_a = ProcessorWrapper::<BridgeTestPlugin>::new([0; 16]);
            let processor_b = ProcessorWrapper::<BridgeTestPlugin>::new([0; 16]);
            let controller_a = ControllerWrapper::<BridgeTestPlugin>::new();
            let controller_b = ControllerWrapper::<BridgeTestPlugin>::new();

            let processor_connection_a = connection_from_processor(processor_a);
            let processor_connection_b = connection_from_processor(processor_b);
            let controller_connection_a = connection_from_controller(controller_a);
            let controller_connection_b = connection_from_controller(controller_b);

            let connection_vtbl_a = *(processor_connection_a as *const *const ConnectionPointVtbl);
            let connection_vtbl_b = *(processor_connection_b as *const *const ConnectionPointVtbl);
            assert_eq!(
                ((*connection_vtbl_a).connect)(processor_connection_a, controller_connection_a,),
                kResultOk
            );
            assert_eq!(
                ((*connection_vtbl_b).connect)(processor_connection_b, controller_connection_b,),
                kResultOk
            );

            assert_eq!(
                ControllerWrapper::<BridgeTestPlugin>::set_param_normalized(
                    controller_a as *mut c_void,
                    17,
                    0.8,
                ),
                kResultOk
            );
            assert_eq!((*processor_a).parameter_bridge.get(17), 0.8);
            assert_eq!((*processor_b).parameter_bridge.get(17), 0.25);

            ((*connection_vtbl_a).release)(processor_connection_a);
            ((*connection_vtbl_b).release)(processor_connection_b);
            let controller_connection_vtbl_a =
                *(controller_connection_a as *const *const ConnectionPointVtbl);
            let controller_connection_vtbl_b =
                *(controller_connection_b as *const *const ConnectionPointVtbl);
            ((*controller_connection_vtbl_a).release)(controller_connection_a);
            ((*controller_connection_vtbl_b).release)(controller_connection_b);
            ProcessorWrapper::<BridgeTestPlugin>::component_release(processor_a as *mut c_void);
            ProcessorWrapper::<BridgeTestPlugin>::component_release(processor_b as *mut c_void);
            ControllerWrapper::<BridgeTestPlugin>::release(controller_a as *mut c_void);
            ControllerWrapper::<BridgeTestPlugin>::release(controller_b as *mut c_void);
        }
    }

    #[test]
    fn connection_points_disconnect_and_reconnect_to_another_processor() {
        unsafe {
            let processor_a = ProcessorWrapper::<BridgeTestPlugin>::new([0; 16]);
            let processor_b = ProcessorWrapper::<BridgeTestPlugin>::new([0; 16]);
            let controller = ControllerWrapper::<BridgeTestPlugin>::new();

            let processor_connection_a = connection_from_processor(processor_a);
            let processor_connection_b = connection_from_processor(processor_b);
            let controller_connection = connection_from_controller(controller);
            let processor_vtbl_a = *(processor_connection_a as *const *const ConnectionPointVtbl);
            let processor_vtbl_b = *(processor_connection_b as *const *const ConnectionPointVtbl);
            let controller_vtbl = *(controller_connection as *const *const ConnectionPointVtbl);

            assert_eq!(
                ((*processor_vtbl_a).connect)(processor_connection_a, controller_connection),
                kResultOk
            );
            assert_eq!(
                ((*controller_vtbl).connect)(controller_connection, processor_connection_a),
                kResultOk
            );
            assert_eq!(
                ControllerWrapper::<BridgeTestPlugin>::set_param_normalized(
                    controller.cast(),
                    17,
                    0.8,
                ),
                kResultOk
            );
            assert_eq!((*processor_a).parameter_bridge.get(17), 0.8);

            assert_eq!(
                ((*processor_vtbl_a).disconnect)(processor_connection_a, controller_connection),
                kResultOk
            );
            assert_eq!(
                ((*controller_vtbl).disconnect)(controller_connection, processor_connection_a),
                kResultOk
            );
            assert_eq!((*controller).parameter_bridge.get(17), 0.8);
            assert_eq!(
                ControllerWrapper::<BridgeTestPlugin>::set_param_normalized(
                    controller.cast(),
                    17,
                    0.6,
                ),
                kResultOk
            );
            assert_eq!((*processor_a).parameter_bridge.get(17), 0.8);

            assert!((*processor_b).parameter_bridge.set(17, 0.4));
            assert_eq!(
                ((*processor_vtbl_b).connect)(processor_connection_b, controller_connection),
                kResultOk
            );
            assert_eq!(
                ((*controller_vtbl).connect)(controller_connection, processor_connection_b),
                kResultOk
            );
            assert_eq!((*controller).parameter_bridge.get(17), 0.4);
            assert_eq!(
                ControllerWrapper::<BridgeTestPlugin>::set_param_normalized(
                    controller.cast(),
                    17,
                    0.9,
                ),
                kResultOk
            );
            assert_eq!((*processor_a).parameter_bridge.get(17), 0.8);
            assert_eq!((*processor_b).parameter_bridge.get(17), 0.9);

            assert_eq!(
                ((*processor_vtbl_a).disconnect)(processor_connection_a, controller_connection),
                kResultFalse
            );
            assert_eq!((*controller).parameter_bridge.get(17), 0.9);
            assert_eq!(
                ((*processor_vtbl_b).disconnect)(processor_connection_b, controller_connection),
                kResultOk
            );
            assert_eq!(
                ((*controller_vtbl).disconnect)(controller_connection, processor_connection_b),
                kResultOk
            );

            ((*processor_vtbl_a).release)(processor_connection_a);
            ((*processor_vtbl_b).release)(processor_connection_b);
            ((*controller_vtbl).release)(controller_connection);
            ProcessorWrapper::<BridgeTestPlugin>::component_release(processor_a.cast());
            ProcessorWrapper::<BridgeTestPlugin>::component_release(processor_b.cast());
            ControllerWrapper::<BridgeTestPlugin>::release(controller.cast());
        }
    }

    #[test]
    fn gui_controller_connection_point_disconnects_from_its_processor() {
        unsafe {
            let processor = ProcessorWrapper::<HostGuiTestPlugin>::new([0; 16]);
            let controller = GuiControllerWrapper::<HostGuiTestPlugin>::new();
            let processor_connection = connection_from_processor(processor);
            let controller_connection = connection_from_gui_controller(controller);
            let processor_vtbl = *(processor_connection as *const *const ConnectionPointVtbl);
            let controller_vtbl = *(controller_connection as *const *const ConnectionPointVtbl);

            assert_eq!(
                ((*processor_vtbl).connect)(processor_connection, controller_connection),
                kResultOk
            );
            assert_eq!(
                ((*controller_vtbl).connect)(controller_connection, processor_connection),
                kResultOk
            );
            assert_eq!(
                GuiControllerWrapper::<HostGuiTestPlugin>::set_param_normalized(
                    controller.cast(),
                    17,
                    0.7,
                ),
                kResultOk
            );
            assert_eq!((*processor).parameter_bridge.get(17), 0.7);

            assert_eq!(
                ((*controller_vtbl).disconnect)(controller_connection, processor_connection),
                kResultOk
            );
            assert_eq!(
                ((*processor_vtbl).disconnect)(processor_connection, controller_connection),
                kResultOk
            );
            assert_eq!(
                GuiControllerWrapper::<HostGuiTestPlugin>::set_param_normalized(
                    controller.cast(),
                    17,
                    0.2,
                ),
                kResultOk
            );
            assert_eq!((*controller).parameter_bridge.get(17), 0.2);
            assert_eq!((*processor).parameter_bridge.get(17), 0.7);

            ((*processor_vtbl).release)(processor_connection);
            ((*controller_vtbl).release)(controller_connection);
            ProcessorWrapper::<HostGuiTestPlugin>::component_release(processor.cast());
            GuiControllerWrapper::<HostGuiTestPlugin>::release(controller.cast());
        }
    }

    #[test]
    fn component_handler_is_retained_and_receives_parameter_gestures() {
        unsafe {
            let controller = ControllerWrapper::<BridgeTestPlugin>::new();
            let mut handler = TestComponentHandler {
                vtbl: &TEST_COMPONENT_HANDLER_VTBL,
                refs: AtomicU32::new(1),
                begin_id: AtomicU32::new(u32::MAX),
                perform_id: AtomicU32::new(u32::MAX),
                perform_value: AtomicU64::new(0),
                end_id: AtomicU32::new(u32::MAX),
            };

            assert_eq!(
                ControllerWrapper::<BridgeTestPlugin>::set_component_handler(
                    controller.cast::<c_void>(),
                    (&mut handler as *mut TestComponentHandler).cast::<c_void>(),
                ),
                kResultOk
            );
            assert_eq!(handler.refs.load(Ordering::SeqCst), 2);

            let host = &(*controller).host;
            assert!(host.begin_edit(17));
            assert!(host.perform_edit(17, 1.5));
            assert!(!host.perform_edit(17, f64::NAN));
            assert!(host.end_edit(17));
            assert_eq!(handler.begin_id.load(Ordering::SeqCst), 17);
            assert_eq!(handler.perform_id.load(Ordering::SeqCst), 17);
            assert_eq!(
                f64::from_bits(handler.perform_value.load(Ordering::SeqCst)),
                1.0
            );
            assert_eq!(handler.end_id.load(Ordering::SeqCst), 17);

            assert_eq!(
                ControllerWrapper::<BridgeTestPlugin>::set_component_handler(
                    controller.cast::<c_void>(),
                    std::ptr::null_mut(),
                ),
                kResultOk
            );
            assert_eq!(handler.refs.load(Ordering::SeqCst), 1);
            assert!(!host.begin_edit(17));
            assert_eq!(
                ControllerWrapper::<BridgeTestPlugin>::release(controller.cast::<c_void>()),
                0
            );
        }
    }

    #[test]
    fn plug_frame_is_retained_and_receives_resize_requests() {
        unsafe {
            let controller = GuiControllerWrapper::<HostGuiTestPlugin>::new();
            let plugin = (*controller).plugin.as_mut().unwrap() as *mut HostGuiTestPlugin;
            let view = PlugViewWrapper::new(controller, plugin, (*controller).host.clone());
            assert_eq!((*controller).ref_count.load(Ordering::SeqCst), 2);
            let mut frame = TestPlugFrame {
                vtbl: &TEST_PLUG_FRAME_VTBL,
                refs: AtomicU32::new(1),
                view: AtomicPtr::new(std::ptr::null_mut()),
                width: AtomicU32::new(0),
                height: AtomicU32::new(0),
            };

            assert_eq!(
                PlugViewWrapper::<HostGuiTestPlugin>::set_frame(
                    view.cast::<c_void>(),
                    (&mut frame as *mut TestPlugFrame).cast::<c_void>(),
                ),
                kResultOk
            );
            assert_eq!(frame.refs.load(Ordering::SeqCst), 2);
            assert!((*controller).host.request_resize(640, 480));
            assert_eq!(frame.view.load(Ordering::SeqCst), view.cast::<c_void>());
            assert_eq!(frame.width.load(Ordering::SeqCst), 640);
            assert_eq!(frame.height.load(Ordering::SeqCst), 480);
            assert!(!(*controller).host.request_resize(u32::MAX, 480));

            assert_eq!(
                PlugViewWrapper::<HostGuiTestPlugin>::release(view.cast::<c_void>()),
                0
            );
            assert_eq!((*controller).ref_count.load(Ordering::SeqCst), 1);
            assert_eq!(frame.refs.load(Ordering::SeqCst), 1);
            assert!(!(*controller).host.request_resize(800, 600));
            assert_eq!(
                GuiControllerWrapper::<HostGuiTestPlugin>::release(controller.cast::<c_void>(),),
                0
            );
        }
    }

    #[test]
    fn validates_default_and_explicit_vst3_arrangements() {
        assert_eq!(default_speaker_arrangement(0), Some(SpeakerArr::kEmpty));
        assert_eq!(default_speaker_arrangement(1), Some(SpeakerArr::kMono));
        assert_eq!(default_speaker_arrangement(2), Some(SpeakerArr::kStereo));
        assert_eq!(default_speaker_arrangement(3), None);

        let surround = crate::PortConfig {
            name: "Surround",
            channels: 6,
            port_type: crate::PortType::Main,
            speaker_arrangement: Some(SpeakerArr::k51),
        };
        assert_eq!(speaker_arrangement(&surround), Some(SpeakerArr::k51));

        let invalid =
            crate::PortConfig::mono("Invalid").with_speaker_arrangement(SpeakerArr::kStereo);
        assert_eq!(speaker_arrangement(&invalid), None);
    }

    #[test]
    fn negotiates_explicit_multichannel_arrangements_through_the_audio_vtable() {
        unsafe {
            let processor = ProcessorWrapper::<SurroundLayoutTestPlugin>::new([0; 16]);
            let audio = std::ptr::addr_of_mut!((*processor).vtbl_audio).cast::<c_void>();
            let mut input = 0;
            let mut output = 0;

            assert_eq!(
                ProcessorWrapper::<SurroundLayoutTestPlugin>::audio_get_bus_arrangement(
                    audio,
                    BusDirections::kInput,
                    0,
                    &mut input,
                ),
                kResultOk
            );
            assert_eq!(input, SpeakerArr::k51);
            assert_eq!(
                ProcessorWrapper::<SurroundLayoutTestPlugin>::audio_get_bus_arrangement(
                    audio,
                    BusDirections::kOutput,
                    0,
                    &mut output,
                ),
                kResultOk
            );
            assert_eq!(output, SpeakerArr::k71Music);
            assert_eq!(
                ProcessorWrapper::<SurroundLayoutTestPlugin>::audio_set_bus_arrangements(
                    audio,
                    &mut input,
                    1,
                    &mut output,
                    1,
                ),
                kResultOk
            );

            input = SpeakerArr::k60Cine;
            assert_eq!(input.count_ones(), 6);
            assert_eq!(
                ProcessorWrapper::<SurroundLayoutTestPlugin>::audio_set_bus_arrangements(
                    audio,
                    &mut input,
                    1,
                    &mut output,
                    1,
                ),
                kResultFalse
            );

            ProcessorWrapper::<SurroundLayoutTestPlugin>::component_release(processor.cast());
        }
    }

    #[test]
    fn zero_sample_blocks_still_flush_parameter_changes() {
        unsafe {
            ZERO_FLUSH_VALUE.store(0.0_f64.to_bits(), Ordering::SeqCst);
            ZERO_FLUSH_PROCESS_CALLS.store(0, Ordering::SeqCst);
            let processor = ProcessorWrapper::<ZeroFlushTestPlugin>::new([0; 16]);
            let audio = std::ptr::addr_of_mut!((*processor).vtbl_audio).cast::<c_void>();
            let mut setup = ProcessSetup {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                max_samples_per_block: 1,
                sample_rate: 48_000.0,
            };
            assert_eq!(
                ProcessorWrapper::<ZeroFlushTestPlugin>::audio_setup_processing(audio, &mut setup,),
                kResultOk
            );
            begin_test_processing(processor, audio);
            let mut queue = TestParamQueue {
                vtbl: &TEST_PARAM_QUEUE_VTBL,
                id: 31,
                points: vec![(0, 0.75)],
            };
            let mut changes = TestParameterChanges {
                vtbl: &TEST_PARAMETER_CHANGES_VTBL,
                queues: vec![(&mut queue as *mut TestParamQueue).cast::<c_void>()],
            };
            let mut input = AudioBusBuffers {
                num_channels: 0,
                silence_flags: 0,
                buffers: std::ptr::null_mut(),
            };
            let mut output = AudioBusBuffers {
                num_channels: 0,
                silence_flags: 0,
                buffers: std::ptr::null_mut(),
            };
            let mut data = ProcessData {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                num_samples: 0,
                num_inputs: 1,
                num_outputs: 1,
                inputs: &mut input,
                outputs: &mut output,
                input_parameter_changes: (&mut changes as *mut TestParameterChanges)
                    .cast::<c_void>(),
                output_parameter_changes: std::ptr::null_mut(),
                input_events: std::ptr::null_mut(),
                output_events: std::ptr::null_mut(),
                process_context: std::ptr::null_mut(),
            };

            assert_eq!(
                ProcessorWrapper::<ZeroFlushTestPlugin>::audio_process(audio, &mut data),
                kResultOk
            );
            assert_eq!(
                f64::from_bits(ZERO_FLUSH_VALUE.load(Ordering::SeqCst)),
                0.75
            );
            assert_eq!(ZERO_FLUSH_PROCESS_CALLS.load(Ordering::SeqCst), 0);
            ProcessorWrapper::<ZeroFlushTestPlugin>::component_release(processor.cast::<c_void>());
        }
    }

    #[test]
    fn dense_parameter_input_returns_out_of_memory_before_dsp() {
        let _guard = OVERFLOW_TEST_LOCK.lock().unwrap();
        unsafe {
            OVERFLOW_PROCESS_CALLS.store(0, Ordering::SeqCst);
            let processor = ProcessorWrapper::<OverflowTestPlugin>::new([0; 16]);
            let audio = std::ptr::addr_of_mut!((*processor).vtbl_audio).cast::<c_void>();
            let mut setup = ProcessSetup {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                max_samples_per_block: 8,
                sample_rate: 48_000.0,
            };
            assert_eq!(
                ProcessorWrapper::<OverflowTestPlugin>::audio_setup_processing(audio, &mut setup),
                kResultOk
            );
            begin_test_processing(processor, audio);

            let mut queue = TestParamQueue {
                vtbl: &TEST_PARAM_QUEUE_VTBL,
                id: 77,
                points: vec![(0, 0.1), (1, 0.2), (2, 0.3)],
            };
            let mut changes = TestParameterChanges {
                vtbl: &TEST_PARAMETER_CHANGES_VTBL,
                queues: vec![(&mut queue as *mut TestParamQueue).cast::<c_void>()],
            };
            let mut data = ProcessData {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                num_samples: 8,
                num_inputs: 0,
                num_outputs: 0,
                inputs: std::ptr::null_mut(),
                outputs: std::ptr::null_mut(),
                input_parameter_changes: (&mut changes as *mut TestParameterChanges)
                    .cast::<c_void>(),
                output_parameter_changes: std::ptr::null_mut(),
                input_events: std::ptr::null_mut(),
                output_events: std::ptr::null_mut(),
                process_context: std::ptr::null_mut(),
            };

            let (status, allocator_calls) = count_allocator_calls(|| {
                ProcessorWrapper::<OverflowTestPlugin>::audio_process(audio, &mut data)
            });
            assert_eq!(status, kOutOfMemory);
            assert_eq!(allocator_calls, 0);
            assert_eq!(OVERFLOW_PROCESS_CALLS.load(Ordering::SeqCst), 0);
            assert_eq!((*processor).process_ctx.as_ref().unwrap().max_events(), 2);
            ProcessorWrapper::<OverflowTestPlugin>::component_release(processor.cast::<c_void>());
        }
    }

    #[test]
    fn huge_parameter_queue_count_is_rejected_before_traversal() {
        let _guard = OVERFLOW_TEST_LOCK.lock().unwrap();
        unsafe {
            OVERFLOW_PROCESS_CALLS.store(0, Ordering::SeqCst);
            let processor = ProcessorWrapper::<OverflowTestPlugin>::new([0; 16]);
            let audio = std::ptr::addr_of_mut!((*processor).vtbl_audio).cast::<c_void>();
            let mut setup = ProcessSetup {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                max_samples_per_block: 8,
                sample_rate: 48_000.0,
            };
            assert_eq!(
                ProcessorWrapper::<OverflowTestPlugin>::audio_setup_processing(audio, &mut setup),
                kResultOk
            );
            begin_test_processing(processor, audio);

            let mut changes = TestParameterChanges {
                vtbl: &HUGE_PARAMETER_CHANGES_VTBL,
                queues: Vec::new(),
            };
            let mut data = ProcessData {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                num_samples: 8,
                num_inputs: 0,
                num_outputs: 0,
                inputs: std::ptr::null_mut(),
                outputs: std::ptr::null_mut(),
                input_parameter_changes: (&mut changes as *mut TestParameterChanges)
                    .cast::<c_void>(),
                output_parameter_changes: std::ptr::null_mut(),
                input_events: std::ptr::null_mut(),
                output_events: std::ptr::null_mut(),
                process_context: std::ptr::null_mut(),
            };

            let (status, allocator_calls) = count_allocator_calls(|| {
                ProcessorWrapper::<OverflowTestPlugin>::audio_process(audio, &mut data)
            });
            assert_eq!(status, kOutOfMemory);
            assert_eq!(allocator_calls, 0);
            assert_eq!(OVERFLOW_PROCESS_CALLS.load(Ordering::SeqCst), 0);
            ProcessorWrapper::<OverflowTestPlugin>::component_release(processor.cast::<c_void>());
        }
    }

    #[test]
    fn huge_known_parameter_point_count_is_rejected_before_get_point() {
        let _guard = OVERFLOW_TEST_LOCK.lock().unwrap();
        unsafe {
            OVERFLOW_PROCESS_CALLS.store(0, Ordering::SeqCst);
            let processor = ProcessorWrapper::<OverflowTestPlugin>::new([0; 16]);
            let audio = std::ptr::addr_of_mut!((*processor).vtbl_audio).cast::<c_void>();
            let mut setup = ProcessSetup {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                max_samples_per_block: 8,
                sample_rate: 48_000.0,
            };
            assert_eq!(
                ProcessorWrapper::<OverflowTestPlugin>::audio_setup_processing(audio, &mut setup),
                kResultOk
            );
            begin_test_processing(processor, audio);

            let mut queue = TestParamQueue {
                vtbl: &HUGE_PARAM_QUEUE_VTBL,
                id: 77,
                points: Vec::new(),
            };
            let mut changes = TestParameterChanges {
                vtbl: &TEST_PARAMETER_CHANGES_VTBL,
                queues: vec![(&mut queue as *mut TestParamQueue).cast::<c_void>()],
            };
            let mut data = ProcessData {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                num_samples: 8,
                num_inputs: 0,
                num_outputs: 0,
                inputs: std::ptr::null_mut(),
                outputs: std::ptr::null_mut(),
                input_parameter_changes: (&mut changes as *mut TestParameterChanges)
                    .cast::<c_void>(),
                output_parameter_changes: std::ptr::null_mut(),
                input_events: std::ptr::null_mut(),
                output_events: std::ptr::null_mut(),
                process_context: std::ptr::null_mut(),
            };

            let (status, allocator_calls) = count_allocator_calls(|| {
                ProcessorWrapper::<OverflowTestPlugin>::audio_process(audio, &mut data)
            });
            assert_eq!(status, kOutOfMemory);
            assert_eq!(allocator_calls, 0);
            assert_eq!(OVERFLOW_PROCESS_CALLS.load(Ordering::SeqCst), 0);
            ProcessorWrapper::<OverflowTestPlugin>::component_release(processor.cast::<c_void>());
        }
    }

    #[test]
    fn in_budget_parameter_processing_does_not_use_the_allocator() {
        let _guard = OVERFLOW_TEST_LOCK.lock().unwrap();
        unsafe {
            OVERFLOW_PROCESS_CALLS.store(0, Ordering::SeqCst);
            let processor = ProcessorWrapper::<OverflowTestPlugin>::new([0; 16]);
            let audio = std::ptr::addr_of_mut!((*processor).vtbl_audio).cast::<c_void>();
            let mut setup = ProcessSetup {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                max_samples_per_block: 8,
                sample_rate: 48_000.0,
            };
            assert_eq!(
                ProcessorWrapper::<OverflowTestPlugin>::audio_setup_processing(audio, &mut setup),
                kResultOk
            );
            begin_test_processing(processor, audio);

            let mut queue = TestParamQueue {
                vtbl: &TEST_PARAM_QUEUE_VTBL,
                id: 77,
                points: vec![(7, 0.1), (1, 0.2)],
            };
            let mut changes = TestParameterChanges {
                vtbl: &TEST_PARAMETER_CHANGES_VTBL,
                queues: vec![(&mut queue as *mut TestParamQueue).cast::<c_void>()],
            };
            let mut data = ProcessData {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                num_samples: 8,
                num_inputs: 0,
                num_outputs: 0,
                inputs: std::ptr::null_mut(),
                outputs: std::ptr::null_mut(),
                input_parameter_changes: (&mut changes as *mut TestParameterChanges)
                    .cast::<c_void>(),
                output_parameter_changes: std::ptr::null_mut(),
                input_events: std::ptr::null_mut(),
                output_events: std::ptr::null_mut(),
                process_context: std::ptr::null_mut(),
            };

            let (status, allocator_calls) = count_allocator_calls(|| {
                ProcessorWrapper::<OverflowTestPlugin>::audio_process(audio, &mut data)
            });
            assert_eq!(status, kResultOk);
            assert_eq!(allocator_calls, 0);
            assert_eq!(OVERFLOW_PROCESS_CALLS.load(Ordering::SeqCst), 1);
            assert_eq!(
                (*processor).process_ctx.as_ref().unwrap().param_changes(),
                [
                    crate::ParamChange {
                        sample_offset: 1,
                        id: 77,
                        value: 0.2,
                    },
                    crate::ParamChange {
                        sample_offset: 7,
                        id: 77,
                        value: 0.1,
                    },
                ]
            );
            ProcessorWrapper::<OverflowTestPlugin>::component_release(processor.cast::<c_void>());
        }
    }

    #[test]
    fn combined_parameter_and_midi_input_returns_out_of_memory_before_dsp() {
        let _guard = OVERFLOW_TEST_LOCK.lock().unwrap();
        unsafe {
            OVERFLOW_PROCESS_CALLS.store(0, Ordering::SeqCst);
            let processor = ProcessorWrapper::<OverflowTestPlugin>::new([0; 16]);
            let audio = std::ptr::addr_of_mut!((*processor).vtbl_audio).cast::<c_void>();
            let mut setup = ProcessSetup {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                max_samples_per_block: 8,
                sample_rate: 48_000.0,
            };
            assert_eq!(
                ProcessorWrapper::<OverflowTestPlugin>::audio_setup_processing(audio, &mut setup),
                kResultOk
            );
            begin_test_processing(processor, audio);

            let mut queue = TestParamQueue {
                vtbl: &TEST_PARAM_QUEUE_VTBL,
                id: 77,
                points: vec![(0, 0.1), (1, 0.2)],
            };
            let mut changes = TestParameterChanges {
                vtbl: &TEST_PARAMETER_CHANGES_VTBL,
                queues: vec![(&mut queue as *mut TestParamQueue).cast::<c_void>()],
            };
            let mut events = TestEventList {
                vtbl: &TEST_EVENT_LIST_VTBL,
                event: Event {
                    bus_index: 0,
                    sample_offset: 2,
                    ppq_position: 0.0,
                    flags: 0,
                    type_: EventTypes::kNoteOnEvent,
                    event: EventData {
                        note_on: NoteOnEvent {
                            channel: 0,
                            pitch: 60,
                            tuning: 0.0,
                            velocity: 1.0,
                            length: 0,
                            note_id: 1,
                        },
                    },
                },
            };
            let mut data = ProcessData {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                num_samples: 8,
                num_inputs: 0,
                num_outputs: 0,
                inputs: std::ptr::null_mut(),
                outputs: std::ptr::null_mut(),
                input_parameter_changes: (&mut changes as *mut TestParameterChanges)
                    .cast::<c_void>(),
                output_parameter_changes: std::ptr::null_mut(),
                input_events: (&mut events as *mut TestEventList).cast::<c_void>(),
                output_events: std::ptr::null_mut(),
                process_context: std::ptr::null_mut(),
            };

            let (status, allocator_calls) = count_allocator_calls(|| {
                ProcessorWrapper::<OverflowTestPlugin>::audio_process(audio, &mut data)
            });
            assert_eq!(status, kOutOfMemory);
            assert_eq!(allocator_calls, 0);
            assert_eq!(OVERFLOW_PROCESS_CALLS.load(Ordering::SeqCst), 0);
            assert_eq!(
                (*processor)
                    .process_ctx
                    .as_ref()
                    .unwrap()
                    .param_changes()
                    .len(),
                2
            );
            ProcessorWrapper::<OverflowTestPlugin>::component_release(processor.cast::<c_void>());
        }
    }

    #[test]
    fn merges_every_parameter_point_without_preapplying_the_final_value() {
        unsafe {
            TIMED_PARAMETER_CHANGES.lock().unwrap().clear();
            TIMED_PARAMETER_PARAMS_CALLS.store(0, Ordering::SeqCst);
            TIMED_PARAMETER_PROCESS_CALLS.store(0, Ordering::SeqCst);
            TIMED_PARAMETER_VALUE_DURING_PROCESS.store(0.0_f64.to_bits(), Ordering::SeqCst);
            TIMED_PARAMETER_FINAL_A.store(0.0_f64.to_bits(), Ordering::SeqCst);
            TIMED_PARAMETER_FINAL_B.store(0.0_f64.to_bits(), Ordering::SeqCst);

            let processor = ProcessorWrapper::<TimedParameterTestPlugin>::new([0; 16]);
            let audio = std::ptr::addr_of_mut!((*processor).vtbl_audio).cast::<c_void>();
            let mut setup = ProcessSetup {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                max_samples_per_block: 16,
                sample_rate: 48_000.0,
            };
            assert_eq!(
                ProcessorWrapper::<TimedParameterTestPlugin>::audio_setup_processing(
                    audio, &mut setup,
                ),
                kResultOk
            );
            begin_test_processing(processor, audio);

            let mut queue_a = TestParamQueue {
                vtbl: &TEST_PARAM_QUEUE_VTBL,
                id: 41,
                points: vec![(7, 0.1), (3, 0.2), (7, 0.3)],
            };
            let mut queue_b = TestParamQueue {
                vtbl: &TEST_PARAM_QUEUE_VTBL,
                id: 42,
                points: vec![(-4, 0.4), (7, 0.5), (99, 0.6)],
            };
            let mut duplicate_queue_a = TestParamQueue {
                vtbl: &TEST_PARAM_QUEUE_VTBL,
                id: 41,
                points: vec![(1, 0.9)],
            };
            let mut changes = TestParameterChanges {
                vtbl: &TEST_PARAMETER_CHANGES_VTBL,
                queues: vec![
                    (&mut queue_a as *mut TestParamQueue).cast::<c_void>(),
                    (&mut queue_b as *mut TestParamQueue).cast::<c_void>(),
                    (&mut duplicate_queue_a as *mut TestParamQueue).cast::<c_void>(),
                ],
            };
            let mut data = ProcessData {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                num_samples: 16,
                num_inputs: 0,
                num_outputs: 0,
                inputs: std::ptr::null_mut(),
                outputs: std::ptr::null_mut(),
                input_parameter_changes: (&mut changes as *mut TestParameterChanges)
                    .cast::<c_void>(),
                output_parameter_changes: std::ptr::null_mut(),
                input_events: std::ptr::null_mut(),
                output_events: std::ptr::null_mut(),
                process_context: std::ptr::null_mut(),
            };

            assert_eq!(
                ProcessorWrapper::<TimedParameterTestPlugin>::audio_process(audio, &mut data),
                kResultOk
            );

            assert_eq!(TIMED_PARAMETER_PARAMS_CALLS.load(Ordering::SeqCst), 1);
            assert_eq!(TIMED_PARAMETER_PROCESS_CALLS.load(Ordering::SeqCst), 1);
            assert_eq!(
                f64::from_bits(TIMED_PARAMETER_VALUE_DURING_PROCESS.load(Ordering::SeqCst)),
                0.0
            );
            assert_eq!(
                *TIMED_PARAMETER_CHANGES.lock().unwrap(),
                vec![
                    crate::ParamChange {
                        sample_offset: 0,
                        id: 42,
                        value: 0.4,
                    },
                    crate::ParamChange {
                        sample_offset: 1,
                        id: 41,
                        value: 0.9,
                    },
                    crate::ParamChange {
                        sample_offset: 3,
                        id: 41,
                        value: 0.2,
                    },
                    crate::ParamChange {
                        sample_offset: 7,
                        id: 41,
                        value: 0.1,
                    },
                    crate::ParamChange {
                        sample_offset: 7,
                        id: 41,
                        value: 0.3,
                    },
                    crate::ParamChange {
                        sample_offset: 7,
                        id: 42,
                        value: 0.5,
                    },
                    crate::ParamChange {
                        sample_offset: 15,
                        id: 42,
                        value: 0.6,
                    },
                ]
            );
            assert_eq!(
                f64::from_bits(TIMED_PARAMETER_FINAL_A.load(Ordering::SeqCst)),
                0.3
            );
            assert_eq!(
                f64::from_bits(TIMED_PARAMETER_FINAL_B.load(Ordering::SeqCst)),
                0.6
            );
            assert_eq!((*processor).parameter_bridge.get(41), 0.3);
            assert_eq!((*processor).parameter_bridge.get(42), 0.6);

            ProcessorWrapper::<TimedParameterTestPlugin>::component_release(
                processor.cast::<c_void>(),
            );
        }
    }

    #[test]
    fn process_rejects_blocks_larger_than_the_negotiated_maximum() {
        unsafe {
            let processor = ProcessorWrapper::<BridgeTestPlugin>::new([0; 16]);
            let audio = std::ptr::addr_of_mut!((*processor).vtbl_audio).cast::<c_void>();
            let mut setup = ProcessSetup {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                max_samples_per_block: 1024,
                sample_rate: 48_000.0,
            };
            assert_eq!(
                ProcessorWrapper::<BridgeTestPlugin>::audio_setup_processing(audio, &mut setup),
                kResultOk
            );
            begin_test_processing(processor, audio);
            let mut data = ProcessData {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                num_samples: 1025,
                num_inputs: 0,
                num_outputs: 0,
                inputs: std::ptr::null_mut(),
                outputs: std::ptr::null_mut(),
                input_parameter_changes: std::ptr::null_mut(),
                output_parameter_changes: std::ptr::null_mut(),
                input_events: std::ptr::null_mut(),
                output_events: std::ptr::null_mut(),
                process_context: std::ptr::null_mut(),
            };

            assert_eq!(
                ProcessorWrapper::<BridgeTestPlugin>::audio_process(audio, &mut data),
                kInvalidArgument
            );
            ProcessorWrapper::<BridgeTestPlugin>::component_release(processor.cast::<c_void>());
        }
    }

    #[test]
    fn process_requires_the_vst3_active_and_processing_lifecycle() {
        unsafe {
            let processor = ProcessorWrapper::<BridgeTestPlugin>::new([0; 16]);
            let component = processor.cast::<c_void>();
            let audio = std::ptr::addr_of_mut!((*processor).vtbl_audio).cast::<c_void>();
            let mut data = ProcessData {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                num_samples: 0,
                num_inputs: 0,
                num_outputs: 0,
                inputs: std::ptr::null_mut(),
                outputs: std::ptr::null_mut(),
                input_parameter_changes: std::ptr::null_mut(),
                output_parameter_changes: std::ptr::null_mut(),
                input_events: std::ptr::null_mut(),
                output_events: std::ptr::null_mut(),
                process_context: std::ptr::null_mut(),
            };

            assert_eq!(
                ProcessorWrapper::<BridgeTestPlugin>::audio_process(audio, &mut data),
                kNotInitialized
            );

            let mut setup = ProcessSetup {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                max_samples_per_block: 8,
                sample_rate: 48_000.0,
            };
            assert_eq!(
                ProcessorWrapper::<BridgeTestPlugin>::audio_setup_processing(audio, &mut setup),
                kResultOk
            );
            assert_eq!(
                ProcessorWrapper::<BridgeTestPlugin>::audio_process(audio, &mut data),
                kNotInitialized
            );
            assert_eq!(
                ProcessorWrapper::<BridgeTestPlugin>::processor_initialize(
                    component,
                    std::ptr::null_mut(),
                ),
                kResultOk
            );
            assert_eq!(
                ProcessorWrapper::<BridgeTestPlugin>::audio_set_processing(audio, 1),
                kNotInitialized
            );
            assert_eq!(
                ProcessorWrapper::<BridgeTestPlugin>::processor_set_active(component, 1),
                kResultOk
            );
            assert_eq!(
                ProcessorWrapper::<BridgeTestPlugin>::audio_process(audio, &mut data),
                kResultFalse
            );
            assert_eq!(
                ProcessorWrapper::<BridgeTestPlugin>::audio_set_processing(audio, 1),
                kResultOk
            );
            assert_eq!(
                ProcessorWrapper::<BridgeTestPlugin>::audio_process(audio, &mut data),
                kResultOk
            );
            assert_eq!(
                ProcessorWrapper::<BridgeTestPlugin>::audio_set_processing(audio, 0),
                kResultOk
            );
            assert_eq!(
                ProcessorWrapper::<BridgeTestPlugin>::audio_process(audio, &mut data),
                kResultFalse
            );
            assert_eq!(
                ProcessorWrapper::<BridgeTestPlugin>::audio_set_processing(audio, 1),
                kResultOk
            );
            assert_eq!(
                ProcessorWrapper::<BridgeTestPlugin>::processor_set_active(component, 0),
                kResultOk
            );
            assert_eq!(
                ProcessorWrapper::<BridgeTestPlugin>::audio_process(audio, &mut data),
                kResultFalse
            );
            assert_eq!(
                ProcessorWrapper::<BridgeTestPlugin>::audio_set_processing(audio, 1),
                kNotInitialized
            );

            ProcessorWrapper::<BridgeTestPlugin>::component_release(component);
        }
    }

    #[test]
    fn setup_processing_rejects_invalid_host_configuration() {
        unsafe {
            let processor = ProcessorWrapper::<BridgeTestPlugin>::new([0; 16]);
            let audio = std::ptr::addr_of_mut!((*processor).vtbl_audio).cast::<c_void>();

            assert_eq!(
                ProcessorWrapper::<BridgeTestPlugin>::audio_setup_processing(
                    audio,
                    std::ptr::null_mut(),
                ),
                kInvalidArgument
            );

            for (max_samples_per_block, sample_rate) in [(-1, 48_000.0), (64, 0.0), (64, f64::NAN)]
            {
                let mut setup = ProcessSetup {
                    process_mode: ProcessModes::kRealtime,
                    symbolic_sample_size: SymbolicSampleSizes::kSample32,
                    max_samples_per_block,
                    sample_rate,
                };
                assert_eq!(
                    ProcessorWrapper::<BridgeTestPlugin>::audio_setup_processing(audio, &mut setup,),
                    kInvalidArgument
                );
            }

            let mut unsupported_setup = ProcessSetup {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample64,
                max_samples_per_block: 64,
                sample_rate: 48_000.0,
            };
            assert_eq!(
                ProcessorWrapper::<BridgeTestPlugin>::audio_setup_processing(
                    audio,
                    &mut unsupported_setup,
                ),
                kResultFalse
            );

            let mut oversized_setup = ProcessSetup {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                max_samples_per_block: (MAX_PROCESS_FRAMES + 1) as i32,
                sample_rate: 48_000.0,
            };
            assert_eq!(
                ProcessorWrapper::<BridgeTestPlugin>::audio_setup_processing(
                    audio,
                    &mut oversized_setup,
                ),
                kOutOfMemory
            );

            assert!((*processor).process_ctx.is_none());

            ProcessorWrapper::<BridgeTestPlugin>::component_release(processor.cast::<c_void>());
        }
    }

    #[test]
    fn process_rejects_malformed_audio_bus_storage() {
        unsafe {
            let processor = ProcessorWrapper::<MultiBusTestPlugin>::new([0; 16]);
            assert_eq!(
                ProcessorWrapper::<MultiBusTestPlugin>::processor_get_bus_info(
                    std::ptr::null_mut(),
                    MediaTypes::kAudio,
                    BusDirections::kInput,
                    0,
                    &mut BusInfo::default(),
                ),
                kInvalidArgument
            );
            let audio = std::ptr::addr_of_mut!((*processor).vtbl_audio).cast::<c_void>();
            let mut setup = ProcessSetup {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                max_samples_per_block: 8,
                sample_rate: 48_000.0,
            };
            assert_eq!(
                ProcessorWrapper::<MultiBusTestPlugin>::audio_setup_processing(audio, &mut setup,),
                kResultOk
            );
            begin_test_processing(processor, audio);

            let mut data = ProcessData {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                num_samples: 1,
                num_inputs: 1,
                num_outputs: 0,
                inputs: std::ptr::null_mut(),
                outputs: std::ptr::null_mut(),
                input_parameter_changes: std::ptr::null_mut(),
                output_parameter_changes: std::ptr::null_mut(),
                input_events: std::ptr::null_mut(),
                output_events: std::ptr::null_mut(),
                process_context: std::ptr::null_mut(),
            };
            assert_eq!(
                ProcessorWrapper::<MultiBusTestPlugin>::audio_process(audio, &mut data),
                kInvalidArgument
            );

            let mut bus = AudioBusBuffers {
                num_channels: 1,
                silence_flags: 0,
                buffers: std::ptr::null_mut(),
            };
            data.inputs = &mut bus;
            assert_eq!(
                ProcessorWrapper::<MultiBusTestPlugin>::audio_process(audio, &mut data),
                kInvalidArgument
            );

            data.inputs = &mut bus;
            data.num_inputs = 3;
            assert_eq!(
                ProcessorWrapper::<MultiBusTestPlugin>::audio_process(audio, &mut data),
                kInvalidArgument
            );

            data.num_inputs = 0;
            data.inputs = std::ptr::null_mut();
            data.num_outputs = 1;
            data.outputs = std::ptr::null_mut();
            assert_eq!(
                ProcessorWrapper::<MultiBusTestPlugin>::audio_process(audio, &mut data),
                kInvalidArgument
            );

            ProcessorWrapper::<MultiBusTestPlugin>::component_release(processor.cast::<c_void>());
        }
    }

    #[test]
    fn bus_queries_reject_invalid_host_arguments() {
        unsafe {
            let processor = ProcessorWrapper::<MultiBusTestPlugin>::new([0; 16]);
            assert_eq!(
                ProcessorWrapper::<MultiBusTestPlugin>::processor_get_bus_info(
                    processor.cast::<c_void>(),
                    MediaTypes::kAudio,
                    BusDirections::kInput,
                    0,
                    std::ptr::null_mut(),
                ),
                kInvalidArgument
            );
            let mut bus = BusInfo::default();
            assert_eq!(
                ProcessorWrapper::<MultiBusTestPlugin>::processor_get_bus_info(
                    processor.cast::<c_void>(),
                    MediaTypes::kAudio,
                    -1,
                    0,
                    &mut bus,
                ),
                kInvalidArgument
            );
            assert_eq!(
                ProcessorWrapper::<MultiBusTestPlugin>::processor_get_bus_info(
                    processor.cast::<c_void>(),
                    MediaTypes::kAudio,
                    BusDirections::kInput,
                    1,
                    &mut bus,
                ),
                kResultOk
            );
            assert_eq!(bus.bus_type, BusTypes::kAux);
            assert_eq!(
                ProcessorWrapper::<MultiBusTestPlugin>::processor_get_bus_count(
                    processor.cast::<c_void>(),
                    MediaTypes::kAudio,
                    -1,
                ),
                0
            );
            ProcessorWrapper::<MultiBusTestPlugin>::component_release(processor.cast::<c_void>());
        }
    }

    #[test]
    fn process_flattens_all_audio_buses_in_declared_order() {
        unsafe {
            let processor = ProcessorWrapper::<MultiBusTestPlugin>::new([0; 16]);
            let audio = std::ptr::addr_of_mut!((*processor).vtbl_audio).cast::<c_void>();
            let mut setup = ProcessSetup {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                max_samples_per_block: 2,
                sample_rate: 48_000.0,
            };
            assert_eq!(
                ProcessorWrapper::<MultiBusTestPlugin>::audio_setup_processing(audio, &mut setup,),
                kResultOk
            );
            begin_test_processing(processor, audio);

            let input_a = [1.0_f32, 2.0];
            let input_b = [10.0_f32, 20.0];
            let mut input_a_channels = [input_a.as_ptr() as *mut c_void];
            let mut input_b_channels = [input_b.as_ptr() as *mut c_void];
            let mut inputs = [
                AudioBusBuffers {
                    num_channels: 1,
                    silence_flags: 0,
                    buffers: input_a_channels.as_mut_ptr(),
                },
                AudioBusBuffers {
                    num_channels: 1,
                    silence_flags: 0,
                    buffers: input_b_channels.as_mut_ptr(),
                },
            ];
            let mut output_a = [0.0_f32; 2];
            let mut output_b = [0.0_f32; 2];
            let mut output_a_channels = [output_a.as_mut_ptr().cast::<c_void>()];
            let mut output_b_channels = [output_b.as_mut_ptr().cast::<c_void>()];
            let mut outputs = [
                AudioBusBuffers {
                    num_channels: 1,
                    silence_flags: 0,
                    buffers: output_a_channels.as_mut_ptr(),
                },
                AudioBusBuffers {
                    num_channels: 1,
                    silence_flags: 0,
                    buffers: output_b_channels.as_mut_ptr(),
                },
            ];
            let mut data = ProcessData {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                num_samples: 2,
                num_inputs: inputs.len() as int32,
                num_outputs: outputs.len() as int32,
                inputs: inputs.as_mut_ptr(),
                outputs: outputs.as_mut_ptr(),
                input_parameter_changes: std::ptr::null_mut(),
                output_parameter_changes: std::ptr::null_mut(),
                input_events: std::ptr::null_mut(),
                output_events: std::ptr::null_mut(),
                process_context: std::ptr::null_mut(),
            };

            assert_eq!(
                ProcessorWrapper::<MultiBusTestPlugin>::audio_process(audio, &mut data),
                kResultOk
            );
            assert_eq!(output_a, [2.0, 4.0]);
            assert_eq!(output_b, [30.0, 60.0]);
            ProcessorWrapper::<MultiBusTestPlugin>::component_release(processor.cast::<c_void>());
        }
    }

    #[test]
    fn forwards_vst3_note_sample_offsets_to_plugins() {
        unsafe {
            LAST_NOTE_OFFSET.store(u32::MAX, Ordering::SeqCst);
            let processor = ProcessorWrapper::<MidiOffsetTestPlugin>::new([0; 16]);
            let audio = std::ptr::addr_of_mut!((*processor).vtbl_audio).cast::<c_void>();
            let mut setup = ProcessSetup {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                max_samples_per_block: 64,
                sample_rate: 48_000.0,
            };
            assert_eq!(
                ProcessorWrapper::<MidiOffsetTestPlugin>::audio_setup_processing(audio, &mut setup),
                kResultOk
            );
            begin_test_processing(processor, audio);
            let mut events = TestEventList {
                vtbl: &TEST_EVENT_LIST_VTBL,
                event: Event {
                    bus_index: 0,
                    sample_offset: 23,
                    ppq_position: 0.0,
                    flags: 0,
                    type_: EventTypes::kNoteOnEvent,
                    event: EventData {
                        note_on: NoteOnEvent {
                            channel: 2,
                            pitch: 64,
                            tuning: 0.0,
                            velocity: 0.75,
                            length: 0,
                            note_id: 7,
                        },
                    },
                },
            };
            let mut data = ProcessData {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                num_samples: 64,
                num_inputs: 0,
                num_outputs: 0,
                inputs: std::ptr::null_mut(),
                outputs: std::ptr::null_mut(),
                input_parameter_changes: std::ptr::null_mut(),
                output_parameter_changes: std::ptr::null_mut(),
                input_events: &mut events as *mut TestEventList as *mut c_void,
                output_events: std::ptr::null_mut(),
                process_context: std::ptr::null_mut(),
            };

            assert_eq!(
                ProcessorWrapper::<MidiOffsetTestPlugin>::audio_process(audio, &mut data),
                kResultOk
            );
            assert_eq!(LAST_NOTE_OFFSET.load(Ordering::SeqCst), 23);
            ProcessorWrapper::<MidiOffsetTestPlugin>::component_release(processor as *mut c_void);
        }
    }

    #[test]
    fn process_rejects_an_event_list_with_no_vtable() {
        unsafe {
            let processor = ProcessorWrapper::<MidiOffsetTestPlugin>::new([0; 16]);
            let audio = std::ptr::addr_of_mut!((*processor).vtbl_audio).cast::<c_void>();
            let mut setup = ProcessSetup {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                max_samples_per_block: 1,
                sample_rate: 48_000.0,
            };
            assert_eq!(
                ProcessorWrapper::<MidiOffsetTestPlugin>::audio_setup_processing(audio, &mut setup,),
                kResultOk
            );
            begin_test_processing(processor, audio);
            let mut events = TestEventList {
                vtbl: std::ptr::null(),
                event: Event::default(),
            };
            let mut data = ProcessData {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                num_samples: 1,
                num_inputs: 0,
                num_outputs: 0,
                inputs: std::ptr::null_mut(),
                outputs: std::ptr::null_mut(),
                input_parameter_changes: std::ptr::null_mut(),
                output_parameter_changes: std::ptr::null_mut(),
                input_events: (&mut events as *mut TestEventList).cast::<c_void>(),
                output_events: std::ptr::null_mut(),
                process_context: std::ptr::null_mut(),
            };
            assert_eq!(
                ProcessorWrapper::<MidiOffsetTestPlugin>::audio_process(audio, &mut data),
                kInvalidArgument
            );
            ProcessorWrapper::<MidiOffsetTestPlugin>::component_release(processor.cast::<c_void>());
        }
    }

    struct PanickingConstructorVstPlugin;

    impl Plugin for PanickingConstructorVstPlugin {
        fn info() -> PluginInfo {
            PluginInfo {
                id: "com.example.panicking-constructor",
                name: "Panicking Constructor",
                ..PluginInfo::default()
            }
        }

        fn new(_host: HostHandle) -> Self {
            panic!("intentional VST3 constructor panic");
        }

        fn get_param(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_param(&mut self, _id: u32, _value: f64) {}

        fn process(&mut self, _ctx: &mut ProcessContext) -> ProcessResult {
            Ok(())
        }
    }

    #[test]
    fn processor_constructor_panics_return_null() {
        assert!(ProcessorWrapper::<PanickingConstructorVstPlugin>::new([0; 16]).is_null());
    }

    struct PanickingInitVstPlugin;

    impl Plugin for PanickingInitVstPlugin {
        fn info() -> PluginInfo {
            PluginInfo {
                id: "com.example.panicking-init",
                name: "Panicking Init",
                ..PluginInfo::default()
            }
        }

        fn new(_host: HostHandle) -> Self {
            Self
        }

        fn audio_config() -> AudioConfig {
            AudioConfig {
                inputs: Vec::new(),
                outputs: Vec::new(),
                accepts_midi: false,
            }
        }

        fn init(&mut self) -> bool {
            panic!("intentional VST3 initialize panic");
        }

        fn get_param(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_param(&mut self, _id: u32, _value: f64) {}

        fn process(&mut self, _ctx: &mut ProcessContext) -> ProcessResult {
            Ok(())
        }
    }

    struct PanickingProcessVstPlugin;

    impl Plugin for PanickingProcessVstPlugin {
        fn info() -> PluginInfo {
            PluginInfo {
                id: "com.example.panicking-process",
                name: "Panicking Process",
                ..PluginInfo::default()
            }
        }

        fn new(_host: HostHandle) -> Self {
            Self
        }

        fn audio_config() -> AudioConfig {
            AudioConfig {
                inputs: Vec::new(),
                outputs: Vec::new(),
                accepts_midi: false,
            }
        }

        fn get_param(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_param(&mut self, _id: u32, _value: f64) {}

        fn process(&mut self, _ctx: &mut ProcessContext) -> ProcessResult {
            panic!("intentional VST3 process panic");
        }
    }

    #[test]
    fn raw_lifecycle_and_process_callbacks_contain_panics() {
        unsafe {
            let processor = ProcessorWrapper::<PanickingInitVstPlugin>::new([0; 16]);
            assert_eq!(
                ProcessorWrapper::<PanickingInitVstPlugin>::processor_initialize(
                    processor.cast::<c_void>(),
                    std::ptr::null_mut(),
                ),
                kInternalError
            );
            ProcessorWrapper::<PanickingInitVstPlugin>::component_release(processor.cast());

            let processor = ProcessorWrapper::<PanickingProcessVstPlugin>::new([0; 16]);
            let component = processor.cast::<c_void>();
            assert_eq!(
                ProcessorWrapper::<PanickingProcessVstPlugin>::processor_initialize(
                    component,
                    std::ptr::null_mut(),
                ),
                kResultOk
            );
            let audio = std::ptr::addr_of_mut!((*processor).vtbl_audio).cast::<c_void>();
            let mut setup = ProcessSetup {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                max_samples_per_block: 1,
                sample_rate: 48_000.0,
            };
            assert_eq!(
                ProcessorWrapper::<PanickingProcessVstPlugin>::audio_setup_processing(
                    audio, &mut setup,
                ),
                kResultOk
            );
            begin_test_processing(processor, audio);
            let mut data = ProcessData {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                num_samples: 1,
                num_inputs: 0,
                num_outputs: 0,
                inputs: std::ptr::null_mut(),
                outputs: std::ptr::null_mut(),
                input_parameter_changes: std::ptr::null_mut(),
                output_parameter_changes: std::ptr::null_mut(),
                input_events: std::ptr::null_mut(),
                output_events: std::ptr::null_mut(),
                process_context: std::ptr::null_mut(),
            };
            assert_eq!(
                ProcessorWrapper::<PanickingProcessVstPlugin>::audio_process(audio, &mut data),
                kInternalError
            );
            // A failed block must disable processing so a host cannot keep
            // calling a processor whose state may have been left poisoned.
            assert_eq!(
                ProcessorWrapper::<PanickingProcessVstPlugin>::audio_process(audio, &mut data),
                kResultFalse
            );
            assert!(!(*processor).processing);
            ProcessorWrapper::<PanickingProcessVstPlugin>::component_release(component);
        }
    }

    struct PanickingControllerSetParamPlugin;

    impl Plugin for PanickingControllerSetParamPlugin {
        fn info() -> PluginInfo {
            PluginInfo::default()
        }

        fn new(_host: HostHandle) -> Self {
            Self
        }

        fn params() -> Vec<ParamInfo> {
            vec![ParamInfo::new(91, "Panics")]
        }

        fn get_param(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_param(&mut self, _id: u32, _value: f64) {
            panic!("intentional controller parameter panic");
        }

        fn process(&mut self, _ctx: &mut ProcessContext) -> ProcessResult {
            Ok(())
        }
    }

    impl GuiPlugin for PanickingControllerSetParamPlugin {
        fn gui_size() -> GuiSize {
            GuiSize::new(320, 200)
        }

        fn gui_create(&mut self, _parent: RawWindowHandle) -> bool {
            true
        }

        fn gui_destroy(&mut self) {}
    }

    static SANITIZED_GUI_PARAM_VALUE: AtomicU64 = AtomicU64::new(0);

    struct SanitizingGuiParamPlugin;

    impl Plugin for SanitizingGuiParamPlugin {
        fn info() -> PluginInfo {
            PluginInfo::default()
        }

        fn new(_host: HostHandle) -> Self {
            Self
        }

        fn params() -> Vec<ParamInfo> {
            vec![ParamInfo::new(92, "Sanitized").default(0.25)]
        }

        fn get_param(&self, _id: u32) -> f64 {
            f64::from_bits(SANITIZED_GUI_PARAM_VALUE.load(Ordering::SeqCst))
        }

        fn set_param(&mut self, _id: u32, value: f64) {
            SANITIZED_GUI_PARAM_VALUE.store(value.to_bits(), Ordering::SeqCst);
        }

        fn process(&mut self, _ctx: &mut ProcessContext) -> ProcessResult {
            Ok(())
        }
    }

    impl GuiPlugin for SanitizingGuiParamPlugin {
        fn gui_size() -> GuiSize {
            GuiSize::new(320, 200)
        }

        fn gui_create(&mut self, _parent: RawWindowHandle) -> bool {
            true
        }

        fn gui_destroy(&mut self) {}
    }

    #[test]
    fn gui_controller_parameter_panics_are_contained_and_not_published() {
        unsafe {
            let controller = ControllerWrapper::<PanickingControllerSetParamPlugin>::new();
            let result = ((*(*controller).vtbl).set_param_normalized)(controller.cast(), 91, 0.75);
            // The non-GUI controller intentionally has no plugin instance;
            // it only publishes values into its shared bridge.
            assert_eq!(result, kResultOk);
            assert_eq!((*controller).parameter_bridge.get(91), 0.75);
            ControllerWrapper::<PanickingControllerSetParamPlugin>::release(controller.cast());

            let controller = GuiControllerWrapper::<PanickingControllerSetParamPlugin>::new();
            let result = ((*(*controller).vtbl).set_param_normalized)(controller.cast(), 91, 0.75);
            assert_eq!(result, kInternalError);
            assert_eq!((*controller).parameter_bridge.get(91), 0.0);
            GuiControllerWrapper::<PanickingControllerSetParamPlugin>::release(controller.cast());
        }
    }

    #[test]
    fn gui_controller_parameter_values_are_sanitized_before_user_callback() {
        unsafe {
            SANITIZED_GUI_PARAM_VALUE.store(0.25_f64.to_bits(), Ordering::SeqCst);
            let controller = GuiControllerWrapper::<SanitizingGuiParamPlugin>::new();
            let set_param = (*(*controller).vtbl).set_param_normalized;

            assert_eq!(set_param(controller.cast(), 92, f64::NAN), kResultOk);
            assert_eq!(
                f64::from_bits(SANITIZED_GUI_PARAM_VALUE.load(Ordering::SeqCst)),
                0.25
            );
            assert_eq!(set_param(controller.cast(), 92, f64::INFINITY), kResultOk);
            assert_eq!(
                f64::from_bits(SANITIZED_GUI_PARAM_VALUE.load(Ordering::SeqCst)),
                0.25
            );
            assert_eq!(set_param(controller.cast(), 92, -2.0), kResultOk);
            assert_eq!(
                f64::from_bits(SANITIZED_GUI_PARAM_VALUE.load(Ordering::SeqCst)),
                0.0
            );
            assert_eq!(set_param(controller.cast(), 92, 2.0), kResultOk);
            assert_eq!(
                f64::from_bits(SANITIZED_GUI_PARAM_VALUE.load(Ordering::SeqCst)),
                1.0
            );
            assert_eq!((*controller).parameter_bridge.get(92), 1.0);
            GuiControllerWrapper::<SanitizingGuiParamPlugin>::release(controller.cast());
        }
    }

    // ======= Bus activation =======

    static BUS_ACTIVATIONS: Mutex<Vec<(bool, u32, bool)>> = Mutex::new(Vec::new());
    /// `BUS_ACTIVATIONS` is process-global, so the tests that inspect it must
    /// not run concurrently with each other.
    static BUS_ACTIVATION_TEST_LOCK: Mutex<()> = Mutex::new(());

    struct BusActivationPlugin;

    impl Plugin for BusActivationPlugin {
        fn info() -> PluginInfo {
            PluginInfo::default()
        }

        fn new(_host: HostHandle) -> Self {
            Self
        }

        fn audio_config() -> AudioConfig {
            AudioConfig {
                inputs: vec![
                    crate::PortConfig::stereo("Main In"),
                    crate::PortConfig {
                        name: "Sidechain",
                        channels: 2,
                        port_type: crate::PortType::Aux,
                        speaker_arrangement: Some(SpeakerArr::kStereo),
                    },
                ],
                outputs: vec![crate::PortConfig::stereo("Main Out")],
                accepts_midi: true,
            }
        }

        fn activate_bus(&mut self, is_input: bool, bus_index: u32, active: bool) -> bool {
            BUS_ACTIVATIONS
                .lock()
                .unwrap()
                .push((is_input, bus_index, active));
            // The sidechain may be refused; the main buses may not.
            !(is_input && bus_index == 1 && active)
        }

        fn get_param(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_param(&mut self, _id: u32, _value: f64) {}

        fn process(&mut self, _ctx: &mut ProcessContext) -> ProcessResult {
            Ok(())
        }
    }

    #[test]
    fn activate_bus_validates_the_index_and_forwards_to_the_plugin() {
        let _serialize = BUS_ACTIVATION_TEST_LOCK.lock().unwrap();
        unsafe {
            BUS_ACTIVATIONS.lock().unwrap().clear();
            let processor = ProcessorWrapper::<BusActivationPlugin>::new([0; 16]);
            let component = processor.cast::<c_void>();
            assert_eq!(
                ProcessorWrapper::<BusActivationPlugin>::processor_initialize(
                    component,
                    std::ptr::null_mut()
                ),
                kResultOk
            );

            let activate_bus = ProcessorWrapper::<BusActivationPlugin>::processor_activate_bus;

            // Declared audio buses reach the plugin.
            assert_eq!(
                activate_bus(component, MediaTypes::kAudio, BusDirections::kInput, 0, 1),
                kResultOk
            );
            assert_eq!(
                activate_bus(component, MediaTypes::kAudio, BusDirections::kOutput, 0, 0),
                kResultOk
            );
            // A refusal from the plugin surfaces as kResultFalse, not a silent
            // acceptance.
            assert_eq!(
                activate_bus(component, MediaTypes::kAudio, BusDirections::kInput, 1, 1),
                kResultFalse
            );
            // Deactivating the same sidechain is allowed.
            assert_eq!(
                activate_bus(component, MediaTypes::kAudio, BusDirections::kInput, 1, 0),
                kResultOk
            );

            assert_eq!(
                BUS_ACTIVATIONS.lock().unwrap().as_slice(),
                &[
                    (true, 0, true),
                    (false, 0, false),
                    (true, 1, true),
                    (true, 1, false)
                ]
            );

            // Out-of-range and undeclared buses are rejected before the
            // callback runs.
            let before = BUS_ACTIVATIONS.lock().unwrap().len();
            assert_eq!(
                activate_bus(component, MediaTypes::kAudio, BusDirections::kInput, 2, 1),
                kInvalidArgument
            );
            assert_eq!(
                activate_bus(component, MediaTypes::kAudio, BusDirections::kOutput, 1, 1),
                kInvalidArgument
            );
            assert_eq!(
                activate_bus(component, MediaTypes::kAudio, BusDirections::kInput, -1, 1),
                kInvalidArgument
            );
            assert_eq!(BUS_ACTIVATIONS.lock().unwrap().len(), before);

            // The single event bus is handled by the wrapper itself.
            assert_eq!(
                activate_bus(component, MediaTypes::kEvent, BusDirections::kInput, 0, 1),
                kResultOk
            );
            assert_eq!(
                activate_bus(component, MediaTypes::kEvent, BusDirections::kInput, 1, 1),
                kInvalidArgument
            );
            assert_eq!(BUS_ACTIVATIONS.lock().unwrap().len(), before);

            ProcessorWrapper::<BusActivationPlugin>::component_release(component);
        }
    }

    #[test]
    fn activate_bus_before_initialize_reports_not_initialized() {
        let _serialize = BUS_ACTIVATION_TEST_LOCK.lock().unwrap();
        unsafe {
            BUS_ACTIVATIONS.lock().unwrap().clear();
            let processor = ProcessorWrapper::<BusActivationPlugin>::new([0; 16]);
            let component = processor.cast::<c_void>();
            assert_eq!(
                ProcessorWrapper::<BusActivationPlugin>::processor_activate_bus(
                    component,
                    MediaTypes::kAudio,
                    BusDirections::kInput,
                    0,
                    1,
                ),
                kNotInitialized
            );
            assert!(BUS_ACTIVATIONS.lock().unwrap().is_empty());
            ProcessorWrapper::<BusActivationPlugin>::component_release(component);
        }
    }

    /// Bus counts the next `PropBusPlugin::audio_config` will report, as
    /// (inputs, outputs).
    static PROP_BUS_COUNTS: Mutex<(usize, usize)> = Mutex::new((0, 0));
    /// Bus indices the plugin callback actually saw.
    static PROP_BUS_FORWARDED: Mutex<Vec<(bool, u32)>> = Mutex::new(Vec::new());
    /// The two statics above are process-global.
    static PROP_BUS_LOCK: Mutex<()> = Mutex::new(());

    struct PropBusPlugin;

    impl Plugin for PropBusPlugin {
        fn info() -> PluginInfo {
            PluginInfo::default()
        }

        fn new(_host: HostHandle) -> Self {
            Self
        }

        fn audio_config() -> AudioConfig {
            let (inputs, outputs) = *PROP_BUS_COUNTS.lock().unwrap();
            AudioConfig {
                inputs: (0..inputs)
                    .map(|_| crate::PortConfig::stereo("In"))
                    .collect(),
                outputs: (0..outputs)
                    .map(|_| crate::PortConfig::stereo("Out"))
                    .collect(),
                accepts_midi: true,
            }
        }

        fn activate_bus(&mut self, is_input: bool, bus_index: u32, _active: bool) -> bool {
            PROP_BUS_FORWARDED
                .lock()
                .unwrap()
                .push((is_input, bus_index));
            true
        }

        fn get_param(&self, _id: u32) -> f64 {
            0.0
        }

        fn set_param(&mut self, _id: u32, _value: f64) {}

        fn process(&mut self, _ctx: &mut ProcessContext) -> ProcessResult {
            Ok(())
        }
    }

    proptest::proptest! {
        /// For any declared bus topology and any index a host may pass —
        /// including negative and out-of-range ones — `activateBus` must
        /// forward exactly the in-range audio buses and reject the rest. An
        /// undeclared bus must never reach the plugin, and a declared one must
        /// never be silently accepted without the plugin being told.
        #[test]
        fn activate_bus_forwards_exactly_the_declared_indices(
            inputs in 0usize..4,
            outputs in 0usize..4,
            probe_is_input in proptest::prelude::any::<bool>(),
            probe_index in -2i32..8,
            probe_active in proptest::prelude::any::<bool>(),
        ) {
            let _serialize = PROP_BUS_LOCK.lock().unwrap();
            *PROP_BUS_COUNTS.lock().unwrap() = (inputs, outputs);
            PROP_BUS_FORWARDED.lock().unwrap().clear();

            unsafe {
                let processor = ProcessorWrapper::<PropBusPlugin>::new([0; 16]);
                let component = processor.cast::<c_void>();
                let init = ProcessorWrapper::<PropBusPlugin>::processor_initialize(
                    component,
                    std::ptr::null_mut(),
                );
                proptest::prop_assert_eq!(init, kResultOk);

                let dir = if probe_is_input {
                    BusDirections::kInput
                } else {
                    BusDirections::kOutput
                };
                let result = ProcessorWrapper::<PropBusPlugin>::processor_activate_bus(
                    component,
                    MediaTypes::kAudio,
                    dir,
                    probe_index,
                    probe_active as TBool,
                );

                let declared = if probe_is_input { inputs } else { outputs };
                let in_range = probe_index >= 0 && (probe_index as usize) < declared;
                let forwarded = PROP_BUS_FORWARDED.lock().unwrap().clone();

                if in_range {
                    proptest::prop_assert_eq!(result, kResultOk);
                    proptest::prop_assert_eq!(
                        forwarded.as_slice(),
                        &[(probe_is_input, probe_index as u32)]
                    );
                } else {
                    proptest::prop_assert_eq!(
                        result,
                        kInvalidArgument,
                        "index {} with {} declared bus(es) in that direction",
                        probe_index,
                        declared
                    );
                    proptest::prop_assert!(
                        forwarded.is_empty(),
                        "an undeclared bus must not reach the plugin"
                    );
                }

                ProcessorWrapper::<PropBusPlugin>::component_release(component);
            }
        }
    }
}
