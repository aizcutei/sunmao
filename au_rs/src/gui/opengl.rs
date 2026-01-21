pub mod glow_safe {
    pub use glow::HasContext;
    pub const ARRAY_BUFFER: u32 = glow::ARRAY_BUFFER;
    pub const STATIC_DRAW: u32 = glow::STATIC_DRAW;
    pub const TRIANGLES: u32 = glow::TRIANGLES;
    pub const COLOR_BUFFER_BIT: u32 = glow::COLOR_BUFFER_BIT;
    pub const VERTEX_SHADER: u32 = glow::VERTEX_SHADER;
    pub const FRAGMENT_SHADER: u32 = glow::FRAGMENT_SHADER;
    pub const BLEND: u32 = glow::BLEND;
    pub const SRC_ALPHA: u32 = glow::SRC_ALPHA;
    pub const ONE_MINUS_SRC_ALPHA: u32 = glow::ONE_MINUS_SRC_ALPHA;

    pub type Program = glow::Program;
    pub type VertexArray = glow::VertexArray;
    pub type Buffer = glow::Buffer;
    pub type UniformLocation = glow::UniformLocation;
    pub type Shader = glow::Shader;

    pub struct GlowCtx {
        ctx: glow::Context,
    }

    impl GlowCtx {
        pub fn new() -> Self {
            let ctx = unsafe {
                glow::Context::from_loader_function(|name| {
                    crate::gl_get_proc_address(name) as *const _
                })
            };
            Self { ctx }
        }

        pub fn raw(&self) -> &glow::Context {
            &self.ctx
        }

        pub fn viewport(&self, x: i32, y: i32, width: i32, height: i32) {
            unsafe { self.ctx.viewport(x, y, width, height) };
        }

        pub fn clear_color(&self, r: f32, g: f32, b: f32, a: f32) {
            unsafe { self.ctx.clear_color(r, g, b, a) };
        }

        pub fn clear(&self, mask: u32) {
            unsafe { self.ctx.clear(mask) };
        }

        pub fn use_program(&self, program: Option<Program>) {
            unsafe { self.ctx.use_program(program) };
        }

        pub fn uniform1f(&self, location: Option<&UniformLocation>, value: f32) {
            unsafe { self.ctx.uniform_1_f32(location, value) };
        }

        pub fn bind_vertex_array(&self, vao: Option<VertexArray>) {
            unsafe { self.ctx.bind_vertex_array(vao) };
        }

        pub fn draw_arrays(&self, mode: u32, first: i32, count: i32) {
            unsafe { self.ctx.draw_arrays(mode, first, count) };
        }

        pub fn flush(&self) {
            unsafe { self.ctx.flush() };
        }

        pub fn create_program(&self) -> Program {
            unsafe { self.ctx.create_program().expect("create_program failed") }
        }

        pub fn create_shader(&self, kind: u32) -> glow::Shader {
            unsafe { self.ctx.create_shader(kind).expect("create_shader failed") }
        }

        pub fn shader_source(&self, shader: glow::Shader, source: &str) {
            unsafe { self.ctx.shader_source(shader, source) };
        }

        pub fn compile_shader(&self, shader: glow::Shader) {
            unsafe { self.ctx.compile_shader(shader) };
        }

        pub fn attach_shader(&self, program: Program, shader: glow::Shader) {
            unsafe { self.ctx.attach_shader(program, shader) };
        }

        pub fn link_program(&self, program: Program) {
            unsafe { self.ctx.link_program(program) };
        }

        pub fn delete_shader(&self, shader: glow::Shader) {
            unsafe { self.ctx.delete_shader(shader) };
        }

        pub fn bind_attrib_location(&self, program: Program, index: u32, name: &str) {
            unsafe { self.ctx.bind_attrib_location(program, index, name) };
        }

        pub fn get_uniform_location(
            &self,
            program: Program,
            name: &str,
        ) -> Option<UniformLocation> {
            unsafe { self.ctx.get_uniform_location(program, name) }
        }

        pub fn create_vertex_array(&self) -> VertexArray {
            unsafe {
                self.ctx
                    .create_vertex_array()
                    .expect("create_vertex_array failed")
            }
        }

        pub fn create_buffer(&self) -> Buffer {
            unsafe { self.ctx.create_buffer().expect("create_buffer failed") }
        }

        pub fn bind_buffer(&self, target: u32, buffer: Option<Buffer>) {
            unsafe { self.ctx.bind_buffer(target, buffer) };
        }

        pub fn buffer_data_f32(&self, target: u32, data: &[f32], usage: u32) {
            unsafe {
                let bytes = bytemuck::cast_slice(data);
                self.ctx.buffer_data_u8_slice(target, bytes, usage);
            }
        }

        pub fn enable_vertex_attrib_array(&self, index: u32) {
            unsafe { self.ctx.enable_vertex_attrib_array(index) };
        }

        pub fn vertex_attrib_pointer_f32(&self, index: u32, size: i32, stride: i32, offset: i32) {
            unsafe {
                self.ctx
                    .vertex_attrib_pointer_f32(index, size, glow::FLOAT, false, stride, offset);
            }
        }

        pub fn enable(&self, cap: u32) {
            unsafe { self.ctx.enable(cap) };
        }

        pub fn blend_func(&self, sfactor: u32, dfactor: u32) {
            unsafe { self.ctx.blend_func(sfactor, dfactor) };
        }

        pub fn get_shader_compile_status(&self, shader: glow::Shader) -> bool {
            unsafe { self.ctx.get_shader_compile_status(shader) }
        }

        pub fn get_shader_info_log(&self, shader: glow::Shader) -> String {
            unsafe { self.ctx.get_shader_info_log(shader) }
        }

        pub fn get_program_link_status(&self, program: Program) -> bool {
            unsafe { self.ctx.get_program_link_status(program) }
        }

        pub fn get_program_info_log(&self, program: Program) -> String {
            unsafe { self.ctx.get_program_info_log(program) }
        }
    }
}
