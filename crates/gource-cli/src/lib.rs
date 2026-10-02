//! Bevy-free headless CLI and offscreen runner for Gource.
//!
//! Handles `--help`, `-H`, `--log-command`, `--save-config`, `--output-custom-log`,
//! `--output-stats`, and headless `-o` (`--output-ppm-stream`) / `--output-video`
//! rendering via [`gource_render::WgpuRenderer`].

use std::{io::Write, path::Path};

use gource_app::{AppOptions, GourceApp, PlatformRequest, Viewport};
use gource_draw::{DrawList, PpmExporter, VideoCodec, VideoConfig, VideoExporter, VideoSink};
use gource_render::{OffscreenTarget, WgpuRenderer};
use gource_settings::{CliAction, Config, DisplaySettings, help::help_text, parse_command_line};
use gource_vcs::VcsError;

/// Program name used in `SDLAppQuit`-compatible error messages.
pub const EXEC_NAME: &str = "gource";

/// Format an error message matching `SDLAppQuit(error)`.
pub fn quit_message(error: &str) -> String {
    format!("{EXEC_NAME}: {error}\nTry '{EXEC_NAME} --help' for more information.\n\n")
}

/// Result of command-line handling or headless execution.
#[derive(Debug)]
pub enum Outcome {
    /// Print `stdout` and `stderr`, then exit with `code`.
    Exit {
        stdout: String,
        stderr: String,
        code: u8,
    },
    /// Run the simulation with the parsed [`Config`].
    Run(Box<Config>),
}

impl Outcome {
    pub fn success(stdout: impl Into<String>) -> Self {
        Self::Exit {
            stdout: stdout.into(),
            stderr: String::new(),
            code: 0,
        }
    }

    pub fn quit(error: &str) -> Self {
        Self::Exit {
            stdout: String::new(),
            stderr: quit_message(error),
            code: 1,
        }
    }
}

/// Export repository history statistics as CSV (`--output-stats FILE`).
pub fn write_stats_csv(
    settings: &gource_settings::GourceSettings,
    output: &str,
) -> Result<(), VcsError> {
    let mut options = gource_app::vcs_options(settings);
    options.include_numstat = true;

    let mut commitlog =
        gource_vcs::LogMill::fetch_blocking(&settings.path, &options).map_err(VcsError::Message)?;
    commitlog.wait_for_input(true);

    let mut builder = gource_history::HistoryBuilder::new(
        gource_history::CohortMode::Year,
        gource_history::ChurnDecayModel::LifoYoungestFirst,
    );

    while let Some(commit) = commitlog.next_commit() {
        let files = commit
            .files
            .into_iter()
            .map(|cf| {
                let op = match cf.action {
                    gource_vcs::FileAction::Add => gource_history::ChangeOp::Add,
                    gource_vcs::FileAction::Delete => gource_history::ChangeOp::Delete,
                    _ => gource_history::ChangeOp::Modify,
                };
                let lines_added =
                    cf.lines_added
                        .unwrap_or(if cf.action == gource_vcs::FileAction::Delete {
                            0
                        } else {
                            10
                        });
                let lines_removed = cf.lines_removed.unwrap_or(0);
                gource_history::FileChangeInput {
                    path: cf.filename,
                    op,
                    lines_added,
                    lines_removed,
                    byte_size: None,
                    is_binary: cf.is_binary,
                }
            })
            .collect();

        builder.add_commit(gource_history::CommitInput {
            timestamp: commit.timestamp,
            username: commit.username,
            files,
        });
    }

    let history = builder.finish();
    let csv = history.export_csv();

    if output == "-" {
        let mut stdout = std::io::stdout();
        stdout.write_all(csv.as_bytes())?;
        stdout.flush()?;
    } else {
        let mut file = std::fs::File::create(output)?;
        file.write_all(csv.as_bytes())?;
        file.flush()?;
    }

    Ok(())
}

