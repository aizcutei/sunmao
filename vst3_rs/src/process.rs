//! Audio processing context

use std::ffi::c_void;
use vst3_sys::vst::processcontext::{ProcessContext as SysProcessContext, ProcessContextFlags};

/// A normalized parameter value that becomes effective at a sample offset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamChange {
    pub sample_offset: u32,
    pub id: u32,
    pub value: f64,
}

/// Failure reported by a safe VST3 processing callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessError {
    /// Activation-owned realtime scratch was exhausted.
    OutOfMemory,
    /// The host supplied invalid processing data.
    InvalidArgument,
    /// The plugin could not complete processing.
    Internal,
}

/// Result returned by [`crate::Plugin::process`].
pub type ProcessResult = Result<(), ProcessError>;

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
    param_changes: Vec<ParamChange>,
    param_sort_scratch: Vec<ParamChange>,
    max_events: usize,
}

impl ProcessContext {
    /// Create a new process context
    pub(crate) fn new(
        num_samples: usize,
        sample_rate: f64,
        num_in_channels: usize,
        num_out_channels: usize,
        max_events: usize,
    ) -> Self {
        Self {
            num_samples,
            sample_rate,
            tempo: None,
            is_playing: true,
            sample_pos: 0,
            inputs: (0..num_in_channels)
                .map(|_| vec![0.0; num_samples])
                .collect(),
            outputs: (0..num_out_channels)
                .map(|_| vec![0.0; num_samples])
                .collect(),
            param_changes: Vec::with_capacity(max_events),
            param_sort_scratch: Vec::with_capacity(max_events),
            max_events,
        }
    }

