#![allow(unsafe_op_in_unsafe_fn)]

pub use au_sys::{
    AudioComponentDescription, AudioComponentPlugInInterface, AudioUnitCocoaViewInfo, BufferList,
    NSPoint, NSRect, NSSize, ParameterInfo, ParameterUnit, export_au_component, fourcc,
    gl_get_proc_address,
};
pub use au_sys::{AudioUnitGetParameter, AudioUnitSetParameter, kAudioUnitScope_Global};
use au_sys::{get_parameter_direct, set_parameter_direct};
use std::ffi::c_void;

pub struct PluginInfo {
    pub name: &'static str,
    pub component_type: u32,
    pub component_subtype: u32,
    pub manufacturer: u32,
    pub version: u32,
    pub flags: u32,
    pub flags_mask: u32,
    pub input_channels: i16,
    pub output_channels: i16,
    pub supports_midi: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TransportInfo {
    pub tempo: Option<f64>,
    pub is_playing: Option<bool>,
    pub sample_pos: Option<i64>,
}

pub fn current_transport() -> TransportInfo {
    let Some(callbacks) = au_sys::current_host_callbacks() else {
        return TransportInfo::default();
    };

    let mut tempo = None;
    if let Some(cb) = callbacks.beatAndTempoProc {
        let mut tempo_val: f64 = 0.0;
        let status = unsafe { cb(callbacks.hostUserData, std::ptr::null_mut(), &mut tempo_val) };
        if status == au_sys::noErr {
            tempo = Some(tempo_val);
        }
    }

    let mut is_playing = None;
    let mut sample_pos = None;

    if let Some(cb) = callbacks.transportStateProc2 {
        let mut playing: au_sys::Boolean = 0;
        let mut recording: au_sys::Boolean = 0;
        let mut changed: au_sys::Boolean = 0;
        let mut current_sample: au_sys::Float64 = 0.0;
        let mut cycling: au_sys::Boolean = 0;
        let mut cycle_start: au_sys::Float64 = 0.0;
        let mut cycle_end: au_sys::Float64 = 0.0;
        let status = unsafe {
            cb(
                callbacks.hostUserData,
                &mut playing,
                &mut recording,
                &mut changed,
                &mut current_sample,
                &mut cycling,
                &mut cycle_start,
                &mut cycle_end,
            )
        };
        if status == au_sys::noErr {
            let _ = (recording, changed, cycling, cycle_start, cycle_end);
            is_playing = Some(playing != 0);
            sample_pos = Some(current_sample as i64);
        }
    } else if let Some(cb) = callbacks.transportStateProc {
        let mut playing: au_sys::Boolean = 0;
        let mut changed: au_sys::Boolean = 0;
        let mut current_sample: au_sys::Float64 = 0.0;
        let mut cycling: au_sys::Boolean = 0;
        let mut cycle_start: au_sys::Float64 = 0.0;
        let mut cycle_end: au_sys::Float64 = 0.0;
        let status = unsafe {
            cb(
                callbacks.hostUserData,
                &mut playing,
                &mut changed,
                &mut current_sample,
                &mut cycling,
                &mut cycle_start,
                &mut cycle_end,
            )
        };
        if status == au_sys::noErr {
            let _ = (changed, cycling, cycle_start, cycle_end);
            is_playing = Some(playing != 0);
            sample_pos = Some(current_sample as i64);
        }
    }

    TransportInfo {
        tempo,
        is_playing,
        sample_pos,
    }
}

pub fn set_parameter(
    unit: *mut c_void,
    id: u32,
    scope: u32,
    element: u32,
    value: f32,
    buffer_offset_frames: u32,
) -> i32 {
    let status = unsafe {
        AudioUnitSetParameter(
            unit as *mut _,
            id,
            scope,
            element,
            value,
            buffer_offset_frames,
        )
    };
    if status == 0 {
        status
    } else {
        set_parameter_direct(unit, id, value)
    }
}

pub fn set_parameter_local(unit: *mut c_void, id: u32, value: f32) -> i32 {
    set_parameter_direct(unit, id, value)
}

pub fn get_parameter(unit: *mut c_void, id: u32, scope: u32, element: u32) -> Result<f32, i32> {
    let mut value: f32 = 0.0;
    let status = unsafe { AudioUnitGetParameter(unit as *mut _, id, scope, element, &mut value) };
    if status == 0 {
        Ok(value)
    } else {
        match get_parameter_direct(unit, id) {
            Ok(val) => Ok(val),
            Err(err) => Err(err),
        }
    }
}

pub fn get_parameter_local(unit: *mut c_void, id: u32) -> Result<f32, i32> {
    get_parameter_direct(unit, id)
}

impl PluginInfo {
    pub const fn descriptor(
        self,
        parameters: &'static [ParameterInfo],
        gui: Option<fn() -> AudioUnitCocoaViewInfo>,
    ) -> au_sys::AuComponentDescriptor {
        au_sys::AuComponentDescriptor {
            name: self.name,
            component_type: self.component_type,
            component_subtype: self.component_subtype,
            manufacturer: self.manufacturer,
            version: self.version,
            flags: self.flags,
            flags_mask: self.flags_mask,
            input_channels: self.input_channels,
            output_channels: self.output_channels,
            supports_midi: self.supports_midi,
            parameters,
            cocoa_view_info: gui,
            cocoa_view_class: None,
            cocoa_view_bundle_id: None,
            cocoa_view_init: None,
        }
    }
}

pub trait Plugin: Send {
    fn init(sample_rate: f64, max_frames: u32) -> Self
    where
        Self: Sized;

