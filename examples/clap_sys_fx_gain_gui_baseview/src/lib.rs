use baseview::{
    Event, EventStatus, MouseButton, MouseEvent, PhySize, Point, Size, Window, WindowEvent,
    WindowHandler, WindowOpenOptions, WindowScalePolicy,
};
use clap_sys::audio_buffer::clap_audio_buffer_t;
use clap_sys::events::{CLAP_EVENT_PARAM_VALUE, clap_event_param_value_t, clap_input_events_t};
use clap_sys::ext::audio_ports::{
    CLAP_AUDIO_PORT_IS_MAIN, CLAP_PORT_STEREO, clap_audio_port_info_t, clap_plugin_audio_ports_t,
};
use clap_sys::ext::gui::{
    CLAP_EXT_GUI, CLAP_WINDOW_API_COCOA, CLAP_WINDOW_API_WIN32, CLAP_WINDOW_API_X11,
    clap_gui_resize_hints_t, clap_plugin_gui_t, clap_window_t,
};
use clap_sys::ext::params::{CLAP_PARAM_IS_AUTOMATABLE, clap_param_info_t, clap_plugin_params_t};
use clap_sys::ext::state::{CLAP_EXT_STATE, clap_plugin_state_t};
use clap_sys::factory::plugin_factory::{CLAP_PLUGIN_FACTORY_ID, clap_plugin_factory_t};
use clap_sys::host::clap_host_t;
use clap_sys::id::{CLAP_INVALID_ID, clap_id};
use clap_sys::plugin::{clap_plugin_descriptor_t, clap_plugin_t};
use clap_sys::plugin_features::{CLAP_PLUGIN_FEATURE_AUDIO_EFFECT, CLAP_PLUGIN_FEATURE_STEREO};
use clap_sys::process::{CLAP_PROCESS_CONTINUE, clap_process_status, clap_process_t};
use clap_sys::stream::{clap_istream_t, clap_ostream_t};
use clap_sys::string_sizes::CLAP_PATH_SIZE;
use clap_sys::version::{CLAP_VERSION, clap_version_is_compatible};
#[cfg(target_os = "macos")]
use objc::runtime::{Object, YES};
#[cfg(target_os = "macos")]
use objc::{msg_send, sel, sel_impl};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use softbuffer::{Context, Surface};
use std::ffi::{CStr, CString, c_char, c_void};
use std::num::NonZeroU32;
use std::ptr;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

const PARAM_GAIN: clap_id = 0;
const GUI_WIDTH: f64 = 400.0;
const GUI_HEIGHT: f64 = 120.0;
const SLIDER_MARGIN: f64 = 20.0;
const SLIDER_HEIGHT: f64 = 20.0;
const KNOB_WIDTH: f64 = 12.0;

struct GainEffectGui {
    host: *const clap_host_t,
    gain_bits: Arc<AtomicU64>,
    gui_parent: Option<RawWindowHandle>,
    gui_handle: Option<baseview::WindowHandle>,
    gui_open: bool,
}

struct SyncDescriptor(clap_plugin_descriptor_t);
unsafe impl Sync for SyncDescriptor {}

struct SyncFeatures(&'static [*const c_char]);
unsafe impl Sync for SyncFeatures {}

static FEATURES: SyncFeatures = SyncFeatures(&[
    CLAP_PLUGIN_FEATURE_AUDIO_EFFECT.as_ptr() as *const c_char,
    CLAP_PLUGIN_FEATURE_STEREO.as_ptr() as *const c_char,
    ptr::null(),
]);

static DESCRIPTOR: SyncDescriptor = SyncDescriptor(clap_plugin_descriptor_t {
    clap_version: CLAP_VERSION,
    id: b"com.sunmao.clap_sys.fx_gain_gui_baseview\0".as_ptr() as *const c_char,
    name: b"Clap Sys Fx Gain Gui Baseview\0".as_ptr() as *const c_char,
    vendor: b"aizcutei\0".as_ptr() as *const c_char,
    url: b"https://aizcutei.github.io/sunmao\0".as_ptr() as *const c_char,
    manual_url: b"https://aizcutei.github.io/sunmao/manual\0".as_ptr() as *const c_char,
    support_url: b"https://aizcutei.github.io/sunmao/support\0".as_ptr() as *const c_char,
    version: b"0.1\0".as_ptr() as *const c_char,
    description: b"Simple gain effect with baseview GUI\0".as_ptr() as *const c_char,
    features: FEATURES.0.as_ptr(),
});

struct ParentWindow {
    handle: RawWindowHandle,
}

impl HasWindowHandle for ParentWindow {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(self.handle) })
    }
}