/// Handle command-line arguments (`args` excludes argv[0]).
pub fn handle_command_line(args: &[String]) -> Outcome {
    let action = match parse_command_line(args) {
        Ok(action) => action,
        Err(error) => return Outcome::quit(&error.0),
    };

    match action {
        CliAction::Help { extended } => Outcome::success(help_text(extended)),
        CliAction::PrintLogCommand { vcs } => {
            let cmd = gource_vcs::log_command(&vcs).unwrap_or_default();
            Outcome::success(format!("{cmd}\n"))
        }
        CliAction::SaveConfig { path, config } => match config.conf.save(Path::new(&path)) {
            Ok(()) => Outcome::success(""),
            Err(error) => Outcome::quit(&error.0),
        },
        CliAction::OutputCustomLog { output, config } => {
            let options = gource_app::vcs_options(&config.gource);
            match gource_vcs::write_custom_log(&config.gource.path, &output, &options) {
                Ok(()) => Outcome::success(""),
                Err(VcsError::Message(message)) if !message.is_empty() => Outcome::quit(&message),
                Err(_) => Outcome::success(""),
            }
        }
        CliAction::Run(config) => {
            if !config.gource.output_stats_filename.is_empty() {
                match write_stats_csv(&config.gource, &config.gource.output_stats_filename) {
                    Ok(()) => Outcome::success(""),
                    Err(VcsError::Message(msg)) if !msg.is_empty() => Outcome::quit(&msg),
                    Err(_) => Outcome::success(""),
                }
            } else {
                Outcome::Run(Box::new(config))
            }
        }
    }
}

/// Save an RGBA8 buffer to a PNG file (RGBA when `with_alpha` is true, RGB otherwise).
pub fn save_png(
    path: &Path,
    width: u32,
    height: u32,
    rgba: &[u8],
    with_alpha: bool,
) -> image::ImageResult<()> {
    let img = image::RgbaImage::from_raw(width, height, rgba.to_vec()).ok_or_else(|| {
        image::ImageError::Parameter(image::error::ParameterError::from_kind(
            image::error::ParameterErrorKind::DimensionMismatch,
        ))
    })?;
    if with_alpha {
        img.save_with_format(path, image::ImageFormat::Png)
    } else {
        image::DynamicImage::ImageRgba8(img)
            .to_rgb8()
            .save_with_format(path, image::ImageFormat::Png)
    }
}

/// Resolve the target `(width, height, fps)` from [`DisplaySettings`].
pub fn resolve_display_dimensions(display: &DisplaySettings) -> (u32, u32, u32) {
    let width = if display.display_width > 0 {
        display.display_width as u32
    } else {
        1024
    };
    let height = if display.display_height > 0 {
        display.display_height as u32
    } else {
        768
    };
    let fps = if display.video_fps > 0 {
        display.video_fps
    } else if display.output_framerate > 0 {
        display.output_framerate as u32
    } else {
        60
    };
    (width, height, fps)
}

/// Open the configured [`VideoSink`] (`--output-video` or `--output-ppm-stream`), if any.
pub fn open_exporter(display: &DisplaySettings) -> Result<Option<Box<dyn VideoSink>>, String> {
    let (width, height, fps) = resolve_display_dimensions(display);
    if !display.output_video.is_empty() {
        let mut vcfg = VideoConfig::new(&display.output_video, width, height, fps);
        if let Some(codec) = VideoCodec::parse_name(&display.video_codec) {
            vcfg.codec = codec;
        }
        if !display.video_bitrate.is_empty() {
            vcfg.bitrate = Some(display.video_bitrate.clone());
        }
        VideoExporter::new(vcfg)
            .map(|exp| Some(Box::new(exp) as Box<dyn VideoSink>))
            .map_err(|e| {
                format!(
                    "could not initialize video export to '{}': {e}",
                    display.output_video
                )
            })
    } else if !display.output_ppm_filename.is_empty() {
        PpmExporter::new(&display.output_ppm_filename)
            .map(|exp| Some(Box::new(exp) as Box<dyn VideoSink>))
            .map_err(|_| format!("could not write to '{}'", display.output_ppm_filename))
    } else {
        Ok(None)
    }
}

