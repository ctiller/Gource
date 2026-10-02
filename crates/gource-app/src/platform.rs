//! Requests from the simulation to the platform layer (window, cursor,
//! capture), and the viewport description the platform provides.

use gource_core::Vec2;
use std::path::PathBuf;

/// The drawable area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// Size in physical pixels (the draw list coordinate space).
    pub width: u32,
    pub height: u32,
    /// Physical / logical pixel ratio (`display.viewport_dpi_ratio`), >= 1 on
    /// HiDPI displays.
    pub dpi_ratio: f32,
}

impl Viewport {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            dpi_ratio: 1.0,
        }
    }

    pub fn size(&self) -> Vec2 {
        Vec2::new(self.width as f32, self.height as f32)
    }
}

/// Something the simulation wants the platform to do.
#[derive(Debug, Clone, PartialEq)]
pub enum PlatformRequest {
    /// Exit the application.
    Quit,
    /// Alt+Enter.
    ToggleFullscreen,
    /// F11.
    ToggleFrameless,
    /// Show or hide the system cursor.
    SetCursorVisible(bool),
    /// Grab (confine + hide) the cursor for dragging, or release it.
    SetCursorGrab(bool),
    /// Move the cursor to a window position.
    WarpCursor(Vec2),
    /// Save the frame being drawn now as a PNG (F12).
    Screenshot { path: PathBuf, with_alpha: bool },
    /// Send the frame being drawn now to the video exporter
    /// (`--output-ppm-stream`).
    CaptureFrame,
    /// Fatal error (`SDLAppQuit(message)` / an uncaught `SDLAppException`):
    /// the frontend prints `gource: {message}` and the "Try 'gource --help'"
    /// hint to stderr and exits with status 1.
    Fatal(String),
    /// Print the help text and exit with status 0 (an `SDLAppException`
    /// with `showHelp()` set, e.g. no repository found at the default path).
    ShowHelpAndExit,
}