/// Wrapper to hold window handles for softbuffer 0.4 lifetime requirements
struct WrappedWindow {
    raw_window_handle: RawWindowHandle,
    raw_display_handle: RawDisplayHandle,
}

impl HasWindowHandle for WrappedWindow {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(self.raw_window_handle) })
    }
}

impl HasDisplayHandle for WrappedWindow {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        Ok(unsafe { raw_window_handle::DisplayHandle::borrow_raw(self.raw_display_handle) })
    }
}

struct GainGuiHandler {
    _wrapped: Option<Rc<WrappedWindow>>,
    _ctx: Option<Context<Rc<WrappedWindow>>>,
    surface: Option<Surface<Rc<WrappedWindow>, Rc<WrappedWindow>>>,
    physical_size: PhySize,
    logical_size: Size,
    gain_bits: Arc<AtomicU64>,
    dragging: bool,
    cursor: Point,
}

impl GainGuiHandler {
    fn new(_window: &mut Window, gain_bits: Arc<AtomicU64>) -> Self {
        let physical_size = PhySize::new(GUI_WIDTH as u32, GUI_HEIGHT as u32);

        Self {
            _wrapped: None,
            _ctx: None,
            surface: None,
            physical_size,
            logical_size: Size::new(GUI_WIDTH, GUI_HEIGHT),
            gain_bits,
            dragging: false,
            cursor: Point::new(0.0, 0.0),
        }
    }

    fn ensure_resources(&mut self, window: &Window) {
        if self.surface.is_some() {
            return;
        }

        // Extract raw handles from baseview window (raw-window-handle 0.6)
        let Ok(window_handle) = window.window_handle() else {
            return;
        };
        let Ok(display_handle) = window.display_handle() else {
            return;
        };

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
            Err(_) => {
                return;
            }
        };

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Surface::new(&ctx, wrapped.clone())
        }));

        match result {
            Ok(Ok(mut s)) => {
                if let (Some(width), Some(height)) = (
                    NonZeroU32::new(self.physical_size.width),
                    NonZeroU32::new(self.physical_size.height),
                ) {
                    let _ = s.resize(width, height);
                }
                self.surface = Some(s);
                self._ctx = Some(ctx);
                self._wrapped = Some(wrapped);
            }
            Ok(Err(_)) => {}
            Err(_) => {}
        }
    }

    fn gain_value(&self) -> f64 {
        let gain = f64::from_bits(self.gain_bits.load(Ordering::Relaxed));
        gain.clamp(0.0, 2.0)
    }

    fn set_gain_from_position(&self, pos: Point) {
        let slider_width = (self.logical_size.width - 2.0 * SLIDER_MARGIN).max(1.0);
        let track_x = SLIDER_MARGIN;
        let normalized = ((pos.x - track_x) / slider_width).clamp(0.0, 1.0);
        let gain = normalized * 2.0;
        self.gain_bits.store(gain.to_bits(), Ordering::Relaxed);
    }

    fn slider_rect(&self) -> (f64, f64, f64, f64) {
        let slider_width = (self.logical_size.width - 2.0 * SLIDER_MARGIN).max(1.0);
        let x = SLIDER_MARGIN;
        let y = (self.logical_size.height * 0.5) - (SLIDER_HEIGHT * 0.5);
        (x, y, slider_width, SLIDER_HEIGHT)
    }

    fn knob_rect(&self, gain: f64) -> (f64, f64, f64, f64) {
        let (x, y, width, height) = self.slider_rect();
        let normalized = (gain / 2.0).clamp(0.0, 1.0);
        let knob_x = x + normalized * (width - KNOB_WIDTH);
        (knob_x, y - 6.0, KNOB_WIDTH, height + 12.0)
    }

    fn point_in_rect(pos: Point, rect: (f64, f64, f64, f64)) -> bool {
        let (x, y, w, h) = rect;
        pos.x >= x && pos.x <= x + w && pos.y >= y && pos.y <= y + h
    }

    fn draw(&mut self, window: &Window) {
        self.ensure_resources(window);

        let width = self.physical_size.width as usize;
        let height = self.physical_size.height as usize;
        if width == 0 || height == 0 {
            return;
        }

        let gain = self.gain_value();
        let track = self.slider_rect();
        let knob = self.knob_rect(gain);
        let scale_x = self.physical_size.width as f64 / self.logical_size.width.max(1.0);
        let scale_y = self.physical_size.height as f64 / self.logical_size.height.max(1.0);

        if let Some(surface) = &mut self.surface {
            if let Ok(mut buffer) = surface.buffer_mut() {
                buffer.fill(0xFF202020);
                fill_rect(
                    &mut buffer,
                    width,
                    height,
                    track,
                    scale_x,
                    scale_y,
                    0xFF3A3A3A,
                );
                fill_rect(
                    &mut buffer,
                    width,
                    height,
                    knob,
                    scale_x,
                    scale_y,
                    0xFF1BA1E2,
                );
                let _ = buffer.present();
            }
        }
    }
}

