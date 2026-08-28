//! WebView Renderer Backend for SunMao GUI
//!
//! This module implements the `GuiContext` trait for WebView-based rendering.
//! It generates HTML5 Canvas drawing commands that can be executed in a WebView.

use crate::{Color, Fill, GuiContext, Stroke, TextAlign};
use serde::Serialize;

/// A drawing command to be sent to the WebView
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum DrawCommand {
    FillRect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: String,
    },
    StrokeRect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: String,
        width_: f32,
    },
    FillCircle {
        cx: f32,
        cy: f32,
        radius: f32,
        color: String,
    },
    StrokeCircle {
        cx: f32,
        cy: f32,
        radius: f32,
        color: String,
        width: f32,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        color: String,
        width: f32,
    },
    Arc {
        cx: f32,
        cy: f32,
        radius: f32,
        start: f32,
        end: f32,
        color: String,
        width: f32,
    },
    FillArc {
        cx: f32,
        cy: f32,
        radius: f32,
        start: f32,
        end: f32,
        color: String,
    },
    Text {
        text: String,
        x: f32,
        y: f32,
        size: f32,
        color: String,
        align: String,
    },
    BeginPath,
    ClosePath,
    Clear {
        width: f32,
        height: f32,
    },
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
        if width.is_finite() && height.is_finite() && width >= 0.0 && height >= 0.0 {
            self.width = width;
            self.height = height;
        }
    }

    /// Set the scale factor
    pub fn set_scale(&mut self, scale: f32) {
        if scale.is_finite() && scale > 0.0 {
            self.scale = scale;
        }
    }

    /// Begin a new frame
    pub fn begin_frame(&mut self) {
        self.commands.clear();
        self.commands.push(DrawCommand::Clear {
            width: self.width,
            height: self.height,
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
                    js.push_str(&format!("ctx.clearRect(0, 0, {}, {});\n", width, height));
                }
                DrawCommand::FillRect {
                    x,
                    y,
                    width,
                    height,
                    color,
                } => {
                    let color = js_string_literal(color);
                    js.push_str(&format!(
                        "ctx.fillStyle = {};\nctx.fillRect({}, {}, {}, {});\n",
                        color, x, y, width, height
                    ));
                }
                DrawCommand::StrokeRect {
                    x,
                    y,
                    width,
                    height,
                    color,
                    width_: stroke_width,
                } => {
                    let color = js_string_literal(color);
                    js.push_str(&format!(
                        "ctx.strokeStyle = {};\nctx.lineWidth = {};\nctx.strokeRect({}, {}, {}, {});\n",
                        color, stroke_width, x, y, width, height
                    ));
                }
                DrawCommand::FillCircle {
                    cx,
                    cy,
                    radius,
                    color,
                } => {
                    let color = js_string_literal(color);
                    js.push_str(&format!(
                        "ctx.fillStyle = {};\nctx.beginPath();\nctx.arc({}, {}, {}, 0, Math.PI * 2);\nctx.fill();\n",
                        color, cx, cy, radius
                    ));
                }
                DrawCommand::StrokeCircle {
                    cx,
                    cy,
                    radius,
                    color,
                    width,
                } => {
                    let color = js_string_literal(color);
                    js.push_str(&format!(
                        "ctx.strokeStyle = {};\nctx.lineWidth = {};\nctx.beginPath();\nctx.arc({}, {}, {}, 0, Math.PI * 2);\nctx.stroke();\n",
                        color, width, cx, cy, radius
                    ));
                }
                DrawCommand::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    color,
                    width,
                } => {
                    let color = js_string_literal(color);
                    js.push_str(&format!(
                        "ctx.strokeStyle = {};\nctx.lineWidth = {};\nctx.beginPath();\nctx.moveTo({}, {});\nctx.lineTo({}, {});\nctx.stroke();\n",
                        color, width, x1, y1, x2, y2
                    ));
                }
                DrawCommand::Arc {
                    cx,
                    cy,
                    radius,
                    start,
                    end,
                    color,
                    width,
                } => {
                    let color = js_string_literal(color);
                    js.push_str(&format!(
                        "ctx.strokeStyle = {};\nctx.lineWidth = {};\nctx.beginPath();\nctx.arc({}, {}, {}, {}, {});\nctx.stroke();\n",
                        color, width, cx, cy, radius, start, end
                    ));
                }
                DrawCommand::FillArc {
                    cx,
                    cy,
                    radius,
                    start,
                    end,
                    color,
                } => {
                    let color = js_string_literal(color);
                    js.push_str(&format!(
                        "ctx.fillStyle = {};\nctx.beginPath();\nctx.moveTo({}, {});\nctx.arc({}, {}, {}, {}, {});\nctx.closePath();\nctx.fill();\n",
                        color, cx, cy, cx, cy, radius, start, end
                    ));
                }
                DrawCommand::Text {
                    text,
                    x,
                    y,
                    size,
                    color,
                    align,
                } => {
                    // JSON string encoding is also valid JavaScript string
                    // syntax and handles quotes, backslashes, newlines and
                    // U+2028/U+2029. Direct interpolation here allowed plugin
                    // labels to break or inject the generated script.
                    let text = js_string_literal(text);
                    let color = js_string_literal(color);
                    let align = js_string_literal(align);
                    js.push_str(&format!(
                        "ctx.fillStyle = {};\nctx.font = '{}px sans-serif';\nctx.textAlign = {};\nctx.fillText({}, {}, {});\n",
                        color, size, align, text, x, y
                    ));
                }
                DrawCommand::BeginPath => js.push_str("ctx.beginPath();\n"),
                DrawCommand::ClosePath => js.push_str("ctx.closePath();\n"),
            }
        }

        js
    }
}

