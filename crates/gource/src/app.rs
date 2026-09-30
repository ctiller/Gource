//! Assembles the Bevy app around a [`Simulation`]: window, input, the frame
//! loop, platform requests, capture and exit handling (the SDL main loop of
//! `SDLApp::run` plus the setup in `main.cpp`).

use std::time::Duration;

use bevy::{
    app::AppExit,
    log::{Level, LogPlugin},
    prelude::*,
    render::RenderPlugin,
    window::{CursorOptions, ExitCondition, PrimaryWindow, WindowCloseRequested, WindowPlugin},
};
use gource_draw::VideoSink;
use gource_settings::{DisplaySettings, LogLevel, help::help_text};
use gource_sim::{PlatformRequest, Viewport};

use crate::{
    capture::{CaptureJob, Deadline, MAX_FRAMES_IN_FLIGHT, Recorder, spawn_capture},
    cli::quit_message,
    input::forward_input,
    render::{DrawListPlugin, DrawListSet, FrameDrawList, FrameTextures, GpuTextures},
    sim::{SimResource, Simulation},
    warmup::{Warmup, run_warmup},
    window,
};

/// How long to wait for outstanding video frames when quitting.
pub const EXIT_FLUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// Everything the frontend needs besides the simulation.
pub struct AppConfig {
    pub display: DisplaySettings,
    pub log_level: LogLevel,
    /// Video or PPM stream writer, when recording.
    pub exporter: Option<Box<dyn VideoSink>>,
}

/// Map `--log-level` to the tracing level of Bevy's logger. The C++ default
/// (off) still shows errors from the renderer.
pub fn tracing_level(level: LogLevel) -> Level {
    match level {
        LogLevel::Off | LogLevel::Error => Level::ERROR,
        LogLevel::Warn => Level::WARN,
        LogLevel::Console | LogLevel::Info | LogLevel::Script => Level::INFO,
        LogLevel::Debug => Level::DEBUG,
        LogLevel::Pedantic => Level::TRACE,
    }
}

/// The viewport of a window: its drawable size in physical pixels.
pub fn viewport_of(window: &Window) -> Viewport {
    Viewport {
        width: window.physical_width(),
        height: window.physical_height(),
        dpi_ratio: window.scale_factor(),
    }
}

/// Requests returned by the simulation this frame, executed by
/// [`handle_requests`].
#[derive(Resource, Default)]
pub struct PendingRequests(pub Vec<PlatformRequest>);

/// `--screen`, needed when toggling fullscreen.
#[derive(Resource, Clone, Copy)]
pub struct ScreenSetting(pub i32);

/// How and when to leave the app.
#[derive(Resource, Default, Debug)]
pub struct ExitState {
    quitting: bool,
    done: bool,
    stdout: String,
    stderr: String,
    code: u8,
    deadline: Option<Deadline>,
}

impl ExitState {
    /// Leave normally once outstanding frames are written.
    pub fn quit(&mut self) {
        self.quitting = true;
    }

    /// `SDLAppQuit(message)`.
    pub fn fail(&mut self, message: &str) {
        if !self.quitting || self.code == 0 {
            self.stderr = quit_message(message);
            self.code = 1;
        }
        self.quitting = true;
    }

    /// Print the help text and exit successfully.
    pub fn help(&mut self) {
        if !self.quitting {
            self.stdout = help_text(false);
        }
        self.quitting = true;
    }

    pub fn is_quitting(&self) -> bool {
        self.quitting
    }

    pub fn code(&self) -> u8 {
        self.code
    }
}

/// Build the app. Call [`App::run`] on the result.
pub fn build_app(sim: Box<dyn Simulation>, config: AppConfig) -> App {
    let recording = config.exporter.is_some();
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(window::primary_window(&config.display)),
                primary_cursor_options: Some(CursorOptions::default()),
                // Closing the window goes through the normal quit path so that
                // recorded frames are flushed.
                exit_condition: ExitCondition::DontExit,
                close_when_requested: false,
            })
            .set(LogPlugin {
                level: tracing_level(config.log_level),
                ..default()
            })
            .set(RenderPlugin {
                // A pipeline needed by a recorded frame must not be compiled
                // in the background while the frame is captured.
                synchronous_pipeline_compilation: recording,
                ..default()
            }),
    )
    .add_plugins(DrawListPlugin {
        multisample: config.display.multisample,
    });
    install_frame_loop(
        &mut app,
        sim,
        Recorder::new(config.exporter),
        config.display.screen,
    );
    if recording {
        app.init_resource::<Warmup>().add_systems(
            Update,
            run_warmup.before(run_frame).in_set(DrawListSet::Produce),
        );
    }
    app
}

