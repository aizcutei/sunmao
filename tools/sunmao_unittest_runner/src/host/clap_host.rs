use super::{
    process_frame_count, validate_host_events, GuiGestureEvidence, HostEvent, HostPlugin,
    ParamInfo, PluginFormat, PluginInfo,
};
use crate::gui_window::PluginGuiWindow;
use clap_sys::audio_buffer::clap_audio_buffer_t;
use clap_sys::entry::clap_plugin_entry_t;
use clap_sys::events::*;
use clap_sys::ext::audio_ports::*;
use clap_sys::ext::gui::*;
use clap_sys::ext::note_ports::*;
use clap_sys::ext::params::*;
use clap_sys::ext::state::*;
use clap_sys::host::clap_host_t;
use clap_sys::plugin::{clap_plugin_descriptor_t, clap_plugin_t};
use clap_sys::process::clap_process_t;
use clap_sys::stream::{clap_istream_t, clap_ostream_t};
use clap_sys::version::CLAP_VERSION;
use std::ffi::{c_char, c_void, CString};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, AtomicUsize, Ordering};

#[derive(Default)]
struct ClapHostState {
    flush_requested: AtomicBool,
    gui_attached: AtomicBool,
    gui_window: AtomicPtr<PluginGuiWindow>,
    resize_count: AtomicUsize,
    resize_width: AtomicU32,
    resize_height: AtomicU32,
    output_event_count: AtomicUsize,
    output_event_type: AtomicU32,
    output_param_id: AtomicU32,
    output_param_value: AtomicU64,
    output_gesture_begin_count: AtomicUsize,
    output_param_value_count: AtomicUsize,
    output_gesture_end_count: AtomicUsize,
    gesture_active: AtomicBool,
    gesture_param_id: AtomicU32,
    gesture_has_value: AtomicBool,
    gesture_value: AtomicU64,
    completed_gesture_count: AtomicUsize,
    completed_gesture_param_id: AtomicU32,
    completed_gesture_value: AtomicU64,
}

pub struct ClapHostPlugin {
    info: PluginInfo,
    _lib: libloading::Library,
    plugin: *const clap_plugin_t,
    entry: *const clap_plugin_entry_t,
    host: *mut clap_host_t,
    _host_state: Box<ClapHostState>,
    // Keep the host alive
    _host_name: CString,
    _host_vendor: CString,
    _host_url: CString,
    _host_version: CString,
    // Audio buffers
    input_buffers: Vec<Vec<f32>>,
    output_buffers: Vec<Vec<f32>>,
    input_channel_ptrs: Vec<*mut f32>,
    output_channel_ptrs: Vec<*mut f32>,
    input_port_channels: Vec<usize>,
    output_port_channels: Vec<usize>,
    sample_rate: f64,
    max_frames: u32,
    active: bool,
    processing: bool,
    shut_down: bool,
}

unsafe impl Send for ClapHostPlugin {}