    fn reset(&mut self) {}

    fn process(
        &mut self,
        inputs: Option<BufferList<'_>>,
        outputs: &mut BufferList<'_>,
        frames: usize,
    );

    fn parameters(&self) -> &'static [ParameterInfo];

    fn get_parameter(&self, id: u32) -> f32;

    fn set_parameter(&mut self, id: u32, value: f32);

    fn handle_midi_event(&mut self, _status: u8, _data1: u8, _data2: u8, _offset: u32) {}

    fn start_note(&mut self, _pitch: f32, _velocity: f32, _offset: u32) -> u32 {
        0
    }

    fn stop_note(&mut self, _note_id: u32, _offset: u32) {}
}

pub fn for_each_channel<'a, 'b, F>(
    mut inputs: Option<BufferList<'a>>,
    outputs: &mut BufferList<'b>,
    frames: usize,
    mut f: F,
) where
    F: FnMut(Option<&[f32]>, &mut [f32]),
{
    let channels = outputs.len();
    for ch in 0..channels {
        let out = unsafe { outputs.channel_mut(ch) };
        let out_len = frames.min(out.len());
        let out_slice = &mut out[..out_len];
        let in_slice = inputs
            .as_mut()
            .map(|input| unsafe { input.channel_mut(ch) })
            .map(|buf| {
                let in_len = frames.min(buf.len());
                &buf[..in_len]
            });
        f(in_slice, out_slice);
    }
}

pub struct PluginWrapper<T>(T);

impl<T: Plugin> au_sys::AuPlugin for PluginWrapper<T> {
    fn new(sample_rate: f64, max_frames: u32) -> Self {
        PluginWrapper(T::init(sample_rate, max_frames))
    }

    fn reset(&mut self) {
        self.0.reset()
    }

    fn process(
        &mut self,
        inputs: Option<BufferList<'_>>,
        outputs: &mut BufferList<'_>,
        frames: usize,
    ) {
        self.0.process(inputs, outputs, frames)
    }

    fn parameters(&self) -> &'static [ParameterInfo] {
        self.0.parameters()
    }

    fn get_parameter(&self, id: u32) -> f32 {
        self.0.get_parameter(id)
    }

    fn set_parameter(&mut self, id: u32, value: f32) {
        self.0.set_parameter(id, value)
    }

    fn handle_midi_event(&mut self, status: u8, data1: u8, data2: u8, offset: u32) {
        self.0.handle_midi_event(status, data1, data2, offset)
    }

    fn start_note(&mut self, pitch: f32, velocity: f32, offset: u32) -> u32 {
        self.0.start_note(pitch, velocity, offset)
    }

    fn stop_note(&mut self, note_id: u32, offset: u32) {
        self.0.stop_note(note_id, offset)
    }
}

pub mod gui;
pub use gui::opengl::glow_safe;

#[macro_export]
macro_rules! export_au_plugin {
    ($factory_fn:ident, $plugin:ty, $info:expr, $parameters:expr $(,)?) => {
        $crate::export_au_component!(
            $factory_fn,
            $crate::PluginWrapper<$plugin>,
            $info.descriptor($parameters, None)
        );
    };
    ($factory_fn:ident, $plugin:ty, $info:expr, $parameters:expr, gui: None $(,)?) => {
        $crate::export_au_component!(
            $factory_fn,
            $crate::PluginWrapper<$plugin>,
            $info.descriptor($parameters, None)
        );
    };
    ($factory_fn:ident, $plugin:ty, $info:expr, $parameters:expr, gui: { handler: $gui:ty, config: $gui_config:expr } $(,)?) => {
        #[allow(dead_code)]
        fn __au_rs_cocoa_view_info() -> $crate::AudioUnitCocoaViewInfo {
            $crate::gui::register_gui::<$gui>($gui_config)
        }

        $crate::export_au_component!(
            $factory_fn,
            $crate::PluginWrapper<$plugin>,
            $info.descriptor($parameters, Some(__au_rs_cocoa_view_info))
        );
    };
}
