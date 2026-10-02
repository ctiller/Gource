use std::path::PathBuf;
use std::time::Duration;

use gource_app::app::{AppOptions, GourceApp, vcs_options};
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

fn wait_for_log_load(app: &mut GourceApp, viewport: Viewport, list: &mut DrawList) {
    for _ in 0..300 {
        app.frame(0.016, viewport, list);
        let _ = app.take_requests();
        if !app.display_date().is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn test_app_vcs_options_and_display_date() {
    let log_path = test_data_path("coverage_test.log");
    let args = [
        "gource".to_string(),
        "--log-format".to_string(),
        "custom".to_string(),
        "--git-branch".to_string(),
        "main".to_string(),
        "--file-filter".to_string(),
        ".*\\.tmp".to_string(),
        "--file-show-filter".to_string(),
        ".*\\.rs".to_string(),
        "--user-filter".to_string(),
        "bot.*".to_string(),
        "--user-show-filter".to_string(),
        "Alice".to_string(),
        "--hash-seed".to_string(),
        "42".to_string(),
        log_path.to_str().unwrap().to_string(),
    ];
    let cli_action = parse_command_line(&args[1..]).expect("parse");
    let config = match cli_action {
        CliAction::Run(cfg) => cfg,
        _ => unreachable!(),
    };

    let opts = vcs_options(&config.gource);
    assert_eq!(opts.log_format, "custom");
    assert_eq!(opts.git_branch, "main");
    assert_eq!(opts.filters.file_filters.len(), 1);
    assert_eq!(opts.filters.file_show_filters.len(), 1);
    assert_eq!(opts.filters.user_filters.len(), 1);
    assert_eq!(opts.filters.user_show_filters.len(), 1);
}

#[test]
fn test_app_accessors() {
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

    // Test accessors
    assert_eq!(app.display_date(), "");
    let _ = app.shell();
    let _ = app.shell_mut();
    let _ = app.gfx();
    assert!(!app.is_finished());

    wait_for_log_load(&mut app, viewport, &mut list);
    assert!(!app.display_date().is_empty());
}

#[test]
fn test_gource_disable_input() {
    let log_path = test_data_path("coverage_test.log");
    let args = [
        "gource".to_string(),
        "--disable-input".to_string(),
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

    wait_for_log_load(&mut app, viewport, &mut list);

    // Non-Escape keys should be ignored
    app.input(&InputEvent::KeyDown {
        key: Key::Space,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    app.frame(0.016, viewport, &mut list);
    assert!(!app.is_finished());

    // Mouse move should be ignored
    app.input(&InputEvent::MouseMove {
        pos: Vec2::new(100.0, 100.0),
        delta: Vec2::ZERO,
    });
    app.frame(0.016, viewport, &mut list);

    // Escape should quit even with disable_input
    app.input(&InputEvent::KeyDown {
        key: Key::Escape,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    let reqs = app.take_requests();
    assert!(reqs.contains(&PlatformRequest::Quit));
    assert!(app.is_finished());
}

#[test]
fn test_gource_selection_and_mousetrace_and_focus() {
    let log_path = test_data_path("coverage_test.log");
    let args = [
        "gource".to_string(),
        "--camera-mode".to_string(),
        "track".to_string(),
        "--seconds-per-day".to_string(),
        "0.1".to_string(),
        "--auto-skip-seconds".to_string(),
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

    wait_for_log_load(&mut app, viewport, &mut list);

    // Run enough frames to populate users and files in the scene
    for _ in 0..50 {
        app.frame(0.1, viewport, &mut list);
        let _ = app.take_requests();
        std::thread::sleep(Duration::from_millis(2));
    }

    // Access underlying gource to test direct selection, camera lock-on, and mousetrace
    let shell = app.shell_mut();
    if let Some(ref mut gource) = shell.gource {
        // Find a user and a file in world
        let user_id = gource.world.users.keys().next();
        let file_id = gource.world.files.keys().next();

        assert!(user_id.is_some(), "Should have users in world");
        assert!(file_id.is_some(), "Should have files in world");

        // 1. Select user
        gource.select_user(user_id);
        assert_eq!(gource.selected_user, user_id);
        assert!(gource.camera.is_locked_on());

        // Repeated selection is a no-op
        gource.select_user(user_id);

        // 2. Select file (swapping selection from user to file)
        gource.select_file(file_id);
        assert_eq!(gource.selected_file, file_id);
        assert_eq!(gource.selected_user, None);
        assert!(gource.camera.is_locked_on());

        // Repeated selection of file is a no-op
        gource.select_file(file_id);

        // 3. Select user again (swapping selection from file to user)
        gource.select_user(user_id);
        assert_eq!(gource.selected_user, user_id);
        assert_eq!(gource.selected_file, None);

        // 4. Deselect user
        gource.select_user(None);
        assert_eq!(gource.selected_user, None);
        assert!(!gource.camera.is_locked_on());

        // 5. Deselect file
        gource.select_file(file_id);
        gource.select_file(None);
        assert_eq!(gource.selected_file, None);
        assert!(!gource.camera.is_locked_on());

        // 6. Test mousetrace tooltip generation
        // Hover user tooltip
        gource.hover_user = user_id;
        gource.hover_file = None;

        // 7. Test mouse click on hover_user
        gource.input(&InputEvent::MouseButton {
            button: MouseButton::Left,
            pressed: true,
            pos: Vec2::new(512.0, 384.0),
        });
        assert_eq!(gource.selected_user, user_id);

        // 8. Hover file tooltip
        gource.hover_user = None;
        gource.hover_file = file_id;
        gource.input(&InputEvent::MouseButton {
            button: MouseButton::Left,
            pressed: true,
            pos: Vec2::new(512.0, 384.0),
        });
        assert_eq!(gource.selected_file, file_id);

        // 9. Click on empty background deselects and grabs mouse
        gource.hover_user = None;
        gource.hover_file = None;
        gource.input(&InputEvent::MouseButton {
            button: MouseButton::Left,
            pressed: true,
            pos: Vec2::new(10.0, 10.0),
        });
        assert_eq!(gource.selected_file, None);
        assert_eq!(gource.selected_user, None);
        assert!(gource.grab_mouse);

        // Release mouse button ungrabs mouse
        gource.input(&InputEvent::MouseButton {
            button: MouseButton::Left,
            pressed: false,
            pos: Vec2::new(10.0, 10.0),
        });
        assert!(!gource.grab_mouse);

        // 10. Mousetrace over user location in world
        let proj = gource.camera.projection(viewport.size());
        if let Some(uid) = user_id
            && let Some(u) = gource.world.users.get(uid)
        {
            let user_screen = proj.to_screen(u.pawn.pos);
            gource.mouse_pos = user_screen;
            gource.mousetrace(&proj);
        }

        // Test mousetrace at empty space clears hover
        gource.mouse_pos = Vec2::new(-9999.0, -9999.0);
        gource.mousetrace(&proj);
        assert_eq!(gource.hover_user, None);
        assert_eq!(gource.hover_file, None);

        // Now test drawing with hover_file and hover_user set to render tooltips
        gource.hover_file = file_id;
        gource.selected_file = None;
    }

    // Call app.frame() to draw the tooltip textbox for hover_file
    app.frame(0.016, viewport, &mut list);

    if let Some(ref mut gource) = app.shell_mut().gource {
        let user_id = gource.world.users.keys().next();
        gource.hover_file = None;
        gource.hover_user = user_id;
        gource.selected_user = None;
    }

    // Call app.frame() to draw the tooltip textbox for hover_user
    app.frame(0.016, viewport, &mut list);
}

#[test]
fn test_gource_time_travel_handling() {
    let log_path = test_data_path("coverage_test.log");
    // Test both with no_time_travel true (default) and false
    for no_time_travel in [true, false] {
        let mut args = vec![
            "gource".to_string(),
            "--seconds-per-day".to_string(),
            "0.05".to_string(),
        ];
        if !no_time_travel {
            args.push("--no-time-travel".to_string());
        }
        args.push(log_path.to_str().unwrap().to_string());

        let cli_action = parse_command_line(&args[1..]).expect("parse");
        let config = match cli_action {
            CliAction::Run(cfg) => cfg,
            _ => unreachable!(),
        };

        let mut app = GourceApp::new(config, AppOptions::default()).expect("create app");
        let viewport = Viewport::new(800, 600);
        let mut list = DrawList::new(gource_core::UVec2::new(800, 600));

        wait_for_log_load(&mut app, viewport, &mut list);

        // Run until end of commits
        for _ in 0..100 {
            app.frame(0.1, viewport, &mut list);
            let _ = app.take_requests();
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
