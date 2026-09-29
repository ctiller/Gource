//! Platform-independent input events (replacing SDL events).

use glam::Vec2;

/// Keys Gource reacts to. Printable keys are reported as lowercase
/// characters of the *logical* key (layout aware, like SDL keysyms), e.g.
/// `Char('=')`, `Char('+')`, `Char('[')`, `Char('.')`, `Char('a')`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Escape,
    Return,
    Tab,
    Space,
    Up,
    Down,
    Left,
    Right,
    F1,
    F2,
    F3,
    F4,
    F5,
    F11,
    F12,
    KeypadPlus,
    KeypadMinus,
    Char(char),
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

/// An input event in window pixel coordinates (origin top-left, the same
/// space as the draw list).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputEvent {
    KeyDown {
        key: Key,
        modifiers: Modifiers,
        /// True for auto-repeat events.
        repeat: bool,
    },
    KeyUp {
        key: Key,
        modifiers: Modifiers,
    },
    /// Cursor moved to `pos`; `delta` is the relative motion (also reported
    /// while the cursor is grabbed).
    MouseMove {
        pos: Vec2,
        delta: Vec2,
    },
    MouseButton {
        button: MouseButton,
        pressed: bool,
        pos: Vec2,
    },
    /// Wheel motion: positive = away from the user (SDL `wheel.y > 0`).
    MouseWheel {
        delta: f32,
    },
    /// Window focus gained (`true`) or lost.
    Focus(bool),
}
