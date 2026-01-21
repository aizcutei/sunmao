//! WebView Renderer Backend for SunMao GUI
//!
//! This crate implements the `GuiContext` trait for WebView-based rendering.
//! It generates HTML5 Canvas drawing commands that can be executed in a WebView.

use sunmao_gui::{GuiContext, Color, Fill, Stroke, TextAlign};
use serde::Serialize;
use std::f32::consts::PI;

/// A drawing command to be sent to the WebView
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum DrawCommand {
    FillRect { x: f32, y: f32, width: f32, height: f32, color: String },
    StrokeRect { x: f32, y: f32, width: f32, height: f32, color: String, width_: f32 },
    FillCircle { cx: f32, cy: f32, radius: f32, color: String },
    StrokeCircle { cx: f32, cy: f32, radius: f32, color: String, width: f32 },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, color: String, width: f32 },
    Arc { cx: f32, cy: f32, radius: f32, start: f32, end: f32, color: String, width: f32 },
    FillArc { cx: f32, cy: f32, radius: f32, start: f32, end: f32, color: String },
    Text { text: String, x: f32, y: f32, size: f32, color: String, align: String },
    BeginPath,
    ClosePath,
    Clear { width: f32, height: f32 },
}

/// WebView-based implementation of GuiContext.
///
/// This context accumulates drawing commands as JSON that can be sent
/// to a WebView for rendering on an HTML5 Canvas.
pub struct WebViewContext {
    width: f32,
    height: f32,
    scale: f32,
    commands: Vec<DrawCommand>,
}

impl WebViewContext {
    /// Create a new WebView context
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            scale: 1.0,
            commands: Vec::new(),
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
        self.commands.clear();
        self.commands.push(DrawCommand::Clear { 
            width: self.width, 
            height: self.height 
        });
    }
    
    /// Get the accumulated commands as JSON
    pub fn get_commands_json(&self) -> String {
        serde_json::to_string(&self.commands).unwrap_or_default()
    }
    
    /// Get the accumulated commands
    pub fn get_commands(&self) -> &[DrawCommand] {
        &self.commands
    }
    
    /// Generate JavaScript code to execute the drawing commands
    pub fn generate_js(&self) -> String {
        let mut js = String::new();
        js.push_str("const ctx = document.getElementById('canvas').getContext('2d');\n");
        
        for cmd in &self.commands {
            match cmd {
                DrawCommand::Clear { width, height } => {
                    js.push_str(&format!(
                        "ctx.clearRect(0, 0, {}, {});\n", width, height
                    ));
                }
                DrawCommand::FillRect { x, y, width, height, color } => {
                    js.push_str(&format!(
                        "ctx.fillStyle = '{}';\nctx.fillRect({}, {}, {}, {});\n",
                        color, x, y, width, height
                    ));
                }
                DrawCommand::StrokeRect { x, y, width, height, color, width_: stroke_width } => {
                    js.push_str(&format!(
                        "ctx.strokeStyle = '{}';\nctx.lineWidth = {};\nctx.strokeRect({}, {}, {}, {});\n",
                        color, stroke_width, x, y, width, height
                    ));
                }
                DrawCommand::FillCircle { cx, cy, radius, color } => {
                    js.push_str(&format!(
                        "ctx.fillStyle = '{}';\nctx.beginPath();\nctx.arc({}, {}, {}, 0, Math.PI * 2);\nctx.fill();\n",
                        color, cx, cy, radius
                    ));
                }
                DrawCommand::StrokeCircle { cx, cy, radius, color, width } => {
                    js.push_str(&format!(
                        "ctx.strokeStyle = '{}';\nctx.lineWidth = {};\nctx.beginPath();\nctx.arc({}, {}, {}, 0, Math.PI * 2);\nctx.stroke();\n",
                        color, width, cx, cy, radius
                    ));
                }
                DrawCommand::Line { x1, y1, x2, y2, color, width } => {
                    js.push_str(&format!(
                        "ctx.strokeStyle = '{}';\nctx.lineWidth = {};\nctx.beginPath();\nctx.moveTo({}, {});\nctx.lineTo({}, {});\nctx.stroke();\n",
                        color, width, x1, y1, x2, y2
                    ));
                }
                DrawCommand::Arc { cx, cy, radius, start, end, color, width } => {
                    js.push_str(&format!(
                        "ctx.strokeStyle = '{}';\nctx.lineWidth = {};\nctx.beginPath();\nctx.arc({}, {}, {}, {}, {});\nctx.stroke();\n",
                        color, width, cx, cy, radius, start, end
                    ));
                }
                DrawCommand::FillArc { cx, cy, radius, start, end, color } => {
                    js.push_str(&format!(
                        "ctx.fillStyle = '{}';\nctx.beginPath();\nctx.moveTo({}, {});\nctx.arc({}, {}, {}, {}, {});\nctx.closePath();\nctx.fill();\n",
                        color, cx, cy, cx, cy, radius, start, end
                    ));
                }
                DrawCommand::Text { text, x, y, size, color, align } => {
                    js.push_str(&format!(
                        "ctx.fillStyle = '{}';\nctx.font = '{}px sans-serif';\nctx.textAlign = '{}';\nctx.fillText('{}', {}, {});\n",
                        color, size, align, text, x, y
                    ));
                }
                _ => {}
            }
        }
        
        js
    }
    
    fn color_to_css(color: Color) -> String {
        format!("rgba({},{},{},{})", 
            (color.r * 255.0) as u8,
            (color.g * 255.0) as u8,
            (color.b * 255.0) as u8,
            color.a
        )
    }
}