/// Add the simulation, the per-frame systems and exit handling to `app`.
///
/// Needs the window messages/components (`WindowPlugin`), `Time` and the
/// [`FrameDrawList`]/[`FrameTextures`]/[`GpuTextures`] resources (normally
/// from [`DrawListPlugin`]); it does not need a GPU, so tests can install it
/// into a `MinimalPlugins` app with a manually spawned primary window.
pub fn install_frame_loop(
    app: &mut App,
    sim: Box<dyn Simulation>,
    recorder: Recorder,
    screen: i32,
) {
    app.insert_resource(SimResource::new(sim))
        .insert_resource(recorder)
        .insert_resource(ScreenSetting(screen))
        .init_resource::<ExitState>()
        .init_resource::<PendingRequests>()
        .add_systems(
            Update,
            (
                forward_input,
                handle_close_requests,
                run_frame,
                handle_requests,
            )
                .chain()
                .in_set(DrawListSet::Produce),
        )
        .add_systems(Last, (write_frames, finish_exit).chain());
}

/// Run the app to completion and return the process exit status.
pub fn run(sim: Box<dyn Simulation>, config: AppConfig) -> u8 {
    match build_app(sim, config).run() {
        AppExit::Success => 0,
        AppExit::Error(code) => code.get(),
    }
}

pub fn handle_close_requests(
    mut requests: MessageReader<WindowCloseRequested>,
    mut exit: ResMut<ExitState>,
) {
    if requests.read().count() > 0 {
        exit.quit();
    }
}

