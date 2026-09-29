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

#[test]
fn test_scrubber_full_coverage() {
    use gource_sim::scrubber::SimScrubber;

    // Test Default trait
    let mut scrubber = SimScrubber::default();
    assert_eq!(scrubber.checkpoints.len(), 0);
    assert_eq!(scrubber.reverse_buffer_len(), 0);

    let mut app = load_test_app(&[]);
    let gource = app.shell_mut().gource.as_mut().unwrap();
    let snap = gource.snapshot();

    // Test maybe_record_checkpoint with currtime == 0 (does not record)
    scrubber.maybe_record_checkpoint(0, 1.0, || snap.clone());
    assert_eq!(scrubber.checkpoints.len(), 0);

    // Test maybe_record_checkpoint with currtime > 0 and empty checkpoints
    scrubber.maybe_record_checkpoint(100, 1.0, || snap.clone());
    assert_eq!(scrubber.checkpoints.len(), 1);
    assert_eq!(scrubber.last_checkpoint_runtime, 1.0);

    // Test maybe_record_checkpoint when runtime diff is less than threshold (should not record)
    scrubber.maybe_record_checkpoint(200, 1.2, || snap.clone());
    assert_eq!(scrubber.checkpoints.len(), 1);

    // Test maybe_record_checkpoint when runtime diff >= threshold (should record)
    scrubber.maybe_record_checkpoint(300, 2.5, || snap.clone());
    assert_eq!(scrubber.checkpoints.len(), 2);
    assert_eq!(scrubber.last_checkpoint_runtime, 2.5);

    // Test on_patch_applied with Dynamics where retained checkpoints exist (currtime <= current_ts)
    scrubber.on_patch_applied(&[gource_settings::SettingClass::Dynamics], i64::MAX);
    assert_eq!(scrubber.checkpoints.len(), 2);

    // Prune when current_ts < snapshot currtime
    scrubber.on_patch_applied(&[gource_settings::SettingClass::Timeline], -1);
    assert_eq!(scrubber.checkpoints.len(), 0);

    // Test reverse buffer capacity and clear
    scrubber.max_reverse_frames = 2;
    scrubber.push_reverse_frame(snap.clone());
    scrubber.push_reverse_frame(snap.clone());
    scrubber.push_reverse_frame(snap);
    assert_eq!(scrubber.reverse_buffer_len(), 2);
    scrubber.clear_reverse_buffer();
    assert_eq!(scrubber.reverse_buffer_len(), 0);
}

#[test]
fn test_gource_tuning_panel_and_timeline_coverage() {
    use gource_history::{MarkerKind, PlaybackDirection, TimelineMarker};
    use gource_widgets::tuning_panel::TuningTab;

    let mut app = load_test_app(&[]);
    let gource = app.shell_mut().gource.as_mut().unwrap();

    // 1. build_tuning_panel_data across all TuningTab variants
    for tab in [
        TuningTab::Visual,
        TuningTab::Dynamics,
        TuningTab::Timeline,
        TuningTab::Structural,
    ] {
        gource.tuning_tab = tab;
        let data = gource.build_tuning_panel_data();
        assert_eq!(data.active_tab, tab);
        assert!(!data.rows.is_empty());
    }

    // 2. build_timeline_bar_data with markers of all MarkerKind variants
    let _ = gource.ensure_history();
    if let Some(ref mut tl) = gource.scrubber.timeline {
        tl.markers.push(TimelineMarker {
            timestamp: 1000,
            label: "v1.0".to_string(),
            kind: MarkerKind::Tag,
        });
        tl.markers.push(TimelineMarker {
            timestamp: 1500,
            label: "Big Refactor".to_string(),
            kind: MarkerKind::Caption,
        });
        tl.markers.push(TimelineMarker {
            timestamp: 2000,
            label: "Release 2.0".to_string(),
            kind: MarkerKind::Milestone,
        });
    }

    // Test direction labels for Forward (paused/unpaused) and Reverse (paused/unpaused)
    gource.scrubber.state.playback_direction = PlaybackDirection::Forward;
    gource.paused = false;
    let data_fwd = gource.build_timeline_bar_data();
    assert!(data_fwd.markers.len() >= 3);

    gource.paused = true;
    let _ = gource.build_timeline_bar_data();

    gource.scrubber.state.playback_direction = PlaybackDirection::Reverse;
    gource.paused = false;
    let _ = gource.build_timeline_bar_data();

    gource.paused = true;
    let _ = gource.build_timeline_bar_data();

    // Test timeline_bar hover card
    gource.timeline_bar.hovered = true;
    let (t_min, t_max, _, _) = gource.timeline_bar.track_rect();
    gource.mouse_pos.x = (t_min + t_max) * 0.5;
    let data_hover = gource.build_timeline_bar_data();
    assert!(data_hover.hover_info.is_some());

    // Mouse x outside track bounds
    gource.mouse_pos.x = t_min - 100.0;
    let data_hover_outside = gource.build_timeline_bar_data();
    assert!(data_hover_outside.hover_info.is_none());
}

