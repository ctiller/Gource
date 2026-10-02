//! Coverage tests for shell reload and viewport resize, Gource hover, captions, and logmill error handling.

use std::time::Duration;

use gource_app::app::{AppOptions, GourceApp};
use gource_app::input::{InputEvent, Key, Modifiers};
use gource_app::platform::{PlatformRequest, Viewport};
use gource_core::Vec2;
use gource_draw::{DrawList, Gfx, TextureOptions};
use gource_settings::{CliAction, parse_command_line};

fn wait_for_app_load(app: &mut GourceApp) {
    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(gource_core::UVec2::new(800, 600));

    // First frame spawns log loader
    app.frame(1.0 / 60.0, viewport, &mut list);
    let _ = app.take_requests();

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        app.frame(1.0 / 60.0, viewport, &mut list);
        let _ = app.take_requests();
        if app
            .shell()
            .gource
            .as_ref()
            .is_some_and(|g| g.commitlog.is_some())
        {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "log failed to load");
        std::thread::sleep(Duration::from_millis(2));
    }

    // Step a few more frames to process initial commits
    for _ in 0..5 {
        app.frame(1.0 / 60.0, viewport, &mut list);
        let _ = app.take_requests();
    }
}

// -----------------------------------------------------------------------------
// 1. Shell reload tests: missing file (Io error) and corrupted file (Decode error)
// -----------------------------------------------------------------------------

#[test]
fn test_shell_reload_file_deleted_io_error() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("log.txt");
    std::fs::write(&log_path, "1635724800|Alice|A|file1.txt\n").unwrap();

    let img_path = dir.path().join("custom.png");
    std::fs::write(&img_path, gource_draw::resources::FILE_PNG).unwrap();

    let args = ["gource".to_string(), log_path.to_str().unwrap().to_string()];
    let CliAction::Run(config) = parse_command_line(&args[1..]).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(gource_core::UVec2::new(800, 600));
    app.frame(0.016, viewport, &mut list);
    let _ = app.take_requests();

    // Load file-backed texture in shell.gfx.textures
    let _tex_id = app
        .shell_mut()
        .gfx
        .textures
        .load_file(&img_path, TextureOptions::plain())
        .expect("load texture");

    // Delete the file on disk
    std::fs::remove_file(&img_path).unwrap();

    // Trigger reload via shell.reload()
    app.shell_mut().reload();

    assert!(app.shell().is_finished);
    let reqs = app.take_requests();
    assert_eq!(
        reqs,
        vec![
            PlatformRequest::Fatal(format!(
                "failed to load resource '{}'",
                img_path.to_str().unwrap()
            )),
            PlatformRequest::Quit,
        ]
    );
}

#[test]
fn test_shell_reload_file_corrupted_decode_error() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("log.txt");
    std::fs::write(&log_path, "1635724800|Alice|A|file1.txt\n").unwrap();

    let img_path = dir.path().join("custom.png");
    std::fs::write(&img_path, gource_draw::resources::FILE_PNG).unwrap();

    let args = ["gource".to_string(), log_path.to_str().unwrap().to_string()];
    let CliAction::Run(config) = parse_command_line(&args[1..]).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(gource_core::UVec2::new(800, 600));
    app.frame(0.016, viewport, &mut list);
    let _ = app.take_requests();

    // Load file-backed texture
    let _tex_id = app
        .shell_mut()
        .gfx
        .textures
        .load_file(&img_path, TextureOptions::plain())
        .expect("load texture");

    // Overwrite the file with corrupt bytes
    std::fs::write(&img_path, b"corrupt not a png header or image bytes").unwrap();

    // Trigger reload via F5 key in shell
    app.input(&InputEvent::KeyDown {
        key: Key::F5,
        modifiers: Modifiers::default(),
        repeat: false,
    });

    assert!(app.shell().is_finished);
    let reqs = app.take_requests();
    assert_eq!(
        reqs,
        vec![
            PlatformRequest::Fatal(format!(
                "failed to load resource '{}'",
                img_path.to_str().unwrap()
            )),
            PlatformRequest::Quit,
        ]
    );
}

// -----------------------------------------------------------------------------
// 2. Viewport resize reload: lines 203-205 in shell.rs
// -----------------------------------------------------------------------------

