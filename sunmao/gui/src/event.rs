//! Event system for GUI input handling.
//!
//! Provides a unified event model for mouse, keyboard, and touch input.

use crate::Point;

/// Mouse button
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Keyboard modifier keys
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool, // Command on macOS, Windows key on Windows
}

impl Modifiers {
    pub fn none() -> Self {
        Self::default()
    }
    
    pub fn shift() -> Self {
        Self { shift: true, ..Default::default() }
    }
    
    pub fn ctrl() -> Self {
        Self { ctrl: true, ..Default::default() }
    }
    
    pub fn alt() -> Self {
        Self { alt: true, ..Default::default() }
    }
}

/// Key code for keyboard events
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    // Alphanumeric
    A, B, C, D, E, F, G, H, I, J, K, L, M,
    N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Num0, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9,
    
    // Function keys
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    
    // Special keys
    Escape, Tab, Space, Enter, Backspace, Delete,
    Left, Right, Up, Down,
    Home, End, PageUp, PageDown,
    
    // Modifiers (for key events)
    Shift, Control, Alt, Meta,
    
    Unknown,
}

/// GUI input event
#[derive(Debug, Clone)]
pub enum Event {
    /// Mouse moved to position
    MouseMove {
        x: f32,
        y: f32,
        modifiers: Modifiers,
    },
    
    /// Mouse button pressed
    MouseDown {
        x: f32,
        y: f32,
        button: MouseButton,
        modifiers: Modifiers,
    },
    
    /// Mouse button released
    MouseUp {
        x: f32,
        y: f32,
        button: MouseButton,
        modifiers: Modifiers,
    },
    
    /// Mouse scroll/wheel
    Scroll {
        x: f32,
        y: f32,
        delta_x: f32,
        delta_y: f32,
        modifiers: Modifiers,
    },
    
    /// Mouse entered widget area
    MouseEnter,
    
    /// Mouse left widget area
    MouseLeave,
    
    /// Key pressed
    KeyDown {
        key: KeyCode,
        modifiers: Modifiers,
    },
    
    /// Key released
    KeyUp {
        key: KeyCode,
        modifiers: Modifiers,
    },
    
    /// Text input
    TextInput {
        text: String,
    },
    
    /// Widget gained focus
    FocusIn,
    
    /// Widget lost focus
    FocusOut,
    
    /// Drag started (for parameter widgets)
    DragStart {
        x: f32,
        y: f32,
    },
    
    /// Drag moved
    DragMove {
        x: f32,
        y: f32,
        delta_x: f32,
        delta_y: f32,
    },
    
    /// Drag ended
    DragEnd {
        x: f32,
        y: f32,
    },
}

impl Event {
    /// Get the mouse position for mouse events
    pub fn position(&self) -> Option<Point> {
        match self {
            Event::MouseMove { x, y, .. } |
            Event::MouseDown { x, y, .. } |
            Event::MouseUp { x, y, .. } |
            Event::Scroll { x, y, .. } |
            Event::DragStart { x, y } |
            Event::DragMove { x, y, .. } |
            Event::DragEnd { x, y } => Some(Point::new(*x, *y)),
            _ => None,
        }
    }
    
    /// Get modifiers for events that have them
    pub fn modifiers(&self) -> Modifiers {
        match self {
            Event::MouseMove { modifiers, .. } |
            Event::MouseDown { modifiers, .. } |
            Event::MouseUp { modifiers, .. } |
            Event::Scroll { modifiers, .. } |
            Event::KeyDown { modifiers, .. } |
            Event::KeyUp { modifiers, .. } => *modifiers,
            _ => Modifiers::none(),
        }
    }
}
