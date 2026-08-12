use super::{
    process_frame_count, validate_host_events, GuiGestureEvidence, HostEvent, HostPlugin,
    ParamInfo, PluginFormat, PluginInfo,
};
use crate::gui_window::PluginGuiWindow;
use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{
    AtomicBool, AtomicI32, AtomicPtr, AtomicU32, AtomicU64, AtomicUsize, Ordering,
};
use vst3_sys::base::ibstream::{IBStreamVtbl, StreamSeekMode};
use vst3_sys::base::ipluginbase::*;
use vst3_sys::base::types::*;
use vst3_sys::gui::iplugview::*;
use vst3_sys::vst::iaudioprocessor::*;
use vst3_sys::vst::icomponent::*;
use vst3_sys::vst::ieditcontroller::*;
use vst3_sys::vst::ievents::*;
use vst3_sys::vst::iparameters::*;
use vst3_sys::vst::ivstmessage::*;
use vst3_sys::vst::types::*;

#[repr(C)]
struct HostComHeader {
    _vtbl: *const c_void,
    refs: AtomicU32,
}

#[repr(C)]
struct Vst3HostComponentHandler {
    vtbl: *const IComponentHandlerVtbl,
    refs: AtomicU32,
    begin_count: AtomicUsize,
    perform_count: AtomicUsize,
    end_count: AtomicUsize,
    last_param_id: AtomicU32,
    last_param_value: AtomicU64,
    gesture_active: AtomicBool,
    gesture_param_id: AtomicU32,
    gesture_has_value: AtomicBool,
    gesture_value: AtomicU64,
    completed_gesture_count: AtomicUsize,
    completed_gesture_param_id: AtomicU32,
    completed_gesture_value: AtomicU64,
    restart_flags: AtomicI32,
}

impl Vst3HostComponentHandler {
    fn new() -> Self {
        Self {
            vtbl: &HOST_COMPONENT_HANDLER_VTBL,
            refs: AtomicU32::new(1),
            begin_count: AtomicUsize::new(0),
            perform_count: AtomicUsize::new(0),
            end_count: AtomicUsize::new(0),
            last_param_id: AtomicU32::new(0),
            last_param_value: AtomicU64::new(0.0_f64.to_bits()),
            gesture_active: AtomicBool::new(false),
            gesture_param_id: AtomicU32::new(0),
            gesture_has_value: AtomicBool::new(false),
            gesture_value: AtomicU64::new(0.0_f64.to_bits()),
            completed_gesture_count: AtomicUsize::new(0),
            completed_gesture_param_id: AtomicU32::new(0),
            completed_gesture_value: AtomicU64::new(0.0_f64.to_bits()),
            restart_flags: AtomicI32::new(0),
        }
    }
}

#[repr(C)]
struct Vst3HostPlugFrame {
    vtbl: *const IPlugFrameVtbl,
    refs: AtomicU32,
    attached: AtomicBool,
    window: AtomicPtr<PluginGuiWindow>,
    resize_count: AtomicUsize,
    width: AtomicU32,
    height: AtomicU32,
}

impl Vst3HostPlugFrame {
    fn new() -> Self {
        Self {
            vtbl: &HOST_PLUG_FRAME_VTBL,
            refs: AtomicU32::new(1),
            attached: AtomicBool::new(false),
            window: AtomicPtr::new(ptr::null_mut()),
            resize_count: AtomicUsize::new(0),
            width: AtomicU32::new(0),
            height: AtomicU32::new(0),
        }
    }
}

pub struct Vst3HostPlugin {
    info: PluginInfo,
    _lib: libloading::Library,
    component: *mut c_void,
    processor: *mut c_void,
    controller: *mut c_void,
    component_vtbl: *const IComponentVtbl,
    processor_vtbl: *const IAudioProcessorVtbl,
    controller_vtbl: *const IEditControllerVtbl,
    component_connection: *mut c_void,
    controller_connection: *mut c_void,
    input_buffers: Vec<Vec<f32>>,
    output_buffers: Vec<Vec<f32>>,
    input_channel_ptrs: Vec<*mut f32>,
    output_channel_ptrs: Vec<*mut f32>,
    input_bus_channels: Vec<usize>,
    output_bus_channels: Vec<usize>,
    sample_rate: f64,
    max_frames: u32,
    _component_handler: Box<Vst3HostComponentHandler>,
    plug_frame: Box<Vst3HostPlugFrame>,
    // GUI state
    view: *mut c_void,
}

unsafe impl Send for Vst3HostPlugin {}

unsafe extern "system" fn host_component_handler_query(
    this: *mut c_void,
    iid: *const TUID,
    object: *mut *mut c_void,
) -> tresult {
    if this.is_null() || iid.is_null() || object.is_null() {
        return kInvalidArgument;
    }
    if iid_equal(&*iid, &vst3_sys::base::iid::IUnknown)
        || iid_equal(&*iid, &vst3_sys::vst::iid::IComponentHandler)
    {
        *object = this;
        host_interface_add_ref(this);
        kResultOk
    } else {
        *object = ptr::null_mut();
        kNoInterface
    }
}

unsafe extern "system" fn host_plug_frame_query(
    this: *mut c_void,
    iid: *const TUID,
    object: *mut *mut c_void,
) -> tresult {
    if this.is_null() || iid.is_null() || object.is_null() {
        return kInvalidArgument;
    }
    if iid_equal(&*iid, &vst3_sys::base::iid::IUnknown)
        || iid_equal(&*iid, &vst3_sys::gui::iid::IPlugFrame)
    {
        *object = this;
        host_interface_add_ref(this);
        kResultOk
    } else {
        *object = ptr::null_mut();
        kNoInterface
    }
}

unsafe extern "system" fn host_interface_add_ref(this: *mut c_void) -> uint32 {
    let refs = &(*(this as *const HostComHeader)).refs;
    refs.fetch_add(1, Ordering::AcqRel).saturating_add(1)
}

unsafe extern "system" fn host_interface_release(this: *mut c_void) -> uint32 {
    let refs = &(*(this as *const HostComHeader)).refs;
    refs.fetch_sub(1, Ordering::AcqRel).saturating_sub(1)
}

unsafe extern "system" fn host_begin_edit(this: *mut c_void, id: ParamID) -> tresult {
    let handler = &*(this as *const Vst3HostComponentHandler);
    handler.last_param_id.store(id, Ordering::Release);
    handler.gesture_param_id.store(id, Ordering::Release);
    handler.gesture_has_value.store(false, Ordering::Release);
    handler.gesture_active.store(true, Ordering::Release);
    handler.begin_count.fetch_add(1, Ordering::AcqRel);
    kResultOk
}

unsafe extern "system" fn host_perform_edit(
    this: *mut c_void,
    id: ParamID,
    value: ParamValue,
) -> tresult {
    if !value.is_finite() {
        return kInvalidArgument;
    }
    let handler = &*(this as *const Vst3HostComponentHandler);
    handler.last_param_id.store(id, Ordering::Release);
    handler
        .last_param_value
        .store(value.clamp(0.0, 1.0).to_bits(), Ordering::Release);
    handler.perform_count.fetch_add(1, Ordering::AcqRel);
    if handler.gesture_active.load(Ordering::Acquire)
        && handler.gesture_param_id.load(Ordering::Acquire) == id
    {
        handler
            .gesture_value
            .store(value.clamp(0.0, 1.0).to_bits(), Ordering::Release);
        handler.gesture_has_value.store(true, Ordering::Release);
    }
    kResultOk
}

unsafe extern "system" fn host_end_edit(this: *mut c_void, id: ParamID) -> tresult {
    let handler = &*(this as *const Vst3HostComponentHandler);
    handler.last_param_id.store(id, Ordering::Release);
    handler.end_count.fetch_add(1, Ordering::AcqRel);
    let completed = handler.gesture_active.load(Ordering::Acquire)
        && handler.gesture_param_id.load(Ordering::Acquire) == id
        && handler.gesture_has_value.load(Ordering::Acquire);
    handler.gesture_active.store(false, Ordering::Release);
    if completed {
        handler
            .completed_gesture_param_id
            .store(id, Ordering::Release);
        handler.completed_gesture_value.store(
            handler.gesture_value.load(Ordering::Acquire),
            Ordering::Release,
        );
        handler
            .completed_gesture_count
            .fetch_add(1, Ordering::AcqRel);
    }
    kResultOk
}

unsafe extern "system" fn host_restart_component(this: *mut c_void, flags: int32) -> tresult {
    (*(this as *const Vst3HostComponentHandler))
        .restart_flags
        .store(flags, Ordering::Release);
    kResultOk
}

