use std::path::PathBuf;

use gource_app::app::{AppOptions, GourceApp, vcs_options};
use gource_app::input::{InputEvent, Key, Modifiers, MouseButton};
use gource_app::platform::{PlatformRequest, Viewport};
use gource_core::Vec2;
use gource_draw::DrawList;
use gource_settings::{
    CliAction, ConfFile, Config, DisplaySettings, GourceSettings, parse_command_line,
};

fn test_data_path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("app")
        .join(file)
}

#[test]
fn test_app_custom_log_full_lifecycle() {
    let log_path = test_data_path("custom.log");
    let args = [
        "gource".to_string(),
        "--seconds-per-day".to_string(),
        "0.1".to_string(),
        "--auto-skip-seconds".to_string(),
        "0.1".to_string(),
        "--stop-at-end".to_string(),
        "--file-idle-time".to_string(),
        "0.1".to_string(),
        log_path.to_str().unwrap().to_string(),
    ];

    let cli_action = parse_command_line(&args[1..]).expect("failed to parse command line");
    let config = match cli_action {
        CliAction::Run(cfg) => cfg,
        other => panic!("expected CliAction::Run, got {:?}", other),
    };

    let opts = AppOptions::default();
    let mut app = GourceApp::new(config, opts).expect("failed to create GourceApp");

    assert!(!app.is_finished());
    let _ = app.gfx();

    let viewport = Viewport::new(1024, 768);
    let mut list = DrawList::new(gource_core::UVec2::new(1024, 768));

    // Run simulation loop until finish or max frames
    let mut frames = 0;
    while !app.is_finished() && frames < 500 {
        app.frame(0.016, viewport, &mut list);
        let _reqs = app.take_requests();
        frames += 1;
    }

    assert!(frames > 10, "Simulated at least 10 frames");
}

#[test]
fn test_app_input_events_and_controls() {
    let log_path = test_data_path("gource-custom.log");
    let args = [
        "gource".to_string(),
        "--seconds-per-day".to_string(),
        "1.0".to_string(),
        "--auto-skip-seconds".to_string(),
        "0.5".to_string(),
        log_path.to_str().unwrap().to_string(),
    ];

    let cli_action = parse_command_line(&args[1..]).expect("parse command line");
    let config = match cli_action {
        CliAction::Run(cfg) => cfg,
        _ => unreachable!(),
    };

    let mut app = GourceApp::new(config, AppOptions::default()).expect("new GourceApp");
    let viewport = Viewport::new(1280, 720);
    let mut list = DrawList::new(gource_core::UVec2::new(1280, 720));

    // Advance 5 frames to load initial log
    for _ in 0..5 {
        app.frame(0.016, viewport, &mut list);
    }

    let default_mods = Modifiers::default();

    // Test hotkeys
    let test_keys = [
        Key::Space, // pause/unpause
        Key::Space,
        Key::Char('q'), // debug toggle
        Key::Char('w'), // trace debug
        Key::Char('y'), // quadtree debug
        Key::Char('t'), // hide tree
        Key::Char('g'), // hide users
        Key::Char('u'), // usernames cycle 1
        Key::Char('u'), // usernames cycle 2
        Key::Char('u'), // usernames cycle 3
        Key::Char('d'), // dirnames cycle 1
        Key::Char('d'), // dirnames cycle 2
        Key::Char('d'), // dirnames cycle 3
        Key::Char('f'), // filenames cycle 1
        Key::Char('f'), // filenames cycle 2
        Key::Char('f'), // filenames cycle 3
        Key::Char('r'), // hide root
        Key::Char('k'), // show key
        Key::Char('c'), // splash
        Key::Char('v'), // camera mode toggle
        Key::Char('s'), // recolour
        Key::Tab,       // select next user
        Key::Char('='), // days per second increase
        Key::Char('+'),
        Key::Char('-'), // days per second decrease
        Key::Char('['), // force gravity
        Key::Char(']'),
        Key::Char('.'), // time scale
        Key::Char(','),
        Key::Char('/'),
        Key::KeypadPlus, // zoom
        Key::KeypadMinus,
        Key::Right, // arrow pan
        Key::Left,
        Key::Up,
        Key::Down,
        Key::Char('m'), // mouse toggle
        Key::Char('n'), // skip idle
        Key::F5,        // reset
        Key::F11,       // toggle frameless
        Key::F12,       // screenshot
    ];

    for key in test_keys {
        app.input(&InputEvent::KeyDown {
            key,
            modifiers: default_mods,
            repeat: false,
        });
        app.input(&InputEvent::KeyUp {
            key,
            modifiers: default_mods,
        });
        app.frame(0.016, viewport, &mut list);
    }

    // Test Alt+Enter fullscreen toggle
    let alt_mods = Modifiers {
        alt: true,
        ctrl: false,
        shift: false,
        meta: false,
    };
    app.input(&InputEvent::KeyDown {
        key: Key::Return,
        modifiers: alt_mods,
        repeat: false,
    });
    let reqs = app.take_requests();
    assert!(reqs.contains(&PlatformRequest::ToggleFullscreen));

    // Mouse movement and hover
    app.input(&InputEvent::MouseMove {
        pos: Vec2::new(640.0, 360.0),
        delta: Vec2::new(1.0, 1.0),
    });
    app.frame(0.016, viewport, &mut list);

    // Mouse wheel zoom
    app.input(&InputEvent::MouseWheel { delta: 1.0 });
    app.input(&InputEvent::MouseWheel { delta: -1.0 });
    app.frame(0.016, viewport, &mut list);

    // Mouse clicks (Left, Right, Middle)
    app.input(&InputEvent::MouseButton {
        button: MouseButton::Left,
        pressed: true,
        pos: Vec2::new(640.0, 360.0),
    });
    app.input(&InputEvent::MouseMove {
        pos: Vec2::new(650.0, 370.0),
        delta: Vec2::new(10.0, 10.0),
    });
    app.input(&InputEvent::MouseButton {
        button: MouseButton::Left,
        pressed: false,
        pos: Vec2::new(650.0, 370.0),
    });

    app.input(&InputEvent::MouseButton {
        button: MouseButton::Right,
        pressed: true,
        pos: Vec2::new(650.0, 370.0),
    });
    app.input(&InputEvent::MouseMove {
        pos: Vec2::new(660.0, 370.0),
        delta: Vec2::new(10.0, 0.0),
    });
    app.input(&InputEvent::MouseButton {
        button: MouseButton::Right,
        pressed: false,
        pos: Vec2::new(660.0, 370.0),
    });

    // Slider click (progress slider at bottom of screen)
    app.input(&InputEvent::MouseButton {
        button: MouseButton::Left,
        pressed: true,
        pos: Vec2::new(640.0, 700.0),
    });
    app.input(&InputEvent::MouseButton {
        button: MouseButton::Left,
        pressed: false,
        pos: Vec2::new(640.0, 700.0),
    });

    // Window focus
    app.input(&InputEvent::Focus(false));
    app.input(&InputEvent::Focus(true));

    app.frame(0.016, viewport, &mut list);

    // Escape exits
    app.input(&InputEvent::KeyDown {
        key: Key::Escape,
        modifiers: default_mods,
        repeat: false,
    });
    assert!(app.is_finished());
    let reqs = app.take_requests();
    assert!(reqs.contains(&PlatformRequest::Quit));
}

