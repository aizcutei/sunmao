use au_sys::{export_au_component, fourcc, AuComponentDescriptor, AuPlugin, BufferList, ParameterInfo, ParameterUnit};

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

impl AuPlugin for GainEffect {
    fn new(_sample_rate: f64, _max_frames: u32) -> Self {
        Self { gain: 1.0 }
    }

    fn process(&mut self, mut inputs: Option<BufferList<'_>>, outputs: &mut BufferList<'_>, frames: usize) {
        let channels = outputs.len();
        for ch in 0..channels {
            let out = unsafe { outputs.channel_mut(ch) };
            let in_buf = inputs.as_mut().map(|input| unsafe { input.channel_mut(ch) });
            for i in 0..frames.min(out.len()) {
                let sample = in_buf.as_ref().and_then(|buf| buf.get(i)).copied().unwrap_or(0.0);
                out[i] = sample * self.gain;
            }
        }
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

export_au_component!(
    RustAUFactory,
    GainEffect,
    AuComponentDescriptor {
        name: "Au Sys Fx Gain",
        component_type: fourcc(b"aufx"),
        component_subtype: fourcc(b"sgn0"),
        manufacturer: fourcc(b"RUST"),
        version: 0x0001_0000,
        flags: 0,
        flags_mask: 0,
        input_channels: 2,
        output_channels: 2,
        supports_midi: false,
        parameters: &PARAMETERS,
        cocoa_view_info: None,
        cocoa_view_class: None,
        cocoa_view_bundle_id: None,
        cocoa_view_init: None,
    }
);
