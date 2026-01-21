//! Layout system for positioning widgets.
//!
//! Provides basic layout primitives like rectangles and positioning helpers.

/// A rectangle defined by position and size
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
    
    pub const fn from_pos_size(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
    
    pub const fn zero() -> Self {
        Self { x: 0.0, y: 0.0, width: 0.0, height: 0.0 }
    }
    
    pub fn right(&self) -> f32 { self.x + self.width }
    pub fn bottom(&self) -> f32 { self.y + self.height }
    pub fn center_x(&self) -> f32 { self.x + self.width / 2.0 }
    pub fn center_y(&self) -> f32 { self.y + self.height / 2.0 }
    
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }
    
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.right() && self.right() > other.x &&
        self.y < other.bottom() && self.bottom() > other.y
    }
    
    pub fn inset(&self, amount: f32) -> Rect {
        Rect {
            x: self.x + amount,
            y: self.y + amount,
            width: (self.width - amount * 2.0).max(0.0),
            height: (self.height - amount * 2.0).max(0.0),
        }
    }
    
    pub fn offset(&self, dx: f32, dy: f32) -> Rect {
        Rect {
            x: self.x + dx,
            y: self.y + dy,
            width: self.width,
            height: self.height,
        }
    }
}

/// A 2D point
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    
    pub const fn zero() -> Self {
        Self { x: 0.0, y: 0.0 }
    }
}

/// A 2D size
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
    
    pub const fn zero() -> Self {
        Self { width: 0.0, height: 0.0 }
    }
}

/// Padding values for layout
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Padding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Padding {
    pub const fn all(value: f32) -> Self {
        Self { top: value, right: value, bottom: value, left: value }
    }
    
    pub const fn horizontal(value: f32) -> Self {
        Self { top: 0.0, right: value, bottom: 0.0, left: value }
    }
    
    pub const fn vertical(value: f32) -> Self {
        Self { top: value, right: 0.0, bottom: value, left: 0.0 }
    }
    
    pub const fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self { top, right, bottom, left }
    }
}

/// Alignment options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Alignment {
    #[default]
    Start,
    Center,
    End,
}

/// Layout direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Horizontal,
    Vertical,
}

/// Simple layout helper for arranging widgets
pub struct Layout {
    pub bounds: Rect,
    pub padding: Padding,
    pub spacing: f32,
    pub direction: Direction,
    cursor: f32,
}

impl Layout {
    pub fn new(bounds: Rect) -> Self {
        Self {
            bounds,
            padding: Padding::all(0.0),
            spacing: 8.0,
            direction: Direction::Horizontal,
            cursor: 0.0,
        }
    }
    
    pub fn horizontal(bounds: Rect) -> Self {
        Self {
            direction: Direction::Horizontal,
            cursor: bounds.x,
            ..Self::new(bounds)
        }
    }
    
    pub fn vertical(bounds: Rect) -> Self {
        Self {
            direction: Direction::Vertical,
            cursor: bounds.y,
            ..Self::new(bounds)
        }
    }
    
    /// Allocate space for the next widget
    pub fn allocate(&mut self, width: f32, height: f32) -> Rect {
        let rect = match self.direction {
            Direction::Horizontal => {
                let r = Rect::new(
                    self.cursor + self.padding.left,
                    self.bounds.y + self.padding.top,
                    width,
                    height,
                );
                self.cursor += width + self.spacing;
                r
            }
            Direction::Vertical => {
                let r = Rect::new(
                    self.bounds.x + self.padding.left,
                    self.cursor + self.padding.top,
                    width,
                    height,
                );
                self.cursor += height + self.spacing;
                r
            }
        };
        rect
    }
    
    /// Get remaining space
    pub fn remaining(&self) -> Rect {
        match self.direction {
            Direction::Horizontal => Rect::new(
                self.cursor,
                self.bounds.y,
                self.bounds.right() - self.cursor,
                self.bounds.height,
            ),
            Direction::Vertical => Rect::new(
                self.bounds.x,
                self.cursor,
                self.bounds.width,
                self.bounds.bottom() - self.cursor,
            ),
        }
    }
}
