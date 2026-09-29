//! Coverage tests for GourceShell (multi-repo, error conditions, keys, recording).

use gource_draw::DrawList;
use gource_settings::{CliAction, parse_command_line};
use gource_sim::app::{AppOptions, GourceApp};
use gource_sim::input::{InputEvent, Key, Modifiers};
use gource_sim::platform::{PlatformRequest, Viewport};

#[test]
fn test_shell_multi_repo_default_stop_at_time_and_exhaustion() {
    let dir = tempfile::tempdir().unwrap();
    let log1 = dir.path().join("log1.txt");
    let log2 = dir.path().join("log2.txt");
    std::fs::write(&log1, "1635724800|Alice|A|file1.txt\n").unwrap();
    std::fs::write(&log2, "1635724800|Bob|A|file2.txt\n").unwrap();

    // Multi-repo without explicit stop-at-time or stop-position (triggers line 93: stop_at_time = 60.0)
    let conf_content = format!(
        "[display]\nviewport=800x600\n\n[gource]\npath={}\n\n[gource]\npath={}\n",
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

    // When recording is true, after repo 2 finishes, current_repo_idx >= gource_sections.len()
    // triggers line 77: return Ok(None)
    // and line 183-185: shell.is_finished = true, shell.requests.push_back(PlatformRequest::Quit)
    let mut app = GourceApp::new(config, AppOptions { recording: true }).expect("create app");
    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(glam::UVec2::new(800, 600));

    // First frame initializes gource
    app.frame(0.016, viewport, &mut list);
    let _ = app.take_requests();

    // Verify first repo stop_at_time was defaulted to 60.0
    assert_eq!(
        app.shell().gource.as_ref().unwrap().settings.stop_at_time,
        60.0
    );

    // Trigger repo switch via Return key is only when repo_count > 1, but when recording,
    // let's trigger finish on current gource
    app.shell_mut().gource.as_mut().unwrap().is_finished = true;
    app.frame(0.016, viewport, &mut list);
    let _ = app.take_requests();

    // Repo 2 is now loaded. Verify its stop_at_time is also 60.0
    assert_eq!(
        app.shell().gource.as_ref().unwrap().settings.stop_at_time,
        60.0
    );

    // Finish repo 2
    app.shell_mut().gource.as_mut().unwrap().is_finished = true;
    // This frame triggers get_next -> returns Ok(None) -> triggers Quit
    app.frame(0.016, viewport, &mut list);
    let reqs = app.take_requests();

    assert!(app.is_finished());
    assert!(reqs.contains(&PlatformRequest::Quit));
}

#[test]
fn test_shell_recording_alt_return_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("log.txt");
    std::fs::write(&log, "1635724800|Alice|A|file1.txt\n").unwrap();

    let args = ["gource".to_string(), log.to_str().unwrap().to_string()];
    let CliAction::Run(config) = parse_command_line(&args[1..]).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions { recording: true }).unwrap();
    let viewport = Viewport::new(640, 480);
    let mut list = DrawList::new(glam::UVec2::new(640, 480));
    app.frame(0.016, viewport, &mut list);

    // Alt+Return when recording: line 142 should ignore ToggleFullscreen
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
    let reqs = app.take_requests();
    assert!(!reqs.contains(&PlatformRequest::ToggleFullscreen));
}

#[test]
fn test_shell_f5_reset_when_gource_present() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("log.txt");
    std::fs::write(&log, "1635724800|Alice|A|file1.txt\n").unwrap();

    let args = ["gource".to_string(), log.to_str().unwrap().to_string()];
    let CliAction::Run(config) = parse_command_line(&args[1..]).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    let viewport = Viewport::new(640, 480);
    let mut list = DrawList::new(glam::UVec2::new(640, 480));
    app.frame(0.016, viewport, &mut list);

    // F5 triggers gource.reset()
    app.input(&InputEvent::KeyDown {
        key: Key::F5,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    // Check app still alive and running
    assert!(!app.is_finished());
}

#[test]
fn test_shell_frame_when_gource_is_none() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("log.txt");
    std::fs::write(&log, "1635724800|Alice|A|file1.txt\n").unwrap();

    let args = ["gource".to_string(), log.to_str().unwrap().to_string()];
    let CliAction::Run(config) = parse_command_line(&args[1..]).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    let viewport = Viewport::new(640, 480);
    let mut list = DrawList::new(glam::UVec2::new(640, 480));

    // Clear gource to None (triggers match &self.gource { None => true } on line 169)
    app.shell_mut().gource = None;
    app.frame(0.016, viewport, &mut list);

    // Frame should load next gource or terminate cleanly
    let reqs = app.take_requests();
    assert!(app.is_finished());
    assert!(reqs.contains(&PlatformRequest::Quit));
}

