use glam::{Vec2, Vec3};
use gource_history::{
    ChangeOp, ChangeRecord, CohortMode, History, IndexedCommit, LiveFileState, PathTable,
    TreeSnapshot, UserTable,
};
use gource_settings::{FileColourMode, FileSizeMetric, GourceSettings};
use gource_sim::file::File;
use gource_sim::world::World;
use slotmap::SlotMap;

#[test]
fn test_file_weights_and_pulse_animation() {
    let mut file = File::new("/src/engine.rs", Vec3::ONE, Vec2::ZERO, 1, 8.0, 4.0, false);
    let base_size = file.pawn.size;
    assert_eq!(file.lines, 0);
    assert_eq!(file.touch_count, 0);

    // Apply line additions with pulse
    file.apply_line_delta(Some(100), Some(10), 1.0);
    assert_eq!(file.lines, 90);
    assert_eq!(file.total_added, 100);
    assert_eq!(file.total_removed, 10);
    assert_eq!(file.pulse_delta, 90);
    assert!(file.pulse_timer > 0.0);
    assert!(file.pulse_scale > 0.0);

    // Check pulse visual
    let (ring_size, col) = file.pulse_visual().expect("active pulse");
    assert!(ring_size >= file.pawn.size);
    // Greenish for positive pulse_delta
    assert!(col.y > col.x);

    // Set weight target and verify interpolation during logic step
    file.set_weight_target(400.0, 100.0, 8.0);
    assert!(file.target_size > base_size);

    let initial_size = file.pawn.size;
    let initial_timer = file.pulse_timer;
    file.logic(0.1, 10.0);
    assert!(file.pawn.size > initial_size);
    assert!(file.pulse_timer < initial_timer);

    // Test negative pulse delta (more lines removed than added)
    file.apply_line_delta(Some(5), Some(50), 1.5);
    assert_eq!(file.lines, 45); // 90 + 5 - 50 = 45
    assert_eq!(file.pulse_delta, -45);
    let (_, col_neg) = file.pulse_visual().expect("active negative pulse");
    // Reddish for negative pulse_delta
    assert!(col_neg.x > col_neg.y);

    // Test churn ratio
    assert_eq!(file.total_added, 105);
    assert_eq!(file.total_removed, 60);
    let expected_churn = 60.0 / 165.0;
    assert!((file.churn_ratio() - expected_churn).abs() < 1e-5);
}

#[test]
fn test_file_colour_modes() {
    let mut file = File::new(
        "/src/app.rs",
        Vec3::new(0.3, 0.4, 0.5),
        Vec2::ZERO,
        1,
        8.0,
        4.0,
        false,
    );
    file.created_timestamp = 1000;
    file.touch_count = 1;
    file.last_action = 10.0;
    file.pawn.elapsed = 20.0; // lc >= 1.0, not recently touched

    // 1. Extension mode
    let c_ext = file.display_colour(FileColourMode::Extension, 10000);
    assert_eq!(c_ext, Vec3::new(0.3, 0.4, 0.5));

    // 2. Churn mode
    file.apply_line_delta(Some(100), Some(100), 0.0);
    assert!((file.churn_ratio() - 0.5).abs() < 1e-5);
    let c_churn = file.display_colour(FileColourMode::Churn, 10000);
    let cool = Vec3::new(0.2, 0.65, 0.95);
    let hot = Vec3::new(1.0, 0.25, 0.15);
    let expected_churn_col = cool.lerp(hot, 0.5);
    assert!((c_churn - expected_churn_col).length() < 1e-4);

    // 3. Age mode
    let one_year = 365 * 86400;
    let c_age = file.display_colour(FileColourMode::Age, 1000 + one_year);
    let ancient = Vec3::new(0.85, 0.4, 0.7);
    assert!((c_age - ancient).length() < 1e-4);

    // 4. Cohort mode
    let cohort_col = Vec3::new(0.8, 0.2, 0.9);
    file.dominant_cohort_colour = Some(cohort_col);
    let c_cohort = file.display_colour(FileColourMode::Cohort, 1000);
    assert_eq!(c_cohort, cohort_col);

    // 5. Selected pawn returns Vec3::ONE
    file.pawn.selected = true;
    assert_eq!(
        file.display_colour(FileColourMode::Extension, 1000),
        Vec3::ONE
    );

    // 6. Touch blend within 1 second of action
    file.pawn.selected = false;
    file.pawn.elapsed = 10.5;
    file.last_action = 10.0; // lc = 0.5
    file.touch_colour = Vec3::ZERO;
    let c_touch = file.display_colour(FileColourMode::Extension, 1000);
    assert!((c_touch - (Vec3::new(0.3, 0.4, 0.5) * 0.5)).length() < 1e-4);
}

