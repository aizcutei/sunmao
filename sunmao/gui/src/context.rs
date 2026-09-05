//! GUI Context - Abstraction for rendering operations.
//!
//! The `GuiContext` trait defines the interface that renderer backends must implement.
//! Widgets use this trait to draw themselves without knowing the underlying renderer.

/// Color representation (RGBA, 0.0-1.0 range)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    // Common colors
    pub const WHITE: Color = Color::rgb(1.0, 1.0, 1.0);
    pub const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);
    pub const RED: Color = Color::rgb(1.0, 0.0, 0.0);
    pub const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
    pub const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);
    pub const TRANSPARENT: Color = Color::rgba(0.0, 0.0, 0.0, 0.0);

    // UI colors
    pub const BACKGROUND: Color = Color::rgb(0.15, 0.15, 0.18);
    pub const FOREGROUND: Color = Color::rgb(0.9, 0.9, 0.9);
    pub const ACCENT: Color = Color::rgb(0.2, 0.6, 1.0);
    pub const HIGHLIGHT: Color = Color::rgb(0.3, 0.7, 1.0);

    /// Relative luminance, used to check that a colour pair stays readable.
    ///
    /// Rec. 709 coefficients on the stored (already linear) components. This is
    /// a contrast *heuristic* for theme checks, not a colour-managed value.
    pub fn luminance(&self) -> f32 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }

    pub fn to_hex(&self) -> String {
        let r = (self.r.clamp(0.0, 1.0) * 255.0).round() as u8;
        let g = (self.g.clamp(0.0, 1.0) * 255.0).round() as u8;
        let b = (self.b.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("#{:02X}{:02X}{:02X}", r, g, b)
    }
}

impl From<u32> for Color {
    fn from(hex: u32) -> Self {
        let r = ((hex >> 16) & 0xFF) as f32 / 255.0;
        let g = ((hex >> 8) & 0xFF) as f32 / 255.0;
        let b = (hex & 0xFF) as f32 / 255.0;
        Color::rgb(r, g, b)
    }
}

/// Font style for text rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontStyle {
    Normal,
    Bold,
    Italic,
    BoldItalic,
}

/// Text alignment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

/// Text vertical alignment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextVAlign {
    Top,
    Middle,
    Bottom,
}

/// Stroke style for lines and outlines
#[derive(Debug, Clone, Copy)]
pub struct Stroke {
    pub color: Color,
    pub width: f32,
}

impl Stroke {
    pub fn new(color: Color, width: f32) -> Self {
        Self { color, width }
    }
}

/// Fill style for shapes
#[derive(Debug, Clone, Copy)]
pub enum Fill {
    Solid(Color),
    // Future: Gradient, Pattern, etc.
}

impl Fill {
    pub fn color(self) -> Color {
        match self {
            Fill::Solid(color) => color,
        }
    }
}

/// Abstraction for rendering operations.
///
/// Renderer backends implement this trait to provide drawing capabilities.
/// Widgets use this trait to draw themselves without knowing the underlying renderer.
pub trait GuiContext {
    /// Get the current viewport size
    fn size(&self) -> (f32, f32);

    /// Get the current scale factor (for HiDPI)
    fn scale_factor(&self) -> f32 {
        1.0
    }

    // --- Drawing primitives ---

    /// Fill a rectangle
    fn fill_rect(&mut self, x: f32, y: f32, width: f32, height: f32, fill: Fill);

    /// Stroke a rectangle outline
    fn stroke_rect(&mut self, x: f32, y: f32, width: f32, height: f32, stroke: Stroke);

    /// Fill a rounded rectangle
    fn fill_rounded_rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
        fill: Fill,
    );

    /// Stroke a rounded rectangle
    fn stroke_rounded_rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
        stroke: Stroke,
    );

    /// Fill a circle
    fn fill_circle(&mut self, cx: f32, cy: f32, radius: f32, fill: Fill);

    /// Stroke a circle outline
    fn stroke_circle(&mut self, cx: f32, cy: f32, radius: f32, stroke: Stroke);

    /// Draw a line
    fn stroke_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, stroke: Stroke);

    /// Draw an arc (for knobs)
    fn stroke_arc(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        stroke: Stroke,
    );

    /// Fill an arc segment
    fn fill_arc(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        fill: Fill,
    );

    // --- Text rendering ---

    /// Draw text
    fn draw_text(&mut self, text: &str, x: f32, y: f32, size: f32, color: Color, align: TextAlign);

    /// Measure text width
    fn measure_text(&self, text: &str, size: f32) -> f32;

    // --- State management ---

    /// Save the current transform state
    fn save(&mut self);

    /// Restore the previous transform state
    fn restore(&mut self);

    /// Translate the coordinate system
    fn translate(&mut self, x: f32, y: f32);

    /// Set a clipping rectangle
    fn clip(&mut self, x: f32, y: f32, width: f32, height: f32);

    /// Clear the clipping region
    fn reset_clip(&mut self);
}

/// A null context for testing or when no GUI is needed
pub struct NullContext {
    width: f32,
    height: f32,
}

impl NullContext {
    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

impl GuiContext for NullContext {
    fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }
    fn fill_rect(&mut self, _: f32, _: f32, _: f32, _: f32, _: Fill) {}
    fn stroke_rect(&mut self, _: f32, _: f32, _: f32, _: f32, _: Stroke) {}
    fn fill_rounded_rect(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: Fill) {}
    fn stroke_rounded_rect(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: Stroke) {}
    fn fill_circle(&mut self, _: f32, _: f32, _: f32, _: Fill) {}
    fn stroke_circle(&mut self, _: f32, _: f32, _: f32, _: Stroke) {}
    fn stroke_line(&mut self, _: f32, _: f32, _: f32, _: f32, _: Stroke) {}
    fn stroke_arc(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: Stroke) {}
    fn fill_arc(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: Fill) {}
    fn draw_text(&mut self, _: &str, _: f32, _: f32, _: f32, _: Color, _: TextAlign) {}
    fn measure_text(&self, _: &str, _: f32) -> f32 {
        0.0
    }
    fn save(&mut self) {}
    fn restore(&mut self) {}
    fn translate(&mut self, _: f32, _: f32) {}
    fn clip(&mut self, _: f32, _: f32, _: f32, _: f32) {}
    fn reset_clip(&mut self) {}
}
