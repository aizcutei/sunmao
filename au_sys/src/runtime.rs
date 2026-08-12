#![allow(unsafe_op_in_unsafe_fn, non_upper_case_globals, unused_mut, dead_code)]

use libc::c_void;
use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::CString;
use std::io::Write;
use std::os::raw::c_long;
use std::path::Path;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::core::{AuPlugin, BufferList, ParameterInfo};
use crate::sys::*;
static FACTORY_PTR: AtomicPtr<AuFactory> = AtomicPtr::new(ptr::null_mut());
static INSTANCE_MAP: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();
fn instance_map() -> &'static Mutex<HashMap<usize, usize>> {
    INSTANCE_MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

fn log_msg(message: &str) {
    const LOG_PATH: &str = "/tmp/sunmao_au_gui.log";
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG_PATH)
    {
        let _ = writeln!(file, "{}", message);
    }
}

thread_local! {
    static HOST_CALLBACKS: Cell<*const HostCallbackInfo> = Cell::new(ptr::null());
}

pub fn current_host_callbacks() -> Option<&'static HostCallbackInfo> {
    HOST_CALLBACKS.with(|cell| {
        let ptr = cell.get();
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { &*ptr })
        }
    })
}

fn set_current_host_callbacks(ptr: *const HostCallbackInfo) {
    HOST_CALLBACKS.with(|cell| cell.set(ptr));
}

pub fn instance_ptr_for_unit(unit: *mut c_void) -> *mut c_void {
    if unit.is_null() {
        return std::ptr::null_mut();
    }
    let map = match instance_map().lock() {
        Ok(map) => map,
        Err(_) => return std::ptr::null_mut(),
    };
    map.get(&(unit as usize)).copied().unwrap_or(0) as *mut c_void
}

#[repr(C)]
pub struct AuComponentDescriptor {
    pub name: &'static str,
    pub component_type: OSType,
    pub component_subtype: OSType,
    pub manufacturer: OSType,
    pub version: UInt32,
    pub flags: UInt32,
    pub flags_mask: UInt32,
    pub input_channels: SInt16,
    pub output_channels: SInt16,
    pub supports_midi: bool,
    pub parameters: &'static [ParameterInfo],
    pub cocoa_view_info: Option<fn() -> AudioUnitCocoaViewInfo>,
    pub cocoa_view_class: Option<&'static str>,
    pub cocoa_view_bundle_id: Option<&'static str>,
    pub cocoa_view_init: Option<fn()>,
}

pub struct AuFactory {
    pub descriptor: AuComponentDescriptor,
    pub create: fn(f64, u32) -> Box<dyn AuPlugin>,
}

pub fn set_factory(factory: &'static AuFactory) {
    FACTORY_PTR.store(factory as *const _ as *mut AuFactory, Ordering::SeqCst);
}

pub const fn fourcc(bytes: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*bytes)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn au_component_factory(
    _in_desc: *const AudioComponentDescription,
) -> *mut AudioComponentPlugInInterface {
    log_msg("[AU] au_component_factory called");
    let fp = FACTORY_PTR.load(Ordering::SeqCst);
    log_msg(&format!("[AU] au_component_factory: factory_ptr={:?}", fp));
    if fp.is_null() {
        log_msg("[AU] factory_ptr is null, returning null");
        return ptr::null_mut();
    }
    log_msg("[AU] au_component_factory: creating vtable");
    let result = Box::into_raw(Box::new(AudioComponentPlugInInterface {
        Open: Some(open_instance),
        Close: Some(close_instance),
        Lookup: Some(lookup_selector),
        reserved: ptr::null_mut(),
    }));
    log_msg(&format!(
        "[AU] au_component_factory: returning vtable={:?}",
        result
    ));
    result
}

struct AuInstance {
    descriptor: &'static AuComponentDescriptor,
    plugin: Option<Box<dyn AuPlugin>>,
    input_format: AudioStreamBasicDescription,
    output_format: AudioStreamBasicDescription,
    input_render_cb: Option<AURenderCallbackStruct>,
    host_callbacks: Option<HostCallbackInfo>,
    sample_rate: f64,
    max_frames: u32,
    bypass: bool,
    input_buffers: Vec<Vec<f32>>,
    present_preset: AUPreset,
    property_listeners: Vec<PropertyListener>,
    component_instance: AudioUnit,
    param_values: HashMap<AudioUnitParameterID, AudioUnitParameterValue>,
    is_connected: bool,
    storage_handle: Handle,
}

struct PropertyListener {
    property_id: AudioUnitPropertyID,
    proc: AudioUnitPropertyListenerProc,
    user_data: *mut c_void,
}

fn notify_property_listeners(
    instance: &AuInstance,
    property_id: AudioUnitPropertyID,
    scope: AudioUnitScope,
    element: AudioUnitElement,
) {
    for listener in &instance.property_listeners {
        if listener.property_id == property_id {
            if let Some(proc) = listener.proc {
                unsafe {
                    proc(
                        listener.user_data,
                        instance.component_instance,
                        property_id,
                        scope,
                        element,
                    )
                };
            }
        }
    }
}

fn cfstring_from_ostype(value: OSType) -> CFStringRef {
    let bytes = value.to_be_bytes();
    let string = String::from_utf8_lossy(&bytes).into_owned();
    let cstring = CString::new(string).unwrap_or_else(|_| CString::new("????").unwrap());
    unsafe {
        CFStringCreateWithCString(kCFAllocatorDefault, cstring.as_ptr(), kCFStringEncodingUTF8)
    }
}

fn cfstring_from_str(value: &str) -> CFStringRef {
    let cstring = CString::new(value).unwrap_or_else(|_| CString::new("").unwrap());
    unsafe {
        CFStringCreateWithCString(kCFAllocatorDefault, cstring.as_ptr(), kCFStringEncodingUTF8)
    }
}

fn cfnumber_from_ostype(value: OSType) -> CFNumberRef {
    let val = value as i32;
    unsafe {
        CFNumberCreate(
            kCFAllocatorDefault,
            kCFNumberSInt32Type,
            &val as *const _ as *const c_void,
        )
    }
}