#[test]
fn test_app_multi_repo_sequencing() {
    let log1 = test_data_path("custom.log");
    let log2 = test_data_path("gource-custom.log");

    let conf_content = format!(
        "[display]\nviewport=800x600\n\n[gource]\npath={}\nstop-at-time=0.05\n\n[gource]\npath={}\nstop-at-time=0.05\n",
        log1.display(),
        log2.display()
    );

    let conf = ConfFile::parse(&conf_content, "test.conf").expect("parse conf");
    let display = DisplaySettings::import(&conf).expect("import display");
    let gource =
        GourceSettings::import(&conf, conf.sections_named("gource").next()).expect("import gource");

    let config = Config {
        conf,
        display,
        gource,
    };

    let mut app = GourceApp::new(config, AppOptions::default()).expect("create app");
    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(gource_core::UVec2::new(800, 600));

    // First repo running
    for _ in 0..10 {
        app.frame(0.02, viewport, &mut list);
    }

    // Switch repo with Enter key
    app.input(&InputEvent::KeyDown {
        key: Key::Return,
        modifiers: Modifiers::default(),
        repeat: false,
    });

    // Advance frames to trigger transition fading and second repo
    for _ in 0..15 {
        app.frame(0.02, viewport, &mut list);
    }

    assert!(!app.take_requests().is_empty() || !app.is_finished());
}

