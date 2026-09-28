//! [`GourceApp`]: the whole Gource application behind a small,
//! frontend-agnostic interface.
//!
//! This is the port of `GourceShell` (gource_shell.cpp: repository
//! sequencing, transitions, window-level keys) driving `Gource` (gource.cpp:
//! reading commits, the scene, camera, HUD and input handling). The frontend
//! (the Bevy binary, or tests) calls, once per displayed frame:
//!
//! 1. [`GourceApp::input`] for each pending input event (in order),
//! 2. [`GourceApp::frame`] with the elapsed wall-clock time, which advances
//!    the simulation and tessellates the frame into a [`DrawList`],
//! 3. [`GourceApp::take_requests`] and executes the returned
//!    [`PlatformRequest`]s (quit, capture this frame, cursor, fullscreen...),
//!
//! and mirrors [`GourceApp::gfx`]'s texture store to the GPU.

use gource_core::StringHasher;
use gource_draw::{DrawList, Gfx};
use gource_settings::{Config, GourceSettings};
use gource_vcs::{CommitFilters, VcsOptions};

use crate::shell::GourceShell;
use crate::{InputEvent, PlatformRequest, Viewport};

/// A fatal error while starting up (reported like `SDLAppQuit`, e.g.
/// "failed to load resource 'foo.png'").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppError(pub String);

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AppError {}

/// How the frontend runs the app.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AppOptions {
    /// Frames are being recorded (`--output-ppm-stream`; the C++
    /// `FrameExporter` is non-null). The simulation then uses the fixed tick
    /// rate derived from `display.output_framerate`, emits
    /// [`PlatformRequest::CaptureFrame`] on the frames to record, hides the
    /// position slider and disables interactive repository switching, like
    /// the C++ code does when an exporter is present.
    pub recording: bool,
}

/// The running application.
pub struct GourceApp {
    shell: GourceShell,
}

const _: () = {
    fn assert_send<T: Send + 'static>() {}
    let _ = assert_send::<GourceApp>;
};

impl GourceApp {
    /// `GourceShell::GourceShell(conf, exporter)` + `init()`: load resources
    /// (textures, fonts) and prepare the first repository. Repositories are
    /// the `[gource]` sections of `config.conf` (one per `--path`/config
    /// section); each is imported with [`GourceSettings::import`].
    pub fn new(config: Config, options: AppOptions) -> Result<Self, AppError> {
        let shell = GourceShell::new(config, options)?;
        Ok(Self { shell })
    }

    /// Handle one input event (`GourceShell`/`Gource` key, mouse and window
    /// focus handlers).
    pub fn input(&mut self, event: &InputEvent) {
        self.shell.input(event);
    }

    /// `GourceShell::update(t, dt)`: advance by `dt` seconds of wall-clock
    /// time and draw the frame for `viewport` into `list` (resetting it
    /// first).
    pub fn frame(&mut self, dt: f32, viewport: Viewport, list: &mut DrawList) {
        self.shell.frame(dt, viewport, list);
    }

    /// Requests for the platform produced since the last call, in order.
    pub fn take_requests(&mut self) -> Vec<PlatformRequest> {
        self.shell.requests.drain(..).collect()
    }

    /// Textures and fonts referenced by the draw lists.
    pub fn gfx(&self) -> &Gfx {
        &self.shell.gfx
    }

    /// True once the app has finished (`appFinished`); a
    /// [`PlatformRequest::Quit`] has been emitted at that point.
    pub fn is_finished(&self) -> bool {
        self.shell.is_finished
    }

    /// The current display date string from the active simulation.
    pub fn display_date(&self) -> &str {
        self.shell
            .gource
            .as_ref()
            .map(|g| g.display_date.as_str())
            .unwrap_or("")
    }

    /// Access the underlying shell.
    pub fn shell(&self) -> &GourceShell {
        &self.shell
    }

    /// Mutable access to the underlying shell.
    pub fn shell_mut(&mut self) -> &mut GourceShell {
        &mut self.shell
    }
}

/// The VCS layer's view of the settings (what the C++ log readers take from
/// `gGourceSettings`): log format, git branch, date range, filters, hash
/// seed. Also used by the frontend for `--output-custom-log`.
pub fn vcs_options(settings: &GourceSettings) -> VcsOptions {
    VcsOptions {
        log_format: settings.log_format.clone(),
        git_branch: settings.git_branch.clone(),
        author_time: settings.author_time,
        start_timestamp: settings.start_timestamp,
        stop_timestamp: settings.stop_timestamp,
        default_path: settings.default_path,
        filters: CommitFilters {
            file_filters: settings.file_filters.clone(),
            file_show_filters: settings.file_show_filters.clone(),
            user_filters: settings.user_filters.clone(),
            user_show_filters: settings.user_show_filters.clone(),
        },
        hasher: StringHasher::new(settings.hash_seed),
    }
}