static HOST_COMPONENT_HANDLER_VTBL: IComponentHandlerVtbl = IComponentHandlerVtbl {
    unknown: IUnknownVtbl {
        query_interface: host_component_handler_query,
        add_ref: host_interface_add_ref,
        release: host_interface_release,
    },
    begin_edit: host_begin_edit,
    perform_edit: host_perform_edit,
    end_edit: host_end_edit,
    restart_component: host_restart_component,
};

unsafe extern "system" fn host_resize_view(
    this: *mut c_void,
    view: *mut c_void,
    size: *mut ViewRect,
) -> tresult {
    if this.is_null() || view.is_null() || size.is_null() {
        return kInvalidArgument;
    }
    let frame = &*(this as *const Vst3HostPlugFrame);
    if !frame.attached.load(Ordering::Acquire) {
        return kResultFalse;
    }
    let size = &mut *size;
    let width = size.width();
    let height = size.height();
    if width <= 0 || height <= 0 {
        return kInvalidArgument;
    }
    frame.width.store(width as u32, Ordering::Release);
    frame.height.store(height as u32, Ordering::Release);
    frame.resize_count.fetch_add(1, Ordering::AcqRel);

    let window = frame.window.load(Ordering::Acquire);
    if !window.is_null() {
        (&*window).set_content_view_size(width as f64, height as f64);
    }
    let view_vtbl = *(view as *const *const IPlugViewVtbl);
    if view_vtbl.is_null() {
        return kInvalidArgument;
    }
    ((*view_vtbl).on_size)(view, size)
}

static HOST_PLUG_FRAME_VTBL: IPlugFrameVtbl = IPlugFrameVtbl {
    unknown: IUnknownVtbl {
        query_interface: host_plug_frame_query,
        add_ref: host_interface_add_ref,
        release: host_interface_release,
    },
    resize_view: host_resize_view,
};