impl GuiContext for WebViewContext {
    fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }
    
    fn scale_factor(&self) -> f32 {
        self.scale
    }
    
    fn fill_rect(&mut self, x: f32, y: f32, width: f32, height: f32, fill: Fill) {
        let color = match fill { Fill::Solid(c) => Self::color_to_css(c) };
        self.commands.push(DrawCommand::FillRect { x, y, width, height, color });
    }
    
    fn stroke_rect(&mut self, x: f32, y: f32, width: f32, height: f32, stroke: Stroke) {
        self.commands.push(DrawCommand::StrokeRect { 
            x, y, width, height, 
            color: Self::color_to_css(stroke.color),
            width_: stroke.width,
        });
    }
    
    fn fill_rounded_rect(&mut self, x: f32, y: f32, width: f32, height: f32, _radius: f32, fill: Fill) {
        // Simplified - WebView can use CSS border-radius, but for canvas we just fill rect
        self.fill_rect(x, y, width, height, fill);
    }
    
    fn stroke_rounded_rect(&mut self, x: f32, y: f32, width: f32, height: f32, _radius: f32, stroke: Stroke) {
        self.stroke_rect(x, y, width, height, stroke);
    }
    
    fn fill_circle(&mut self, cx: f32, cy: f32, radius: f32, fill: Fill) {
        let color = match fill { Fill::Solid(c) => Self::color_to_css(c) };
        self.commands.push(DrawCommand::FillCircle { cx, cy, radius, color });
    }
    
    fn stroke_circle(&mut self, cx: f32, cy: f32, radius: f32, stroke: Stroke) {
        self.commands.push(DrawCommand::StrokeCircle { 
            cx, cy, radius,
            color: Self::color_to_css(stroke.color),
            width: stroke.width,
        });
    }
    
    fn stroke_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, stroke: Stroke) {
        self.commands.push(DrawCommand::Line { 
            x1, y1, x2, y2,
            color: Self::color_to_css(stroke.color),
            width: stroke.width,
        });
    }
    
    fn stroke_arc(&mut self, cx: f32, cy: f32, radius: f32, start_angle: f32, end_angle: f32, stroke: Stroke) {
        self.commands.push(DrawCommand::Arc { 
            cx, cy, radius,
            start: start_angle,
            end: end_angle,
            color: Self::color_to_css(stroke.color),
            width: stroke.width,
        });
    }
    
    fn fill_arc(&mut self, cx: f32, cy: f32, radius: f32, start_angle: f32, end_angle: f32, fill: Fill) {
        let color = match fill { Fill::Solid(c) => Self::color_to_css(c) };
        self.commands.push(DrawCommand::FillArc { 
            cx, cy, radius,
            start: start_angle,
            end: end_angle,
            color,
        });
    }
    
    fn draw_text(&mut self, text: &str, x: f32, y: f32, size: f32, color: Color, align: TextAlign) {
        let align_str = match align {
            TextAlign::Left => "left",
            TextAlign::Center => "center",
            TextAlign::Right => "right",
        };
        self.commands.push(DrawCommand::Text { 
            text: text.to_string(),
            x, y, size,
            color: Self::color_to_css(color),
            align: align_str.to_string(),
        });
    }
    
    fn measure_text(&self, text: &str, size: f32) -> f32 {
        // Approximate: 0.6 * size per character
        text.len() as f32 * size * 0.6
    }
    
    fn save(&mut self) {}
    fn restore(&mut self) {}
    fn translate(&mut self, _x: f32, _y: f32) {}
    fn clip(&mut self, _x: f32, _y: f32, _width: f32, _height: f32) {}
    fn reset_clip(&mut self) {}
}

/// Generate a complete HTML page with embedded canvas and drawing code
pub fn generate_html_page(title: &str, width: u32, height: u32, draw_commands: &str) -> String {
    format!(r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>{title}</title>
    <style>
        body {{
            margin: 0;
            background: #1a1a1e;
            display: flex;
            justify-content: center;
            align-items: center;
            min-height: 100vh;
        }}
        canvas {{
            background: #25252a;
            border-radius: 8px;
        }}
    </style>
</head>
<body>
    <canvas id="canvas" width="{width}" height="{height}"></canvas>
    <script>
{draw_commands}
    </script>
</body>
</html>"#)
}
