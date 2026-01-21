//! Audio processing context

use std::ffi::c_void;
use vst3_sys::vst::processcontext::{ProcessContext as SysProcessContext, ProcessContextFlags};

/// Safe wrapper around VST3 ProcessData
pub struct ProcessContext {
    /// Number of samples in this block
    pub num_samples: usize,
    /// Current sample rate
    pub sample_rate: f64,
    /// Current tempo in BPM
    tempo: Option<f64>,
    /// Whether transport is playing
    is_playing: bool,
    /// Current position in samples
    sample_pos: i64,
    /// Input audio buffers (channel, sample)
    inputs: Vec<Vec<f32>>,
    /// Output audio buffers (channel, sample)
    outputs: Vec<Vec<f32>>,
    /// Parameter changes to apply
    param_changes: Vec<(u32, f64)>,
}

impl ProcessContext {
    /// Create a new process context
    pub(crate) fn new(num_samples: usize, sample_rate: f64, num_in_channels: usize, num_out_channels: usize) -> Self {
        Self {
            num_samples,
            sample_rate,
            tempo: None,
            is_playing: true,
            sample_pos: 0,
            inputs: (0..num_in_channels).map(|_| vec![0.0; num_samples]).collect(),
            outputs: (0..num_out_channels).map(|_| vec![0.0; num_samples]).collect(),
            param_changes: Vec::new(),
        }
    }
    
    /// Get input buffer for a channel
    pub fn input(&self, channel: usize) -> &[f32] {
        self.inputs.get(channel).map(|v| v.as_slice()).unwrap_or(&[])
    }
    
    /// Get mutable output buffer for a channel
    pub fn output_mut(&mut self, channel: usize) -> &mut [f32] {
        self.outputs.get_mut(channel).map(|v| v.as_mut_slice()).unwrap_or(&mut [])
    }
    
    /// Number of input channels
    pub fn num_inputs(&self) -> usize {
        self.inputs.len()
    }
    
    /// Number of output channels
    pub fn num_outputs(&self) -> usize {
        self.outputs.len()
    }
    
    /// Copy from raw input buffers
    pub(crate) unsafe fn copy_from_raw_inputs(&mut self, buffers: *const *const f32, num_channels: usize, num_samples: usize) {
        for ch in 0..num_channels.min(self.inputs.len()) {
            let src = *buffers.add(ch);
            if !src.is_null() {
                let dst = &mut self.inputs[ch];
                for i in 0..num_samples.min(dst.len()) {
                    dst[i] = *src.add(i);
                }
            }
        }
    }
    
    /// Copy to raw output buffers
    pub(crate) unsafe fn copy_to_raw_outputs(&self, buffers: *const *mut f32, num_channels: usize, num_samples: usize) {
        for ch in 0..num_channels.min(self.outputs.len()) {
            let dst = *buffers.add(ch);
            if !dst.is_null() {
                let src = &self.outputs[ch];
                for i in 0..num_samples.min(src.len()) {
                    *dst.add(i) = src[i];
                }
            }
        }
    }
    
    /// Add a parameter change
    pub(crate) fn add_param_change(&mut self, id: u32, value: f64) {
        self.param_changes.push((id, value));
    }
    
    /// Get parameter changes for this block
    pub fn param_changes(&self) -> &[(u32, f64)] {
        &self.param_changes
    }
    
    /// Clear parameter changes
    pub(crate) fn clear_param_changes(&mut self) {
        self.param_changes.clear();
    }

    pub fn tempo(&self) -> Option<f64> {
        self.tempo
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing
    }

    pub fn sample_pos(&self) -> i64 {
        self.sample_pos
    }

    pub(crate) fn update_transport_from_raw(&mut self, raw: *const c_void) {
        if raw.is_null() {
            self.tempo = None;
            self.is_playing = true;
            self.sample_pos = 0;
            return;
        }

        let ctx = unsafe { &*(raw as *const SysProcessContext) };
        self.is_playing = (ctx.state & ProcessContextFlags::kPlaying) != 0;
        self.tempo = if (ctx.state & ProcessContextFlags::kTempoValid) != 0 {
            Some(ctx.tempo)
        } else {
            None
        };
        self.sample_pos = if (ctx.state & ProcessContextFlags::kContTimeValid) != 0 {
            ctx.continous_time_samples
        } else {
            ctx.project_time_samples
        };
    }
}