#[test]
fn test_dirnode_weighted_radius_and_file_positions() {
    let mut files = SlotMap::with_key();
    let mut dir = gource_sim::dirnode::DirNode::new("/test/", 8.0, 1.5);

    let fid1 = files.insert(File::new(
        "/test/a.rs",
        Vec3::ONE,
        Vec2::ZERO,
        1,
        8.0,
        4.0,
        false,
    ));
    let fid2 = files.insert(File::new(
        "/test/b.rs",
        Vec3::ONE,
        Vec2::ZERO,
        2,
        8.0,
        4.0,
        false,
    ));

    dir.files.push(fid1);
    dir.files.push(fid2);
    dir.visible_count = 2;

    files[fid1].pawn.set_hidden(false);
    files[fid2].pawn.set_hidden(false);
    files[fid1].pawn.size = 16.0;
    files[fid1].radius = 8.0;
    files[fid2].pawn.size = 24.0;
    files[fid2].radius = 12.0;

    // Test calc_weighted_radius
    dir.calc_weighted_radius(1.5, [], &files);
    assert!(dir.dir_radius > 1.0);
    assert!(dir.parent_radius > 1.0);

    // Test update_weighted_file_positions
    dir.update_weighted_file_positions(8.0, &mut files);
    assert_eq!(
        files[fid1].dest,
        gource_sim::dirnode::DirNode::calc_file_dest(1, 0)
    );
    // The second file moves to ring 1 and ring advance uses max file size in ring
    assert!(files[fid2].distance > 0.0);

    // Test hidden file skipped in weighted file positions & radius
    let fid_hidden = files.insert(File::new(
        "/test/hidden.rs",
        Vec3::ONE,
        Vec2::ZERO,
        3,
        8.0,
        4.0,
        false,
    ));
    dir.files.push(fid_hidden);
    // hidden is true by default
    dir.update_weighted_file_positions(8.0, &mut files);
    assert_eq!(files[fid_hidden].dest, Vec2::ZERO);
    assert_eq!(files[fid_hidden].distance, 0.0);

    // Test calc_weighted_radius with children areas
    dir.calc_weighted_radius(1.5, [100.0, 50.0], &files);
    assert!(dir.dir_radius > 10.0);
}

#[test]
fn test_file_weights_edge_cases() {
    let mut file = File::new("/src/zero.rs", Vec3::ONE, Vec2::ZERO, 1, 8.0, 4.0, false);
    // Zero churn when total_added + total_removed == 0
    assert_eq!(file.churn_ratio(), 0.0);
    // pulse_visual is None when pulse_timer == 0 or pulse_scale == 0
    assert!(file.pulse_visual().is_none());

    // apply_line_delta with None and 0 pulse_setting
    file.apply_line_delta(None, None, 0.0);
    assert_eq!(file.lines, 0);
    assert!(file.pulse_visual().is_none());

    // apply_line_delta with pulse_setting = 0 does not trigger pulse
    file.apply_line_delta(Some(50), None, 0.0);
    assert_eq!(file.lines, 50);
    assert!(file.pulse_visual().is_none());

    // apply_line_delta removing more than total lines clamps to 0
    file.apply_line_delta(None, Some(100), 0.5);
    assert_eq!(file.lines, 0);
    assert!(file.pulse_visual().is_some());
}

