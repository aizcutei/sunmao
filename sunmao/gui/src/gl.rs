//! OpenGL Renderer Backend for SunMao GUI
//!
//! This module implements the `GuiContext` trait using OpenGL via the `glow` crate.
//! It provides a simple 2D rendering context for drawing SunMao GUI widgets.

use crate::{Color, Fill, GuiContext, Stroke, TextAlign};
use glow::HasContext;
use std::f32::consts::PI;

/// OpenGL-based implementation of GuiContext.
pub struct GlContext {
    gl: glow::Context,
    width: f32,
    height: f32,
    scale: f32,
    // Simple shader program for 2D rendering
    program: glow::Program,
    vao: Option<glow::VertexArray>,
    use_vao: bool,
    vbo: glow::Buffer,
    // Uniform locations
    u_transform: glow::UniformLocation,
    u_color: glow::UniformLocation,
}

impl GlContext {
    /// Create a new OpenGL context.
    ///
    /// # Safety
    /// The glow::Context must be valid and current.
    pub unsafe fn new(gl: glow::Context, width: f32, height: f32) -> Result<Self, String> {
        // Create shader program
        let program = Self::create_program(&gl)?;

        let u_transform = gl
            .get_uniform_location(program, "u_transform")
            .ok_or("Failed to get u_transform location")?;
        let u_color = gl
            .get_uniform_location(program, "u_color")
            .ok_or("Failed to get u_color location")?;

        let version_string = gl.get_parameter_string(glow::VERSION);
        let major_version = version_string
            .split('.')
            .next()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .unwrap_or(3);
        let use_vao = major_version >= 3;

        // Create VBO (and VAO when available)
        let vbo = gl.create_buffer().map_err(|e| e.to_string())?;
        let vao = if use_vao {
            let vao = gl.create_vertex_array().map_err(|e| e.to_string())?;
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            // Position attribute
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);
            gl.bind_vertex_array(None);
            Some(vao)
        } else {
            None
        };

