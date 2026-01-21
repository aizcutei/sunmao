//! VST3 Gain Effect Example with Baseview GUI
//!
//! A simple stereo gain effect plugin with a slider GUI.

#![allow(unsafe_op_in_unsafe_fn)]

use baseview::{
    Event, EventStatus, MouseButton, MouseEvent, PhySize, Point, Size, Window, WindowEvent,
    WindowHandler, WindowOpenOptions, WindowScalePolicy,
};
use raw_window_handle::{
    HasWindowHandle, HasDisplayHandle, RawDisplayHandle, RawWindowHandle,
};
#[cfg(target_os = "macos")]
use objc::runtime::{Object, YES};
#[cfg(target_os = "macos")]
use objc::{msg_send, sel, sel_impl};
use softbuffer::{Context, Surface};
use std::ffi::c_void;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use vst3_sys::*;

// =============================================================================
// Plugin UIDs
// =============================================================================

const CID_PROCESSOR: TUID = uid!(0x22223333, 0x44445555, 0x66667777, 0x88881111);
const CID_CONTROLLER: TUID = uid!(0x22223333, 0x44445555, 0x66667777, 0x88882222);

// =============================================================================
// Parameter IDs and GUI Constants
// =============================================================================

const PARAM_GAIN: ParamID = 0;
const GUI_WIDTH: i32 = 400;
const GUI_HEIGHT: i32 = 120;
const SLIDER_MARGIN: f64 = 20.0;
const SLIDER_HEIGHT: f64 = 20.0;
const KNOB_WIDTH: f64 = 12.0;

// =============================================================================
// Shared Gain State (between processor, controller, and GUI)
// =============================================================================

struct SharedGainState {
    gain_bits: AtomicU64,
}

impl SharedGainState {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            gain_bits: AtomicU64::new(1.0_f64.to_bits()),
        })
    }
    
    fn get_gain(&self) -> f64 {
        f64::from_bits(self.gain_bits.load(Ordering::Relaxed))
    }
    
    fn set_gain(&self, value: f64) {
        self.gain_bits.store(value.to_bits(), Ordering::Relaxed);
    }
}

// =============================================================================
// Processor Implementation
// =============================================================================

#[repr(C)]
struct GainProcessorObj {
    vtbl_component: *const ComponentVtbl,
    vtbl_audio: *const AudioProcessorVtbl,
    ref_count: AtomicI32,
    gain_state: Arc<SharedGainState>,
    sample_rate: f64,
}

#[repr(C)]
struct ComponentVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const TUID, *mut *mut c_void) -> tresult,
    add_ref: unsafe extern "system" fn(*mut c_void) -> uint32,
    release: unsafe extern "system" fn(*mut c_void) -> uint32,
    initialize: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
    terminate: unsafe extern "system" fn(*mut c_void) -> tresult,
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

#[repr(C)]
struct AudioProcessorVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const TUID, *mut *mut c_void) -> tresult,
    add_ref: unsafe extern "system" fn(*mut c_void) -> uint32,
    release: unsafe extern "system" fn(*mut c_void) -> uint32,
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
    fn new(gain_state: Arc<SharedGainState>) -> *mut Self {
        Box::into_raw(Box::new(GainProcessorObj {
            vtbl_component: &COMPONENT_VTBL,
            vtbl_audio: &AUDIO_VTBL,
            ref_count: AtomicI32::new(1),
            gain_state,
            sample_rate: 44100.0,
        }))
    }
    
    unsafe fn from_component(this: *mut c_void) -> *mut Self { this as *mut Self }
    unsafe fn from_audio(this: *mut c_void) -> *mut Self {
        (this as *mut u8).sub(std::mem::size_of::<*const c_void>()) as *mut Self
    }
}

