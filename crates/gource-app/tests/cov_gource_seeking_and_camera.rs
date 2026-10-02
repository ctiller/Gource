//! Coverage tests for gource.rs: seeking, looping, auto-skip, auto-rotate, stop-position.

use std::time::Duration;

use gource_app::PlatformRequest;
use gource_app::app::{AppOptions, GourceApp};
use gource_app::input::{InputEvent, MouseButton};
use gource_app::platform::Viewport;
use gource_core::Vec2;
use gource_draw::DrawList;
use gource_settings::{CliAction, parse_command_line};

#[test]
fn test_gource_seek_to_and_slider_hover_date() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("seek_test.log");
    // Multiple commits across several timestamps
    let mut content = String::new();
    for i in 0..50 {
        let t = 1635724800 + i * 3600;
        content.push_str(&format!("{t}|User{i}|A|path/file{i}.txt\n"));
    }
    std::fs::write(&log_path, &content).unwrap();

    let argv = vec![
        "gource".to_string(),
        "--seconds-per-day".to_string(),
        "0.1".to_string(),
        log_path.to_str().unwrap().to_string(),
    ];
    let CliAction::Run(config) = parse_command_line(&argv).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(gource_core::UVec2::new(800, 600));

    // Wait for log to load
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        app.frame(0.016, viewport, &mut list);
        let _ = app.take_requests();
        if app.shell().gource.as_ref().is_some_and(|g| {
            g.logmill.as_ref().is_none_or(|m| m.is_finished()) && g.commitlog.is_some()
        }) {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "log failed to load");
        std::thread::sleep(Duration::from_millis(2));
    }

    // Advance 10 frames
    for _ in 0..10 {
        app.frame(0.016, viewport, &mut list);
        let _ = app.take_requests();
    }

    // Hover over slider to trigger date_at_position & date_text_width
    // Slider bounds: min.y is 600 - 70 = 530, max.y is 600 - 35 = 565.
    let slider_pos = Vec2::new(400.0, 545.0);
    app.input(&InputEvent::MouseMove {
        pos: slider_pos,
        delta: Vec2::ZERO,
    });
    app.frame(0.016, viewport, &mut list);

    // Click slider to trigger seek_to
    app.input(&InputEvent::MouseButton {
        button: MouseButton::Left,
        pressed: true,
        pos: slider_pos,
    });
    app.frame(0.016, viewport, &mut list);

    // Call seek_to while paused
    let g = app.shell_mut().gource.as_mut().unwrap();
    g.paused = true;
    g.seek_to(0.5);
    assert!(!g.paused, "seek_to unpauses");

    // Call seek_to at 0.0
    g.seek_to(0.0);
    assert_eq!(g.currtime, 0);

    // Cover can_seek() branches:
    // 1. hide_progress = true -> returns false
    g.settings.hide_progress = true;
    assert!(!g.can_seek());
    g.settings.hide_progress = false;

    // 2. commitlog is None -> returns false
    let orig_log = g.commitlog.take();
    assert!(!g.can_seek());
    g.commitlog = orig_log;
}

#[test]
fn test_gource_looping_and_idle_skip_to_commit() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("loop_test.log");
    // Commit 1 at T0, Commit 2 at T0 + 100000 (huge gap to test auto-skip)
    let content = "1635724800|Alice|A|file1.txt\n1635824800|Bob|A|file2.txt\n";
    std::fs::write(&log_path, content).unwrap();

    let argv = vec![
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
    let CliAction::Run(config) = parse_command_line(&argv).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(gource_core::UVec2::new(800, 600));

    // Wait for log load
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        app.frame(0.016, viewport, &mut list);
        let _ = app.take_requests();
        if app.shell().gource.as_ref().is_some_and(|g| {
            g.logmill.as_ref().is_none_or(|m| m.is_finished()) && g.commitlog.is_some()
        }) {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "log load timeout");
        std::thread::sleep(Duration::from_millis(2));
    }

    // Step frames to trigger auto-skip and eventually loop back
    for _ in 0..100 {
        app.frame(0.05, viewport, &mut list);
        let _ = app.take_requests();
    }
}