fn native_vst3_platform_type() -> Option<&'static [u8]> {
    #[cfg(target_os = "macos")]
    {
        Some(kPlatformTypeNSView)
    }
    #[cfg(target_os = "windows")]
    {
        Some(kPlatformTypeHWND)
    }
    #[cfg(target_os = "linux")]
    {
        Some(kPlatformTypeX11EmbedWindowID)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

fn as_fid_string<T>(bytes: &[T]) -> FIDString {
    bytes.as_ptr().cast::<vst3_sys::char8>()
}

unsafe fn release_plugin_view(view: *mut c_void) {
    let unknown = &**(view as *const *const IUnknownVtbl);
    (unknown.release)(view);
}

impl Vst3HostPlugin {
    pub fn load(bundle_path: &str, class_index: u32) -> Result<Self, String> {
        let dylib_path = super::scanner::find_vst3_module(std::path::Path::new(bundle_path))
            .ok_or_else(|| format!("No VST3 module found in {bundle_path}"))?;

        unsafe {
            let lib = super::load_plugin_library(&dylib_path)
                .map_err(|e| format!("Failed to load {}: {}", dylib_path.display(), e))?;

            let get_factory: libloading::Symbol<unsafe extern "C" fn() -> *mut c_void> = lib
                .get(b"GetPluginFactory")
                .map_err(|e| format!("No GetPluginFactory: {}", e))?;

            let factory_ptr = get_factory();
            if factory_ptr.is_null() {
                return Err("GetPluginFactory returned null".into());
            }

            let factory = &**(factory_ptr as *const *const IPluginFactoryVtbl);

            let count = (factory.count_classes)(factory_ptr);
            if class_index as i32 >= count {
                return Err(format!(
                    "Class index {} out of range ({})",
                    class_index, count
                ));
            }

            let mut class_info = PClassInfoData {
                cid: [0; 16],
                cardinality: 0,
                category: [0; 32],
                name: [0; 64],
            };
            if (factory.get_class_info)(factory_ptr, class_index as i32, &mut class_info) != 0 {
                return Err("Failed to get class info".into());
            }

            // Create instance - try with IComponent IID first
            let mut component_ptr: *mut c_void = ptr::null_mut();
            let result = (factory.create_instance)(
                factory_ptr,
                as_fid_string(&class_info.cid),
                as_fid_string(&vst3_sys::vst::iid::IComponent),
                &mut component_ptr as *mut *mut c_void,
            );
            if result != 0 || component_ptr.is_null() {
                return Err(format!("create_instance failed: {}", result));
            }

            // Get vtable by reading the vtable pointer
            let component_vtbl = *(component_ptr as *const *const IComponentVtbl);

            // Try to QueryInterface for IAudioProcessor
            let unknown = &**(component_ptr as *const *const IUnknownVtbl);
            let iid_processor = vst3_sys::vst::iid::IAudioProcessor;
            let mut processor_ptr: *mut c_void = ptr::null_mut();
            let qi_result = (unknown.query_interface)(
                component_ptr,
                &iid_processor as *const TUID,
                &mut processor_ptr as *mut *mut c_void,
            );

            if qi_result != kResultOk || processor_ptr.is_null() {
                ((*component_vtbl).base.unknown.release)(component_ptr);
                return Err(format!(
                    "IComponent does not implement IAudioProcessor: {}",
                    qi_result
                ));
            }
            let processor = processor_ptr;
            let processor_vtbl = *(processor_ptr as *const *const IAudioProcessorVtbl);

            // Initialize
            let init_result = ((*component_vtbl).base.initialize)(component_ptr, ptr::null_mut());
            if init_result != 0 {
                return Err(format!("IComponent::initialize failed: {}", init_result));
            }

            let mut controller_cid = [0; 16];
            let controller_id_result =
                ((*component_vtbl).get_controller_class_id)(component_ptr, &mut controller_cid);
            if controller_id_result != kResultOk {
                return Err(format!(
                    "IComponent::getControllerClassId failed: {}",
                    controller_id_result
                ));
            }

            let mut controller: *mut c_void = ptr::null_mut();
            let controller_result = (factory.create_instance)(
                factory_ptr,
                as_fid_string(&controller_cid),
                as_fid_string(&vst3_sys::vst::iid::IEditController),
                &mut controller,
            );
            if controller_result != kResultOk || controller.is_null() {
                return Err(format!(
                    "Failed to create VST3 edit controller: {}",
                    controller_result
                ));
            }
            let controller_vtbl = *(controller as *const *const IEditControllerVtbl);
            let controller_init = ((*controller_vtbl).base.initialize)(controller, ptr::null_mut());
            if controller_init != kResultOk {
                return Err(format!(
                    "IEditController::initialize failed: {}",
                    controller_init
                ));
            }

            let component_connection = query_connection_point(component_ptr);
            let controller_connection = query_connection_point(controller);
            if !component_connection.is_null() && !controller_connection.is_null() {
                let component_connection_vtbl =
                    *(component_connection as *const *const IConnectionPointVtbl);
                let controller_connection_vtbl =
                    *(controller_connection as *const *const IConnectionPointVtbl);
                let component_connect = ((*component_connection_vtbl).connect)(
                    component_connection,
                    controller_connection,
                );
                let controller_connect = ((*controller_connection_vtbl).connect)(
                    controller_connection,
                    component_connection,
                );
                if component_connect != kResultOk || controller_connect != kResultOk {
                    return Err(format!(
                        "Failed to connect VST3 processor/controller: component={}, controller={}",
                        component_connect, controller_connect
                    ));
                }
            }

            let input_bus_channels = query_bus_channels(
                component_ptr,
                component_vtbl,
                MediaTypes::kAudio,
                BusDirections::kInput,
            )?;
            let output_bus_channels = query_bus_channels(
                component_ptr,
                component_vtbl,
                MediaTypes::kAudio,
                BusDirections::kOutput,
            )?;
            negotiate_bus_arrangements(
                processor,
                processor_vtbl,
                &input_bus_channels,
                &output_bus_channels,
            )?;
            let event_input_buses = ((*component_vtbl).get_bus_count)(
                component_ptr,
                MediaTypes::kEvent,
                BusDirections::kInput,
            )
            .max(0) as usize;
            let input_channels = input_bus_channels.iter().sum::<usize>();
            let output_channels = output_bus_channels.iter().sum::<usize>();

            let info = PluginInfo {
                name: char8_to_string(&class_info.name),
                vendor: String::new(),
                version: String::new(),
                id: format!("{:?}", class_info.cid),
                path: bundle_path.to_string(),
                format: PluginFormat::VST3,
                class_index,
                input_channels: input_channels as u32,
                output_channels: output_channels as u32,
                is_synth: input_channels == 0 && output_channels > 0 && event_input_buses > 0,
            };

            let mut component_handler = Box::new(Vst3HostComponentHandler::new());
            let handler_result = ((*controller_vtbl).set_component_handler)(
                controller,
                (&mut *component_handler as *mut Vst3HostComponentHandler).cast(),
            );
            if handler_result != kResultOk {
                return Err(format!(
                    "IEditController::setComponentHandler failed: {handler_result}"
                ));
            }
            let plug_frame = Box::new(Vst3HostPlugFrame::new());

            let factory_unknown =
                &**(factory_ptr as *const *const vst3_sys::base::types::IUnknownVtbl);
            (factory_unknown.release)(factory_ptr);

            Ok(Self {
                info,
                _lib: lib,
                component: component_ptr,
                processor,
                controller,
                component_vtbl,
                processor_vtbl,
                controller_vtbl,
                component_connection,
                controller_connection,
                input_buffers: vec![vec![0.0; 4096]; input_channels],
                output_buffers: vec![vec![0.0; 4096]; output_channels],
                input_channel_ptrs: vec![ptr::null_mut(); input_channels],
                output_channel_ptrs: vec![ptr::null_mut(); output_channels],
                input_bus_channels,
                output_bus_channels,
                sample_rate: 44100.0,
                max_frames: 4096,
                _component_handler: component_handler,
                plug_frame,
                view: ptr::null_mut(),
            })
        }
    }
}

impl HostPlugin for Vst3HostPlugin {
    fn info(&self) -> &PluginInfo {
        &self.info
    }

    fn initialize(&mut self, sample_rate: f64, max_frames: u32) -> Result<(), String> {
        self.sample_rate = sample_rate;
        self.max_frames = max_frames;
        let input_channels = self.info.input_channels as usize;
        let output_channels = self.info.output_channels as usize;
        self.input_buffers = vec![vec![0.0; max_frames as usize]; input_channels];
        self.output_buffers = vec![vec![0.0; max_frames as usize]; output_channels];
        self.input_channel_ptrs = vec![ptr::null_mut(); input_channels];
        self.output_channel_ptrs = vec![ptr::null_mut(); output_channels];

        unsafe {
            let mut setup = ProcessSetup {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                max_samples_per_block: max_frames as i32,
                sample_rate,
            };
            let result = ((*self.processor_vtbl).setup_processing)(self.processor, &mut setup);
            if result != 0 {
                return Err(format!("setup_processing failed: {}", result));
            }

            // Activate buses
            let in_count = ((*self.component_vtbl).get_bus_count)(
                self.component,
                MediaTypes::kAudio,
                BusDirections::kInput,
            );
            for i in 0..in_count {
                ((*self.component_vtbl).activate_bus)(
                    self.component,
                    MediaTypes::kAudio,
                    BusDirections::kInput,
                    i,
                    1,
                );
            }
            let out_count = ((*self.component_vtbl).get_bus_count)(
                self.component,
                MediaTypes::kAudio,
                BusDirections::kOutput,
            );
            for i in 0..out_count {
                ((*self.component_vtbl).activate_bus)(
                    self.component,
                    MediaTypes::kAudio,
                    BusDirections::kOutput,
                    i,
                    1,
                );
            }

            let event_in_count = ((*self.component_vtbl).get_bus_count)(
                self.component,
                MediaTypes::kEvent,
                BusDirections::kInput,
            );
            for i in 0..event_in_count {
                ((*self.component_vtbl).activate_bus)(
                    self.component,
                    MediaTypes::kEvent,
                    BusDirections::kInput,
                    i,
                    1,
                );
            }

            let result = ((*self.component_vtbl).set_active)(self.component, 1);
            if result != 0 {
                return Err(format!("set_active failed: {}", result));
            }

            let result = ((*self.processor_vtbl).set_processing)(self.processor, 1);
            if result != 0 {
                return Err(format!("set_processing failed: {}", result));
            }
        }
        Ok(())
    }

    fn param_count(&self) -> u32 {
        if self.controller.is_null() {
            return 0;
        }
        unsafe { ((*self.controller_vtbl).get_parameter_count)(self.controller).max(0) as u32 }
    }

    fn param_info(&self, index: u32) -> Option<ParamInfo> {
        if self.controller.is_null() {
            return None;
        }
        let mut raw = ParameterInfo {
            id: 0,
            title: [0; 128],
            short_title: [0; 128],
            units: [0; 128],
            step_count: 0,
            default_normalized_value: 0.0,
            unit_id: 0,
            flags: 0,
        };
        let result = unsafe {
            ((*self.controller_vtbl).get_parameter_info)(self.controller, index as i32, &mut raw)
        };
        if result != kResultOk {
            return None;
        }
        Some(ParamInfo {
            id: raw.id,
            name: char16_to_string(&raw.title),
            min: 0.0,
            max: 1.0,
            default: raw.default_normalized_value,
            is_stepped: raw.step_count > 0,
            can_automate: (raw.flags & ParameterFlags::kCanAutomate) != 0,
        })
    }

    fn param_get(&self, id: u32) -> Option<f64> {
        if self.controller.is_null() {
            return None;
        }
        Some(unsafe { ((*self.controller_vtbl).get_param_normalized)(self.controller, id) })
    }

    fn param_set(&mut self, id: u32, value: f64) -> Result<(), String> {
        if self.controller.is_null() {
            return Err("VST3 plugin has no edit controller".into());
        }
        let result =
            unsafe { ((*self.controller_vtbl).set_param_normalized)(self.controller, id, value) };
        if result == kResultOk {
            Ok(())
        } else {
            Err(format!("setParamNormalized failed: {}", result))
        }
    }

    fn process(&mut self, input: &[f32], output: &mut [f32]) -> Result<(), String> {
        self.process_with_events(input, output, &[])
    }

    fn process_with_events(
        &mut self,
        input: &[f32],
        output: &mut [f32],
        events: &[HostEvent],
    ) -> Result<(), String> {
        let input_channels = self.info.input_channels as usize;
        let output_channels = self.info.output_channels as usize;
        let frames = process_frame_count(
            input.len(),
            input_channels,
            output.len(),
            output_channels,
            self.max_frames as usize,
        )?;
        validate_host_events(events, frames)?;

        for channel in 0..input_channels {
            for frame in 0..frames {
                self.input_buffers[channel][frame] = input[frame * input_channels + channel];
            }
            self.input_channel_ptrs[channel] = self.input_buffers[channel].as_mut_ptr();
        }
        for channel in 0..output_channels {
            self.output_buffers[channel][..frames].fill(0.0);
            self.output_channel_ptrs[channel] = self.output_buffers[channel].as_mut_ptr();
        }

        let mut input_channel_offset = 0usize;
        let mut input_buses: Vec<AudioBusBuffers> = self
            .input_bus_channels
            .iter()
            .map(|&channel_count| {
                let buffers = if channel_count == 0 {
                    ptr::null_mut()
                } else {
                    unsafe {
                        self.input_channel_ptrs
                            .as_mut_ptr()
                            .add(input_channel_offset) as *mut *mut c_void
                    }
                };
                input_channel_offset += channel_count;
                AudioBusBuffers {
                    num_channels: channel_count as i32,
                    silence_flags: 0,
                    buffers,
                }
            })
            .collect();

        let mut output_channel_offset = 0usize;
        let mut output_buses: Vec<AudioBusBuffers> = self
            .output_bus_channels
            .iter()
            .map(|&channel_count| {
                let buffers = if channel_count == 0 {
                    ptr::null_mut()
                } else {
                    unsafe {
                        self.output_channel_ptrs
                            .as_mut_ptr()
                            .add(output_channel_offset) as *mut *mut c_void
                    }
                };
                output_channel_offset += channel_count;
                AudioBusBuffers {
                    num_channels: channel_count as i32,
                    silence_flags: 0,
                    buffers,
                }
            })
            .collect();

        let mut event_list = Vst3EventList::new(events);
        let mut parameter_changes = Vst3ParameterChanges::new(events);
        unsafe {
            let mut data = ProcessData {
                process_mode: ProcessModes::kRealtime,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                num_samples: frames as i32,
                num_inputs: input_buses.len() as i32,
                num_outputs: output_buses.len() as i32,
                inputs: if input_buses.is_empty() {
                    ptr::null_mut()
                } else {
                    input_buses.as_mut_ptr()
                },
                outputs: if output_buses.is_empty() {
                    ptr::null_mut()
                } else {
                    output_buses.as_mut_ptr()
                },
                input_parameter_changes: if parameter_changes.is_empty() {
                    ptr::null_mut()
                } else {
                    parameter_changes.as_raw()
                },
                output_parameter_changes: ptr::null_mut(),
                input_events: event_list.as_raw(),
                output_events: ptr::null_mut(),
                process_context: ptr::null_mut(),
            };

            let result = ((*self.processor_vtbl).process)(self.processor, &mut data);
            if result != 0 {
                return Err(format!("process failed: {}", result));
            }
        }

        for frame in 0..frames {
            for channel in 0..output_channels {
                output[frame * output_channels + channel] = self.output_buffers[channel][frame];
            }
        }

        Ok(())
    }

    fn reset(&mut self) -> Result<(), String> {
        unsafe {
            if self.processor.is_null() {
                return Err("VST3 processor is not available".into());
            }
            let stop_result = ((*self.processor_vtbl).set_processing)(self.processor, 0);
            if stop_result != kResultOk {
                return Err(format!("set_processing(false) failed: {stop_result}"));
            }
            let start_result = ((*self.processor_vtbl).set_processing)(self.processor, 1);
            if start_result != kResultOk {
                return Err(format!("set_processing(true) failed: {start_result}"));
            }
        }
        Ok(())
    }

    fn save_state(&mut self) -> Result<Vec<u8>, String> {
        let mut stream = MemoryStream::new();
        let result = unsafe { ((*self.component_vtbl).get_state)(self.component, stream.as_raw()) };
        if result != kResultOk {
            return Err(format!("IComponent::getState failed: {}", result));
        }
        Ok(stream.into_bytes())
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), String> {
        unsafe {
            let stop_result = ((*self.processor_vtbl).set_processing)(self.processor, 0);
            if stop_result != kResultOk {
                return Err(format!("set_processing(false) failed: {}", stop_result));
            }
            let deactivate_result = ((*self.component_vtbl).set_active)(self.component, 0);
            if deactivate_result != kResultOk {
                let restart_result = ((*self.processor_vtbl).set_processing)(self.processor, 1);
                return Err(format!(
                    "set_active(false) failed: {}; processing restart={}",
                    deactivate_result, restart_result
                ));
            }

            let mut component_stream = MemoryStream::from_bytes(data.to_vec());
            let component_result =
                ((*self.component_vtbl).set_state)(self.component, component_stream.as_raw());

            let mut controller_stream = MemoryStream::from_bytes(data.to_vec());
            let controller_result = ((*self.controller_vtbl).set_component_state)(
                self.controller,
                controller_stream.as_raw(),
            );

            let activate_result = ((*self.component_vtbl).set_active)(self.component, 1);
            let start_result = if activate_result == kResultOk {
                ((*self.processor_vtbl).set_processing)(self.processor, 1)
            } else {
                kResultFalse
            };

            if component_result != kResultOk {
                return Err(format!("IComponent::setState failed: {}", component_result));
            }
            if controller_result != kResultOk {
                return Err(format!(
                    "IEditController::setComponentState failed: {}",
                    controller_result
                ));
            }
            if activate_result != kResultOk || start_result != kResultOk {
                return Err(format!(
                    "Failed to resume after state load: active={}, processing={}",
                    activate_result, start_result
                ));
            }
            Ok(())
        }
    }

    fn shutdown(&mut self) {
        self.close_gui();
        unsafe {
            if !self.processor.is_null() {
                ((*self.processor_vtbl).set_processing)(self.processor, 0);
            }
            if !self.component.is_null() {
                ((*self.component_vtbl).set_active)(self.component, 0);
            }

            if !self.component_connection.is_null() && !self.controller_connection.is_null() {
                let component_vtbl =
                    *(self.component_connection as *const *const IConnectionPointVtbl);
                let controller_vtbl =
                    *(self.controller_connection as *const *const IConnectionPointVtbl);
                ((*component_vtbl).disconnect)(
                    self.component_connection,
                    self.controller_connection,
                );
                ((*controller_vtbl).disconnect)(
                    self.controller_connection,
                    self.component_connection,
                );
                ((*component_vtbl).unknown.release)(self.component_connection);
                ((*controller_vtbl).unknown.release)(self.controller_connection);
                self.component_connection = ptr::null_mut();
                self.controller_connection = ptr::null_mut();
            }

            if !self.controller.is_null() {
                let _ = ((*self.controller_vtbl).set_component_handler)(
                    self.controller,
                    ptr::null_mut(),
                );
                ((*self.controller_vtbl).base.terminate)(self.controller);
                release_unknown(self.controller);
                self.controller = ptr::null_mut();
            }
            if !self.processor.is_null() && self.processor != self.component {
                release_unknown(self.processor);
                self.processor = ptr::null_mut();
            }
            if !self.component.is_null() {
                ((*self.component_vtbl).base.terminate)(self.component);
                release_unknown(self.component);
                self.component = ptr::null_mut();
            }
        }
    }

    fn open_gui(&mut self, window: &PluginGuiWindow) -> Result<(), String> {
        unsafe {
            if self.controller.is_null() {
                return Err("VST3 plugin has no edit controller".into());
            }

            // Call createView
            let create_view = (*self.controller_vtbl).create_view;
            let view_name = as_fid_string(b"editor\0");
            let view_ptr = create_view(self.controller, view_name);
            if view_ptr.is_null() {
                return Err("IEditController::createView() returned null".into());
            }

            // Get view vtable
            let view_vtbl = *(view_ptr as *const *const IPlugViewVtbl);

            // Get preferred size
            let mut rect = ViewRect::new(0, 0, 400, 300);
            let get_size = (*view_vtbl).get_size;
            let _ = get_size(view_ptr, &mut rect as *mut ViewRect);
            let w = rect.width() as f64;
            let h = rect.height() as f64;

            // Resize window
            window.set_content_view_size(w, h);

            let Some(platform_type) = native_vst3_platform_type() else {
                release_plugin_view(view_ptr);
                return Err("No native VST3 GUI platform type for this platform".into());
            };
            let platform_type_ptr = as_fid_string(platform_type);
            let supported = ((*view_vtbl).is_platform_type_supported)(view_ptr, platform_type_ptr);
            if supported != kResultTrue {
                release_plugin_view(view_ptr);
                return Err(format!(
                    "IPlugView does not support platform type '{}' ({supported})",
                    String::from_utf8_lossy(&platform_type[..platform_type.len() - 1])
                ));
            }

            let set_frame_result = ((*view_vtbl).set_frame)(
                view_ptr,
                (&mut *self.plug_frame as *mut Vst3HostPlugFrame).cast(),
            );
            if set_frame_result != kResultOk {
                release_plugin_view(view_ptr);
                return Err(format!("IPlugView::setFrame() failed: {set_frame_result}"));
            }
            self.plug_frame.window.store(
                (window as *const PluginGuiWindow).cast_mut(),
                Ordering::Release,
            );
            self.plug_frame.attached.store(true, Ordering::Release);

            let attached = (*view_vtbl).attached;
            let result = attached(view_ptr, window.content_view(), platform_type_ptr);
            if result != 0 {
                self.plug_frame.attached.store(false, Ordering::Release);
                self.plug_frame
                    .window
                    .store(ptr::null_mut(), Ordering::Release);
                let _ = ((*view_vtbl).set_frame)(view_ptr, ptr::null_mut());
                release_plugin_view(view_ptr);
                return Err(format!("IPlugView::attached() failed: {}", result));
            }

            self.view = view_ptr;
            Ok(())
        }
    }

    fn resize_gui(&mut self, width: u32, height: u32) -> Result<(u32, u32), String> {
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err(format!("invalid VST3 GUI size {width}x{height}"));
        }

        unsafe {
            if self.view.is_null() {
                return Err("VST3 GUI is not attached".into());
            }
            let view_vtbl = *(self.view as *const *const IPlugViewVtbl);
            if view_vtbl.is_null() {
                return Err("VST3 IPlugView vtable is null".into());
            }
            if ((*view_vtbl).can_resize)(self.view) != kResultTrue {
                return Err("VST3 plugin reports a fixed-size GUI".into());
            }

            let mut rect = ViewRect::new(0, 0, width as i32, height as i32);
            let constrained = ((*view_vtbl).check_size_constraint)(self.view, &mut rect);
            if constrained != kResultOk {
                return Err(format!(
                    "IPlugView::checkSizeConstraint() failed: {constrained}"
                ));
            }
            if rect.width() <= 0 || rect.height() <= 0 {
                return Err("VST3 plugin constrained the GUI to an empty size".into());
            }
            let adjusted_width = rect.width() as u32;
            let adjusted_height = rect.height() as u32;
            let window = self.plug_frame.window.load(Ordering::Acquire);
            if window.is_null() {
                return Err("VST3 GUI host window is unavailable".into());
            }
            // Grow or shrink the host container before notifying the view.
            // AppKit child autoresizing otherwise applies the same delta after
            // the plugin has already resized its embedded native view.
            (&*window).set_content_view_size(adjusted_width.into(), adjusted_height.into());
            let resized = ((*view_vtbl).on_size)(self.view, &mut rect);
            if resized != kResultOk {
                return Err(format!("IPlugView::onSize() failed: {resized}"));
            }
            Ok((adjusted_width, adjusted_height))
        }
    }

    fn gui_gesture_evidence(&self) -> Option<GuiGestureEvidence> {
        Some(GuiGestureEvidence {
            begin_count: self._component_handler.begin_count.load(Ordering::Acquire),
            value_count: self
                ._component_handler
                .perform_count
                .load(Ordering::Acquire),
            end_count: self._component_handler.end_count.load(Ordering::Acquire),
            last_param_id: self
                ._component_handler
                .last_param_id
                .load(Ordering::Acquire),
            last_value: f64::from_bits(
                self._component_handler
                    .last_param_value
                    .load(Ordering::Acquire),
            ),
            completed_count: self
                ._component_handler
                .completed_gesture_count
                .load(Ordering::Acquire),
            last_completed_param_id: self
                ._component_handler
                .completed_gesture_param_id
                .load(Ordering::Acquire),
            last_completed_value: f64::from_bits(
                self._component_handler
                    .completed_gesture_value
                    .load(Ordering::Acquire),
            ),
        })
    }

    fn close_gui(&mut self) {
        unsafe {
            if !self.view.is_null() {
                let view_vtbl = *(self.view as *const *const IPlugViewVtbl);
                let removed = (*view_vtbl).removed;
                removed(self.view);
                self.plug_frame.attached.store(false, Ordering::Release);
                self.plug_frame
                    .window
                    .store(ptr::null_mut(), Ordering::Release);
                let _ = ((*view_vtbl).set_frame)(self.view, ptr::null_mut());

                // Release the view via IUnknown::release
                let unknown = &**(self.view as *const *const IUnknownVtbl);
                (unknown.release)(self.view);

                self.view = ptr::null_mut();
            }
        }
    }
}