impl WindowHandler for GainGuiHandler {
    fn on_frame(&mut self, window: &mut Window) {
        self.draw(window);
    }

    fn on_event(&mut self, _window: &mut Window, event: Event) -> EventStatus {
        match event {
            Event::Window(WindowEvent::Resized(info)) => {
                let new_size = info.physical_size();
                self.physical_size = new_size;
                self.logical_size = info.logical_size();
                if let (Some(width), Some(height)) = (
                    NonZeroU32::new(new_size.width),
                    NonZeroU32::new(new_size.height),
                ) {
                    if let Some(surface) = &mut self.surface {
                        let _ = surface.resize(width, height);
                    }
                }
            }
            Event::Mouse(MouseEvent::CursorMoved { position, .. }) => {
                self.cursor = position;
                if self.dragging {
                    self.set_gain_from_position(position);
                }
            }
            Event::Mouse(MouseEvent::ButtonPressed {
                button: MouseButton::Left,
                ..
            }) => {
                if Self::point_in_rect(self.cursor, self.slider_rect()) {
                    self.dragging = true;
                    self.set_gain_from_position(self.cursor);
                }
            }
            Event::Mouse(MouseEvent::ButtonReleased {
                button: MouseButton::Left,
                ..
            }) => {
                self.dragging = false;
            }
            _ => {}
        }

        EventStatus::Captured
    }
}

fn fill_rect(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    rect: (f64, f64, f64, f64),
    scale_x: f64,
    scale_y: f64,
    color: u32,
) {
    let (x, y, w, h) = rect;
    let start_x = (x * scale_x).round().max(0.0) as usize;
    let start_y = (y * scale_y).round().max(0.0) as usize;
    let end_x = ((x + w) * scale_x).round().min(width as f64) as usize;
    let end_y = ((y + h) * scale_y).round().min(height as f64) as usize;
    for row in start_y..end_y {
        let row_start = row * width;
        for col in start_x..end_x {
            buffer[row_start + col] = color;
        }
    }
}

fn gain_from_bits(bits: u64) -> f64 {
    f64::from_bits(bits).clamp(0.0, 2.0)
}

fn raw_window_handle_from_clap(window: &clap_window_t) -> Option<RawWindowHandle> {
    if window.api.is_null() {
        return None;
    }
    let api = unsafe { CStr::from_ptr(window.api) };
    if api.to_bytes_with_nul() == CLAP_WINDOW_API_COCOA.as_bytes() {
        use raw_window_handle::AppKitWindowHandle;
        unsafe {
            if let Some(ns_view) = std::ptr::NonNull::new(window.handle.cocoa) {
                let handle = AppKitWindowHandle::new(ns_view);
                return Some(RawWindowHandle::AppKit(handle));
            }
        }
        return None;
    }
    if api.to_bytes_with_nul() == CLAP_WINDOW_API_WIN32.as_bytes() {
        use raw_window_handle::Win32WindowHandle;
        use std::num::NonZeroIsize;
        unsafe {
            if let Some(hwnd) = NonZeroIsize::new(window.handle.win32 as isize) {
                let handle = Win32WindowHandle::new(hwnd);
                return Some(RawWindowHandle::Win32(handle));
            }
        }
        return None;
    }
    if api.to_bytes_with_nul() == CLAP_WINDOW_API_X11.as_bytes() {
        use raw_window_handle::XlibWindowHandle;
        unsafe {
            let handle = XlibWindowHandle::new(window.handle.x11);
            return Some(RawWindowHandle::Xlib(handle));
        }
    }
    None
}