#[test]
fn test_shell_viewport_resize_triggers_gource_reload() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("log.txt");
    std::fs::write(&log_path, "1635724800|Alice|A|file1.txt\n").unwrap();

    let args = ["gource".to_string(), log_path.to_str().unwrap().to_string()];
    let CliAction::Run(config) = parse_command_line(&args[1..]).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    let vp1 = Viewport::new(1024, 768);
    let mut list = DrawList::new(gource_core::UVec2::new(1024, 768));

    // First frame with initial viewport matches last_viewport (no reload)
    app.frame(0.016, vp1, &mut list);
    assert!(!app.shell().gource.as_ref().unwrap().reloaded);

    // Frame with a different viewport triggers resize reload (lines 203-207)
    let vp2 = Viewport::new(800, 600);
    let mut list2 = DrawList::new(gource_core::UVec2::new(800, 600));
    app.frame(0.016, vp2, &mut list2);

    assert_eq!(app.shell().last_viewport, Some(vp2));
    assert!(app.shell().gource.as_ref().unwrap().reloaded);
}

// -----------------------------------------------------------------------------
// 3. Captions offset and reloaded repositioning: lines 184-192 and 1273-1283
// -----------------------------------------------------------------------------

#[test]
fn test_caption_offset_x_and_reloaded_repositioning() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("log.txt");
    std::fs::write(
        &log_path,
        "1635724800|Alice|A|file1.txt\n1635725000|Alice|M|file1.txt\n",
    )
    .unwrap();

    let cap_path = dir.path().join("captions.txt");
    std::fs::write(
        &cap_path,
        "2021-11-01 00:00:00|Caption 1\n2021-11-01 00:01:00|Caption 2\n",
    )
    .unwrap();

    // 1. Negative offset: -20
    {
        let args = [
            "gource".to_string(),
            "--caption-file".to_string(),
            cap_path.to_str().unwrap().to_string(),
            "--caption-offset".to_string(),
            "-20".to_string(),
            "--caption-duration".to_string(),
            "100.0".to_string(),
            log_path.to_str().unwrap().to_string(),
        ];
        let CliAction::Run(config) = parse_command_line(&args[1..]).unwrap() else {
            panic!("expected run");
        };

        let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
        wait_for_app_load(&mut app);

        // Advance frames so captions are triggered
        let viewport = Viewport::new(800, 600);
        let mut list = DrawList::new(gource_core::UVec2::new(800, 600));
        for _ in 0..10 {
            app.frame(0.05, viewport, &mut list);
            let _ = app.take_requests();
        }

        let gource = app.shell_mut().gource.as_mut().unwrap();
        assert!(!gource.active_captions.is_empty());
        let cap_x = gource.active_captions[0].pos.x;
        // With negative offset -20 and display width 800: x = 800 - 20 - width
        assert!(cap_x > 0.0);

        // Test reload repositioning (lines 1277-1281)
        gource.reloaded = true;
        app.frame(0.016, viewport, &mut list);
        assert!(!app.shell().gource.as_ref().unwrap().reloaded);
    }

    // 2. Zero offset: 0 (centred)
    {
        let args = [
            "gource".to_string(),
            "--caption-file".to_string(),
            cap_path.to_str().unwrap().to_string(),
            "--caption-offset".to_string(),
            "0".to_string(),
            "--caption-duration".to_string(),
            "100.0".to_string(),
            log_path.to_str().unwrap().to_string(),
        ];
        let CliAction::Run(config) = parse_command_line(&args[1..]).unwrap() else {
            panic!("expected run");
        };

        let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
        wait_for_app_load(&mut app);

        let viewport = Viewport::new(800, 600);
        let mut list = DrawList::new(gource_core::UVec2::new(800, 600));
        for _ in 0..10 {
            app.frame(0.05, viewport, &mut list);
            let _ = app.take_requests();
        }

        let gource = app.shell_mut().gource.as_mut().unwrap();
        assert!(!gource.active_captions.is_empty());
        let cap_x = gource.active_captions[0].pos.x;
        // Centred: (800 / 2) - (width / 2)
        assert!(cap_x > 0.0);

        // Trigger resize to test repositioning with new width
        let vp_new = Viewport::new(1200, 900);
        let mut list2 = DrawList::new(gource_core::UVec2::new(1200, 900));
        app.frame(0.016, vp_new, &mut list2);
        let gource_after = app.shell().gource.as_ref().unwrap();
        let cap_x_after = gource_after.active_captions[0].pos.x;
        assert!(cap_x_after > cap_x);
    }

    // 3. Positive offset: 50
    {
        let args = [
            "gource".to_string(),
            "--caption-file".to_string(),
            cap_path.to_str().unwrap().to_string(),
            "--caption-offset".to_string(),
            "50".to_string(),
            "--caption-duration".to_string(),
            "100.0".to_string(),
            log_path.to_str().unwrap().to_string(),
        ];
        let CliAction::Run(config) = parse_command_line(&args[1..]).unwrap() else {
            panic!("expected run");
        };

        let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
        wait_for_app_load(&mut app);

        let viewport = Viewport::new(800, 600);
        let mut list = DrawList::new(gource_core::UVec2::new(800, 600));
        for _ in 0..10 {
            app.frame(0.05, viewport, &mut list);
            let _ = app.take_requests();
        }

        let gource = app.shell().gource.as_ref().unwrap();
        assert!(!gource.active_captions.is_empty());
        assert_eq!(gource.active_captions[0].pos.x, 50.0);
    }
}

