use crate::events::Event;
use clap_sys::audio_buffer::clap_audio_buffer_t;
use clap_sys::events::{
    CLAP_TRANSPORT_HAS_BEATS_TIMELINE, CLAP_TRANSPORT_HAS_SECONDS_TIMELINE,
    CLAP_TRANSPORT_HAS_TEMPO, CLAP_TRANSPORT_IS_PLAYING, clap_event_transport_t,
    clap_input_events_t, clap_output_events_t,
};
use clap_sys::fixedpoint::{CLAP_BEATTIME_FACTOR, CLAP_SECTIME_FACTOR, clap_sectime};
use clap_sys::process::{clap_process_status, clap_process_t};
use std::marker::PhantomData;
use std::ops::{Index, IndexMut};
use std::slice;

/// Largest host-negotiated processing block accepted by the CLAP adapter.
///
/// CLAP exposes the block bound as a `u32`, but accepting an arbitrary host
/// value would let a malformed host request an unbounded activation-time
/// allocation. One million frames is substantially above normal DAW block
/// sizes while keeping the adapter's memory contract explicit and portable.
pub const MAX_PROCESS_FRAMES: u32 = 1 << 20;

/// Largest channel topology accepted by the format adapter.
///
/// The value is deliberately shared by the activation scratch allocator and
/// the raw-buffer validator.  A host must not be able to turn a valid frame
/// count into an unbounded number of per-channel allocations.
pub const MAX_PROCESS_CHANNELS: usize = 256;

/// Maximum number of f32 samples reserved for one input or output direction.
///
/// This is a count of samples, rather than bytes, so the contract is stable
/// across the f32/f64 host buffer representations supported by CLAP.
pub const MAX_PROCESS_AUDIO_SAMPLES: usize = 16 << 20;

/// Maximum number of events traversed or retained for one processing block.
pub const MAX_PROCESS_EVENTS: usize = 1 << 16;

pub struct ProcessContext<'a> {
    pub frames_count: u32,
    pub audio_inputs: AudioInputs<'a>,
    pub audio_outputs: AudioOutputs<'a>,
    input_events: *const clap_input_events_t,
    output_events: *const clap_output_events_t,
    transport: *const clap_event_transport_t,
}

/// Borrowed view of the activation-owned input channel storage.
pub struct AudioInputs<'a> {
    channels: &'a [Vec<f32>],
    frames_count: usize,
}

impl<'a> AudioInputs<'a> {
    fn new(channels: &'a [Vec<f32>], frames_count: usize) -> Self {
        Self {
            channels,
            frames_count,
        }
    }

    pub fn len(&self) -> usize {
        self.channels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.channels.is_empty()
    }

    pub fn iter(&self) -> AudioInputIter<'_> {
        AudioInputIter {
            channels: self.channels.iter(),
            frames_count: self.frames_count,
        }
    }
}

impl Index<usize> for AudioInputs<'_> {
    type Output = [f32];

    fn index(&self, index: usize) -> &Self::Output {
        &self.channels[index][..self.frames_count]
    }
}

pub struct AudioInputIter<'a> {
    channels: slice::Iter<'a, Vec<f32>>,
    frames_count: usize,
}

impl<'a> Iterator for AudioInputIter<'a> {
    type Item = &'a [f32];

    fn next(&mut self) -> Option<Self::Item> {
        self.channels
            .next()
            .map(|channel| &channel[..self.frames_count])
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.channels.size_hint()
    }
}

impl ExactSizeIterator for AudioInputIter<'_> {}

impl<'a, 'b> IntoIterator for &'b AudioInputs<'a> {
    type Item = &'b [f32];
    type IntoIter = AudioInputIter<'b>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Mutable borrowed view of the activation-owned output channel storage.
pub struct AudioOutputs<'a> {
    channels: &'a mut [Vec<f32>],
    frames_count: usize,
}

impl<'a> AudioOutputs<'a> {
    fn new(channels: &'a mut [Vec<f32>], frames_count: usize) -> Self {
        Self {
            channels,
            frames_count,
        }
    }