fn open_gui_window(effect: &mut GainEffectGui) -> bool {
    if effect.gui_handle.is_some() {
        return true;
    }
    let parent_handle = match effect.gui_parent {
        Some(handle) => handle,
        None => return false,
    };
    let parent = ParentWindow {
        handle: parent_handle,
    };
    let options = WindowOpenOptions::new(
        "CLAP Sys Gain",
        Size::new(GUI_WIDTH, GUI_HEIGHT),
        WindowScalePolicy::SystemScaleFactor,
    );
    let gain_bits = effect.gain_bits.clone();
    let window_handle = Window::open_parented(&parent, options, move |window| {
        GainGuiHandler::new(window, gain_bits)
    });
    effect.gui_handle = Some(window_handle);
    true
}

unsafe extern "C" fn plugin_init(_plugin: *const clap_plugin_t) -> bool {
    // Logging removed as requested
    true
}

unsafe extern "C" fn plugin_destroy(plugin: *const clap_plugin_t) {
    unsafe {
        let instance = (*plugin).plugin_data as *mut GainEffectGui;
        if !instance.is_null() {
            let _ = Box::from_raw(instance);
        }
        let _ = Box::from_raw(plugin as *mut clap_plugin_t);
    }
}

unsafe extern "C" fn plugin_activate(
    _plugin: *const clap_plugin_t,
    _sample_rate: f64,
    _min_frames_count: u32,
    _max_frames_count: u32,
) -> bool {
    true
}

unsafe extern "C" fn plugin_deactivate(_plugin: *const clap_plugin_t) {}

unsafe extern "C" fn plugin_start_processing(_plugin: *const clap_plugin_t) -> bool {
    true
}

unsafe extern "C" fn plugin_stop_processing(_plugin: *const clap_plugin_t) {}

unsafe extern "C" fn plugin_reset(_plugin: *const clap_plugin_t) {}

unsafe fn apply_param_events(effect: &GainEffectGui, in_events: *const clap_input_events_t) {
    if in_events.is_null() {
        return;
    }
    let size_fn = (*in_events).size;
    let get_fn = (*in_events).get;
    if size_fn.is_none() || get_fn.is_none() {
        return;
    }
    let size = size_fn.unwrap()(in_events);
    for index in 0..size {
        let header = get_fn.unwrap()(in_events, index);
        if header.is_null() {
            continue;
        }
        if (*header).space_id == clap_sys::events::CLAP_CORE_EVENT_SPACE_ID
            && (*header).type_ == CLAP_EVENT_PARAM_VALUE
            && (*header).size >= std::mem::size_of::<clap_event_param_value_t>() as u32
        {
            let event = &*(header as *const clap_event_param_value_t);
            if event.param_id == PARAM_GAIN {
                effect
                    .gain_bits
                    .store(event.value.clamp(0.0, 2.0).to_bits(), Ordering::Relaxed);
            }
        }
    }
}

unsafe fn process_audio_f32(
    effect: &GainEffectGui,
    input: &clap_audio_buffer_t,
    output: &mut clap_audio_buffer_t,
    frames: usize,
) {
    if input.data32.is_null() || output.data32.is_null() {
        return;
    }
    let gain = gain_from_bits(effect.gain_bits.load(Ordering::Relaxed));
    let in_channels = std::slice::from_raw_parts(input.data32, input.channel_count as usize);
    let out_channels = std::slice::from_raw_parts_mut(output.data32, output.channel_count as usize);
    let channels = in_channels.len().min(out_channels.len());
    for ch in 0..channels {
        let in_ptr = in_channels[ch];
        let out_ptr = out_channels[ch];
        if in_ptr.is_null() || out_ptr.is_null() {
            continue;
        }
        let in_buf = std::slice::from_raw_parts(in_ptr, frames);
        let out_buf = std::slice::from_raw_parts_mut(out_ptr, frames);
        for i in 0..frames {
            out_buf[i] = in_buf[i] * gain as f32;
        }
    }
}

unsafe fn process_audio_f64(
    effect: &GainEffectGui,
    input: &clap_audio_buffer_t,
    output: &mut clap_audio_buffer_t,
    frames: usize,
) {
    if input.data64.is_null() || output.data64.is_null() {
        return;
    }
    let gain = gain_from_bits(effect.gain_bits.load(Ordering::Relaxed));
    let in_channels = std::slice::from_raw_parts(input.data64, input.channel_count as usize);
    let out_channels = std::slice::from_raw_parts_mut(output.data64, output.channel_count as usize);
    let channels = in_channels.len().min(out_channels.len());
    for ch in 0..channels {
        let in_ptr = in_channels[ch];
        let out_ptr = out_channels[ch];
        if in_ptr.is_null() || out_ptr.is_null() {
            continue;
        }
        let in_buf = std::slice::from_raw_parts(in_ptr, frames);
        let out_buf = std::slice::from_raw_parts_mut(out_ptr, frames);
        for i in 0..frames {
            out_buf[i] = in_buf[i] * gain;
        }
    }
}

