//! VST3 Gain Effect using vst3_rs wrapper
//!
//! This example demonstrates how simple it is to create a VST3 plugin
//! using the vst3_rs safety wrapper.

use vst3_rs::*;

/// Simple Gain Plugin
struct MyGain {
    gain: f64,
}

impl Plugin for MyGain {
    fn info() -> PluginInfo {
        PluginInfo {
            id: "com.sunmao.vst3_rs_fx_gain",
            name: "Vst3 Rs Fx Gain",
            vendor: "aizcutei",
            url: "https://aizcutei.github.io/sunmao",
            email: "info@example.com",
            version: "0.1.0",
            category: "Fx",
        }
    }

    fn new(_host: HostHandle) -> Self {
        Self { gain: 1.0 }
    }

    fn params() -> Vec<ParamInfo> {
        vec![
            ParamInfo::new(0, "Gain")
                .range(0.0, 2.0)
                .default(0.5)
                .units(""),
        ]
    }

    fn get_param(&self, _id: u32) -> f64 {
        // Normalize: 0-2 range to 0-1
        self.gain / 2.0
    }

    fn set_param(&mut self, _id: u32, value: f64) {
        // Denormalize: 0-1 to 0-2
        self.gain = value * 2.0;
    }

    fn process(&mut self, ctx: &mut ProcessContext) -> ProcessResult {
        let gain = self.gain as f32;
        let num_samples = ctx.num_samples;

        for ch in 0..ctx.num_outputs().min(ctx.num_inputs()) {
            for sample in 0..num_samples {
                let input = ctx.input(ch)[sample];
                ctx.output_mut(ch)[sample] = input * gain;
            }
        }
        Ok(())
    }
}

// Export the plugin using the macro!
export_vst3_plugin!(MyGain);

#[cfg(test)]
use vst3_rs::vst3_sys as vst3_test_api;

#[cfg(test)]
#[path = "../../realtime_test_support.rs"]
mod realtime_test_support;

#[cfg(test)]
#[path = "../../vst3_callback_test_support.rs"]
mod vst3_callback_test_support;

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::c_void;
    use vst3_callback_test_support::ActiveProcessor;
    use vst3_test_api::*;

    #[test]
    fn direct_vst3_effect_callback_does_not_allocate() {
        unsafe {
            let mut processor = ActiveProcessor::new(__vst3_rs_impl::GetPluginFactory(), 16);
            let input_left = [1.0_f32; 16];
            let input_right = [0.5_f32; 16];
            let mut output_left = [0.0_f32; 16];
            let mut output_right = [0.0_f32; 16];
            let mut input_channels = [
                input_left.as_ptr() as *mut c_void,
                input_right.as_ptr() as *mut c_void,
            ];
            let mut output_channels = [
                output_left.as_mut_ptr() as *mut c_void,
                output_right.as_mut_ptr() as *mut c_void,
            ];
            let mut input = AudioBusBuffers {
                num_channels: 2,
                silence_flags: 0,
                buffers: input_channels.as_mut_ptr(),
            };
            let mut output = AudioBusBuffers {
                num_channels: 2,
                silence_flags: 0,
                buffers: output_channels.as_mut_ptr(),
            };
            let mut data = ProcessData {
                process_mode: 0,
                symbolic_sample_size: SymbolicSampleSizes::kSample32,
                num_samples: 16,
                num_inputs: 1,
                num_outputs: 1,
                inputs: &mut input,
                outputs: &mut output,
                input_parameter_changes: std::ptr::null_mut(),
                output_parameter_changes: std::ptr::null_mut(),
                input_events: std::ptr::null_mut(),
                output_events: std::ptr::null_mut(),
                process_context: std::ptr::null_mut(),
            };

            let (status, allocator_calls) =
                realtime_test_support::count_allocator_calls(|| processor.process(&mut data));
            assert_eq!(status, kResultOk);
            assert_eq!(allocator_calls, 0);
            assert_eq!(output_left, [1.0; 16]);
            assert_eq!(output_right, [0.5; 16]);
        }
    }
}