unsafe fn query_bus_channels(
    component: *mut c_void,
    vtbl: *const IComponentVtbl,
    media_type: MediaType,
    direction: BusDirection,
) -> Result<Vec<usize>, String> {
    let count = ((*vtbl).get_bus_count)(component, media_type, direction).max(0);
    let mut channels = Vec::with_capacity(count as usize);
    for index in 0..count {
        let mut info = BusInfo::default();
        let result = ((*vtbl).get_bus_info)(component, media_type, direction, index, &mut info);
        if result != kResultOk {
            return Err(format!(
                "get_bus_info failed for media={}, direction={}, index={}: {}",
                media_type, direction, index, result
            ));
        }
        channels.push(info.channel_count.max(0) as usize);
    }
    Ok(channels)
}

fn validate_bus_arrangements(
    direction: &str,
    channels: &[usize],
    arrangements: &[SpeakerArrangement],
) -> Result<(), String> {
    if channels.len() != arrangements.len() {
        return Err(format!(
            "VST3 {direction} arrangement count mismatch: buses={}, arrangements={}",
            channels.len(),
            arrangements.len()
        ));
    }
    for (index, (&channels, &arrangement)) in channels.iter().zip(arrangements).enumerate() {
        if arrangement.count_ones() as usize != channels {
            return Err(format!(
                "VST3 {direction} bus {index} arrangement has {} speakers but advertises {channels} channels",
                arrangement.count_ones()
            ));
        }
    }
    Ok(())
}

