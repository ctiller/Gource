//! Tests targeting gource.rs coverage: keys, shortcuts, dpi ratio, seek, hover, captions.

use std::time::Duration;

use glam::Vec2;
use gource_app::app::{AppOptions, GourceApp};
use gource_app::gource::Gource;
use gource_app::input::{InputEvent, Key, Modifiers, MouseButton};
use gource_app::platform::{PlatformRequest, Viewport};
use gource_draw::DrawList;
use gource_settings::{CliAction, parse_command_line};

struct TestApp {
    app: GourceApp,
    _dir: tempfile::TempDir,
}

impl TestApp {
    fn new(log: &str, args: &[&str], opts: AppOptions) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("log.txt");
        std::fs::write(&log_path, log).unwrap();

        let mut argv: Vec<String> = vec!["gource".to_string()];
        for a in args {
            argv.push(a.to_string());
        }
        argv.push(log_path.to_str().unwrap().to_string());

        let CliAction::Run(config) = parse_command_line(&argv).expect("parse args") else {
            panic!("expected run");
        };

        let app = GourceApp::new(config, opts).expect("create GourceApp");
        Self { app, _dir: dir }
    }

    fn gource(&self) -> &Gource {
        self.app.shell().gource.as_ref().expect("gource running")
    }

    fn wait_for_load(&mut self) {
        let viewport = Viewport::new(800, 600);
        let mut list = DrawList::new(glam::UVec2::new(800, 600));

        // Step first frame so gource spawns background log loader
        self.app.frame(1.0 / 60.0, viewport, &mut list);
        let _ = self.app.take_requests();

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            self.app.frame(1.0 / 60.0, viewport, &mut list);
            let _ = self.app.take_requests();
            if self
                .gource()
                .logmill
                .as_ref()
                .is_none_or(|m| m.is_finished())
                && self.gource().commitlog.is_some()
            {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "log failed to load");
            std::thread::sleep(Duration::from_millis(2));
        }

        // Run a few frames to populate initial commit
        for _ in 0..5 {
            self.app.frame(1.0 / 60.0, viewport, &mut list);
            let _ = self.app.take_requests();
        }
    }
}