#[test]
fn test_gource_hit_handlers() {
    use gource_widgets::timeline_bar::TimelineHit;
    use gource_widgets::tuning_panel::{TuningHit, TuningTab};

    let mut app = load_test_app(&[]);
    let gource = app.shell_mut().gource.as_mut().unwrap();

    // 1. handle_tuning_hit on all variants
    gource.tuning_panel.show(true);
    gource.handle_tuning_hit(TuningHit::Close);
    assert!(!gource.tuning_panel.is_visible());

    gource.handle_tuning_hit(TuningHit::Tab(TuningTab::Dynamics));
    assert_eq!(gource.tuning_tab, TuningTab::Dynamics);

    // RowReset, RowSlider, RowToggle, RowCycle
    gource.handle_tuning_hit(TuningHit::RowReset { setting_index: 0 });
    gource.handle_tuning_hit(TuningHit::RowSlider {
        setting_index: 0,
        frac: 0.5,
    });
    gource.handle_tuning_hit(TuningHit::RowToggle { setting_index: 0 });
    gource.handle_tuning_hit(TuningHit::RowCycle { setting_index: 0 });

    // Test cycling all cyclable setting IDs
    for (idx, id) in SettingId::all().iter().enumerate() {
        match id {
            SettingId::CameraMode
            | SettingId::FileColourMode
            | SettingId::FileSizeMetric
            | SettingId::DashboardPeriod => {
                gource.handle_tuning_hit(TuningHit::RowCycle { setting_index: idx });
            }
            _ => {}
        }
    }

    gource.handle_tuning_hit(TuningHit::SaveConfig);
    assert!(gource.tuning_status.as_ref().unwrap().contains("Saved"));

    gource.handle_tuning_hit(TuningHit::CopyCli);
    assert!(gource.tuning_status.as_ref().unwrap().contains("Flags:"));

    gource.handle_tuning_hit(TuningHit::ResetAll);
    assert!(gource.tuning_status.as_ref().unwrap().contains("Reset"));

    gource.handle_tuning_hit(TuningHit::PanelBackground);
    gource.handle_tuning_hit(TuningHit::None);

    // 2. handle_timeline_hit on all variants
    gource.handle_timeline_hit(TimelineHit::DirectionButton);
    gource.handle_timeline_hit(TimelineHit::ClipInHandle);
    gource.handle_timeline_hit(TimelineHit::ClipOutHandle);
    gource.handle_timeline_hit(TimelineHit::Track(0.5));
    gource.handle_timeline_hit(TimelineHit::Marker(0));
    gource.handle_timeline_hit(TimelineHit::None);
}

#[test]
fn test_gource_dashboards_all_periods_and_empty_hist() {
    use gource_draw::DrawList;
    use gource_settings::{DashboardPanel, DashboardPeriod};

    let mut app = load_test_app(&[]);
    let viewport = Viewport::new(800, 600);
    let mut gfx = Gfx::new();
    let gource = app.shell_mut().gource.as_mut().unwrap();

    gource.settings.dashboards = vec![
        DashboardPanel::Lines,
        DashboardPanel::Diff,
        DashboardPanel::Theseus,
        DashboardPanel::Editors,
        DashboardPanel::Commits,
        DashboardPanel::Churn,
    ];
    gource.settings.hide_dashboards = false;

    for period in [
        DashboardPeriod::Day,
        DashboardPeriod::Week,
        DashboardPeriod::Month,
        DashboardPeriod::Year,
    ] {
        gource.settings.dashboard_period = period;
        let mut list = DrawList::new(UVec2::new(800, 600));
        gource.draw_dashboards(&mut gfx, &mut list, viewport);
    }
}

#[test]
fn test_gource_seek_and_reverse_edge_cases() {
    use gource_draw::DrawList;

    let mut app = load_test_app(&[]);
    let viewport = Viewport::new(800, 600);
    let mut gfx = Gfx::new();
    let dt = 1.0 / 60.0;
    let gource = app.shell_mut().gource.as_mut().unwrap();

    // Advance 60 frames to populate history and checkpoints
    for _ in 0..60 {
        let mut list = DrawList::new(UVec2::new(800, 600));
        gource.update(dt, viewport, &mut gfx, &mut list).unwrap();
    }

    let cp_ts = gource.scrubber.checkpoints.checkpoints()[0].currtime;

    // 1. Seek with RestoredAndReplayed: target slightly ahead of checkpoint
    let replay_target = cp_ts + 200;
    let outcome = gource.seek_to_timestamp(replay_target, 500, viewport, &mut gfx);
    assert!(outcome.is_ok());

    // 2. Seek with max_replay_ticks = 0 to trigger MaterializedFromHistory
    let outcome_mat = gource.seek_to_timestamp(cp_ts, 0, viewport, &mut gfx);
    assert!(outcome_mat.is_ok());

    // 3. Seek to -1 (before repository history) to trigger FallbackLegacySeek
    let outcome_fallback = gource.seek_to_timestamp(-1, 0, viewport, &mut gfx);
    assert_eq!(
        outcome_fallback.unwrap(),
        gource_sim::scrubber::SeekOutcome::FallbackLegacySeek
    );

    // 4. step_reverse when reverse_buffer is non-empty
    gource.scrubber.push_reverse_frame(gource.snapshot());
    assert!(gource.step_reverse(viewport, &mut gfx).unwrap());

    // 5. step_reverse when reverse_buffer is empty and currtime > 0
    gource.scrubber.clear_reverse_buffer();
    assert!(gource.currtime > 0);
    let _ = gource.step_reverse(viewport, &mut gfx);

    // 6. step_reverse when currtime == 0 returns Ok(false)
    gource.currtime = 0;
    assert!(!gource.step_reverse(viewport, &mut gfx).unwrap());
}

