//! OpenGL Renderer Backend for SunMao GUI
//!
//! This crate implements the `GuiContext` trait using OpenGL via the `glow` crate.
//! It provides a simple 2D rendering context for drawing SunMao GUI widgets.

use glow::HasContext;
use std::f32::consts::PI;
use sunmao_gui::{Color, Fill, GuiContext, Stroke, TextAlign};

/// OpenGL-based implementation of GuiContext.
pub struct GlContext {
    gl: glow::Context,
    width: f32,
    height: f32,
    scale: f32,
    // Simple shader program for 2D rendering
    program: glow::Program,
    vao: glow::VertexArray,
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

        // Create VAO and VBO
        let vao = gl.create_vertex_array().map_err(|e| e.to_string())?;
        let vbo = gl.create_buffer().map_err(|e| e.to_string())?;

        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));

        // Position attribute
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);

        gl.bind_vertex_array(None);

        Ok(Self {
            gl,
            width,
            height,
            scale: 1.0,
            program,
            vao,
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
        let vertex_shader_source = r#"
            #version 330 core
            layout (location = 0) in vec2 a_pos;
            uniform mat4 u_transform;
            void main() {
                gl_Position = u_transform * vec4(a_pos, 0.0, 1.0);
            }
        "#;

        let fragment_shader_source = r#"
            #version 330 core
            uniform vec4 u_color;
            out vec4 FragColor;
            void main() {
                FragColor = u_color;
            }
        "#;

        let program = gl.create_program().map_err(|e| e.to_string())?;

        let vs = gl
            .create_shader(glow::VERTEX_SHADER)
            .map_err(|e| e.to_string())?;
        gl.shader_source(vs, vertex_shader_source);
        gl.compile_shader(vs);
        if !gl.get_shader_compile_status(vs) {
            let log = gl.get_shader_info_log(vs);
            return Err(format!("Vertex shader error: {}", log));
        }

        let fs = gl
            .create_shader(glow::FRAGMENT_SHADER)
            .map_err(|e| e.to_string())?;
        gl.shader_source(fs, fragment_shader_source);
        gl.compile_shader(fs);
        if !gl.get_shader_compile_status(fs) {
            let log = gl.get_shader_info_log(fs);
            return Err(format!("Fragment shader error: {}", log));
        }

        gl.attach_shader(program, vs);
        gl.attach_shader(program, fs);
        gl.link_program(program);

        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            return Err(format!("Program link error: {}", log));
        }

        gl.delete_shader(vs);
        gl.delete_shader(fs);

        Ok(program)
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
            self.gl.bind_vertex_array(Some(self.vao));
        }
    }

    /// End a frame
    pub fn end_frame(&mut self) {
        unsafe {
            self.gl.bind_vertex_array(None);
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
            2.0 / (right - left), 0.0,                  0.0,                0.0,
            0.0,                  2.0 / (top - bottom), 0.0,                0.0,
            0.0,                  0.0,                  -2.0 / (far - near), 0.0,
            tx,                   ty,                   tz,                 1.0,
        ];

        unsafe {
            self.gl
                .uniform_matrix_4_f32_slice(Some(&self.u_transform), false, &matrix);
        }
    }

    fn draw_vertices(&self, vertices: &[f32], mode: u32) {
        unsafe {
            self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            self.gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(vertices),
                glow::DYNAMIC_DRAW,
            );
            self.gl.draw_arrays(mode, 0, (vertices.len() / 2) as i32);
        }
    }

    fn generate_circle_vertices(&self, cx: f32, cy: f32, radius: f32, segments: usize) -> Vec<f32> {
        let mut vertices = Vec::with_capacity(segments * 2 + 4);
        vertices.push(cx);
        vertices.push(cy);

        for i in 0..=segments {
            let angle = (i as f32 / segments as f32) * 2.0 * PI;
            vertices.push(cx + radius * angle.cos());
            vertices.push(cy + radius * angle.sin());
        }

        vertices
    }

    fn generate_arc_vertices(
        &self,
        cx: f32,
        cy: f32,
        radius: f32,
        start: f32,
        end: f32,
        segments: usize,
    ) -> Vec<f32> {
        let mut vertices = Vec::with_capacity(segments * 2 + 2);
        let range = end - start;

        for i in 0..=segments {
            let t = i as f32 / segments as f32;
            let angle = start + t * range;
            vertices.push(cx + radius * angle.cos());
            vertices.push(cy + radius * angle.sin());
        }

        vertices
    }

    fn generate_rounded_rect_vertices(
        &self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        segments: usize,
    ) -> Vec<f32> {
        let r = r.min(w / 2.0).min(h / 2.0);
        let mut vertices = Vec::new();

        // Center point for triangle fan
        vertices.push(x + w / 2.0);
        vertices.push(y + h / 2.0);

        let corners = [
            (x + r, y + r, PI, PI * 1.5),           // Top-left
            (x + w - r, y + r, PI * 1.5, PI * 2.0), // Top-right
            (x + w - r, y + h - r, 0.0, PI * 0.5),  // Bottom-right
            (x + r, y + h - r, PI * 0.5, PI),       // Bottom-left
        ];

        for (cx, cy, start, end) in corners {
            for i in 0..=segments {
                let t = i as f32 / segments as f32;
                let angle = start + t * (end - start);
                vertices.push(cx + r * angle.cos());
                vertices.push(cy + r * angle.sin());
            }
        }

        // Close the shape
        vertices.push(vertices[2]);
        vertices.push(vertices[3]);

        vertices
    }
}