// -----------------------------------------------------------------------------
// 4. Mouseover / hover transitions: lines 1509-1550 in gource.rs
// -----------------------------------------------------------------------------

#[test]
fn test_mouseover_hover_file_and_user_transitions() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("log.txt");
    std::fs::write(
        &log_path,
        "1635724800|Alice|A|src/test1.txt\n1635724810|Bob|A|src/test2.txt\n",
    )
    .unwrap();

    let args = ["gource".to_string(), log_path.to_str().unwrap().to_string()];
    let CliAction::Run(config) = parse_command_line(&args[1..]).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(gource_core::UVec2::new(800, 600));

    wait_for_app_load(&mut app);

    // Run enough frames to populate the scene
    for _ in 0..30 {
        app.frame(0.05, viewport, &mut list);
        let _ = app.take_requests();
    }

    let gource = app.shell_mut().gource.as_mut().unwrap();

    // Verify we have users and files populated
    let mut file_ids = gource.world.files.keys();
    let fid1 = file_ids.next().expect("file 1 exists");
    let fid2 = file_ids.next().expect("file 2 exists");

    let uid = gource.world.users.keys().next().expect("user exists");

    // Position files and user distinctly in world space
    let root_dir = gource.world.root;
    gource.world.dirs[root_dir].pos = Vec2::ZERO;

    let f1 = gource.world.files.get_mut(fid1).unwrap();
    f1.pawn.pos = Vec2::new(50.0, 50.0);
    f1.pawn.size = 30.0;
    f1.pawn.set_hidden(false);

    let f2 = gource.world.files.get_mut(fid2).unwrap();
    f2.pawn.pos = Vec2::new(150.0, 50.0);
    f2.pawn.size = 30.0;
    f2.pawn.set_hidden(false);

    let u = gource.world.users.get_mut(uid).unwrap();
    u.pawn.pos = Vec2::new(50.0, 150.0);
    u.pawn.size = 30.0;
    u.pawn.set_hidden(false);
    u.pawn.elapsed = 0.0;

    let proj = gource.camera.projection(viewport.size());

    // 1. Hover over user
    gource.mouse_pos = proj.to_screen(Vec2::new(50.0, 150.0));
    gource.mousetrace(&proj);
    assert_eq!(gource.hover_user, Some(uid));
    assert_eq!(gource.hover_file, None);
    assert!(gource.world.users[uid].pawn.is_mouseover());

    // 2. Switch hover from user to file fid1 (triggers lines 1518-1522 and 1523-1533)
    gource.mouse_pos = proj.to_screen(Vec2::new(50.0, 50.0));
    gource.mousetrace(&proj);
    assert_eq!(gource.hover_file, Some(fid1));
    assert_eq!(gource.hover_user, None);
    assert!(gource.world.files[fid1].pawn.is_mouseover());
    assert!(!gource.world.users[uid].pawn.is_mouseover());

    // 3. Same file hover again: should hit `self.hover_file != Some(fid)` false branch
    gource.mousetrace(&proj);
    assert_eq!(gource.hover_file, Some(fid1));
    assert!(gource.world.files[fid1].pawn.is_mouseover());

    // 4. Switch hover from file fid1 to file fid2 (triggers lines 1524-1532)
    gource.mouse_pos = proj.to_screen(Vec2::new(150.0, 50.0));
    gource.mousetrace(&proj);
    assert_eq!(gource.hover_file, Some(fid2));
    assert!(gource.world.files[fid2].pawn.is_mouseover());
    assert!(!gource.world.files[fid1].pawn.is_mouseover());

    // 5. Switch hover from file fid2 to user uid (triggers lines 1534-1550)
    gource.mouse_pos = proj.to_screen(Vec2::new(50.0, 150.0));
    gource.mousetrace(&proj);
    assert_eq!(gource.hover_user, Some(uid));
    assert_eq!(gource.hover_file, None);
    assert!(gource.world.users[uid].pawn.is_mouseover());
    assert!(!gource.world.files[fid2].pawn.is_mouseover());

    // 6. Same user hover again: should hit `self.hover_user != Some(uid)` false branch
    gource.mousetrace(&proj);
    assert_eq!(gource.hover_user, Some(uid));
    assert!(gource.world.users[uid].pawn.is_mouseover());

    // 7. Move mouse to empty space: clears both hovers (lines 1551-1560)
    gource.mouse_pos = proj.to_screen(Vec2::new(500.0, 500.0));
    gource.mousetrace(&proj);
    assert_eq!(gource.hover_file, None);
    assert_eq!(gource.hover_user, None);
    assert!(!gource.world.users[uid].pawn.is_mouseover());
    assert!(!gource.world.files[fid2].pawn.is_mouseover());
}

