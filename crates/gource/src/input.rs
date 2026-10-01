//! Translation of Bevy/winit input into [`gource_app::InputEvent`]s.
//!
//! Window events are read from the ordered [`WindowEvent`] stream so key and
//! mouse events reach the simulation in the order they happened (like the
//! SDL event loop of the C++ version). Positions are converted from logical
//! to physical pixels.

use bevy::{
    input::{
        ButtonState,
        keyboard::{Key as BevyKey, KeyCode, KeyboardInput},
        mouse::{MouseButton as BevyMouseButton, MouseScrollUnit},
    },
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow, WindowEvent},
};
use gource_app::{InputEvent, Key, Modifiers, MouseButton};

use crate::sim::SimResource;

/// Undo the US-layout Shift mapping for symbol keys, so e.g. Shift+'.'
/// reports '.' like an SDL keysym does (SDL keysyms are unshifted).
pub fn unshift_us(c: char) -> char {
    match c {
        '+' => '=',
        '_' => '-',
        '>' => '.',
        '<' => ',',
        '{' => '[',
        '}' => ']',
        '?' => '/',
        ':' => ';',
        '"' => '\'',
        '~' => '`',
        '|' => '\\',
        '!' => '1',
        '@' => '2',
        '#' => '3',
        '$' => '4',
        '%' => '5',
        '^' => '6',
        '&' => '7',
        '*' => '8',
        '(' => '9',
        ')' => '0',
        other => other,
    }
}

/// Map a key press to a Gource key. Named keys come from the physical key
/// code; printable keys from the logical (layout-aware) key, lowercased.
///
/// The C++ version tests both `SDLK_PLUS` and `SDLK_EQUALS` for zooming, so
/// '+' is kept as-is unless Shift produced it.
pub fn map_key(code: KeyCode, logical: &BevyKey, shift: bool) -> Key {
    match code {
        KeyCode::Escape => return Key::Escape,
        KeyCode::Enter | KeyCode::NumpadEnter => return Key::Return,
        KeyCode::Tab => return Key::Tab,
        KeyCode::Space => return Key::Space,
        KeyCode::Backspace => return Key::Backspace,
        KeyCode::ArrowUp => return Key::Up,
        KeyCode::ArrowDown => return Key::Down,
        KeyCode::ArrowLeft => return Key::Left,
        KeyCode::ArrowRight => return Key::Right,
        KeyCode::F1 => return Key::F1,
        KeyCode::F2 => return Key::F2,
        KeyCode::F3 => return Key::F3,
        KeyCode::F4 => return Key::F4,
        KeyCode::F5 => return Key::F5,
        KeyCode::F11 => return Key::F11,
        KeyCode::F12 => return Key::F12,
        KeyCode::NumpadAdd => return Key::KeypadPlus,
        KeyCode::NumpadSubtract => return Key::KeypadMinus,
        _ => {}
    }
    match logical {
        BevyKey::Character(s) => match s.chars().next() {
            Some(c) => {
                let c = c.to_lowercase().next().unwrap_or(c);
                Key::Char(if shift { unshift_us(c) } else { c })
            }
            None => Key::Other,
        },
        BevyKey::Space => Key::Space,
        BevyKey::Enter => Key::Return,
        BevyKey::Backspace => Key::Backspace,
        BevyKey::Escape => Key::Escape,
        BevyKey::Tab => Key::Tab,
        BevyKey::F1 => Key::F1,
        BevyKey::F2 => Key::F2,
        BevyKey::F3 => Key::F3,
        BevyKey::F4 => Key::F4,
        BevyKey::F5 => Key::F5,
        BevyKey::F11 => Key::F11,
        BevyKey::F12 => Key::F12,
        _ => Key::Other,
    }
}

