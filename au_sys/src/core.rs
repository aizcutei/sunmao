#![allow(unsafe_op_in_unsafe_fn)]

use crate::sys::{
    AudioBuffer, AudioBufferList, AudioUnitParameterUnit, kAudioUnitParameterUnit_Generic,
    kAudioUnitParameterUnit_LinearGain,
};

pub struct BufferList<'a> {
    list: *mut AudioBufferList,
    frames: usize,
    _marker: std::marker::PhantomData<&'a mut AudioBufferList>,
}

impl<'a> BufferList<'a> {
    pub unsafe fn from_raw(list: *mut AudioBufferList, frames: usize) -> Self {
        Self {
            list,
            frames,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn len(&self) -> usize {
        unsafe { (*self.list).mNumberBuffers as usize }
    }

    pub unsafe fn buffer_mut(&mut self, index: usize) -> &mut AudioBuffer {
        let buffers = std::slice::from_raw_parts_mut(
            (*self.list).mBuffers.as_mut_ptr(),
            (*self.list).mNumberBuffers as usize,
        );
        &mut buffers[index]
    }

    pub unsafe fn channel_mut(&mut self, index: usize) -> &mut [f32] {
        let buffer = self.buffer_mut(index);
        if buffer.mData.is_null() {
            return &mut [];
        }
        std::slice::from_raw_parts_mut(buffer.mData as *mut f32, self.frames)
    }
}

#[derive(Clone, Copy)]
pub enum ParameterUnit {
    Generic,
    LinearGain,
}

impl ParameterUnit {
    pub fn as_au_unit(self) -> AudioUnitParameterUnit {
        match self {
            ParameterUnit::Generic => kAudioUnitParameterUnit_Generic,
            ParameterUnit::LinearGain => kAudioUnitParameterUnit_LinearGain,
        }
    }
}

#[derive(Clone, Copy)]
pub struct ParameterInfo {
    pub id: u32,
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub unit: ParameterUnit,
}

pub trait AuPlugin: Send {
    fn new(sample_rate: f64, max_frames: u32) -> Self
    where
        Self: Sized;

    fn reset(&mut self) {}

    fn process(&mut self, inputs: Option<BufferList<'_>>, outputs: &mut BufferList<'_>, frames: usize);

    fn parameters(&self) -> &'static [ParameterInfo];

    fn get_parameter(&self, id: u32) -> f32;

    fn set_parameter(&mut self, id: u32, value: f32);

    fn handle_midi_event(&mut self, _status: u8, _data1: u8, _data2: u8, _offset: u32) {}

    fn start_note(&mut self, _pitch: f32, _velocity: f32, _offset: u32) -> u32 {
        0
    }

    fn stop_note(&mut self, _note_id: u32, _offset: u32) {}
}