// Component interface functions
unsafe extern "system" fn component_query_interface(this: *mut c_void, iid: *const TUID, obj: *mut *mut c_void) -> tresult {
    let iid = &*iid;
    let base = GainProcessorObj::from_component(this);
    if iid_equal(iid, &iid::IUnknown) || iid_equal(iid, &base_iid::IPluginBase) || iid_equal(iid, &vst_iid::IComponent) {
        component_add_ref(this);
        *obj = this;
        return kResultOk;
    }
    if iid_equal(iid, &vst_iid::IAudioProcessor) {
        component_add_ref(this);
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
    if count == 0 { let _ = Box::from_raw(obj); }
    count as uint32
}

unsafe extern "system" fn audio_query_interface(this: *mut c_void, iid: *const TUID, obj: *mut *mut c_void) -> tresult {
    let iid = &*iid;
    let base = GainProcessorObj::from_audio(this);
    if iid_equal(iid, &iid::IUnknown) || iid_equal(iid, &vst_iid::IAudioProcessor) {
        audio_add_ref(this);
        *obj = this;
        return kResultOk;
    }
    if iid_equal(iid, &base_iid::IPluginBase) || iid_equal(iid, &vst_iid::IComponent) {
        audio_add_ref(this);
        *obj = base as *mut c_void;
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
    if count == 0 { let _ = Box::from_raw(obj); }
    count as uint32
}

// IComponent methods
unsafe extern "system" fn processor_initialize(_this: *mut c_void, _context: *mut c_void) -> tresult { kResultOk }
unsafe extern "system" fn processor_terminate(_this: *mut c_void) -> tresult { kResultOk }
unsafe extern "system" fn processor_get_controller_class_id(_this: *mut c_void, class_id: *mut TUID) -> tresult {
    *class_id = CID_CONTROLLER;
    kResultOk
}
unsafe extern "system" fn processor_set_io_mode(_this: *mut c_void, _mode: IoMode) -> tresult { kResultOk }
unsafe extern "system" fn processor_get_bus_count(_this: *mut c_void, media_type: MediaType, dir: BusDirection) -> int32 {
    if media_type == MediaTypes::kAudio && (dir == BusDirections::kInput || dir == BusDirections::kOutput) { 1 } else { 0 }
}

unsafe extern "system" fn processor_get_bus_info(_this: *mut c_void, media_type: MediaType, dir: BusDirection, index: int32, bus: *mut BusInfo) -> tresult {
    if media_type != MediaTypes::kAudio || index != 0 { return kInvalidArgument; }
    let bus = &mut *bus;
    bus.media_type = MediaTypes::kAudio;
    bus.direction = dir;
    bus.channel_count = 2;
    bus.bus_type = BusTypes::kMain;
    bus.flags = BusFlags::kDefaultActive;
    str16cpy_safe(&mut bus.name, if dir == BusDirections::kInput { "Input" } else { "Output" });
    kResultOk
}

unsafe extern "system" fn processor_get_routing_info(_this: *mut c_void, _in_info: *mut RoutingInfo, _out_info: *mut RoutingInfo) -> tresult { kNotImplemented }
unsafe extern "system" fn processor_activate_bus(_this: *mut c_void, _media_type: MediaType, _dir: BusDirection, _index: int32, _state: TBool) -> tresult { kResultOk }
unsafe extern "system" fn processor_set_active(_this: *mut c_void, _state: TBool) -> tresult { kResultOk }
unsafe extern "system" fn processor_set_state(_this: *mut c_void, _state: *mut c_void) -> tresult { kResultOk }
unsafe extern "system" fn processor_get_state(_this: *mut c_void, _state: *mut c_void) -> tresult { kResultOk }

// IAudioProcessor methods
unsafe extern "system" fn audio_set_bus_arrangements(_this: *mut c_void, _inputs: *mut SpeakerArrangement, _num_ins: int32, _outputs: *mut SpeakerArrangement, _num_outs: int32) -> tresult { kResultOk }
unsafe extern "system" fn audio_get_bus_arrangement(_this: *mut c_void, _dir: BusDirection, _index: int32, arr: *mut SpeakerArrangement) -> tresult {
    *arr = SpeakerArr::kStereo;
    kResultOk
}
unsafe extern "system" fn audio_can_process_sample_size(_this: *mut c_void, symbolic_sample_size: int32) -> tresult {
    if symbolic_sample_size == SymbolicSampleSizes::kSample32 { kResultOk } else { kResultFalse }
}
unsafe extern "system" fn audio_get_latency_samples(_this: *mut c_void) -> uint32 { 0 }
unsafe extern "system" fn audio_setup_processing(this: *mut c_void, setup: *mut ProcessSetup) -> tresult {
    let obj = GainProcessorObj::from_audio(this);
    (*obj).sample_rate = (*setup).sample_rate;
    kResultOk
}
unsafe extern "system" fn audio_set_processing(_this: *mut c_void, _state: TBool) -> tresult { kResultOk }

unsafe extern "system" fn audio_process(this: *mut c_void, data: *mut ProcessData) -> tresult {
    let obj = GainProcessorObj::from_audio(this);
    let data = &*data;
    if data.num_samples == 0 { return kResultOk; }
    
    // Read parameter changes
    if !data.input_parameter_changes.is_null() {
        let param_changes = data.input_parameter_changes;
        let vtbl = *(param_changes as *const *const IParameterChangesVtbl);
        let num_params = ((*vtbl).get_parameter_count)(param_changes);
        for i in 0..num_params {
            let queue = ((*vtbl).get_parameter_data)(param_changes, i);
            if !queue.is_null() {
                let queue_vtbl = *(queue as *const *const IParamValueQueueVtbl);
                let param_id = ((*queue_vtbl).get_parameter_id)(queue);
                if param_id == PARAM_GAIN {
                    let num_points = ((*queue_vtbl).get_point_count)(queue);
                    if num_points > 0 {
                        let mut sample_offset: int32 = 0;
                        let mut value: ParamValue = 0.0;
                        if ((*queue_vtbl).get_point)(queue, num_points - 1, &mut sample_offset, &mut value) == kResultOk {
                            (*obj).gain_state.set_gain(value * 2.0);
                        }
                    }
                }
            }
        }
    }
    
    let gain = (*obj).gain_state.get_gain() as f32;
    
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

unsafe extern "system" fn audio_get_tail_samples(_this: *mut c_void) -> uint32 { kNoTail }

// =============================================================================
// GUI Handler
// =============================================================================

/// Wrapper to hold window handles for softbuffer 0.4 lifetime requirements
struct WrappedWindow {
    raw_window_handle: RawWindowHandle,
    raw_display_handle: RawDisplayHandle,
}

impl HasWindowHandle for WrappedWindow {
    fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(self.raw_window_handle) })
    }
}