impl GuiContext for GlContext {
    fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    fn scale_factor(&self) -> f32 {
        self.scale
    }

    fn fill_rect(&mut self, x: f32, y: f32, width: f32, height: f32, fill: Fill) {
        let color = match fill {
            Fill::Solid(c) => c,
        };

        self.set_ortho_transform();
        self.set_color(color);

        #[rustfmt::skip]
        let vertices: [f32; 12] = [
            x,         y,
            x + width, y,
            x + width, y + height,
            x,         y,
            x + width, y + height,
            x,         y + height,
        ];

        self.draw_vertices(&vertices, glow::TRIANGLES);
    }

    fn stroke_rect(&mut self, x: f32, y: f32, width: f32, height: f32, stroke: Stroke) {
        self.set_ortho_transform();
        self.set_color(stroke.color);

        unsafe {
            self.gl.line_width(stroke.width);
        }

        #[rustfmt::skip]
        let vertices: [f32; 10] = [
            x,         y,
            x + width, y,
            x + width, y + height,
            x,         y + height,
            x,         y,
        ];

        self.draw_vertices(&vertices, glow::LINE_STRIP);
    }

    fn fill_rounded_rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
        fill: Fill,
    ) {
        let color = match fill {
            Fill::Solid(c) => c,
        };

        self.set_ortho_transform();
        self.set_color(color);

        let vertices = self.generate_rounded_rect_vertices(x, y, width, height, radius, 8);
        self.draw_vertices(&vertices, glow::TRIANGLE_FAN);
    }

    fn stroke_rounded_rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
        stroke: Stroke,
    ) {
        self.set_ortho_transform();
        self.set_color(stroke.color);

        unsafe {
            self.gl.line_width(stroke.width);
        }

        let vertices = self.generate_rounded_rect_vertices(x, y, width, height, radius, 8);
        // Skip center vertex for line loop
        self.draw_vertices(&vertices[2..], glow::LINE_LOOP);
    }

    fn fill_circle(&mut self, cx: f32, cy: f32, radius: f32, fill: Fill) {
        let color = match fill {
            Fill::Solid(c) => c,
        };

        self.set_ortho_transform();
        self.set_color(color);

        let vertices = self.generate_circle_vertices(cx, cy, radius, 32);
        self.draw_vertices(&vertices, glow::TRIANGLE_FAN);
    }

    fn stroke_circle(&mut self, cx: f32, cy: f32, radius: f32, stroke: Stroke) {
        self.set_ortho_transform();
        self.set_color(stroke.color);

        unsafe {
            self.gl.line_width(stroke.width);
        }

        let vertices = self.generate_circle_vertices(cx, cy, radius, 32);
        // Skip center vertex for line loop
        self.draw_vertices(&vertices[2..], glow::LINE_LOOP);
    }

    fn stroke_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, stroke: Stroke) {
        self.set_ortho_transform();
        self.set_color(stroke.color);

        unsafe {
            self.gl.line_width(stroke.width);
        }

        let vertices: [f32; 4] = [x1, y1, x2, y2];
        self.draw_vertices(&vertices, glow::LINES);
    }

    fn stroke_arc(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        stroke: Stroke,
    ) {
        self.set_ortho_transform();
        self.set_color(stroke.color);

        unsafe {
            self.gl.line_width(stroke.width);
        }

        let vertices = self.generate_arc_vertices(cx, cy, radius, start_angle, end_angle, 32);
        self.draw_vertices(&vertices, glow::LINE_STRIP);
    }

    fn fill_arc(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        fill: Fill,
    ) {
        let color = match fill {
            Fill::Solid(c) => c,
        };

        self.set_ortho_transform();
        self.set_color(color);

        let mut vertices = vec![cx, cy];
        let arc = self.generate_arc_vertices(cx, cy, radius, start_angle, end_angle, 32);
        vertices.extend(arc);

        self.draw_vertices(&vertices, glow::TRIANGLE_FAN);
    }

    fn draw_text(
        &mut self,
        _text: &str,
        _x: f32,
        _y: f32,
        _size: f32,
        _color: Color,
        _align: TextAlign,
    ) {
        // Text rendering requires font loading - placeholder for now
        // In a real implementation, you'd use a library like glyph_brush or ab_glyph
    }

    fn measure_text(&self, _text: &str, _size: f32) -> f32 {
        // Placeholder - approximate 8px per character
        0.0
    }

    fn save(&mut self) {
        // Placeholder for state stack
    }

    fn restore(&mut self) {
        // Placeholder for state stack
    }

    fn translate(&mut self, _x: f32, _y: f32) {
        // Would modify transformation matrix
    }

    fn clip(&mut self, x: f32, y: f32, width: f32, height: f32) {
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                (x * self.scale) as i32,
                ((self.height - y - height) * self.scale) as i32,
                (width * self.scale) as i32,
                (height * self.scale) as i32,
            );
        }
    }

    fn reset_clip(&mut self) {
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }
    }
}

impl Drop for GlContext {
    fn drop(&mut self) {
        unsafe {
            self.gl.delete_program(self.program);
            self.gl.delete_vertex_array(self.vao);
            self.gl.delete_buffer(self.vbo);
        }
    }
}