unsafe extern "C" fn plugin_process(
    plugin: *const clap_plugin_t,
    process: *const clap_process_t,
) -> clap_process_status {
    let effect = &mut *((*plugin).plugin_data as *mut GainEffectGui);
    let process = &*process;
    apply_param_events(effect, process.in_events);

    if process.audio_inputs.is_null()
        || process.audio_outputs.is_null()
        || process.audio_inputs_count == 0
        || process.audio_outputs_count == 0
    {
        return CLAP_PROCESS_CONTINUE;
    }
    let frames = process.frames_count as usize;
    let input = &*process.audio_inputs;
    let output = &mut *process.audio_outputs;

    if !output.data32.is_null() && !input.data32.is_null() {
        process_audio_f32(effect, input, output, frames);
    } else if !output.data64.is_null() && !input.data64.is_null() {
        process_audio_f64(effect, input, output, frames);
    }

    CLAP_PROCESS_CONTINUE
}

unsafe extern "C" fn plugin_get_extension(
    _plugin: *const clap_plugin_t,
    id: *const c_char,
) -> *const std::ffi::c_void {
    if id.is_null() {
        return ptr::null();
    }
    let id_cstr = CStr::from_ptr(id);
    if id_cstr.to_bytes_with_nul() == clap_sys::ext::audio_ports::CLAP_EXT_AUDIO_PORTS.as_bytes() {
        return &AUDIO_PORTS as *const _ as *const std::ffi::c_void;
    }
    if id_cstr.to_bytes_with_nul() == clap_sys::ext::params::CLAP_EXT_PARAMS.as_bytes() {
        return &PARAMS as *const _ as *const std::ffi::c_void;
    }
    if id_cstr.to_bytes_with_nul() == CLAP_EXT_STATE.as_bytes() {
        return &STATE as *const _ as *const std::ffi::c_void;
    }
    if id_cstr.to_bytes_with_nul() == CLAP_EXT_GUI.as_bytes() {
        return &GUI as *const _ as *const std::ffi::c_void;
    }
    ptr::null()
}

unsafe extern "C" fn plugin_on_main_thread(_plugin: *const clap_plugin_t) {}

unsafe extern "C" fn audio_ports_count(_plugin: *const clap_plugin_t, _is_input: bool) -> u32 {
    1
}

fn write_cstr_to_array(dst: &mut [c_char], bytes: &[u8]) {
    dst.fill(0);
    let max = dst.len().saturating_sub(1);
    let len = bytes.len().saturating_sub(1).min(max);
    for i in 0..len {
        dst[i] = bytes[i] as c_char;
    }
}

unsafe extern "C" fn audio_ports_get(
    _plugin: *const clap_plugin_t,
    index: u32,
    _is_input: bool,
    info: *mut clap_audio_port_info_t,
) -> bool {
    if index != 0 || info.is_null() {
        return false;
    }
    let info = &mut *info;
    info.id = 0;
    write_cstr_to_array(&mut info.name, b"Main\0");
    info.flags = CLAP_AUDIO_PORT_IS_MAIN;
    info.channel_count = 2;
    info.port_type = CLAP_PORT_STEREO.as_ptr() as *const c_char;
    info.in_place_pair = CLAP_INVALID_ID;
    true
}

static AUDIO_PORTS: clap_plugin_audio_ports_t = clap_plugin_audio_ports_t {
    count: Some(audio_ports_count),
    get: Some(audio_ports_get),
};

unsafe extern "C" fn params_count(_plugin: *const clap_plugin_t) -> u32 {
    1
}

unsafe extern "C" fn params_get_info(
    _plugin: *const clap_plugin_t,
    param_index: u32,
    param_info: *mut clap_param_info_t,
) -> bool {
    if param_index != 0 || param_info.is_null() {
        return false;
    }
    let info = &mut *param_info;
    info.id = PARAM_GAIN;
    info.flags = CLAP_PARAM_IS_AUTOMATABLE;
    info.cookie = ptr::null_mut();
    write_cstr_to_array(&mut info.name, b"Gain\0");
    info.module = [0; CLAP_PATH_SIZE];
    info.min_value = 0.0;
    info.max_value = 2.0;
    info.default_value = 1.0;
    true
}

