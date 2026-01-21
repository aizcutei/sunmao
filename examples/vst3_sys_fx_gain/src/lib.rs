//! VST3 Gain Effect Example using vst3_sys
//!
//! A simple stereo gain effect plugin.

#![allow(unsafe_op_in_unsafe_fn)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicI32, Ordering};
use vst3_sys::*;

// =============================================================================
// Plugin UIDs
// =============================================================================

const CID_PROCESSOR: TUID = uid!(0x12345678, 0x11111111, 0x22222222, 0x33333333);
const CID_CONTROLLER: TUID = uid!(0x12345678, 0x11111111, 0x22222222, 0x44444444);

// =============================================================================
// Parameter IDs
// =============================================================================

const PARAM_GAIN: ParamID = 0;

// =============================================================================
// Processor Implementation - Dual vtables for IComponent + IAudioProcessor
// =============================================================================

// COM object layout: BOTH vtable pointers first, then data
// When QueryInterface is called for IComponent, return &vtbl_component
// When QueryInterface is called for IAudioProcessor, return &vtbl_audio
#[repr(C)]
struct GainProcessorObj {
    vtbl_component: *const ComponentVtbl,
    vtbl_audio: *const AudioProcessorVtbl,
    ref_count: AtomicI32,
    gain: f32,
    sample_rate: f64,
    active: bool,
    processing: bool,
}

// IComponent vtable: IUnknown -> IPluginBase -> IComponent
#[repr(C)]
struct ComponentVtbl {
    // IUnknown
    query_interface: unsafe extern "system" fn(*mut c_void, *const TUID, *mut *mut c_void) -> tresult,
    add_ref: unsafe extern "system" fn(*mut c_void) -> uint32,
    release: unsafe extern "system" fn(*mut c_void) -> uint32,
    // IPluginBase
    initialize: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
    terminate: unsafe extern "system" fn(*mut c_void) -> tresult,
    // IComponent
    get_controller_class_id: unsafe extern "system" fn(*mut c_void, *mut TUID) -> tresult,
    set_io_mode: unsafe extern "system" fn(*mut c_void, IoMode) -> tresult,
    get_bus_count: unsafe extern "system" fn(*mut c_void, MediaType, BusDirection) -> int32,
    get_bus_info: unsafe extern "system" fn(*mut c_void, MediaType, BusDirection, int32, *mut BusInfo) -> tresult,
    get_routing_info: unsafe extern "system" fn(*mut c_void, *mut RoutingInfo, *mut RoutingInfo) -> tresult,
    activate_bus: unsafe extern "system" fn(*mut c_void, MediaType, BusDirection, int32, TBool) -> tresult,
    set_active: unsafe extern "system" fn(*mut c_void, TBool) -> tresult,
    set_state: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
    get_state: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
}

// IAudioProcessor vtable: IUnknown -> IAudioProcessor (doesn't inherit from IPluginBase)
#[repr(C)]
struct AudioProcessorVtbl {
    // IUnknown
    query_interface: unsafe extern "system" fn(*mut c_void, *const TUID, *mut *mut c_void) -> tresult,
    add_ref: unsafe extern "system" fn(*mut c_void) -> uint32,
    release: unsafe extern "system" fn(*mut c_void) -> uint32,
    // IAudioProcessor
    set_bus_arrangements: unsafe extern "system" fn(*mut c_void, *mut SpeakerArrangement, int32, *mut SpeakerArrangement, int32) -> tresult,
    get_bus_arrangement: unsafe extern "system" fn(*mut c_void, BusDirection, int32, *mut SpeakerArrangement) -> tresult,
    can_process_sample_size: unsafe extern "system" fn(*mut c_void, int32) -> tresult,
    get_latency_samples: unsafe extern "system" fn(*mut c_void) -> uint32,
    setup_processing: unsafe extern "system" fn(*mut c_void, *mut ProcessSetup) -> tresult,
    set_processing: unsafe extern "system" fn(*mut c_void, TBool) -> tresult,
    process: unsafe extern "system" fn(*mut c_void, *mut ProcessData) -> tresult,
    get_tail_samples: unsafe extern "system" fn(*mut c_void) -> uint32,
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

impl GainProcessorObj {
    fn new() -> *mut Self {
        let obj = Box::new(GainProcessorObj {
            vtbl_component: &COMPONENT_VTBL,
            vtbl_audio: &AUDIO_VTBL,
            ref_count: AtomicI32::new(1),
            gain: 1.0,
            sample_rate: 44100.0,
            active: false,
            processing: false,
        });
        Box::into_raw(obj)
    }
    
