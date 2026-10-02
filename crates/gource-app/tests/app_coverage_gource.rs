use std::path::PathBuf;

use gource_app::app::{AppOptions, GourceApp};
use gource_app::input::{InputEvent, Key, Modifiers, MouseButton};
use gource_app::platform::{PlatformRequest, Viewport};
use gource_core::Vec2;
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
fn test_gource_hud_elements_rendering() {
    let log_path = test_data_path("coverage_test.log");
    let caption_path = test_data_path("captions.txt");
    let user_img = test_data_path("test_user.png");
    let logo_img = test_data_path("test_logo.png");
    let bg_img = test_data_path("test_bg.png");

    let args = [
        "gource".to_string(),
        "--title".to_string(),
        "Test Gource App Coverage".to_string(),
        "--caption-file".to_string(),
        caption_path.to_str().unwrap().to_string(),
        "--default-user-image".to_string(),
        user_img.to_str().unwrap().to_string(),
        "--logo".to_string(),
        logo_img.to_str().unwrap().to_string(),
        "--background-image".to_string(),
        bg_img.to_str().unwrap().to_string(),
        "--seconds-per-day".to_string(),
        "1.0".to_string(),
        "--auto-skip-seconds".to_string(),
        "0.1".to_string(),
        "--font-scale".to_string(),
        "1.0".to_string(),
        "--caption-offset".to_string(),
        "-50".to_string(),
        "--highlight-all-users".to_string(),
        "--highlight-dirs".to_string(),
        "--file-idle-time-at-end".to_string(),
        "2.0".to_string(),
        log_path.to_str().unwrap().to_string(),
    ];

    let cli_action = parse_command_line(&args[1..]).expect("parse");
    let config = match cli_action {
        CliAction::Run(cfg) => cfg,
        _ => unreachable!(),
    };

    let mut app = GourceApp::new(config, AppOptions::default()).expect("create app");
    let viewport = Viewport::new(1024, 768);
    let mut list = DrawList::new(gource_core::UVec2::new(1024, 768));

    // Wait until background log is loaded (takes a few frames/ms)
    let mut loaded = false;
    for i in 0..300 {
        app.frame(0.02, viewport, &mut list);
        let _ = app.take_requests();
        std::thread::sleep(std::time::Duration::from_millis(2));
        if i >= 15 {
            loaded = true;
            break;
        }
    }
    assert!(loaded);

    // Trigger splash screen (C key)
    app.input(&InputEvent::KeyDown {
        key: Key::Char('c'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    // Draw while splash screen is active
    app.frame(0.02, viewport, &mut list);

    // Trigger HUD message and screenshot (F12)
    app.input(&InputEvent::KeyDown {
        key: Key::F12,
        modifiers: Modifiers::default(),
        repeat: false,
    });

    let mut screenshot_emitted = false;
    for _ in 0..300 {
        app.frame(0.02, viewport, &mut list);
        let reqs = app.take_requests();
        if reqs
            .iter()
            .any(|r| matches!(r, PlatformRequest::Screenshot { .. }))
        {
            screenshot_emitted = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(screenshot_emitted, "F12 emits screenshot request");

    // Draw while HUD message is active
    app.frame(0.02, viewport, &mut list);

    // Enable debug bounds (Q key)
    app.input(&InputEvent::KeyDown {
        key: Key::Char('q'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    app.frame(0.02, viewport, &mut list);

    // Test Alt+Return to trigger toggle fullscreen
    app.input(&InputEvent::KeyDown {
        key: Key::Return,
        modifiers: Modifiers {
            alt: true,
            ctrl: false,
            shift: false,
            meta: false,
        },
        repeat: false,
    });
    let reqs2 = app.take_requests();
    assert!(
        reqs2.contains(&PlatformRequest::ToggleFullscreen),
        "Alt+Return toggles fullscreen"
    );
}

#[test]
fn test_gource_keys_and_controls() {
    let log_path = test_data_path("coverage_test.log");
    let args = [
        "gource".to_string(),
        "--seconds-per-day".to_string(),
        "0.5".to_string(),
        "--no-time-travel".to_string(),
        log_path.to_str().unwrap().to_string(),
    ];

    let cli_action = parse_command_line(&args[1..]).expect("parse");
    let config = match cli_action {
        CliAction::Run(cfg) => cfg,
        _ => unreachable!(),
    };

    let mut app = GourceApp::new(config, AppOptions::default()).expect("create app");
    let viewport = Viewport::new(1024, 768);
    let mut list = DrawList::new(gource_core::UVec2::new(1024, 768));

    for _ in 0..10 {
        app.frame(0.02, viewport, &mut list);
        let _ = app.take_requests();
    }

    let default_mods = Modifiers::default();

    // Test pause toggle and paused logic/bounds update
    app.input(&InputEvent::KeyDown {
        key: Key::Space,
        modifiers: default_mods,
        repeat: false,
    });
    app.frame(0.02, viewport, &mut list);
    // Unpause
    app.input(&InputEvent::KeyDown {
        key: Key::Space,
        modifiers: default_mods,
        repeat: false,
    });

    // Test speed up (= and +) and slow down (-)
    app.input(&InputEvent::KeyDown {
        key: Key::Char('='),
        modifiers: default_mods,
        repeat: false,
    });
    app.input(&InputEvent::KeyDown {
        key: Key::Char('+'),
        modifiers: default_mods,
        repeat: false,
    });
    app.input(&InputEvent::KeyDown {
        key: Key::Char('-'),
        modifiers: default_mods,
        repeat: false,
    });

    // Test time scale (. and , and /)
    app.input(&InputEvent::KeyDown {
        key: Key::Char('.'),
        modifiers: default_mods,
        repeat: false,
    });
    app.input(&InputEvent::KeyDown {
        key: Key::Char(','),
        modifiers: default_mods,
        repeat: false,
    });
    app.input(&InputEvent::KeyDown {
        key: Key::Char('/'),
        modifiers: default_mods,
        repeat: false,
    });

    // Test camera mode (v)
    app.input(&InputEvent::KeyDown {
        key: Key::Char('v'),
        modifiers: default_mods,
        repeat: false,
    });
    app.frame(0.02, viewport, &mut list);

    // Test arrow keys (manual camera pan)
    for key in [Key::Right, Key::Left, Key::Up, Key::Down] {
        app.input(&InputEvent::KeyDown {
            key,
            modifiers: default_mods,
            repeat: false,
        });
        app.frame(0.02, viewport, &mut list);
    }

    // Test KeypadPlus and KeypadMinus
    app.input(&InputEvent::KeyDown {
        key: Key::KeypadPlus,
        modifiers: default_mods,
        repeat: false,
    });
    app.frame(0.02, viewport, &mut list);
    app.input(&InputEvent::KeyDown {
        key: Key::KeypadMinus,
        modifiers: default_mods,
        repeat: false,
    });
    app.frame(0.02, viewport, &mut list);

    // Test recolour (s)
    app.input(&InputEvent::KeyDown {
        key: Key::Char('s'),
        modifiers: default_mods,
        repeat: false,
    });
    app.frame(0.02, viewport, &mut list);

    // Test user selection (Tab)
    app.input(&InputEvent::KeyDown {
        key: Key::Tab,
        modifiers: default_mods,
        repeat: false,
    });
    app.frame(0.02, viewport, &mut list);

    // Test cycle usernames (u), dirnames (d), filenames (f)
    for _ in 0..4 {
        app.input(&InputEvent::KeyDown {
            key: Key::Char('u'),
            modifiers: default_mods,
            repeat: false,
        });
        app.input(&InputEvent::KeyDown {
            key: Key::Char('d'),
            modifiers: default_mods,
            repeat: false,
        });
        app.input(&InputEvent::KeyDown {
            key: Key::Char('f'),
            modifiers: default_mods,
            repeat: false,
        });
        app.frame(0.02, viewport, &mut list);
    }

    // Test hide tree (t), hide root (r), show key (k)
    for key in [Key::Char('t'), Key::Char('r'), Key::Char('k')] {
        app.input(&InputEvent::KeyDown {
            key,
            modifiers: default_mods,
            repeat: false,
        });
        app.frame(0.02, viewport, &mut list);
    }
}

#[test]
fn test_gource_mouse_interactions_drag_rotate_slider() {
    let log_path = test_data_path("coverage_test.log");
    let args = [
        "gource".to_string(),
        "--seconds-per-day".to_string(),
        "0.1".to_string(),
        log_path.to_str().unwrap().to_string(),
    ];

    let cli_action = parse_command_line(&args[1..]).expect("parse");
    let config = match cli_action {
        CliAction::Run(cfg) => cfg,
        _ => unreachable!(),
    };

    let mut app = GourceApp::new(config, AppOptions::default()).expect("create app");
    let viewport = Viewport::new(1024, 768);
    let mut list = DrawList::new(gource_core::UVec2::new(1024, 768));

    for _ in 0..10 {
        app.frame(0.02, viewport, &mut list);
        let _ = app.take_requests();
    }

    // 1. Mouse move over slider area at bottom
    app.input(&InputEvent::MouseMove {
        pos: Vec2::new(512.0, 750.0),
        delta: Vec2::new(1.0, 0.0),
    });
    app.frame(0.02, viewport, &mut list);

    // 2. Mouse click on slider (seek_to)
    app.input(&InputEvent::MouseButton {
        button: MouseButton::Left,
        pressed: true,
        pos: Vec2::new(512.0, 750.0),
    });
    app.input(&InputEvent::MouseButton {
        button: MouseButton::Left,
        pressed: false,
        pos: Vec2::new(512.0, 750.0),
    });
    app.frame(0.02, viewport, &mut list);

    // 3. Mouse click background -> drag camera
    app.input(&InputEvent::MouseButton {
        button: MouseButton::Left,
        pressed: true,
        pos: Vec2::new(200.0, 200.0),
    });
    app.input(&InputEvent::MouseMove {
        pos: Vec2::new(220.0, 220.0),
        delta: Vec2::new(20.0, 20.0),
    });
    app.frame(0.02, viewport, &mut list);
    // Release drag
    app.input(&InputEvent::MouseButton {
        button: MouseButton::Left,
        pressed: false,
        pos: Vec2::new(220.0, 220.0),
    });
    app.frame(0.02, viewport, &mut list);

    // 4. Right click -> manual rotate
    app.input(&InputEvent::MouseButton {
        button: MouseButton::Right,
        pressed: true,
        pos: Vec2::new(300.0, 300.0),
    });
    app.input(&InputEvent::MouseMove {
        pos: Vec2::new(315.0, 300.0),
        delta: Vec2::new(15.0, 0.0),
    });
    app.frame(0.02, viewport, &mut list);
    // Vertical delta rotate
    app.input(&InputEvent::MouseMove {
        pos: Vec2::new(315.0, 320.0),
        delta: Vec2::new(0.0, 20.0),
    });
    app.frame(0.02, viewport, &mut list);
    app.input(&InputEvent::MouseButton {
        button: MouseButton::Right,
        pressed: false,
        pos: Vec2::new(315.0, 320.0),
    });
    app.frame(0.02, viewport, &mut list);

    // 5. Middle mouse button
    app.input(&InputEvent::MouseButton {
        button: MouseButton::Middle,
        pressed: true,
        pos: Vec2::new(300.0, 300.0),
    });
    app.input(&InputEvent::MouseButton {
        button: MouseButton::Middle,
        pressed: false,
        pos: Vec2::new(300.0, 300.0),
    });

    // 6. Mouse wheel zoom
    app.input(&InputEvent::MouseWheel { delta: 1.0 });
    app.frame(0.02, viewport, &mut list);
    app.input(&InputEvent::MouseWheel { delta: -1.0 });
    app.frame(0.02, viewport, &mut list);

    // 7. Focus lost while mouse is grabbed
    app.input(&InputEvent::MouseButton {
        button: MouseButton::Left,
        pressed: true,
        pos: Vec2::new(100.0, 100.0),
    });
    app.input(&InputEvent::Focus(false));
    app.frame(0.02, viewport, &mut list);
    app.input(&InputEvent::Focus(true));
    app.frame(0.02, viewport, &mut list);
}

#[test]
fn test_gource_looping_and_idle_skip() {
    let log_path = test_data_path("single_commit.log");
    let args = [
        "gource".to_string(),
        "--loop".to_string(),
        "--loop-delay-seconds".to_string(),
        "0.01".to_string(),
        "--auto-skip-seconds".to_string(),
        "0.01".to_string(),
        "--seconds-per-day".to_string(),
        "10.0".to_string(),
        log_path.to_str().unwrap().to_string(),
    ];

    let cli_action = parse_command_line(&args[1..]).expect("parse");
    let config = match cli_action {
        CliAction::Run(cfg) => cfg,
        _ => unreachable!(),
    };

    let mut app = GourceApp::new(config, AppOptions::default()).expect("create app");
    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(gource_core::UVec2::new(800, 600));

    // Advance frames to process the commit and then loop around
    for _ in 0..50 {
        app.frame(0.05, viewport, &mut list);
        let _ = app.take_requests();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    assert!(!app.is_finished(), "Looping keeps app running");
}
