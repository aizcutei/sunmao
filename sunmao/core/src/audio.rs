//! Audio buffer abstraction.

/// A unified audio buffer for processing.
///
/// Provides access to input and output channel data.
pub struct AudioBuffer<'a> {
    inputs: &'a [&'a [f32]],
    outputs: &'a mut [&'a mut [f32]],
    num_samples: usize,
}

impl<'a> AudioBuffer<'a> {
    /// Create a new AudioBuffer.
    pub fn new(
        inputs: &'a [&'a [f32]],
        outputs: &'a mut [&'a mut [f32]],
        num_samples: usize,
    ) -> Self {
        Self { inputs, outputs, num_samples }
    }

    /// Number of samples in this buffer.
    pub fn num_samples(&self) -> usize {
        self.num_samples
    }

    /// Number of input channels.
    pub fn num_input_channels(&self) -> usize {
        self.inputs.len()
    }

    /// Number of output channels.
    pub fn num_output_channels(&self) -> usize {
        self.outputs.len()
    }

    /// Get input channel data.
    pub fn input(&self, channel: usize) -> &[f32] {
        self.inputs.get(channel).copied().unwrap_or(&[])
    }

    /// Get mutable output channel data.
    pub fn output(&mut self, channel: usize) -> &mut [f32] {
        if channel < self.outputs.len() {
            self.outputs[channel]
        } else {
            &mut []
        }
    }

    /// Apply gain to all output channels.
    pub fn apply_gain(&mut self, gain: f32) {
        for ch in 0..self.outputs.len() {
            for sample in self.outputs[ch].iter_mut() {
                *sample *= gain;
            }
        }
    }

    /// Clear all output channels.
    pub fn clear(&mut self) {
        for ch in 0..self.outputs.len() {
            for sample in self.outputs[ch].iter_mut() {
                *sample = 0.0;
            }
        }
    }

    /// Copy input to output (passthrough).
    pub fn copy_input_to_output(&mut self) {
        let channels = self.inputs.len().min(self.outputs.len());
        for ch in 0..channels {
            let len = self.inputs[ch].len().min(self.outputs[ch].len());
            self.outputs[ch][..len].copy_from_slice(&self.inputs[ch][..len]);
        }
    }
}