fn js_string_literal(value: &str) -> String {
    // JSON string syntax is valid JavaScript, but JSON does not escape HTML
    // delimiters. Escape them as well so a label containing `</script>` stays
    // inside the string when generated drawing code is embedded in a page.
    serde_json::to_string(value)
        .map(|literal| {
            literal
                .replace('&', "\\u0026")
                .replace('<', "\\u003c")
                .replace('>', "\\u003e")
                .replace('\u{2028}', "\\u2028")
                .replace('\u{2029}', "\\u2029")
        })
        .unwrap_or_else(|_| "\"\"".to_string())
}

impl GuiContext for WebViewContext {
    fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    fn scale_factor(&self) -> f32 {
        self.scale
    }

    fn fill_rect(&mut self, x: f32, y: f32, width: f32, height: f32, fill: Fill) {
        self.commands.push(DrawCommand::FillRect {
            x,
            y,
            width,
            height,
            color: fill.color().to_hex(),
        });
    }

    fn stroke_rect(&mut self, x: f32, y: f32, width: f32, height: f32, stroke: Stroke) {
        self.commands.push(DrawCommand::StrokeRect {
            x,
            y,
            width,
            height,
            color: stroke.color.to_hex(),
            width_: stroke.width,
        });
    }

    fn fill_rounded_rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        _radius: f32,
        fill: Fill,
    ) {
        self.fill_rect(x, y, width, height, fill);
    }

    fn stroke_rounded_rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        _radius: f32,
        stroke: Stroke,
    ) {
        self.stroke_rect(x, y, width, height, stroke);
    }

    fn fill_circle(&mut self, cx: f32, cy: f32, radius: f32, fill: Fill) {
        self.commands.push(DrawCommand::FillCircle {
            cx,
            cy,
            radius,
            color: fill.color().to_hex(),
        });
    }

    fn stroke_circle(&mut self, cx: f32, cy: f32, radius: f32, stroke: Stroke) {
        self.commands.push(DrawCommand::StrokeCircle {
            cx,
            cy,
            radius,
            color: stroke.color.to_hex(),
            width: stroke.width,
        });
    }

    fn stroke_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, stroke: Stroke) {
        self.commands.push(DrawCommand::Line {
            x1,
            y1,
            x2,
            y2,
            color: stroke.color.to_hex(),
            width: stroke.width,
        });
    }

    fn stroke_arc(&mut self, cx: f32, cy: f32, radius: f32, start: f32, end: f32, stroke: Stroke) {
        self.commands.push(DrawCommand::Arc {
            cx,
            cy,
            radius,
            start,
            end,
            color: stroke.color.to_hex(),
            width: stroke.width,
        });
    }

    fn fill_arc(&mut self, cx: f32, cy: f32, radius: f32, start: f32, end: f32, fill: Fill) {
        self.commands.push(DrawCommand::FillArc {
            cx,
            cy,
            radius,
            start,
            end,
            color: fill.color().to_hex(),
        });
    }

    fn draw_text(&mut self, text: &str, x: f32, y: f32, size: f32, color: Color, align: TextAlign) {
        let align = match align {
            TextAlign::Left => "left",
            TextAlign::Center => "center",
            TextAlign::Right => "right",
        };
        self.commands.push(DrawCommand::Text {
            text: text.to_string(),
            x,
            y,
            size,
            color: color.to_hex(),
            align: align.to_string(),
        });
    }

    fn measure_text(&self, text: &str, size: f32) -> f32 {
        if !size.is_finite() || size <= 0.0 {
            return 0.0;
        }
        // Canvas measurement is only available in the WebView process. This
        // conservative estimate keeps renderer-independent layout useful
        // until a round-trip metrics API is added.
        text.chars().count() as f32 * size * 0.55
    }

    fn save(&mut self) {}
    fn restore(&mut self) {}
    fn translate(&mut self, _x: f32, _y: f32) {}
    fn clip(&mut self, _x: f32, _y: f32, _width: f32, _height: f32) {}
    fn reset_clip(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_javascript_escapes_plugin_text_as_a_string_literal() {
        let mut context = WebViewContext::new(100.0, 50.0);
        context.draw_text(
            "quote':\\line\n</script>",
            4.0,
            8.0,
            12.0,
            Color::WHITE,
            TextAlign::Left,
        );

        let javascript = context.generate_js();
        assert!(
            javascript.contains("ctx.fillText(\"quote':\\\\line\\n\\u003c/script\\u003e\", 4, 8);")
        );
        assert!(!javascript.contains("</script>"));
        assert!(!javascript.contains("ctx.fillText('quote'"));
    }

    #[test]
    fn scale_and_text_metrics_reject_invalid_values() {
        let mut context = WebViewContext::new(100.0, 50.0);
        context.set_scale(2.0);
        context.set_scale(f32::NAN);
        assert_eq!(context.scale_factor(), 2.0);
        assert!(context.measure_text("gain", 12.0) > 0.0);
        assert_eq!(context.measure_text("gain", f32::NAN), 0.0);
    }
}
