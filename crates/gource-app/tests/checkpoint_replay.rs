//! Deterministic SimSnapshot checkpointing, restore, and replay-equivalence tests (Phase 1C).

use gource_app::app::{AppOptions, GourceApp};
use gource_app::checkpoint::CheckpointStore;
use gource_app::platform::Viewport;
use gource_draw::{DrawList, Gfx};
use gource_settings::{CliAction, parse_command_line};
use std::path::PathBuf;
use std::time::Duration;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn wait_for_app_load(app: &mut GourceApp) {
    let viewport = Viewport::new(800, 600);
    let mut list = DrawList::new(glam::UVec2::new(800, 600));

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
}

#[test]
fn test_checkpoint_store_basic_and_thinning() {
    let log_path = repo_root().join("crates/gource-app/tests/data/app/custom.log");
    let args = [
        "gource".to_string(),
        "--seconds-per-day".to_string(),
        "1.0".to_string(),
        log_path.to_str().unwrap().to_string(),
    ];
    let CliAction::Run(config) = parse_command_line(&args[1..]).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    wait_for_app_load(&mut app);

    let gource = app.shell_mut().gource.as_mut().unwrap();

    let mut store = CheckpointStore::new(4);
    assert!(store.is_empty());
    assert_eq!(store.len(), 0);

    // Create snapshots with distinct currtime and runtime
    let mut snap1 = gource.snapshot();
    snap1.currtime = 100;
    snap1.runtime = 1.0;
    store.insert(snap1);

    let mut snap2 = gource.snapshot();
    snap2.currtime = 200;
    snap2.runtime = 2.0;
    store.insert(snap2);

    let mut snap3 = gource.snapshot();
    snap3.currtime = 300;
    snap3.runtime = 3.0;
    store.insert(snap3);

    let mut snap4 = gource.snapshot();
    snap4.currtime = 400;
    snap4.runtime = 4.0;
    store.insert(snap4);

    assert_eq!(store.len(), 4);
    assert!(!store.is_empty());

    // Inserting a 5th element triggers thinning:
    // older half of 4 items is indices 0, 1 (times 100, 200).
    // keep index 0 (100), drop index 1 (200). Newer half (300, 400) kept.
    // Resulting list before insert: [100, 300, 400], then append 500 -> 4 items.
    let mut snap5 = gource.snapshot();
    snap5.currtime = 500;
    snap5.runtime = 5.0;
    store.insert(snap5);

    assert_eq!(store.len(), 4);
    let times: Vec<i64> = store.checkpoints().iter().map(|s| s.currtime).collect();
    assert_eq!(times, vec![100, 300, 400, 500]);

    // Query nearest before time
    assert!(store.nearest_before_time(50).is_none());
    assert_eq!(
        store.nearest_before_time(100).map(|s| s.currtime),
        Some(100)
    );
    assert_eq!(
        store.nearest_before_time(250).map(|s| s.currtime),
        Some(100)
    );
    assert_eq!(
        store.nearest_before_time(350).map(|s| s.currtime),
        Some(300)
    );
    assert_eq!(
        store.nearest_before_time(500).map(|s| s.currtime),
        Some(500)
    );
    assert_eq!(
        store.nearest_before_time(999).map(|s| s.currtime),
        Some(500)
    );

    // Query nearest before runtime
    assert!(store.nearest_before_runtime(0.5).is_none());
    assert_eq!(
        store.nearest_before_runtime(1.0).map(|s| s.runtime),
        Some(1.0)
    );
    assert_eq!(
        store.nearest_before_runtime(3.5).map(|s| s.runtime),
        Some(3.0)
    );
    assert_eq!(
        store.nearest_before_runtime(10.0).map(|s| s.runtime),
        Some(5.0)
    );

    // Clear
    store.clear();
    assert!(store.is_empty());
    assert_eq!(store.len(), 0);

    // Thin on small store (< 2)
    store.thin();
    assert!(store.is_empty());
}

#[test]
fn test_change_colours_determinism() {
    let log_path = repo_root().join("crates/gource-app/tests/data/app/custom.log");
    let args = [
        "gource".to_string(),
        "--seconds-per-day".to_string(),
        "1.0".to_string(),
        log_path.to_str().unwrap().to_string(),
    ];
    let CliAction::Run(config) = parse_command_line(&args[1..]).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    wait_for_app_load(&mut app);

    let gource = app.shell_mut().gource.as_mut().unwrap();

    // Advance a few commits to have files and users
    let viewport = Viewport::new(800, 600);
    let mut gfx = Gfx::new();
    let mut list = DrawList::new(glam::UVec2::new(800, 600));

    for _ in 0..10 {
        let _ = gource.update(1.0 / 60.0, viewport, &mut gfx, &mut list);
    }

    // Save snapshot before colour change
    let snapshot = gource.snapshot();

    // Run colour changes
    gource.change_colours();
    let seed1 = gource.world.hasher.seed;
    let hash1 = gource.world.state_hash();

    // Restore snapshot and recolour again
    gource.restore(&snapshot);
    gource.change_colours();
    let seed2 = gource.world.hasher.seed;
    let hash2 = gource.world.state_hash();

    assert_eq!(seed1, seed2, "change_colours must be fully deterministic");
    assert_eq!(hash1, hash2, "simulation state must match bit-for-bit");
}