#[test]
fn test_gource_stop_position_and_start_position() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("stop_pos.log");
    let mut content = String::new();
    for i in 0..20 {
        let t = 1635724800 + i * 3600;
        content.push_str(&format!("{t}|User{i}|A|dir/f{i}.txt\n"));
    }
    std::fs::write(&log_path, &content).unwrap();

    let argv = vec![
        "gource".to_string(),
        "--start-position".to_string(),
        "0.1".to_string(),
        "--stop-position".to_string(),
        "0.5".to_string(),
        "--seconds-per-day".to_string(),
        "10.0".to_string(),
        log_path.to_str().unwrap().to_string(),
    ];
    let CliAction::Run(config) = parse_command_line(&argv).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(gource_core::UVec2::new(800, 600));

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        app.frame(0.016, viewport, &mut list);
        let _ = app.take_requests();
        if app.shell().gource.as_ref().is_some_and(|g| {
            g.logmill.as_ref().is_none_or(|m| m.is_finished()) && g.commitlog.is_some()
        }) {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "load timeout");
        std::thread::sleep(Duration::from_millis(2));
    }

    for _ in 0..50 {
        app.frame(0.05, viewport, &mut list);
        let _ = app.take_requests();
    }
}

#[test]
fn test_gource_aborting_text_rendering() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("abort.log");
    let content = "1635724800|Alice|A|file1.txt\n";
    std::fs::write(&log_path, content).unwrap();

    let argv = vec!["gource".to_string(), log_path.to_str().unwrap().to_string()];
    let CliAction::Run(config) = parse_command_line(&argv).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(gource_core::UVec2::new(800, 600));

    // First frame starts gource
    app.frame(0.016, viewport, &mut list);

    // Set is_finished on gource while commitlog is None to trigger "Aborting" text draw
    if let Some(ref mut g) = app.shell_mut().gource {
        g.is_finished = true;
    }
    app.frame(0.016, viewport, &mut list);
}

#[test]
fn test_gource_auto_rotate_and_aspect_ratio() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("rotate.log");
    let content = "1635724800|Alice|A|file1.txt\n";
    std::fs::write(&log_path, content).unwrap();

    let argv = vec!["gource".to_string(), log_path.to_str().unwrap().to_string()];
    let CliAction::Run(config) = parse_command_line(&argv).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(gource_core::UVec2::new(800, 600));

    // Wait for log
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        app.frame(0.016, viewport, &mut list);
        let _ = app.take_requests();
        if app.shell().gource.as_ref().is_some_and(|g| {
            g.logmill.as_ref().is_none_or(|m| m.is_finished()) && g.commitlog.is_some()
        }) {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "load timeout");
        std::thread::sleep(Duration::from_millis(2));
    }

    // Test rotation_remaining_angle in update_camera
    let g = app.shell_mut().gource.as_mut().unwrap();
    g.rotation_remaining_angle = 90.0;
    app.frame(0.016, viewport, &mut list);
    assert!(
        app.shell()
            .gource
            .as_ref()
            .unwrap()
            .rotation_remaining_angle
            < 90.0
    );

    // Also test aspect ratio check triggering rotation_remaining_angle:
    // Calling update_camera directly with dir_bounds configured
    let g = app.shell_mut().gource.as_mut().unwrap();
    g.world.dir_bounds.min = Vec2::new(0.0, 0.0);
    g.world.dir_bounds.max = Vec2::new(50.0, 300.0); // area 15000 > 10000, ratio 50/300 = 0.166 < 0.67
    g.rotation_remaining_angle = 0.0;
    g.update_camera(0.016, viewport);
    assert_eq!(g.rotation_remaining_angle, 90.0);

    // Test aspect <= 1.0 (portrait viewport): triggers else { h / w.max(1.0) }
    let portrait_viewport = Viewport::new(600, 800);
    g.world.dir_bounds.min = Vec2::new(0.0, 0.0);
    g.world.dir_bounds.max = Vec2::new(300.0, 50.0); // ratio 50/300 = 0.166 < 0.67
    g.rotation_remaining_angle = 0.0;
    g.update_camera(0.016, portrait_viewport);
    assert_eq!(g.rotation_remaining_angle, 90.0);
}