impl HasDisplayHandle for WrappedWindow {
    fn display_handle(&self) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        Ok(unsafe { raw_window_handle::DisplayHandle::borrow_raw(self.raw_display_handle) })
    }
}

struct GainGuiHandler {
    _wrapped: Option<Rc<WrappedWindow>>,
    _ctx: Option<Context<Rc<WrappedWindow>>>,
    surface: Option<Surface<Rc<WrappedWindow>, Rc<WrappedWindow>>>,
    physical_size: PhySize,
    logical_size: Size,
    gain_state: Arc<SharedGainState>,
    dragging: bool,
    cursor: Point,
}

impl GainGuiHandler {
    fn new(_window: &mut Window, gain_state: Arc<SharedGainState>) -> Self {
        Self {
            _wrapped: None,
            _ctx: None,
            surface: None,
            physical_size: PhySize::new(GUI_WIDTH as u32, GUI_HEIGHT as u32),
            logical_size: Size::new(GUI_WIDTH as f64, GUI_HEIGHT as f64),
            gain_state,
            dragging: false,
            cursor: Point::new(0.0, 0.0),
        }
    }

    fn ensure_resources(&mut self, window: &Window) {
        if self.surface.is_some() { return; }
        
        // Extract raw handles from baseview window (raw-window-handle 0.6)
        let Ok(window_handle) = window.window_handle() else { return; };
        let Ok(display_handle) = window.display_handle() else { return; };
        
        let mut raw_window = window_handle.as_raw();
        let raw_display = display_handle.as_raw();
        
        #[cfg(target_os = "macos")]
        {
            if let RawWindowHandle::AppKit(ref h) = raw_window {
                // In raw-window-handle 0.6+, ns_view is NonNull<c_void>
                let view = h.ns_view.as_ptr() as *mut Object;
                unsafe {
                    let _: () = msg_send![view, setWantsLayer: YES];
                }
                // Note: ns_window was removed in 0.6 - not needed for softbuffer
            }
        }
        
        // Create wrapped window for softbuffer (needs owned reference with 'static lifetime)
        let wrapped = Rc::new(WrappedWindow {
            raw_window_handle: raw_window,
            raw_display_handle: raw_display,
        });
        
        let ctx = match Context::new(wrapped.clone()) {
            Ok(ctx) => ctx,
            Err(_) => return,
        };
        
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Surface::new(&ctx, wrapped.clone())));
        if let Ok(Ok(mut s)) = result
        {
            if let (Some(w), Some(h)) = (NonZeroU32::new(self.physical_size.width), NonZeroU32::new(self.physical_size.height)) {
                let _ = s.resize(w, h);
            }
            self.surface = Some(s);
            self._ctx = Some(ctx);
            self._wrapped = Some(wrapped);
        }
    }

    fn gain_value(&self) -> f64 { self.gain_state.get_gain().clamp(0.0, 2.0) }

    fn set_gain_from_position(&self, pos: Point) {
        let slider_width = (self.logical_size.width - 2.0 * SLIDER_MARGIN).max(1.0);
        let normalized = ((pos.x - SLIDER_MARGIN) / slider_width).clamp(0.0, 1.0);
        self.gain_state.set_gain(normalized * 2.0);
    }

    fn slider_rect(&self) -> (f64, f64, f64, f64) {
        let slider_width = (self.logical_size.width - 2.0 * SLIDER_MARGIN).max(1.0);
        (SLIDER_MARGIN, (self.logical_size.height * 0.5) - (SLIDER_HEIGHT * 0.5), slider_width, SLIDER_HEIGHT)
    }

    fn knob_rect(&self, gain: f64) -> (f64, f64, f64, f64) {
        let (x, y, width, height) = self.slider_rect();
        let normalized = (gain / 2.0).clamp(0.0, 1.0);
        let knob_x = x + normalized * (width - KNOB_WIDTH);
        (knob_x, y - 6.0, KNOB_WIDTH, height + 12.0)
    }

    fn point_in_rect(pos: Point, rect: (f64, f64, f64, f64)) -> bool {
        pos.x >= rect.0 && pos.x <= rect.0 + rect.2 && pos.y >= rect.1 && pos.y <= rect.1 + rect.3
    }

    fn draw(&mut self, window: &Window) {
        self.ensure_resources(window);
        let width = self.physical_size.width as usize;
        let height = self.physical_size.height as usize;
        if width == 0 || height == 0 { return; }

        let gain = self.gain_value();
        let track = self.slider_rect();
        let knob = self.knob_rect(gain);
        let scale_x = self.physical_size.width as f64 / self.logical_size.width.max(1.0);
        let scale_y = self.physical_size.height as f64 / self.logical_size.height.max(1.0);

        if let Some(surface) = &mut self.surface {
            if let Ok(mut buffer) = surface.buffer_mut() {
                buffer.fill(0xFF202020);
                fill_rect(&mut buffer, width, height, track, scale_x, scale_y, 0xFF3A3A3A);
                fill_rect(&mut buffer, width, height, knob, scale_x, scale_y, 0xFF1BA1E2);
                let _ = buffer.present();
            }
        }
    }
}