        Ok(Self {
            gl,
            width,
            height,
            scale: 1.0,
            program,
            vao,
            use_vao,
            vbo,
            u_transform,
            u_color,
        })
    }

    /// Create a new OpenGL context from a function pointer loader.
    ///
    /// This is useful when working with GUI frameworks like baseview that
    /// provide a `get_proc_address` function.
    ///
    /// # Safety
    /// The loader function must return valid OpenGL function pointers.
    pub unsafe fn from_loader<F>(loader: F, width: f32, height: f32) -> Result<Self, String>
    where
        F: FnMut(&str) -> *const std::ffi::c_void,
    {
        let gl = glow::Context::from_loader_function(loader);
        Self::new(gl, width, height)
    }

    /// Set the viewport size (in physical pixels)
    pub fn set_viewport(&mut self, width: u32, height: u32) {
        // Store logical size (will be used with scale)
        self.width = width as f32 / self.scale;
        self.height = height as f32 / self.scale;
        unsafe {
            self.gl.viewport(0, 0, width as i32, height as i32);
        }
    }

    /// Clear the framebuffer with a color
    pub fn clear(&mut self, color: Color) {
        unsafe {
            self.gl.clear_color(color.r, color.g, color.b, color.a);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }
    }

    unsafe fn create_program(gl: &glow::Context) -> Result<glow::Program, String> {
        let program = gl.create_program().map_err(|e| e.to_string())?;

        let sources = [
            (
                "330",
                r#"#version 330 core
layout (location = 0) in vec2 a_pos;
uniform mat4 u_transform;
void main() {
    gl_Position = u_transform * vec4(a_pos, 0.0, 1.0);
}
"#,
                r#"#version 330 core
uniform vec4 u_color;
out vec4 FragColor;
void main() {
    FragColor = u_color;
}
"#,
            ),
            (
                "150",
                r#"#version 150
layout (location = 0) in vec2 a_pos;
uniform mat4 u_transform;
void main() {
    gl_Position = u_transform * vec4(a_pos, 0.0, 1.0);
}
"#,
                r#"#version 150
uniform vec4 u_color;
out vec4 FragColor;
void main() {
    FragColor = u_color;
}
"#,
            ),
            (
                "120",
                r#"#version 120
attribute vec2 a_pos;
uniform mat4 u_transform;
void main() {
    gl_Position = u_transform * vec4(a_pos, 0.0, 1.0);
}
"#,
                r#"#version 120
uniform vec4 u_color;
void main() {
    gl_FragColor = u_color;
}
"#,
            ),
        ];

        let mut last_error = None;
        for (label, vertex_shader_source, fragment_shader_source) in sources {
            let vs = gl
                .create_shader(glow::VERTEX_SHADER)
                .map_err(|e| e.to_string())?;
            gl.shader_source(vs, vertex_shader_source);
            gl.compile_shader(vs);
            if !gl.get_shader_compile_status(vs) {
                let log = gl.get_shader_info_log(vs);
                gl.delete_shader(vs);
                last_error = Some(format!("Vertex shader error: {}", log));
                continue;
            }

            let fs = gl
                .create_shader(glow::FRAGMENT_SHADER)
                .map_err(|e| e.to_string())?;
            gl.shader_source(fs, fragment_shader_source);
            gl.compile_shader(fs);
            if !gl.get_shader_compile_status(fs) {
                let log = gl.get_shader_info_log(fs);
                gl.delete_shader(vs);
                gl.delete_shader(fs);
                last_error = Some(format!("Fragment shader error: {}", log));
                continue;
            }

            gl.attach_shader(program, vs);
            gl.attach_shader(program, fs);
            gl.bind_attrib_location(program, 0, "a_pos");
            gl.link_program(program);

            if !gl.get_program_link_status(program) {
                let log = gl.get_program_info_log(program);
                gl.delete_shader(vs);
                gl.delete_shader(fs);
                last_error = Some(format!("Program link error: {}", log));
                continue;
            }

            gl.delete_shader(vs);
            gl.delete_shader(fs);
            return Ok(program);
        }

        let error = last_error.unwrap_or_else(|| "Shader compilation failed".to_string());
        Err(error)
    }

    /// Resize the context
    pub fn resize(&mut self, width: f32, height: f32) {
        self.width = width;
        self.height = height;
    }

    /// Set the scale factor (for HiDPI)
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }

    /// Begin a frame
    pub fn begin_frame(&mut self) {
        unsafe {
            self.gl.viewport(
                0,
                0,
                (self.width * self.scale) as i32,
                (self.height * self.scale) as i32,
            );
            self.gl.enable(glow::BLEND);
            self.gl
                .blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            self.gl.use_program(Some(self.program));
            if let Some(vao) = self.vao {
                if self.use_vao {
                    self.gl.bind_vertex_array(Some(vao));
                }
            }
        }
    }

    /// End a frame
    pub fn end_frame(&mut self) {
        unsafe {
            if self.use_vao {
                self.gl.bind_vertex_array(None);
            }
            self.gl.use_program(None);
        }
    }

    fn set_color(&self, color: Color) {
        unsafe {
            self.gl
                .uniform_4_f32(Some(&self.u_color), color.r, color.g, color.b, color.a);
        }
    }

    fn set_ortho_transform(&self) {
        // Create orthographic projection matrix
        let left = 0.0;
        let right = self.width;
        let bottom = self.height;
        let top = 0.0;
        let near = -1.0;
        let far = 1.0;

        let tx = -(right + left) / (right - left);
        let ty = -(top + bottom) / (top - bottom);
        let tz = -(far + near) / (far - near);

        #[rustfmt::skip]
        let matrix: [f32; 16] = [
            2.0 / (right - left), 0.0, 0.0, 0.0,
            0.0, 2.0 / (top - bottom), 0.0, 0.0,
            0.0, 0.0, -2.0 / (far - near), 0.0,
            tx, ty, tz, 1.0,
        ];

        unsafe {
            self.gl
                .uniform_matrix_4_f32_slice(Some(&self.u_transform), false, &matrix);
        }
    }

    fn draw_vertices(&self, vertices: &[f32]) {
        unsafe {
            self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            let vertex_u8 = core::slice::from_raw_parts(
                vertices.as_ptr() as *const u8,
                vertices.len() * std::mem::size_of::<f32>(),
            );
            self.gl
                .buffer_data_u8_slice(glow::ARRAY_BUFFER, vertex_u8, glow::DYNAMIC_DRAW);
            if !self.use_vao {
                self.gl.enable_vertex_attrib_array(0);
                self.gl
                    .vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);
            }
            self.gl
                .draw_arrays(glow::TRIANGLES, 0, (vertices.len() / 2) as i32);
        }
    }
}