fn native_clap_gui_api() -> Option<&'static str> {
    #[cfg(target_os = "macos")]
    {
        Some(CLAP_WINDOW_API_COCOA)
    }
    #[cfg(target_os = "windows")]
    {
        Some(CLAP_WINDOW_API_WIN32)
    }
    #[cfg(target_os = "linux")]
    {
        Some(CLAP_WINDOW_API_X11)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

fn native_clap_window(parent: *mut c_void) -> Option<clap_window_t> {
    let api = native_clap_gui_api()?;
    #[cfg(target_os = "macos")]
    let handle = clap_window_handle_u { cocoa: parent };
    #[cfg(target_os = "windows")]
    let handle = clap_window_handle_u { win32: parent };
    #[cfg(target_os = "linux")]
    let handle = clap_window_handle_u {
        x11: parent as usize as clap_xwnd,
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    let handle = clap_window_handle_u {
        ptr: ptr::null_mut(),
    };

    Some(clap_window_t {
        api: api.as_ptr() as *const c_char,
        handle,
    })
}

impl ClapHostPlugin {
    /// Load a CLAP plugin from a .clap file by plugin ID.
    pub fn load(path: &str, plugin_id: &str) -> Result<Self, String> {
        unsafe {
            let dylib_path = super::scanner::find_clap_module(std::path::Path::new(path))
                .ok_or_else(|| format!("No CLAP module found in {}", path))?;

            let lib = super::load_plugin_library(&dylib_path)
                .map_err(|e| format!("Failed to load {}: {}", dylib_path.display(), e))?;

            let entry: libloading::Symbol<*const clap_sys::entry::clap_plugin_entry_t> = lib
                .get(b"clap_entry")
                .map_err(|e| format!("No clap_entry symbol: {}", e))?;
            let entry = &**entry;

            let init = entry.init.ok_or("clap_entry.init is null")?;
            let path_c = CString::new(path).unwrap();
            if !init(path_c.as_ptr()) {
                return Err("clap_entry.init() returned false".into());
            }

            let get_factory = entry.get_factory.ok_or("clap_entry.get_factory is null")?;
            let factory_id = CString::new("clap.plugin-factory").unwrap();
            let factory_ptr = get_factory(factory_id.as_ptr());
            if factory_ptr.is_null() {
                return Err("get_factory returned null".into());
            }

            let factory =
                &*(factory_ptr as *const clap_sys::factory::plugin_factory::clap_plugin_factory_t);
            let get_plugin_count = factory
                .get_plugin_count
                .ok_or("factory.get_plugin_count is null")?;
            let get_desc = factory
                .get_plugin_descriptor
                .ok_or("factory.get_plugin_descriptor is null")?;
            let create_plugin = factory
                .create_plugin
                .ok_or("factory.create_plugin is null")?;

            let count = get_plugin_count(factory);
            let mut target_desc: *const clap_plugin_descriptor_t = ptr::null();

            for i in 0..count {
                let desc = get_desc(factory, i);
                if desc.is_null() {
                    continue;
                }
                let id_str = cstr_from_ptr((*desc).id);
                if id_str == plugin_id {
                    target_desc = desc;
                    break;
                }
            }

            if target_desc.is_null() {
                return Err(format!("Plugin ID '{}' not found in {}", plugin_id, path));
            }

            // Create host struct
            let host_name = CString::new("SunMao Test Runner").unwrap();
            let host_vendor = CString::new("SunMao").unwrap();
            let host_url = CString::new("https://aizcutei.github.io/sunmao").unwrap();
            let host_version = CString::new("1.0.0").unwrap();
            let mut host_state = Box::new(ClapHostState::default());

            let host = Box::new(clap_host_t {
                clap_version: CLAP_VERSION,
                host_data: (&mut *host_state as *mut ClapHostState).cast(),
                name: host_name.as_ptr(),
                vendor: host_vendor.as_ptr(),
                url: host_url.as_ptr(),
                version: host_version.as_ptr(),
                get_extension: Some(host_get_extension),
                request_restart: Some(host_request_restart),
                request_process: Some(host_request_process),
                request_callback: Some(host_request_callback),
            });
            let host_ptr = Box::into_raw(host);

            let plugin_ptr = create_plugin(factory, host_ptr, (*target_desc).id);
            if plugin_ptr.is_null() {
                drop(Box::from_raw(host_ptr));
                return Err("create_plugin returned null".into());
            }

            let plugin = &*plugin_ptr;
            let init_fn = plugin.init.ok_or("plugin.init is null")?;
            if !init_fn(plugin_ptr) {
                plugin.destroy.map(|f| f(plugin_ptr));
                drop(Box::from_raw(host_ptr));
                return Err("plugin.init() returned false".into());
            }

            let desc = &*plugin.desc;
            let input_port_channels = audio_port_channels(plugin_ptr, true);
            let output_port_channels = audio_port_channels(plugin_ptr, false);
            let input_channels = input_port_channels.iter().sum::<usize>();
            let output_channels = output_port_channels.iter().sum::<usize>();
            let accepts_notes = note_input_port_count(plugin_ptr) > 0;
            let advertises_instrument = descriptor_has_feature(desc, "instrument")
                || descriptor_has_feature(desc, "synthesizer");
            let info = PluginInfo {
                name: cstr_from_ptr(desc.name),
                vendor: cstr_from_ptr(desc.vendor),
                version: cstr_from_ptr(desc.version),
                id: plugin_id.to_string(),
                path: path.to_string(),
                format: PluginFormat::CLAP,
                class_index: 0,
                input_channels: input_channels as u32,
                output_channels: output_channels as u32,
                is_synth: input_channels == 0
                    && output_channels > 0
                    && (accepts_notes || advertises_instrument),
            };

            Ok(Self {
                info,
                _lib: lib,
                plugin: plugin_ptr,
                entry,
                host: host_ptr,
                _host_state: host_state,
                _host_name: host_name,
                _host_vendor: host_vendor,
                _host_url: host_url,
                _host_version: host_version,
                input_buffers: vec![vec![0.0; 4096]; input_channels],
                output_buffers: vec![vec![0.0; 4096]; output_channels],
                input_channel_ptrs: vec![ptr::null_mut(); input_channels],
                output_channel_ptrs: vec![ptr::null_mut(); output_channels],
                input_port_channels,
                output_port_channels,
                sample_rate: 44100.0,
                max_frames: 4096,
                active: false,
                processing: false,
                shut_down: false,
            })
        }
    }

    fn flush_requested_parameter_events(&self) -> Result<(), String> {
        if self.plugin.is_null()
            || !self
                ._host_state
                .flush_requested
                .swap(false, Ordering::AcqRel)
        {
            return Ok(());
        }

        unsafe {
            let plugin = &*self.plugin;
            let get_extension = plugin.get_extension.ok_or("no get_extension")?;
            let params_ptr = get_extension(self.plugin, CLAP_EXT_PARAMS.as_ptr().cast());
            if params_ptr.is_null() {
                return Err("Plugin requested a parameter flush without clap.params".into());
            }
            let params = &*(params_ptr as *const clap_plugin_params_t);
            let flush = params.flush.ok_or("params.flush is null")?;
            let input = clap_input_events_t {
                ctx: ptr::null_mut(),
                size: Some(empty_events_size),
                get: Some(empty_events_get),
            };
            let output = clap_output_events_t {
                ctx: (&*self._host_state as *const ClapHostState)
                    .cast_mut()
                    .cast(),
                try_push: Some(output_events_try_push),
            };
            flush(self.plugin, &input, &output);
        }
        Ok(())
    }
}

impl HostPlugin for ClapHostPlugin {
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
            let plugin = &*self.plugin;
            let activate = plugin.activate.ok_or("plugin.activate is null")?;
            if !activate(self.plugin, sample_rate, 1, max_frames) {
                return Err("plugin.activate() failed".into());
            }
            self.active = true;

            let start = plugin
                .start_processing
                .ok_or("plugin.start_processing is null")?;
            if !start(self.plugin) {
                return Err("plugin.start_processing() failed".into());
            }
            self.processing = true;
        }
        Ok(())
    }

    fn param_count(&self) -> u32 {
        unsafe {
            let plugin = &*self.plugin;
            let Some(ext) = plugin.get_extension else {
                return 0;
            };
            let params_ptr = ext(self.plugin, b"clap.params\0".as_ptr() as *const c_char);
            if params_ptr.is_null() {
                return 0;
            }
            let params = &*(params_ptr as *const clap_plugin_params_t);
            params.count.map(|f| f(self.plugin)).unwrap_or(0)
        }
    }

    fn param_info(&self, index: u32) -> Option<ParamInfo> {
        unsafe {
            let plugin = &*self.plugin;
            let ext = plugin.get_extension?;
            let params_ptr = ext(self.plugin, b"clap.params\0".as_ptr() as *const c_char);
            if params_ptr.is_null() {
                return None;
            }
            let params = &*(params_ptr as *const clap_plugin_params_t);
            let get_info = params.get_info?;
            let mut info = std::mem::zeroed::<clap_param_info_t>();
            if get_info(self.plugin, index, &mut info) {
                Some(ParamInfo {
                    id: info.id,
                    name: cstr_from_char8(&info.name),
                    min: info.min_value,
                    max: info.max_value,
                    default: info.default_value,
                    is_stepped: (info.flags & CLAP_PARAM_IS_STEPPED) != 0,
                    can_automate: (info.flags & CLAP_PARAM_IS_AUTOMATABLE) != 0,
                })
            } else {
                None
            }
        }
    }

    fn param_get(&self, id: u32) -> Option<f64> {
        unsafe {
            let plugin = &*self.plugin;
            let ext = plugin.get_extension?;
            let params_ptr = ext(self.plugin, b"clap.params\0".as_ptr() as *const c_char);
            if params_ptr.is_null() {
                return None;
            }
            let params = &*(params_ptr as *const clap_plugin_params_t);
            let get_value = params.get_value?;
            let mut value: f64 = 0.0;
            if get_value(self.plugin, id, &mut value) {
                Some(value)
            } else {
                None
            }
        }
    }

    fn param_set(&mut self, id: u32, value: f64) -> Result<(), String> {
        // Use param flush to set parameter value
        unsafe {
            let plugin = &*self.plugin;
            let ext = plugin.get_extension.ok_or("no get_extension")?;
            let params_ptr = ext(self.plugin, b"clap.params\0".as_ptr() as *const c_char);
            if params_ptr.is_null() {
                return Err("Plugin has no params extension".into());
            }
            let params = &*(params_ptr as *const clap_plugin_params_t);
            let flush = params.flush.ok_or("params.flush is null")?;

            // Create a param value event
            let event = clap_event_param_value_t {
                header: clap_event_header_t {
                    size: std::mem::size_of::<clap_event_param_value_t>() as u32,
                    time: 0,
                    space_id: CLAP_CORE_EVENT_SPACE_ID,
                    type_: CLAP_EVENT_PARAM_VALUE,
                    flags: 0,
                },
                param_id: id,
                cookie: ptr::null_mut(),
                note_id: -1,
                port_index: 0,
                channel: -1,
                key: -1,
                value,
            };

            let events = clap_input_events_t {
                ctx: &event as *const _ as *mut c_void,
                size: Some(input_events_size),
                get: Some(input_events_get),
            };

            let out_events = clap_output_events_t {
                ctx: (&*self._host_state as *const ClapHostState)
                    .cast_mut()
                    .cast(),
                try_push: Some(output_events_try_push),
            };

            flush(self.plugin, &events, &out_events);
        }
        Ok(())
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
        self.flush_requested_parameter_events()?;
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
        let input_buses: Vec<clap_audio_buffer_t> = self
            .input_port_channels
            .iter()
            .map(|&channel_count| {
                let data32 = if channel_count == 0 {
                    ptr::null_mut()
                } else {
                    unsafe {
                        self.input_channel_ptrs
                            .as_mut_ptr()
                            .add(input_channel_offset)
                    }
                };
                input_channel_offset += channel_count;
                clap_audio_buffer_t {
                    data32,
                    data64: ptr::null_mut(),
                    channel_count: channel_count as u32,
                    latency: 0,
                    constant_mask: 0,
                }
            })
            .collect();

        let mut output_channel_offset = 0usize;
        let mut output_buses: Vec<clap_audio_buffer_t> = self
            .output_port_channels
            .iter()
            .map(|&channel_count| {
                let data32 = if channel_count == 0 {
                    ptr::null_mut()
                } else {
                    unsafe {
                        self.output_channel_ptrs
                            .as_mut_ptr()
                            .add(output_channel_offset)
                    }
                };
                output_channel_offset += channel_count;
                clap_audio_buffer_t {
                    data32,
                    data64: ptr::null_mut(),
                    channel_count: channel_count as u32,
                    latency: 0,
                    constant_mask: 0,
                }
            })
            .collect();

        let event_list = ClapInputEventList::new(events);
        let in_events = event_list.raw();
        let out_events = clap_output_events_t {
            ctx: (&*self._host_state as *const ClapHostState)
                .cast_mut()
                .cast(),
            try_push: Some(output_events_try_push),
        };

        unsafe {
            let process = clap_process_t {
                steady_time: 0,
                frames_count: frames as u32,
                transport: ptr::null(),
                audio_inputs: if input_buses.is_empty() {
                    ptr::null()
                } else {
                    input_buses.as_ptr()
                },
                audio_outputs: if output_buses.is_empty() {
                    ptr::null_mut()
                } else {
                    output_buses.as_mut_ptr()
                },
                audio_inputs_count: input_buses.len() as u32,
                audio_outputs_count: output_buses.len() as u32,
                in_events: &in_events,
                out_events: &out_events,
            };

            let plugin = &*self.plugin;
            let process_fn = plugin.process.ok_or("plugin.process is null")?;
            let status = process_fn(self.plugin, &process);
            if status == clap_sys::process::CLAP_PROCESS_ERROR {
                return Err("plugin.process() returned error".into());
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
            let plugin = &*self.plugin;
            if let Some(reset) = plugin.reset {
                reset(self.plugin);
            }
        }
        Ok(())
    }

    fn save_state(&mut self) -> Result<Vec<u8>, String> {
        unsafe {
            let plugin = &*self.plugin;
            let ext = plugin.get_extension.ok_or("no get_extension")?;
            let state_ptr = ext(self.plugin, b"clap.state\0".as_ptr() as *const c_char);
            if state_ptr.is_null() {
                return Err("Plugin has no state extension".into());
            }
            let state = &*(state_ptr as *const clap_plugin_state_t);
            let save = state.save.ok_or("state.save is null")?;

            let mut data = Vec::new();
            let stream = clap_ostream_t {
                ctx: &mut data as *mut Vec<u8> as *mut c_void,
                write: Some(stream_write),
            };

            if !save(self.plugin, &stream) {
                return Err("state.save() failed".into());
            }
            Ok(data)
        }
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), String> {
        unsafe {
            let plugin = &*self.plugin;
            let ext = plugin.get_extension.ok_or("no get_extension")?;
            let state_ptr = ext(self.plugin, b"clap.state\0".as_ptr() as *const c_char);
            if state_ptr.is_null() {
                return Err("Plugin has no state extension".into());
            }
            let state = &*(state_ptr as *const clap_plugin_state_t);
            let load = state.load.ok_or("state.load is null")?;

            let mut cursor = std::io::Cursor::new(data);
            let stream = clap_istream_t {
                ctx: &mut cursor as *mut std::io::Cursor<&[u8]> as *mut c_void,
                read: Some(stream_read),
            };

            if !load(self.plugin, &stream) {
                return Err("state.load() failed".into());
            }
            Ok(())
        }
    }

    fn shutdown(&mut self) {
        if self.shut_down {
            return;
        }
        if self._host_state.gui_attached.load(Ordering::Acquire) {
            self.close_gui();
        }
        unsafe {
            if !self.plugin.is_null() {
                let plugin = &*self.plugin;
                if self.processing {
                    if let Some(stop) = plugin.stop_processing {
                        stop(self.plugin);
                    }
                    self.processing = false;
                }
                if self.active {
                    if let Some(deactivate) = plugin.deactivate {
                        deactivate(self.plugin);
                    }
                    self.active = false;
                }
                if let Some(destroy) = plugin.destroy {
                    destroy(self.plugin);
                }
                self.plugin = ptr::null();
            }
            if !self.host.is_null() {
                drop(Box::from_raw(self.host));
                self.host = ptr::null_mut();
            }
            if !self.entry.is_null() {
                if let Some(deinit) = (*self.entry).deinit {
                    deinit();
                }
            }
        }
        self.shut_down = true;
    }

    fn service_host_requests(&mut self) -> Result<(), String> {
        self.flush_requested_parameter_events()
    }

    fn gui_gesture_evidence(&self) -> Option<GuiGestureEvidence> {
        Some(GuiGestureEvidence {
            begin_count: self
                ._host_state
                .output_gesture_begin_count
                .load(Ordering::Acquire),
            value_count: self
                ._host_state
                .output_param_value_count
                .load(Ordering::Acquire),
            end_count: self
                ._host_state
                .output_gesture_end_count
                .load(Ordering::Acquire),
            last_param_id: self._host_state.output_param_id.load(Ordering::Acquire),
            last_value: f64::from_bits(self._host_state.output_param_value.load(Ordering::Acquire)),
            completed_count: self
                ._host_state
                .completed_gesture_count
                .load(Ordering::Acquire),
            last_completed_param_id: self
                ._host_state
                .completed_gesture_param_id
                .load(Ordering::Acquire),
            last_completed_value: f64::from_bits(
                self._host_state
                    .completed_gesture_value
                    .load(Ordering::Acquire),
            ),
        })
    }

    fn open_gui(&mut self, window: &PluginGuiWindow) -> Result<(), String> {
        unsafe {
            let plugin = &*self.plugin;
            let ext = plugin.get_extension.ok_or("no get_extension")?;
            let gui_ptr = ext(self.plugin, b"clap.gui\0".as_ptr() as *const c_char);
            if gui_ptr.is_null() {
                return Err("Plugin has no GUI extension".into());
            }
            let gui = &*(gui_ptr as *const clap_plugin_gui_t);

            let create = gui.create.ok_or("gui.create is null")?;
            let set_parent = gui.set_parent.ok_or("gui.set_parent is null")?;
            let show = gui.show.ok_or("gui.show is null")?;

            let api = native_clap_gui_api().ok_or("No native CLAP GUI API for this platform")?;
            let is_api_supported = gui.is_api_supported.ok_or("gui.is_api_supported is null")?;
            if !is_api_supported(self.plugin, api.as_ptr() as *const c_char, false) {
                return Err(format!(
                    "Plugin does not support the native '{}' CLAP GUI API",
                    api.trim_end_matches('\0')
                ));
            }
            if !create(self.plugin, api.as_ptr() as *const c_char, false) {
                return Err("gui.create() returned false".into());
            }
            self._host_state.gui_window.store(
                (window as *const PluginGuiWindow).cast_mut(),
                Ordering::Release,
            );
            self._host_state.gui_attached.store(true, Ordering::Release);

            // Get preferred size
            let mut width: u32 = 400;
            let mut height: u32 = 300;
            if let Some(get_size) = gui.get_size {
                get_size(self.plugin, &mut width, &mut height);
            }

            // Resize window to plugin size
            window.set_content_view_size(width as f64, height as f64);

            // Set parent
            let clap_window = native_clap_window(window.content_view())
                .ok_or("No native CLAP parent window for this platform")?;
            if !set_parent(self.plugin, &clap_window) {
                // Clean up
                if let Some(destroy) = gui.destroy {
                    destroy(self.plugin);
                }
                self._host_state
                    .gui_attached
                    .store(false, Ordering::Release);
                self._host_state
                    .gui_window
                    .store(ptr::null_mut(), Ordering::Release);
                return Err("gui.set_parent() returned false".into());
            }

            // Show
            if !show(self.plugin) {
                // Clean up
                if let Some(destroy) = gui.destroy {
                    destroy(self.plugin);
                }
                self._host_state
                    .gui_attached
                    .store(false, Ordering::Release);
                self._host_state
                    .gui_window
                    .store(ptr::null_mut(), Ordering::Release);
                return Err("gui.show() returned false".into());
            }

            Ok(())
        }
    }

    fn resize_gui(&mut self, width: u32, height: u32) -> Result<(u32, u32), String> {
        if width == 0 || height == 0 {
            return Err("CLAP GUI size must be positive".into());
        }
        if !self._host_state.gui_attached.load(Ordering::Acquire) {
            return Err("CLAP GUI is not attached".into());
        }

        unsafe {
            let plugin = &*self.plugin;
            let extension = plugin.get_extension.ok_or("no get_extension")?;
            let gui_ptr = extension(self.plugin, b"clap.gui\0".as_ptr().cast());
            if gui_ptr.is_null() {
                return Err("Plugin has no GUI extension".into());
            }
            let gui = &*(gui_ptr as *const clap_plugin_gui_t);
            let can_resize = gui.can_resize.ok_or("gui.can_resize is null")?;
            if !can_resize(self.plugin) {
                return Err("CLAP plugin reports a fixed-size GUI".into());
            }

            let mut adjusted_width = width;
            let mut adjusted_height = height;
            if let Some(adjust_size) = gui.adjust_size {
                if !adjust_size(self.plugin, &mut adjusted_width, &mut adjusted_height) {
                    return Err("gui.adjust_size() returned false".into());
                }
            }
            if adjusted_width == 0 || adjusted_height == 0 {
                return Err("CLAP plugin adjusted the GUI to an empty size".into());
            }
            let window = self._host_state.gui_window.load(Ordering::Acquire);
            if window.is_null() {
                return Err("CLAP GUI host window is unavailable".into());
            }
            // Resize the host container first. Embedded native views may have
            // autoresizing enabled; resizing the child first would apply the
            // parent delta a second time when the container changes.
            (&*window).set_content_view_size(adjusted_width.into(), adjusted_height.into());
            let set_size = gui.set_size.ok_or("gui.set_size is null")?;
            if !set_size(self.plugin, adjusted_width, adjusted_height) {
                return Err("gui.set_size() returned false".into());
            }
            Ok((adjusted_width, adjusted_height))
        }
    }

    fn close_gui(&mut self) {
        let _ = self.flush_requested_parameter_events();
        unsafe {
            let plugin = &*self.plugin;
            if let Some(ext) = plugin.get_extension {
                let gui_ptr = ext(self.plugin, b"clap.gui\0".as_ptr() as *const c_char);
                if !gui_ptr.is_null() {
                    let gui = &*(gui_ptr as *const clap_plugin_gui_t);
                    if let Some(hide) = gui.hide {
                        hide(self.plugin);
                    }
                    if let Some(destroy) = gui.destroy {
                        destroy(self.plugin);
                    }
                }
            }
        }
        self._host_state
            .gui_attached
            .store(false, Ordering::Release);
        self._host_state
            .gui_window
            .store(ptr::null_mut(), Ordering::Release);
    }
}

impl Drop for ClapHostPlugin {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct ClapInputEventList {
    events: Vec<ClapInputEvent>,
}

enum ClapInputEvent {
    Note(clap_event_note_t),
    ParamValue(clap_event_param_value_t),
}

impl ClapInputEvent {
    fn header(&self) -> &clap_event_header_t {
        match self {
            Self::Note(event) => &event.header,
            Self::ParamValue(event) => &event.header,
        }
    }
}

impl ClapInputEventList {
    fn new(events: &[HostEvent]) -> Self {
        let events = events
            .iter()
            .copied()
            .map(|event| match event {
                HostEvent::NoteOn {
                    sample_offset,
                    channel,
                    pitch,
                    velocity,
                } => ClapInputEvent::Note(clap_event_note_t {
                    header: clap_event_header_t {
                        size: std::mem::size_of::<clap_event_note_t>() as u32,
                        time: sample_offset,
                        space_id: CLAP_CORE_EVENT_SPACE_ID,
                        type_: CLAP_EVENT_NOTE_ON,
                        flags: CLAP_EVENT_IS_LIVE,
                    },
                    note_id: -1,
                    port_index: 0,
                    channel: channel as i16,
                    key: pitch as i16,
                    velocity: velocity as f64,
                }),
                HostEvent::NoteOff {
                    sample_offset,
                    channel,
                    pitch,
                    velocity,
                } => ClapInputEvent::Note(clap_event_note_t {
                    header: clap_event_header_t {
                        size: std::mem::size_of::<clap_event_note_t>() as u32,
                        time: sample_offset,
                        space_id: CLAP_CORE_EVENT_SPACE_ID,
                        type_: CLAP_EVENT_NOTE_OFF,
                        flags: CLAP_EVENT_IS_LIVE,
                    },
                    note_id: -1,
                    port_index: 0,
                    channel: channel as i16,
                    key: pitch as i16,
                    velocity: velocity as f64,
                }),
                HostEvent::ParamValue {
                    sample_offset,
                    id,
                    value,
                } => ClapInputEvent::ParamValue(clap_event_param_value_t {
                    header: clap_event_header_t {
                        size: std::mem::size_of::<clap_event_param_value_t>() as u32,
                        time: sample_offset,
                        space_id: CLAP_CORE_EVENT_SPACE_ID,
                        type_: CLAP_EVENT_PARAM_VALUE,
                        flags: CLAP_EVENT_IS_LIVE,
                    },
                    param_id: id,
                    cookie: ptr::null_mut(),
                    note_id: -1,
                    port_index: -1,
                    channel: -1,
                    key: -1,
                    value,
                }),
            })
            .collect();
        Self { events }
    }

    fn raw(&self) -> clap_input_events_t {
        clap_input_events_t {
            ctx: self as *const Self as *mut c_void,
            size: Some(process_events_size),
            get: Some(process_events_get),
        }
    }
}

unsafe fn audio_port_channels(plugin: *const clap_plugin_t, is_input: bool) -> Vec<usize> {
    let Some(get_extension) = (*plugin).get_extension else {
        return Vec::new();
    };
    let extension = get_extension(plugin, CLAP_EXT_AUDIO_PORTS.as_ptr() as *const c_char);
    if extension.is_null() {
        return Vec::new();
    }
    let ports = &*(extension as *const clap_plugin_audio_ports_t);
    let (Some(count), Some(get)) = (ports.count, ports.get) else {
        return Vec::new();
    };

    (0..count(plugin, is_input))
        .filter_map(|index| {
            let mut info = std::mem::zeroed::<clap_audio_port_info_t>();
            get(plugin, index, is_input, &mut info).then_some(info.channel_count as usize)
        })
        .collect()
}

unsafe fn note_input_port_count(plugin: *const clap_plugin_t) -> u32 {
    let Some(get_extension) = (*plugin).get_extension else {
        return 0;
    };
    let extension = get_extension(plugin, CLAP_EXT_NOTE_PORTS.as_ptr() as *const c_char);
    if extension.is_null() {
        return 0;
    }
    let ports = &*(extension as *const clap_plugin_note_ports_t);
    ports.count.map(|count| count(plugin, true)).unwrap_or(0)
}

unsafe fn descriptor_has_feature(desc: &clap_plugin_descriptor_t, expected: &str) -> bool {
    if desc.features.is_null() {
        return false;
    }
    let mut feature = desc.features;
    while !(*feature).is_null() {
        if std::ffi::CStr::from_ptr(*feature).to_bytes() == expected.as_bytes() {
            return true;
        }
        feature = feature.add(1);
    }
    false
}

// ---- Callback implementations ----

unsafe extern "C" fn host_get_extension(
    _host: *const clap_host_t,
    extension_id: *const c_char,
) -> *const c_void {
    if extension_id.is_null() {
        return ptr::null();
    }
    match std::ffi::CStr::from_ptr(extension_id).to_bytes_with_nul() {
        bytes if bytes == CLAP_EXT_PARAMS.as_bytes() => {
            (&HOST_PARAMS as *const clap_host_params_t).cast()
        }
        bytes if bytes == CLAP_EXT_GUI.as_bytes() => (&HOST_GUI as *const clap_host_gui_t).cast(),
        _ => ptr::null(),
    }
}

unsafe extern "C" fn host_request_restart(_host: *const clap_host_t) {}
unsafe extern "C" fn host_request_process(_host: *const clap_host_t) {}
unsafe extern "C" fn host_request_callback(_host: *const clap_host_t) {}

unsafe fn host_state<'a>(host: *const clap_host_t) -> Option<&'a ClapHostState> {
    if host.is_null() || (*host).host_data.is_null() {
        None
    } else {
        Some(&*((*host).host_data as *const ClapHostState))
    }
}

unsafe extern "C" fn host_params_request_flush(host: *const clap_host_t) {
    if let Some(state) = host_state(host) {
        state.flush_requested.store(true, Ordering::Release);
    }
}

unsafe extern "C" fn host_gui_request_resize(
    host: *const clap_host_t,
    width: u32,
    height: u32,
) -> bool {
    let Some(state) = host_state(host) else {
        return false;
    };
    if !state.gui_attached.load(Ordering::Acquire) {
        return false;
    }
    state.resize_width.store(width, Ordering::Release);
    state.resize_height.store(height, Ordering::Release);
    state.resize_count.fetch_add(1, Ordering::AcqRel);
    let window = state.gui_window.load(Ordering::Acquire);
    if !window.is_null() {
        (&*window).set_content_view_size(width as f64, height as f64);
    }
    true
}

static HOST_PARAMS: clap_host_params_t = clap_host_params_t {
    rescan: None,
    clear: None,
    request_flush: Some(host_params_request_flush),
};

static HOST_GUI: clap_host_gui_t = clap_host_gui_t {
    resize_hints_changed: None,
    request_resize: Some(host_gui_request_resize),
    request_show: None,
    request_hide: None,
    closed: None,
};

// Input events (for parameter setting)
unsafe extern "C" fn input_events_size(_list: *const clap_input_events_t) -> u32 {
    // Single event
    1
}

unsafe extern "C" fn input_events_get(
    list: *const clap_input_events_t,
    index: u32,
) -> *const clap_event_header_t {
    if index == 0 {
        (*list).ctx as *const clap_event_header_t
    } else {
        ptr::null()
    }
}

unsafe extern "C" fn empty_events_size(_list: *const clap_input_events_t) -> u32 {
    0
}

unsafe extern "C" fn empty_events_get(
    _list: *const clap_input_events_t,
    _index: u32,
) -> *const clap_event_header_t {
    ptr::null()
}

unsafe extern "C" fn process_events_size(list: *const clap_input_events_t) -> u32 {
    if list.is_null() || (*list).ctx.is_null() {
        return 0;
    }
    let events = &*((*list).ctx as *const ClapInputEventList);
    events.events.len() as u32
}

unsafe extern "C" fn process_events_get(
    list: *const clap_input_events_t,
    index: u32,
) -> *const clap_event_header_t {
    if list.is_null() || (*list).ctx.is_null() {
        return ptr::null();
    }
    let events = &*((*list).ctx as *const ClapInputEventList);
    events
        .events
        .get(index as usize)
        .map(|event| event.header() as *const clap_event_header_t)
        .unwrap_or(ptr::null())
}

unsafe extern "C" fn output_events_try_push(
    list: *const clap_output_events_t,
    event: *const clap_event_header_t,
) -> bool {
    if list.is_null() || event.is_null() || (*list).ctx.is_null() {
        return false;
    }
    let state = &*((*list).ctx as *const ClapHostState);
    state.output_event_count.fetch_add(1, Ordering::AcqRel);
    state
        .output_event_type
        .store((*event).type_ as u32, Ordering::Release);
    if (*event).space_id != CLAP_CORE_EVENT_SPACE_ID {
        return true;
    }
    match (*event).type_ {
        CLAP_EVENT_PARAM_GESTURE_BEGIN | CLAP_EVENT_PARAM_GESTURE_END
            if (*event).size >= std::mem::size_of::<clap_event_param_gesture_t>() as u32 =>
        {
            let gesture = &*(event as *const clap_event_param_gesture_t);
            state
                .output_param_id
                .store(gesture.param_id, Ordering::Release);
            if (*event).type_ == CLAP_EVENT_PARAM_GESTURE_BEGIN {
                state
                    .gesture_param_id
                    .store(gesture.param_id, Ordering::Release);
                state.gesture_has_value.store(false, Ordering::Release);
                state.gesture_active.store(true, Ordering::Release);
                state
                    .output_gesture_begin_count
                    .fetch_add(1, Ordering::AcqRel);
            } else {
                state
                    .output_gesture_end_count
                    .fetch_add(1, Ordering::AcqRel);
                let completed = state.gesture_active.load(Ordering::Acquire)
                    && state.gesture_param_id.load(Ordering::Acquire) == gesture.param_id
                    && state.gesture_has_value.load(Ordering::Acquire);
                state.gesture_active.store(false, Ordering::Release);
                if completed {
                    state
                        .completed_gesture_param_id
                        .store(gesture.param_id, Ordering::Release);
                    state.completed_gesture_value.store(
                        state.gesture_value.load(Ordering::Acquire),
                        Ordering::Release,
                    );
                    state.completed_gesture_count.fetch_add(1, Ordering::AcqRel);
                }
            }
        }
        CLAP_EVENT_PARAM_VALUE
            if (*event).size >= std::mem::size_of::<clap_event_param_value_t>() as u32 =>
        {
            let value = &*(event as *const clap_event_param_value_t);
            state
                .output_param_id
                .store(value.param_id, Ordering::Release);
            state
                .output_param_value
                .store(value.value.to_bits(), Ordering::Release);
            state
                .output_param_value_count
                .fetch_add(1, Ordering::AcqRel);
            if state.gesture_active.load(Ordering::Acquire)
                && state.gesture_param_id.load(Ordering::Acquire) == value.param_id
            {
                state
                    .gesture_value
                    .store(value.value.to_bits(), Ordering::Release);
                state.gesture_has_value.store(true, Ordering::Release);
            }
        }
        _ => {}
    }
    true
}

// Stream callbacks for state save/load
unsafe extern "C" fn stream_write(
    stream: *const clap_ostream_t,
    buffer: *const c_void,
    size: u64,
) -> i64 {
    let data = &mut *((*stream).ctx as *mut Vec<u8>);
    let slice = std::slice::from_raw_parts(buffer as *const u8, size as usize);
    data.extend_from_slice(slice);
    size as i64
}

unsafe extern "C" fn stream_read(
    stream: *const clap_istream_t,
    buffer: *mut c_void,
    size: u64,
) -> i64 {
    let cursor = &mut *((*stream).ctx as *mut std::io::Cursor<&[u8]>);
    let slice = std::slice::from_raw_parts_mut(buffer as *mut u8, size as usize);
    match std::io::Read::read(cursor, slice) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

fn cstr_from_ptr(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_str()
        .unwrap_or("")
        .to_string()
}

fn cstr_from_char8(buf: &[c_char]) -> String {
    let bytes: Vec<u8> = buf.iter().map(|&b| b as u8).collect();
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn callback_test_host(state: &mut ClapHostState) -> clap_host_t {
        clap_host_t {
            clap_version: CLAP_VERSION,
            host_data: (state as *mut ClapHostState).cast(),
            name: ptr::null(),
            vendor: ptr::null(),
            url: ptr::null(),
            version: ptr::null(),
            get_extension: Some(host_get_extension),
            request_restart: None,
            request_process: None,
            request_callback: None,
        }
    }

    #[test]
    fn host_extensions_accept_flush_resize_and_parameter_output() {
        let mut state = ClapHostState::default();
        let host = callback_test_host(&mut state);

        unsafe {
            let params = host_get_extension(&host, CLAP_EXT_PARAMS.as_ptr().cast())
                as *const clap_host_params_t;
            let gui =
                host_get_extension(&host, CLAP_EXT_GUI.as_ptr().cast()) as *const clap_host_gui_t;
            assert!(!params.is_null());
            assert!(!gui.is_null());

            ((*params).request_flush.unwrap())(&host);
            assert!(state.flush_requested.load(Ordering::Acquire));

            assert!(!((*gui).request_resize.unwrap())(&host, 640, 360));
            state.gui_attached.store(true, Ordering::Release);
            assert!(((*gui).request_resize.unwrap())(&host, 960, 540));
            assert_eq!(state.resize_count.load(Ordering::Acquire), 1);
            assert_eq!(state.resize_width.load(Ordering::Acquire), 960);
            assert_eq!(state.resize_height.load(Ordering::Acquire), 540);

            let value = clap_event_param_value_t {
                header: clap_event_header_t {
                    size: std::mem::size_of::<clap_event_param_value_t>() as u32,
                    time: 0,
                    space_id: CLAP_CORE_EVENT_SPACE_ID,
                    type_: CLAP_EVENT_PARAM_VALUE,
                    flags: 0,
                },
                param_id: 42,
                cookie: ptr::null_mut(),
                note_id: -1,
                port_index: -1,
                channel: -1,
                key: -1,
                value: 0.75,
            };
            let output = clap_output_events_t {
                ctx: (&mut state as *mut ClapHostState).cast(),
                try_push: Some(output_events_try_push),
            };
            assert!(output_events_try_push(&output, &value.header));
            assert_eq!(state.output_event_count.load(Ordering::Acquire), 1);
            assert_eq!(
                state.output_event_type.load(Ordering::Acquire),
                CLAP_EVENT_PARAM_VALUE as u32
            );
            assert_eq!(state.output_param_id.load(Ordering::Acquire), 42);
            assert_eq!(
                f64::from_bits(state.output_param_value.load(Ordering::Acquire)),
                0.75
            );

            let gesture = |type_| clap_event_param_gesture_t {
                header: clap_event_header_t {
                    size: std::mem::size_of::<clap_event_param_gesture_t>() as u32,
                    time: 0,
                    space_id: CLAP_CORE_EVENT_SPACE_ID,
                    type_,
                    flags: 0,
                },
                param_id: 42,
            };
            let begin = gesture(CLAP_EVENT_PARAM_GESTURE_BEGIN);
            let end = gesture(CLAP_EVENT_PARAM_GESTURE_END);
            assert!(output_events_try_push(&output, &begin.header));
            assert!(output_events_try_push(&output, &value.header));
            assert!(output_events_try_push(&output, &end.header));
            assert_eq!(state.completed_gesture_count.load(Ordering::Acquire), 1);
            assert_eq!(state.completed_gesture_param_id.load(Ordering::Acquire), 42);
            assert_eq!(
                f64::from_bits(state.completed_gesture_value.load(Ordering::Acquire)),
                0.75
            );

            let foreign_value = clap_event_header_t {
                size: std::mem::size_of::<clap_event_param_value_t>() as u32,
                time: 0,
                space_id: CLAP_CORE_EVENT_SPACE_ID.wrapping_add(1),
                type_: CLAP_EVENT_PARAM_VALUE,
                flags: 0,
            };
            assert!(output_events_try_push(&output, &foreign_value));
            assert_eq!(state.output_param_value_count.load(Ordering::Acquire), 2);
        }
    }

    #[test]
    fn native_gui_window_uses_the_platform_api_and_handle() {
        let parent = 0x1234usize as *mut c_void;
        let window = native_clap_window(parent).expect("supported test platform");
        let api = unsafe { std::ffi::CStr::from_ptr(window.api) };

        #[cfg(target_os = "macos")]
        unsafe {
            assert_eq!(api.to_bytes(), b"cocoa");
            assert_eq!(window.handle.cocoa, parent);
        }
        #[cfg(target_os = "windows")]
        unsafe {
            assert_eq!(api.to_bytes(), b"win32");
            assert_eq!(window.handle.win32, parent);
        }
        #[cfg(target_os = "linux")]
        unsafe {
            assert_eq!(api.to_bytes(), b"x11");
            assert_eq!(window.handle.x11 as usize, parent as usize);
        }
    }

    #[test]
    fn native_note_events_keep_type_and_sample_offset() {
        let list = ClapInputEventList::new(&[
            HostEvent::NoteOn {
                sample_offset: 17,
                channel: 2,
                pitch: 64,
                velocity: 0.75,
            },
            HostEvent::NoteOff {
                sample_offset: 31,
                channel: 2,
                pitch: 64,
                velocity: 0.25,
            },
        ]);
        let raw = list.raw();

        unsafe {
            assert_eq!(process_events_size(&raw), 2);
            let on = &*(process_events_get(&raw, 0) as *const clap_event_note_t);
            let off = &*(process_events_get(&raw, 1) as *const clap_event_note_t);
            assert_eq!(on.header.type_, CLAP_EVENT_NOTE_ON);
            assert_eq!(on.header.time, 17);
            assert_eq!(on.channel, 2);
            assert_eq!(on.key, 64);
            assert_eq!(off.header.type_, CLAP_EVENT_NOTE_OFF);
            assert_eq!(off.header.time, 31);
            assert!(process_events_get(&raw, 2).is_null());
        }
    }

    #[test]
    fn native_parameter_events_keep_id_value_order_and_sample_offset() {
        let list = ClapInputEventList::new(&[
            HostEvent::ParamValue {
                sample_offset: 17,
                id: 42,
                value: 0.25,
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
        let raw = list.raw();

        unsafe {
            assert_eq!(process_events_size(&raw), 3);
            let first = &*(process_events_get(&raw, 0) as *const clap_event_param_value_t);
            let second = &*(process_events_get(&raw, 1) as *const clap_event_param_value_t);
            let last = &*(process_events_get(&raw, 2) as *const clap_event_param_value_t);
            assert_eq!(first.header.type_, CLAP_EVENT_PARAM_VALUE);
            assert_eq!(
                (first.header.time, first.param_id, first.value),
                (17, 42, 0.25)
            );
            assert_eq!(
                (second.header.time, second.param_id, second.value),
                (31, 42, 0.75)
            );
            assert_eq!((last.header.time, last.param_id, last.value), (31, 42, 0.5));
        }
    }
}