impl WindowHandler for GainGuiHandler {
    fn on_frame(&mut self, window: &mut Window) { self.draw(window); }

    fn on_event(&mut self, _window: &mut Window, event: Event) -> EventStatus {
        match event {
            Event::Window(WindowEvent::Resized(info)) => {
                self.physical_size = info.physical_size();
                self.logical_size = info.logical_size();
                if let (Some(w), Some(h)) = (NonZeroU32::new(self.physical_size.width), NonZeroU32::new(self.physical_size.height)) {
                    if let Some(s) = &mut self.surface { let _ = s.resize(w, h); }
                }
            }
            Event::Mouse(MouseEvent::CursorMoved { position, .. }) => {
                self.cursor = position;
                if self.dragging { self.set_gain_from_position(position); }
            }
            Event::Mouse(MouseEvent::ButtonPressed { button: MouseButton::Left, .. }) => {
                if Self::point_in_rect(self.cursor, self.slider_rect()) {
                    self.dragging = true;
                    self.set_gain_from_position(self.cursor);
                }
            }
            Event::Mouse(MouseEvent::ButtonReleased { button: MouseButton::Left, .. }) => { self.dragging = false; }
            _ => {}
        }
        EventStatus::Captured
    }
}

fn fill_rect(buffer: &mut [u32], width: usize, height: usize, rect: (f64, f64, f64, f64), scale_x: f64, scale_y: f64, color: u32) {
    let (x, y, w, h) = rect;
    let start_x = (x * scale_x).round().max(0.0) as usize;
    let start_y = (y * scale_y).round().max(0.0) as usize;
    let end_x = ((x + w) * scale_x).round().min(width as f64) as usize;
    let end_y = ((y + h) * scale_y).round().min(height as f64) as usize;
    for row in start_y..end_y {
        for col in start_x..end_x {
            buffer[row * width + col] = color;
        }
    }
}

// =============================================================================
// IPlugView Implementation
// =============================================================================

#[repr(C)]
struct PlugViewObj {
    vtbl: *const IPlugViewVtbl,
    ref_count: AtomicI32,
    gain_state: Arc<SharedGainState>,
    gui_handle: Option<baseview::WindowHandle>,
}

static PLUGVIEW_VTBL: IPlugViewVtbl = IPlugViewVtbl {
    unknown: IUnknownVtbl {
        query_interface: plugview_query_interface,
        add_ref: plugview_add_ref,
        release: plugview_release,
    },
    is_platform_type_supported: plugview_is_platform_type_supported,
    attached: plugview_attached,
    removed: plugview_removed,
    on_wheel: plugview_on_wheel,
    on_key_down: plugview_on_key_down,
    on_key_up: plugview_on_key_up,
    get_size: plugview_get_size,
    on_size: plugview_on_size,
    on_focus: plugview_on_focus,
    set_frame: plugview_set_frame,
    can_resize: plugview_can_resize,
    check_size_constraint: plugview_check_size_constraint,
};

impl PlugViewObj {
    fn new(gain_state: Arc<SharedGainState>) -> *mut Self {
        Box::into_raw(Box::new(PlugViewObj {
            vtbl: &PLUGVIEW_VTBL,
            ref_count: AtomicI32::new(1),
            gain_state,
            gui_handle: None,
        }))
    }
}