impl GuiContext for GlContext {
    fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, fill: Fill) {
        self.set_ortho_transform();
        self.set_color(fill_color(fill));
        let vertices = [x, y, x + w, y, x + w, y + h, x, y, x + w, y + h, x, y + h];
        self.draw_vertices(&vertices);
    }

    fn fill_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, _radius: f32, fill: Fill) {
        self.fill_rect(x, y, w, h, fill);
    }

    fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32, stroke: Stroke) {
        self.set_ortho_transform();
        self.set_color(stroke.color);
        let t = stroke.width;
        // Top
        self.draw_vertices(&[x, y, x + w, y, x + w, y + t, x, y, x + w, y + t, x, y + t]);
        // Bottom
        self.draw_vertices(&[
            x,
            y + h - t,
            x + w,
            y + h - t,
            x + w,
            y + h,
            x,
            y + h - t,
            x + w,
            y + h,
            x,
            y + h,
        ]);
        // Left
        self.draw_vertices(&[
            x,
            y + t,
            x + t,
            y + t,
            x + t,
            y + h - t,
            x,
            y + t,
            x + t,
            y + h - t,
            x,
            y + h - t,
        ]);
        // Right
        self.draw_vertices(&[
            x + w - t,
            y + t,
            x + w,
            y + t,
            x + w,
            y + h - t,
            x + w - t,
            y + t,
            x + w,
            y + h - t,
            x + w - t,
            y + h - t,
        ]);
    }

    fn stroke_rounded_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        _radius: f32,
        stroke: Stroke,
    ) {
        self.stroke_rect(x, y, w, h, stroke);
    }

    fn fill_circle(&mut self, cx: f32, cy: f32, radius: f32, fill: Fill) {
        self.set_ortho_transform();
        self.set_color(fill_color(fill));
        let segments = 32;
        let mut vertices: Vec<f32> = Vec::with_capacity(segments * 6);
        for i in 0..segments {
            let theta1 = (i as f32 / segments as f32) * 2.0 * PI;
            let theta2 = ((i + 1) as f32 / segments as f32) * 2.0 * PI;
            let x1 = cx + radius * theta1.cos();
            let y1 = cy + radius * theta1.sin();
            let x2 = cx + radius * theta2.cos();
            let y2 = cy + radius * theta2.sin();
            vertices.extend_from_slice(&[cx, cy, x1, y1, x2, y2]);
        }
        self.draw_vertices(&vertices);
    }

    fn stroke_circle(&mut self, cx: f32, cy: f32, radius: f32, stroke: Stroke) {
        self.set_ortho_transform();
        self.set_color(stroke.color);
        let segments = 32;
        let mut vertices: Vec<f32> = Vec::with_capacity(segments * 12);
        let inner_r = (radius - stroke.width).max(0.0);
        for i in 0..segments {
            let theta1 = (i as f32 / segments as f32) * 2.0 * PI;
            let theta2 = ((i + 1) as f32 / segments as f32) * 2.0 * PI;
            let x1o = cx + radius * theta1.cos();
            let y1o = cy + radius * theta1.sin();
            let x2o = cx + radius * theta2.cos();
            let y2o = cy + radius * theta2.sin();
            let x1i = cx + inner_r * theta1.cos();
            let y1i = cy + inner_r * theta1.sin();
            let x2i = cx + inner_r * theta2.cos();
            let y2i = cy + inner_r * theta2.sin();
            vertices
                .extend_from_slice(&[x1i, y1i, x1o, y1o, x2o, y2o, x1i, y1i, x2o, y2o, x2i, y2i]);
        }
        self.draw_vertices(&vertices);
    }

    fn stroke_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, stroke: Stroke) {
        self.set_ortho_transform();
        self.set_color(stroke.color);
        let t = stroke.width;
        let dx = x2 - x1;
        let dy = y2 - y1;
        let len = (dx * dx + dy * dy).sqrt().max(1.0);
        let ux = -dy / len * t * 0.5;
        let uy = dx / len * t * 0.5;
        let vertices = [
            x1 + ux,
            y1 + uy,
            x2 + ux,
            y2 + uy,
            x2 - ux,
            y2 - uy,
            x1 + ux,
            y1 + uy,
            x2 - ux,
            y2 - uy,
            x1 - ux,
            y1 - uy,
        ];
        self.draw_vertices(&vertices);
    }

    fn stroke_arc(&mut self, cx: f32, cy: f32, radius: f32, start: f32, end: f32, stroke: Stroke) {
        self.set_ortho_transform();
        self.set_color(stroke.color);
        let segments = 32;
        let mut vertices: Vec<f32> = Vec::with_capacity(segments * 12);
        let inner_r = (radius - stroke.width).max(0.0);
        for i in 0..segments {
            let t1 = start + (end - start) * (i as f32 / segments as f32);
            let t2 = start + (end - start) * ((i + 1) as f32 / segments as f32);
            let x1o = cx + radius * t1.cos();
            let y1o = cy + radius * t1.sin();
            let x2o = cx + radius * t2.cos();
            let y2o = cy + radius * t2.sin();
            let x1i = cx + inner_r * t1.cos();
            let y1i = cy + inner_r * t1.sin();
            let x2i = cx + inner_r * t2.cos();
            let y2i = cy + inner_r * t2.sin();
            vertices
                .extend_from_slice(&[x1i, y1i, x1o, y1o, x2o, y2o, x1i, y1i, x2o, y2o, x2i, y2i]);
        }
        self.draw_vertices(&vertices);
    }

    fn fill_arc(&mut self, cx: f32, cy: f32, radius: f32, start: f32, end: f32, fill: Fill) {
        self.set_ortho_transform();
        self.set_color(fill_color(fill));
        let segments = 32;
        let mut vertices: Vec<f32> = Vec::with_capacity(segments * 6);
        for i in 0..segments {
            let t1 = start + (end - start) * (i as f32 / segments as f32);
            let t2 = start + (end - start) * ((i + 1) as f32 / segments as f32);
            let x1 = cx + radius * t1.cos();
            let y1 = cy + radius * t1.sin();
            let x2 = cx + radius * t2.cos();
            let y2 = cy + radius * t2.sin();
            vertices.extend_from_slice(&[cx, cy, x1, y1, x2, y2]);
        }
        self.draw_vertices(&vertices);
    }

    fn draw_text(&mut self, text: &str, x: f32, y: f32, size: f32, color: Color, align: TextAlign) {
        // Placeholder: text rendering not implemented for GL backend
        let _ = (text, x, y, size, color, align);
    }

    fn measure_text(&self, _text: &str, _size: f32) -> f32 {
        0.0
    }

    fn save(&mut self) {}

    fn restore(&mut self) {}

    fn translate(&mut self, _x: f32, _y: f32) {}

    fn clip(&mut self, _x: f32, _y: f32, _width: f32, _height: f32) {}

    fn reset_clip(&mut self) {}
}

fn fill_color(fill: Fill) -> Color {
    match fill {
        Fill::Solid(color) => color,
    }
}