#[test]
fn test_snapshot_restore_and_replay_equivalence_120_frames() {
    let log_path = repo_root().join("crates/gource-app/tests/data/app/gource-custom.log");
    assert!(log_path.exists(), "test log must exist: {:?}", log_path);

    let args = [
        "gource".to_string(),
        "--seconds-per-day".to_string(),
        "0.1".to_string(),
        log_path.to_str().unwrap().to_string(),
    ];
    let CliAction::Run(config) = parse_command_line(&args[1..]).unwrap() else {
        panic!("expected run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).unwrap();
    wait_for_app_load(&mut app);

    let gource = app.shell_mut().gource.as_mut().unwrap();

    let viewport = Viewport::new(800, 600);
    let mut gfx = Gfx::new();
    let dt = 1.0 / 60.0;

    let mut snap30 = None;
    let mut snap60 = None;
    let mut baseline_draw_lists = Vec::new();

    // Step frames 0..120
    for frame in 0..120 {
        let mut list = DrawList::new(glam::UVec2::new(800, 600));
        gource
            .update(dt, viewport, &mut gfx, &mut list)
            .expect("update succeeds");

        if frame == 29 {
            // frame 30 snapshot (0-indexed frame 29)
            snap30 = Some(gource.snapshot());
        }
        if frame == 59 {
            // frame 60 snapshot
            snap60 = Some(gource.snapshot());
        }
        if frame >= 60 {
            baseline_draw_lists.push(list);
        }
    }

    assert!(snap30.is_some());
    assert!(snap60.is_some());
    assert_eq!(baseline_draw_lists.len(), 60); // frames 60..119 inclusive

    // Snapshot at end of 120 frames
    let final_world_baseline = gource.world.clone();
    let final_cursor_baseline = gource.commit_cursor;
    let final_runtime_baseline = gource.runtime;

    // Now restore snapshot 30, and run 90 frames to reach frame 120!
    let snap30 = snap30.unwrap();
    gource.restore(&snap30);

    assert_eq!(gource.commit_cursor, snap30.commit_cursor);
    assert_eq!(gource.framecount, snap30.framecount);
    assert_eq!(gource.runtime, snap30.runtime);

    let mut replayed_draw_lists = Vec::new();

    for frame in 30..120 {
        let mut list = DrawList::new(glam::UVec2::new(800, 600));
        gource
            .update(dt, viewport, &mut gfx, &mut list)
            .expect("update succeeds");

        if frame >= 60 {
            replayed_draw_lists.push(list);
        }
    }

    assert_eq!(replayed_draw_lists.len(), 60);

    // Assert bit-for-bit identical scalar states
    assert_eq!(gource.commit_cursor, final_cursor_baseline);
    assert_eq!(gource.runtime, final_runtime_baseline);
    assert_eq!(gource.world.users.len(), final_world_baseline.users.len());
    assert_eq!(gource.world.files.len(), final_world_baseline.files.len());
    assert_eq!(gource.world.dirs.len(), final_world_baseline.dirs.len());

    // Compare all DrawLists from frame 60..120 bit-for-bit
    for i in 0..60 {
        assert_eq!(
            replayed_draw_lists[i],
            baseline_draw_lists[i],
            "DrawList mismatch at frame {}",
            60 + i
        );
    }

    // Now restore frame 60 snapshot directly and run 60 frames to reach frame 120
    let snap60 = snap60.unwrap();
    gource.restore(&snap60);

    let mut replayed_from_60 = Vec::new();
    for _ in 60..120 {
        let mut list = DrawList::new(glam::UVec2::new(800, 600));
        gource
            .update(dt, viewport, &mut gfx, &mut list)
            .expect("update succeeds");
        replayed_from_60.push(list);
    }

    for i in 0..60 {
        assert_eq!(
            replayed_from_60[i],
            baseline_draw_lists[i],
            "DrawList mismatch when replaying from frame 60 at frame {}",
            60 + i
        );
    }
}