unsafe extern "system" fn plugview_query_interface(this: *mut c_void, iid: *const TUID, obj: *mut *mut c_void) -> tresult {
    let iid = &*iid;
    if iid_equal(iid, &iid::IUnknown) || iid_equal(iid, &gui_iid::IPlugView) {
        plugview_add_ref(this);
        *obj = this;
        return kResultOk;
    }
    *obj = std::ptr::null_mut();
    kNoInterface
}

unsafe extern "system" fn plugview_add_ref(this: *mut c_void) -> uint32 {
    (*(this as *mut PlugViewObj)).ref_count.fetch_add(1, Ordering::SeqCst) as uint32 + 1
}

unsafe extern "system" fn plugview_release(this: *mut c_void) -> uint32 {
    let obj = this as *mut PlugViewObj;
    let count = (*obj).ref_count.fetch_sub(1, Ordering::SeqCst) - 1;
    if count == 0 { let _ = Box::from_raw(obj); }
    count as uint32
}

unsafe extern "system" fn plugview_is_platform_type_supported(_this: *mut c_void, type_: FIDString) -> tresult {
    let type_bytes = std::slice::from_raw_parts(type_ as *const u8, 16);
    #[cfg(target_os = "macos")]
    if type_bytes.starts_with(b"NSView") { return kResultOk; }
    #[cfg(target_os = "windows")]
    if type_bytes.starts_with(b"HWND") { return kResultOk; }
    #[cfg(target_os = "linux")]
    if type_bytes.starts_with(b"X11") { return kResultOk; }
    kResultFalse
}

unsafe extern "system" fn plugview_attached(this: *mut c_void, parent: *mut c_void, type_: FIDString) -> tresult {
    let obj = this as *mut PlugViewObj;
    if (*obj).gui_handle.is_some() { return kResultOk; }
    
    let type_bytes = std::slice::from_raw_parts(type_ as *const u8, 16);
    
    #[cfg(target_os = "macos")]
    let parent_handle = {
        use raw_window_handle::AppKitWindowHandle;
        if type_bytes.starts_with(b"NSView") {
            std::ptr::NonNull::new(parent).map(|ns_view| {
                RawWindowHandle::AppKit(AppKitWindowHandle::new(ns_view))
            })
        } else { None }
    };
    
    #[cfg(target_os = "windows")]
    let parent_handle = {
        use raw_window_handle::Win32WindowHandle;
        use std::num::NonZeroIsize;
        if type_bytes.starts_with(b"HWND") {
            NonZeroIsize::new(parent as isize).map(|hwnd| {
                RawWindowHandle::Win32(Win32WindowHandle::new(hwnd))
            })
        } else { None }
    };
    
    #[cfg(target_os = "linux")]
    let parent_handle = {
        use raw_window_handle::XlibWindowHandle;
        if type_bytes.starts_with(b"X11") {
            Some(RawWindowHandle::Xlib(XlibWindowHandle::new(parent as u64)))
        } else { None }
    };
    
    if let Some(handle) = parent_handle {
        struct ParentWindow(RawWindowHandle);
        impl HasWindowHandle for ParentWindow {
            fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
                Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(self.0) })
            }
        }
        let parent_window = ParentWindow(handle);
        
        let options = WindowOpenOptions {
            title: "Gain".into(),
            size: Size::new(GUI_WIDTH as f64, GUI_HEIGHT as f64),
            scale: WindowScalePolicy::SystemScaleFactor,

        };
        
        let gain_state = (*obj).gain_state.clone();
        let handle = Window::open_parented(&parent_window, options, move |window| {
            GainGuiHandler::new(window, gain_state)
        });
        (*obj).gui_handle = Some(handle);
        return kResultOk;
    }
    
    kResultFalse
}

unsafe extern "system" fn plugview_removed(this: *mut c_void) -> tresult {
    let obj = this as *mut PlugViewObj;
    if let Some(mut handle) = (*obj).gui_handle.take() {
        handle.close();
    }
    kResultOk
}

unsafe extern "system" fn plugview_on_wheel(_this: *mut c_void, _distance: f32) -> tresult { kResultOk }
unsafe extern "system" fn plugview_on_key_down(_this: *mut c_void, _key: char16, _key_code: int16, _modifiers: int16) -> tresult { kResultOk }
unsafe extern "system" fn plugview_on_key_up(_this: *mut c_void, _key: char16, _key_code: int16, _modifiers: int16) -> tresult { kResultOk }

unsafe extern "system" fn plugview_get_size(_this: *mut c_void, size: *mut ViewRect) -> tresult {
    let size = &mut *size;
    size.left = 0;
    size.top = 0;
    size.right = GUI_WIDTH;
    size.bottom = GUI_HEIGHT;
    kResultOk
}