unsafe extern "C" fn params_get_value(
    plugin: *const clap_plugin_t,
    param_id: clap_id,
    out_value: *mut f64,
) -> bool {
    if out_value.is_null() || param_id != PARAM_GAIN {
        return false;
    }
    let effect = &*((*plugin).plugin_data as *const GainEffectGui);
    *out_value = gain_from_bits(effect.gain_bits.load(Ordering::Relaxed));
    true
}

unsafe extern "C" fn params_value_to_text(
    _plugin: *const clap_plugin_t,
    param_id: clap_id,
    value: f64,
    out_buffer: *mut c_char,
    out_buffer_capacity: u32,
) -> bool {
    if param_id != PARAM_GAIN || out_buffer.is_null() || out_buffer_capacity == 0 {
        return false;
    }
    let text = format!("{:.3}", value);
    let bytes = text.as_bytes();
    let capacity = out_buffer_capacity as usize;
    let len = bytes.len().min(capacity.saturating_sub(1));
    let dst = std::slice::from_raw_parts_mut(out_buffer, capacity);
    dst.fill(0);
    for i in 0..len {
        dst[i] = bytes[i] as c_char;
    }
    true
}

unsafe extern "C" fn params_text_to_value(
    _plugin: *const clap_plugin_t,
    param_id: clap_id,
    param_value_text: *const c_char,
    out_value: *mut f64,
) -> bool {
    if param_id != PARAM_GAIN || param_value_text.is_null() || out_value.is_null() {
        return false;
    }
    let text = CStr::from_ptr(param_value_text);
    if let Ok(value_str) = text.to_str() {
        if let Ok(parsed) = value_str.parse::<f64>() {
            *out_value = parsed.clamp(0.0, 2.0);
            return true;
        }
    }
    false
}

unsafe extern "C" fn params_flush(
    plugin: *const clap_plugin_t,
    input: *const clap_input_events_t,
    _output: *const clap_sys::events::clap_output_events_t,
) {
    let effect = &mut *((*plugin).plugin_data as *mut GainEffectGui);
    apply_param_events(effect, input);
}

static PARAMS: clap_plugin_params_t = clap_plugin_params_t {
    count: Some(params_count),
    get_info: Some(params_get_info),
    get_value: Some(params_get_value),
    value_to_text: Some(params_value_to_text),
    text_to_value: Some(params_text_to_value),
    flush: Some(params_flush),
};

fn stream_write_all(stream: *const clap_ostream_t, mut buffer: *const u8, mut size: usize) -> bool {
    if stream.is_null() {
        return false;
    }
    let write_fn = unsafe { (*stream).write };
    let Some(write_fn) = write_fn else {
        return false;
    };
    while size > 0 {
        let written = unsafe { write_fn(stream, buffer as *const _, size as u64) };
        if written <= 0 {
            return false;
        }
        let written = written as usize;
        buffer = unsafe { buffer.add(written) };
        size -= written;
    }
    true
}

fn stream_read_exact(stream: *const clap_istream_t, mut buffer: *mut u8, mut size: usize) -> bool {
    if stream.is_null() {
        return false;
    }
    let read_fn = unsafe { (*stream).read };
    let Some(read_fn) = read_fn else { return false };
    while size > 0 {
        let read = unsafe { read_fn(stream, buffer as *mut _, size as u64) };
        if read <= 0 {
            return false;
        }
        let read = read as usize;
        buffer = unsafe { buffer.add(read) };
        size -= read;
    }
    true
}

unsafe extern "C" fn state_save(
    plugin: *const clap_plugin_t,
    stream: *const clap_ostream_t,
) -> bool {
    if plugin.is_null() {
        return false;
    }
    let effect = &*((*plugin).plugin_data as *const GainEffectGui);
    let gain = gain_from_bits(effect.gain_bits.load(Ordering::Relaxed)).to_le_bytes();
    stream_write_all(stream, gain.as_ptr(), gain.len())
}

unsafe extern "C" fn state_load(
    plugin: *const clap_plugin_t,
    stream: *const clap_istream_t,
) -> bool {
    if plugin.is_null() {
        return false;
    }
    let mut buffer = [0u8; 8];
    if !stream_read_exact(stream, buffer.as_mut_ptr(), buffer.len()) {
        return false;
    }
    let effect = &mut *((*plugin).plugin_data as *mut GainEffectGui);
    effect.gain_bits.store(
        f64::from_le_bytes(buffer).clamp(0.0, 2.0).to_bits(),
        Ordering::Relaxed,
    );
    true
}

static STATE: clap_plugin_state_t = clap_plugin_state_t {
    save: Some(state_save),
    load: Some(state_load),
};

