//! Window configuration from the display settings (port of the
//! `SDLAppDisplay` setup in `main.cpp` / `display.cpp`) and the window-level
//! platform requests.

use bevy::{
    prelude::*,
    window::{
        CompositeAlphaMode, CursorGrabMode, CursorOptions, MonitorSelection, PresentMode,
        WindowMode, WindowPosition, WindowResolution,
    },
};
use gource_settings::DisplaySettings;

/// `SDLAppInit("Gource", ...)`.
pub const WINDOW_TITLE: &str = "Gource";

/// The monitor for `--screen N` (1-based; anything else means the primary
/// monitor, like the C++ check against `SDL_GetNumVideoDisplays`).
pub fn monitor(screen: i32) -> MonitorSelection {
    if screen > 0 {
        MonitorSelection::Index((screen - 1) as usize)
    } else {
        MonitorSelection::Primary
    }
}

/// Whether the C++ would enable high DPI awareness: always, except when a
/// viewport size was given without `--high-dpi` (so that recording at a
/// fixed resolution gets exactly that many pixels).
pub fn high_dpi(display: &DisplaySettings) -> bool {
    !(display.viewport_specified && !display.high_dpi)
}

/// Build the primary window for `display`.
///
/// Differences from SDL: fullscreen always uses the desktop resolution
/// (borderless fullscreen, as SDL's own fullscreen toggle does) rather than
/// switching video modes; a zero width or height in windowed mode opens a
/// maximized window.
pub fn primary_window(display: &DisplaySettings) -> Window {
    let recording = !display.output_ppm_filename.is_empty();

    let width = display.display_width.max(0) as u32;
    let height = display.display_height.max(0) as u32;
    let (width, height) = if width == 0 || height == 0 {
        (1024, 768)
    } else {
        (width, height)
    };

    let mut resolution = WindowResolution::new(width, height);
    if !high_dpi(display) {
        resolution = resolution.with_scale_factor_override(1.0);
    }

    let mode = if display.fullscreen {
        WindowMode::BorderlessFullscreen(monitor(display.screen))
    } else {
        WindowMode::Windowed
    };

    let position = if !display.fullscreen && display.window_x >= 0 && display.window_y >= 0 {
        WindowPosition::At(IVec2::new(display.window_x, display.window_y))
    } else {
        WindowPosition::Centered(monitor(display.screen))
    };

    let mut window = Window {
        title: WINDOW_TITLE.to_owned(),
        resolution,
        position,
        mode,
        present_mode: if display.vsync {
            PresentMode::AutoVsync
        } else {
            PresentMode::AutoNoVsync
        },
        decorations: !display.frameless,
        // `SDL_WINDOW_RESIZABLE` only when not recording and not frameless.
        resizable: display.resizable && !recording && !display.frameless,
        transparent: display.transparent,
        ..default()
    };

    if display.transparent {
        // The GL window keeps an alpha channel; compositors treat it as
        // premultiplied on Linux.
        window.composite_alpha_mode = if cfg!(target_os = "macos") {
            CompositeAlphaMode::PostMultiplied
        } else {
            CompositeAlphaMode::PreMultiplied
        };
    }

    if !display.fullscreen && (display.display_width <= 0 || display.display_height <= 0) {
        window.set_maximized(true);
    }

    window
}

/// `display.toggleFullscreen()`.
pub fn toggle_fullscreen(window: &mut Window, screen: i32) {
    window.mode = match window.mode {
        WindowMode::Windowed => WindowMode::BorderlessFullscreen(monitor(screen)),
        _ => WindowMode::Windowed,
    };
}

/// `display.toggleFrameless()`: ignored in fullscreen.
pub fn toggle_frameless(window: &mut Window) {
    if window.mode != WindowMode::Windowed {
        return;
    }
    window.decorations = !window.decorations;
}

/// `SDL_ShowCursor`.
pub fn set_cursor_visible(cursor: &mut CursorOptions, visible: bool) {
    cursor.visible = visible;
}