    pub fn len(&self) -> usize {
        self.channels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.channels.is_empty()
    }

    pub fn iter(&self) -> AudioInputIter<'_> {
        AudioInputIter {
            channels: self.channels.iter(),
            frames_count: self.frames_count,
        }
    }

    pub fn iter_mut(&mut self) -> AudioOutputIter<'_> {
        AudioOutputIter {
            channels: self.channels.iter_mut(),
            frames_count: self.frames_count,
        }
    }
}

impl Index<usize> for AudioOutputs<'_> {
    type Output = [f32];

    fn index(&self, index: usize) -> &Self::Output {
        &self.channels[index][..self.frames_count]
    }
}

impl IndexMut<usize> for AudioOutputs<'_> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.channels[index][..self.frames_count]
    }
}

pub struct AudioOutputIter<'a> {
    channels: slice::IterMut<'a, Vec<f32>>,
    frames_count: usize,
}

impl<'a> Iterator for AudioOutputIter<'a> {
    type Item = &'a mut [f32];

    fn next(&mut self) -> Option<Self::Item> {
        self.channels
            .next()
            .map(|channel| &mut channel[..self.frames_count])
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.channels.size_hint()
    }
}

impl ExactSizeIterator for AudioOutputIter<'_> {}

impl<'a, 'b> IntoIterator for &'b AudioOutputs<'a> {
    type Item = &'b [f32];
    type IntoIter = AudioInputIter<'b>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, 'b> IntoIterator for &'b mut AudioOutputs<'a> {
    type Item = &'b mut [f32];
    type IntoIter = AudioOutputIter<'b>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<'a> ProcessContext<'a> {
    pub fn transport(&self) -> Option<Transport> {
        if self.transport.is_null() {
            None
        } else {
            Some(Transport {
                raw: unsafe { *self.transport },
            })
        }
    }

    /// Number of raw host events, or `None` for a malformed input event list.
    pub fn event_count(&self) -> Option<u32> {
        if self.input_events.is_null() {
            Some(0)
        } else {
            unsafe {
                let events = &*self.input_events;
                match (events.size, events.get) {
                    (Some(size), Some(_)) => {
                        let count = size(self.input_events);
                        (count as usize <= MAX_PROCESS_EVENTS).then_some(count)
                    }
                    _ => None,
                }
            }
        }
    }

    pub fn events(&self) -> InputEventIterator<'a> {
        let count = self.event_count().unwrap_or(0);

        InputEventIterator {
            events: self.input_events,
            index: 0,
            count,
            _marker: PhantomData,
        }
    }
}

/// Activation-owned channel storage used to adapt CLAP's f32/f64 bus buffers
/// to the framework's f32 processing API.
pub(crate) struct ProcessBuffers {
    input_bus_channels: Vec<u32>,
    output_bus_channels: Vec<u32>,
    input_channels: Vec<Vec<f32>>,
    output_channels: Vec<Vec<f32>>,
    max_frames: usize,
    active: bool,
}

impl ProcessBuffers {
    pub(crate) fn new(input_bus_channels: Vec<u32>, output_bus_channels: Vec<u32>) -> Self {
        Self {
            input_bus_channels,
            output_bus_channels,
            input_channels: Vec::new(),
            output_channels: Vec::new(),
            max_frames: 0,
            active: false,
        }
    }

    pub(crate) fn activate(&mut self, max_frames: u32) -> bool {
        if max_frames > MAX_PROCESS_FRAMES {
            return false;
        }
        let max_frames = max_frames as usize;
        let Some(input_channels) = allocate_channels(&self.input_bus_channels, max_frames) else {
            return false;
        };
        let Some(output_channels) = allocate_channels(&self.output_bus_channels, max_frames) else {
            return false;
        };

        self.input_channels = input_channels;
        self.output_channels = output_channels;
        self.max_frames = max_frames;
        self.active = true;
        true
    }

    pub(crate) fn deactivate(&mut self) {
        self.input_channels.clear();
        self.output_channels.clear();
        self.max_frames = 0;
        self.active = false;
    }