#[test]
fn test_shell_frame_get_next_error_emits_fatal_and_quit() {
    let dir = tempfile::tempdir().unwrap();
    let log1 = dir.path().join("log1.txt");
    let log2 = dir.path().join("log2.txt");
    let bad_font = dir.path().join("bad_font.ttf");
    std::fs::write(&log1, "1635724800|Alice|A|file1.txt\n").unwrap();
    std::fs::write(&log2, "1635724800|Bob|A|file2.txt\n").unwrap();
    std::fs::write(&bad_font, b"not a valid font").unwrap();

    // Repo 1 is valid, but Repo 2 has an invalid font-file which causes Gource::new inside get_next to fail
    let conf_content = format!(
        "[display]\nviewport=800x600\n\n[gource]\npath={}\n\n[gource]\npath={}\nfont-file={}\n",
        log1.display(),
        log2.display(),
        bad_font.display()
    );

    let conf =
        gource_settings::ConfFile::parse(&conf_content, "multi_err.conf").expect("parse conf");
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
    let mut list = DrawList::new(glam::UVec2::new(800, 600));

    // First frame initializes repo 1
    app.frame(0.016, viewport, &mut list);
    let _ = app.take_requests();

    // Trigger repo switch with Return key
    app.input(&InputEvent::KeyDown {
        key: Key::Return,
        modifiers: Modifiers::default(),
        repeat: false,
    });

    // This frame attempts get_next for repo 2, which fails and triggers lines 187-191:
    // self.is_finished = true;
    // self.requests.push_back(PlatformRequest::Fatal(err.0));
    // self.requests.push_back(PlatformRequest::Quit);
    app.frame(0.016, viewport, &mut list);
    let reqs = app.take_requests();

    assert!(app.is_finished());
    assert!(reqs.iter().any(|r| matches!(r, PlatformRequest::Fatal(_))));
    assert!(reqs.contains(&PlatformRequest::Quit));
}

#[test]
fn test_gource_direct_input_escape_and_fullscreen() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("log.txt");
    std::fs::write(&log, "1635724800|Alice|A|file1.txt\n").unwrap();

    let args = ["gource".to_string(), log.to_str().unwrap().to_string()];
    let CliAction::Run(config) = parse_command_line(&args[1..]).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    let viewport = Viewport::new(640, 480);
    let mut list = DrawList::new(glam::UVec2::new(640, 480));

    // Wait for log to load so commitlog is Some
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        app.frame(0.016, viewport, &mut list);
        let _ = app.take_requests();
        if app
            .shell()
            .gource
            .as_ref()
            .is_some_and(|g| g.commitlog.is_some())
        {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "log load timeout");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }

    let gource = app.shell_mut().gource.as_mut().unwrap();

    // 1. Repeat key ignored
    gource.input(&InputEvent::KeyDown {
        key: Key::Char('='),
        modifiers: Modifiers::default(),
        repeat: true,
    });

    // 2. Alt+Return on gource directly emits ToggleFullscreen
    gource.input(&InputEvent::KeyDown {
        key: Key::Return,
        modifiers: Modifiers {
            alt: true,
            ctrl: false,
            shift: false,
            meta: false,
        },
        repeat: false,
    });
    assert!(
        gource
            .pending_requests
            .contains(&PlatformRequest::ToggleFullscreen)
    );

    // 3. F11 on gource directly emits ToggleFrameless
    gource.input(&InputEvent::KeyDown {
        key: Key::F11,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert!(
        gource
            .pending_requests
            .contains(&PlatformRequest::ToggleFrameless)
    );

    // 4. Escape on gource directly emits Quit and marks is_finished
    gource.input(&InputEvent::KeyDown {
        key: Key::Escape,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert!(gource.is_finished);
    assert!(gource.pending_requests.contains(&PlatformRequest::Quit));

    // 5. Escape when disable_input is true
    gource.settings.disable_input = true;
    gource.is_finished = false;
    gource.input(&InputEvent::KeyDown {
        key: Key::Escape,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert!(gource.is_finished);
}

#[test]
fn test_gource_user_fade_deselection() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("log.txt");
    std::fs::write(&log, "1635724800|Alice|A|file1.txt\n").unwrap();

    let args = ["gource".to_string(), log.to_str().unwrap().to_string()];
    let CliAction::Run(config) = parse_command_line(&args[1..]).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    let viewport = Viewport::new(640, 480);
    let mut list = DrawList::new(glam::UVec2::new(640, 480));

    // Run until commit is processed and user is created
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        app.frame(0.016, viewport, &mut list);
        let _ = app.take_requests();
        if app
            .shell()
            .gource
            .as_ref()
            .is_some_and(|g| !g.world.users.is_empty())
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "user creation timeout"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }

    let gource = app.shell_mut().gource.as_mut().unwrap();
    let uid = gource.world.users.keys().next().unwrap();
    gource.select_user(Some(uid));
    assert_eq!(gource.selected_user, Some(uid));

    // Make the user fade by clearing actions and increasing pawn.elapsed beyond user_idle_time
    if let Some(user) = gource.world.users.get_mut(uid) {
        user.actions.clear();
        user.active_actions.clear();
        user.pawn.elapsed = 100.0;
        user.last_action = 0.0;
    }

    // Step a frame: logic checks if selected user is fading and deselects (line 1227)
    app.frame(0.016, viewport, &mut list);
    assert_eq!(app.shell().gource.as_ref().unwrap().selected_user, None);
}