/// Grab the cursor while dragging (`SDL_SetWindowGrab` + hidden cursor), or
/// release it. Relative motion keeps arriving as `MouseMotion`.
pub fn set_cursor_grab(cursor: &mut CursorOptions, grab: bool) {
    cursor.grab_mode = if grab {
        CursorGrabMode::Locked
    } else {
        CursorGrabMode::None
    };
    cursor.visible = !grab;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display() -> DisplaySettings {
        DisplaySettings::default()
    }

    #[test]
    fn defaults_match_sdl_setup() {
        let window = primary_window(&display());
        assert_eq!(window.title, "Gource");
        assert_eq!(window.resolution.physical_width(), 1024);
        assert_eq!(window.resolution.physical_height(), 768);
        assert_eq!(window.mode, WindowMode::Windowed);
        assert_eq!(window.present_mode, PresentMode::AutoVsync);
        assert!(window.resizable);
        assert!(window.decorations);
        assert!(!window.transparent);
        assert_eq!(
            window.position,
            WindowPosition::Centered(MonitorSelection::Primary)
        );
    }

    #[test]
    fn viewport_without_high_dpi_is_exact_pixels() {
        let mut d = display();
        d.display_width = 1280;
        d.display_height = 720;
        d.viewport_specified = true;
        let window = primary_window(&d);
        assert_eq!(window.resolution.scale_factor_override(), Some(1.0));
        assert_eq!(window.resolution.physical_width(), 1280);

        d.high_dpi = true;
        assert!(high_dpi(&d));
        assert_eq!(primary_window(&d).resolution.scale_factor_override(), None);
    }

    #[test]
    fn recording_and_frameless_disable_resizing() {
        let mut d = display();
        d.output_ppm_filename = "out.ppm".into();
        assert!(!primary_window(&d).resizable);
        let mut d = display();
        d.frameless = true;
        let window = primary_window(&d);
        assert!(!window.resizable);
        assert!(!window.decorations);
    }

    #[test]
    fn fullscreen_screen_position_and_vsync() {
        let mut d = display();
        d.fullscreen = true;
        d.screen = 2;
        d.window_x = 10;
        d.window_y = 20;
        d.vsync = false;
        let window = primary_window(&d);
        assert_eq!(
            window.mode,
            WindowMode::BorderlessFullscreen(MonitorSelection::Index(1))
        );
        assert_eq!(
            window.position,
            WindowPosition::Centered(MonitorSelection::Index(1))
        );
        assert_eq!(window.present_mode, PresentMode::AutoNoVsync);

        d.fullscreen = false;
        assert_eq!(
            primary_window(&d).position,
            WindowPosition::At(IVec2::new(10, 20))
        );
    }

    #[test]
    fn transparent_and_zero_size() {
        let mut d = display();
        d.transparent = true;
        d.display_width = 0;
        let window = primary_window(&d);
        assert!(window.transparent);
        assert_eq!(
            window.composite_alpha_mode,
            CompositeAlphaMode::PreMultiplied
        );
        assert_eq!(window.resolution.physical_width(), 1024);
    }

    #[test]
    fn toggles() {
        let mut window = primary_window(&display());
        toggle_frameless(&mut window);
        assert!(!window.decorations);
        toggle_fullscreen(&mut window, -1);
        assert_eq!(
            window.mode,
            WindowMode::BorderlessFullscreen(MonitorSelection::Primary)
        );
        toggle_frameless(&mut window);
        assert!(
            !window.decorations,
            "frameless toggle is ignored in fullscreen"
        );
        toggle_fullscreen(&mut window, -1);
        assert_eq!(window.mode, WindowMode::Windowed);

        let mut cursor = CursorOptions::default();
        set_cursor_grab(&mut cursor, true);
        assert_eq!(cursor.grab_mode, CursorGrabMode::Locked);
        assert!(!cursor.visible);
        set_cursor_grab(&mut cursor, false);
        assert_eq!(cursor.grab_mode, CursorGrabMode::None);
        assert!(cursor.visible);
        set_cursor_visible(&mut cursor, false);
        assert!(!cursor.visible);
    }
}