    /// # Safety
    ///
    /// `process` and every buffer pointer it describes must remain valid for
    /// the duration of this call, as required by the CLAP process contract.
    pub(crate) unsafe fn process(
        &mut self,
        process: *const clap_process_t,
        callback: impl FnOnce(ProcessContext<'_>) -> clap_process_status,
    ) -> Result<clap_process_status, ()> {
        if process.is_null() || !self.active {
            return Err(());
        }

        let process = unsafe { &*process };
        let frames_count = process.frames_count as usize;
        if frames_count > self.max_frames {
            return Err(());
        }

        let input_buses = unsafe {
            validate_buses(
                process.audio_inputs,
                process.audio_inputs_count,
                &self.input_bus_channels,
                frames_count,
            )?
        };
        let output_buses = unsafe {
            validate_buses(
                process.audio_outputs.cast_const(),
                process.audio_outputs_count,
                &self.output_bus_channels,
                frames_count,
            )?
        };

        unsafe { copy_inputs(input_buses, &mut self.input_channels, frames_count) };
        for output in &mut self.output_channels {
            output[..frames_count].fill(0.0);
        }

        let context = ProcessContext {
            frames_count: process.frames_count,
            audio_inputs: AudioInputs::new(&self.input_channels, frames_count),
            audio_outputs: AudioOutputs::new(&mut self.output_channels, frames_count),
            input_events: process.in_events,
            output_events: process.out_events,
            transport: process.transport,
        };

        let status = callback(context);
        unsafe { copy_outputs(output_buses, &self.output_channels, frames_count) };
        Ok(status)
    }
}

fn allocate_channels(bus_channels: &[u32], max_frames: usize) -> Option<Vec<Vec<f32>>> {
    let channel_count = bus_channels
        .iter()
        .try_fold(0usize, |total, &count| total.checked_add(count as usize))?;
    if channel_count > MAX_PROCESS_CHANNELS
        || channel_count
            .checked_mul(max_frames)
            .is_none_or(|samples| samples > MAX_PROCESS_AUDIO_SAMPLES)
    {
        return None;
    }
    let mut channels = Vec::new();
    channels.try_reserve_exact(channel_count).ok()?;
    for _ in 0..channel_count {
        let mut channel = Vec::new();
        channel.try_reserve_exact(max_frames).ok()?;
        channel.resize(max_frames, 0.0);
        channels.push(channel);
    }
    Some(channels)
}

#[derive(Clone, Copy)]
enum SampleData {
    /// A bus with no channels has no channel-pointer array. CLAP permits both
    /// data pointers to be null in this case.
    Empty,
    F32(*mut *mut f32),
    F64(*mut *mut f64),
}

fn sample_data(buffer: &clap_audio_buffer_t) -> Result<SampleData, ()> {
    if buffer.channel_count == 0 {
        return match (buffer.data32.is_null(), buffer.data64.is_null()) {
            // Hosts commonly omit storage for empty buses, while the CLAP
            // audio-buffer contract also permits advertising one sample type.
            (true, true) | (false, true) | (true, false) => Ok(SampleData::Empty),
            (false, false) => Err(()),
        };
    }
    match (buffer.data32.is_null(), buffer.data64.is_null()) {
        (false, true) => Ok(SampleData::F32(buffer.data32)),
        (true, false) => Ok(SampleData::F64(buffer.data64)),
        _ => Err(()),
    }
}

unsafe fn validate_buses<'a>(
    buses: *const clap_audio_buffer_t,
    bus_count: u32,
    expected_channels: &[u32],
    frames_count: usize,
) -> Result<&'a [clap_audio_buffer_t], ()> {
    if bus_count as usize != expected_channels.len() {
        return Err(());
    }
    if expected_channels.is_empty() {
        return Ok(&[]);
    }
    if buses.is_null() {
        return Err(());
    }

    let buses = unsafe { slice::from_raw_parts(buses, expected_channels.len()) };
    for (bus, &channel_count) in buses.iter().zip(expected_channels) {
        if bus.channel_count != channel_count {
            return Err(());
        }
        match sample_data(bus)? {
            SampleData::Empty => {}
            SampleData::F32(channels) => {
                if frames_count > 0 {
                    for index in 0..channel_count as usize {
                        if unsafe { (*channels.add(index)).is_null() } {
                            return Err(());
                        }
                    }
                }
            }
            SampleData::F64(channels) => {
                if frames_count > 0 {
                    for index in 0..channel_count as usize {
                        if unsafe { (*channels.add(index)).is_null() } {
                            return Err(());
                        }
                    }
                }
            }
        }
    }
    Ok(buses)
}

