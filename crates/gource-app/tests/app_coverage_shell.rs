use std::path::PathBuf;

use gource_app::app::{AppError, AppOptions, GourceApp};
use gource_app::input::{InputEvent, Key, Modifiers};
use gource_app::platform::{PlatformRequest, Viewport};
use gource_draw::DrawList;
use gource_settings::{CliAction, parse_command_line};

fn test_data_path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("app_coverage")
        .join(file)
}

#[test]
fn test_app_error_display() {
    let err = AppError("custom error message".to_string());
    assert_eq!(format!("{err}"), "custom error message");
    let dyn_err: &dyn std::error::Error = &err;
    assert_eq!(dyn_err.to_string(), "custom error message");
}

#[test]
fn test_shell_failed_to_get_next_emits_fatal_and_quit() {
    // Write an invalid empty font file to trigger font loading failure inside Gource::new()
    let bad_font = std::env::temp_dir().join("gource_bad_font.ttf");
    let _ = std::fs::write(&bad_font, b"not a valid font header");

    let log_path = test_data_path("coverage_test.log");
    let args = [
        "gource".to_string(),
        "--font-file".to_string(),
        bad_font.to_str().unwrap().to_string(),
        log_path.to_str().unwrap().to_string(),
    ];

    let cli_action = parse_command_line(&args[1..]).expect("parse");
    let config = match cli_action {
        CliAction::Run(cfg) => cfg,
        _ => unreachable!(),
    };

    // When Gource::new fails inside GourceShell::new, GourceApp::new returns Err(AppError)
    let app_res = GourceApp::new(config.clone(), AppOptions::default());
    assert!(app_res.is_err(), "App creation fails when font is invalid");

    let _ = std::fs::remove_file(bad_font);
}

#[test]
fn test_shell_f11_toggle_frameless_and_delay() {
    let log_path = test_data_path("coverage_test.log");
    let args = ["gource".to_string(), log_path.to_str().unwrap().to_string()];

    let cli_action = parse_command_line(&args[1..]).expect("parse");
    let config = match cli_action {
        CliAction::Run(cfg) => cfg,
        _ => unreachable!(),
    };

    let mut app = GourceApp::new(config, AppOptions::default()).expect("create app");
    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(gource_core::UVec2::new(800, 600));

    app.frame(0.016, viewport, &mut list);

    // Send F11 key
    app.input(&InputEvent::KeyDown {
        key: Key::F11,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    let reqs = app.take_requests();
    assert!(
        reqs.contains(&PlatformRequest::ToggleFrameless),
        "F11 toggles frameless"
    );

    // Immediate second F11 should be ignored due to toggle_delay
    app.input(&InputEvent::KeyDown {
        key: Key::F11,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    let reqs2 = app.take_requests();
    assert!(
        !reqs2.contains(&PlatformRequest::ToggleFrameless),
        "F11 toggle delay prevents rapid re-triggering"
    );

    // Advance 0.3s to pass the 0.25s delay
    app.frame(0.3, viewport, &mut list);

    // Now F11 should work again
    app.input(&InputEvent::KeyDown {
        key: Key::F11,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    let reqs3 = app.take_requests();
    assert!(
        reqs3.contains(&PlatformRequest::ToggleFrameless),
        "F11 toggles frameless after delay expires"
    );
}

#[test]
fn test_shell_multi_repo_loop_cycle_and_stop_time() {
    let log1 = test_data_path("coverage_test.log");
    let log2 = test_data_path("coverage_test.log");

    // Two repos in config with stop_at_time configured
    let conf_content = format!(
        "[display]\nviewport=800x600\n\n[gource]\npath={}\nstop-at-time=0.01\n\n[gource]\npath={}\nstop-at-time=0.01\n",
        log1.display(),
        log2.display()
    );

    let conf = gource_settings::ConfFile::parse(&conf_content, "multi.conf").expect("parse conf");
    let display = gource_settings::DisplaySettings::import(&conf).expect("import display");
    let gource =
        gource_settings::GourceSettings::import(&conf, conf.sections_named("gource").next())
            .expect("import gource");

    let config = gource_settings::Config {
        conf,
        display,
        gource,
    };

    let mut app = GourceApp::new(config, AppOptions::default()).expect("create app");
    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(gource_core::UVec2::new(800, 600));

    // Run first repo until it stops (0.01s)
    for _ in 0..10 {
        app.frame(0.01, viewport, &mut list);
        let _ = app.take_requests();
    }

    // Switch repo with Return
    app.input(&InputEvent::KeyDown {
        key: Key::Return,
        modifiers: Modifiers::default(),
        repeat: false,
    });

    // Advance to switch to repo 2
    for _ in 0..10 {
        app.frame(0.01, viewport, &mut list);
        let _ = app.take_requests();
    }

    // Switch again: repo 2 -> repo 0 (wraps around because repo_count > 1 and !recording)
    app.input(&InputEvent::KeyDown {
        key: Key::Return,
        modifiers: Modifiers::default(),
        repeat: false,
    });

    for _ in 0..10 {
        app.frame(0.01, viewport, &mut list);
        let _ = app.take_requests();
    }

    assert!(!app.is_finished(), "Multi-repo loops when not recording");
}

#[test]
fn test_shell_recording_multi_repo_stops_at_end() {
    let log1 = test_data_path("single_commit.log");
    let log2 = test_data_path("single_commit.log");

    let conf_content = format!(
        "[display]\nviewport=800x600\n\n[gource]\npath={}\nstop-at-time=0.01\nseconds-per-day=10.0\nuser-idle-time=0.01\n\n[gource]\npath={}\nstop-at-time=0.01\nseconds-per-day=10.0\nuser-idle-time=0.01\n",
        log1.display(),
        log2.display()
    );

    let conf =
        gource_settings::ConfFile::parse(&conf_content, "multi_rec.conf").expect("parse conf");
    let display = gource_settings::DisplaySettings::import(&conf).expect("import display");
    let gource =
        gource_settings::GourceSettings::import(&conf, conf.sections_named("gource").next())
            .expect("import gource");

    let config = gource_settings::Config {
        conf,
        display,
        gource,
    };

    // When recording is true, multi repo should return Ok(None) after the last repo
    let opts = AppOptions {
        recording: true,
        ..Default::default()
    };
    let mut app = GourceApp::new(config, opts).expect("create app");
    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(gource_core::UVec2::new(800, 600));

    // Run until finish or 1000 frames
    let mut frames = 0;
    while !app.is_finished() && frames < 1000 {
        app.frame(0.02, viewport, &mut list);
        let _ = app.take_requests();
        frames += 1;
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    assert!(app.is_finished(), "Recording terminates after last repo");
}
