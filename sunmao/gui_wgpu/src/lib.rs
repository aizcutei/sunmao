//! WGPU Renderer Backend for SunMao GUI
//!
//! This crate implements the `GuiContext` trait using WGPU for modern cross-platform graphics.

use sunmao_gui::{GuiContext, Color, Fill, Stroke, TextAlign};
use std::f32::consts::PI;
use wgpu::util::DeviceExt;

/// Vertex for 2D rendering
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x4,
    ];
    
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// WGPU-based implementation of GuiContext.
pub struct WgpuContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    width: f32,
    height: f32,
    scale: f32,
    // Vertices accumulated during a frame
    vertices: Vec<Vertex>,
    // Current color
    current_color: Color,
}

impl WgpuContext {
    /// Create a new WGPU context.
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        width: f32,
        height: f32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SunMao GUI Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SunMao GUI Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SunMao GUI Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        
        Self {
            device,
            queue,
            pipeline,
            width,
            height,
            scale: 1.0,
            vertices: Vec::new(),
            current_color: Color::WHITE,
        }
    }
    
    /// Resize the context
    pub fn resize(&mut self, width: f32, height: f32) {
        self.width = width;
        self.height = height;
    }
    
    /// Set the scale factor
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }
    
    /// Begin a frame - clear vertex buffer
    pub fn begin_frame(&mut self) {
        self.vertices.clear();
    }
    
    /// End a frame - render accumulated vertices
    pub fn end_frame(&self, view: &wgpu::TextureView) {
        if self.vertices.is_empty() {
            return;
        }
        
        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SunMao GUI Vertex Buffer"),
            contents: bytemuck::cast_slice(&self.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("SunMao GUI Encoder"),
        });
        
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("SunMao GUI Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Don't clear, just draw on top
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            
            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            render_pass.draw(0..self.vertices.len() as u32, 0..1);
        }
        
        self.queue.submit(std::iter::once(encoder.finish()));
    }
    
    fn to_ndc(&self, x: f32, y: f32) -> [f32; 2] {
        [
            (x / self.width) * 2.0 - 1.0,
            1.0 - (y / self.height) * 2.0, // Flip Y
        ]
    }
    
    fn add_triangle(&mut self, p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], color: Color) {
        let c = [color.r, color.g, color.b, color.a];
        self.vertices.push(Vertex { position: self.to_ndc(p1[0], p1[1]), color: c });
        self.vertices.push(Vertex { position: self.to_ndc(p2[0], p2[1]), color: c });
        self.vertices.push(Vertex { position: self.to_ndc(p3[0], p3[1]), color: c });
    }
    
    fn add_quad(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        self.add_triangle([x, y], [x + w, y], [x + w, y + h], color);
        self.add_triangle([x, y], [x + w, y + h], [x, y + h], color);
    }
    
    fn add_circle(&mut self, cx: f32, cy: f32, radius: f32, color: Color, segments: usize) {
        for i in 0..segments {
            let a1 = (i as f32 / segments as f32) * 2.0 * PI;
            let a2 = ((i + 1) as f32 / segments as f32) * 2.0 * PI;
            
            self.add_triangle(
                [cx, cy],
                [cx + radius * a1.cos(), cy + radius * a1.sin()],
                [cx + radius * a2.cos(), cy + radius * a2.sin()],
                color,
            );
        }
    }
    
    fn add_arc(&mut self, cx: f32, cy: f32, radius: f32, start: f32, end: f32, color: Color, segments: usize) {
        let range = end - start;
        for i in 0..segments {
            let t1 = i as f32 / segments as f32;
            let t2 = (i + 1) as f32 / segments as f32;
            let a1 = start + t1 * range;
            let a2 = start + t2 * range;
            
            self.add_triangle(
                [cx, cy],
                [cx + radius * a1.cos(), cy + radius * a1.sin()],
                [cx + radius * a2.cos(), cy + radius * a2.sin()],
                color,
            );
        }
    }
    
    fn add_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: Color) {
        let dx = x2 - x1;
        let dy = y2 - y1;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 0.001 { return; }
        
        let nx = -dy / len * width * 0.5;
        let ny = dx / len * width * 0.5;
        
        self.add_triangle(
            [x1 - nx, y1 - ny],
            [x1 + nx, y1 + ny],
            [x2 + nx, y2 + ny],
            color,
        );
        self.add_triangle(
            [x1 - nx, y1 - ny],
            [x2 + nx, y2 + ny],
            [x2 - nx, y2 - ny],
            color,
        );
    }
}