unsafe extern "system" fn plugview_on_size(_this: *mut c_void, _new_size: *mut ViewRect) -> tresult { kResultOk }
unsafe extern "system" fn plugview_on_focus(_this: *mut c_void, _state: TBool) -> tresult { kResultOk }
unsafe extern "system" fn plugview_set_frame(_this: *mut c_void, _frame: *mut c_void) -> tresult { kResultOk }
unsafe extern "system" fn plugview_can_resize(_this: *mut c_void) -> tresult { kResultFalse }
unsafe extern "system" fn plugview_check_size_constraint(_this: *mut c_void, _rect: *mut ViewRect) -> tresult { kResultOk }

// =============================================================================
// Controller Implementation (with GUI support)
// =============================================================================

#[repr(C)]
struct GainControllerObj {
    vtbl: *const ControllerVtbl,
    ref_count: AtomicI32,
    gain_state: Arc<SharedGainState>,
}

#[repr(C)]
struct ControllerVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const TUID, *mut *mut c_void) -> tresult,
    add_ref: unsafe extern "system" fn(*mut c_void) -> uint32,
    release: unsafe extern "system" fn(*mut c_void) -> uint32,
    initialize: unsafe extern "system" fn(*mut c_void, *mut c_void) -> tresult,
    terminate: unsafe extern "system" fn(*mut c_void) -> tresult,
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

impl GainControllerObj {
    fn new(gain_state: Arc<SharedGainState>) -> *mut Self {
        Box::into_raw(Box::new(GainControllerObj {
            vtbl: &CONTROLLER_VTBL,
            ref_count: AtomicI32::new(1),
            gain_state,
        }))
    }
}

unsafe extern "system" fn controller_query_interface(this: *mut c_void, iid: *const TUID, obj: *mut *mut c_void) -> tresult {
    let iid = &*iid;
    if iid_equal(iid, &iid::IUnknown) || iid_equal(iid, &base_iid::IPluginBase) || iid_equal(iid, &vst_iid::IEditController) {
        controller_add_ref(this);
        *obj = this;
        return kResultOk;
    }
    *obj = std::ptr::null_mut();
    kNoInterface
}

unsafe extern "system" fn controller_add_ref(this: *mut c_void) -> uint32 {
    (*(this as *mut GainControllerObj)).ref_count.fetch_add(1, Ordering::SeqCst) as uint32 + 1
}

unsafe extern "system" fn controller_release(this: *mut c_void) -> uint32 {
    let obj = this as *mut GainControllerObj;
    let count = (*obj).ref_count.fetch_sub(1, Ordering::SeqCst) - 1;
    if count == 0 { let _ = Box::from_raw(obj); }
    count as uint32
}

unsafe extern "system" fn controller_initialize(_this: *mut c_void, _context: *mut c_void) -> tresult { kResultOk }
unsafe extern "system" fn controller_terminate(_this: *mut c_void) -> tresult { kResultOk }
unsafe extern "system" fn controller_set_component_state(_this: *mut c_void, _state: *mut c_void) -> tresult { kResultOk }
unsafe extern "system" fn controller_set_state(_this: *mut c_void, _state: *mut c_void) -> tresult { kResultOk }
unsafe extern "system" fn controller_get_state(_this: *mut c_void, _state: *mut c_void) -> tresult { kResultOk }
unsafe extern "system" fn controller_get_parameter_count(_this: *mut c_void) -> int32 { 1 }

