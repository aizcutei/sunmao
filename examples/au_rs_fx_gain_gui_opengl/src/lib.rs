use au_rs::{
    BufferList, ParameterInfo, ParameterUnit, Plugin, PluginInfo, export_au_plugin,
    for_each_channel, fourcc,
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

pub struct GainEffectOpenGl {
    gain: f32,
}

impl Plugin for GainEffectOpenGl {
    fn init(_sample_rate: f64, _max_frames: u32) -> Self {
        Self { gain: 1.0 }
    }

    fn process(
        &mut self,
        mut inputs: Option<BufferList<'_>>,
        outputs: &mut BufferList<'_>,
        frames: usize,
    ) {
        if inputs.is_none() {
            // In-place processing: output already contains input.
            for ch in 0..outputs.len() {
                let out = unsafe { outputs.channel_mut(ch) };
                let out_len = frames.min(out.len());
                for sample in out[..out_len].iter_mut() {
                    *sample *= self.gain;
                }
            }
            return;
        }
        for_each_channel(inputs, outputs, frames, |input, output| {
            for (idx, out_sample) in output.iter_mut().enumerate() {
                let sample = input.and_then(|buf| buf.get(idx)).copied().unwrap_or(0.0);
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

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
mod gui {
    use std::ffi::c_void;

    use au_rs::{
        NSPoint, NSRect, NSSize, get_parameter_local, glow_safe,
        gui::{
            GuiConfig, GuiHandler, flush_context, make_current_context, open_gl_context,
            set_best_resolution, set_needs_display, set_pixel_format, update_open_gl_view,
            view_backing_bounds, view_bounds,
        },
        set_parameter_local,
    };
    use objc::runtime::Object;

    use crate::PARAM_GAIN;

    const NSOPENGLPFA_DOUBLEBUFFER: u32 = 5;
    const NSOPENGLPFA_ACCELERATED: u32 = 73;
    const NSOPENGLPFA_OPENGL_PROFILE: u32 = 99;
    const NSOPENGL_PROFILE_VERSION_3_2_CORE: u32 = 0x3200;

    pub struct GainOpenGlGui {
        dragging: bool,
        gain: f32,
        width: f64,
        height: f64,
        gl: Option<GlResources>,
        ctx: Option<glow_safe::GlowCtx>,
    }

    struct GlResources {
        program: glow_safe::Program,
        vao: glow_safe::VertexArray,
        vbo: glow_safe::Buffer,
        gain_uniform: Option<glow_safe::UniformLocation>,
        aspect_uniform: Option<glow_safe::UniformLocation>,
    }

    impl Default for GainOpenGlGui {
        fn default() -> Self {
            Self {
                dragging: false,
                gain: 1.0,
                width: 400.0,
                height: 100.0,
                gl: None,
                ctx: None,
            }
        }
    }

    impl GainOpenGlGui {
        fn set_gain(&mut self, view: *mut Object, au: *mut c_void, gain: f32) {
            self.gain = gain.clamp(0.0, 2.0);
            if !au.is_null() {
                let _ = set_parameter_local(au, PARAM_GAIN, self.gain);
            }
            set_needs_display(view);
        }

        fn gain_from_x(&self, x: f64) -> f32 {
            let margin = 20.0;
            let slider_width = (self.width - 2.0 * margin).max(1.0);
            let local_x = (x - margin).clamp(0.0, slider_width);
            ((local_x / slider_width * 2.0) as f32).clamp(0.0, 2.0)
        }
    }

    impl GuiHandler for GainOpenGlGui {
        fn init(&mut self, view: *mut Object, size: NSSize, audio_unit: *mut c_void) {
            self.width = size.width;
            self.height = size.height;

            let attrs = [
                NSOPENGLPFA_OPENGL_PROFILE,
                NSOPENGL_PROFILE_VERSION_3_2_CORE,
                NSOPENGLPFA_ACCELERATED,
                NSOPENGLPFA_DOUBLEBUFFER,
                0,
            ];
            set_pixel_format(view, &attrs);
            set_best_resolution(view, true);

            // Get initial gain
            if !audio_unit.is_null() {
                if let Ok(value) = get_parameter_local(audio_unit, PARAM_GAIN) {
                    self.gain = value.clamp(0.0, 2.0);
                }
            }
        }

        fn draw(&mut self, view: *mut Object, _audio_unit: *mut c_void, _rect: NSRect) {
            let ctx = open_gl_context(view);
            make_current_context(ctx);

            if self.ctx.is_none() {
                self.ctx = Some(glow_safe::GlowCtx::new());
            }
            if self.gl.is_none() {
                let gl_ctx = self.ctx.as_ref().expect("GL context missing");
                match init_resources(gl_ctx) {
                    Ok(res) => self.gl = Some(res),
                    Err(e) => {
                        eprintln!("Failed to init GL resources: {}", e);
                    }
                }
            }

            if let Some(gl) = self.ctx.as_ref() {
                // Always clear background
                let bounds: NSRect = view_backing_bounds(view);
                let width = bounds.size.width.max(1.0) as i32;
                let height = bounds.size.height.max(1.0) as i32;

                gl.viewport(0, 0, width, height);
                // Vibrant dark blue background
                gl.clear_color(0.12, 0.12, 0.18, 1.0);
                gl.clear(glow_safe::COLOR_BUFFER_BIT);

                if let Some(resources) = self.gl.as_ref() {
                    let aspect = width as f32 / height as f32;

                    gl.enable(glow_safe::BLEND);
                    gl.blend_func(glow_safe::SRC_ALPHA, glow_safe::ONE_MINUS_SRC_ALPHA);

                    gl.use_program(Some(resources.program));

                    let normalized_gain = (self.gain / 2.0).clamp(0.0, 1.0);

                    gl.uniform1f(resources.gain_uniform.as_ref(), normalized_gain);
                    gl.uniform1f(resources.aspect_uniform.as_ref(), aspect);

                    gl.bind_vertex_array(Some(resources.vao));
                    gl.draw_arrays(glow_safe::TRIANGLES, 0, 6);
                }

                gl.flush();
                flush_context(ctx);
            }
        }

        fn reshape(&mut self, view: *mut Object, _audio_unit: *mut c_void) {
            update_open_gl_view(view);
            // view_bounds gives us logical size for interaction calculations
            let bounds = view_bounds(view);
            self.width = bounds.size.width;
            self.height = bounds.size.height;
            set_needs_display(view);
        }

        fn mouse_down(
            &mut self,
            view: *mut Object,
            audio_unit: *mut c_void,
            point: NSPoint,
            _flags: u64,
        ) {
            self.dragging = true;
            let gain = self.gain_from_x(point.x);
            self.set_gain(view, audio_unit, gain);
        }

        fn mouse_dragged(
            &mut self,
            view: *mut Object,
            audio_unit: *mut c_void,
            point: NSPoint,
            _flags: u64,
        ) {
            if self.dragging {
                let gain = self.gain_from_x(point.x);
                self.set_gain(view, audio_unit, gain);
            }
        }

        fn mouse_up(
            &mut self,
            view: *mut Object,
            _audio_unit: *mut c_void,
            _point: NSPoint,
            _flags: u64,
        ) {
            self.dragging = false;
            set_needs_display(view);
        }

        fn key_down(
            &mut self,
            view: *mut Object,
            audio_unit: *mut c_void,
            key_code: u16,
            _flags: u64,
        ) {
            if key_code == 53 {
                self.set_gain(view, audio_unit, 1.0);
            }
        }
    }

    pub fn gui_config() -> GuiConfig {
        GuiConfig {
            factory_class: "RustAUFactory",
            view_class: "RustGainGuiView",
            view_superclass: "NSOpenGLView",
            description: "Rust Gain (OpenGL)",
            preferred_size: Some(NSSize {
                width: 400.0,
                height: 100.0,
            }),
        }
    }

    fn init_resources(gl: &glow_safe::GlowCtx) -> Result<GlResources, String> {
        let program = create_program(gl)?;
        let gain_uniform = gl.get_uniform_location(program, "u_gain");
        let aspect_uniform = gl.get_uniform_location(program, "u_aspect");
        let vao = gl.create_vertex_array();
        let vbo = gl.create_buffer();

        // Full screen quad
        #[rustfmt::skip]
        let vertices: [f32; 12] = [
             -1.0, -1.0,
              1.0, -1.0, 
             -1.0,  1.0, 
             -1.0,  1.0, 
              1.0, -1.0, 
              1.0,  1.0
        ];

        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow_safe::ARRAY_BUFFER, Some(vbo));
        gl.buffer_data_f32(glow_safe::ARRAY_BUFFER, &vertices, glow_safe::STATIC_DRAW);
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, (2 * std::mem::size_of::<f32>()) as i32, 0);

        Ok(GlResources {
            program,
            vao,
            vbo,
            gain_uniform,
            aspect_uniform,
        })
    }

    fn create_program(gl: &glow_safe::GlowCtx) -> Result<glow_safe::Program, String> {
        let vertex_source = b"#version 150
in vec2 position;
out vec2 v_uv;

void main() {
    v_uv = position * 0.5 + 0.5;
    gl_Position = vec4(position, 0.0, 1.0);
}
";
        let fragment_source = b"#version 150
uniform float u_gain;   // 0.0 to 1.0
uniform float u_aspect; // width / height
in vec2 v_uv;
out vec4 color;

void main() {
    vec2 uv = v_uv;
    vec2 p = uv * 2.0 - 1.0;
    
    // Scale X-coordinates by aspect ratio so our geometry isn't stretched
    // p.x now represents typical 'square' coordinates horizontally
    p.x *= u_aspect;
    
    // Track params
    float track_height = 0.1; 
    float track_corner = 0.05;
    float track_width = 1.6; // total width in normalized space
    
    // Normalized track SDF (centered)
    vec2 p_track = p;
    // Box size: width/2, height/2
    vec2 track_size = vec2(track_width/2.0, track_height/2.0);
    vec2 d_track = abs(p_track) - track_size;
    float track_dist = length(max(d_track, 0.0)) + min(max(d_track.x, d_track.y), 0.0) - track_corner;
    
    // Knob params
    float knob_radius = 0.12;
    
    // Knob position calculation
    // Map u_gain (0-1) to track range (-0.8 to 0.8)
    float knob_x = (u_gain - 0.5) * track_width;
    
    vec2 p_knob = p;
    p_knob.x -= knob_x;
    
    float knob_dist = length(p_knob) - knob_radius;
    
    // Rendering
    float alpha_track = 1.0 - smoothstep(-0.01, 0.01, track_dist);
    float alpha_knob = 1.0 - smoothstep(-0.01, 0.01, knob_dist);
    
    vec4 track_color = vec4(0.3, 0.3, 0.35, 1.0);
    vec4 knob_color = vec4(0.2, 0.7, 1.0, 1.0);
    
    vec4 c = vec4(0.0);
    c = mix(c, track_color, alpha_track);
    c = mix(c, knob_color, alpha_knob);
    
    color = c;
}
";

        let vertex_shader = compile_shader(gl, glow_safe::VERTEX_SHADER, vertex_source)?;
        let fragment_shader = compile_shader(gl, glow_safe::FRAGMENT_SHADER, fragment_source)?;

        let program = gl.create_program();
        gl.attach_shader(program, vertex_shader);
        gl.attach_shader(program, fragment_shader);
        gl.bind_attrib_location(program, 0, "position");
        gl.link_program(program);

        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            return Err(format!("Program link error: {}", log));
        }

        gl.delete_shader(vertex_shader);
        gl.delete_shader(fragment_shader);

        Ok(program)
    }

    fn compile_shader(
        gl: &glow_safe::GlowCtx,
        kind: u32,
        source: &[u8],
    ) -> Result<glow_safe::Shader, String> {
        let shader = gl.create_shader(kind);
        gl.shader_source(shader, std::str::from_utf8(source).unwrap_or_default());
        gl.compile_shader(shader);

        if !gl.get_shader_compile_status(shader) {
            let log = gl.get_shader_info_log(shader);
            return Err(format!("Shader compile error: {}", log));
        }

        Ok(shader)
    }
}

#[cfg(not(target_os = "macos"))]
mod gui {
    use au_rs::gui::GuiConfig;

    pub struct GainOpenGlGui;

    impl Default for GainOpenGlGui {
        fn default() -> Self {
            GainOpenGlGui
        }
    }

    pub fn gui_config() -> GuiConfig {
        GuiConfig {
            factory_class: "RustAUFactory",
            view_class: "RustGainGuiView",
            view_superclass: "NSView",
            description: "Rust Gain (OpenGL)",
            preferred_size: None,
        }
    }
}

export_au_plugin!(
    RustAUFactory,
    GainEffectOpenGl,
    PluginInfo {
        name: "Au Rs Fx Gain Gui Opengl",
        component_type: fourcc(b"aufx"),
        component_subtype: fourcc(b"rgg1"),
        manufacturer: fourcc(b"RUST"),
        version: 0x0001_0000,
        flags: 0,
        flags_mask: 0,
        input_channels: 2,
        output_channels: 2,
        supports_midi: false,
    },
    &PARAMETERS,
    gui: { handler: gui::GainOpenGlGui, config: gui::gui_config() }
);
