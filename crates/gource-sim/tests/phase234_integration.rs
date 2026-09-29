//! Comprehensive end-to-end integration tests for Phase 2, Phase 3, and Phase 4.
//!
//! Covers:
//! - Phase 2: Git-of-Theseus & file sizing/cohorts, cohort colours, weighted layout
//! - Phase 3: Interactive timeline scrubber, checkpoint capture/thinning, seek/replay, reverse playback
//! - Phase 4: HUD analytics dashboard stack (Lines, Diff, Theseus, Editors, Commits, Churn),
//!   live tuning panel, interactive settings patches (`SettingsPatch`), and F1-F4 keyboard toggles.

use std::path::PathBuf;
use std::time::Duration;

use glam::UVec2;
use gource_draw::{DrawList, Gfx};
use gource_history::PlaybackDirection;
use gource_settings::{
    CliAction, DashboardPanel, FileColourMode, FileSizeMetric, SettingId, SettingValue,
    SettingsPatch, parse_command_line,
};
use gource_sim::app::{AppOptions, GourceApp};
use gource_sim::input::{InputEvent, Key, Modifiers};
use gource_sim::platform::Viewport;
use gource_sim::scrubber::SeekOutcome;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn load_test_app(extra_args: &[&str]) -> GourceApp {
    let log_path = repo_root().join("crates/gource-sim/tests/data/app/custom.log");
    assert!(log_path.exists(), "custom.log must exist: {:?}", log_path);

    let mut args = vec![
        "gource".to_string(),
        "--seconds-per-day".to_string(),
        "0.1".to_string(),
        log_path.to_str().unwrap().to_string(),
    ];
    for arg in extra_args {
        args.push(arg.to_string());
    }

    let CliAction::Run(config) = parse_command_line(&args[1..]).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(UVec2::new(800, 600));

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

    app
}

#[test]
fn test_phase2_git_of_theseus_and_file_sizing_and_cohorts() {
    let mut app = load_test_app(&[
        "--file-size-metric",
        "lines",
        "--file-colour-mode",
        "cohort",
    ]);
    let viewport = Viewport::new(800, 600);
    let dt = 1.0 / 60.0;

    // Advance 60 frames to process commits with lines and cohorts
    for _ in 0..60 {
        let mut list = DrawList::new(UVec2::new(800, 600));
        app.frame(dt, viewport, &mut list);
    }

    let gource = app.shell_mut().gource.as_mut().unwrap();
    assert!(!gource.world.files.is_empty(), "world should contain files");

    // Verify files have cohort data and non-zero lines
    let mut found_cohort_colour = false;
    for file in gource.world.files.values() {
        if !file.pawn.is_hidden() {
            assert!(file.target_size > 0.0);
            if file.dominant_cohort_colour.is_some() {
                found_cohort_colour = true;
            }
        }
    }
    assert!(
        found_cohort_colour,
        "files should have cohort colour assigned"
    );

    // Verify weighted directory layout computes non-zero radii
    gource.world.update_weighted_layout();
    assert!(
        gource.world.dirs[gource.world.root].radius() > 0.0,
        "root radius must be positive"
    );
}