unsafe fn query_bus_arrangements(
    processor: *mut c_void,
    vtbl: *const IAudioProcessorVtbl,
    direction: BusDirection,
    count: usize,
) -> Result<Vec<SpeakerArrangement>, String> {
    let mut arrangements = Vec::with_capacity(count);
    for index in 0..count {
        let mut arrangement = SpeakerArr::kEmpty;
        let result =
            ((*vtbl).get_bus_arrangement)(processor, direction, index as int32, &mut arrangement);
        if result != kResultOk {
            return Err(format!(
                "IAudioProcessor::getBusArrangement failed for direction={direction}, index={index}: {result}"
            ));
        }
        arrangements.push(arrangement);
    }
    Ok(arrangements)
}

unsafe fn negotiate_bus_arrangements(
    processor: *mut c_void,
    vtbl: *const IAudioProcessorVtbl,
    input_channels: &[usize],
    output_channels: &[usize],
) -> Result<(), String> {
    let mut inputs =
        query_bus_arrangements(processor, vtbl, BusDirections::kInput, input_channels.len())?;
    let mut outputs = query_bus_arrangements(
        processor,
        vtbl,
        BusDirections::kOutput,
        output_channels.len(),
    )?;
    validate_bus_arrangements("input", input_channels, &inputs)?;
    validate_bus_arrangements("output", output_channels, &outputs)?;

    let input_ptr = if inputs.is_empty() {
        ptr::null_mut()
    } else {
        inputs.as_mut_ptr()
    };
    let output_ptr = if outputs.is_empty() {
        ptr::null_mut()
    } else {
        outputs.as_mut_ptr()
    };
    let result = ((*vtbl).set_bus_arrangements)(
        processor,
        input_ptr,
        inputs.len() as int32,
        output_ptr,
        outputs.len() as int32,
    );
    if result == kResultOk {
        Ok(())
    } else {
        Err(format!(
            "IAudioProcessor::setBusArrangements rejected declared layouts: {result}"
        ))
    }
}

#[repr(C)]
struct Vst3EventList {
    vtbl: *const IEventListVtbl,
    ref_count: AtomicU32,
    events: Vec<Event>,
}

impl Vst3EventList {
    fn new(events: &[HostEvent]) -> Self {
        let events = events
            .iter()
            .copied()
            .filter_map(|event| match event {
                HostEvent::NoteOn {
                    sample_offset,
                    channel,
                    pitch,
                    velocity,
                } => Some(Event {
                    bus_index: 0,
                    sample_offset: sample_offset as i32,
                    ppq_position: 0.0,
                    flags: EventFlags::kIsLive,
                    type_: EventTypes::kNoteOnEvent,
                    event: EventData {
                        note_on: NoteOnEvent {
                            channel: channel as i16,
                            pitch: pitch as i16,
                            tuning: 0.0,
                            velocity,
                            length: 0,
                            note_id: -1,
                        },
                    },
                }),
                HostEvent::NoteOff {
                    sample_offset,
                    channel,
                    pitch,
                    velocity,
                } => Some(Event {
                    bus_index: 0,
                    sample_offset: sample_offset as i32,
                    ppq_position: 0.0,
                    flags: EventFlags::kIsLive,
                    type_: EventTypes::kNoteOffEvent,
                    event: EventData {
                        note_off: NoteOffEvent {
                            channel: channel as i16,
                            pitch: pitch as i16,
                            velocity,
                            note_id: -1,
                            tuning: 0.0,
                        },
                    },
                }),
                HostEvent::ParamValue { .. } => None,
            })
            .collect();
        Self {
            vtbl: &VST3_EVENT_LIST_VTBL,
            ref_count: AtomicU32::new(1),
            events,
        }
    }

    fn as_raw(&mut self) -> *mut c_void {
        self as *mut Self as *mut c_void
    }
}

static VST3_EVENT_LIST_VTBL: IEventListVtbl = IEventListVtbl {
    unknown: IUnknownVtbl {
        query_interface: vst3_event_query_interface,
        add_ref: vst3_event_add_ref,
        release: vst3_event_release,
    },
    get_event_count: vst3_event_count,
    get_event: vst3_event_get,
    add_event: vst3_event_add,
};

unsafe extern "system" fn vst3_event_query_interface(
    this: *mut c_void,
    iid: *const TUID,
    object: *mut *mut c_void,
) -> tresult {
    if this.is_null() || iid.is_null() || object.is_null() {
        return kInvalidArgument;
    }
    *object = ptr::null_mut();
    if *iid == vst3_sys::base::iid::IUnknown || *iid == vst3_sys::vst::iid::IEventList {
        *object = this;
        vst3_event_add_ref(this);
        kResultOk
    } else {
        kNoInterface
    }
}

unsafe extern "system" fn vst3_event_add_ref(this: *mut c_void) -> u32 {
    let list = &*(this as *const Vst3EventList);
    list.ref_count.fetch_add(1, Ordering::Relaxed) + 1
}

unsafe extern "system" fn vst3_event_release(this: *mut c_void) -> u32 {
    let list = &*(this as *const Vst3EventList);
    let previous = list
        .ref_count
        .fetch_update(Ordering::Release, Ordering::Relaxed, |count| {
            count.checked_sub(1)
        })
        .unwrap_or(0);
    previous.saturating_sub(1)
}

unsafe extern "system" fn vst3_event_count(this: *mut c_void) -> i32 {
    if this.is_null() {
        return 0;
    }
    let list = &*(this as *const Vst3EventList);
    list.events.len() as i32
}