/// Track modifier state from key events. Returns true if `code` is a
/// modifier key.
pub fn update_modifiers(modifiers: &mut Modifiers, code: KeyCode, pressed: bool) -> bool {
    match code {
        KeyCode::ShiftLeft | KeyCode::ShiftRight => modifiers.shift = pressed,
        KeyCode::ControlLeft | KeyCode::ControlRight => modifiers.ctrl = pressed,
        KeyCode::AltLeft | KeyCode::AltRight => modifiers.alt = pressed,
        KeyCode::SuperLeft | KeyCode::SuperRight => modifiers.meta = pressed,
        _ => return false,
    }
    true
}

pub fn map_mouse_button(button: BevyMouseButton) -> Option<MouseButton> {
    match button {
        BevyMouseButton::Left => Some(MouseButton::Left),
        BevyMouseButton::Middle => Some(MouseButton::Middle),
        BevyMouseButton::Right => Some(MouseButton::Right),
        _ => None,
    }
}

/// Wheel motion in "clicks": positive = away from the user.
pub fn wheel_delta(unit: MouseScrollUnit, y: f32) -> f32 {
    match unit {
        MouseScrollUnit::Line => y,
        MouseScrollUnit::Pixel => y / MouseScrollUnit::SCROLL_UNIT_CONVERSION_FACTOR,
    }
}

/// Input state carried between frames.
#[derive(Default)]
pub struct InputState {
    pub modifiers: Modifiers,
    /// Last cursor position in physical pixels.
    pub cursor: Vec2,
}

/// Convert one window event. `scale` is the window scale factor, `grabbed`
/// whether the cursor is currently grabbed (relative motion mode).
pub fn convert_event(
    state: &mut InputState,
    event: &WindowEvent,
    scale: f32,
    grabbed: bool,
) -> Option<InputEvent> {
    match event {
        WindowEvent::KeyboardInput(KeyboardInput {
            key_code,
            logical_key,
            state: key_state,
            repeat,
            ..
        }) => {
            let pressed = *key_state == ButtonState::Pressed;
            update_modifiers(&mut state.modifiers, *key_code, pressed);
            let key = map_key(*key_code, logical_key, state.modifiers.shift);
            Some(if pressed {
                InputEvent::KeyDown {
                    key,
                    modifiers: state.modifiers,
                    repeat: *repeat,
                }
            } else {
                InputEvent::KeyUp {
                    key,
                    modifiers: state.modifiers,
                }
            })
        }
        WindowEvent::CursorMoved(moved) => {
            let pos = moved.position * scale;
            let delta = moved.delta.map(|d| d * scale).unwrap_or(pos - state.cursor);
            state.cursor = pos;
            // While grabbed, motion comes from the raw `MouseMotion` stream
            // only (X11 confines instead of locking, so both would arrive).
            (!grabbed).then_some(InputEvent::MouseMove { pos, delta })
        }
        WindowEvent::MouseMotion(motion) if grabbed => Some(InputEvent::MouseMove {
            pos: state.cursor,
            delta: motion.delta,
        }),
        WindowEvent::MouseButtonInput(input) => {
            map_mouse_button(input.button).map(|button| InputEvent::MouseButton {
                button,
                pressed: input.state == ButtonState::Pressed,
                pos: state.cursor,
            })
        }
        WindowEvent::MouseWheel(wheel) => {
            let delta = wheel_delta(wheel.unit, wheel.y);
            (delta != 0.0).then_some(InputEvent::MouseWheel { delta })
        }
        WindowEvent::WindowFocused(focus) => Some(InputEvent::Focus(focus.focused)),
        _ => None,
    }
}

