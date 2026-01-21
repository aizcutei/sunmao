//! Internal wrapper module for COM vtable generation
//!
//! This module contains the unsafe FFI glue that converts Plugin trait
//! implementations into VST3 COM interfaces.

use crate::{HostHandle, ParamInfo, Plugin, ProcessContext};
use std::ffi::c_void;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicI32, Ordering};
use vst3_sys::*;

/// Internal processor wrapper
#[repr(C)]
pub struct ProcessorWrapper<P: Plugin> {
    vtbl_component: *const ComponentVtbl,
    vtbl_audio: *const AudioProcessorVtbl,
    ref_count: AtomicI32,
    controller_cid: TUID,
    plugin: Option<P>,
    sample_rate: f64,
    max_frames: u32,
    process_ctx: Option<ProcessContext>,
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
        let vtbl_component = Box::leak(Box::new(Self::make_component_vtbl(controller_cid)));
        let vtbl_audio = Box::leak(Box::new(Self::make_audio_vtbl()));

        let host = HostHandle::new();
        let plugin = P::new(host);

        let wrapper = Box::new(Self {
            vtbl_component,
            vtbl_audio,
            ref_count: AtomicI32::new(1),
            controller_cid,
            plugin: Some(plugin),
            sample_rate: 44100.0,
            max_frames: 1024,
            process_ctx: None,
        });
        Box::into_raw(wrapper)
    }

    fn make_component_vtbl(controller_cid: TUID) -> ComponentVtbl {
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

    unsafe fn from_component(this: *mut c_void) -> *mut Self {
        this as *mut Self
    }
    unsafe fn from_audio(this: *mut c_void) -> *mut Self {
        (this as *mut u8).sub(std::mem::size_of::<*const c_void>()) as *mut Self
    }

    // Component interface
    unsafe extern "system" fn component_query_interface(
        this: *mut c_void,
        iid: *const TUID,
        obj: *mut *mut c_void,
    ) -> tresult {
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
        *obj = std::ptr::null_mut();
        kNoInterface
    }

    unsafe extern "system" fn component_add_ref(this: *mut c_void) -> uint32 {
        let obj = Self::from_component(this);
        (*obj).ref_count.fetch_add(1, Ordering::SeqCst) as uint32 + 1
    }

    unsafe extern "system" fn component_release(this: *mut c_void) -> uint32 {
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
        let iid = &*iid;
        let base = Self::from_audio(this);
        if iid_equal(iid, &iid::IUnknown) || iid_equal(iid, &vst_iid::IAudioProcessor) {
            Self::audio_add_ref(this);
            *obj = this;
            return kResultOk;
        }
        if iid_equal(iid, &base_iid::IPluginBase) || iid_equal(iid, &vst_iid::IComponent) {
            Self::audio_add_ref(this);
            *obj = base as *mut c_void;
            return kResultOk;
        }
        *obj = std::ptr::null_mut();
        kNoInterface
    }

    unsafe extern "system" fn audio_add_ref(this: *mut c_void) -> uint32 {
        let obj = Self::from_audio(this);
        (*obj).ref_count.fetch_add(1, Ordering::SeqCst) as uint32 + 1
    }

    unsafe extern "system" fn audio_release(this: *mut c_void) -> uint32 {
        let obj = Self::from_audio(this);
        let count = (*obj).ref_count.fetch_sub(1, Ordering::SeqCst) - 1;
        if count == 0 {
            let _ = Box::from_raw(obj);
        }
        count as uint32
    }

    // IComponent methods
    unsafe extern "system" fn processor_initialize(
        this: *mut c_void,
        _context: *mut c_void,
    ) -> tresult {
        let obj = Self::from_component(this);
        if let Some(plugin) = (*obj).plugin.as_mut() {
            if plugin.init() {
                kResultOk
            } else {
                kResultFalse
            }
        } else {
            kResultFalse
        }
    }

    unsafe extern "system" fn processor_terminate(_this: *mut c_void) -> tresult {
        kResultOk
    }

    unsafe extern "system" fn processor_get_controller_class_id(
        this: *mut c_void,
        class_id: *mut TUID,
    ) -> tresult {
        let obj = Self::from_component(this);
        *class_id = (*obj).controller_cid;
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
        let config = P::audio_config();
        if media_type == MediaTypes::kAudio {
            if dir == BusDirections::kInput {
                config.inputs.len() as int32
            } else {
                config.outputs.len() as int32
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
        _this: *mut c_void,
        media_type: MediaType,
        dir: BusDirection,
        index: int32,
        bus: *mut BusInfo,
    ) -> tresult {
        let config = P::audio_config();
        let bus = &mut *bus;

        if media_type == MediaTypes::kAudio {
            let ports = if dir == BusDirections::kInput {
                &config.inputs
            } else {
                &config.outputs
            };
            if let Some(port) = ports.get(index as usize) {
                bus.media_type = MediaTypes::kAudio;
                bus.direction = dir;
                bus.channel_count = port.channels as int32;
                bus.bus_type = BusTypes::kMain;
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
        _this: *mut c_void,
        _media_type: MediaType,
        _dir: BusDirection,
        _index: int32,
        _state: TBool,
    ) -> tresult {
        kResultOk
    }

    unsafe extern "system" fn processor_set_active(this: *mut c_void, state: TBool) -> tresult {
        let obj = Self::from_component(this);
        if let Some(plugin) = (*obj).plugin.as_mut() {
            if state != 0 {
                if plugin.activate((*obj).sample_rate, (*obj).max_frames) {
                    kResultOk
                } else {
                    kResultFalse
                }
            } else {
                plugin.deactivate();
                kResultOk
            }
        } else {
            kResultFalse
        }
    }

    unsafe extern "system" fn processor_set_state(
        _this: *mut c_void,
        _state: *mut c_void,
    ) -> tresult {
        kResultOk
    }
    unsafe extern "system" fn processor_get_state(
        _this: *mut c_void,
        _state: *mut c_void,
    ) -> tresult {
        kResultOk
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

    unsafe extern "system" fn audio_get_latency_samples(this: *mut c_void) -> uint32 {
        let obj = Self::from_audio(this);
        (*obj).plugin.as_ref().map(|p| p.latency()).unwrap_or(0)
    }

    unsafe extern "system" fn audio_setup_processing(
        this: *mut c_void,
        setup: *mut ProcessSetup,
    ) -> tresult {
        let obj = Self::from_audio(this);
        (*obj).sample_rate = (*setup).sample_rate;
        (*obj).max_frames = (*setup).max_samples_per_block as u32;

        // Create process context
        let config = P::audio_config();
        let num_in = config.inputs.iter().map(|p| p.channels as usize).sum();
        let num_out = config.outputs.iter().map(|p| p.channels as usize).sum();
        (*obj).process_ctx = Some(ProcessContext::new(
            (*obj).max_frames as usize,
            (*obj).sample_rate,
            num_in,
            num_out,
        ));

        kResultOk
    }

    unsafe extern "system" fn audio_set_processing(this: *mut c_void, state: TBool) -> tresult {
        let obj = Self::from_audio(this);
        if state == 0 {
            if let Some(plugin) = (*obj).plugin.as_mut() {
                plugin.reset();
            }
        }
        kResultOk
    }

    unsafe extern "system" fn audio_process(this: *mut c_void, data: *mut ProcessData) -> tresult {
        let obj = Self::from_audio(this);
        let data_ref = &*data;

        if data_ref.num_samples == 0 {
            return kResultOk;
        }

        let plugin = match (*obj).plugin.as_mut() {
            Some(p) => p,
            None => return kResultOk,
        };

        // Read parameter changes
        let params = P::params();
        if !data_ref.input_parameter_changes.is_null() {
            let param_changes = data_ref.input_parameter_changes;
            let vtbl = *(param_changes as *const *const IParameterChangesVtbl);
            let num_params = ((*vtbl).get_parameter_count)(param_changes);
            for i in 0..num_params {
                let queue = ((*vtbl).get_parameter_data)(param_changes, i);
                if !queue.is_null() {
                    let queue_vtbl = *(queue as *const *const IParamValueQueueVtbl);
                    let param_id = ((*queue_vtbl).get_parameter_id)(queue);
                    let num_points = ((*queue_vtbl).get_point_count)(queue);
                    if num_points > 0 {
                        let mut offset: int32 = 0;
                        let mut value: ParamValue = 0.0;
                        if ((*queue_vtbl).get_point)(queue, num_points - 1, &mut offset, &mut value)
                            == kResultOk
                        {
                            plugin.set_param(param_id, value);
                        }
                    }
                }
            }
        }

        // Process MIDI events
        if !data_ref.input_events.is_null() {
            let events = data_ref.input_events;
            let vtbl = *(events as *const *const IEventListVtbl);
            let num_events = ((*vtbl).get_event_count)(events);
            for i in 0..num_events {
                let mut event = Event::default();
                if ((*vtbl).get_event)(events, i, &mut event) == kResultOk {
                    match event.type_ {
                        EventTypes::kNoteOnEvent => {
                            let note = event.event.note_on;
                            plugin.note_on(note.channel, note.pitch, note.velocity);
                        }
                        EventTypes::kNoteOffEvent => {
                            let note = event.event.note_off;
                            plugin.note_off(note.channel, note.pitch, note.velocity);
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

            // Copy inputs
            if !data_ref.inputs.is_null() && data_ref.num_inputs > 0 {
                let inputs = &*data_ref.inputs;
                ctx.copy_from_raw_inputs(
                    inputs.buffers as *const *const f32,
                    inputs.num_channels as usize,
                    ctx.num_samples,
                );
            }

            // Call plugin process
            plugin.process(ctx);

            // Copy outputs
            if !data_ref.outputs.is_null() && data_ref.num_outputs > 0 {
                let outputs = &*data_ref.outputs;
                ctx.copy_to_raw_outputs(
                    outputs.buffers as *const *mut f32,
                    outputs.num_channels as usize,
                    ctx.num_samples,
                );
            }
        }

        kResultOk
    }

    unsafe extern "system" fn audio_get_tail_samples(this: *mut c_void) -> uint32 {
        let obj = Self::from_audio(this);
        (*obj).plugin.as_ref().map(|p| p.tail()).unwrap_or(kNoTail)
    }
}

/// Internal controller wrapper
#[repr(C)]
pub struct ControllerWrapper<P: Plugin> {
    vtbl: *const ControllerVtbl,
    ref_count: AtomicI32,
    params: Vec<ParamInfo>,
    param_values: Vec<f64>,
    _marker: PhantomData<P>,
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
        let params = P::params();
        let param_values: Vec<f64> = params.iter().map(|p| p.default).collect();

        let vtbl = Box::leak(Box::new(Self::make_vtbl()));

        let wrapper = Box::new(Self {
            vtbl,
            ref_count: AtomicI32::new(1),
            params,
            param_values,
            _marker: PhantomData,
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

    unsafe extern "system" fn query_interface(
        this: *mut c_void,
        iid: *const TUID,
        obj: *mut *mut c_void,
    ) -> tresult {
        let iid = &*iid;
        if iid_equal(iid, &iid::IUnknown)
            || iid_equal(iid, &base_iid::IPluginBase)
            || iid_equal(iid, &vst_iid::IEditController)
        {
            Self::add_ref(this);
            *obj = this;
            return kResultOk;
        }
        *obj = std::ptr::null_mut();
        kNoInterface
    }

    unsafe extern "system" fn add_ref(this: *mut c_void) -> uint32 {
        (*(this as *mut Self))
            .ref_count
            .fetch_add(1, Ordering::SeqCst) as uint32
            + 1
    }

    unsafe extern "system" fn release(this: *mut c_void) -> uint32 {
        let obj = this as *mut Self;
        let count = (*obj).ref_count.fetch_sub(1, Ordering::SeqCst) - 1;
        if count == 0 {
            let _ = Box::from_raw(obj);
        }
        count as uint32
    }

    unsafe extern "system" fn initialize(_this: *mut c_void, _context: *mut c_void) -> tresult {
        kResultOk
    }
    unsafe extern "system" fn terminate(_this: *mut c_void) -> tresult {
        kResultOk
    }
    unsafe extern "system" fn set_component_state(
        _this: *mut c_void,
        _state: *mut c_void,
    ) -> tresult {
        kResultOk
    }
    unsafe extern "system" fn set_state(_this: *mut c_void, _state: *mut c_void) -> tresult {
        kResultOk
    }
    unsafe extern "system" fn get_state(_this: *mut c_void, _state: *mut c_void) -> tresult {
        kResultOk
    }

    unsafe extern "system" fn get_parameter_count(this: *mut c_void) -> int32 {
        (*(this as *mut Self)).params.len() as int32
    }

    unsafe extern "system" fn get_parameter_info(
        this: *mut c_void,
        param_index: int32,
        info: *mut ParameterInfo,
    ) -> tresult {
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
            info.flags = ParameterFlags::kCanAutomate;
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
        let obj = this as *mut Self;
        if let Some(param) = (&(*obj).params).iter().find(|p| p.id == id) {
            let plain = param.to_plain(value);
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
        let obj = this as *mut Self;
        if let Some(param) = (&(*obj).params).iter().find(|p| p.id == id) {
            param.to_plain(value)
        } else {
            value
        }
    }

    unsafe extern "system" fn plain_param_to_normalized(
        this: *mut c_void,
        id: ParamID,
        value: ParamValue,
    ) -> ParamValue {
        let obj = this as *mut Self;
        if let Some(param) = (&(*obj).params).iter().find(|p| p.id == id) {
            param.to_normalized(value)
        } else {
            value
        }
    }

    unsafe extern "system" fn get_param_normalized(this: *mut c_void, id: ParamID) -> ParamValue {
        let obj = this as *mut Self;
        (&(*obj).param_values)
            .get(id as usize)
            .copied()
            .unwrap_or(0.0)
    }

    unsafe extern "system" fn set_param_normalized(
        this: *mut c_void,
        id: ParamID,
        value: ParamValue,
    ) -> tresult {
        let obj = this as *mut Self;
        if let Some(v) = (&mut (*obj).param_values).get_mut(id as usize) {
            *v = value;
            return kResultOk;
        }
        kInvalidArgument
    }

    unsafe extern "system" fn set_component_handler(
        _this: *mut c_void,
        _handler: *mut c_void,
    ) -> tresult {
        kResultOk
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
}

impl<P: GuiPlugin> PlugViewWrapper<P> {
    pub fn new(plugin: *mut P) -> *mut Self {
        let vtbl = Box::leak(Box::new(Self::make_vtbl()));
        let size = P::gui_size();

        let wrapper = Box::new(Self {
            vtbl,
            ref_count: AtomicI32::new(1),
            plugin,
            size,
        });
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
        (*(this as *mut Self))
            .ref_count
            .fetch_add(1, Ordering::SeqCst) as uint32
            + 1
    }

    unsafe extern "system" fn release(this: *mut c_void) -> uint32 {
        let obj = this as *mut Self;
        let count = (*obj).ref_count.fetch_sub(1, Ordering::SeqCst) - 1;
        if count == 0 {
            let _ = Box::from_raw(obj);
        }
        count as uint32
    }

    unsafe extern "system" fn is_platform_type_supported(
        _this: *mut c_void,
        type_: FIDString,
    ) -> tresult {
        if type_.is_null() {
            return kResultFalse;
        }
        let type_str = std::ffi::CStr::from_ptr(type_ as *const i8);
        if let Ok(s) = type_str.to_str() {
            if P::is_platform_supported(s) {
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
        let obj = this as *mut Self;
        if parent.is_null() {
            return kInvalidArgument;
        }

        // Convert platform type and parent to RawWindowHandle
        let type_str = if !type_.is_null() {
            std::ffi::CStr::from_ptr(type_ as *const i8)
                .to_str()
                .unwrap_or("")
        } else {
            ""
        };

        let handle = match type_str {
            "NSView" => {
                #[cfg(target_os = "macos")]
                {
                    use raw_window_handle::AppKitWindowHandle;
                    let ns_view = std::ptr::NonNull::new(parent).expect("parent is null");
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
                    let hwnd = NonZeroIsize::new(parent as isize).expect("parent HWND is null");
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

        if (*(*obj).plugin).gui_create(handle) {
            kResultOk
        } else {
            kResultFalse
        }
    }

    unsafe extern "system" fn removed(this: *mut c_void) -> tresult {
        let obj = this as *mut Self;
        (*(*obj).plugin).gui_destroy();
        kResultOk
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
        let obj = this as *mut Self;
        *size = ViewRect::new(0, 0, (*obj).size.width as i32, (*obj).size.height as i32);
        kResultOk
    }

    unsafe extern "system" fn on_size(this: *mut c_void, new_size: *mut ViewRect) -> tresult {
        let obj = this as *mut Self;
        let rect = &*new_size;
        (*obj).size = GuiSize::new(rect.width() as u32, rect.height() as u32);
        (*(*obj).plugin).gui_resize((*obj).size);
        kResultOk
    }

    unsafe extern "system" fn on_focus(_this: *mut c_void, _state: TBool) -> tresult {
        kResultOk
    }
    unsafe extern "system" fn set_frame(_this: *mut c_void, _frame: *mut c_void) -> tresult {
        kResultOk
    }
    unsafe extern "system" fn can_resize(_this: *mut c_void) -> tresult {
        kResultFalse
    }
    unsafe extern "system" fn check_size_constraint(
        _this: *mut c_void,
        _rect: *mut ViewRect,
    ) -> tresult {
        kResultOk
    }
}

// =============================================================================
// GuiControllerWrapper - Controller that creates IPlugView for GUI plugins
// =============================================================================

/// Controller wrapper for GUI plugins that can create IPlugView
#[repr(C)]
pub struct GuiControllerWrapper<P: GuiPlugin> {
    vtbl: *const ControllerVtbl,
    ref_count: AtomicI32,
    params: Vec<ParamInfo>,
    param_values: Vec<f64>,
    plugin: Option<P>,
}

impl<P: GuiPlugin> GuiControllerWrapper<P> {
    pub fn new() -> *mut Self {
        let params = P::params();
        let param_values: Vec<f64> = params.iter().map(|p| p.default).collect();

        let vtbl = Box::leak(Box::new(Self::make_vtbl()));
        let host = HostHandle::new();
        let plugin = P::new(host);

        let wrapper = Box::new(Self {
            vtbl,
            ref_count: AtomicI32::new(1),
            params,
            param_values,
            plugin: Some(plugin),
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

    unsafe extern "system" fn query_interface(
        this: *mut c_void,
        iid: *const TUID,
        obj: *mut *mut c_void,
    ) -> tresult {
        let iid = &*iid;
        if iid_equal(iid, &iid::IUnknown)
            || iid_equal(iid, &base_iid::IPluginBase)
            || iid_equal(iid, &vst_iid::IEditController)
        {
            Self::add_ref(this);
            *obj = this;
            return kResultOk;
        }
        *obj = std::ptr::null_mut();
        kNoInterface
    }

    unsafe extern "system" fn add_ref(this: *mut c_void) -> uint32 {
        (*(this as *mut Self))
            .ref_count
            .fetch_add(1, Ordering::SeqCst) as uint32
            + 1
    }

    unsafe extern "system" fn release(this: *mut c_void) -> uint32 {
        let obj = this as *mut Self;
        let count = (*obj).ref_count.fetch_sub(1, Ordering::SeqCst) - 1;
        if count == 0 {
            let _ = Box::from_raw(obj);
        }
        count as uint32
    }

    unsafe extern "system" fn initialize(_this: *mut c_void, _context: *mut c_void) -> tresult {
        kResultOk
    }
    unsafe extern "system" fn terminate(_this: *mut c_void) -> tresult {
        kResultOk
    }
    unsafe extern "system" fn set_component_state(
        _this: *mut c_void,
        _state: *mut c_void,
    ) -> tresult {
        kResultOk
    }
    unsafe extern "system" fn set_state(_this: *mut c_void, _state: *mut c_void) -> tresult {
        kResultOk
    }
    unsafe extern "system" fn get_state(_this: *mut c_void, _state: *mut c_void) -> tresult {
        kResultOk
    }

    unsafe extern "system" fn get_parameter_count(this: *mut c_void) -> int32 {
        (*(this as *mut Self)).params.len() as int32
    }

    unsafe extern "system" fn get_parameter_info(
        this: *mut c_void,
        param_index: int32,
        info: *mut ParameterInfo,
    ) -> tresult {
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
            info.flags = ParameterFlags::kCanAutomate;
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
        let obj = this as *mut Self;
        if let Some(param) = (&(*obj).params).iter().find(|p| p.id == id) {
            let plain = param.to_plain(value);
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
        let obj = this as *mut Self;
        if let Some(param) = (&(*obj).params).iter().find(|p| p.id == id) {
            param.to_plain(value)
        } else {
            value
        }
    }

    unsafe extern "system" fn plain_param_to_normalized(
        this: *mut c_void,
        id: ParamID,
        value: ParamValue,
    ) -> ParamValue {
        let obj = this as *mut Self;
        if let Some(param) = (&(*obj).params).iter().find(|p| p.id == id) {
            param.to_normalized(value)
        } else {
            value
        }
    }

    unsafe extern "system" fn get_param_normalized(this: *mut c_void, id: ParamID) -> ParamValue {
        let obj = this as *mut Self;
        (&(*obj).param_values)
            .get(id as usize)
            .copied()
            .unwrap_or(0.0)
    }

    unsafe extern "system" fn set_param_normalized(
        this: *mut c_void,
        id: ParamID,
        value: ParamValue,
    ) -> tresult {
        let obj = this as *mut Self;
        if let Some(v) = (&mut (*obj).param_values).get_mut(id as usize) {
            *v = value;
            if let Some(plugin) = (*obj).plugin.as_mut() {
                plugin.set_param(id as u32, value);
            }
            return kResultOk;
        }
        kInvalidArgument
    }

    unsafe extern "system" fn set_component_handler(
        _this: *mut c_void,
        _handler: *mut c_void,
    ) -> tresult {
        kResultOk
    }

    unsafe extern "system" fn create_view(this: *mut c_void, name: FIDString) -> *mut c_void {
        if name.is_null() {
            return std::ptr::null_mut();
        }
        let name_str = std::ffi::CStr::from_ptr(name as *const i8);
        if name_str.to_bytes() != b"editor" {
            return std::ptr::null_mut();
        }

        let obj = this as *mut Self;
        if let Some(plugin) = (*obj).plugin.as_mut() {
            // Create PlugViewWrapper with pointer to plugin
            let plugin_ptr = plugin as *mut P;
            PlugViewWrapper::new(plugin_ptr) as *mut c_void
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

            const CID_PROCESSOR: TUID = uid!(0x11111111, 0x22222222, 0x33333333, 0x44444444);
            const CID_CONTROLLER: TUID = uid!(0x11111111, 0x22222222, 0x33333333, 0x55555555);

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
                (*(this as *mut PluginFactoryObj))
                    .ref_count
                    .fetch_sub(1, Ordering::SeqCst) as uint32
            }

            unsafe extern "system" fn factory_get_factory_info(
                _this: *mut c_void,
                info: *mut PFactoryInfoData,
            ) -> tresult {
                let plugin_info = <$plugin_type as $crate::Plugin>::info();
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
                let plugin_info = <$plugin_type as $crate::Plugin>::info();
                let info = &mut *info;
                match index {
                    0 => {
                        info.cid = CID_PROCESSOR;
                        info.cardinality = PClassInfo::kManyInstances;
                        strcpy_safe(&mut info.category, kVstAudioEffectClass);
                        strcpy_safe(&mut info.name, plugin_info.name.as_bytes());
                    }
                    1 => {
                        info.cid = CID_CONTROLLER;
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
                let plugin_info = <$plugin_type as $crate::Plugin>::info();
                let info = &mut *info;
                match index {
                    0 => {
                        info.cid = CID_PROCESSOR;
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
                        info.cid = CID_CONTROLLER;
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
                _iid: FIDString,
                obj: *mut *mut c_void,
            ) -> tresult {
                let cid_bytes = std::slice::from_raw_parts(cid as *const i8, 16);
                let mut cid_arr: TUID = [0; 16];
                cid_arr.copy_from_slice(cid_bytes);

                if iid_equal(&cid_arr, &CID_PROCESSOR) {
                    *obj = $crate::wrapper::ProcessorWrapper::<$plugin_type>::new(CID_CONTROLLER)
                        as *mut c_void;
                    return kResultOk;
                }
                if iid_equal(&cid_arr, &CID_CONTROLLER) {
                    *obj = $crate::wrapper::ControllerWrapper::<$plugin_type>::new() as *mut c_void;
                    return kResultOk;
                }
                *obj = std::ptr::null_mut();
                kNoInterface
            }

            unsafe extern "system" fn factory_get_class_info_unicode(
                _this: *mut c_void,
                index: int32,
                info: *mut PClassInfoWData,
            ) -> tresult {
                let plugin_info = <$plugin_type as $crate::Plugin>::info();
                let info = &mut *info;
                match index {
                    0 => {
                        info.cid = CID_PROCESSOR;
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
                        info.cid = CID_CONTROLLER;
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
                FACTORY
                    .get_or_init(|| {
                        SendSyncPtr(Box::into_raw(Box::new(PluginFactoryObj {
                            vtbl: &FACTORY_VTBL,
                            ref_count: AtomicI32::new(1),
                        })))
                    })
                    .0 as *mut c_void
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

            const CID_PROCESSOR: TUID = uid!(0x11111111, 0x22222222, 0x33333333, 0x44444444);
            const CID_CONTROLLER: TUID = uid!(0x11111111, 0x22222222, 0x33333333, 0x55555555);

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
                (*(this as *mut PluginFactoryObj))
                    .ref_count
                    .fetch_sub(1, Ordering::SeqCst) as uint32
            }

            unsafe extern "system" fn factory_get_factory_info(
                _this: *mut c_void,
                info: *mut PFactoryInfoData,
            ) -> tresult {
                let plugin_info = <$plugin_type as $crate::Plugin>::info();
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
                let plugin_info = <$plugin_type as $crate::Plugin>::info();
                let info = &mut *info;
                match index {
                    0 => {
                        info.cid = CID_PROCESSOR;
                        info.cardinality = PClassInfo::kManyInstances;
                        strcpy_safe(&mut info.category, kVstAudioEffectClass);
                        strcpy_safe(&mut info.name, plugin_info.name.as_bytes());
                    }
                    1 => {
                        info.cid = CID_CONTROLLER;
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
                let plugin_info = <$plugin_type as $crate::Plugin>::info();
                let info = &mut *info;
                match index {
                    0 => {
                        info.cid = CID_PROCESSOR;
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
                        info.cid = CID_CONTROLLER;
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
                _iid: FIDString,
                obj: *mut *mut c_void,
            ) -> tresult {
                let cid_bytes = std::slice::from_raw_parts(cid as *const i8, 16);
                let mut cid_arr: TUID = [0; 16];
                cid_arr.copy_from_slice(cid_bytes);

                if iid_equal(&cid_arr, &CID_PROCESSOR) {
                    *obj = $crate::wrapper::ProcessorWrapper::<$plugin_type>::new(CID_CONTROLLER)
                        as *mut c_void;
                    return kResultOk;
                }
                if iid_equal(&cid_arr, &CID_CONTROLLER) {
                    // Use GuiControllerWrapper for GUI plugins
                    *obj =
                        $crate::wrapper::GuiControllerWrapper::<$plugin_type>::new() as *mut c_void;
                    return kResultOk;
                }
                *obj = std::ptr::null_mut();
                kNoInterface
            }

            unsafe extern "system" fn factory_get_class_info_unicode(
                _this: *mut c_void,
                index: int32,
                info: *mut PClassInfoWData,
            ) -> tresult {
                let plugin_info = <$plugin_type as $crate::Plugin>::info();
                let info = &mut *info;
                match index {
                    0 => {
                        info.cid = CID_PROCESSOR;
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
                        info.cid = CID_CONTROLLER;
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
                FACTORY
                    .get_or_init(|| {
                        SendSyncPtr(Box::into_raw(Box::new(PluginFactoryObj {
                            vtbl: &FACTORY_VTBL,
                            ref_count: AtomicI32::new(1),
                        })))
                    })
                    .0 as *mut c_void
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