fn cfnumber_from_u32(value: u32) -> CFNumberRef {
    let val = value as i32;
    unsafe {
        CFNumberCreate(
            kCFAllocatorDefault,
            kCFNumberSInt32Type,
            &val as *const _ as *const c_void,
        )
    }
}

fn is_auvaltool_host() -> bool {
    std::env::args()
        .next()
        .and_then(|arg| {
            Path::new(&arg)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .map(|name| name == "auval" || name == "auvaltool")
        .unwrap_or(false)
}

impl AuInstance {
    fn new(descriptor: &'static AuComponentDescriptor, component_instance: AudioUnit) -> Self {
        let sample_rate = 44100.0;
        let max_frames = 512;
        let input_format = make_stream_format(sample_rate, descriptor.input_channels.max(0) as u32);
        let output_format =
            make_stream_format(sample_rate, descriptor.output_channels.max(1) as u32);
        Self {
            descriptor,
            plugin: None,
            input_format,
            output_format,
            input_render_cb: None,
            host_callbacks: None,
            sample_rate,
            max_frames,
            bypass: false,
            input_buffers: Vec::new(),
            present_preset: AUPreset {
                presetNumber: -1,
                presetName: ptr::null(),
            },
            property_listeners: Vec::new(),
            component_instance,
            param_values: descriptor
                .parameters
                .iter()
                .map(|param| (param.id, param.default))
                .collect(),
            is_connected: false,
            storage_handle: std::ptr::null_mut(),
        }
    }

    fn ensure_input_buffers(&mut self) {
        let channels = self.descriptor.input_channels.max(0) as usize;
        if channels == 0 {
            self.input_buffers.clear();
            return;
        }
        if self.input_buffers.len() != channels
            || self.input_buffers[0].len() != self.max_frames as usize
        {
            self.input_buffers = (0..channels)
                .map(|_| vec![0.0f32; self.max_frames as usize])
                .collect();
        }
    }
}

unsafe fn get_instance(mut self_ptr: *mut c_void) -> Option<&'static mut AuInstance> {
    if self_ptr.is_null() {
        return None;
    }

    if let Ok(map) = instance_map().lock() {
        if let Some(instance_ptr) = map.get(&(self_ptr as usize)) {
            let instance_ptr = *instance_ptr as *mut AuInstance;
            if !instance_ptr.is_null() {
                return Some(unsafe { &mut *instance_ptr });
            }
        }
    }

    let handle = unsafe { GetComponentInstanceStorage(self_ptr as AudioComponentInstance) };
    if !handle.is_null() {
        let instance_ptr = unsafe { *handle as *mut AuInstance };
        if !instance_ptr.is_null() {
            return Some(unsafe { &mut *instance_ptr });
        }
    }

    None
}

unsafe extern "C" fn open_instance(
    self_ptr: *mut c_void,
    _instance: AudioComponentInstance,
) -> OSStatus {
    log_msg("[AU] open_instance called");
    let factory = FACTORY_PTR.load(Ordering::SeqCst);
    log_msg(&format!("[AU] open_instance: factory_ptr={:?}", factory));
    if factory.is_null() {
        log_msg("[AU] open_instance: factory is null, returning error");
        return kAudioUnitErr_FailedInitialization;
    }
    let factory = &*factory;
    log_msg("[AU] open_instance: creating AuInstance");
    let instance_handle = if _instance.is_null() {
        self_ptr as AudioUnit
    } else {
        _instance as AudioUnit
    };
    let instance_box = Box::new(AuInstance::new(&factory.descriptor, instance_handle));
    log_msg("[AU] open_instance: AuInstance created");
    let instance_ptr = Box::into_raw(instance_box) as *mut AuInstance;
    let storage_handle = Box::into_raw(Box::new(instance_ptr as *mut c_void)) as Handle;
    SetComponentInstanceStorage(self_ptr as AudioComponentInstance, storage_handle);
    if !_instance.is_null() && _instance != self_ptr {
        SetComponentInstanceStorage(_instance as AudioComponentInstance, storage_handle);
    }
    (*instance_ptr).storage_handle = storage_handle;
    if let Ok(mut map) = instance_map().lock() {
        let instance_value = instance_ptr as usize;
        let self_key = self_ptr as usize;
        map.insert(self_key, instance_value);
        if !_instance.is_null() {
            let instance_key = _instance as usize;
            map.insert(instance_key, instance_value);
        }
    }
    log_msg("[AU] open_instance: returning noErr");
    noErr
}

unsafe extern "C" fn close_instance(self_ptr: *mut c_void) -> OSStatus {
    if self_ptr.is_null() {
        return noErr;
    }

    let storage_handle = unsafe { GetComponentInstanceStorage(self_ptr as AudioComponentInstance) };
    if !storage_handle.is_null() {
        unsafe {
            SetComponentInstanceStorage(self_ptr as AudioComponentInstance, std::ptr::null_mut())
        };
    }

    let instance_ptr = {
        let mut map = match instance_map().lock() {
            Ok(map) => map,
            Err(_) => return noErr,
        };
        let instance_ptr = map.remove(&(self_ptr as usize));
        if let Some(value) = instance_ptr {
            map.retain(|_, ptr| *ptr != value);
        }
        instance_ptr
    };
    if let Some(instance_ptr) = instance_ptr {
        let instance_ptr = instance_ptr as *mut AuInstance;
        if !instance_ptr.is_null() {
            let instance_handle = (*instance_ptr).component_instance as *mut c_void;
            if !instance_handle.is_null() && instance_handle != self_ptr {
                unsafe {
                    SetComponentInstanceStorage(
                        instance_handle as AudioComponentInstance,
                        std::ptr::null_mut(),
                    );
                }
            }
            let handle = (*instance_ptr).storage_handle;
            if !handle.is_null() {
                drop(Box::from_raw(handle));
            } else if !storage_handle.is_null() {
                drop(Box::from_raw(storage_handle));
            }
            drop(Box::from_raw(instance_ptr));
        }
    }
    // free the per-instance component interface
    unsafe {
        drop(Box::from_raw(
            self_ptr as *mut AudioComponentPlugInInterface,
        ));
    }

    noErr
}

unsafe extern "C" fn lookup_selector(selector: SInt16) -> AudioComponentMethod {
    let supports_midi = unsafe {
        let factory = FACTORY_PTR.load(Ordering::SeqCst);
        !factory.is_null() && (*factory).descriptor.supports_midi
    };
    let method = match selector {
        kAudioUnitInitializeSelect => audio_unit_initialize as *const c_void,
        kAudioUnitUninitializeSelect => audio_unit_uninitialize as *const c_void,
        kAudioUnitGetPropertyInfoSelect => audio_unit_get_property_info as *const c_void,
        kAudioUnitGetPropertySelect => audio_unit_get_property as *const c_void,
        kAudioUnitSetPropertySelect => audio_unit_set_property as *const c_void,
        kAudioUnitAddPropertyListenerSelect => audio_unit_add_property_listener as *const c_void,
        kAudioUnitRemovePropertyListenerSelect => {
            audio_unit_remove_property_listener as *const c_void
        }
        kAudioUnitRemovePropertyListenerWithUserDataSelect => {
            audio_unit_remove_property_listener_with_user_data as *const c_void
        }
        kAudioUnitAddRenderNotifySelect => audio_unit_add_render_notify as *const c_void,
        kAudioUnitRemoveRenderNotifySelect => audio_unit_remove_render_notify as *const c_void,
        kAudioUnitGetParameterSelect => audio_unit_get_parameter as *const c_void,
        kAudioUnitSetParameterSelect => audio_unit_set_parameter as *const c_void,
        kAudioUnitRenderSelect => audio_unit_render as *const c_void,
        kAudioUnitResetSelect => audio_unit_reset as *const c_void,
        kMusicDeviceMIDIEventSelect if supports_midi => music_device_midi_event as *const c_void,
        kMusicDeviceStartNoteSelect if supports_midi => music_device_start_note as *const c_void,
        kMusicDeviceStopNoteSelect if supports_midi => music_device_stop_note as *const c_void,
        _ => ptr::null(),
    };
    method as AudioComponentMethod
}

unsafe extern "C" fn audio_unit_initialize(self_ptr: *mut c_void) -> OSStatus {
    let Some(instance) = get_instance(self_ptr) else {
        return kAudioUnitErr_FailedInitialization;
    };
    if instance.plugin.is_some() {
        return kAudioUnitErr_Initialized;
    }
    let factory = FACTORY_PTR.load(Ordering::SeqCst);
    if factory.is_null() {
        return kAudioUnitErr_FailedInitialization;
    }
    let factory = &*factory;
    instance.plugin = Some((factory.create)(instance.sample_rate, instance.max_frames));
    if let Some(plugin) = instance.plugin.as_mut() {
        plugin.reset();
        for (id, value) in instance.param_values.clone() {
            plugin.set_parameter(id, value);
        }
    }
    noErr
}

unsafe extern "C" fn audio_unit_uninitialize(self_ptr: *mut c_void) -> OSStatus {
    let Some(instance) = get_instance(self_ptr) else {
        return kAudioUnitErr_Uninitialized;
    };
    instance.plugin = None;
    noErr
}

unsafe extern "C" fn audio_unit_get_property_info(
    self_ptr: *mut c_void,
    in_id: AudioUnitPropertyID,
    in_scope: AudioUnitScope,
    _in_element: AudioUnitElement,
    out_data_size: *mut UInt32,
    out_writable: *mut Boolean,
) -> OSStatus {
    let Some(instance) = get_instance(self_ptr) else {
        return kAudioUnitErr_Uninitialized;
    };
    let (size, writable) = match in_id {
        kAudioUnitProperty_AuRsInstance => {
            if in_scope != kAudioUnitScope_Global {
                return kAudioUnitErr_InvalidScope;
            }
            (std::mem::size_of::<*mut c_void>() as u32, 0)
        }
        kAudioUnitProperty_StreamFormat => {
            if in_scope == kAudioUnitScope_Input && instance.descriptor.input_channels == 0 {
                return kAudioUnitErr_InvalidElement;
            }
            (std::mem::size_of::<AudioStreamBasicDescription>() as u32, 1)
        }
        kAudioUnitProperty_SampleRate => (std::mem::size_of::<Float64>() as u32, 1),
        kAudioUnitProperty_MaximumFramesPerSlice => (std::mem::size_of::<UInt32>() as u32, 1),
        kAudioUnitProperty_ElementCount => (std::mem::size_of::<UInt32>() as u32, 0),
        kAudioUnitProperty_ParameterList => {
            if in_scope != kAudioUnitScope_Global {
                return kAudioUnitErr_InvalidScope;
            }
            let count = instance.descriptor.parameters.len();
            (
                (count * std::mem::size_of::<AudioUnitParameterID>()) as u32,
                0,
            )
        }
        kAudioUnitProperty_ParameterInfo => {
            if in_scope != kAudioUnitScope_Global {
                return kAudioUnitErr_InvalidScope;
            }
            (std::mem::size_of::<AudioUnitParameterInfo>() as u32, 0)
        }
        kAudioUnitProperty_HostCallbacks => {
            if in_scope != kAudioUnitScope_Global {
                return kAudioUnitErr_InvalidScope;
            }
            (std::mem::size_of::<HostCallbackInfo>() as u32, 1)
        }
        kAudioUnitProperty_Latency => {
            if in_scope != kAudioUnitScope_Global {
                return kAudioUnitErr_InvalidScope;
            }
            (std::mem::size_of::<Float64>() as u32, 0)
        }
        kAudioUnitProperty_TailTime => {
            if in_scope != kAudioUnitScope_Global {
                return kAudioUnitErr_InvalidScope;
            }
            (std::mem::size_of::<Float64>() as u32, 0)
        }
        kAudioUnitProperty_SupportedNumChannels => (std::mem::size_of::<AUChannelInfo>() as u32, 0),
        kAudioUnitProperty_BypassEffect => {
            if in_scope != kAudioUnitScope_Global {
                return kAudioUnitErr_InvalidScope;
            }
            (std::mem::size_of::<UInt32>() as u32, 1)
        }
        kAudioUnitProperty_SetRenderCallback => {
            (std::mem::size_of::<AURenderCallbackStruct>() as u32, 1)
        }
        kAudioUnitProperty_InPlaceProcessing => (std::mem::size_of::<UInt32>() as u32, 0),
        kAudioUnitProperty_CocoaUI => {
            if in_scope != kAudioUnitScope_Global {
                return kAudioUnitErr_InvalidScope;
            }
            if is_auvaltool_host() {
                log_msg("[AU] CocoaUI property rejected: auvaltool host");
                return kAudioUnitErr_InvalidProperty;
            }
            if instance.descriptor.cocoa_view_info.is_none()
                && instance.descriptor.cocoa_view_class.is_none()
            {
                log_msg("[AU] CocoaUI property rejected: no cocoa view info");
                return kAudioUnitErr_InvalidProperty;
            }
            log_msg("[AU] CocoaUI property info requested");
            (std::mem::size_of::<AudioUnitCocoaViewInfo>() as u32, 0)
        }
        kAudioUnitProperty_MakeConnection => (std::mem::size_of::<AudioUnitConnection>() as u32, 1),
        kAudioUnitProperty_PresentPreset => (std::mem::size_of::<AUPreset>() as u32, 1),
        kAudioUnitProperty_ClassInfo => (std::mem::size_of::<*const c_void>() as u32, 1),
        _ => return kAudioUnitErr_InvalidProperty,
    };

    if !out_data_size.is_null() {
        *out_data_size = size;
    }
    if !out_writable.is_null() {
        *out_writable = writable;
    }
    if in_id == kAudioUnitProperty_ElementCount
        && in_scope != kAudioUnitScope_Input
        && in_scope != kAudioUnitScope_Output
    {
        return kAudioUnitErr_InvalidScope;
    }
    noErr
}

unsafe extern "C" fn audio_unit_get_property(
    self_ptr: *mut c_void,
    in_id: AudioUnitPropertyID,
    in_scope: AudioUnitScope,
    in_element: AudioUnitElement,
    out_data: *mut c_void,
    io_data_size: *mut UInt32,
) -> OSStatus {
    let Some(instance) = get_instance(self_ptr) else {
        return kAudioUnitErr_Uninitialized;
    };

    if out_data.is_null() || io_data_size.is_null() {
        return kAudioUnitErr_InvalidPropertyValue;
    }

    match in_id {
        kAudioUnitProperty_AuRsInstance => {
            if in_scope != kAudioUnitScope_Global {
                return kAudioUnitErr_InvalidScope;
            }
            ptr::write(
                out_data as *mut *mut c_void,
                instance as *mut AuInstance as *mut c_void,
            );
            *io_data_size = std::mem::size_of::<*mut c_void>() as u32;
        }
        kAudioUnitProperty_StreamFormat => {
            if in_scope == kAudioUnitScope_Input && instance.descriptor.input_channels == 0 {
                return kAudioUnitErr_InvalidElement;
            }
            let format = match in_scope {
                kAudioUnitScope_Input => &instance.input_format,
                kAudioUnitScope_Output => &instance.output_format,
                _ => return kAudioUnitErr_InvalidScope,
            };
            ptr::write(
                out_data as *mut AudioStreamBasicDescription,
                ptr::read(format),
            );
            *io_data_size = std::mem::size_of::<AudioStreamBasicDescription>() as u32;
        }
        kAudioUnitProperty_SampleRate => {
            ptr::write(out_data as *mut Float64, instance.sample_rate);
            *io_data_size = std::mem::size_of::<Float64>() as u32;
        }
        kAudioUnitProperty_MaximumFramesPerSlice => {
            ptr::write(out_data as *mut UInt32, instance.max_frames);
            *io_data_size = std::mem::size_of::<UInt32>() as u32;
        }
        kAudioUnitProperty_ElementCount => {
            let value = match in_scope {
                kAudioUnitScope_Input => {
                    if instance.descriptor.input_channels > 0 {
                        1
                    } else {
                        0
                    }
                }
                kAudioUnitScope_Output => 1,
                _ => return kAudioUnitErr_InvalidScope,
            };
            ptr::write(out_data as *mut UInt32, value);
            *io_data_size = std::mem::size_of::<UInt32>() as u32;
        }
        kAudioUnitProperty_ParameterList => {
            if in_scope != kAudioUnitScope_Global {
                return kAudioUnitErr_InvalidScope;
            }
            let params = instance.descriptor.parameters;
            let out = out_data as *mut AudioUnitParameterID;
            for (idx, param) in params.iter().enumerate() {
                ptr::write(out.add(idx), param.id);
            }
            *io_data_size = (params.len() * std::mem::size_of::<AudioUnitParameterID>()) as u32;
        }
        kAudioUnitProperty_ParameterInfo => {
            if in_scope != kAudioUnitScope_Global {
                return kAudioUnitErr_InvalidScope;
            }
            let param = match find_param(instance.descriptor.parameters, in_element) {
                Ok(param) => param,
                Err(status) => return status,
            };
            let info = build_parameter_info(param);
            ptr::write(out_data as *mut AudioUnitParameterInfo, info);
            *io_data_size = std::mem::size_of::<AudioUnitParameterInfo>() as u32;
        }
        kAudioUnitProperty_HostCallbacks => {
            if in_scope != kAudioUnitScope_Global {
                return kAudioUnitErr_InvalidScope;
            }
            let Some(callbacks) = instance.host_callbacks else {
                return kAudioUnitErr_InvalidPropertyValue;
            };
            ptr::write(out_data as *mut HostCallbackInfo, callbacks);
            *io_data_size = std::mem::size_of::<HostCallbackInfo>() as u32;
        }
        kAudioUnitProperty_Latency => {
            if in_scope != kAudioUnitScope_Global {
                return kAudioUnitErr_InvalidScope;
            }
            ptr::write(out_data as *mut Float64, 0.0);
            *io_data_size = std::mem::size_of::<Float64>() as u32;
        }
        kAudioUnitProperty_TailTime => {
            if in_scope != kAudioUnitScope_Global {
                return kAudioUnitErr_InvalidScope;
            }
            ptr::write(out_data as *mut Float64, 0.0);
            *io_data_size = std::mem::size_of::<Float64>() as u32;
        }
        kAudioUnitProperty_SupportedNumChannels => {
            let info = AUChannelInfo {
                inChannels: instance.descriptor.input_channels,
                outChannels: instance.descriptor.output_channels,
            };
            ptr::write(out_data as *mut AUChannelInfo, info);
            *io_data_size = std::mem::size_of::<AUChannelInfo>() as u32;
        }
        kAudioUnitProperty_BypassEffect => {
            if in_scope != kAudioUnitScope_Global {
                return kAudioUnitErr_InvalidScope;
            }
            ptr::write(out_data as *mut UInt32, if instance.bypass { 1 } else { 0 });
            *io_data_size = std::mem::size_of::<UInt32>() as u32;
        }
        kAudioUnitProperty_SetRenderCallback => {
            if let Some(cb) = instance.input_render_cb.as_ref() {
                let copy = AURenderCallbackStruct {
                    inputProc: cb.inputProc,
                    inputProcRefCon: cb.inputProcRefCon,
                };
                ptr::write(out_data as *mut AURenderCallbackStruct, copy);
                *io_data_size = std::mem::size_of::<AURenderCallbackStruct>() as u32;
            } else {
                return kAudioUnitErr_InvalidPropertyValue;
            }
        }
        kAudioUnitProperty_InPlaceProcessing => {
            ptr::write(out_data as *mut UInt32, 0);
            *io_data_size = std::mem::size_of::<UInt32>() as u32;
        }
        kAudioUnitProperty_CocoaUI => {
            if in_scope != kAudioUnitScope_Global {
                return kAudioUnitErr_InvalidScope;
            }
            if is_auvaltool_host() {
                log_msg("[AU] CocoaUI get rejected: auvaltool host");
                return kAudioUnitErr_InvalidProperty;
            }
            if let Some(info_fn) = instance.descriptor.cocoa_view_info {
                log_msg("[AU] CocoaUI get: using cocoa_view_info");
                let info = info_fn();
                ptr::write(out_data as *mut AudioUnitCocoaViewInfo, info);
                *io_data_size = std::mem::size_of::<AudioUnitCocoaViewInfo>() as u32;
                return noErr;
            }
            let Some(class_name) = instance.descriptor.cocoa_view_class else {
                log_msg("[AU] CocoaUI get: missing cocoa_view_class");
                return kAudioUnitErr_InvalidProperty;
            };
            let Some(bundle_id) = instance.descriptor.cocoa_view_bundle_id else {
                log_msg("[AU] CocoaUI get: missing cocoa_view_bundle_id");
                return kAudioUnitErr_InvalidPropertyValue;
            };
            if let Some(init) = instance.descriptor.cocoa_view_init {
                init();
            }
            let bundle_id_cf = cfstring_from_str(bundle_id);
            let bundle = unsafe { CFBundleGetBundleWithIdentifier(bundle_id_cf) };
            if bundle.is_null() {
                return kAudioUnitErr_InvalidPropertyValue;
            }
            let bundle_url = unsafe { CFBundleCopyBundleURL(bundle) };
            if bundle_url.is_null() {
                return kAudioUnitErr_InvalidPropertyValue;
            }
            let class_name_cf = cfstring_from_str(class_name);
            let info = AudioUnitCocoaViewInfo {
                mCocoaAUViewBundleLocation: bundle_url,
                mCocoaAUViewClass: [class_name_cf],
            };
            ptr::write(out_data as *mut AudioUnitCocoaViewInfo, info);
            *io_data_size = std::mem::size_of::<AudioUnitCocoaViewInfo>() as u32;
        }
        kAudioUnitProperty_MakeConnection => {
            return kAudioUnitErr_InvalidProperty;
        }
        kAudioUnitProperty_PresentPreset => {
            let preset = AUPreset {
                presetNumber: instance.present_preset.presetNumber,
                presetName: instance.present_preset.presetName,
            };
            ptr::write(out_data as *mut AUPreset, preset);
            *io_data_size = std::mem::size_of::<AUPreset>() as u32;
        }
        kAudioUnitProperty_ClassInfo => {
            let key_type = cfstring_from_str("type");
            let key_subtype = cfstring_from_str("subtype");
            let key_manufacturer = cfstring_from_str("manufacturer");
            let key_version = cfstring_from_str("version");
            let key_name = cfstring_from_str("name");
            let keys = [
                key_type,
                key_subtype,
                key_manufacturer,
                key_version,
                key_name,
            ];
            let val_type = cfnumber_from_ostype(instance.descriptor.component_type);
            let val_subtype = cfnumber_from_ostype(instance.descriptor.component_subtype);
            let val_manufacturer = cfnumber_from_ostype(instance.descriptor.manufacturer);
            let val_version = cfnumber_from_u32(instance.descriptor.version);
            let val_name = cfstring_from_str(instance.descriptor.name);
            let values = [
                val_type,
                val_subtype,
                val_manufacturer,
                val_version,
                val_name,
            ];
            let dict = unsafe {
                CFDictionaryCreate(
                    kCFAllocatorDefault,
                    keys.as_ptr() as *const *const c_void,
                    values.as_ptr() as *const *const c_void,
                    keys.len() as c_long,
                    &kCFTypeDictionaryKeyCallBacks,
                    &kCFTypeDictionaryValueCallBacks,
                )
            };
            for key in keys {
                unsafe { CFRelease(key as CFTypeRef) };
            }
            for value in values {
                unsafe { CFRelease(value as CFTypeRef) };
            }
            ptr::write(out_data as *mut CFDictionaryRef, dict);
            *io_data_size = std::mem::size_of::<CFDictionaryRef>() as u32;
        }
        _ => return kAudioUnitErr_InvalidProperty,
    }

    noErr
}

unsafe extern "C" fn audio_unit_set_property(
    self_ptr: *mut c_void,
    in_id: AudioUnitPropertyID,
    in_scope: AudioUnitScope,
    _in_element: AudioUnitElement,
    in_data: *const c_void,
    in_data_size: UInt32,
) -> OSStatus {
    let Some(instance) = get_instance(self_ptr) else {
        return kAudioUnitErr_Uninitialized;
    };

    if in_data.is_null() {
        return kAudioUnitErr_InvalidPropertyValue;
    }

    match in_id {
        kAudioUnitProperty_StreamFormat => {
            if in_data_size as usize != std::mem::size_of::<AudioStreamBasicDescription>() {
                return kAudioUnitErr_InvalidPropertyValue;
            }
            if instance.plugin.is_some() {
                return kAudioUnitErr_Initialized;
            }
            let format = ptr::read(in_data as *const AudioStreamBasicDescription);
            if !is_supported_format(&format, in_scope, instance.descriptor) {
                return kAudioUnitErr_FormatNotSupported;
            }
            match in_scope {
                kAudioUnitScope_Input => instance.input_format = format,
                kAudioUnitScope_Output => instance.output_format = format,
                _ => return kAudioUnitErr_InvalidScope,
            }
        }
        kAudioUnitProperty_SampleRate => {
            if in_data_size as usize != std::mem::size_of::<Float64>() {
                return kAudioUnitErr_InvalidPropertyValue;
            }
            if instance.plugin.is_some() {
                return kAudioUnitErr_Initialized;
            }
            instance.sample_rate = ptr::read(in_data as *const Float64);
            instance.input_format.mSampleRate = instance.sample_rate;
            instance.output_format.mSampleRate = instance.sample_rate;
        }
        kAudioUnitProperty_MaximumFramesPerSlice => {
            if in_data_size as usize != std::mem::size_of::<UInt32>() {
                return kAudioUnitErr_InvalidPropertyValue;
            }
            if instance.plugin.is_some() {
                return kAudioUnitErr_Initialized;
            }
            instance.max_frames = ptr::read(in_data as *const UInt32);
            notify_property_listeners(
                instance,
                kAudioUnitProperty_MaximumFramesPerSlice,
                in_scope,
                0,
            );
        }
        kAudioUnitProperty_HostCallbacks => {
            if in_scope != kAudioUnitScope_Global {
                return kAudioUnitErr_InvalidScope;
            }
            if in_data_size as usize != std::mem::size_of::<HostCallbackInfo>() {
                return kAudioUnitErr_InvalidPropertyValue;
            }
            let callbacks = ptr::read(in_data as *const HostCallbackInfo);
            instance.host_callbacks = Some(callbacks);
        }
        kAudioUnitProperty_BypassEffect => {
            if in_data_size as usize != std::mem::size_of::<UInt32>() {
                return kAudioUnitErr_InvalidPropertyValue;
            }
            instance.bypass = ptr::read(in_data as *const UInt32) != 0;
        }
        kAudioUnitProperty_SetRenderCallback => {
            if in_data_size as usize != std::mem::size_of::<AURenderCallbackStruct>() {
                return kAudioUnitErr_InvalidPropertyValue;
            }
            let cb = ptr::read(in_data as *const AURenderCallbackStruct);
            instance.input_render_cb = Some(cb);
        }
        kAudioUnitProperty_MakeConnection => {
            if in_data_size as usize != std::mem::size_of::<AudioUnitConnection>() {
                return kAudioUnitErr_InvalidPropertyValue;
            }
            instance.is_connected = true;
            return noErr;
        }
        kAudioUnitProperty_PresentPreset => {
            if in_data_size as usize != std::mem::size_of::<AUPreset>() {
                return kAudioUnitErr_InvalidPropertyValue;
            }
            instance.present_preset = ptr::read(in_data as *const AUPreset);
        }
        kAudioUnitProperty_ClassInfo => {
            // Accept and ignore class info writes.
        }
        _ => return kAudioUnitErr_InvalidProperty,
    }

    noErr
}

unsafe extern "C" fn audio_unit_add_property_listener(
    _self_ptr: *mut c_void,
    _prop: AudioUnitPropertyID,
    _proc: AudioUnitPropertyListenerProc,
    _user_data: *mut c_void,
) -> OSStatus {
    let Some(instance) = get_instance(_self_ptr) else {
        return noErr;
    };
    if let Some(proc) = _proc {
        instance.property_listeners.push(PropertyListener {
            property_id: _prop,
            proc: Some(proc),
            user_data: _user_data,
        });
    }
    noErr
}

unsafe extern "C" fn audio_unit_remove_property_listener(
    _self_ptr: *mut c_void,
    _prop: AudioUnitPropertyID,
    _proc: AudioUnitPropertyListenerProc,
) -> OSStatus {
    let Some(instance) = get_instance(_self_ptr) else {
        return noErr;
    };
    instance
        .property_listeners
        .retain(|listener| listener.property_id != _prop);
    noErr
}

unsafe extern "C" fn audio_unit_remove_property_listener_with_user_data(
    _self_ptr: *mut c_void,
    _prop: AudioUnitPropertyID,
    _proc: AudioUnitPropertyListenerProc,
    _user_data: *mut c_void,
) -> OSStatus {
    let Some(instance) = get_instance(_self_ptr) else {
        return noErr;
    };
    instance
        .property_listeners
        .retain(|listener| listener.property_id != _prop);
    noErr
}

unsafe extern "C" fn audio_unit_add_render_notify(
    _self_ptr: *mut c_void,
    _proc: AURenderCallback,
    _user_data: *mut c_void,
) -> OSStatus {
    noErr
}

unsafe extern "C" fn audio_unit_remove_render_notify(
    _self_ptr: *mut c_void,
    _proc: AURenderCallback,
    _user_data: *mut c_void,
) -> OSStatus {
    noErr
}

unsafe extern "C" fn audio_unit_get_parameter(
    self_ptr: *mut c_void,
    in_id: AudioUnitParameterID,
    in_scope: AudioUnitScope,
    _in_element: AudioUnitElement,
    out_value: *mut AudioUnitParameterValue,
) -> OSStatus {
    let Some(instance) = get_instance(self_ptr) else {
        return kAudioUnitErr_Uninitialized;
    };
    if in_scope != kAudioUnitScope_Global {
        return kAudioUnitErr_InvalidScope;
    }
    if out_value.is_null() {
        return kAudioUnitErr_InvalidPropertyValue;
    }
    let value = instance
        .param_values
        .get(&in_id)
        .copied()
        .unwrap_or_else(|| {
            instance
                .plugin
                .as_ref()
                .map(|plugin| plugin.get_parameter(in_id))
                .unwrap_or(0.0)
        });
    ptr::write(out_value, value);
    noErr
}

unsafe extern "C" fn audio_unit_set_parameter(
    self_ptr: *mut c_void,
    in_id: AudioUnitParameterID,
    in_scope: AudioUnitScope,
    _in_element: AudioUnitElement,
    in_value: AudioUnitParameterValue,
    _in_buffer_offset_in_frames: UInt32,
) -> OSStatus {
    let Some(instance) = get_instance(self_ptr) else {
        return kAudioUnitErr_Uninitialized;
    };
    if in_scope != kAudioUnitScope_Global {
        return kAudioUnitErr_InvalidScope;
    }
    instance.param_values.insert(in_id, in_value);
    if let Some(plugin) = instance.plugin.as_mut() {
        plugin.set_parameter(in_id, in_value);
    }
    noErr
}

pub fn set_parameter_direct(
    unit: *mut c_void,
    id: AudioUnitParameterID,
    value: AudioUnitParameterValue,
) -> OSStatus {
    unsafe {
        let Some(instance) = get_instance(unit) else {
            return kAudioUnitErr_Uninitialized;
        };
        instance.param_values.insert(id, value);
        if let Some(plugin) = instance.plugin.as_mut() {
            plugin.set_parameter(id, value);
        }
        noErr
    }
}

pub fn get_parameter_direct(
    unit: *mut c_void,
    id: AudioUnitParameterID,
) -> Result<AudioUnitParameterValue, OSStatus> {
    unsafe {
        let Some(instance) = get_instance(unit) else {
            return Err(kAudioUnitErr_Uninitialized);
        };
        let value = instance.param_values.get(&id).copied().unwrap_or_else(|| {
            instance
                .plugin
                .as_ref()
                .map(|plugin| plugin.get_parameter(id))
                .unwrap_or(0.0)
        });
        Ok(value)
    }
}

unsafe extern "C" fn audio_unit_render(
    self_ptr: *mut c_void,
    io_action_flags: *mut AudioUnitRenderActionFlags,
    in_time_stamp: *const AudioTimeStamp,
    in_output_bus_number: UInt32,
    in_number_frames: UInt32,
    io_data: *mut AudioBufferList,
) -> OSStatus {
    let Some(instance) = get_instance(self_ptr) else {
        return kAudioUnitErr_Uninitialized;
    };

    if io_data.is_null() {
        return kAudioUnitErr_InvalidPropertyValue;
    }

    if in_number_frames > instance.max_frames {
        return kAudioUnitErr_TooManyFramesToProcess;
    }

    let mut output = BufferList::from_raw(io_data, in_number_frames as usize);
    let mut input_list = build_input_list(
        instance,
        io_action_flags,
        in_time_stamp,
        in_output_bus_number,
        in_number_frames,
    );

    if instance.descriptor.input_channels > 0 && input_list.is_none() {
        if instance.is_connected {
            for ch in 0..output.len() {
                let out = output.channel_mut(ch);
                for sample in out.iter_mut() {
                    *sample = 0.0;
                }
            }
            return noErr;
        }
        return kAudioUnitErr_NoConnection;
    }

    if instance.bypass {
        if let Some(list) = input_list.as_mut() {
            let input = list.as_buffer_list(in_number_frames as usize);
            copy_buffers(input, &mut output, in_number_frames as usize);
            return noErr;
        }
    }

    let Some(plugin) = instance.plugin.as_mut() else {
        return kAudioUnitErr_Uninitialized;
    };
    let input = input_list
        .as_mut()
        .map(|list| list.as_buffer_list(in_number_frames as usize));
    let host_ptr = instance
        .host_callbacks
        .as_ref()
        .map(|cb| cb as *const HostCallbackInfo)
        .unwrap_or(ptr::null());
    set_current_host_callbacks(host_ptr);
    plugin.process(input, &mut output, in_number_frames as usize);
    set_current_host_callbacks(ptr::null());

    noErr
}

unsafe extern "C" fn audio_unit_reset(
    self_ptr: *mut c_void,
    _in_scope: AudioUnitScope,
    _in_element: AudioUnitElement,
) -> OSStatus {
    let Some(instance) = get_instance(self_ptr) else {
        return kAudioUnitErr_Uninitialized;
    };
    if let Some(plugin) = instance.plugin.as_mut() {
        plugin.reset();
    }
    noErr
}

unsafe extern "C" fn music_device_midi_event(
    self_ptr: *mut c_void,
    in_status: UInt32,
    in_data1: UInt32,
    in_data2: UInt32,
    in_offset_sample_frame: UInt32,
) -> OSStatus {
    let Some(instance) = get_instance(self_ptr) else {
        return kAudioUnitErr_Uninitialized;
    };
    if !instance.descriptor.supports_midi {
        return noErr;
    }
    if let Some(plugin) = instance.plugin.as_mut() {
        plugin.handle_midi_event(
            (in_status & 0xFF) as u8,
            (in_data1 & 0xFF) as u8,
            (in_data2 & 0xFF) as u8,
            in_offset_sample_frame,
        );
    }
    noErr
}

unsafe extern "C" fn music_device_start_note(
    self_ptr: *mut c_void,
    _in_group_id: MusicDeviceGroupID,
    in_params: *const MusicDeviceStdNoteParams,
    out_note_id: *mut NoteInstanceID,
    in_offset_sample_frame: UInt32,
) -> OSStatus {
    let Some(instance) = get_instance(self_ptr) else {
        return kAudioUnitErr_Uninitialized;
    };
    if !instance.descriptor.supports_midi {
        return noErr;
    }
    if in_params.is_null() || out_note_id.is_null() {
        return kAudioUnitErr_InvalidParameter;
    }
    let params = &*in_params;
    let pitch = params.mPitch;
    let velocity = params.mVelocity;
    let mut note_id = 0;
    if let Some(plugin) = instance.plugin.as_mut() {
        note_id = plugin.start_note(pitch, velocity, in_offset_sample_frame);
    }
    ptr::write(out_note_id, note_id);
    noErr
}

unsafe extern "C" fn music_device_stop_note(
    self_ptr: *mut c_void,
    _in_group_id: MusicDeviceGroupID,
    in_note_id: NoteInstanceID,
    in_offset_sample_frame: UInt32,
) -> OSStatus {
    let Some(instance) = get_instance(self_ptr) else {
        return kAudioUnitErr_Uninitialized;
    };
    if !instance.descriptor.supports_midi {
        return noErr;
    }
    if let Some(plugin) = instance.plugin.as_mut() {
        plugin.stop_note(in_note_id, in_offset_sample_frame);
    }
    noErr
}

fn make_stream_format(sample_rate: f64, channels: u32) -> AudioStreamBasicDescription {
    let bytes_per_frame = std::mem::size_of::<f32>() as u32;
    AudioStreamBasicDescription {
        mSampleRate: sample_rate,
        mFormatID: kAudioFormatLinearPCM,
        mFormatFlags: kAudioFormatFlagIsFloat
            | kAudioFormatFlagIsPacked
            | kAudioFormatFlagIsNonInterleaved,
        mBytesPerPacket: bytes_per_frame,
        mFramesPerPacket: 1,
        mBytesPerFrame: bytes_per_frame,
        mChannelsPerFrame: channels,
        mBitsPerChannel: 32,
        mReserved: 0,
    }
}

fn is_supported_format(
    format: &AudioStreamBasicDescription,
    scope: AudioUnitScope,
    descriptor: &AuComponentDescriptor,
) -> bool {
    let channels = match scope {
        kAudioUnitScope_Input => descriptor.input_channels.max(0) as u32,
        kAudioUnitScope_Output => descriptor.output_channels.max(1) as u32,
        _ => return false,
    };
    format.mFormatID == kAudioFormatLinearPCM
        && (format.mFormatFlags & kAudioFormatFlagIsFloat) != 0
        && (format.mFormatFlags & kAudioFormatFlagIsPacked) != 0
        && (format.mFormatFlags & kAudioFormatFlagIsNonInterleaved) != 0
        && format.mChannelsPerFrame == channels
}

fn find_param<'a>(
    params: &'a [ParameterInfo],
    id: AudioUnitParameterID,
) -> Result<&'a ParameterInfo, OSStatus> {
    params
        .iter()
        .find(|param| param.id == id)
        .ok_or(kAudioUnitErr_InvalidParameter)
}