#[test]
fn test_phase3_scrubber_checkpoints_seeking_and_reverse_playback() {
    let mut app = load_test_app(&[]);
    let viewport = Viewport::new(800, 600);
    let mut gfx = Gfx::new();
    let dt = 1.0 / 60.0;

    let gource = app.shell_mut().gource.as_mut().unwrap();

    // Advance 60 frames to record checkpoints and buffer frames
    for _ in 0..60 {
        let mut list = DrawList::new(UVec2::new(800, 600));
        gource.update(dt, viewport, &mut gfx, &mut list).unwrap();
    }

    let forward_ts = gource.currtime;
    assert!(forward_ts > 0, "simulation currtime should advance");
    assert!(
        !gource.scrubber.checkpoints.is_empty(),
        "checkpoints should be captured"
    );
    assert!(
        gource.scrubber.reverse_buffer_len() > 0,
        "reverse buffer should have recent frames"
    );

    // Toggle reverse playback using F4
    gource.input(&InputEvent::KeyDown {
        key: Key::F4,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert_eq!(
        gource.scrubber.state.playback_direction,
        PlaybackDirection::Reverse
    );

    // Step 5 frames in reverse
    for _ in 0..5 {
        let mut list = DrawList::new(UVec2::new(800, 600));
        gource.update(dt, viewport, &mut gfx, &mut list).unwrap();
    }
    assert!(
        gource.currtime <= forward_ts,
        "currtime should rewind or stay at or before forward_ts"
    );

    // Switch back to forward
    gource.input(&InputEvent::KeyDown {
        key: Key::F4,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert_eq!(
        gource.scrubber.state.playback_direction,
        PlaybackDirection::Forward
    );

    // Test seek_to_timestamp
    let target_ts = gource
        .scrubber
        .checkpoints
        .checkpoints()
        .first()
        .unwrap()
        .currtime;
    let outcome = gource
        .seek_to_timestamp(target_ts, 120, viewport, &mut gfx)
        .expect("seek succeeds");
    assert!(
        matches!(
            outcome,
            SeekOutcome::RestoredAndReplayed { .. }
                | SeekOutcome::MaterializedFromHistory { .. }
                | SeekOutcome::FallbackLegacySeek
        ),
        "valid seek outcome expected: {:?}",
        outcome
    );
    assert_eq!(gource.currtime, target_ts);
}

#[test]
fn test_phase4_dashboards_tuning_panel_and_settings_patch() {
    let mut app = load_test_app(&[]);
    let viewport = Viewport::new(800, 600);
    let mut gfx = Gfx::new();
    let dt = 1.0 / 60.0;

    let gource = app.shell_mut().gource.as_mut().unwrap();

    // Advance 30 frames to populate history
    for _ in 0..30 {
        let mut list = DrawList::new(UVec2::new(800, 600));
        gource.update(dt, viewport, &mut gfx, &mut list).unwrap();
    }

    // Toggle F1 (tuning panel), F2 (timeline bar), F3 (dashboards)
    gource.input(&InputEvent::KeyDown {
        key: Key::F1,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert!(gource.tuning_panel.is_visible());

    gource.input(&InputEvent::KeyDown {
        key: Key::F2,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert!(gource.timeline_bar.is_visible());

    // Configure all dashboard panels
    gource.settings.dashboards = vec![
        DashboardPanel::Lines,
        DashboardPanel::Diff,
        DashboardPanel::Theseus,
        DashboardPanel::Editors,
        DashboardPanel::Commits,
        DashboardPanel::Churn,
    ];
    gource.settings.hide_dashboards = false;

    // Render a frame with all widgets active
    let mut full_list = DrawList::new(UVec2::new(800, 600));
    gource
        .update(dt, viewport, &mut gfx, &mut full_list)
        .expect("update with all widgets succeeds");

    // Test live settings patch: Visual, Dynamics, and Structural
    let mut patch = SettingsPatch::new();
    patch.push(SettingId::BloomMultiplier, SettingValue::F32(2.5));
    patch.push(SettingId::TuningMinDirSize, SettingValue::F32(25.0));
    patch.push(
        SettingId::FileSizeMetric,
        SettingValue::FileSizeMetric(FileSizeMetric::Diff),
    );
    patch.push(
        SettingId::FileColourMode,
        SettingValue::FileColourMode(FileColourMode::Cohort),
    );
    patch.push(SettingId::MaxFiles, SettingValue::U32(500));

    let classes = gource.apply_settings_patch(&patch, viewport, &mut gfx);
    assert!(
        classes.contains(&gource_settings::SettingClass::Visual),
        "visual setting applied"
    );
    assert!(
        classes.contains(&gource_settings::SettingClass::Dynamics),
        "dynamics setting applied"
    );
    assert!(
        classes.contains(&gource_settings::SettingClass::Structural),
        "structural setting applied"
    );

    assert_eq!(gource.settings.bloom_multiplier, 2.5);
    assert_eq!(gource.world.tuning.min_dir_size, 25.0);
    assert_eq!(gource.settings.file_size_metric, FileSizeMetric::Diff);
    assert_eq!(gource.settings.file_colour_mode, FileColourMode::Cohort);
    assert_eq!(gource.settings.max_files, 500);

    // Render another frame after patch application
    let mut post_patch_list = DrawList::new(UVec2::new(800, 600));
    gource
        .update(dt, viewport, &mut gfx, &mut post_patch_list)
        .expect("update after settings patch succeeds");
}

#[test]
fn test_interactive_widget_hit_testing_and_toggles() {
    let mut app = load_test_app(&[]);
    let viewport = Viewport::new(800, 600);
    let mut gfx = Gfx::new();
    let dt = 1.0 / 60.0;

    let gource = app.shell_mut().gource.as_mut().unwrap();

    // Advance 30 frames
    for _ in 0..30 {
        let mut list = DrawList::new(UVec2::new(800, 600));
        gource.update(dt, viewport, &mut gfx, &mut list).unwrap();
    }

    // 1. Toggle tuning panel with F1 and hit test tabs and buttons
    gource.input(&InputEvent::KeyDown {
        key: Key::F1,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert!(gource.tuning_panel.is_visible());

    // Mouse move over tuning panel
    let panel_pos = glam::Vec2::new(50.0, 50.0);
    gource.input(&InputEvent::MouseMove {
        pos: panel_pos,
        delta: glam::Vec2::ZERO,
    });

    // Mouse click inside tuning panel
    gource.input(&InputEvent::MouseButton {
        button: gource_sim::input::MouseButton::Left,
        pressed: true,
        pos: panel_pos,
    });

    // 2. Toggle timeline bar with F2 and hit test track & direction button
    gource.input(&InputEvent::KeyDown {
        key: Key::F2,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert!(gource.timeline_bar.is_visible());

    let (t_min, t_max, t_y, _) = gource.timeline_bar.track_rect();
    let track_mid = glam::Vec2::new((t_min + t_max) * 0.5, t_y + 5.0);

    // Mouse click on timeline track to seek
    gource.input(&InputEvent::MouseButton {
        button: gource_sim::input::MouseButton::Left,
        pressed: true,
        pos: track_mid,
    });

    // Direction button click
    let dir_btn_pos = glam::Vec2::new(15.0, t_y + 5.0);
    gource.input(&InputEvent::MouseButton {
        button: gource_sim::input::MouseButton::Left,
        pressed: true,
        pos: dir_btn_pos,
    });

    // F3 toggle dashboards
    let dashboards_before = gource.settings.hide_dashboards;
    gource.input(&InputEvent::KeyDown {
        key: Key::F3,
        modifiers: Modifiers::default(),
        repeat: false,
    });
    assert_ne!(gource.settings.hide_dashboards, dashboards_before);

    // Render frame
    let mut list = DrawList::new(UVec2::new(800, 600));
    gource
        .update(dt, viewport, &mut gfx, &mut list)
        .expect("update succeeds");
}