unsafe extern "C" fn gui_is_api_supported(
    _plugin: *const clap_plugin_t,
    api: *const c_char,
    is_floating: bool,
) -> bool {
    if is_floating || api.is_null() {
        return false;
    }
    let api_cstr = CStr::from_ptr(api);
    api_cstr.to_bytes_with_nul() == CLAP_WINDOW_API_COCOA.as_bytes()
        || api_cstr.to_bytes_with_nul() == CLAP_WINDOW_API_WIN32.as_bytes()
        || api_cstr.to_bytes_with_nul() == CLAP_WINDOW_API_X11.as_bytes()
}

unsafe extern "C" fn gui_get_preferred_api(
    _plugin: *const clap_plugin_t,
    api: *mut *const c_char,
    is_floating: *mut bool,
) -> bool {
    if api.is_null() || is_floating.is_null() {
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        *api = CLAP_WINDOW_API_COCOA.as_ptr() as *const c_char;
    }
    #[cfg(target_os = "windows")]
    {
        *api = CLAP_WINDOW_API_WIN32.as_ptr() as *const c_char;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        *api = CLAP_WINDOW_API_X11.as_ptr() as *const c_char;
    }
    *is_floating = false;
    true
}

unsafe extern "C" fn gui_create(
    plugin: *const clap_plugin_t,
    _api: *const c_char,
    _is_floating: bool,
) -> bool {
    let effect = &mut *((*plugin).plugin_data as *mut GainEffectGui);
    effect.gui_open = true;
    if effect.gui_parent.is_some() {
        return open_gui_window(effect);
    }
    true
}

unsafe extern "C" fn gui_destroy(plugin: *const clap_plugin_t) {
    let effect = &mut *((*plugin).plugin_data as *mut GainEffectGui);
    if let Some(mut handle) = effect.gui_handle.take() {
        handle.close();
    }
    effect.gui_parent = None; // Clear parent so next cycle starts fresh
    effect.gui_open = false;
}

unsafe extern "C" fn gui_set_scale(_plugin: *const clap_plugin_t, _scale: f64) -> bool {
    true
}

unsafe extern "C" fn gui_get_size(
    _plugin: *const clap_plugin_t,
    width: *mut u32,
    height: *mut u32,
) -> bool {
    if width.is_null() || height.is_null() {
        return false;
    }
    *width = GUI_WIDTH as u32;
    *height = GUI_HEIGHT as u32;
    true
}

unsafe extern "C" fn gui_can_resize(_plugin: *const clap_plugin_t) -> bool {
    false
}

unsafe extern "C" fn gui_get_resize_hints(
    _plugin: *const clap_plugin_t,
    hints: *mut clap_gui_resize_hints_t,
) -> bool {
    if hints.is_null() {
        return false;
    }
    *hints = clap_gui_resize_hints_t {
        can_resize_horizontally: false,
        can_resize_vertically: false,
        preserve_aspect_ratio: true,
        aspect_ratio_width: GUI_WIDTH as u32,
        aspect_ratio_height: GUI_HEIGHT as u32,
    };
    true
}

unsafe extern "C" fn gui_adjust_size(
    _plugin: *const clap_plugin_t,
    width: *mut u32,
    height: *mut u32,
) -> bool {
    if width.is_null() || height.is_null() {
        return false;
    }
    *width = GUI_WIDTH as u32;
    *height = GUI_HEIGHT as u32;
    true
}

unsafe extern "C" fn gui_set_size(_plugin: *const clap_plugin_t, width: u32, height: u32) -> bool {
    width == GUI_WIDTH as u32 && height == GUI_HEIGHT as u32
}

unsafe extern "C" fn gui_set_parent(
    plugin: *const clap_plugin_t,
    window: *const clap_window_t,
) -> bool {
    if window.is_null() {
        return false;
    }
    let effect = &mut *((*plugin).plugin_data as *mut GainEffectGui);
    let handle = unsafe { raw_window_handle_from_clap(&*window) };
    effect.gui_parent = handle;
    if effect.gui_parent.is_some() && effect.gui_open {
        return open_gui_window(effect);
    }
    effect.gui_parent.is_some()
}

unsafe extern "C" fn gui_set_transient(
    _plugin: *const clap_plugin_t,
    _window: *const clap_window_t,
) -> bool {
    false
}

unsafe extern "C" fn gui_suggest_title(_plugin: *const clap_plugin_t, _title: *const c_char) {}