unsafe extern "system" fn vst3_event_get(
    this: *mut c_void,
    index: i32,
    event: *mut Event,
) -> tresult {
    if this.is_null() || event.is_null() || index < 0 {
        return kInvalidArgument;
    }
    let list = &*(this as *const Vst3EventList);
    let Some(source) = list.events.get(index as usize) else {
        return kInvalidArgument;
    };
    *event = *source;
    kResultOk
}

unsafe extern "system" fn vst3_event_add(this: *mut c_void, event: *mut Event) -> tresult {
    if this.is_null() || event.is_null() {
        return kInvalidArgument;
    }
    let list = &mut *(this as *mut Vst3EventList);
    list.events.push(*event);
    kResultOk
}

#[repr(C)]
struct Vst3ParameterChanges {
    vtbl: *const IParameterChangesVtbl,
    ref_count: AtomicU32,
    queues: Vec<Vst3ParamValueQueue>,
}

impl Vst3ParameterChanges {
    fn new(events: &[HostEvent]) -> Self {
        let mut queues: Vec<Vst3ParamValueQueue> = Vec::new();
        for event in events.iter().copied() {
            let HostEvent::ParamValue {
                sample_offset,
                id,
                value,
            } = event
            else {
                continue;
            };

            if let Some(queue) = queues.iter_mut().find(|queue| queue.id == id) {
                queue.points.push((sample_offset as i32, value));
            } else {
                queues.push(Vst3ParamValueQueue {
                    vtbl: &VST3_PARAM_VALUE_QUEUE_VTBL,
                    ref_count: AtomicU32::new(1),
                    id,
                    points: vec![(sample_offset as i32, value)],
                });
            }
        }

        Self {
            vtbl: &VST3_PARAMETER_CHANGES_VTBL,
            ref_count: AtomicU32::new(1),
            queues,
        }
    }

    fn is_empty(&self) -> bool {
        self.queues.is_empty()
    }

    fn as_raw(&mut self) -> *mut c_void {
        self as *mut Self as *mut c_void
    }
}

#[repr(C)]
struct Vst3ParamValueQueue {
    vtbl: *const IParamValueQueueVtbl,
    ref_count: AtomicU32,
    id: ParamID,
    points: Vec<(int32, ParamValue)>,
}

static VST3_PARAMETER_CHANGES_VTBL: IParameterChangesVtbl = IParameterChangesVtbl {
    unknown: IUnknownVtbl {
        query_interface: vst3_parameter_changes_query_interface,
        add_ref: vst3_parameter_changes_add_ref,
        release: vst3_parameter_changes_release,
    },
    get_parameter_count: vst3_parameter_changes_count,
    get_parameter_data: vst3_parameter_changes_data,
    add_parameter_data: vst3_parameter_changes_add_data,
};

static VST3_PARAM_VALUE_QUEUE_VTBL: IParamValueQueueVtbl = IParamValueQueueVtbl {
    unknown: IUnknownVtbl {
        query_interface: vst3_param_queue_query_interface,
        add_ref: vst3_param_queue_add_ref,
        release: vst3_param_queue_release,
    },
    get_parameter_id: vst3_param_queue_id,
    get_point_count: vst3_param_queue_point_count,
    get_point: vst3_param_queue_get_point,
    add_point: vst3_param_queue_add_point,
};

unsafe extern "system" fn vst3_parameter_changes_query_interface(
    this: *mut c_void,
    iid: *const TUID,
    object: *mut *mut c_void,
) -> tresult {
    if this.is_null() || iid.is_null() || object.is_null() {
        return kInvalidArgument;
    }
    *object = ptr::null_mut();
    if *iid == vst3_sys::base::iid::IUnknown || *iid == vst3_sys::vst::iid::IParameterChanges {
        *object = this;
        vst3_parameter_changes_add_ref(this);
        kResultOk
    } else {
        kNoInterface
    }
}

unsafe extern "system" fn vst3_parameter_changes_add_ref(this: *mut c_void) -> uint32 {
    let changes = &*(this as *const Vst3ParameterChanges);
    changes.ref_count.fetch_add(1, Ordering::Relaxed) + 1
}

unsafe extern "system" fn vst3_parameter_changes_release(this: *mut c_void) -> uint32 {
    let changes = &*(this as *const Vst3ParameterChanges);
    let previous = changes
        .ref_count
        .fetch_update(Ordering::Release, Ordering::Relaxed, |count| {
            count.checked_sub(1)
        })
        .unwrap_or(0);
    previous.saturating_sub(1)
}

unsafe extern "system" fn vst3_parameter_changes_count(this: *mut c_void) -> int32 {
    if this.is_null() {
        return 0;
    }
    (*(this as *const Vst3ParameterChanges)).queues.len() as int32
}

unsafe extern "system" fn vst3_parameter_changes_data(
    this: *mut c_void,
    index: int32,
) -> *mut c_void {
    if this.is_null() || index < 0 {
        return ptr::null_mut();
    }
    let changes = &mut *(this as *mut Vst3ParameterChanges);
    changes
        .queues
        .get_mut(index as usize)
        .map(|queue| queue as *mut Vst3ParamValueQueue as *mut c_void)
        .unwrap_or(ptr::null_mut())
}

unsafe extern "system" fn vst3_parameter_changes_add_data(
    _this: *mut c_void,
    _id: *const ParamID,
    _index: *mut int32,
) -> *mut c_void {
    ptr::null_mut()
}

unsafe extern "system" fn vst3_param_queue_query_interface(
    this: *mut c_void,
    iid: *const TUID,
    object: *mut *mut c_void,
) -> tresult {
    if this.is_null() || iid.is_null() || object.is_null() {
        return kInvalidArgument;
    }
    *object = ptr::null_mut();
    if *iid == vst3_sys::base::iid::IUnknown || *iid == vst3_sys::vst::iid::IParamValueQueue {
        *object = this;
        vst3_param_queue_add_ref(this);
        kResultOk
    } else {
        kNoInterface
    }
}

unsafe extern "system" fn vst3_param_queue_add_ref(this: *mut c_void) -> uint32 {
    let queue = &*(this as *const Vst3ParamValueQueue);
    queue.ref_count.fetch_add(1, Ordering::Relaxed) + 1
}

unsafe extern "system" fn vst3_param_queue_release(this: *mut c_void) -> uint32 {
    let queue = &*(this as *const Vst3ParamValueQueue);
    let previous = queue
        .ref_count
        .fetch_update(Ordering::Release, Ordering::Relaxed, |count| {
            count.checked_sub(1)
        })
        .unwrap_or(0);
    previous.saturating_sub(1)
}

unsafe extern "system" fn vst3_param_queue_id(this: *mut c_void) -> ParamID {
    if this.is_null() {
        return u32::MAX;
    }
    (*(this as *const Vst3ParamValueQueue)).id
}

unsafe extern "system" fn vst3_param_queue_point_count(this: *mut c_void) -> int32 {
    if this.is_null() {
        return 0;
    }
    (*(this as *const Vst3ParamValueQueue)).points.len() as int32
}

unsafe extern "system" fn vst3_param_queue_get_point(
    this: *mut c_void,
    index: int32,
    sample_offset: *mut int32,
    value: *mut ParamValue,
) -> tresult {
    if this.is_null() || index < 0 || sample_offset.is_null() || value.is_null() {
        return kInvalidArgument;
    }
    let queue = &*(this as *const Vst3ParamValueQueue);
    let Some((point_offset, point_value)) = queue.points.get(index as usize).copied() else {
        return kInvalidArgument;
    };
    *sample_offset = point_offset;
    *value = point_value;
    kResultOk
}

unsafe extern "system" fn vst3_param_queue_add_point(
    _this: *mut c_void,
    _sample_offset: int32,
    _value: ParamValue,
    _index: *mut int32,
) -> tresult {
    kNotImplemented
}

fn char8_to_string(buf: &[char8]) -> String {
    let bytes: Vec<u8> = buf.iter().map(|&b| b as u8).collect();
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).to_string()
}

