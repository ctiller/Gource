use gource_app::world::World;
use gource_core::Vec3;
use gource_settings::GourceSettings;
use gource_vcs::commit::{Commit, CommitFile, FileAction};

/// Test that adding and touching a single file increments visible_count from 0 to 1.
#[test]
fn test_single_file_touch_increments_visible_count() {
    let mut world = World::new(12345, 54321);
    let settings = GourceSettings::default();

    let cf = CommitFile {
        filename: "/src/main.rs".to_string(),
        action: FileAction::Add,
        colour: <[f32; 3]>::from(Vec3::new(1.0, 0.0, 0.0)),
        ..Default::default()
    };

    let fid = world.add_file(&cf, &settings).expect("file added");
    let src_did = *world.dir_map.get("/src/").expect("dir exists");

    // Initially newly added file is hidden
    assert!(world.files[fid].pawn.is_hidden());
    assert_eq!(world.dirs[src_did].visible_count, 0);

    let _uid = world.add_user("alice", &settings);
    let commit = Commit {
        timestamp: 100,
        username: "alice".to_string(),
        files: vec![cf.clone()],
        ..Default::default()
    };
    world.add_file_action(&commit, &cf, fid, 1.0, &settings);

    // Run simulation to let user reach file and touch it
    for step in 0..20 {
        let t = 1.0 + (step as f32) * 0.1;
        world.update_users(t, 0.1, &settings);
    }

    assert!(!world.files[fid].pawn.is_hidden());
    assert_eq!(world.dirs[src_did].visible_count, 1);
}

/// Parity regression test: Repeated modifications to the same file should NOT
/// continuously increment visible_count. In C++ (RFile::setHidden), addVisible()
/// is only called if this->hidden == true. Once a file is visible, subsequent touches
/// must not increment visible_count.
#[test]
fn test_repeated_touch_does_not_inflate_visible_count() {
    let mut world = World::new(12345, 54321);
    let settings = GourceSettings::default();

    let cf = CommitFile {
        filename: "/src/main.rs".to_string(),
        action: FileAction::Add,
        colour: <[f32; 3]>::from(Vec3::new(1.0, 0.0, 0.0)),
        ..Default::default()
    };

    let fid = world.add_file(&cf, &settings).expect("file added");
    let src_did = *world.dir_map.get("/src/").expect("dir exists");

    let _uid = world.add_user("alice", &settings);

    // Commit 1 touches the file
    let commit1 = Commit {
        timestamp: 100,
        username: "alice".to_string(),
        files: vec![cf.clone()],
        ..Default::default()
    };
    world.add_file_action(&commit1, &cf, fid, 1.0, &settings);

    for step in 0..20 {
        let t = 1.0 + (step as f32) * 0.1;
        world.update_users(t, 0.1, &settings);
    }

    assert_eq!(
        world.dirs[src_did].visible_count, 1,
        "visible_count after 1st commit"
    );

    // Commit 2 modifies the SAME file
    let commit2 = Commit {
        timestamp: 110,
        username: "alice".to_string(),
        files: vec![cf.clone()],
        ..Default::default()
    };
    world.add_file_action(&commit2, &cf, fid, 3.0, &settings);

    for step in 0..20 {
        let t = 3.0 + (step as f32) * 0.1;
        world.update_users(t, 0.1, &settings);
    }

    // In C++, visible_count remains 1. In unpatched Rust, it becomes 2.
    assert_eq!(
        world.dirs[src_did].visible_count, 1,
        "visible_count after 2nd commit on already-visible file should still be 1"
    );
}

/// Parity regression test: Directory radius and bounds must match C++ calculations.
/// Inflating visible_count leads to ~2x radius, ~2x bounding box, and ~2x camera distance.
#[test]
fn test_dir_radius_parity_after_multiple_commits() {
    let mut world = World::new(12345, 54321);
    let settings = GourceSettings::default();

    let cf = CommitFile {
        filename: "/src/main.rs".to_string(),
        action: FileAction::Add,
        colour: <[f32; 3]>::from(Vec3::new(1.0, 0.0, 0.0)),
        ..Default::default()
    };

    let fid = world.add_file(&cf, &settings).expect("file added");
    let src_did = *world.dir_map.get("/src/").expect("dir exists");
    let _uid = world.add_user("alice", &settings);

    // Apply 5 commits to the same file
    for i in 0..5 {
        let base_t = (i * 2 + 1) as f32;
        let commit = Commit {
            timestamp: 100 + i as i64 * 10,
            username: "alice".to_string(),
            files: vec![cf.clone()],
            ..Default::default()
        };
        world.add_file_action(&commit, &cf, fid, base_t, &settings);

        for step in 0..15 {
            let t = base_t + (step as f32) * 0.1;
            world.update_users(t, 0.1, &settings);
        }
    }

    // With 1 visible file:
    // file_area = pi * 4^2 ≈ 50.2655
    // total_file_area = 50.2655 * 1
    // parent_radius = max(1.0, sqrt(total_file_area) * 1.5) ≈ 10.6347
    let expected_parent_radius = (50.265482f32.sqrt()) * 1.5;
    let actual_parent_radius = world.dirs[src_did].parent_radius;

    assert!(
        (actual_parent_radius - expected_parent_radius).abs() < 0.1,
        "parent_radius should be ~{expected_parent_radius}, got {actual_parent_radius}"
    );
}