unsafe extern "C" fn gui_show(_plugin: *const clap_plugin_t) -> bool {
    let effect = &mut *((*_plugin).plugin_data as *mut GainEffectGui);
    effect.gui_open = true;

    if effect.gui_parent.is_some() {
        return open_gui_window(effect);
    }

    false
}

unsafe extern "C" fn gui_hide(_plugin: *const clap_plugin_t) -> bool {
    let effect = &mut *((*_plugin).plugin_data as *mut GainEffectGui);
    // Don't close the window on hide - just mark it as not open
    // The window will be closed on gui_destroy
    effect.gui_open = false;
    true
}

static GUI: clap_plugin_gui_t = clap_plugin_gui_t {
    is_api_supported: Some(gui_is_api_supported),
    get_preferred_api: Some(gui_get_preferred_api),
    create: Some(gui_create),
    destroy: Some(gui_destroy),
    set_scale: Some(gui_set_scale),
    get_size: Some(gui_get_size),
    can_resize: Some(gui_can_resize),
    get_resize_hints: Some(gui_get_resize_hints),
    adjust_size: Some(gui_adjust_size),
    set_size: Some(gui_set_size),
    set_parent: Some(gui_set_parent),
    set_transient: Some(gui_set_transient),
    suggest_title: Some(gui_suggest_title),
    show: Some(gui_show),
    hide: Some(gui_hide),
};

unsafe extern "C" fn get_plugin_count(_factory: *const clap_plugin_factory_t) -> u32 {
    1
}

unsafe extern "C" fn get_plugin_descriptor(
    _factory: *const clap_plugin_factory_t,
    index: u32,
) -> *const clap_plugin_descriptor_t {
    if index == 0 {
        &DESCRIPTOR.0
    } else {
        ptr::null()
    }
}

unsafe extern "C" fn create_plugin(
    _factory: *const clap_plugin_factory_t,
    host: *const clap_host_t,
    plugin_id: *const c_char,
) -> *const clap_plugin_t {
    if host.is_null() || plugin_id.is_null() {
        return ptr::null();
    }
    if !clap_version_is_compatible((*host).clap_version) {
        return ptr::null();
    }
    let id_cstr = CStr::from_ptr(plugin_id);
    let target_id = CStr::from_ptr(DESCRIPTOR.0.id);
    if id_cstr != target_id {
        return ptr::null();
    }
    let instance = Box::new(GainEffectGui {
        host,
        gain_bits: Arc::new(AtomicU64::new(1.0_f64.to_bits())),
        gui_parent: None,
        gui_handle: None,
        gui_open: false,
    });
    let plugin = Box::new(clap_plugin_t {
        desc: &DESCRIPTOR.0,
        plugin_data: Box::into_raw(instance) as *mut _,
        init: Some(plugin_init),
        destroy: Some(plugin_destroy),
        activate: Some(plugin_activate),
        deactivate: Some(plugin_deactivate),
        start_processing: Some(plugin_start_processing),
        stop_processing: Some(plugin_stop_processing),
        reset: Some(plugin_reset),
        process: Some(plugin_process),
        get_extension: Some(plugin_get_extension),
        on_main_thread: Some(plugin_on_main_thread),
    });
    Box::into_raw(plugin)
}

static PLUGIN_FACTORY: clap_plugin_factory_t = clap_plugin_factory_t {
    get_plugin_count: Some(get_plugin_count),
    get_plugin_descriptor: Some(get_plugin_descriptor),
    create_plugin: Some(create_plugin),
};

unsafe extern "C" fn entry_init(_plugin_path: *const c_char) -> bool {
    true
}

unsafe extern "C" fn entry_deinit() {}

unsafe extern "C" fn entry_get_factory(factory_id: *const c_char) -> *const std::ffi::c_void {
    if factory_id.is_null() {
        return ptr::null();
    }
    let id_cstr = CStr::from_ptr(factory_id);
    let factory_id_cstr = CStr::from_ptr(CLAP_PLUGIN_FACTORY_ID.as_ptr() as *const c_char);
    if id_cstr == factory_id_cstr {
        &PLUGIN_FACTORY as *const _ as *const std::ffi::c_void
    } else {
        ptr::null()
    }
}

#[unsafe(no_mangle)]
pub static clap_entry: clap_sys::entry::clap_plugin_entry_t =
    clap_sys::entry::clap_plugin_entry_t {
        clap_version: CLAP_VERSION,
        init: Some(entry_init),
        deinit: Some(entry_deinit),
        get_factory: Some(entry_get_factory),
    };
