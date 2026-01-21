//! WGPU Renderer Backend for SunMao GUI
//!
//! This module implements the `GuiContext` trait using WGPU for modern cross-platform graphics.

use crate::{Color, Fill, GuiContext, Stroke, TextAlign};
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
                ..Default::default()
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

    /// Begin a new frame
    pub fn begin_frame(&mut self) {
        self.vertices.clear();
    }

    /// End the frame and render
    pub fn end_frame(&mut self, view: &wgpu::TextureView) {
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("SunMao GUI Vertex Buffer"),
                contents: bytemuck::cast_slice(&self.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("SunMao GUI Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("SunMao GUI Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
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

        self.queue.submit(Some(encoder.finish()));
    }

    fn push_vertex(&mut self, x: f32, y: f32) {
        let nx = (x / self.width) * 2.0 - 1.0;
        let ny = 1.0 - (y / self.height) * 2.0;
        self.vertices.push(Vertex {
            position: [nx, ny],
            color: [
                self.current_color.r,
                self.current_color.g,
                self.current_color.b,
                self.current_color.a,
            ],
        });
    }

    fn set_color(&mut self, color: Color) {
        self.current_color = color;
    }
}

impl GuiContext for WgpuContext {
    fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, fill: Fill) {
        self.set_color(fill.color());
        self.push_vertex(x, y);
        self.push_vertex(x + w, y);
        self.push_vertex(x + w, y + h);
        self.push_vertex(x, y);
        self.push_vertex(x + w, y + h);
        self.push_vertex(x, y + h);
    }

    fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32, stroke: Stroke) {
        self.set_color(stroke.color);
        let t = stroke.width;
        self.fill_rect(x, y, w, t, Fill::Solid(stroke.color));
        self.fill_rect(x, y + h - t, w, t, Fill::Solid(stroke.color));
        self.fill_rect(x, y + t, t, h - 2.0 * t, Fill::Solid(stroke.color));
        self.fill_rect(x + w - t, y + t, t, h - 2.0 * t, Fill::Solid(stroke.color));
    }

    fn fill_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, _radius: f32, fill: Fill) {
        self.fill_rect(x, y, w, h, fill);
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
        self.set_color(fill.color());
        let segments = 32;
        for i in 0..segments {
            let theta1 = (i as f32 / segments as f32) * 2.0 * PI;
            let theta2 = ((i + 1) as f32 / segments as f32) * 2.0 * PI;
            let x1 = cx + radius * theta1.cos();
            let y1 = cy + radius * theta1.sin();
            let x2 = cx + radius * theta2.cos();
            let y2 = cy + radius * theta2.sin();
            self.push_vertex(cx, cy);
            self.push_vertex(x1, y1);
            self.push_vertex(x2, y2);
        }
    }

    fn stroke_circle(&mut self, cx: f32, cy: f32, radius: f32, stroke: Stroke) {
        self.set_color(stroke.color);
        let segments = 32;
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
            self.push_vertex(x1i, y1i);
            self.push_vertex(x1o, y1o);
            self.push_vertex(x2o, y2o);
            self.push_vertex(x1i, y1i);
            self.push_vertex(x2o, y2o);
            self.push_vertex(x2i, y2i);
        }
    }

    fn stroke_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, stroke: Stroke) {
        self.set_color(stroke.color);
        let t = stroke.width;
        let dx = x2 - x1;
        let dy = y2 - y1;
        let len = (dx * dx + dy * dy).sqrt().max(1.0);
        let ux = -dy / len * t * 0.5;
        let uy = dx / len * t * 0.5;
        self.push_vertex(x1 + ux, y1 + uy);
        self.push_vertex(x2 + ux, y2 + uy);
        self.push_vertex(x2 - ux, y2 - uy);
        self.push_vertex(x1 + ux, y1 + uy);
        self.push_vertex(x2 - ux, y2 - uy);
        self.push_vertex(x1 - ux, y1 - uy);
    }

    fn stroke_arc(&mut self, cx: f32, cy: f32, radius: f32, start: f32, end: f32, stroke: Stroke) {
        self.set_color(stroke.color);
        let segments = 32;
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
            self.push_vertex(x1i, y1i);
            self.push_vertex(x1o, y1o);
            self.push_vertex(x2o, y2o);
            self.push_vertex(x1i, y1i);
            self.push_vertex(x2o, y2o);
            self.push_vertex(x2i, y2i);
        }
    }

    fn fill_arc(&mut self, cx: f32, cy: f32, radius: f32, start: f32, end: f32, fill: Fill) {
        self.set_color(fill.color());
        let segments = 32;
        for i in 0..segments {
            let t1 = start + (end - start) * (i as f32 / segments as f32);
            let t2 = start + (end - start) * ((i + 1) as f32 / segments as f32);
            let x1 = cx + radius * t1.cos();
            let y1 = cy + radius * t1.sin();
            let x2 = cx + radius * t2.cos();
            let y2 = cy + radius * t2.sin();
            self.push_vertex(cx, cy);
            self.push_vertex(x1, y1);
            self.push_vertex(x2, y2);
        }
    }

    fn draw_text(&mut self, text: &str, x: f32, y: f32, size: f32, color: Color, align: TextAlign) {
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