impl GuiContext for WgpuContext {
    fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }
    
    fn scale_factor(&self) -> f32 {
        self.scale
    }
    
    fn fill_rect(&mut self, x: f32, y: f32, width: f32, height: f32, fill: Fill) {
        let color = match fill { Fill::Solid(c) => c };
        self.add_quad(x, y, width, height, color);
    }
    
    fn stroke_rect(&mut self, x: f32, y: f32, width: f32, height: f32, stroke: Stroke) {
        let w = stroke.width;
        let c = stroke.color;
        // Top
        self.add_quad(x, y, width, w, c);
        // Bottom
        self.add_quad(x, y + height - w, width, w, c);
        // Left
        self.add_quad(x, y, w, height, c);
        // Right
        self.add_quad(x + width - w, y, w, height, c);
    }
    
    fn fill_rounded_rect(&mut self, x: f32, y: f32, width: f32, height: f32, radius: f32, fill: Fill) {
        let color = match fill { Fill::Solid(c) => c };
        let r = radius.min(width / 2.0).min(height / 2.0);
        
        // Center rectangle
        self.add_quad(x + r, y, width - 2.0 * r, height, color);
        // Left rectangle
        self.add_quad(x, y + r, r, height - 2.0 * r, color);
        // Right rectangle
        self.add_quad(x + width - r, y + r, r, height - 2.0 * r, color);
        
        // Corners (quarter circles)
        let segs = 8;
        // Top-left
        self.add_arc(x + r, y + r, r, PI, PI * 1.5, color, segs);
        // Top-right
        self.add_arc(x + width - r, y + r, r, PI * 1.5, PI * 2.0, color, segs);
        // Bottom-right
        self.add_arc(x + width - r, y + height - r, r, 0.0, PI * 0.5, color, segs);
        // Bottom-left
        self.add_arc(x + r, y + height - r, r, PI * 0.5, PI, color, segs);
    }
    
    fn stroke_rounded_rect(&mut self, x: f32, y: f32, width: f32, height: f32, radius: f32, stroke: Stroke) {
        // Simplified: just stroke a regular rect
        self.stroke_rect(x, y, width, height, stroke);
    }
    
    fn fill_circle(&mut self, cx: f32, cy: f32, radius: f32, fill: Fill) {
        let color = match fill { Fill::Solid(c) => c };
        self.add_circle(cx, cy, radius, color, 32);
    }
    
    fn stroke_circle(&mut self, cx: f32, cy: f32, radius: f32, stroke: Stroke) {
        // Draw as a thick ring
        let inner = radius - stroke.width;
        let outer = radius;
        let segs = 32;
        
        for i in 0..segs {
            let a1 = (i as f32 / segs as f32) * 2.0 * PI;
            let a2 = ((i + 1) as f32 / segs as f32) * 2.0 * PI;
            
            self.add_quad_arc(cx, cy, inner, outer, a1, a2, stroke.color);
        }
    }
    
    fn stroke_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, stroke: Stroke) {
        self.add_line(x1, y1, x2, y2, stroke.width, stroke.color);
    }
    
    fn stroke_arc(&mut self, cx: f32, cy: f32, radius: f32, start_angle: f32, end_angle: f32, stroke: Stroke) {
        let inner = radius - stroke.width * 0.5;
        let outer = radius + stroke.width * 0.5;
        let segs = 32;
        let range = end_angle - start_angle;
        
        for i in 0..segs {
            let t1 = i as f32 / segs as f32;
            let t2 = (i + 1) as f32 / segs as f32;
            let a1 = start_angle + t1 * range;
            let a2 = start_angle + t2 * range;
            
            self.add_quad_arc(cx, cy, inner, outer, a1, a2, stroke.color);
        }
    }
    
    fn fill_arc(&mut self, cx: f32, cy: f32, radius: f32, start_angle: f32, end_angle: f32, fill: Fill) {
        let color = match fill { Fill::Solid(c) => c };
        self.add_arc(cx, cy, radius, start_angle, end_angle, color, 32);
    }
    
    fn draw_text(&mut self, _text: &str, _x: f32, _y: f32, _size: f32, _color: Color, _align: TextAlign) {
        // Text rendering requires font atlas - placeholder
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

impl WgpuContext {
    fn add_quad_arc(&mut self, cx: f32, cy: f32, inner: f32, outer: f32, a1: f32, a2: f32, color: Color) {
        let p1 = [cx + inner * a1.cos(), cy + inner * a1.sin()];
        let p2 = [cx + outer * a1.cos(), cy + outer * a1.sin()];
        let p3 = [cx + outer * a2.cos(), cy + outer * a2.sin()];
        let p4 = [cx + inner * a2.cos(), cy + inner * a2.sin()];
        
        self.add_triangle(p1, p2, p3, color);
        self.add_triangle(p1, p3, p4, color);
    }
}