fn char16_to_string(buf: &[char16]) -> String {
    let end = buf
        .iter()
        .position(|&value| value == 0)
        .unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

unsafe fn query_connection_point(object: *mut c_void) -> *mut c_void {
    if object.is_null() {
        return ptr::null_mut();
    }
    let unknown = &**(object as *const *const IUnknownVtbl);
    let mut connection = ptr::null_mut();
    if (unknown.query_interface)(
        object,
        &vst3_sys::vst::iid::IConnectionPoint,
        &mut connection,
    ) == kResultOk
    {
        connection
    } else {
        ptr::null_mut()
    }
}

unsafe fn release_unknown(object: *mut c_void) {
    if !object.is_null() {
        let unknown = &**(object as *const *const IUnknownVtbl);
        (unknown.release)(object);
    }
}

#[repr(C)]
struct MemoryStream {
    vtbl: *const IBStreamVtbl,
    ref_count: AtomicU32,
    bytes: Vec<u8>,
    position: usize,
}

impl MemoryStream {
    fn new() -> Self {
        Self::from_bytes(Vec::new())
    }

    fn from_bytes(bytes: Vec<u8>) -> Self {
        Self {
            vtbl: &MEMORY_STREAM_VTBL,
            ref_count: AtomicU32::new(1),
            bytes,
            position: 0,
        }
    }

    fn as_raw(&mut self) -> *mut c_void {
        self as *mut Self as *mut c_void
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

static MEMORY_STREAM_VTBL: IBStreamVtbl = IBStreamVtbl {
    unknown: IUnknownVtbl {
        query_interface: memory_stream_query_interface,
        add_ref: memory_stream_add_ref,
        release: memory_stream_release,
    },
    read: memory_stream_read,
    write: memory_stream_write,
    seek: memory_stream_seek,
    tell: memory_stream_tell,
};

unsafe extern "system" fn memory_stream_query_interface(
    this: *mut c_void,
    iid: *const TUID,
    object: *mut *mut c_void,
) -> tresult {
    if this.is_null() || iid.is_null() || object.is_null() {
        return kInvalidArgument;
    }
    if iid_equal(&*iid, &vst3_sys::base::iid::IUnknown)
        || iid_equal(&*iid, &vst3_sys::base::iid::IBStream)
    {
        *object = this;
        memory_stream_add_ref(this);
        kResultOk
    } else {
        *object = ptr::null_mut();
        kNoInterface
    }
}

unsafe extern "system" fn memory_stream_add_ref(this: *mut c_void) -> uint32 {
    let stream = &*(this as *const MemoryStream);
    let previous = stream
        .ref_count
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
            count.checked_add(1)
        })
        .unwrap_or(uint32::MAX);
    previous.saturating_add(1)
}

unsafe extern "system" fn memory_stream_release(this: *mut c_void) -> uint32 {
    let stream = &*(this as *const MemoryStream);
    let previous = stream
        .ref_count
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
            count.checked_sub(1)
        })
        .unwrap_or(0);
    previous.saturating_sub(1)
}

unsafe extern "system" fn memory_stream_read(
    this: *mut c_void,
    buffer: *mut c_void,
    num_bytes: int32,
    num_bytes_read: *mut int32,
) -> tresult {
    if this.is_null() || num_bytes < 0 || (num_bytes > 0 && buffer.is_null()) {
        return kInvalidArgument;
    }
    let stream = &mut *(this as *mut MemoryStream);
    let available = stream.bytes.len().saturating_sub(stream.position);
    let count = available.min(num_bytes as usize);
    if count > 0 {
        ptr::copy_nonoverlapping(
            stream.bytes.as_ptr().add(stream.position),
            buffer as *mut u8,
            count,
        );
        stream.position += count;
    }
    if !num_bytes_read.is_null() {
        *num_bytes_read = count as int32;
    }
    kResultOk
}

unsafe extern "system" fn memory_stream_write(
    this: *mut c_void,
    buffer: *mut c_void,
    num_bytes: int32,
    num_bytes_written: *mut int32,
) -> tresult {
    if this.is_null() || num_bytes < 0 || (num_bytes > 0 && buffer.is_null()) {
        return kInvalidArgument;
    }
    let stream = &mut *(this as *mut MemoryStream);
    let count = num_bytes as usize;
    let Some(end) = stream.position.checked_add(count) else {
        return kOutOfMemory;
    };
    if end > stream.bytes.len() {
        stream.bytes.resize(end, 0);
    }
    if count > 0 {
        ptr::copy_nonoverlapping(
            buffer as *const u8,
            stream.bytes.as_mut_ptr().add(stream.position),
            count,
        );
        stream.position = end;
    }
    if !num_bytes_written.is_null() {
        *num_bytes_written = num_bytes;
    }
    kResultOk
}

unsafe extern "system" fn memory_stream_seek(
    this: *mut c_void,
    offset: int64,
    mode: int32,
    result: *mut int64,
) -> tresult {
    if this.is_null() {
        return kInvalidArgument;
    }
    let stream = &mut *(this as *mut MemoryStream);
    let base = match mode {
        StreamSeekMode::kIBSeekSet => 0i64,
        StreamSeekMode::kIBSeekCur => stream.position as i64,
        StreamSeekMode::kIBSeekEnd => stream.bytes.len() as i64,
        _ => return kInvalidArgument,
    };
    let Some(position) = base.checked_add(offset) else {
        return kInvalidArgument;
    };
    let Ok(position) = usize::try_from(position) else {
        return kInvalidArgument;
    };
    stream.position = position;
    if !result.is_null() {
        *result = position as int64;
    }
    kResultOk
}