/// One iteration of the C++ main loop body: `update(t, dt)`.
#[allow(clippy::too_many_arguments)]
pub fn run_frame(
    time: Res<Time<Real>>,
    sim: Res<SimResource>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut list: ResMut<FrameDrawList>,
    mut textures: ResMut<FrameTextures>,
    gpu: Res<GpuTextures>,
    mut pending: ResMut<PendingRequests>,
    exit: Res<ExitState>,
    recorder: Res<Recorder>,
    warmup: Option<Res<Warmup>>,
) {
    if exit.is_quitting()
        || recorder.in_flight() >= MAX_FRAMES_IN_FLIGHT
        || warmup.is_some_and(|w| !w.is_ready())
    {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let viewport = viewport_of(window);
    if viewport.width == 0 || viewport.height == 0 {
        return;
    }
    let mut sim = sim.lock();
    sim.frame(time.delta_secs(), viewport, &mut list.0);
    textures.collect(&sim.gfx().textures, &gpu);
    pending.0.extend(sim.take_requests());
}

pub fn handle_requests(
    mut commands: Commands,
    mut pending: ResMut<PendingRequests>,
    mut windows: Query<(&mut Window, &mut CursorOptions), With<PrimaryWindow>>,
    mut recorder: ResMut<Recorder>,
    mut exit: ResMut<ExitState>,
    screen: Res<ScreenSetting>,
) {
    let mut job = CaptureJob::default();
    let mut primary = windows.single_mut().ok();
    for request in pending.0.drain(..) {
        match request {
            PlatformRequest::Quit => exit.quit(),
            PlatformRequest::Fatal(message) => exit.fail(&message),
            PlatformRequest::ShowHelpAndExit => exit.help(),
            PlatformRequest::CaptureFrame => {
                if recorder.is_recording() && job.video_frame.is_none() {
                    job.video_frame = Some(recorder.next_frame_index());
                }
            }
            PlatformRequest::Screenshot { path, with_alpha } => {
                job.screenshot = Some((path, with_alpha));
            }
            PlatformRequest::ToggleFullscreen => {
                if let Some((window, _)) = primary.as_mut() {
                    window::toggle_fullscreen(window, screen.0);
                }
            }
            PlatformRequest::ToggleFrameless => {
                if let Some((window, _)) = primary.as_mut() {
                    window::toggle_frameless(window);
                }
            }
            PlatformRequest::SetCursorVisible(visible) => {
                if let Some((_, cursor)) = primary.as_mut() {
                    window::set_cursor_visible(cursor, visible);
                }
            }
            PlatformRequest::SetCursorGrab(grab) => {
                if let Some((_, cursor)) = primary.as_mut() {
                    window::set_cursor_grab(cursor, grab);
                }
            }
            PlatformRequest::WarpCursor(pos) => {
                if let Some((window, _)) = primary.as_mut() {
                    window.set_physical_cursor_position(Some(pos.as_dvec2()));
                }
            }
        }
    }
    let sink = recorder.sink();
    spawn_capture(&mut commands, job, sink);
}

/// Write captured video frames. A failed video stream ends the app (the C++
/// binary is killed by `SIGPIPE` when the reader of `-o -` goes away).
pub fn write_frames(mut recorder: ResMut<Recorder>, mut exit: ResMut<ExitState>) {
    recorder.flush();
    if !exit.is_quitting()
        && let Some(error) = recorder.error()
    {
        exit.fail(&format!("could not write video frames: {error}"));
    }
}

pub fn finish_exit(
    mut exit: ResMut<ExitState>,
    mut recorder: ResMut<Recorder>,
    mut app_exit: MessageWriter<AppExit>,
) {
    if !exit.quitting || exit.done {
        return;
    }
    let deadline = *exit
        .deadline
        .get_or_insert_with(|| Deadline::after(EXIT_FLUSH_TIMEOUT));
    if recorder.in_flight() > 0 && !deadline.passed() {
        return;
    }
    if recorder.in_flight() > 0 {
        error!(
            "gave up waiting for {} video frame(s)",
            recorder.in_flight()
        );
    }
    let finished = recorder.finish();
    if let Some(error) = recorder
        .error()
        .map(str::to_owned)
        .or_else(|| finished.err().map(|e| e.to_string()))
        && exit.code == 0
    {
        exit.fail(&format!("could not write video frames: {error}"));
    }
    exit.done = true;
    print!("{}", exit.stdout);
    eprint!("{}", exit.stderr);
    app_exit.write(if exit.code == 0 {
        AppExit::Success
    } else {
        AppExit::from_code(exit.code)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_levels() {
        assert_eq!(tracing_level(LogLevel::Off), Level::ERROR);
        assert_eq!(tracing_level(LogLevel::Warn), Level::WARN);
        assert_eq!(tracing_level(LogLevel::Info), Level::INFO);
        assert_eq!(tracing_level(LogLevel::Debug), Level::DEBUG);
        assert_eq!(tracing_level(LogLevel::Pedantic), Level::TRACE);
    }

    #[test]
    fn exit_state_keeps_first_failure() {
        let mut exit = ExitState::default();
        assert!(!exit.is_quitting());
        exit.fail("first");
        exit.fail("second");
        assert!(exit.is_quitting());
        assert_eq!(exit.code(), 1);
        assert_eq!(exit.stderr, quit_message("first"));

        let mut exit = ExitState::default();
        exit.help();
        assert_eq!(exit.code(), 0);
        assert_eq!(exit.stdout, help_text(false));

        let mut exit = ExitState::default();
        exit.quit();
        exit.help();
        assert!(exit.stdout.is_empty(), "help after quit is ignored");
        exit.fail("late");
        assert_eq!(exit.code(), 1, "a failure while quitting still fails");
    }

    #[test]
    fn viewport_uses_physical_pixels() {
        let mut window = Window::default();
        window.resolution.set_physical_resolution(1600, 900);
        window.resolution.set_scale_factor(2.0);
        let viewport = viewport_of(&window);
        assert_eq!((viewport.width, viewport.height), (1600, 900));
        assert_eq!(viewport.dpi_ratio, 2.0);
    }
}
