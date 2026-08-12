//! Audio buffer abstraction.

/// A unified audio buffer for processing.
///
/// Provides access to input and output channel data.
pub struct AudioBuffer<'a> {
    inputs: InputStorage<'a>,
    outputs: OutputStorage<'a>,
    num_samples: usize,
}

enum InputStorage<'a> {
    Slices(&'a [&'a [f32]]),
    Planar(&'a [Vec<f32>]),
}

enum OutputStorage<'a> {
    Slices(&'a mut [&'a mut [f32]]),
    Planar(&'a mut [Vec<f32>]),
}

impl<'a> AudioBuffer<'a> {
    /// Create a new AudioBuffer.
    pub fn new(
        inputs: &'a [&'a [f32]],
        outputs: &'a mut [&'a mut [f32]],
        num_samples: usize,
    ) -> Self {
        Self {
            inputs: InputStorage::Slices(inputs),
            outputs: OutputStorage::Slices(outputs),
            num_samples,
        }
    }

    /// Borrow activation-owned planar channel storage without building
    /// temporary vectors of channel references.
    pub fn from_planar(
        inputs: &'a [Vec<f32>],
        outputs: &'a mut [Vec<f32>],
        num_samples: usize,
    ) -> Self {
        Self {
            inputs: InputStorage::Planar(inputs),
            outputs: OutputStorage::Planar(outputs),
            num_samples,
        }
    }

    /// Number of samples in this buffer.
    pub fn num_samples(&self) -> usize {
        self.num_samples
    }

    /// Number of input channels.
    pub fn num_input_channels(&self) -> usize {
        match &self.inputs {
            InputStorage::Slices(inputs) => inputs.len(),
            InputStorage::Planar(inputs) => inputs.len(),
        }
    }

    /// Number of output channels.
    pub fn num_output_channels(&self) -> usize {
        match &self.outputs {
            OutputStorage::Slices(outputs) => outputs.len(),
            OutputStorage::Planar(outputs) => outputs.len(),
        }
    }

    /// Get input channel data.
    pub fn input(&self, channel: usize) -> &[f32] {
        match &self.inputs {
            InputStorage::Slices(inputs) => inputs.get(channel).copied().unwrap_or(&[]),
            InputStorage::Planar(inputs) => inputs
                .get(channel)
                .map(|input| &input[..self.num_samples.min(input.len())])
                .unwrap_or(&[]),
        }
    }

    /// Get mutable output channel data.
    pub fn output(&mut self, channel: usize) -> &mut [f32] {
        let num_samples = self.num_samples;
        match &mut self.outputs {
            OutputStorage::Slices(outputs) => {
                if channel < outputs.len() {
                    outputs[channel]
                } else {
                    &mut []
                }
            }
            OutputStorage::Planar(outputs) => outputs
                .get_mut(channel)
                .map(|output| {
                    let len = num_samples.min(output.len());
                    &mut output[..len]
                })
                .unwrap_or(&mut []),
        }
    }

    /// Apply gain to all output channels.
    pub fn apply_gain(&mut self, gain: f32) {
        for ch in 0..self.num_output_channels() {
            for sample in self.output(ch).iter_mut() {
                *sample *= gain;
            }
        }
    }

    /// Clear all output channels.
    pub fn clear(&mut self) {
        for ch in 0..self.num_output_channels() {
            self.output(ch).fill(0.0);
        }
    }

    /// Copy input to output (passthrough).
    pub fn copy_input_to_output(&mut self) {
        let num_samples = self.num_samples;
        match (&self.inputs, &mut self.outputs) {
            (InputStorage::Slices(inputs), OutputStorage::Slices(outputs)) => {
                copy_channels(
                    inputs.iter().copied(),
                    outputs.iter_mut().map(|output| &mut **output),
                    num_samples,
                );
            }
            (InputStorage::Slices(inputs), OutputStorage::Planar(outputs)) => {
                copy_channels(
                    inputs.iter().copied(),
                    outputs.iter_mut().map(Vec::as_mut_slice),
                    num_samples,
                );
            }
            (InputStorage::Planar(inputs), OutputStorage::Slices(outputs)) => {
                copy_channels(
                    inputs.iter().map(Vec::as_slice),
                    outputs.iter_mut().map(|output| &mut **output),
                    num_samples,
                );
            }
            (InputStorage::Planar(inputs), OutputStorage::Planar(outputs)) => {
                copy_channels(
                    inputs.iter().map(Vec::as_slice),
                    outputs.iter_mut().map(Vec::as_mut_slice),
                    num_samples,
                );
            }
        }
    }
}

fn copy_channels<'a>(
    mut inputs: impl Iterator<Item = &'a [f32]>,
    outputs: impl Iterator<Item = &'a mut [f32]>,
    num_samples: usize,
) {
    for output in outputs {
        // A host may reuse output storage across blocks, and an effect may
        // expose more outputs (or shorter inputs) than it has inputs. Clear
        // the active block before copying the overlapping input prefix so no
        // stale samples escape through the passthrough helper.
        let active_len = num_samples.min(output.len());
        output[..active_len].fill(0.0);
        if let Some(input) = inputs.next() {
            let len = active_len.min(input.len());
            output[..len].copy_from_slice(&input[..len]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planar_storage_is_borrowed_and_limited_to_the_block_size() {
        let inputs = vec![vec![1.0, 2.0, 3.0, 4.0]];
        let input_ptr = inputs[0].as_ptr();
        let mut outputs = vec![vec![0.0; 4]];
        let output_ptr = outputs[0].as_ptr();

        {
            let mut buffer = AudioBuffer::from_planar(&inputs, &mut outputs, 3);
            assert_eq!(buffer.input(0).as_ptr(), input_ptr);
            assert_eq!(buffer.output(0).as_ptr(), output_ptr);
            buffer.copy_input_to_output();
            buffer.apply_gain(2.0);
        }

        assert_eq!(outputs[0], [2.0, 4.0, 6.0, 0.0]);
    }

    #[test]
    fn passthrough_clears_unmatched_channels_and_short_input_tails() {
        let inputs = [&[1.0_f32, 2.0][..]];
        let mut first_output = [9.0_f32; 4];
        let mut second_output = [8.0_f32; 4];
        let mut outputs: [&mut [f32]; 2] = [&mut first_output, &mut second_output];

        let mut buffer = AudioBuffer::new(&inputs, &mut outputs, 4);
        buffer.copy_input_to_output();

        assert_eq!(first_output, [1.0, 2.0, 0.0, 0.0]);
        assert_eq!(second_output, [0.0; 4]);
    }
}