unsafe extern "system" fn controller_get_parameter_info(_this: *mut c_void, param_index: int32, info: *mut ParameterInfo) -> tresult {
    if param_index != 0 { return kInvalidArgument; }
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

unsafe extern "system" fn controller_get_param_string_by_value(_this: *mut c_void, id: ParamID, value: ParamValue, string: *mut String128) -> tresult {
    if id == PARAM_GAIN {
        str16cpy_safe(&mut *string, &format!("{}%", (value * 200.0) as i32));
        return kResultOk;
    }
    kInvalidArgument
}

unsafe extern "system" fn controller_get_param_value_by_string(_this: *mut c_void, _id: ParamID, _string: *const TChar, _value: *mut ParamValue) -> tresult { kNotImplemented }
unsafe extern "system" fn controller_normalized_param_to_plain(_this: *mut c_void, _id: ParamID, value: ParamValue) -> ParamValue { value * 200.0 }
unsafe extern "system" fn controller_plain_param_to_normalized(_this: *mut c_void, _id: ParamID, value: ParamValue) -> ParamValue { value / 200.0 }

unsafe extern "system" fn controller_get_param_normalized(this: *mut c_void, _id: ParamID) -> ParamValue {
    (*(this as *mut GainControllerObj)).gain_state.get_gain() / 2.0
}

unsafe extern "system" fn controller_set_param_normalized(this: *mut c_void, _id: ParamID, value: ParamValue) -> tresult {
    (*(this as *mut GainControllerObj)).gain_state.set_gain(value * 2.0);
    kResultOk
}

unsafe extern "system" fn controller_set_component_handler(_this: *mut c_void, _handler: *mut c_void) -> tresult { kResultOk }

unsafe extern "system" fn controller_create_view(this: *mut c_void, name: FIDString) -> *mut c_void {
    let name_bytes = std::slice::from_raw_parts(name as *const u8, 16);
    if name_bytes.starts_with(b"editor") {
        let controller = this as *mut GainControllerObj;
        return PlugViewObj::new((*controller).gain_state.clone()) as *mut c_void;
    }
    std::ptr::null_mut()
}

// =============================================================================
// Plugin Factory
// =============================================================================

#[repr(C)]
struct PluginFactoryObj {
    vtbl: *const PluginFactoryVtbl,
    ref_count: AtomicI32,
    gain_state: Arc<SharedGainState>,
}

#[repr(C)]
struct PluginFactoryVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const TUID, *mut *mut c_void) -> tresult,
    add_ref: unsafe extern "system" fn(*mut c_void) -> uint32,
    release: unsafe extern "system" fn(*mut c_void) -> uint32,
    get_factory_info: unsafe extern "system" fn(*mut c_void, *mut PFactoryInfoData) -> tresult,
    count_classes: unsafe extern "system" fn(*mut c_void) -> int32,
    get_class_info: unsafe extern "system" fn(*mut c_void, int32, *mut PClassInfoData) -> tresult,
    create_instance: unsafe extern "system" fn(*mut c_void, FIDString, FIDString, *mut *mut c_void) -> tresult,
    get_class_info2: unsafe extern "system" fn(*mut c_void, int32, *mut PClassInfo2Data) -> tresult,
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

struct SendSyncPtr(*mut PluginFactoryObj);
unsafe impl Send for SendSyncPtr {}
unsafe impl Sync for SendSyncPtr {}
static FACTORY: OnceLock<SendSyncPtr> = OnceLock::new();

unsafe extern "system" fn factory_query_interface(this: *mut c_void, iid: *const TUID, obj: *mut *mut c_void) -> tresult {
    let iid = &*iid;
    if iid_equal(iid, &iid::IUnknown) || iid_equal(iid, &base_iid::IPluginFactory) || iid_equal(iid, &base_iid::IPluginFactory2) || iid_equal(iid, &base_iid::IPluginFactory3) {
        factory_add_ref(this);
        *obj = this;
        return kResultOk;
    }
    *obj = std::ptr::null_mut();
    kNoInterface
}

unsafe extern "system" fn factory_add_ref(this: *mut c_void) -> uint32 {
    (*(this as *mut PluginFactoryObj)).ref_count.fetch_add(1, Ordering::SeqCst) as uint32 + 1
}

unsafe extern "system" fn factory_release(this: *mut c_void) -> uint32 {
    (*(this as *mut PluginFactoryObj)).ref_count.fetch_sub(1, Ordering::SeqCst) as uint32
}

unsafe extern "system" fn factory_get_factory_info(_this: *mut c_void, info: *mut PFactoryInfoData) -> tresult {
    let info = &mut *info;
    strcpy_safe(&mut info.vendor, b"aizcutei\0");
    strcpy_safe(&mut info.url, b"https://aizcutei.github.io/sunmao\0");
    strcpy_safe(&mut info.email, b"info@example.com\0");
    info.flags = PFactoryInfo::Flags::kUnicode;
    kResultOk
}

unsafe extern "system" fn factory_count_classes(_this: *mut c_void) -> int32 { 2 }

unsafe extern "system" fn factory_get_class_info(_this: *mut c_void, index: int32, info: *mut PClassInfoData) -> tresult {
    let info = &mut *info;
    match index {
        0 => { info.cid = CID_PROCESSOR; info.cardinality = PClassInfo::kManyInstances; strcpy_safe(&mut info.category, kVstAudioEffectClass); strcpy_safe(&mut info.name, b"Vst3 Sys Fx Gain GUI Baseview\0"); }
        1 => { info.cid = CID_CONTROLLER; info.cardinality = PClassInfo::kManyInstances; strcpy_safe(&mut info.category, kVstComponentControllerClass); strcpy_safe(&mut info.name, b"Vst3 Sys Fx Gain GUI Baseview Controller\0"); }
        _ => return kInvalidArgument,
    }
    kResultOk
}