#[test]
fn test_app_recording_mode() {
    let log_path = test_data_path("custom.log");
    let args = [
        "gource".to_string(),
        "--stop-at-end".to_string(),
        log_path.to_str().unwrap().to_string(),
    ];

    let cli_action = parse_command_line(&args[1..]).expect("parse");
    let config = match cli_action {
        CliAction::Run(cfg) => cfg,
        _ => unreachable!(),
    };

    let opts = AppOptions {
        recording: true,
        ..Default::default()
    };
    let mut app = GourceApp::new(config, opts).expect("create recording app");
    let viewport = Viewport::new(640, 480);
    let mut list = DrawList::new(gource_core::UVec2::new(640, 480));

    let mut captured = false;
    for frame_i in 0..300 {
        app.frame(0.016, viewport, &mut list);
        let reqs = app.take_requests();
        if reqs
            .iter()
            .any(|r| matches!(r, PlatformRequest::CaptureFrame))
        {
            captured = true;
            break;
        }
        if app.is_finished() {
            println!("App finished early at frame {frame_i}, reqs: {reqs:?}");
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }

    assert!(captured, "CaptureFrame emitted in recording mode");
}

#[test]
fn test_vcs_options_conversion() {
    let conf_text = "[gource]\nlog-format=git\ngit-branch=main\nstart-date=2020-01-01\nstop-date=2020-01-02\nfile-filter=^.*\\.tmp$\n";
    let conf = ConfFile::parse(conf_text, "vcs.conf").expect("parse conf");
    let settings =
        GourceSettings::import(&conf, conf.sections_named("gource").next()).expect("import");

    let vcs = vcs_options(&settings);
    assert_eq!(vcs.log_format, "git");
    assert_eq!(vcs.git_branch, "main");
    assert!(vcs.start_timestamp > 0);
    assert!(vcs.stop_timestamp > 0);
    assert_eq!(vcs.filters.file_filters.len(), 1);
}

/// Frame indices (0-based) at which `CaptureFrame` was requested, stopping
/// after `captures` captures (or 400 frames).
fn capture_frames(framerate: &str, captures: usize) -> Vec<usize> {
    let log_path = test_data_path("custom.log");
    let args = [
        "gource".to_string(),
        "-r".to_string(),
        framerate.to_string(),
        log_path.to_str().unwrap().to_string(),
    ];
    let config = match parse_command_line(&args[1..]).expect("parse") {
        CliAction::Run(cfg) => cfg,
        _ => unreachable!(),
    };
    let mut app = GourceApp::new(
        config,
        AppOptions {
            recording: true,
            ..Default::default()
        },
    )
    .expect("create app");
    let viewport = Viewport::new(640, 480);
    let mut list = DrawList::new(gource_core::UVec2::new(640, 480));

    let mut frames = Vec::new();
    for frame in 0..400 {
        app.frame(0.016, viewport, &mut list);
        let captured = app
            .take_requests()
            .into_iter()
            .filter(|req| *req == PlatformRequest::CaptureFrame)
            .count();
        assert!(captured <= 1, "at most one capture per frame");
        if captured == 1 {
            frames.push(frame);
            if frames.len() == captures {
                break;
            }
        }
        // The log is opened on a background thread and captures only start
        // once it is open (C++: `if(frameExporter != 0 && commitlog ...)`),
        // so the first capture frame depends on timing; the spacing doesn't.
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    frames
}

#[test]
fn test_recording_output_framerates() {
    // C++ exports a frame when `framecount % (frameskip + 1) == 0`, with
    // frameskip 1 for -r 30 (1/60 ticks), 2 for -r 25 (1/75 ticks) and 0 for
    // -r 60.
    for (framerate, spacing) in [("30", 2), ("25", 3), ("60", 1)] {
        let frames = capture_frames(framerate, 10);
        assert_eq!(frames.len(), 10, "-r {framerate}: captures at {frames:?}");
        for pair in frames.windows(2) {
            assert_eq!(
                pair[1] - pair[0],
                spacing,
                "-r {framerate}: captures at {frames:?}"
            );
        }
    }
}

#[test]
fn test_headless_recording_finishes_and_emits_quit_exactly_once() {
    let log_path = test_data_path("custom.log");
    let args = [
        "gource".to_string(),
        "-1280x720".to_string(),
        "--seconds-per-day".to_string(),
        "0.2".to_string(),
        "--auto-skip-seconds".to_string(),
        "0.1".to_string(),
        "--stop-at-time".to_string(),
        "1.0".to_string(),
        "-r".to_string(),
        "30".to_string(),
        log_path.to_str().unwrap().to_string(),
    ];

    let cli_action = parse_command_line(&args[1..]).expect("parse");
    let config = match cli_action {
        CliAction::Run(cfg) => cfg,
        _ => unreachable!(),
    };

    let opts = AppOptions {
        recording: true,
        ..Default::default()
    };
    let mut app = GourceApp::new(config, opts).expect("create app");
    let viewport = Viewport::new(1280, 720);
    let mut list = DrawList::new(gource_core::UVec2::new(1280, 720));

    let mut quit_count = 0;
    let mut frames = 0;
    while frames < 2000 {
        app.frame(0.016, viewport, &mut list);
        let reqs = app.take_requests();
        for req in reqs {
            if req == PlatformRequest::Quit {
                quit_count += 1;
            }
        }
        frames += 1;
        if app.is_finished() {
            // Once finished, call frame a few more times to verify Quit is NOT emitted again
            for _ in 0..5 {
                app.frame(0.016, viewport, &mut list);
                let extra_reqs = app.take_requests();
                for req in extra_reqs {
                    if req == PlatformRequest::Quit {
                        quit_count += 1;
                    }
                }
            }
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    assert!(
        app.is_finished(),
        "App finished when stop_position_reached and users empty"
    );
    assert_eq!(quit_count, 1, "PlatformRequest::Quit emitted exactly once");
}