#[test]
fn test_dpi_ratio_font_scaling() {
    let dir = tempfile::tempdir().unwrap();
    let log1 = dir.path().join("log1.txt");
    let log2 = dir.path().join("log2.txt");
    std::fs::write(&log1, "1635724800|Alice|A|file1.txt\n").unwrap();
    std::fs::write(&log2, "1635724800|Bob|A|file2.txt\n").unwrap();

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

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    let viewport = Viewport {
        width: 800,
        height: 600,
        dpi_ratio: 2.0,
    };
    let mut list = DrawList::new(glam::UVec2::new(800, 600));

    // First repo was created inside GourceShell::new with initial_viewport (dpi_ratio 1.0)
    app.frame(0.016, viewport, &mut list);

    // Switch repo with Return: next frame calls get_next(viewport) where viewport.dpi_ratio == 2.0!
    app.input(&InputEvent::KeyDown {
        key: Key::Return,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    app.frame(0.016, viewport, &mut list);

    let g = app.shell().gource.as_ref().unwrap();
    assert_eq!(g.settings.font_scale, 2.0);
}

#[test]
fn test_gource_keys_and_time_speed_controls() {
    let log =
        "1635724800|Alice|A|file1.txt\n1635724900|Bob|A|file2.txt\n1635725000|Carol|A|file3.txt\n";
    let mut test = TestApp::new(log, &["--seconds-per-day", "1.0"], AppOptions::default());
    test.wait_for_load();

    // Verify initial speed
    assert_eq!(test.gource().settings.days_per_second, 1.0);

    // Key '=' increases days_per_second (when >= 1.0)
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('='),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert_eq!(test.gource().settings.days_per_second, 2.0);

    // Key '-' decreases days_per_second (when > 1.0)
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('-'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert_eq!(test.gource().settings.days_per_second, 1.0);

    // Another '-' drops it below 1.0 (to 0.5)
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('-'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert_eq!(test.gource().settings.days_per_second, 0.5);

    // Key '+' increases when < 1.0 (0.5 * 2.0 = 1.0)
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('+'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert_eq!(test.gource().settings.days_per_second, 1.0);

    // Time scale '.' and ','
    assert_eq!(test.gource().settings.time_scale, 1.0);
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('.'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert_eq!(test.gource().settings.time_scale, 2.0);

    test.app.input(&InputEvent::KeyDown {
        key: Key::Char(','),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert_eq!(test.gource().settings.time_scale, 1.0);

    test.app.input(&InputEvent::KeyDown {
        key: Key::Char(','),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert_eq!(test.gource().settings.time_scale, 0.5);

    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('.'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert_eq!(test.gource().settings.time_scale, 1.0);

    // '/' resets time_scale to 1.0
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('/'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert_eq!(test.gource().settings.time_scale, 1.0);

    // Arrow keys: Left, Right, Up, Down
    test.app.input(&InputEvent::KeyDown {
        key: Key::Right,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    test.app.input(&InputEvent::KeyDown {
        key: Key::Left,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    test.app.input(&InputEvent::KeyDown {
        key: Key::Up,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    test.app.input(&InputEvent::KeyDown {
        key: Key::Down,
        modifiers: Modifiers::default(),
        repeat: false,
    });

    // Gravity tuning '[' and ']'
    let g_before = test.gource().world.tuning.force_gravity;
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('['),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert!(test.gource().world.tuning.force_gravity < g_before);
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char(']'),
        modifiers: Modifiers::default(),
        repeat: false,
    });

    // KeypadPlus and KeypadMinus zoom
    test.app.input(&InputEvent::KeyDown {
        key: Key::KeypadPlus,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    test.app.input(&InputEvent::KeyDown {
        key: Key::KeypadMinus,
        modifiers: Modifiers::default(),
        repeat: false,
    });

    // Keys: q, w, y, c, v, s, Space, Tab
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('q'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert!(test.gource().debug);

    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('w'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert!(test.gource().trace_debug);

    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('y'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert!(test.gource().quadtree_debug);

    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('c'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert_eq!(test.gource().splash, 15.0);

    let uid = test.gource().world.users.keys().next();
    test.app.shell_mut().gource.as_mut().unwrap().selected_user = uid;
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('v'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    // Toggle back to overview
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('v'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('s'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert!(test.gource().recolour);

    let grav_before = test.gource().world.tuning.gravity;
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('z'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert_ne!(test.gource().world.tuning.gravity, grav_before);

    // 'm' key toggles mouse cursor visibility
    let mouse_hidden_before = test.gource().settings.hide_mouse;
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('m'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert_ne!(test.gource().settings.hide_mouse, mouse_hidden_before);

    test.app.input(&InputEvent::KeyDown {
        key: Key::Space,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert!(test.gource().paused);

    // Tab key user switching
    test.app.input(&InputEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert!(test.gource().selected_user.is_some());

    // Cycle Tab again to select next user
    test.app.input(&InputEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers::default(),
        repeat: false,
    });

    // Keys u, d, f state toggles
    // 'u': hide_usernames / highlight_all_users cycle
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('u'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('u'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('u'),
        modifiers: Modifiers::default(),
        repeat: false,
    });

    // 'd': hide_dirnames / highlight_dirs cycle
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('d'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('d'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('d'),
        modifiers: Modifiers::default(),
        repeat: false,
    });

    // 'f': hide_filenames / file_extensions cycle
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('f'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('f'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('f'),
        modifiers: Modifiers::default(),
        repeat: false,
    });

    // 'r', 'k', 't', 'g', 'n'
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('r'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('k'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('t'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('g'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    test.app.input(&InputEvent::KeyDown {
        key: Key::Char('n'),
        modifiers: Modifiers::default(),
        repeat: false,
    });

    // Advance frame while paused: covers paused branch in update()
    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(glam::UVec2::new(800, 600));
    test.app.frame(0.016, viewport, &mut list);

    // Unpause
    test.app.input(&InputEvent::KeyDown {
        key: Key::Space,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    // F12 screenshot
    test.app.input(&InputEvent::KeyDown {
        key: Key::F12,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    test.app.frame(0.016, viewport, &mut list);
    let reqs = test.app.take_requests();
    assert!(
        reqs.iter()
            .any(|r| matches!(r, PlatformRequest::Screenshot { .. }))
    );

    // Test select_next_user when world has no users (line 568: if self.world.users.is_empty() { return; })
    test.app
        .shell_mut()
        .gource
        .as_mut()
        .unwrap()
        .world
        .users
        .clear();
    test.app.input(&InputEvent::KeyDown {
        key: Key::Tab,
        modifiers: Modifiers::default(),
        repeat: false,
    });
}

#[test]
fn test_mouse_interactions_drag_rotate_zoom_slider() {
    let log = "1635724800|Alice|A|file1.txt\n1635724900|Bob|A|file2.txt\n";
    let mut test = TestApp::new(log, &[], AppOptions::default());
    test.wait_for_load();

    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(glam::UVec2::new(800, 600));

    // Mouse wheel zoom in / out
    test.app.input(&InputEvent::MouseWheel { delta: 1.0 });
    test.app.input(&InputEvent::MouseWheel { delta: -1.0 });

    // Right mouse button down and drag (manual rotate)
    test.app.input(&InputEvent::MouseButton {
        button: MouseButton::Right,
        pressed: true,
        pos: Vec2::new(400.0, 300.0),
    });
    test.app.input(&InputEvent::MouseMove {
        pos: Vec2::new(420.0, 300.0),
        delta: Vec2::new(20.0, 0.0),
    });
    test.app.input(&InputEvent::MouseMove {
        pos: Vec2::new(420.0, 320.0),
        delta: Vec2::new(0.0, 20.0),
    });
    test.app.frame(0.016, viewport, &mut list);
    test.app.input(&InputEvent::MouseButton {
        button: MouseButton::Right,
        pressed: false,
        pos: Vec2::new(420.0, 320.0),
    });

    // Left mouse drag (manual pan / camera drag)
    test.app.input(&InputEvent::MouseButton {
        button: MouseButton::Left,
        pressed: true,
        pos: Vec2::new(400.0, 300.0),
    });
    test.app.input(&InputEvent::MouseMove {
        pos: Vec2::new(410.0, 310.0),
        delta: Vec2::new(10.0, 10.0),
    });
    test.app.frame(0.016, viewport, &mut list);
    test.app.input(&InputEvent::MouseButton {
        button: MouseButton::Left,
        pressed: false,
        pos: Vec2::new(410.0, 310.0),
    });

    // Middle click test
    test.app.input(&InputEvent::MouseButton {
        button: MouseButton::Middle,
        pressed: true,
        pos: Vec2::new(400.0, 300.0),
    });
    test.app.input(&InputEvent::MouseButton {
        button: MouseButton::Middle,
        pressed: false,
        pos: Vec2::new(400.0, 300.0),
    });

    // Slider hover and click (slider bounds: min.y is 600 - 70 = 530, max.y is 600 - 35 = 565)
    let slider_pos = Vec2::new(400.0, 545.0);
    test.app.input(&InputEvent::MouseMove {
        pos: slider_pos,
        delta: Vec2::ZERO,
    });
    test.app.input(&InputEvent::MouseButton {
        button: MouseButton::Left,
        pressed: true,
        pos: slider_pos,
    });
    test.app.input(&InputEvent::MouseButton {
        button: MouseButton::Left,
        pressed: false,
        pos: slider_pos,
    });
    test.app.frame(0.016, viewport, &mut list);

    // Focus lost when grab_mouse active
    test.app.shell_mut().gource.as_mut().unwrap().grab_mouse = true;
    test.app.input(&InputEvent::Focus(false));
    assert!(!test.gource().grab_mouse);
}

#[test]
fn test_captions_loading_and_display() {
    let dir = tempfile::tempdir().unwrap();
    let caption_file = dir.path().join("captions.txt");
    std::fs::write(
        &caption_file,
        "# Comment line\n1635724800|First release\n1635724850|Second feature\n",
    )
    .unwrap();

    let log = "1635724800|Alice|A|file1.txt\n1635724900|Bob|A|file2.txt\n";
    let mut test = TestApp::new(
        log,
        &[
            "--caption-file",
            caption_file.to_str().unwrap(),
            "--caption-offset",
            "-50",
            "--title",
            "Project Title",
        ],
        AppOptions::default(),
    );
    test.wait_for_load();

    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(glam::UVec2::new(800, 600));

    // Run enough frames to trigger caption processing
    for _ in 0..20 {
        test.app.frame(0.05, viewport, &mut list);
        let _ = test.app.take_requests();
    }
}
