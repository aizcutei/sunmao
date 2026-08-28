//! Audio buffer abstraction.

/// A unified audio buffer for processing.
///
/// Provides access to input and output channel data.
pub struct AudioBuffer<'a> {
    inputs: InputStorage<'a>,
    outputs: OutputStorage<'a>,
    num_samples: usize,
    /// First channel index of each declared input bus, plus a trailing end
    /// marker. Empty when the host only exposes a flat channel list.
    input_bus_bounds: &'a [usize],
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
            input_bus_bounds: &[],
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
            input_bus_bounds: &[],
        }
    }

    /// Attaches the input bus layout produced by the format adapter.
    ///
    /// `bounds` holds the first channel index of every declared input bus plus
    /// a trailing end marker, so bus `i` owns channels
    /// `bounds[i]..bounds[i + 1]`.
    pub fn with_input_bus_bounds(mut self, bounds: &'a [usize]) -> Self {
        self.input_bus_bounds = bounds;
        self
    }

    /// Number of input buses the host connected, or 0 when the adapter did not
    /// provide a bus layout.
    pub fn num_input_buses(&self) -> usize {
        self.input_bus_bounds.len().saturating_sub(1)
    }

    /// Channel range owned by one input bus.
    ///
    /// Returns `None` for an unknown bus, so a plugin whose sidechain the host
    /// left unconnected takes the same path as one running without a
    /// sidechain at all.
    pub fn input_bus_channels(&self, bus: usize) -> Option<std::ops::Range<usize>> {
        let start = *self.input_bus_bounds.get(bus)?;
        let end = *self.input_bus_bounds.get(bus + 1)?;
        let available = self.num_input_channels();
        (start < end && end <= available).then_some(start..end)
    }

    /// One channel of an input bus, or an empty slice when either the bus or
    /// the channel is absent.
    pub fn input_bus(&self, bus: usize, channel: usize) -> &[f32] {
        match self.input_bus_channels(bus) {
            Some(range) if channel < range.len() => self.input(range.start + channel),
            _ => &[],
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
        let num_samples = self.num_samples;
        match &self.inputs {
            InputStorage::Slices(inputs) => inputs
                .get(channel)
                .map(|input| &input[..num_samples.min(input.len())])
                .unwrap_or(&[]),
            InputStorage::Planar(inputs) => inputs
                .get(channel)
                .map(|input| &input[..num_samples.min(input.len())])
                .unwrap_or(&[]),
        }
    }

    /// Get mutable output channel data.
    pub fn output(&mut self, channel: usize) -> &mut [f32] {
        let num_samples = self.num_samples;
        match &mut self.outputs {
            OutputStorage::Slices(outputs) => outputs
                .get_mut(channel)
                .map(|output| {
                    let len = num_samples.min(output.len());
                    &mut output[..len]
                })
                .unwrap_or(&mut []),
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

    #[test]
    fn slice_storage_is_limited_to_the_active_block() {
        let inputs = [&[1.0_f32, 2.0, 3.0, 4.0][..]];
        let mut output = [0.0_f32; 4];
        let mut outputs: [&mut [f32]; 1] = [&mut output];

        let mut buffer = AudioBuffer::new(&inputs, &mut outputs, 2);
        assert_eq!(buffer.input(0), &[1.0, 2.0]);
        assert_eq!(buffer.output(0).len(), 2);
        buffer.output(0).fill(3.0);

        // Samples outside the active block are owned by the caller and must
        // remain untouched by the framework's channel accessors.
        assert_eq!(output, [3.0, 3.0, 0.0, 0.0]);
    }

    #[test]
    fn input_bus_bounds_split_the_flat_channel_list() {
        let main_left = [1.0_f32, 1.0];
        let main_right = [2.0_f32, 2.0];
        let key_left = [3.0_f32, 3.0];
        let key_right = [4.0_f32, 4.0];
        let inputs: [&[f32]; 4] = [&main_left, &main_right, &key_left, &key_right];
        let mut out_left = [0.0_f32; 2];
        let mut out_right = [0.0_f32; 2];
        let mut outputs: [&mut [f32]; 2] = [&mut out_left, &mut out_right];
        // Two stereo input buses: main is channels 0..2, sidechain is 2..4.
        let bounds = [0_usize, 2, 4];
        let buffer = AudioBuffer::new(&inputs, &mut outputs, 2).with_input_bus_bounds(&bounds);

        assert_eq!(buffer.num_input_buses(), 2);
        assert_eq!(buffer.input_bus_channels(0), Some(0..2));
        assert_eq!(buffer.input_bus_channels(1), Some(2..4));
        assert_eq!(buffer.input_bus(1, 0), &[3.0, 3.0]);
        assert_eq!(buffer.input_bus(1, 1), &[4.0, 4.0]);
    }

    #[test]
    fn an_absent_bus_reads_as_empty_rather_than_panicking() {
        let left = [1.0_f32, 1.0];
        let right = [2.0_f32, 2.0];
        let inputs: [&[f32]; 2] = [&left, &right];
        let mut out_left = [0.0_f32; 2];
        let mut outputs: [&mut [f32]; 1] = [&mut out_left];
        // The host connected the main bus only, so the sidechain bounds run
        // past the channels actually provided.
        let bounds = [0_usize, 2, 4];
        let buffer = AudioBuffer::new(&inputs, &mut outputs, 2).with_input_bus_bounds(&bounds);

        assert_eq!(buffer.input_bus_channels(0), Some(0..2));
        assert_eq!(buffer.input_bus_channels(1), None, "not connected");
        assert!(buffer.input_bus(1, 0).is_empty());
        assert!(buffer.input_bus(9, 0).is_empty(), "unknown bus");
    }

    #[test]
    fn a_buffer_without_a_bus_layout_reports_no_buses() {
        let left = [1.0_f32, 1.0];
        let inputs: [&[f32]; 1] = [&left];
        let mut out_left = [0.0_f32; 2];
        let mut outputs: [&mut [f32]; 1] = [&mut out_left];
        let buffer = AudioBuffer::new(&inputs, &mut outputs, 2);

        assert_eq!(buffer.num_input_buses(), 0);
        assert_eq!(buffer.input_bus_channels(0), None);
        assert!(buffer.input_bus(0, 0).is_empty());
    }
}
