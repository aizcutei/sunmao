#![allow(unsafe_op_in_unsafe_fn)]

use au_sys::{
    AuComponentDescriptor, AuPlugin, BufferList, ParameterInfo, ParameterUnit, export_au_component,
    fourcc,
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

pub struct GainEffectGui {
    gain: f32,
}

impl AuPlugin for GainEffectGui {
    fn new(_sample_rate: f64, _max_frames: u32) -> Self {
        cocoa_ui::init_cocoa_view_factory();
        Self { gain: 1.0 }
    }

    fn process(
        &mut self,
        mut inputs: Option<BufferList<'_>>,
        outputs: &mut BufferList<'_>,
        frames: usize,
    ) {
        let channels = outputs.len();
        for ch in 0..channels {
            let out = unsafe { outputs.channel_mut(ch) };
            let in_buf = inputs
                .as_mut()
                .map(|input| unsafe { input.channel_mut(ch) });
            for i in 0..frames.min(out.len()) {
                let sample = in_buf
                    .as_ref()
                    .and_then(|buf| buf.get(i))
                    .copied()
                    .unwrap_or(0.0);
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

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
mod cocoa_ui {
    use crate::PARAM_GAIN;
    use au_sys::{
        AudioUnitCocoaViewInfo, AudioUnitSetParameter, CocoaViewConfig, NSPoint, NSRect, NSSize,
        ViewCallbacks, cocoa_view_info as au_cocoa_view_info, gl_get_proc_address,
        init_cocoa_view_factory as au_init_cocoa_view_factory, kAudioUnitScope_Global,
        set_view_user_data,
    };
    use objc::runtime::{Object, YES};
    use objc::{class, msg_send, sel, sel_impl};
    use std::sync::Once;

    static GL_LOADED: Once = Once::new();
    const NSOPENGLPFA_DOUBLEBUFFER: u32 = 5;
    const NSOPENGLPFA_ACCELERATED: u32 = 73;
    const NSOPENGLPFA_OPENGL_PROFILE: u32 = 99;
    const NSOPENGL_PROFILE_VERSION_3_2_CORE: u32 = 0x3200;
    struct GuiState {
        dragging: bool,
        start_x: f64,
        start_gain: f32,
        gain: f32,
        angle: f32,
    }

    struct ViewState {
        gui: GuiState,
        gl: Option<GlResources>,
    }

    struct GlResources {
        program: u32,
        vao: u32,
        vbo: u32,
        angle_uniform: i32,
    }

    pub fn init_cocoa_view_factory() {
        au_cocoa_stubs::ensure_linked();
        au_init_cocoa_view_factory(
            CocoaViewConfig {
                factory_class: "RustGainGuiViewFactory",
                view_class: "RustGainGuiView",
                view_superclass: "NSOpenGLView",
                description: "Rust Gain GUI",
                view_init: Some(view_init),
                preferred_size: Some(NSSize {
                    width: 400.0,
                    height: 200.0,
                }),
            },
            ViewCallbacks {
                draw: Some(draw_rect),
                reshape: Some(reshape),
                mouse_down: Some(mouse_down),
                mouse_dragged: Some(mouse_dragged),
                mouse_up: Some(mouse_up),
                key_down: Some(key_down),
                deinit: Some(deinit_view),
            },
        );
    }

    pub fn cocoa_view_info() -> AudioUnitCocoaViewInfo {
        au_cocoa_view_info()
    }

    fn view_init(view: *mut Object, _size: NSSize, _audio_unit: *mut std::ffi::c_void) {
        unsafe {
            let attrs = [
                NSOPENGLPFA_OPENGL_PROFILE,
                NSOPENGL_PROFILE_VERSION_3_2_CORE,
                NSOPENGLPFA_ACCELERATED,
                NSOPENGLPFA_DOUBLEBUFFER,
                0,
            ];
            let pixel_format: *mut Object = msg_send![class!(NSOpenGLPixelFormat), alloc];
            let pixel_format: *mut Object =
                msg_send![pixel_format, initWithAttributes: attrs.as_ptr()];
            let _: () = msg_send![view, setPixelFormat: pixel_format];
            let _: () = msg_send![view, setWantsBestResolutionOpenGLSurface: YES];
        }
        let state = Box::new(ViewState {
            gui: GuiState {
                dragging: false,
                start_x: 0.0,
                start_gain: 1.0,
                gain: 1.0,
                angle: 0.0,
            },
            gl: None,
        });
        set_view_user_data(view, Box::into_raw(state) as *mut _);
    }

    fn load_gl() {
        GL_LOADED.call_once(|| {
            gl::load_with(|name| gl_get_proc_address(name));
        });
    }

    fn draw_rect(
        view: *mut Object,
        _au: *mut std::ffi::c_void,
        user_data: *mut std::ffi::c_void,
        _rect: NSRect,
    ) {
        load_gl();
        unsafe {
            let ctx: *mut Object = msg_send![view, openGLContext];
            if !ctx.is_null() {
                let _: () = msg_send![ctx, makeCurrentContext];
            }
            let state = match view_state(user_data) {
                Some(state) => state,
                None => return,
            };
            let angle = state.gui.angle;
            let resources = ensure_resources(state);
            let bounds: NSRect = msg_send![view, bounds];
            let width = bounds.size.width.max(1.0) as i32;
            let height = bounds.size.height.max(1.0) as i32;

            gl::Viewport(0, 0, width, height);
            gl::ClearColor(0.08, 0.08, 0.1, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);

            gl::UseProgram(resources.program);
            if resources.angle_uniform >= 0 {
                gl::Uniform1f(resources.angle_uniform, angle);
            }
            gl::BindVertexArray(resources.vao);
            gl::DrawArrays(gl::TRIANGLES, 0, 3);

            gl::Flush();
            if !ctx.is_null() {
                let _: () = msg_send![ctx, flushBuffer];
            }
        }
    }

    fn reshape(view: *mut Object, _au: *mut std::ffi::c_void, _user_data: *mut std::ffi::c_void) {
        load_gl();
        unsafe {
            let _: () = msg_send![view, setNeedsDisplay: YES];
        }
    }

    fn mouse_down(
        view: *mut Object,
        _au: *mut std::ffi::c_void,
        user_data: *mut std::ffi::c_void,
        point: NSPoint,
        _flags: u64,
    ) {
        if let Some(state) = view_state(user_data) {
            state.gui.dragging = true;
            state.gui.start_x = point.x;
            state.gui.start_gain = state.gui.gain;
        }
        unsafe {
            let _: () = msg_send![view, setNeedsDisplay: YES];
        }
    }

    fn mouse_dragged(
        view: *mut Object,
        au: *mut std::ffi::c_void,
        user_data: *mut std::ffi::c_void,
        point: NSPoint,
        _flags: u64,
    ) {
        let mut next_gain = None;
        if let Some(state) = view_state(user_data) {
            if state.gui.dragging {
                let delta = (point.x - state.gui.start_x) as f32 / 200.0;
                let gain = (state.gui.start_gain + delta).clamp(0.0, 2.0);
                state.gui.gain = gain;
                state.gui.angle = (gain - 1.0) * 1.5;
                next_gain = Some(gain);
            }
        }
        if let Some(gain) = next_gain {
            unsafe {
                if !au.is_null() {
                    let _ = AudioUnitSetParameter(
                        au as *mut _,
                        PARAM_GAIN,
                        kAudioUnitScope_Global,
                        0,
                        gain,
                        0,
                    );
                }
                let _: () = msg_send![view, setNeedsDisplay: YES];
            }
        }
    }

    fn mouse_up(
        view: *mut Object,
        _au: *mut std::ffi::c_void,
        user_data: *mut std::ffi::c_void,
        _point: NSPoint,
        _flags: u64,
    ) {
        if let Some(state) = view_state(user_data) {
            state.gui.dragging = false;
        }
        unsafe {
            let _: () = msg_send![view, setNeedsDisplay: YES];
        }
    }

    fn key_down(
        view: *mut Object,
        au: *mut std::ffi::c_void,
        user_data: *mut std::ffi::c_void,
        key_code: u16,
        _flags: u64,
    ) {
        if key_code == 53 {
            if let Some(state) = view_state(user_data) {
                state.gui.gain = 1.0;
                state.gui.angle = 0.0;
            }
            unsafe {
                if !au.is_null() {
                    let _ = AudioUnitSetParameter(
                        au as *mut _,
                        PARAM_GAIN,
                        kAudioUnitScope_Global,
                        0,
                        1.0,
                        0,
                    );
                }
                let _: () = msg_send![view, setNeedsDisplay: YES];
            }
        }
    }

    unsafe fn ensure_resources(state: &mut ViewState) -> &mut GlResources {
        if state.gl.is_none() {
            state.gl = Some(init_resources());
        }
        state.gl.as_mut().unwrap()
    }

    unsafe fn init_resources() -> GlResources {
        let program = create_program();
        let angle_uniform = gl::GetUniformLocation(program, b"u_angle\0".as_ptr() as *const _);
        let mut vao = 0;
        let mut vbo = 0;
        let vertices: [f32; 6] = [0.0, 0.6, -0.6, -0.6, 0.6, -0.6];

        gl::GenVertexArrays(1, &mut vao);
        gl::BindVertexArray(vao);
        gl::GenBuffers(1, &mut vbo);
        gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
        gl::BufferData(
            gl::ARRAY_BUFFER,
            (vertices.len() * std::mem::size_of::<f32>()) as isize,
            vertices.as_ptr() as *const _,
            gl::STATIC_DRAW,
        );
        gl::EnableVertexAttribArray(0);
        gl::VertexAttribPointer(
            0,
            2,
            gl::FLOAT,
            gl::FALSE,
            (2 * std::mem::size_of::<f32>()) as i32,
            std::ptr::null(),
        );

        GlResources {
            program,
            vao,
            vbo,
            angle_uniform,
        }
    }

    unsafe fn create_program() -> u32 {
        let vertex_source = b"#version 150\n\
in vec2 position;\n\
uniform float u_angle;\n\
void main() {\n\
    float c = cos(u_angle);\n\
    float s = sin(u_angle);\n\
    vec2 p = vec2(position.x * c - position.y * s, position.x * s + position.y * c);\n\
    gl_Position = vec4(p, 0.0, 1.0);\n\
}\n";
        let fragment_source = b"#version 150\n\
out vec4 color;\n\
void main() {\n\
    color = vec4(1.0, 0.6, 0.2, 1.0);\n\
}\n";

        let vertex_shader = compile_shader(gl::VERTEX_SHADER, vertex_source);
        let fragment_shader = compile_shader(gl::FRAGMENT_SHADER, fragment_source);

        let program = gl::CreateProgram();
        gl::AttachShader(program, vertex_shader);
        gl::AttachShader(program, fragment_shader);
        gl::BindAttribLocation(program, 0, b"position\0".as_ptr() as *const _);
        gl::LinkProgram(program);
        gl::DeleteShader(vertex_shader);
        gl::DeleteShader(fragment_shader);

        program
    }

    unsafe fn compile_shader(kind: u32, source: &[u8]) -> u32 {
        let shader = gl::CreateShader(kind);
        let source_ptr = source.as_ptr() as *const i8;
        let length = source.len() as i32;
        gl::ShaderSource(shader, 1, &source_ptr, &length);
        gl::CompileShader(shader);
        shader
    }

    fn view_state<'a>(user_data: *mut std::ffi::c_void) -> Option<&'a mut ViewState> {
        if user_data.is_null() {
            return None;
        }
        Some(unsafe { &mut *(user_data as *mut ViewState) })
    }

    fn deinit_view(_view: *mut Object, user_data: *mut std::ffi::c_void) {
        if !user_data.is_null() {
            unsafe {
                let _ = Box::from_raw(user_data as *mut ViewState);
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod cocoa_ui {
    use au_sys::AudioUnitCocoaViewInfo;

    pub fn init_cocoa_view_factory() {}

    pub fn cocoa_view_info() -> AudioUnitCocoaViewInfo {
        unsafe { std::mem::zeroed() }
    }
}

export_au_component!(
    RustAUGuiFactory,
    GainEffectGui,
    AuComponentDescriptor {
        name: "Au Sys Fx Gain Gui Gl",
        component_type: fourcc(b"aufx"),
        component_subtype: fourcc(b"sgg1"),
        manufacturer: fourcc(b"RUST"),
        version: 0x0001_0000,
        flags: 0,
        flags_mask: 0,
        input_channels: 2,
        output_channels: 2,
        supports_midi: false,
        parameters: &PARAMETERS,
        cocoa_view_info: Some(cocoa_ui::cocoa_view_info),
        cocoa_view_class: None,
        cocoa_view_bundle_id: None,
        cocoa_view_init: None,
    }
);