#[test]
fn test_persistent_scrubbing_and_caching_and_drag() {
    use glam::Vec2;
    use gource_widgets::timeline_bar::TimelineHit;

    let mut app = load_test_app(&[]);
    let viewport = Viewport::new(800, 600);
    let mut gfx = Gfx::new();
    let dt = 1.0 / 60.0;
    let gource = app.shell_mut().gource.as_mut().unwrap();

    // 1. Verify lazy pre-indexing on seekable commitlog
    assert!(!gource.history_preindexed);
    let hist = gource.ensure_history();
    assert!(gource.history_preindexed);
    assert!(!hist.is_empty());
    let initial_count = hist.commit_count();
    assert!(initial_count > 0);

    // 2. Disk caching
    let tmp_dir = tempfile::tempdir().unwrap();
    let cache_dir = tmp_dir.path().to_str().unwrap().to_string();
    gource.settings.cache_dir = cache_dir;
    gource.settings.path = "custom.log".to_string();
    gource.history_preindexed = false;
    gource.history_cache = None;

    let cached_hist = gource.ensure_history();
    assert!(gource.history_preindexed);
    assert_eq!(cached_hist.commit_count(), initial_count);

    // Verify cache file was written to disk
    let hash = gource_history::cache::fnv1a_64(gource.settings.path.as_bytes());
    let expected_file = tmp_dir
        .path()
        .join(format!("gource-history-{hash:016x}.bin"));
    assert!(expected_file.exists(), "Cache file should exist on disk");

    // Second ensure_history with cleared cache in memory should load from disk cache
    gource.history_preindexed = false;
    gource.history_cache = None;
    let loaded_hist = gource.ensure_history();
    assert!(gource.history_preindexed);
    assert_eq!(loaded_hist.commit_count(), initial_count);

    // 3. Persistent scrubbing forward and backward
    gource.handle_timeline_hit(TimelineHit::Track(1.0));
    let fwd_cursor = gource.commit_cursor;
    assert!(fwd_cursor > 1);
    assert!(gource.last_percent > 0.0);
    // User images assigned in logic()
    assert!(gource.logic(dt, viewport, &mut gfx).is_ok());
    assert!(gource.world.new_users.is_empty());

    // Scrub backward
    gource.handle_timeline_hit(TimelineHit::Track(0.0));
    let back_cursor = gource.commit_cursor;
    assert!(back_cursor < fwd_cursor);
    assert!(gource.logic(dt, viewport, &mut gfx).is_ok());

    // 4. Marker scrubbing
    gource.handle_timeline_hit(TimelineHit::Marker(0));
    assert!(gource.logic(dt, viewport, &mut gfx).is_ok());

    // 5. Timeline bar dragging and MouseMove scrubbing
    gource.timeline_bar.show(true);
    let track_rect = gource.timeline_bar.bounds;
    let click_pos = Vec2::new(
        track_rect.min.x + track_rect.width() * 0.5,
        track_rect.min.y + track_rect.height() * 0.5,
    );

    // Left click on timeline bar
    gource.input(&InputEvent::MouseButton {
        button: gource_sim::input::MouseButton::Left,
        pressed: true,
        pos: click_pos,
    });
    assert!(gource.timeline_dragging);

    // Drag move to 75%
    let drag_pos = Vec2::new(track_rect.min.x + track_rect.width() * 0.75, click_pos.y);
    gource.input(&InputEvent::MouseMove {
        pos: drag_pos,
        delta: Vec2::new(10.0, 0.0),
    });
    assert!(gource.timeline_dragging);

    // Release mouse
    gource.input(&InputEvent::MouseButton {
        button: gource_sim::input::MouseButton::Left,
        pressed: false,
        pos: drag_pos,
    });
    assert!(!gource.timeline_dragging);

    // 6. Slider persistent scrub routing when timeline bar is hidden
    gource.timeline_bar.show(false);
    gource.world.weighted_mode = true;
    let slider_pos = Vec2::new(400.0, 580.0);
    gource.input(&InputEvent::MouseButton {
        button: gource_sim::input::MouseButton::Left,
        pressed: true,
        pos: slider_pos,
    });
    gource.input(&InputEvent::MouseButton {
        button: gource_sim::input::MouseButton::Left,
        pressed: false,
        pos: slider_pos,
    });

    // 7. Verify draw hides slider when timeline_bar is visible
    gource.timeline_bar.show(true);
    let mut list = DrawList::new(UVec2::new(800, 600));
    gource.draw(dt, viewport, &mut gfx, &mut list);
}