unsafe extern "system" fn factory_get_class_info2(_this: *mut c_void, index: int32, info: *mut PClassInfo2Data) -> tresult {
    let info = &mut *info;
    match index {
        0 => {
            info.cid = CID_PROCESSOR; info.cardinality = PClassInfo::kManyInstances;
            strcpy_safe(&mut info.category, kVstAudioEffectClass); strcpy_safe(&mut info.name, b"Vst3 Sys Fx Gain GUI Baseview\0");
            info.class_flags = ComponentFlags::kSimpleModeSupported;
            strcpy_safe(&mut info.sub_categories, PlugType::kFx); strcpy_safe(&mut info.vendor, b"aizcutei\0");
            strcpy_safe(&mut info.version, b"0.1.0\0"); strcpy_safe(&mut info.sdk_version, b"VST 3.8.0\0");
        }
        1 => {
            info.cid = CID_CONTROLLER; info.cardinality = PClassInfo::kManyInstances;
            strcpy_safe(&mut info.category, kVstComponentControllerClass); strcpy_safe(&mut info.name, b"Vst3 Sys Fx Gain GUI Baseview Controller\0");
            info.class_flags = 0; strcpy_safe(&mut info.sub_categories, b"\0"); strcpy_safe(&mut info.vendor, b"aizcutei\0");
            strcpy_safe(&mut info.version, b"0.1.0\0"); strcpy_safe(&mut info.sdk_version, b"VST 3.8.0\0");
        }
        _ => return kInvalidArgument,
    }
    kResultOk
}

unsafe extern "system" fn factory_create_instance(this: *mut c_void, cid: FIDString, _iid: FIDString, obj: *mut *mut c_void) -> tresult {
    let cid_bytes = std::slice::from_raw_parts(cid as *const i8, 16);
    let mut cid_arr: TUID = [0; 16];
    cid_arr.copy_from_slice(cid_bytes);
    
    let factory = this as *mut PluginFactoryObj;
    
    if iid_equal(&cid_arr, &CID_PROCESSOR) {
        *obj = GainProcessorObj::new((*factory).gain_state.clone()) as *mut c_void;
        return kResultOk;
    }
    if iid_equal(&cid_arr, &CID_CONTROLLER) {
        *obj = GainControllerObj::new((*factory).gain_state.clone()) as *mut c_void;
        return kResultOk;
    }
    *obj = std::ptr::null_mut();
    kNoInterface
}

unsafe extern "system" fn factory_get_class_info_unicode(_this: *mut c_void, index: int32, info: *mut PClassInfoWData) -> tresult {
    let info = &mut *info;
    match index {
        0 => {
            info.cid = CID_PROCESSOR; info.cardinality = PClassInfo::kManyInstances;
            strcpy_safe(&mut info.category, kVstAudioEffectClass); str16cpy(&mut info.name, "Vst3 Sys Fx Gain GUI Baseview");
            info.class_flags = ComponentFlags::kSimpleModeSupported; strcpy_safe(&mut info.sub_categories, PlugType::kFx);
            str16cpy(&mut info.vendor, "aizcutei"); str16cpy(&mut info.version, "0.1.0"); str16cpy(&mut info.sdk_version, "VST 3.8.0");
        }
        1 => {
            info.cid = CID_CONTROLLER; info.cardinality = PClassInfo::kManyInstances;
            strcpy_safe(&mut info.category, kVstComponentControllerClass); str16cpy(&mut info.name, "Vst3 Sys Fx Gain GUI Baseview Controller");
            info.class_flags = 0; strcpy_safe(&mut info.sub_categories, b"\0");
            str16cpy(&mut info.vendor, "aizcutei"); str16cpy(&mut info.version, "0.1.0"); str16cpy(&mut info.sdk_version, "VST 3.8.0");
        }
        _ => return kInvalidArgument,
    }
    kResultOk
}

unsafe extern "system" fn factory_set_host_context(_this: *mut c_void, _context: *mut c_void) -> tresult { kResultOk }

// =============================================================================
// Entry Points
// =============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn GetPluginFactory() -> *mut c_void {
    FACTORY.get_or_init(|| {
        SendSyncPtr(Box::into_raw(Box::new(PluginFactoryObj {
            vtbl: &FACTORY_VTBL,
            ref_count: AtomicI32::new(1),
            gain_state: SharedGainState::new(),
        })))
    }).0 as *mut c_void
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn bundleEntry(_bundle: *mut c_void) -> bool { true }

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn bundleExit() -> bool { true }

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn ModuleEntry(_: *mut c_void) -> bool { true }

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
pub extern "C" fn ModuleExit() -> bool { true }

#[cfg(target_os = "windows")]
#[unsafe(no_mangle)]
pub extern "C" fn InitDll() -> bool { true }

#[cfg(target_os = "windows")]
#[unsafe(no_mangle)]
pub extern "C" fn ExitDll() -> bool { true }
