//! Mapping DOM input to [`gource_app::InputEvent`]s.

use gource_app::input::{Key, Modifiers, MouseButton};

/// `KeyboardEvent.key` -> [`Key`].
pub fn key(name: &str) -> Key {
    match name {
        "Escape" => Key::Escape,
        "Enter" => Key::Return,
        "Tab" => Key::Tab,
        " " => Key::Space,
        "Backspace" => Key::Backspace,
        "ArrowUp" => Key::Up,
        "ArrowDown" => Key::Down,
        "ArrowLeft" => Key::Left,
        "ArrowRight" => Key::Right,
        "F1" => Key::F1,
        "F2" => Key::F2,
        "F3" => Key::F3,
        "F4" => Key::F4,
        "F5" => Key::F5,
        "F11" => Key::F11,
        "F12" => Key::F12,
        _ => {
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Key::Char(c),
                _ => Key::Other,
            }
        }
    }
}

/// `KeyboardEvent.code` distinguishes the keypad's + and -.
pub fn key_with_code(name: &str, code: &str) -> Key {
    match code {
        "NumpadAdd" => Key::KeypadPlus,
        "NumpadSubtract" => Key::KeypadMinus,
        _ => key(name),
    }
}

pub fn modifiers(shift: bool, ctrl: bool, alt: bool, meta: bool) -> Modifiers {
    Modifiers {
        shift,
        ctrl,
        alt,
        meta,
    }
}

/// `MouseEvent.button` -> [`MouseButton`].
pub fn mouse_button(button: i16) -> Option<MouseButton> {
    match button {
        0 => Some(MouseButton::Left),
        1 => Some(MouseButton::Middle),
        2 => Some(MouseButton::Right),
        _ => None,
    }
}

/// `WheelEvent.deltaY` (positive = towards the user, in pixels or lines)
/// -> Gource wheel steps (positive = away from the user).
pub fn wheel_steps(delta_y: f64) -> f32 {
    if delta_y == 0.0 {
        0.0
    } else {
        -(delta_y.signum() as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        assert_eq!(key("Escape"), Key::Escape);
        assert_eq!(key("Enter"), Key::Return);
        assert_eq!(key(" "), Key::Space);
        assert_eq!(key("ArrowLeft"), Key::Left);
        assert_eq!(key("F5"), Key::F5);
        assert_eq!(key("q"), Key::Char('q'));
        assert_eq!(key("é"), Key::Char('é'));
        assert_eq!(key("Shift"), Key::Other);
        assert_eq!(key(""), Key::Other);
        assert_eq!(key_with_code("+", "NumpadAdd"), Key::KeypadPlus);
        assert_eq!(key_with_code("-", "NumpadSubtract"), Key::KeypadMinus);
        assert_eq!(key_with_code("+", "Equal"), Key::Char('+'));
        for (name, k) in [
            ("Tab", Key::Tab),
            ("Backspace", Key::Backspace),
            ("ArrowUp", Key::Up),
            ("ArrowDown", Key::Down),
            ("ArrowRight", Key::Right),
            ("F1", Key::F1),
            ("F2", Key::F2),
            ("F3", Key::F3),
            ("F4", Key::F4),
            ("F11", Key::F11),
            ("F12", Key::F12),
        ] {
            assert_eq!(key(name), k);
        }
    }

    #[test]
    fn mouse() {
        assert_eq!(mouse_button(0), Some(MouseButton::Left));
        assert_eq!(mouse_button(1), Some(MouseButton::Middle));
        assert_eq!(mouse_button(2), Some(MouseButton::Right));
        assert_eq!(mouse_button(3), None);
        assert_eq!(wheel_steps(100.0), -1.0);
        assert_eq!(wheel_steps(-3.0), 1.0);
        assert_eq!(wheel_steps(0.0), 0.0);
        assert!(modifiers(true, false, true, false).alt);
    }
}