unsafe fn copy_inputs(
    buses: &[clap_audio_buffer_t],
    scratch: &mut [Vec<f32>],
    frames_count: usize,
) {
    if frames_count == 0 {
        return;
    }
    let mut scratch = scratch.iter_mut();
    for bus in buses {
        match sample_data(bus).expect("validated input buffer") {
            SampleData::Empty => {}
            SampleData::F32(channels) => {
                for index in 0..bus.channel_count as usize {
                    let source =
                        unsafe { slice::from_raw_parts(*channels.add(index), frames_count) };
                    scratch.next().expect("declared input channel")[..frames_count]
                        .copy_from_slice(source);
                }
            }
            SampleData::F64(channels) => {
                for index in 0..bus.channel_count as usize {
                    let source =
                        unsafe { slice::from_raw_parts(*channels.add(index), frames_count) };
                    let destination =
                        &mut scratch.next().expect("declared input channel")[..frames_count];
                    for (destination, &source) in destination.iter_mut().zip(source) {
                        *destination = source as f32;
                    }
                }
            }
        }
    }
}

unsafe fn copy_outputs(buses: &[clap_audio_buffer_t], scratch: &[Vec<f32>], frames_count: usize) {
    if frames_count == 0 {
        return;
    }
    let mut scratch = scratch.iter();
    for bus in buses {
        match sample_data(bus).expect("validated output buffer") {
            SampleData::Empty => {}
            SampleData::F32(channels) => {
                for index in 0..bus.channel_count as usize {
                    let destination =
                        unsafe { slice::from_raw_parts_mut(*channels.add(index), frames_count) };
                    destination.copy_from_slice(
                        &scratch.next().expect("declared output channel")[..frames_count],
                    );
                }
            }
            SampleData::F64(channels) => {
                for index in 0..bus.channel_count as usize {
                    let destination =
                        unsafe { slice::from_raw_parts_mut(*channels.add(index), frames_count) };
                    let source = &scratch.next().expect("declared output channel")[..frames_count];
                    for (destination, &source) in destination.iter_mut().zip(source) {
                        *destination = source as f64;
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Transport {
    raw: clap_event_transport_t,
}

impl Transport {
    pub fn tempo(&self) -> Option<f64> {
        if (self.raw.flags & CLAP_TRANSPORT_HAS_TEMPO) != 0 {
            self.raw
                .tempo
                .is_finite()
                .then_some(self.raw.tempo)
                .filter(|tempo| *tempo >= 0.0)
        } else {
            None
        }
    }

    pub fn is_playing(&self) -> bool {
        (self.raw.flags & CLAP_TRANSPORT_IS_PLAYING) != 0
    }

    pub fn song_pos_seconds(&self) -> Option<f64> {
        if (self.raw.flags & CLAP_TRANSPORT_HAS_SECONDS_TIMELINE) != 0 {
            let value = fixed_to_f64(self.raw.song_pos_seconds, CLAP_SECTIME_FACTOR);
            value.is_finite().then_some(value)
        } else {
            None
        }
    }

    pub fn song_pos_beats(&self) -> Option<f64> {
        if (self.raw.flags & CLAP_TRANSPORT_HAS_BEATS_TIMELINE) != 0 {
            let value = fixed_to_f64(self.raw.song_pos_beats, CLAP_BEATTIME_FACTOR);
            value.is_finite().then_some(value)
        } else {
            None
        }
    }
}

fn fixed_to_f64(value: clap_sectime, factor: i64) -> f64 {
    value as f64 / factor as f64
}

pub struct InputEventIterator<'a> {
    events: *const clap_input_events_t,
    index: u32,
    count: u32,
    _marker: PhantomData<&'a ()>,
}

impl<'a> Iterator for InputEventIterator<'a> {
    type Item = Event;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.count || self.events.is_null() {
            return None;
        }

        unsafe {
            let get_fn = (*self.events).get?;
            let event_header = get_fn(self.events, self.index);
            self.index += 1;

            if event_header.is_null() {
                return None;
            }

            Some(Event::from_raw(event_header))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap_sys::process::{CLAP_PROCESS_CONTINUE, CLAP_PROCESS_ERROR};
    use std::ptr;

    fn audio_buffer_f32(channels: &mut [*mut f32]) -> clap_audio_buffer_t {
        clap_audio_buffer_t {
            data32: channels.as_mut_ptr(),
            data64: ptr::null_mut(),
            channel_count: channels.len() as u32,
            latency: 0,
            constant_mask: 0,
        }
    }

    fn audio_buffer_f64(channels: &mut [*mut f64]) -> clap_audio_buffer_t {
        clap_audio_buffer_t {
            data32: ptr::null_mut(),
            data64: channels.as_mut_ptr(),
            channel_count: channels.len() as u32,
            latency: 0,
            constant_mask: 0,
        }
    }

    fn process(
        frames_count: u32,
        inputs: &[clap_audio_buffer_t],
        outputs: &mut [clap_audio_buffer_t],
    ) -> clap_process_t {
        clap_process_t {
            steady_time: 0,
            frames_count,
            transport: ptr::null(),
            audio_inputs: inputs.as_ptr(),
            audio_outputs: outputs.as_mut_ptr(),
            audio_inputs_count: inputs.len() as u32,
            audio_outputs_count: outputs.len() as u32,
            in_events: ptr::null(),
            out_events: ptr::null(),
        }
    }

    #[test]
    fn every_channel_from_two_buses_is_flattened_in_port_order() {
        let mut input_0 = [1.0, 2.0, 3.0];
        let mut input_1_left = [4.0, 5.0, 6.0];
        let mut input_1_right = [7.0, 8.0, 9.0];
        let mut input_0_channels = [input_0.as_mut_ptr()];
        let mut input_1_channels = [input_1_left.as_mut_ptr(), input_1_right.as_mut_ptr()];
        let inputs = [
            audio_buffer_f32(&mut input_0_channels),
            audio_buffer_f32(&mut input_1_channels),
        ];

        let mut output_0 = [0.0; 3];
        let mut output_1_left = [0.0; 3];
        let mut output_1_right = [0.0; 3];
        let mut output_0_channels = [output_0.as_mut_ptr()];
        let mut output_1_channels = [output_1_left.as_mut_ptr(), output_1_right.as_mut_ptr()];
        let mut outputs = [
            audio_buffer_f32(&mut output_0_channels),
            audio_buffer_f32(&mut output_1_channels),
        ];
        let process = process(3, &inputs, &mut outputs);

        let mut buffers = ProcessBuffers::new(vec![1, 2], vec![1, 2]);
        assert!(buffers.activate(3));
        let status = unsafe {
            buffers.process(&process, |mut context| {
                assert_eq!(context.audio_inputs.len(), 3);
                assert_eq!(context.audio_outputs.len(), 3);
                assert_eq!(&context.audio_inputs[0], &[1.0, 2.0, 3.0]);
                assert_eq!(&context.audio_inputs[1], &[4.0, 5.0, 6.0]);
                assert_eq!(&context.audio_inputs[2], &[7.0, 8.0, 9.0]);

                for (index, (input, output)) in context
                    .audio_inputs
                    .iter()
                    .zip(context.audio_outputs.iter_mut())
                    .enumerate()
                {
                    for (input, output) in input.iter().zip(output.iter_mut()) {
                        *output = *input * (index + 1) as f32;
                    }
                }
                CLAP_PROCESS_CONTINUE
            })
        }
        .unwrap();

        assert_eq!(status, CLAP_PROCESS_CONTINUE);
        assert_eq!(output_0, [1.0, 2.0, 3.0]);
        assert_eq!(output_1_left, [8.0, 10.0, 12.0]);
        assert_eq!(output_1_right, [21.0, 24.0, 27.0]);
    }

    #[test]
    fn data64_is_converted_for_processing_and_written_back() {
        let mut input_left = [0.125_f64, -0.5, 1.25];
        let mut input_right = [0.25_f64, 0.75, -1.5];
        let mut input_channels = [input_left.as_mut_ptr(), input_right.as_mut_ptr()];
        let inputs = [audio_buffer_f64(&mut input_channels)];

        let mut output_left = [99.0_f64; 3];
        let mut output_right = [99.0_f64; 3];
        let mut output_channels = [output_left.as_mut_ptr(), output_right.as_mut_ptr()];
        let mut outputs = [audio_buffer_f64(&mut output_channels)];
        let process = process(3, &inputs, &mut outputs);

        let mut buffers = ProcessBuffers::new(vec![2], vec![2]);
        assert!(buffers.activate(8));
        let status = unsafe {
            buffers.process(&process, |mut context| {
                for (input, output) in context
                    .audio_inputs
                    .iter()
                    .zip(context.audio_outputs.iter_mut())
                {
                    for (input, output) in input.iter().zip(output.iter_mut()) {
                        *output = *input * 2.0;
                    }
                }
                CLAP_PROCESS_CONTINUE
            })
        }
        .unwrap();

        assert_eq!(status, CLAP_PROCESS_CONTINUE);
        assert_eq!(output_left, [0.25, -1.0, 2.5]);
        assert_eq!(output_right, [0.5, 1.5, -3.0]);
    }

    #[test]
    fn channel_views_reuse_activation_owned_storage() {
        let mut input = [1.0_f32, 2.0, 3.0, 4.0];
        let mut input_channels = [input.as_mut_ptr()];
        let inputs = [audio_buffer_f32(&mut input_channels)];
        let mut output = [0.0_f32; 4];
        let mut output_channels = [output.as_mut_ptr()];
        let mut outputs = [audio_buffer_f32(&mut output_channels)];

        let mut buffers = ProcessBuffers::new(vec![1], vec![1]);
        assert!(buffers.activate(4));
        let input_ptr = buffers.input_channels[0].as_ptr();
        let output_ptr = buffers.output_channels[0].as_ptr();

        for frames_count in [4, 2, 4] {
            let process = process(frames_count, &inputs, &mut outputs);
            unsafe {
                buffers.process(&process, |mut context| {
                    assert_eq!((&context.audio_inputs[0]).as_ptr(), input_ptr);
                    assert_eq!(context.audio_outputs[0].as_ptr(), output_ptr);
                    context.audio_outputs[0].copy_from_slice(&context.audio_inputs[0]);
                    CLAP_PROCESS_CONTINUE
                })
            }
            .unwrap();
        }

        assert_eq!(buffers.input_channels[0].as_ptr(), input_ptr);
        assert_eq!(buffers.output_channels[0].as_ptr(), output_ptr);
        assert_eq!(output, input);
    }

    #[test]
    fn invalid_sample_layout_returns_process_error_without_callback() {
        let mut input = [1.0_f32; 2];
        let mut input64 = [1.0_f64; 2];
        let mut input_channels = [input.as_mut_ptr()];
        let mut input64_channels = [input64.as_mut_ptr()];
        let mut invalid_input = audio_buffer_f32(&mut input_channels);
        invalid_input.data64 = input64_channels.as_mut_ptr();
        let inputs = [invalid_input];

        let mut output = [7.0_f32; 2];
        let mut output_channels = [output.as_mut_ptr()];
        let mut outputs = [audio_buffer_f32(&mut output_channels)];
        let process = process(2, &inputs, &mut outputs);
        let mut buffers = ProcessBuffers::new(vec![1], vec![1]);
        assert!(buffers.activate(2));
        let mut called = false;

        let status = unsafe {
            buffers.process(&process, |_| {
                called = true;
                CLAP_PROCESS_CONTINUE
            })
        }
        .unwrap_or(CLAP_PROCESS_ERROR);

        assert_eq!(status, CLAP_PROCESS_ERROR);
        assert!(!called);
        assert_eq!(output, [7.0, 7.0]);
    }

    #[test]
    fn zero_channel_buses_may_omit_channel_storage() {
        let inputs = [clap_audio_buffer_t {
            data32: ptr::null_mut(),
            data64: ptr::null_mut(),
            channel_count: 0,
            latency: 0,
            constant_mask: 0,
        }];
        let mut outputs = inputs;
        let process = process(4, &inputs, &mut outputs);
        let mut buffers = ProcessBuffers::new(vec![0], vec![0]);
        assert!(buffers.activate(4));

        let status = unsafe {
            buffers.process(&process, |context| {
                assert!(context.audio_inputs.is_empty());
                assert!(context.audio_outputs.is_empty());
                CLAP_PROCESS_CONTINUE
            })
        };
        assert_eq!(status, Ok(CLAP_PROCESS_CONTINUE));
    }

    #[test]
    fn zero_frame_blocks_do_not_dereference_channel_pointers() {
        let mut input_channels = [ptr::null_mut()];
        let inputs = [audio_buffer_f32(&mut input_channels)];
        let mut output_channels = [ptr::null_mut()];
        let mut outputs = [audio_buffer_f32(&mut output_channels)];
        let process = process(0, &inputs, &mut outputs);
        let mut buffers = ProcessBuffers::new(vec![1], vec![1]);
        assert!(buffers.activate(8));

        let status = unsafe {
            buffers.process(&process, |context| {
                assert!(context.audio_inputs[0].is_empty());
                assert!(context.audio_outputs[0].is_empty());
                CLAP_PROCESS_CONTINUE
            })
        };
        assert_eq!(status, Ok(CLAP_PROCESS_CONTINUE));
    }

    #[test]
    fn zero_channel_buses_may_advertise_a_sample_format() {
        let mut no_input_channels: [*mut f32; 0] = [];
        let inputs = [audio_buffer_f32(&mut no_input_channels)];
        let mut no_output_channels: [*mut f32; 0] = [];
        let mut outputs = [audio_buffer_f32(&mut no_output_channels)];
        let process = process(1, &inputs, &mut outputs);
        let mut buffers = ProcessBuffers::new(vec![0], vec![0]);
        assert!(buffers.activate(1));

        let status = unsafe { buffers.process(&process, |_| CLAP_PROCESS_CONTINUE) };
        assert_eq!(status, Ok(CLAP_PROCESS_CONTINUE));
    }

    #[test]
    fn activation_rejects_oversized_host_block_before_allocation() {
        let mut buffers = ProcessBuffers::new(Vec::new(), Vec::new());
        assert!(buffers.activate(MAX_PROCESS_FRAMES));
        buffers.deactivate();
        assert!(!buffers.activate(MAX_PROCESS_FRAMES + 1));
        assert!(!buffers.active);
    }

    #[test]
    fn activation_rejects_oversized_channel_and_sample_budgets_before_allocation() {
        let mut too_many_channels =
            ProcessBuffers::new(vec![MAX_PROCESS_CHANNELS as u32 + 1], Vec::new());
        assert!(!too_many_channels.activate(1));
        assert!(!too_many_channels.active);

        let channels_over_sample_budget =
            (MAX_PROCESS_AUDIO_SAMPLES / MAX_PROCESS_FRAMES as usize + 1) as u32;
        let mut too_many_samples =
            ProcessBuffers::new(Vec::new(), vec![channels_over_sample_budget]);
        assert!(!too_many_samples.activate(MAX_PROCESS_FRAMES));
        assert!(!too_many_samples.active);
    }
}