unsafe extern "system" fn memory_stream_tell(this: *mut c_void, position: *mut int64) -> tresult {
    if this.is_null() || position.is_null() {
        return kInvalidArgument;
    }
    *position = (*(this as *mut MemoryStream)).position as int64;
    kResultOk
}

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(C)]
    struct TestPlugView {
        vtbl: *const IPlugViewVtbl,
        width: AtomicU32,
        height: AtomicU32,
    }

    unsafe extern "system" fn test_view_query(
        _this: *mut c_void,
        _iid: *const TUID,
        object: *mut *mut c_void,
    ) -> tresult {
        if !object.is_null() {
            *object = ptr::null_mut();
        }
        kNoInterface
    }

    unsafe extern "system" fn test_view_ref(_this: *mut c_void) -> uint32 {
        1
    }

    unsafe extern "system" fn test_view_platform(
        _this: *mut c_void,
        _platform: FIDString,
    ) -> tresult {
        kResultOk
    }

    unsafe extern "system" fn test_view_attached(
        _this: *mut c_void,
        _parent: *mut c_void,
        _platform: FIDString,
    ) -> tresult {
        kResultOk
    }

    unsafe extern "system" fn test_view_result(_this: *mut c_void) -> tresult {
        kResultOk
    }

    unsafe extern "system" fn test_view_wheel(_this: *mut c_void, _distance: f32) -> tresult {
        kResultFalse
    }

    unsafe extern "system" fn test_view_key(
        _this: *mut c_void,
        _key: char16,
        _key_code: int16,
        _modifiers: int16,
    ) -> tresult {
        kResultFalse
    }

    unsafe extern "system" fn test_view_get_size(
        this: *mut c_void,
        size: *mut ViewRect,
    ) -> tresult {
        if size.is_null() {
            return kInvalidArgument;
        }
        let view = &*(this as *const TestPlugView);
        *size = ViewRect::new(
            0,
            0,
            view.width.load(Ordering::Acquire) as i32,
            view.height.load(Ordering::Acquire) as i32,
        );
        kResultOk
    }

    unsafe extern "system" fn test_view_on_size(this: *mut c_void, size: *mut ViewRect) -> tresult {
        if size.is_null() {
            return kInvalidArgument;
        }
        let view = &*(this as *const TestPlugView);
        view.width.store((*size).width() as u32, Ordering::Release);
        view.height
            .store((*size).height() as u32, Ordering::Release);
        kResultOk
    }

    unsafe extern "system" fn test_view_focus(_this: *mut c_void, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe extern "system" fn test_view_set_frame(
        _this: *mut c_void,
        _frame: *mut c_void,
    ) -> tresult {
        kResultOk
    }

    unsafe extern "system" fn test_view_rect(_this: *mut c_void, _rect: *mut ViewRect) -> tresult {
        kResultOk
    }

    static TEST_PLUG_VIEW_VTBL: IPlugViewVtbl = IPlugViewVtbl {
        unknown: IUnknownVtbl {
            query_interface: test_view_query,
            add_ref: test_view_ref,
            release: test_view_ref,
        },
        is_platform_type_supported: test_view_platform,
        attached: test_view_attached,
        removed: test_view_result,
        on_wheel: test_view_wheel,
        on_key_down: test_view_key,
        on_key_up: test_view_key,
        get_size: test_view_get_size,
        on_size: test_view_on_size,
        on_focus: test_view_focus,
        set_frame: test_view_set_frame,
        can_resize: test_view_result,
        check_size_constraint: test_view_rect,
    };

    #[test]
    fn component_handler_accepts_gui_automation_notifications() {
        let mut handler = Vst3HostComponentHandler::new();
        let raw = (&mut handler as *mut Vst3HostComponentHandler).cast();
        unsafe {
            assert_eq!(host_begin_edit(raw, 42), kResultOk);
            assert_eq!(host_perform_edit(raw, 42, 0.75), kResultOk);
            assert_eq!(host_end_edit(raw, 42), kResultOk);
            assert_eq!(
                host_restart_component(raw, RestartFlags::kParamValuesChanged),
                kResultOk
            );
        }
        assert_eq!(handler.begin_count.load(Ordering::Acquire), 1);
        assert_eq!(handler.perform_count.load(Ordering::Acquire), 1);
        assert_eq!(handler.end_count.load(Ordering::Acquire), 1);
        assert_eq!(handler.last_param_id.load(Ordering::Acquire), 42);
        assert_eq!(
            f64::from_bits(handler.last_param_value.load(Ordering::Acquire)),
            0.75
        );
        assert_eq!(handler.completed_gesture_count.load(Ordering::Acquire), 1);
        assert_eq!(
            handler.completed_gesture_param_id.load(Ordering::Acquire),
            42
        );
        assert_eq!(
            f64::from_bits(handler.completed_gesture_value.load(Ordering::Acquire)),
            0.75
        );
        assert_eq!(
            handler.restart_flags.load(Ordering::Acquire),
            RestartFlags::kParamValuesChanged
        );

        unsafe {
            assert_eq!(host_begin_edit(raw, 43), kResultOk);
            assert_eq!(host_perform_edit(raw, 42, 0.5), kResultOk);
            assert_eq!(host_end_edit(raw, 43), kResultOk);
        }
        assert_eq!(handler.completed_gesture_count.load(Ordering::Acquire), 1);
    }

    #[test]
    fn plug_frame_accepts_resize_after_the_view_is_attached() {
        let mut frame = Vst3HostPlugFrame::new();
        let mut view = TestPlugView {
            vtbl: &TEST_PLUG_VIEW_VTBL,
            width: AtomicU32::new(0),
            height: AtomicU32::new(0),
        };
        let frame_raw = (&mut frame as *mut Vst3HostPlugFrame).cast();
        let view_raw = (&mut view as *mut TestPlugView).cast();
        let mut size = ViewRect::new(0, 0, 960, 540);

        unsafe {
            assert_eq!(
                host_resize_view(frame_raw, view_raw, &mut size),
                kResultFalse
            );
            frame.attached.store(true, Ordering::Release);
            assert_eq!(host_resize_view(frame_raw, view_raw, &mut size), kResultOk);
        }
        assert_eq!(frame.resize_count.load(Ordering::Acquire), 1);
        assert_eq!(frame.width.load(Ordering::Acquire), 960);
        assert_eq!(frame.height.load(Ordering::Acquire), 540);
        assert_eq!(view.width.load(Ordering::Acquire), 960);
        assert_eq!(view.height.load(Ordering::Acquire), 540);
    }

    #[test]
    fn native_gui_platform_type_matches_the_target() {
        let platform_type = native_vst3_platform_type().expect("supported test platform");
        #[cfg(target_os = "macos")]
        assert_eq!(platform_type, kPlatformTypeNSView);
        #[cfg(target_os = "windows")]
        assert_eq!(platform_type, kPlatformTypeHWND);
        #[cfg(target_os = "linux")]
        assert_eq!(platform_type, kPlatformTypeX11EmbedWindowID);
    }

    #[test]
    fn fid_string_conversion_preserves_tuid_and_ascii_storage() {
        let tuid: TUID = [0x5a; 16];
        let editor = b"editor\0";

        assert_eq!(as_fid_string(&tuid).cast::<i8>(), tuid.as_ptr());
        assert_eq!(as_fid_string(editor).cast::<u8>(), editor.as_ptr());
    }

    #[test]
    fn bus_arrangement_validation_matches_declared_channel_counts() {
        assert!(validate_bus_arrangements(
            "input",
            &[6, 8],
            &[SpeakerArr::k51, SpeakerArr::k71Music]
        )
        .is_ok());
        assert!(validate_bus_arrangements("output", &[6], &[SpeakerArr::k60Cine]).is_ok());
        let mismatch = validate_bus_arrangements("output", &[8], &[SpeakerArr::k51])
            .expect_err("speaker count mismatch");
        assert!(mismatch.contains("6 speakers"));
    }

    #[test]
    fn memory_stream_implements_vst3_stream_contract() {
        unsafe {
            let mut stream = MemoryStream::new();
            let raw = stream.as_raw();
            let payload = b"state";
            let mut written = 0;
            assert_eq!(
                memory_stream_write(
                    raw,
                    payload.as_ptr() as *mut c_void,
                    payload.len() as int32,
                    &mut written,
                ),
                kResultOk
            );
            assert_eq!(written, payload.len() as int32);

            let mut position = -1;
            assert_eq!(
                memory_stream_seek(raw, 0, StreamSeekMode::kIBSeekSet, &mut position),
                kResultOk
            );
            assert_eq!(position, 0);

            let mut first = [0u8; 2];
            let mut read = 0;
            assert_eq!(
                memory_stream_read(
                    raw,
                    first.as_mut_ptr() as *mut c_void,
                    first.len() as int32,
                    &mut read,
                ),
                kResultOk
            );
            assert_eq!(&first, b"st");
            assert_eq!(read, 2);

            let mut remainder = [0u8; 8];
            assert_eq!(
                memory_stream_read(
                    raw,
                    remainder.as_mut_ptr() as *mut c_void,
                    remainder.len() as int32,
                    &mut read,
                ),
                kResultOk
            );
            assert_eq!(&remainder[..read as usize], b"ate");
            assert_eq!(memory_stream_tell(raw, &mut position), kResultOk);
            assert_eq!(position, payload.len() as int64);

            let mut queried = ptr::null_mut();
            assert_eq!(
                memory_stream_query_interface(raw, &vst3_sys::base::iid::IBStream, &mut queried,),
                kResultOk
            );
            assert_eq!(queried, raw);
            assert_eq!(memory_stream_release(raw), 1);
            assert_eq!(memory_stream_release(raw), 0);
            assert_eq!(memory_stream_release(raw), 0);
        }
    }

    #[test]
    fn native_note_events_keep_type_and_sample_offset() {
        let mut list = Vst3EventList::new(&[
            HostEvent::NoteOn {
                sample_offset: 17,
                channel: 3,
                pitch: 67,
                velocity: 0.8,
            },
            HostEvent::NoteOff {
                sample_offset: 31,
                channel: 3,
                pitch: 67,
                velocity: 0.2,
            },
        ]);
        let raw = list.as_raw();

        unsafe {
            assert_eq!(vst3_event_count(raw), 2);
            let mut on = Event::default();
            let mut off = Event::default();
            assert_eq!(vst3_event_get(raw, 0, &mut on), kResultOk);
            assert_eq!(vst3_event_get(raw, 1, &mut off), kResultOk);
            assert_eq!(on.type_, EventTypes::kNoteOnEvent);
            assert_eq!(on.sample_offset, 17);
            assert_eq!(on.event.note_on.channel, 3);
            assert_eq!(on.event.note_on.pitch, 67);
            assert_eq!(off.type_, EventTypes::kNoteOffEvent);
            assert_eq!(off.sample_offset, 31);
            assert_eq!(vst3_event_get(raw, 2, &mut off), kInvalidArgument);
        }
    }

    #[test]
    fn native_parameter_changes_group_queues_and_keep_all_ordered_points() {
        let mut changes = Vst3ParameterChanges::new(&[
            HostEvent::ParamValue {
                sample_offset: 17,
                id: 42,
                value: 0.25,
            },
            HostEvent::ParamValue {
                sample_offset: 20,
                id: 7,
                value: 0.4,
            },
            HostEvent::ParamValue {
                sample_offset: 31,
                id: 42,
                value: 0.75,
            },
            HostEvent::ParamValue {
                sample_offset: 31,
                id: 42,
                value: 0.5,
            },
        ]);
        let raw = changes.as_raw();

        unsafe {
            assert_eq!(vst3_parameter_changes_count(raw), 2);
            let gain_queue = vst3_parameter_changes_data(raw, 0);
            let other_queue = vst3_parameter_changes_data(raw, 1);
            assert_eq!(vst3_param_queue_id(gain_queue), 42);
            assert_eq!(vst3_param_queue_id(other_queue), 7);
            assert_eq!(vst3_param_queue_point_count(gain_queue), 3);
            assert_eq!(vst3_param_queue_point_count(other_queue), 1);

            let mut offset = -1;
            let mut value = -1.0;
            assert_eq!(
                vst3_param_queue_get_point(gain_queue, 0, &mut offset, &mut value),
                kResultOk
            );
            assert_eq!((offset, value), (17, 0.25));
            assert_eq!(
                vst3_param_queue_get_point(gain_queue, 1, &mut offset, &mut value),
                kResultOk
            );
            assert_eq!((offset, value), (31, 0.75));
            assert_eq!(
                vst3_param_queue_get_point(gain_queue, 2, &mut offset, &mut value),
                kResultOk
            );
            assert_eq!((offset, value), (31, 0.5));
            assert!(vst3_parameter_changes_data(raw, 2).is_null());
        }
    }
}
