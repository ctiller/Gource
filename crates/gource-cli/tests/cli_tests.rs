use gource_cli::{
    Outcome, handle_command_line, open_exporter, quit_message, resolve_display_dimensions,
    run_headless, save_png,
};
use gource_settings::{DisplaySettings, help::help_text};

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

fn unwrap_exit(outcome: Outcome) -> (String, String, u8) {
    match outcome {
        Outcome::Exit {
            stdout,
            stderr,
            code,
        } => (stdout, stderr, code),
        Outcome::Run(_) => panic!("expected Outcome::Exit, got Outcome::Run"),
    }
}

#[test]
fn help_and_log_command_and_errors() {
    assert_eq!(
        quit_message("bad flag"),
        "gource: bad flag\nTry 'gource --help' for more information.\n\n"
    );

    let (stdout, stderr, code) = unwrap_exit(handle_command_line(&args(&["--help"])));
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert_eq!(stdout, help_text(false));

    let (stdout, _, code) = unwrap_exit(handle_command_line(&args(&["-H"])));
    assert_eq!(code, 0);
    assert_eq!(stdout, help_text(true));

    let (stdout, _, code) = unwrap_exit(handle_command_line(&args(&["--log-command", "svn"])));
    assert_eq!(code, 0);
    assert_eq!(stdout, "svn log -r 1:HEAD --xml --verbose --quiet\n");

    let (_, stderr, code) = unwrap_exit(handle_command_line(&args(&["--log-command", "cvs"])));
    assert_eq!(code, 1);
    assert!(stderr.contains("cvs2cl"));

    let (_, stderr, code) = unwrap_exit(handle_command_line(&args(&["--unknown-flag"])));
    assert_eq!(code, 1);
    assert!(stderr.starts_with("gource: unknown option"));
}

#[test]
fn save_config_and_custom_log_and_stats() {
    let dir = tempfile::tempdir().unwrap();
    let conf_path = dir.path().join("test.conf");
    let (_, _, code) = unwrap_exit(handle_command_line(&args(&[
        "--save-config",
        conf_path.to_str().unwrap(),
        "--seconds-per-day",
        "2",
    ])));
    assert_eq!(code, 0);
    assert!(
        std::fs::read_to_string(&conf_path)
            .unwrap()
            .contains("seconds-per-day=2")
    );

    let (_, _, code) = unwrap_exit(handle_command_line(&args(&[
        "--save-config",
        "/nonexistent_dir_xyz/bad.conf",
    ])));
    assert_eq!(code, 1);

    let log_in = dir.path().join("in.log");
    std::fs::write(
        &log_in,
        "1609459200|alice|A|src/main.rs|50|0\n1609545600|bob|M|src/main.rs|10|5\n1609632000|alice|D|src/main.rs|0|55\n",
    )
    .unwrap();

    let custom_out = dir.path().join("out.log");
    let (_, _, code) = unwrap_exit(handle_command_line(&args(&[
        "--output-custom-log",
        custom_out.to_str().unwrap(),
        log_in.to_str().unwrap(),
    ])));
    assert_eq!(code, 0);
    assert!(
        std::fs::read_to_string(&custom_out)
            .unwrap()
            .contains("alice")
    );

    let stats_out = dir.path().join("stats.csv");
    let (_, _, code) = unwrap_exit(handle_command_line(&args(&[
        "--output-stats",
        stats_out.to_str().unwrap(),
        log_in.to_str().unwrap(),
    ])));
    assert_eq!(code, 0);
    let csv = std::fs::read_to_string(&stats_out).unwrap();
    assert!(csv.starts_with("commit,timestamp,total_files"));

    let (_, _, code) = unwrap_exit(handle_command_line(&args(&[
        "--output-stats",
        "-",
        log_in.to_str().unwrap(),
    ])));
    assert_eq!(code, 0);

    let empty_log = dir.path().join("empty.log");
    std::fs::write(&empty_log, "").unwrap();
    let (_, _, code) = unwrap_exit(handle_command_line(&args(&[
        "--output-stats",
        stats_out.to_str().unwrap(),
        empty_log.to_str().unwrap(),
    ])));
    assert_eq!(code, 1);

    let (_, _, code) = unwrap_exit(handle_command_line(&args(&[
        "--output-stats",
        "/no_such_dir_xyz/stats.csv",
        log_in.to_str().unwrap(),
    ])));
    assert_eq!(code, 0);

    let (_, _, code) = unwrap_exit(handle_command_line(&args(&[
        "--output-custom-log",
        custom_out.to_str().unwrap(),
        empty_log.to_str().unwrap(),
    ])));
    assert_eq!(code, 1);

    let (_, _, code) = unwrap_exit(handle_command_line(&args(&[
        "--output-custom-log",
        "/no_such_dir_xyz/out.log",
        log_in.to_str().unwrap(),
    ])));
    assert_eq!(code, 0);
}

#[test]
fn display_dimensions_png_and_exporter() {
    let mut disp = DisplaySettings {
        display_width: 0,
        display_height: 0,
        video_fps: 0,
        output_framerate: 0,
        ..Default::default()
    };
    assert_eq!(resolve_display_dimensions(&disp), (1024, 768, 60));

    disp.display_width = 320;
    disp.display_height = 240;
    disp.output_framerate = 30;
    assert_eq!(resolve_display_dimensions(&disp), (320, 240, 30));

    disp.video_fps = 24;
    assert_eq!(resolve_display_dimensions(&disp), (320, 240, 24));

    assert!(open_exporter(&disp).unwrap().is_none());

    disp.output_ppm_filename = "/nonexistent_dir_xyz/out.ppm".into();
    assert!(open_exporter(&disp).is_err());

    disp.output_ppm_filename.clear();
    disp.output_video = "/nonexistent_dir_xyz/out.mp4".into();
    disp.video_codec = "x264".into();
    disp.video_bitrate = "2M".into();
    let _ = open_exporter(&disp);

    let dir = tempfile::tempdir().unwrap();
    let rgb_png = dir.path().join("rgb.png");
    let rgba_png = dir.path().join("rgba.png");
    save_png(&rgb_png, 1, 1, &[10, 20, 30, 128], false).unwrap();
    save_png(&rgba_png, 1, 1, &[10, 20, 30, 128], true).unwrap();
    assert!(save_png(&rgb_png, 2, 2, &[0; 4], false).is_err());
}

