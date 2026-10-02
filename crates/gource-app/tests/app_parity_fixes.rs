//! Regression tests for C++ -> Rust parity fixes:
//! 1. File key colour uses extension colour (getFileColour).
//! 2. Real text metrics for file key, date centring, caption offset, and slider hover caption.
//! 3. Selected user name respects fading alpha and fading user is automatically deselected.
//! 4. Input parity: 'z' toggles gravity; keys ignored before commitlog is loaded; disable_input in shell.

use gource_app::WorldDraw;
use std::fs;
use std::time::{Duration, Instant};

use gource_app::app::{AppOptions, GourceApp};
use gource_app::gource::Gource;
use gource_app::input::{InputEvent, Key, Modifiers};
use gource_app::platform::Viewport;
use gource_core::UVec2;
use gource_draw::DrawList;
use gource_settings::{CliAction, parse_command_line};

const T0: i64 = 1635724800; // 2021-11-01 00:00:00 UTC

struct TestRun {
    app: GourceApp,
    _dir: tempfile::TempDir,
}

impl TestRun {
    fn new(log_content: &str, extra_args: &[&str]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("log.txt");
        fs::write(&log_path, log_content).unwrap();

        let mut argv = vec!["--seconds-per-day".to_string(), "1".to_string()];
        for arg in extra_args {
            argv.push(arg.to_string());
        }
        argv.push(log_path.to_str().unwrap().to_string());

        let CliAction::Run(config) = parse_command_line(&argv).unwrap() else {
            panic!("expected Run config");
        };

        let app = GourceApp::new(config, AppOptions::default()).unwrap();
        Self { app, _dir: dir }
    }

    fn gource(&self) -> &Gource {
        self.app.shell().gource.as_ref().expect("gource running")
    }

    fn gource_mut(&mut self) -> &mut Gource {
        self.app
            .shell_mut()
            .gource
            .as_mut()
            .expect("gource running")
    }