// -----------------------------------------------------------------------------
// 5. LogMill error handling in Gource::logic (lines 1138-1148)
// -----------------------------------------------------------------------------

#[test]
fn test_gource_logic_logmill_error_non_empty() {
    let dir = tempfile::tempdir().unwrap();
    let invalid_dir = dir.path().join("not_a_repo");
    std::fs::create_dir(&invalid_dir).unwrap();

    let args = [
        "gource".to_string(),
        invalid_dir.to_str().unwrap().to_string(),
    ];
    let CliAction::Run(config) = parse_command_line(&args[1..]).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(gource_core::UVec2::new(800, 600));

    // Wait until background logmill thread finishes and reports error
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        app.frame(0.016, viewport, &mut list);
        let reqs = app.take_requests();
        if app.is_finished() {
            assert!(reqs.iter().any(|r| {
                matches!(
                    r,
                    PlatformRequest::Fatal(msg) if msg.contains("directory not supported")
                )
            }));
            assert!(reqs.contains(&PlatformRequest::Quit));
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "expected error timeout"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

// -----------------------------------------------------------------------------
// 6. Additional coverage: follow_users, delete_user hover/selected, user_image_map,
//    hide_mouse cursor, date_at_position, seek_to, manual tree rotate, debug bounds
// -----------------------------------------------------------------------------

#[test]
fn test_gource_follow_user_and_delete_user_selection() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("log.txt");
    std::fs::write(&log_path, "1635724800|Alice|A|file1.txt\n").unwrap();

    let args = [
        "gource".to_string(),
        "--follow-user".to_string(),
        "Alice".to_string(),
        log_path.to_str().unwrap().to_string(),
    ];
    let CliAction::Run(config) = parse_command_line(&args[1..]).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    wait_for_app_load(&mut app);

    let gource = app.shell_mut().gource.as_mut().unwrap();
    let uid = gource.world.users.keys().next().unwrap();
    // follow_users should have selected Alice (line 1364)
    assert_eq!(gource.selected_user, Some(uid));

    // Set hover_user and selected_user to test delete_user clearing them (lines 1388, 1391)
    gource.hover_user = Some(uid);
    gource.selected_user = Some(uid);

    let viewport = Viewport::new(800, 600);
    let mut gfx = Gfx::new();

    // Make user inactive so logic() calls delete_user (lines 1380-1382, 1388, 1391)
    if let Some(user) = gource.world.users.get_mut(uid) {
        user.actions.clear();
        user.active_actions.clear();
        user.pawn.elapsed = 1000.0;
        user.last_action = 0.0;
    }

    // One full fixed tick (1/60 s) so the simulation advances.
    gource.logic(0.02, viewport, &mut gfx).unwrap();
    assert_eq!(gource.hover_user, None);
    assert_eq!(gource.selected_user, None);
    assert!(!gource.world.users.contains_key(uid));
}

#[test]
fn test_gource_user_image_dir_map_and_assign_user_image() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("log.txt");
    std::fs::write(&log_path, "1635724800|Alice|A|file1.txt\n").unwrap();

    let user_dir = dir.path().join("users");
    std::fs::create_dir(&user_dir).unwrap();
    let alice_img = user_dir.join("Alice.png");
    std::fs::write(&alice_img, gource_draw::resources::USER_PNG).unwrap();

    let args = [
        "gource".to_string(),
        "--user-image-dir".to_string(),
        user_dir.to_str().unwrap().to_string(),
        log_path.to_str().unwrap().to_string(),
    ];
    let CliAction::Run(config) = parse_command_line(&args[1..]).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    let mut gfx = Gfx::new();

    // Call assign_user_image with invalid UserId: hits line 1066
    let invalid_uid = slotmap::KeyData::from_ffi(9999).into();
    let gource = app.shell_mut().gource.as_mut().unwrap();
    assert!(gource.assign_user_image(invalid_uid, &mut gfx).is_ok());

    wait_for_app_load(&mut app);

    // After loading Alice, user_image_map was consulted (line 1070)
    let gource = app.shell().gource.as_ref().unwrap();
    let uid = gource.world.users.keys().next().unwrap();
    assert!(gource.world.users[uid].graphic.is_some());
}