#[test]
fn test_gource_hover_tooltip_rendering() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("hover.log");
    let content = "1635724800|Alice|A|dir/test_file.txt\n";
    std::fs::write(&log_path, content).unwrap();

    let argv = vec!["gource".to_string(), log_path.to_str().unwrap().to_string()];
    let CliAction::Run(config) = parse_command_line(&argv).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(gource_core::UVec2::new(800, 600));

    // Wait for log
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        app.frame(0.016, viewport, &mut list);
        let _ = app.take_requests();
        if app.shell().gource.as_ref().is_some_and(|g| {
            g.logmill.as_ref().is_none_or(|m| m.is_finished()) && g.commitlog.is_some()
        }) {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "load timeout");
        std::thread::sleep(Duration::from_millis(2));
    }

    // Step frames to create user and file
    for _ in 0..5 {
        app.frame(0.016, viewport, &mut list);
        let _ = app.take_requests();
    }

    let g = app.shell_mut().gource.as_mut().unwrap();
    let user_id = g.world.users.keys().next();
    let file_id = g.world.files.keys().next();

    // 1. Set hover_file and render frame (triggers hover_file tooltip draw)
    g.hover_file = file_id;
    g.selected_file = None;
    app.frame(0.016, viewport, &mut list);

    // 2. Set hover_user and render frame (triggers hover_user tooltip draw)
    let g = app.shell_mut().gource.as_mut().unwrap();
    g.hover_file = None;
    g.hover_user = user_id;
    g.selected_user = None;
    app.frame(0.016, viewport, &mut list);

    // 3. User tracking with selected user and selected file camera bounds
    let g = app.shell_mut().gource.as_mut().unwrap();
    g.track_users = true;
    g.selected_user = user_id;
    g.selected_file = file_id;
    app.frame(0.016, viewport, &mut list);

    // 4. Test mousetrace selecting a file
    let g = app.shell_mut().gource.as_mut().unwrap();
    g.selected_user = None;
    g.selected_file = None;
    let proj = g.camera.projection(viewport.size());
    if let Some(fid) = file_id
        && let Some(f) = g.world.files.get_mut(fid)
    {
        f.pawn.hidden = false;
        let dir_pos = f
            .dir
            .and_then(|d| g.world.dirs.get(d))
            .map(|d| d.pos)
            .unwrap_or(Vec2::ZERO);
        let world_pos = f.absolute_pos(dir_pos);
        let screen_pos = proj.to_screen(world_pos);
        g.mouse_pos = screen_pos;
        g.world.sync_view(1.0);
        g.mousetrace(&proj);

        // Moving mouse away
        g.mouse_pos = gource_core::Vec2::new(-9999.0, -9999.0);
        g.mousetrace(&proj);
    }
}

#[test]
fn test_gource_additional_uncovered_paths() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("extra.log");
    // Directory modify commit, out of order commits for no_time_travel, and regular commits
    let content =
        "1635724800|Alice|M|somedir/\n1635724810|Bob|A|file1.txt\n1635724805|Charlie|A|file2.txt\n";
    std::fs::write(&log_path, content).unwrap();

    let captions_path = dir.path().join("captions.txt");
    // Multiple captions at same timestamp and different offsets
    let cap_content = "1635724800|First caption\n1635724800|Second caption (stack)\n";
    std::fs::write(&captions_path, cap_content).unwrap();

    let argv = vec![
        "gource".to_string(),
        "--caption-file".to_string(),
        captions_path.to_str().unwrap().to_string(),
        "--title".to_string(),
        "Project Title".to_string(),
        "--no-time-travel".to_string(),
        log_path.to_str().unwrap().to_string(),
    ];
    let CliAction::Run(config) = parse_command_line(&argv).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(gource_core::UVec2::new(800, 600));

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        app.frame(0.016, viewport, &mut list);
        let _ = app.take_requests();
        if app.shell().gource.as_ref().is_some_and(|g| {
            g.logmill.as_ref().is_none_or(|m| m.is_finished()) && g.commitlog.is_some()
        }) {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "load timeout");
        std::thread::sleep(Duration::from_millis(2));
    }

    // Process frames to trigger captions stacking, title start_y, directory modify, and no-time-travel
    for _ in 0..10 {
        app.frame(0.016, viewport, &mut list);
        let _ = app.take_requests();
    }

    // 1. Screenshot filename collision loop
    let dummy_screenshot = std::path::PathBuf::from("gource-0001.png");
    let _ = std::fs::write(&dummy_screenshot, b"dummy");
    let g = app.shell_mut().gource.as_mut().unwrap();
    g.trigger_screenshot();
    assert!(g.pending_requests.iter().any(|r| match r {
        PlatformRequest::Screenshot { path, .. } => path.ends_with("gource-0002.png"),
        _ => false,
    }));
    let _ = std::fs::remove_file(dummy_screenshot);

    // 2. Looping delay branch in logic()
    {
        let g = app.shell_mut().gource.as_mut().unwrap();
        g.settings.looping = true;
        g.commitqueue.clear();
        g.idle_time = 100.0;
        g.settings.loop_delay_seconds = 1.0;
    }
    // Step frame to trigger loop delay seek_to(0.0)
    app.frame(0.016, viewport, &mut list);
    let _ = app.take_requests();
}