#[test]
fn headless_renders_ppm_frames_from_custom_log() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("repo.log");
    std::fs::write(
        &log_path,
        "1609459200|alice|A|src/lib.rs\n1609459260|bob|M|src/lib.rs\n",
    )
    .unwrap();
    let ppm_path = dir.path().join("frames.ppm");

    let outcome = handle_command_line(&args(&[
        "-64x32",
        "-o",
        ppm_path.to_str().unwrap(),
        log_path.to_str().unwrap(),
    ]));
    let Outcome::Run(config) = outcome else {
        panic!("expected Outcome::Run");
    };

    let (stdout, stderr, code) = unwrap_exit(run_headless(*config, Some(3)));
    assert_eq!((stdout.as_str(), stderr.as_str(), code), ("", "", 0));

    let ppm_bytes = std::fs::read(&ppm_path).unwrap();
    assert!(
        ppm_bytes.starts_with(b"P6\n64 32 255\n"),
        "expected P6 header in output PPM stream"
    );
    let single_frame_len = b"P6\n64 32 255\n".len() + 64 * 32 * 3;
    assert_eq!(ppm_bytes.len(), single_frame_len * 2);

    // Also test stop-at-end reaching PlatformRequest::Quit without max_frames,
    // and invalid GourceApp startup error in run_headless.
    let ppm_path2 = dir.path().join("frames2.ppm");
    let outcome2 = handle_command_line(&args(&[
        "-64x32",
        "-o",
        ppm_path2.to_str().unwrap(),
        log_path.to_str().unwrap(),
    ]));
    let Outcome::Run(cfg) = outcome2 else {
        panic!("expected Outcome::Run");
    };
    let mut bad_exp_cfg = (*cfg).clone();
    let inputs = [
        (
            2,
            gource_app::InputEvent::KeyDown {
                key: gource_app::Key::F12,
                modifiers: gource_app::Modifiers::default(),
                repeat: false,
            },
        ),
        (
            2,
            gource_app::InputEvent::KeyDown {
                key: gource_app::Key::F11,
                modifiers: gource_app::Modifiers::default(),
                repeat: false,
            },
        ),
        (
            3,
            gource_app::InputEvent::KeyDown {
                key: gource_app::Key::Escape,
                modifiers: gource_app::Modifiers::default(),
                repeat: false,
            },
        ),
        (
            4,
            gource_app::InputEvent::KeyDown {
                key: gource_app::Key::Escape,
                modifiers: gource_app::Modifiers::default(),
                repeat: false,
            },
        ),
    ];
    let (_, _, code) = unwrap_exit(gource_cli::run_headless_with_inputs(
        *cfg,
        Some(10),
        &inputs,
    ));
    let _ = std::fs::remove_file("gource-0001.png");
    assert_eq!(code, 0);

    // Trigger open_exporter error in run_headless (PPM and video)
    bad_exp_cfg.display.output_ppm_filename = "/no_such_dir_123/out.ppm".into();
    let (_, _, exp_code) = unwrap_exit(run_headless(bad_exp_cfg.clone(), Some(1)));
    assert_eq!(exp_code, 1);

    let mut bad_vid_cfg = bad_exp_cfg.clone();
    bad_vid_cfg.display.output_ppm_filename.clear();
    bad_vid_cfg.display.output_video = "/no_such_dir_123/out.mp4".into();
    bad_vid_cfg.display.display_width = 65; // odd width fails VideoExporter::new immediately
    let (_, vid_err, vid_code) = unwrap_exit(run_headless(bad_vid_cfg, Some(1)));
    assert_eq!(vid_code, 1);
    assert!(!vid_err.is_empty());

    // Trigger GourceApp::new error in run_headless via invalid config entry
    let mut bad_app_cfg = bad_exp_cfg;
    bad_app_cfg.display.output_ppm_filename.clear();
    bad_app_cfg
        .conf
        .set_entry("gource", "max-user-speed", "-10");
    let (_, _, app_err_code) = unwrap_exit(run_headless(bad_app_cfg, Some(1)));
    assert_eq!(app_err_code, 1);

    // Trigger Fatal request when log file is empty
    let empty_log = dir.path().join("empty_fatal.log");
    std::fs::write(&empty_log, "").unwrap();
    if let Outcome::Run(empty_cfg) =
        handle_command_line(&args(&["-64x32", empty_log.to_str().unwrap()]))
    {
        let (_, _, fatal_code) = unwrap_exit(run_headless(*empty_cfg, None));
        assert_eq!(fatal_code, 1);
    }

    // Exercise binary entrypoint (`main.rs`) for both Exit and Run paths.
    let bin = env!("CARGO_BIN_EXE_gource-cli");
    let help_out = std::process::Command::new(bin)
        .arg("--help")
        .output()
        .unwrap();
    assert!(help_out.status.success());
    assert!(!help_out.stdout.is_empty());

    let bad_out = std::process::Command::new(bin)
        .arg("--no-such-flag")
        .output()
        .unwrap();
    assert!(!bad_out.status.success());

    let run_out = std::process::Command::new(bin)
        .args(["-64x32", empty_log.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!run_out.status.success());
}