#[test]
fn test_gource_seek_to_and_date_at_position() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("log.txt");
    std::fs::write(
        &log_path,
        "1635724800|Alice|A|file1.txt\n1635730000|Bob|A|file2.txt\n",
    )
    .unwrap();

    let args = ["gource".to_string(), log_path.to_str().unwrap().to_string()];
    let CliAction::Run(config) = parse_command_line(&args[1..]).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    wait_for_app_load(&mut app);

    let gource = app.shell_mut().gource.as_mut().unwrap();

    // Test seek_to while paused (hits line 507-508)
    gource.paused = true;
    gource.seek_to(0.5);
    assert!(!gource.paused);

    // Test date_at_position at 0.0 (valid commit) and at 1.0 (hits line 524 returning empty string)
    let d0 = gource.date_at_position(0.0);
    assert!(!d0.is_empty());
    let d1 = gource.date_at_position(1.0);
    assert_eq!(d1, "");
}

#[test]
fn test_gource_manual_rotate_and_debug_bounds() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("log.txt");
    std::fs::write(&log_path, "1635724800|Alice|A|file1.txt\n").unwrap();

    let args = [
        "gource".to_string(),
        "--hide-mouse".to_string(),
        log_path.to_str().unwrap().to_string(),
    ];
    let CliAction::Run(config) = parse_command_line(&args[1..]).unwrap() else {
        panic!("expected run");
    };

    // --hide-mouse covers line 323 in Gource::new
    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    wait_for_app_load(&mut app);

    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(gource_core::UVec2::new(800, 600));

    let gource = app.shell_mut().gource.as_mut().unwrap();

    // Test manual tree rotation (lines 1160-1162) and non-manual tree rotation (lines 1164-1165)
    let mut gfx = Gfx::new();

    // 1. Non-manual rotation
    gource.manual_rotate = false;
    gource.rotate_angle = 0.5;
    gource.logic(0.016, viewport, &mut gfx).unwrap();
    assert_eq!(gource.rotate_angle, 0.0);

    // 2. Manual rotation
    gource.manual_rotate = true;
    gource.rotate_angle = 0.5;
    gource.logic(0.016, viewport, &mut gfx).unwrap();
    assert_eq!(gource.rotate_angle, 0.0);

    // 3. Debug bounds rendering: lines 1629-1639 (with track_users = false and track_users = true)
    gource.debug = true;
    gource.track_users = false;
    gource.draw(0.016, viewport, &mut gfx, &mut list);

    gource.track_users = true;
    gource.draw(0.016, viewport, &mut gfx, &mut list);

    // 4. Loading screen when commitlog is None and is_finished = true: hits lines 1585, 1592
    gource.commitlog = None;
    gource.is_finished = true;
    gource.runtime = 1.0; // 1.0 * 3.0 = 3.0 % 4 = 3 (dots "...")
    gource.draw(0.016, viewport, &mut gfx, &mut list);
    gource.runtime = 0.7; // 0.7 * 3.0 = 2.1 as i32 = 2 (dots "..")
    gource.draw(0.016, viewport, &mut gfx, &mut list);
}