    /// Get input buffer for a channel
    pub fn input(&self, channel: usize) -> &[f32] {
        self.inputs
            .get(channel)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get mutable output buffer for a channel
    pub fn output_mut(&mut self, channel: usize) -> &mut [f32] {
        self.outputs
            .get_mut(channel)
            .map(|v| v.as_mut_slice())
            .unwrap_or(&mut [])
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
    pub(crate) unsafe fn copy_from_raw_inputs(
        &mut self,
        buffers: *const *const f32,
        num_channels: usize,
        num_samples: usize,
    ) {
        self.clear_inputs(num_samples);
        unsafe {
            self.copy_from_raw_inputs_at(buffers, num_channels, num_samples, 0, 0);
        }
    }

    pub(crate) fn clear_inputs(&mut self, num_samples: usize) {
        for input in &mut self.inputs {
            let len = num_samples.min(input.len());
            input[..len].fill(0.0);
        }
    }

    pub(crate) unsafe fn copy_from_raw_inputs_at(
        &mut self,
        buffers: *const *const f32,
        num_channels: usize,
        num_samples: usize,
        channel_offset: usize,
        silence_flags: u64,
    ) {
        if buffers.is_null() {
            return;
        }
        let available_channels = self.inputs.len().saturating_sub(channel_offset);
        for ch in 0..num_channels.min(available_channels) {
            if ch < 64 && (silence_flags & (1_u64 << ch)) != 0 {
                continue;
            }
            let src = unsafe { *buffers.add(ch) };
            if !src.is_null() {
                let dst = &mut self.inputs[channel_offset + ch];
                let len = num_samples.min(dst.len());
                unsafe {
                    std::ptr::copy_nonoverlapping(src, dst.as_mut_ptr(), len);
                }
            }
        }
    }

    /// Copy to raw output buffers
    pub(crate) unsafe fn copy_to_raw_outputs(
        &self,
        buffers: *const *mut f32,
        num_channels: usize,
        num_samples: usize,
    ) {
        unsafe {
            self.copy_to_raw_outputs_at(buffers, num_channels, num_samples, 0);
        }
    }

    pub(crate) unsafe fn copy_to_raw_outputs_at(
        &self,
        buffers: *const *mut f32,
        num_channels: usize,
        num_samples: usize,
        channel_offset: usize,
    ) {
        if buffers.is_null() {
            return;
        }
        let available_channels = self.outputs.len().saturating_sub(channel_offset);
        for ch in 0..num_channels.min(available_channels) {
            let dst = unsafe { *buffers.add(ch) };
            if !dst.is_null() {
                let src = &self.outputs[channel_offset + ch];
                let len = num_samples.min(src.len());
                unsafe {
                    std::ptr::copy_nonoverlapping(src.as_ptr(), dst, len);
                }
            }
        }
    }

    /// Add a parameter change without growing realtime scratch.
    pub(crate) fn try_add_param_change(&mut self, sample_offset: u32, id: u32, value: f64) -> bool {
        if self.param_changes.len() >= self.max_events {
            return false;
        }
        self.param_changes.push(ParamChange {
            sample_offset,
            id,
            value,
        });
        true
    }

    /// Get parameter changes for this block
    pub fn param_changes(&self) -> &[ParamChange] {
        &self.param_changes
    }

    pub(crate) fn sort_param_changes_by_offset(&mut self) {
        let len = self.param_changes.len();
        let mut width = 1usize;
        while width < len {
            self.param_sort_scratch.clear();
            let mut start = 0usize;
            while start < len {
                let middle = start.saturating_add(width).min(len);
                let end = middle.saturating_add(width).min(len);
                let (mut left, mut right) = (start, middle);
                while left < middle || right < end {
                    let take_left = left < middle
                        && (right == end
                            || self.param_changes[left].sample_offset
                                <= self.param_changes[right].sample_offset);
                    let change = if take_left {
                        let change = self.param_changes[left];
                        left += 1;
                        change
                    } else {
                        let change = self.param_changes[right];
                        right += 1;
                        change
                    };
                    self.param_sort_scratch.push(change);
                }
                start = end;
            }
            std::mem::swap(&mut self.param_changes, &mut self.param_sort_scratch);
            width = width.saturating_mul(2);
        }
        self.param_sort_scratch.clear();
    }

    /// Clear parameter changes
    pub(crate) fn clear_param_changes(&mut self) {
        self.param_changes.clear();
    }

    pub(crate) fn max_events(&self) -> usize {
        self.max_events
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_and_silent_inputs_are_zeroed_each_block() {
        let mut context = ProcessContext::new(4, 48_000.0, 2, 0, 8);
        let samples = [1.0_f32, 2.0, 3.0, 4.0];
        let buffers = [samples.as_ptr(), samples.as_ptr()];

        unsafe {
            context.copy_from_raw_inputs(buffers.as_ptr(), buffers.len(), samples.len());
        }
        assert_eq!(context.input(0), samples);
        assert_eq!(context.input(1), samples);

        context.clear_inputs(samples.len());
        unsafe {
            context.copy_from_raw_inputs_at(buffers.as_ptr(), buffers.len(), samples.len(), 0, 1);
        }
        assert_eq!(context.input(0), [0.0; 4]);
        assert_eq!(context.input(1), samples);

        unsafe {
            context.copy_from_raw_inputs(std::ptr::null(), 0, samples.len());
        }
        assert_eq!(context.input(0), [0.0; 4]);
        assert_eq!(context.input(1), [0.0; 4]);
    }

    #[test]
    fn parameter_scratch_is_bounded_and_stably_sorted_without_growth() {
        let mut context = ProcessContext::new(8, 48_000.0, 0, 0, 3);
        assert!(context.try_add_param_change(7, 1, 0.1));
        assert!(context.try_add_param_change(2, 1, 0.2));
        assert!(context.try_add_param_change(7, 1, 0.3));
        let capacity = context.param_changes.capacity();

        assert!(!context.try_add_param_change(3, 1, 0.4));
        context.sort_param_changes_by_offset();
        assert_eq!(
            context.param_changes(),
            [
                ParamChange {
                    sample_offset: 2,
                    id: 1,
                    value: 0.2,
                },
                ParamChange {
                    sample_offset: 7,
                    id: 1,
                    value: 0.1,
                },
                ParamChange {
                    sample_offset: 7,
                    id: 1,
                    value: 0.3,
                },
            ]
        );
        assert_eq!(context.param_changes.capacity(), capacity);
        assert_eq!(context.max_events(), 3);
    }
}