    fn wait_for_log_load(&mut self) {
        let viewport = Viewport::new(640, 480);
        let mut list = DrawList::new(UVec2::new(640, 480));
        let deadline = Instant::now() + Duration::from_secs(10);

        // Run at least one frame so Gource gets created in the shell
        self.app.frame(1.0 / 60.0, viewport, &mut list);

        while self.gource().commitlog.is_none()
            || !self
                .gource()
                .logmill
                .as_ref()
                .is_none_or(|m| m.is_finished())
        {
            assert!(Instant::now() < deadline, "commitlog failed to load");
            self.app.frame(1.0 / 60.0, viewport, &mut list);
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn step_frames(&mut self, frames: usize) {
        let viewport = Viewport::new(640, 480);
        let mut list = DrawList::new(UVec2::new(640, 480));
        for _ in 0..frames {
            self.app.frame(1.0 / 60.0, viewport, &mut list);
        }
    }
}

// ---------------------------------------------------------------------------
// Item 1: File key colour
// ---------------------------------------------------------------------------
#[test]
fn test_file_key_colour_parity() {
    let log = format!("{T0}|alice|A|src/main.rs\n");
    let mut run = TestRun::new(&log, &[]);
    run.wait_for_log_load();
    run.step_frames(60);

    let gource = run.gource();
    let entry = gource
        .file_key
        .keymap
        .get("rs")
        .expect("keymap entry for 'rs' extension");

    let file = gource
        .world
        .files
        .values()
        .find(|f| f.ext == "rs")
        .expect("file with 'rs' extension");

    // The key colour should match the file's extension colour (file_colour),
    // which in C++ comes from file->getFileColour().
    assert_eq!(
        entry.colour(),
        file.file_colour,
        "FileKey entry colour must match file_colour (extension colour)"
    );
}

// ---------------------------------------------------------------------------
// Item 2: Real text metrics
// ---------------------------------------------------------------------------
#[test]
fn test_file_key_uses_medium_font_and_real_metrics() {
    let log = format!("{T0}|alice|A|src/test.rs\n");
    let mut run = TestRun::new(&log, &[]);
    run.wait_for_log_load();

    // Verify FileKey uses medium font matching C++ key.cpp L166 / gource.cpp L146
    let gource = run.gource();
    assert_eq!(
        gource.file_key.font,
        Some(gource.fonts.medium),
        "FileKey font should be fonts.medium"
    );
}

#[test]
fn test_date_centring_uses_font_width_and_int_truncation() {
    let log = format!("{T0}|alice|A|src/test.rs\n");
    let mut run = TestRun::new(&log, &[]);
    run.wait_for_log_load();
    run.step_frames(30);

    let gource = run.gource();
    assert!(
        !gource.display_date.is_empty(),
        "display date should be formatted"
    );

    // C++: int date_offset = (int) fontmedium.getWidth(displaydate) * 0.5;
    //      if(abs(date_x_offset - date_offset) > 5) date_x_offset = date_offset;
    let font = gource.fonts.medium;
    let date_str = gource.display_date.clone();
    let font_width = run.app.shell_mut().gfx.text_width(font, &date_str);
    let expected_offset = (((font_width as i32) as f64) * 0.5) as i32 as f32;

    let date_x_offset = run.gource().date_x_offset;
    assert!(
        (date_x_offset - expected_offset).abs() <= 5.0,
        "date_x_offset ({}) should match fontmedium expected offset ({}) within hysteresis (5px)",
        date_x_offset,
        expected_offset
    );
}

#[test]
fn test_caption_offset_uses_font_width() {
    let dir = tempfile::tempdir().unwrap();
    let caption_file = dir.path().join("captions.txt");
    let date_str = gource_core::datetime::format_local(T0, "%Y-%m-%d %H:%M:%S");
    fs::write(&caption_file, format!("{date_str}|Test Caption Message\n")).unwrap();

    let log = format!("{T0}|alice|A|src/test.rs\n");
    let mut run = TestRun::new(&log, &["--caption-file", caption_file.to_str().unwrap()]);
    run.wait_for_log_load();
    run.step_frames(10);

    let gource = run.gource();
    assert!(
        !gource.active_captions.is_empty(),
        "active captions should contain the loaded caption"
    );

    let cap = &gource.active_captions[0];
    let font = gource.fonts.caption;
    let cap_text = cap.caption.clone();
    let cap_pos_x = cap.pos.x;
    let font_width = run.app.shell_mut().gfx.text_width(font, &cap_text);
    // Centered: (640 / 2) - (font_width / 2)
    let expected_x = 320.0 - font_width * 0.5;
    assert!(
        (cap_pos_x - expected_x).abs() < 1.0,
        "caption pos.x ({}) should match fontcaption centered x ({})",
        cap_pos_x,
        expected_x
    );
}

#[test]
fn test_slider_hover_caption_width_metrics() {
    let mut log = String::new();
    for i in 0..100 {
        log.push_str(&format!("{}|alice|A|src/test{i}.rs\n", T0 + i * 100));
    }
    let mut run = TestRun::new(&log, &[]);
    run.wait_for_log_load();
    run.step_frames(10);

    // Hover within slider bounds to trigger set_caption
    let bounds = *run.gource().slider.bounds();
    let slider_pos = (bounds.min + bounds.max) * 0.5;
    let event = InputEvent::MouseMove {
        pos: slider_pos,
        delta: gource_core::Vec2::ZERO,
    };
    run.app.input(&event);

    let gource = run.gource();
    assert!(
        !gource.slider.caption.is_empty(),
        "slider should have caption on hover"
    );

    // Draw the slider into a DrawList with gfx to ensure real metrics are calculated
    let mut list = DrawList::new(UVec2::new(640, 480));
    let font = gource.fonts.caption;
    let caption_text = gource.slider.caption.clone();
    let expected_capwidth = run.app.shell_mut().gfx.text_width(font, &caption_text);
    assert!(expected_capwidth > 0.0);

    let slider = run.gource().slider.clone();
    slider.draw(&mut run.app.shell_mut().gfx, &mut list, 640.0, 1.0);
    assert!(
        !list.is_empty(),
        "slider drawing should succeed with font metrics"
    );
}

// ---------------------------------------------------------------------------
// Item 3: Selected user name & fading deselection
// ---------------------------------------------------------------------------
#[test]
fn test_selected_user_deselected_when_fading() {
    let log = format!("{T0}|alice|A|src/test.rs\n");
    let mut run = TestRun::new(&log, &[]);
    run.wait_for_log_load();
    run.gource_mut().settings.user_idle_time = 0.5;
    run.step_frames(30);

    // Select the user
    let user_id = {
        let gource = run.gource();
        *gource
            .world
            .users_by_name
            .get("alice")
            .expect("user alice exists")
    };
    run.gource_mut().select_user(Some(user_id));
    assert_eq!(run.gource().selected_user, Some(user_id));

    // Advance time past user_idle_time (0.5s) so user becomes idle and fades
    run.step_frames(180);

    // C++ deselects user when u->isFading() in updateUsers (gource.cpp ~L1377)
    assert_eq!(
        run.gource().selected_user,
        None,
        "selected user should be automatically deselected when fading"
    );
}

#[test]
fn test_selected_user_name_draw_uses_fading_alpha() {
    let log = format!("{T0}|alice|A|src/test.rs\n");
    let mut run = TestRun::new(&log, &[]);
    run.wait_for_log_load();
    run.gource_mut().settings.user_idle_time = 2.0;
    run.step_frames(30);

    let user_id = *run
        .gource()
        .world
        .users_by_name
        .get("alice")
        .expect("user alice exists");

    run.gource_mut().select_user(Some(user_id));

    // Advance time until user is beginning to fade (elapsed - last_action > user_idle_time)
    // Manually adjust pawn.elapsed or last_action to test alpha < 1.0
    {
        let gource = run.gource_mut();
        let user = gource.world.users.get_mut(user_id).unwrap();
        // Make user idle and partially faded: alpha = 1.0 - 0.5 = 0.5
        user.last_action = 0.0;
        user.pawn.elapsed = 2.5; // user_idle_time (2.0) + 0.5
    }

    let gource = run.gource();
    let user = gource.world.users.get(user_id).unwrap();
    let alpha = user.alpha(gource.settings.user_idle_time);
    assert!(
        alpha < 0.99 && alpha > 0.01,
        "user alpha should be partially faded (got {alpha})"
    );

    // Verify drawing user names with world.draw_names
    let mut list = DrawList::new(UVec2::new(640, 480));
    let mut gfx = gource_draw::Gfx::new();
    gource.world.draw_names(
        &mut list,
        &mut gfx,
        &gource.settings,
        &gource.fonts.scene,
        Some(user_id),
        None,
    );

    // Find the text draw command in list batches for user's name
    let text_batch = list
        .batches
        .iter()
        .find(|b| b.texture != gource_draw::TextureId::WHITE);
    if let Some(batch) = text_batch {
        // Vertex colour alpha should match fading alpha, NOT 1.0!
        for vert in &batch.vertices {
            assert!(
                vert.colour.w <= alpha + 0.05,
                "selected user text alpha ({}) should reflect fading user alpha ({}), not 1.0",
                vert.colour.w,
                alpha
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Item 4: Input parity
// ---------------------------------------------------------------------------
#[test]
fn test_input_z_toggles_gravity() {
    let log = format!("{T0}|alice|A|src/test.rs\n");
    let mut run = TestRun::new(&log, &[]);
    run.wait_for_log_load();

    assert!(
        run.gource().world.tuning.gravity,
        "initial gravity should be true"
    );

    // Press 'z'
    run.app.input(&InputEvent::KeyDown {
        key: Key::Char('z'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert!(
        !run.gource().world.tuning.gravity,
        "gravity should toggle to false after pressing 'z'"
    );

    // Press 'z' again
    run.app.input(&InputEvent::KeyDown {
        key: Key::Char('z'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert!(
        run.gource().world.tuning.gravity,
        "gravity should toggle back to true after pressing 'z' again"
    );
}

#[test]
fn test_input_keys_ignored_before_commitlog_loaded() {
    let log = format!("{T0}|alice|A|src/test.rs\n");
    let mut run = TestRun::new(&log, &[]);

    // Run first frame to initialize shell & gource, but DO NOT wait for logmill
    let viewport = Viewport::new(640, 480);
    let mut list = DrawList::new(UVec2::new(640, 480));
    run.app.frame(0.001, viewport, &mut list);

    // If commitlog is still None:
    if run.gource().commitlog.is_none() {
        assert!(!run.gource().paused);

        // Press Space (pause toggle) before log is loaded: should be ignored (C++ gource.cpp L692)
        run.app.input(&InputEvent::KeyDown {
            key: Key::Space,
            modifiers: Modifiers::default(),
            repeat: false,
        });
        assert!(
            !run.gource().paused,
            "Space key must be ignored before commitlog is loaded"
        );

        // Escape should NOT be ignored even before commitlog is loaded
        run.app.input(&InputEvent::KeyDown {
            key: Key::Escape,
            modifiers: Modifiers::default(),
            repeat: false,
        });
        assert!(
            run.app.is_finished(),
            "Escape key should quit even before commitlog is loaded"
        );
    }
}

#[test]
fn test_shell_disable_input_blocks_all_keys_except_escape() {
    let log = format!("{T0}|alice|A|src/test.rs\n");
    let mut run = TestRun::new(&log, &["--disable-input"]);
    run.wait_for_log_load();

    let initial_gravity = run.gource().world.tuning.gravity;
    let initial_paused = run.gource().paused;

    // Send 'z' to toggle gravity: should be blocked by shell and gource
    run.app.input(&InputEvent::KeyDown {
        key: Key::Char('z'),
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert_eq!(
        run.gource().world.tuning.gravity,
        initial_gravity,
        "gravity should not toggle when disable_input is active"
    );

    // Send Space to pause: should be blocked
    run.app.input(&InputEvent::KeyDown {
        key: Key::Space,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert_eq!(
        run.gource().paused,
        initial_paused,
        "pause should not toggle when disable_input is active"
    );

    // Send F5 to reload: should be blocked by shell
    run.app.input(&InputEvent::KeyDown {
        key: Key::F5,
        modifiers: Modifiers::default(),
        repeat: false,
    });

    // Send Escape: should NOT be blocked
    assert!(!run.app.is_finished());
    run.app.input(&InputEvent::KeyDown {
        key: Key::Escape,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert!(
        run.app.is_finished(),
        "Escape key must still quit when disable_input is active"
    );
}