#[test]
fn test_materialize_from_snapshot_and_settle() {
    let mut paths = PathTable::new();
    let path_id1 = paths.intern("/src/main.rs");
    let path_id2 = paths.intern("/src/lib.rs");
    let path_id3 = paths.intern("/docs/readme.md");

    let mut users = UserTable::new();
    let user_alice = users.intern("alice");
    let user_bob = users.intern("bob");

    let mut cohorts = gource_history::CohortTable::new();
    let cohort_2024 = cohorts.intern("2024");

    let history = History {
        paths,
        users,
        cohorts,
        cohort_mode: CohortMode::Year,
        decay_model: gource_history::ChurnDecayModel::LifoYoungestFirst,
        changes: vec![
            ChangeRecord {
                path: path_id1,
                op: ChangeOp::Add,
                lines_added: 120,
                lines_removed: 0,
                byte_size: Some(4096),
                is_binary: false,
            },
            ChangeRecord {
                path: path_id2,
                op: ChangeOp::Add,
                lines_added: 80,
                lines_removed: 0,
                byte_size: Some(2048),
                is_binary: false,
            },
            ChangeRecord {
                path: path_id3,
                op: ChangeOp::Add,
                lines_added: 30,
                lines_removed: 0,
                byte_size: Some(1024),
                is_binary: false,
            },
        ],
        commits: vec![
            IndexedCommit {
                timestamp: 1700000000,
                user: user_alice,
                cohort: cohort_2024,
                change_start: 0,
                change_len: 2,
            },
            IndexedCommit {
                timestamp: 1700001000,
                user: user_bob,
                cohort: cohort_2024,
                change_start: 2,
                change_len: 1,
            },
        ],
        metrics: Vec::new(),
        snapshots: Vec::new(),
    };

    let mut snapshot = TreeSnapshot::new();
    snapshot.commit_index = 1;
    snapshot.timestamp = 1700001000;
    snapshot.files = vec![
        LiveFileState::new(path_id1, 120, 4096, user_alice, 1700000000, cohort_2024),
        LiveFileState::new(path_id2, 80, 2048, user_alice, 1700000000, cohort_2024),
        LiveFileState::new(path_id3, 30, 1024, user_bob, 1700001000, cohort_2024),
    ];

    let mut world = World::new(42, 31);
    let settings = GourceSettings {
        file_size_metric: FileSizeMetric::Lines,
        file_colour_mode: FileColourMode::Cohort,
        ..Default::default()
    };

    world.materialize_from_snapshot(&snapshot, &history, &settings, 15);

    // Verify all 3 files materialized
    assert_eq!(world.files.len(), 3);
    assert!(world.files_by_path.contains_key("/src/main.rs"));
    assert!(world.files_by_path.contains_key("/src/lib.rs"));
    assert!(world.files_by_path.contains_key("/docs/readme.md"));

    // Verify files have updated lines and target sizes
    let main_fid = world.files_by_path["/src/main.rs"];
    assert_eq!(world.files[main_fid].lines, 120);
    assert_eq!(world.files[main_fid].byte_size, 4096);
    assert!(world.files[main_fid].target_size > world.tuning.file_diameter);
    assert!(world.files[main_fid].dominant_cohort_colour.is_some());

    // Verify users alice and bob were created and placed
    assert!(world.users_by_name.contains_key("alice"));
    assert!(world.users_by_name.contains_key("bob"));

    // Verify directory hierarchy was created and bounds computed
    assert!(!world.dir_bounds.is_empty());
    assert!(world.dir_map.contains_key("/src/"));
    assert!(world.dir_map.contains_key("/docs/"));
}