/// Run a headless simulation using [`WgpuRenderer`], exporting frames to the configured
/// [`VideoSink`] until the simulation requests exit or `max_frames` is reached.
pub fn run_headless(config: Config, max_frames: Option<u64>) -> Outcome {
    run_headless_with_inputs(config, max_frames, &[])
}

/// Run a headless simulation with optional scripted `(frame_index, InputEvent)` pairs.
pub fn run_headless_with_inputs(
    config: Config,
    max_frames: Option<u64>,
    inputs: &[(u64, gource_app::InputEvent)],
) -> Outcome {
    let display = config.display.clone();
    let (width, height, fps) = resolve_display_dimensions(&display);

    let mut exporter = match open_exporter(&display) {
        Ok(exp) => exp,
        Err(msg) => return Outcome::quit(&msg),
    };

    let options = AppOptions {
        recording: exporter.is_some(),
        ..Default::default()
    };
    let mut app = match GourceApp::new(config, options) {
        Ok(app) => app,
        Err(err) => return Outcome::quit(&err.0),
    };

    let mut renderer = match WgpuRenderer::new_headless(display.multisample) {
        Ok(r) => r,
        Err(err) => return Outcome::quit(&err.to_string()),
    };
    let mut target =
        OffscreenTarget::new(renderer.device(), width, height, renderer.sample_count());

    let viewport = Viewport {
        width,
        height,
        dpi_ratio: 1.0,
    };
    let dt = 1.0 / (fps.max(1) as f32);
    let mut list = DrawList::default();
    let mut frame_count = 0u64;

    let final_outcome = loop {
        if let Some(limit) = max_frames
            && frame_count >= limit
        {
            break Outcome::success("");
        }

        for (target_frame, ev) in inputs {
            if *target_frame == frame_count {
                app.input(ev);
            }
        }

        app.frame(dt, viewport, &mut list);
        frame_count += 1;

        let requests = app.take_requests();
        let mut need_record = false;
        let mut screenshot_req: Option<(std::path::PathBuf, bool)> = None;
        let mut outcome: Option<Outcome> = None;

        for req in requests {
            match req {
                PlatformRequest::CaptureFrame => need_record = true,
                PlatformRequest::Screenshot { path, with_alpha } => {
                    screenshot_req = Some((path, with_alpha));
                }
                PlatformRequest::Quit => {
                    if outcome.is_none() {
                        outcome = Some(Outcome::success(""));
                    }
                }
                PlatformRequest::Fatal(msg) => {
                    outcome = Some(Outcome::quit(&msg));
                }
                PlatformRequest::ShowHelpAndExit => {
                    if outcome.is_none() {
                        outcome = Some(Outcome::success(help_text(false)));
                    }
                }
                PlatformRequest::SetCursorVisible(_)
                | PlatformRequest::SetCursorGrab(_)
                | PlatformRequest::WarpCursor(_)
                | PlatformRequest::ToggleFullscreen
                | PlatformRequest::ToggleFrameless => {}
            }
        }

        if (need_record && exporter.is_some()) || screenshot_req.is_some() {
            let rgba = match renderer.render_offscreen(&list, &app.gfx().textures, &mut target) {
                Ok(rgba) => rgba,
                Err(err) => return Outcome::quit(&err.to_string()),
            };
            if let Some((path, with_alpha)) = screenshot_req
                && let Err(e) = save_png(&path, width, height, &rgba, with_alpha)
            {
                return Outcome::quit(&format!(
                    "could not write screenshot {}: {e}",
                    path.display()
                ));
            }
            if need_record
                && let Some(exp) = exporter.as_mut()
                && let Err(e) = exp.write_frame(width, height, &rgba)
            {
                return Outcome::quit(&e.to_string());
            }
        }

        if let Some(out) = outcome {
            break out;
        }
    };

    if let Some(exp) = exporter.take()
        && let Err(e) = exp.finish()
    {
        return Outcome::quit(&e.to_string());
    }
    final_outcome
}
