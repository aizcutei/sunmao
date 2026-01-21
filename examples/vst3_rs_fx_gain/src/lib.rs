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
                .units("")
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
    
    fn process(&mut self, ctx: &mut ProcessContext) {
        let gain = self.gain as f32;
        let num_samples = ctx.num_samples;
        
        // Process each channel - copy input to output with gain
        for ch in 0..ctx.num_outputs().min(ctx.num_inputs()) {
            // First read input into a temp buffer
            let input_copy: Vec<f32> = ctx.input(ch).iter().take(num_samples).copied().collect();
            
            // Then write to output
            let output = ctx.output_mut(ch);
            for (i, o) in input_copy.iter().zip(output.iter_mut()) {
                *o = *i * gain;
            }
        }
    }
}

// Export the plugin using the macro!
export_vst3_plugin!(MyGain);