    // Get the base object from component interface pointer
    unsafe fn from_component(this: *mut c_void) -> *mut Self {
        this as *mut Self
    }
    
    // Get the base object from audio interface pointer (offset by 1 pointer)
    unsafe fn from_audio(this: *mut c_void) -> *mut Self {
        (this as *mut u8).sub(std::mem::size_of::<*const c_void>()) as *mut Self
    }
}

// Component interface functions
unsafe extern "system" fn component_query_interface(
    this: *mut c_void,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    let iid = &*iid;
    let base = GainProcessorObj::from_component(this);
    
    if iid_equal(iid, &iid::IUnknown)
        || iid_equal(iid, &base_iid::IPluginBase)
        || iid_equal(iid, &vst_iid::IComponent)
    {
        component_add_ref(this);
        *obj = this; // Return component interface pointer
        return kResultOk;
    }
    
    if iid_equal(iid, &vst_iid::IAudioProcessor) {
        component_add_ref(this);
        // Return audio interface pointer (offset into object)
        *obj = &(*base).vtbl_audio as *const _ as *mut c_void;
        return kResultOk;
    }
    
    *obj = std::ptr::null_mut();
    kNoInterface
}

unsafe extern "system" fn component_add_ref(this: *mut c_void) -> uint32 {
    let obj = GainProcessorObj::from_component(this);
    (*obj).ref_count.fetch_add(1, Ordering::SeqCst) as uint32 + 1
}

unsafe extern "system" fn component_release(this: *mut c_void) -> uint32 {
    let obj = GainProcessorObj::from_component(this);
    let count = (*obj).ref_count.fetch_sub(1, Ordering::SeqCst) - 1;
    if count == 0 {
        let _ = Box::from_raw(obj);
    }
    count as uint32
}

// Audio interface functions - these receive &vtbl_audio, need to offset back
unsafe extern "system" fn audio_query_interface(
    this: *mut c_void,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    let iid = &*iid;
    let base = GainProcessorObj::from_audio(this);
    
    if iid_equal(iid, &iid::IUnknown) || iid_equal(iid, &vst_iid::IAudioProcessor) {
        audio_add_ref(this);
        *obj = this;
        return kResultOk;
    }
    
    if iid_equal(iid, &base_iid::IPluginBase) || iid_equal(iid, &vst_iid::IComponent) {
        audio_add_ref(this);
        *obj = base as *mut c_void; // Return component interface pointer
        return kResultOk;
    }
    
    *obj = std::ptr::null_mut();
    kNoInterface
}

unsafe extern "system" fn audio_add_ref(this: *mut c_void) -> uint32 {
    let obj = GainProcessorObj::from_audio(this);
    (*obj).ref_count.fetch_add(1, Ordering::SeqCst) as uint32 + 1
}

unsafe extern "system" fn audio_release(this: *mut c_void) -> uint32 {
    let obj = GainProcessorObj::from_audio(this);
    let count = (*obj).ref_count.fetch_sub(1, Ordering::SeqCst) - 1;
    if count == 0 {
        let _ = Box::from_raw(obj);
    }
    count as uint32
}

// IAudioProcessor implementations
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

unsafe extern "system" fn audio_can_process_sample_size(_this: *mut c_void, symbolic_sample_size: int32) -> tresult {
    if symbolic_sample_size == SymbolicSampleSizes::kSample32 {
        kResultOk
    } else {
        kResultFalse
    }
}

unsafe extern "system" fn audio_get_latency_samples(_this: *mut c_void) -> uint32 {
    0
}

unsafe extern "system" fn audio_setup_processing(this: *mut c_void, setup: *mut ProcessSetup) -> tresult {
    let obj = GainProcessorObj::from_audio(this);
    (*obj).sample_rate = (*setup).sample_rate;
    kResultOk
}

unsafe extern "system" fn audio_set_processing(this: *mut c_void, state: TBool) -> tresult {
    let obj = GainProcessorObj::from_audio(this);
    (*obj).processing = state != 0;
    kResultOk
}

unsafe extern "system" fn audio_process(this: *mut c_void, data: *mut ProcessData) -> tresult {
    let obj = GainProcessorObj::from_audio(this);
    let data = &*data;
    
    if data.num_samples == 0 {
        return kResultOk;
    }
    
    // Read parameter changes from host
    if !data.input_parameter_changes.is_null() {
        // input_parameter_changes is IParameterChanges*
        let param_changes = data.input_parameter_changes;
        // Get vtable pointer at start of object
        let vtbl = *(param_changes as *const *const IParameterChangesVtbl);
        
        let num_params = ((*vtbl).get_parameter_count)(param_changes);
        for i in 0..num_params {
            let queue = ((*vtbl).get_parameter_data)(param_changes, i);
            if !queue.is_null() {
                // queue is IParamValueQueue*
                let queue_vtbl = *(queue as *const *const IParamValueQueueVtbl);
                let param_id = ((*queue_vtbl).get_parameter_id)(queue);
                
                if param_id == PARAM_GAIN {
                    let num_points = ((*queue_vtbl).get_point_count)(queue);
                    if num_points > 0 {
                        let mut sample_offset: int32 = 0;
                        let mut value: ParamValue = 0.0;
                        // Get the last point (most recent value)
                        if ((*queue_vtbl).get_point)(queue, num_points - 1, &mut sample_offset, &mut value) == kResultOk {
                            (*obj).gain = value as f32;
                        }
                    }
                }
            }
        }
    }
    
    let gain = (*obj).gain;
    
    // Process audio
    if !data.inputs.is_null() && !data.outputs.is_null() && data.num_inputs > 0 && data.num_outputs > 0 {
        let inputs = &*data.inputs;
        let outputs = &mut *data.outputs;
        
        let num_channels = inputs.num_channels.min(outputs.num_channels) as usize;
        let num_samples = data.num_samples as usize;
        
        for ch in 0..num_channels {
            let input = *(inputs.buffers as *const *const f32).add(ch);
            let output = *(outputs.buffers as *const *mut f32).add(ch);
            
            for i in 0..num_samples {
                *output.add(i) = *input.add(i) * gain;
            }
        }
    }
    
    kResultOk
}

unsafe extern "system" fn audio_get_tail_samples(_this: *mut c_void) -> uint32 {
    kNoTail
}

unsafe extern "system" fn processor_initialize(_this: *mut c_void, _context: *mut c_void) -> tresult {
    kResultOk
}

unsafe extern "system" fn processor_terminate(_this: *mut c_void) -> tresult {
    kResultOk
}

unsafe extern "system" fn processor_get_controller_class_id(_this: *mut c_void, class_id: *mut TUID) -> tresult {
    *class_id = CID_CONTROLLER;
    kResultOk
}

unsafe extern "system" fn processor_set_io_mode(_this: *mut c_void, _mode: IoMode) -> tresult {
    kResultOk
}

unsafe extern "system" fn processor_get_bus_count(_this: *mut c_void, media_type: MediaType, dir: BusDirection) -> int32 {
    if media_type == MediaTypes::kAudio {
        if dir == BusDirections::kInput || dir == BusDirections::kOutput {
            return 1;
        }
    }
    0
}

unsafe extern "system" fn processor_get_bus_info(
    _this: *mut c_void,
    media_type: MediaType,
    dir: BusDirection,
    index: int32,
    bus: *mut BusInfo,
) -> tresult {
    if media_type != MediaTypes::kAudio || index != 0 {
        return kInvalidArgument;
    }
    
    let bus = &mut *bus;
    bus.media_type = MediaTypes::kAudio;
    bus.direction = dir;
    bus.channel_count = 2;
    bus.bus_type = BusTypes::kMain;
    bus.flags = BusFlags::kDefaultActive;
    
    let name = if dir == BusDirections::kInput { "Input" } else { "Output" };
    str16cpy_safe(&mut bus.name, name);
    
    kResultOk
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

unsafe extern "system" fn processor_set_state(_this: *mut c_void, _state: *mut c_void) -> tresult {
    kResultOk
}

unsafe extern "system" fn processor_get_state(_this: *mut c_void, _state: *mut c_void) -> tresult {
    kResultOk
}

// =============================================================================
// Controller Implementation
// =============================================================================

#[repr(C)]
struct GainControllerObj {
    vtbl: *const GainControllerVtbl,
    ref_count: AtomicI32,
    gain_value: f64,
}

// Combined vtable with all methods in proper C++ vtable order
// IUnknown -> IPluginBase -> IEditController (full hierarchy)
#[repr(C)]
struct GainControllerVtbl {
    // IUnknown
    query_interface: unsafe extern "system" fn(*mut c_void, *const TUID, *mut *mut c_void) -> tresult,
    add_ref: unsafe extern "system" fn(*mut c_void) -> uint32,
    release: unsafe extern "system" fn(*mut c_void) -> uint32,
    // IPluginBase
    initialize: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
    terminate: unsafe extern "system" fn(*mut c_void) -> tresult,
    // IEditController
    set_component_state: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
    set_state: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
    get_state: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
    get_parameter_count: unsafe extern "system" fn(*mut c_void) -> int32,
    get_parameter_info: unsafe extern "system" fn(*mut c_void, int32, *mut ParameterInfo) -> tresult,
    get_param_string_by_value: unsafe extern "system" fn(*mut c_void, ParamID, ParamValue, *mut String128) -> tresult,
    get_param_value_by_string: unsafe extern "system" fn(*mut c_void, ParamID, *const TChar, *mut ParamValue) -> tresult,
    normalized_param_to_plain: unsafe extern "system" fn(*mut c_void, ParamID, ParamValue) -> ParamValue,
    plain_param_to_normalized: unsafe extern "system" fn(*mut c_void, ParamID, ParamValue) -> ParamValue,
    get_param_normalized: unsafe extern "system" fn(*mut c_void, ParamID) -> ParamValue,
    set_param_normalized: unsafe extern "system" fn(*mut c_void, ParamID, ParamValue) -> tresult,
    set_component_handler: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
    create_view: unsafe extern "system" fn(*mut c_void, FIDString) -> *mut c_void,
}

static CONTROLLER_VTBL: GainControllerVtbl = GainControllerVtbl {
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

impl GainControllerObj {
    fn new() -> *mut Self {
        let obj = Box::new(GainControllerObj {
            vtbl: &CONTROLLER_VTBL,
            ref_count: AtomicI32::new(1),
            gain_value: 1.0,
        });
        Box::into_raw(obj)
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
    *obj = std::ptr::null_mut();
    kNoInterface
}

unsafe extern "system" fn controller_add_ref(this: *mut c_void) -> uint32 {
    let obj = this as *mut GainControllerObj;
    (*obj).ref_count.fetch_add(1, Ordering::SeqCst) as uint32 + 1
}

unsafe extern "system" fn controller_release(this: *mut c_void) -> uint32 {
    let obj = this as *mut GainControllerObj;
    let count = (*obj).ref_count.fetch_sub(1, Ordering::SeqCst) - 1;
    if count == 0 {
        let _ = Box::from_raw(obj);
    }
    count as uint32
}

unsafe extern "system" fn controller_initialize(_this: *mut c_void, _context: *mut c_void) -> tresult {
    kResultOk
}

unsafe extern "system" fn controller_terminate(_this: *mut c_void) -> tresult {
    kResultOk
}

unsafe extern "system" fn controller_set_component_state(_this: *mut c_void, _state: *mut c_void) -> tresult {
    kResultOk
}

unsafe extern "system" fn controller_set_state(_this: *mut c_void, _state: *mut c_void) -> tresult {
    kResultOk
}

unsafe extern "system" fn controller_get_state(_this: *mut c_void, _state: *mut c_void) -> tresult {
    kResultOk
}

unsafe extern "system" fn controller_get_parameter_count(_this: *mut c_void) -> int32 {
    1
}

unsafe extern "system" fn controller_get_parameter_info(_this: *mut c_void, param_index: int32, info: *mut ParameterInfo) -> tresult {
    if param_index != 0 {
        return kInvalidArgument;
    }
    let info = &mut *info;
    info.id = PARAM_GAIN;
    str16cpy_safe(&mut info.title, "Gain");
    str16cpy_safe(&mut info.short_title, "Gain");
    str16cpy_safe(&mut info.units, "%");
    info.step_count = 0;
    info.default_normalized_value = 1.0;
    info.unit_id = 0;
    info.flags = ParameterFlags::kCanAutomate;
    kResultOk
}

unsafe extern "system" fn controller_get_param_string_by_value(
    _this: *mut c_void,
    id: ParamID,
    value_normalized: ParamValue,
    string: *mut String128,
) -> tresult {
    if id == PARAM_GAIN {
        // Format as percentage (0-100%)
        let percent = (value_normalized * 100.0) as i32;
        let s = format!("{}%", percent);
        str16cpy_safe(&mut *string, &s);
        return kResultOk;
    }
    kInvalidArgument
}

unsafe extern "system" fn controller_get_param_value_by_string(
    _this: *mut c_void,
    _id: ParamID,
    _string: *const TChar,
    _value_normalized: *mut ParamValue,
) -> tresult {
    kNotImplemented
}

unsafe extern "system" fn controller_normalized_param_to_plain(
    _this: *mut c_void,
    _id: ParamID,
    value_normalized: ParamValue,
) -> ParamValue {
    value_normalized * 100.0
}

unsafe extern "system" fn controller_plain_param_to_normalized(
    _this: *mut c_void,
    _id: ParamID,
    plain_value: ParamValue,
) -> ParamValue {
    plain_value / 100.0
}

unsafe extern "system" fn controller_get_param_normalized(this: *mut c_void, _id: ParamID) -> ParamValue {
    let obj = this as *mut GainControllerObj;
    (*obj).gain_value
}

unsafe extern "system" fn controller_set_param_normalized(this: *mut c_void, _id: ParamID, value: ParamValue) -> tresult {
    let obj = this as *mut GainControllerObj;
    (*obj).gain_value = value;
    kResultOk
}

unsafe extern "system" fn controller_set_component_handler(_this: *mut c_void, _handler: *mut c_void) -> tresult {
    kResultOk
}

unsafe extern "system" fn controller_create_view(_this: *mut c_void, _name: FIDString) -> *mut c_void {
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

// IPluginFactory3 vtable (inherits from IPluginFactory2 -> IPluginFactory -> IUnknown)
#[repr(C)]
struct PluginFactoryVtbl {
    // IUnknown
    query_interface: unsafe extern "system" fn(*mut c_void, *const TUID, *mut *mut c_void) -> tresult,
    add_ref: unsafe extern "system" fn(*mut c_void) -> uint32,
    release: unsafe extern "system" fn(*mut c_void) -> uint32,
    // IPluginFactory
    get_factory_info: unsafe extern "system" fn(*mut c_void, *mut PFactoryInfoData) -> tresult,
    count_classes: unsafe extern "system" fn(*mut c_void) -> int32,
    get_class_info: unsafe extern "system" fn(*mut c_void, int32, *mut PClassInfoData) -> tresult,
    create_instance: unsafe extern "system" fn(*mut c_void, FIDString, FIDString, *mut *mut c_void) -> tresult,
    // IPluginFactory2
    get_class_info2: unsafe extern "system" fn(*mut c_void, int32, *mut PClassInfo2Data) -> tresult,
    // IPluginFactory3
    get_class_info_unicode: unsafe extern "system" fn(*mut c_void, int32, *mut PClassInfoWData) -> tresult,
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
    let obj = this as *mut PluginFactoryObj;
    (*obj).ref_count.fetch_add(1, Ordering::SeqCst) as uint32 + 1
}

unsafe extern "system" fn factory_release(this: *mut c_void) -> uint32 {
    let obj = this as *mut PluginFactoryObj;
    let count = (*obj).ref_count.fetch_sub(1, Ordering::SeqCst) - 1;
    // Factory is static, don't deallocate
    count as uint32
}

unsafe extern "system" fn factory_get_factory_info(_this: *mut c_void, info: *mut PFactoryInfoData) -> tresult {
    let info = &mut *info;
    strcpy_safe(&mut info.vendor, b"aizcutei\0");
    strcpy_safe(&mut info.url, b"https://aizcutei.github.io/sunmao\0");
    strcpy_safe(&mut info.email, b"info@example.com\0");
    info.flags = PFactoryInfo::Flags::kUnicode;
    kResultOk
}

unsafe extern "system" fn factory_count_classes(_this: *mut c_void) -> int32 {
    2 // Processor + Controller
}

unsafe extern "system" fn factory_get_class_info(_this: *mut c_void, index: int32, info: *mut PClassInfoData) -> tresult {
    let info = &mut *info;
    match index {
        0 => {
            info.cid = CID_PROCESSOR;
            info.cardinality = PClassInfo::kManyInstances;
            strcpy_safe(&mut info.category, kVstAudioEffectClass);
            strcpy_safe(&mut info.name, b"Vst3 Sys Fx Gain\0");
        }
        1 => {
            info.cid = CID_CONTROLLER;
            info.cardinality = PClassInfo::kManyInstances;
            strcpy_safe(&mut info.category, kVstComponentControllerClass);
            strcpy_safe(&mut info.name, b"Vst3 Sys Fx Gain Controller\0");
        }
        _ => return kInvalidArgument,
    }
    kResultOk
}

unsafe extern "system" fn factory_get_class_info2(_this: *mut c_void, index: int32, info: *mut PClassInfo2Data) -> tresult {
    let info = &mut *info;
    match index {
        0 => {
            info.cid = CID_PROCESSOR;
            info.cardinality = PClassInfo::kManyInstances;
            strcpy_safe(&mut info.category, kVstAudioEffectClass);
            strcpy_safe(&mut info.name, b"Vst3 Sys Fx Gain\0");
            info.class_flags = ComponentFlags::kSimpleModeSupported;
            strcpy_safe(&mut info.sub_categories, PlugType::kFx);
            strcpy_safe(&mut info.vendor, b"aizcutei\0");
            strcpy_safe(&mut info.version, b"0.1.0\0");
            strcpy_safe(&mut info.sdk_version, b"VST 3.8.0\0");
        }
        1 => {
            info.cid = CID_CONTROLLER;
            info.cardinality = PClassInfo::kManyInstances;
            strcpy_safe(&mut info.category, kVstComponentControllerClass);
            strcpy_safe(&mut info.name, b"Vst3 Sys Fx Gain Controller\0");
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
    _iid: FIDString,
    obj: *mut *mut c_void,
) -> tresult {
    let cid_bytes = std::slice::from_raw_parts(cid as *const i8, 16);
    
    let mut cid_arr: TUID = [0; 16];
    cid_arr.copy_from_slice(cid_bytes);
    
    if iid_equal(&cid_arr, &CID_PROCESSOR) {
        *obj = GainProcessorObj::new() as *mut c_void;
        return kResultOk;
    }
    
    if iid_equal(&cid_arr, &CID_CONTROLLER) {
        *obj = GainControllerObj::new() as *mut c_void;
        return kResultOk;
    }
    
    *obj = std::ptr::null_mut();
    kNoInterface
}

unsafe extern "system" fn factory_get_class_info_unicode(
    _this: *mut c_void, 
    index: int32, 
    info: *mut PClassInfoWData
) -> tresult {
    let info = &mut *info;
    match index {
        0 => {
            info.cid = CID_PROCESSOR;
            info.cardinality = PClassInfo::kManyInstances;
            strcpy_safe(&mut info.category, kVstAudioEffectClass);
            str16cpy(&mut info.name, "Vst3 Sys Fx Gain");
            info.class_flags = ComponentFlags::kSimpleModeSupported;
            strcpy_safe(&mut info.sub_categories, PlugType::kFx);
            str16cpy(&mut info.vendor, "aizcutei");
            str16cpy(&mut info.version, "0.1.0");
            str16cpy(&mut info.sdk_version, "VST 3.8.0");
        }
        1 => {
            info.cid = CID_CONTROLLER;
            info.cardinality = PClassInfo::kManyInstances;
            strcpy_safe(&mut info.category, kVstComponentControllerClass);
            str16cpy(&mut info.name, "Vst3 Sys Fx Gain Controller");
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

unsafe extern "system" fn factory_set_host_context(_this: *mut c_void, _context: *mut c_void) -> tresult {
    kResultOk
}

// =============================================================================
// Entry Points
// =============================================================================

use std::sync::OnceLock;

// Wrapper to make raw pointer Send+Sync (safe because factory is only created once)
struct SendSyncPtr(*mut PluginFactoryObj);
unsafe impl Send for SendSyncPtr {}
unsafe impl Sync for SendSyncPtr {}

static FACTORY: OnceLock<SendSyncPtr> = OnceLock::new();

#[unsafe(no_mangle)]
pub extern "C" fn GetPluginFactory() -> *mut c_void {
    FACTORY.get_or_init(|| {
        let factory = Box::new(PluginFactoryObj {
            vtbl: &FACTORY_VTBL,
            ref_count: AtomicI32::new(1),
        });
        SendSyncPtr(Box::into_raw(factory))
    }).0 as *mut c_void
}

// macOS bundle entry points
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

// Linux entry points
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

// Windows entry points
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