fn build_parameter_info(param: &ParameterInfo) -> AudioUnitParameterInfo {
    let mut name = [0i8; 52];
    let bytes = param.name.as_bytes();
    let max_len = name.len().saturating_sub(1).min(bytes.len());
    for i in 0..max_len {
        name[i] = bytes[i] as i8;
    }

    AudioUnitParameterInfo {
        name,
        unitName: ptr::null(),
        clumpID: 0,
        cfNameString: ptr::null(),
        unit: param.unit.as_au_unit(),
        minValue: param.min,
        maxValue: param.max,
        defaultValue: param.default,
        flags: kAudioUnitParameterFlag_IsReadable | kAudioUnitParameterFlag_IsWritable,
    }
}

struct InputList {
    raw: Vec<u8>,
    list_ptr: *mut AudioBufferList,
}

impl InputList {
    unsafe fn as_buffer_list(&mut self, frames: usize) -> BufferList<'_> {
        BufferList::from_raw(self.list_ptr, frames)
    }
}

unsafe fn build_input_list(
    instance: &mut AuInstance,
    io_action_flags: *mut AudioUnitRenderActionFlags,
    in_time_stamp: *const AudioTimeStamp,
    in_output_bus_number: UInt32,
    in_number_frames: UInt32,
) -> Option<InputList> {
    let (input_proc, input_refcon) = {
        let cb = instance.input_render_cb.as_ref()?;
        let input_proc = cb.inputProc?;
        (input_proc, cb.inputProcRefCon)
    };
    instance.ensure_input_buffers();
    if instance.input_buffers.is_empty() {
        return None;
    }

    let mut buffers: Vec<AudioBuffer> = instance
        .input_buffers
        .iter_mut()
        .map(|buffer| AudioBuffer {
            mNumberChannels: 1,
            mDataByteSize: (in_number_frames as usize * std::mem::size_of::<f32>()) as u32,
            mData: buffer.as_mut_ptr() as *mut c_void,
        })
        .collect();

    let buffer_count = buffers.len();
    let list_size = std::mem::size_of::<AudioBufferList>()
        + (buffer_count.saturating_sub(1)) * std::mem::size_of::<AudioBuffer>();
    let mut raw = vec![0u8; list_size];
    let list_ptr = raw.as_mut_ptr() as *mut AudioBufferList;
    (*list_ptr).mNumberBuffers = buffer_count as u32;
    let buffer_ptr = (*list_ptr).mBuffers.as_mut_ptr();
    for (index, buffer) in buffers.iter_mut().enumerate() {
        let temp = AudioBuffer {
            mNumberChannels: buffer.mNumberChannels,
            mDataByteSize: buffer.mDataByteSize,
            mData: buffer.mData,
        };
        ptr::write(buffer_ptr.add(index), temp);
    }

    let status = input_proc(
        input_refcon,
        io_action_flags,
        in_time_stamp,
        in_output_bus_number,
        in_number_frames,
        list_ptr,
    );
    if status != noErr {
        return None;
    }

    Some(InputList { raw, list_ptr })
}

unsafe fn copy_buffers(mut input: BufferList<'_>, output: &mut BufferList<'_>, frames: usize) {
    let channels = output.len().min(input.len());
    for ch in 0..channels {
        let in_buf = input.channel_mut(ch);
        let out_buf = output.channel_mut(ch);
        let count = frames.min(in_buf.len()).min(out_buf.len());
        out_buf[..count].copy_from_slice(&in_buf[..count]);
    }
}