/// Feeds window events to the simulation.
pub fn forward_input(
    mut events: MessageReader<WindowEvent>,
    windows: Query<(&Window, &CursorOptions), With<PrimaryWindow>>,
    sim: Res<SimResource>,
    mut state: Local<InputState>,
) {
    let (scale, grabbed) = windows
        .single()
        .map(|(w, c)| (w.scale_factor(), c.grab_mode != CursorGrabMode::None))
        .unwrap_or((1.0, false));
    let inputs: Vec<InputEvent> = events
        .read()
        .filter_map(|event| convert_event(&mut state, event, scale, grabbed))
        .collect();
    if inputs.is_empty() {
        return;
    }
    let mut sim = sim.lock();
    for input in &inputs {
        sim.input(input);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::keyboard::NativeKey;

    fn ch(s: &str) -> BevyKey {
        BevyKey::Character(s.into())
    }

    #[test]
    fn named_keys_come_from_key_codes() {
        assert_eq!(
            map_key(KeyCode::Escape, &BevyKey::Escape, false),
            Key::Escape
        );
        assert_eq!(
            map_key(KeyCode::NumpadEnter, &BevyKey::Enter, false),
            Key::Return
        );
        assert_eq!(map_key(KeyCode::F1, &BevyKey::F1, false), Key::F1);
        assert_eq!(map_key(KeyCode::F2, &BevyKey::F2, false), Key::F2);
        assert_eq!(map_key(KeyCode::F3, &BevyKey::F3, false), Key::F3);
        assert_eq!(map_key(KeyCode::F4, &BevyKey::F4, false), Key::F4);
        assert_eq!(map_key(KeyCode::F5, &BevyKey::F5, false), Key::F5);
        assert_eq!(map_key(KeyCode::F11, &BevyKey::F11, false), Key::F11);
        assert_eq!(map_key(KeyCode::F12, &BevyKey::F12, false), Key::F12);
        assert_eq!(map_key(KeyCode::Fn, &BevyKey::F2, false), Key::F2);
        assert_eq!(
            map_key(KeyCode::NumpadAdd, &ch("+"), false),
            Key::KeypadPlus
        );
        assert_eq!(
            map_key(KeyCode::NumpadSubtract, &ch("-"), false),
            Key::KeypadMinus
        );
        assert_eq!(
            map_key(KeyCode::ArrowLeft, &BevyKey::ArrowLeft, false),
            Key::Left
        );
    }

    #[test]
    fn printable_keys_are_logical_and_lowercase() {
        // AZERTY: the physical Q key produces 'a'.
        assert_eq!(map_key(KeyCode::KeyQ, &ch("a"), false), Key::Char('a'));
        assert_eq!(map_key(KeyCode::KeyA, &ch("A"), true), Key::Char('a'));
        assert_eq!(map_key(KeyCode::Equal, &ch("+"), true), Key::Char('='));
        assert_eq!(map_key(KeyCode::Equal, &ch("+"), false), Key::Char('+'));
        assert_eq!(map_key(KeyCode::Period, &ch(">"), true), Key::Char('.'));
        assert_eq!(
            map_key(KeyCode::BracketLeft, &ch("["), false),
            Key::Char('[')
        );
        assert_eq!(
            map_key(
                KeyCode::Fn,
                &BevyKey::Unidentified(NativeKey::Unidentified),
                false
            ),
            Key::Other
        );
        assert_eq!(map_key(KeyCode::KeyZ, &ch(""), false), Key::Other);
    }

    #[test]
    fn modifiers_track_presses() {
        let mut m = Modifiers::default();
        assert!(update_modifiers(&mut m, KeyCode::ShiftRight, true));
        assert!(update_modifiers(&mut m, KeyCode::AltLeft, true));
        assert!(m.shift && m.alt && !m.ctrl);
        assert!(update_modifiers(&mut m, KeyCode::ShiftRight, false));
        assert!(!m.shift);
        assert!(!update_modifiers(&mut m, KeyCode::KeyA, true));
    }

    #[test]
    fn mouse_mapping() {
        assert_eq!(
            map_mouse_button(BevyMouseButton::Middle),
            Some(MouseButton::Middle)
        );
        assert_eq!(map_mouse_button(BevyMouseButton::Back), None);
        assert_eq!(wheel_delta(MouseScrollUnit::Line, -1.0), -1.0);
        assert_eq!(wheel_delta(MouseScrollUnit::Pixel, 200.0), 2.0);
    }
}
