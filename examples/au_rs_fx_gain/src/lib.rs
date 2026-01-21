use au_rs::{
    BufferList, ParameterInfo, ParameterUnit, Plugin, PluginInfo, export_au_plugin, fourcc,
    for_each_channel,
};

const PARAM_GAIN: u32 = 0;

const PARAMETERS: [ParameterInfo; 1] = [ParameterInfo {
    id: PARAM_GAIN,
    name: "Gain",
    min: 0.0,
    max: 2.0,
    default: 1.0,
    unit: ParameterUnit::LinearGain,
}];

pub struct GainEffect {
    gain: f32,
}

impl Plugin for GainEffect {
    fn init(_sample_rate: f64, _max_frames: u32) -> Self {
        Self { gain: 1.0 }
    }

    fn process(
        &mut self,
        inputs: Option<BufferList<'_>>,
        outputs: &mut BufferList<'_>,
        frames: usize,
    ) {
        for_each_channel(inputs, outputs, frames, |input, output| {
            for (idx, out_sample) in output.iter_mut().enumerate() {
                let sample = input
                    .and_then(|buf| buf.get(idx))
                    .copied()
                    .unwrap_or(0.0);
                *out_sample = sample * self.gain;
            }
        });
    }

    fn parameters(&self) -> &'static [ParameterInfo] {
        &PARAMETERS
    }

    fn get_parameter(&self, id: u32) -> f32 {
        match id {
            PARAM_GAIN => self.gain,
            _ => 0.0,
        }
    }

    fn set_parameter(&mut self, id: u32, value: f32) {
        if id == PARAM_GAIN {
            self.gain = value.clamp(0.0, 2.0);
        }
    }
}

export_au_plugin!(
    RustAUFactory,
    GainEffect,
    PluginInfo {
        name: "Au Rs Fx Gain",
        component_type: fourcc(b"aufx"),
        component_subtype: fourcc(b"rgn0"),
        manufacturer: fourcc(b"RUST"),
        version: 0x0001_0000,
        flags: 0,
        flags_mask: 0,
        input_channels: 2,
        output_channels: 2,
        supports_midi: false,
    },
    &PARAMETERS
);
