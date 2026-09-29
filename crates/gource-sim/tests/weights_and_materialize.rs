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
    // Under tight circle packing, visible files are packed touching each other
    let pos1 = files[fid1].dest * files[fid1].distance;
    let pos2 = files[fid2].dest * files[fid2].distance;
    let dist_12 = (pos1 - pos2).length();
    let r1 = (files[fid1].pawn.size * 0.5).max(8.0 * 0.25);
    let r2 = (files[fid2].pawn.size * 0.5).max(8.0 * 0.25);
    assert!(
        (dist_12 - (r1 + r2)).abs() < 1e-2,
        "two files should be tangent"
    );

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
fn test_tight_circle_packing_non_overlapping_and_tightness() {
    let mut files = SlotMap::with_key();
    let base_diameter = 8.0;
    let mut dir = gource_sim::dirnode::DirNode::new("/cluster/", base_diameter, 1.5);

    // Create 20 files with wildly varying sizes (radii 2.0 to 16.0)
    let radii = [
        2.0, 16.0, 4.0, 12.0, 6.0, 14.0, 3.0, 10.0, 8.0, 5.0, 15.0, 7.0, 11.0, 9.0, 13.0, 2.5, 8.5,
        6.5, 11.5, 4.5,
    ];
    let base_diameter = 8.0;

    for (i, &r) in radii.iter().enumerate() {
        let fid = files.insert(File::new(
            &format!("/cluster/file_{i}.rs"),
            Vec3::ONE,
            Vec2::ZERO,
            i as i32,
            base_diameter,
            4.0,
            false,
        ));
        files[fid].pawn.set_hidden(false);
        files[fid].pawn.size = r * 2.0;
        files[fid].radius = r;
        files[fid].target_size = r * 2.0;
        dir.files.push(fid);
    }
    dir.visible_count = radii.len();

    dir.update_weighted_file_positions(base_diameter, &mut files);

    let positions: Vec<(Vec2, f32)> = dir
        .files
        .iter()
        .map(|&fid| {
            let f = &files[fid];
            let pos = f.dest * f.distance;
            let r = (f.pawn.size * 0.5).max(base_diameter * 0.25);
            (pos, r)
        })
        .collect();

    // 1. Non-overlapping guarantee: every pair (a, b) has ||p_a - p_b|| >= (r_a + r_b) - 1e-2
    for i in 0..positions.len() {
        for j in i + 1..positions.len() {
            let (p_a, r_a) = positions[i];
            let (p_b, r_b) = positions[j];
            let dist = (p_a - p_b).length();
            assert!(
                dist >= (r_a + r_b) - 1e-2,
                "overlap detected between file {i} and {j}: dist = {dist}, r_a + r_b = {}",
                r_a + r_b
            );
        }
    }

    // 2. Tightness guarantee: every file (N >= 2) is tangent (within 1e-2) to at least one neighbor
    for (i, &(p_a, r_a)) in positions.iter().enumerate() {
        let mut min_gap = f32::INFINITY;
        for (j, &(p_b, r_b)) in positions.iter().enumerate() {
            if i == j {
                continue;
            }
            let gap = ((p_a - p_b).length() - (r_a + r_b)).abs();
            if gap < min_gap {
                min_gap = gap;
            }
        }
        assert!(
            min_gap <= 1e-2,
            "file {i} is not tangent to any neighbor: min gap = {min_gap}"
        );
    }

    // Compactness: packing efficiency sum(pi * r^2) / (pi * R_bound^2) > 0.55
    let total_file_area: f32 = radii.iter().map(|&r| std::f32::consts::PI * r * r).sum();
    let r_bound = positions
        .iter()
        .map(|(p, r)| p.length() + r)
        .fold(0.0f32, f32::max);
    let bound_area = std::f32::consts::PI * r_bound * r_bound;
    let efficiency = total_file_area / bound_area;
    assert!(
        efficiency > 0.55,
        "packing efficiency too low: {efficiency} (expected > 0.55)"
    );

    // 3. Test calc_weighted_radius encloses packed cluster
    dir.calc_weighted_radius(1.5, [], &files);
    assert!(
        dir.parent_radius >= r_bound * 1.5 - 1e-3,
        "dir.parent_radius ({}) must be at least r_bound * dir_padding ({})",
        dir.parent_radius,
        r_bound * 1.5
    );
}

#[test]
fn test_tight_circle_packing_large_directory() {
    let mut files = SlotMap::with_key();
    let base_diameter = 8.0;
    let mut dir = gource_sim::dirnode::DirNode::new("/big_cluster/", base_diameter, 1.5);

    // 60 files to exercise k > 48 branch and frontier pruning
    for i in 0..60 {
        let r = 2.0 + ((i * 7) % 15) as f32;
        let fid = files.insert(File::new(
            &format!("/big_cluster/file_{i}.rs"),
            Vec3::ONE,
            Vec2::ZERO,
            i,
            base_diameter,
            4.0,
            false,
        ));
        files[fid].pawn.set_hidden(false);
        files[fid].pawn.size = r * 2.0;
        files[fid].radius = r;
        files[fid].target_size = r * 2.0;
        dir.files.push(fid);
    }
    dir.visible_count = 60;

    dir.update_weighted_file_positions(base_diameter, &mut files);

    let positions: Vec<(Vec2, f32)> = dir
        .files
        .iter()
        .map(|&fid| {
            let f = &files[fid];
            let pos = f.dest * f.distance;
            let r = (f.pawn.size * 0.5).max(base_diameter * 0.25);
            (pos, r)
        })
        .collect();

    // Verify non-overlapping
    for i in 0..positions.len() {
        for j in i + 1..positions.len() {
            let (p_a, r_a) = positions[i];
            let (p_b, r_b) = positions[j];
            let dist = (p_a - p_b).length();
            assert!(
                dist >= (r_a + r_b) - 1e-2,
                "overlap in 60-file cluster: dist = {dist}, r_a + r_b = {}",
                r_a + r_b
            );
        }
    }
}

#[test]
fn test_directory_contact_model_resolution() {
    let mut world = World::new(42, 31);
    let d1 = world
        .dirs
        .insert(gource_sim::dirnode::DirNode::new("/src/", 8.0, 1.5));
    world.dir_map.insert("/src/".to_string(), d1);
    world.add_node_to_dir(world.root, d1);

    let d2 = world
        .dirs
        .insert(gource_sim::dirnode::DirNode::new("/tests/", 8.0, 1.5));
    world.dir_map.insert("/tests/".to_string(), d2);
    world.add_node_to_dir(world.root, d2);

    // Add files to d1 and d2 so they have non-zero radius
    let f1 = world.files.insert(File::new(
        "/src/lib.rs",
        Vec3::ONE,
        Vec2::ZERO,
        1,
        8.0,
        4.0,
        false,
    ));
    world.files[f1].pawn.set_hidden(false);
    world.files[f1].pawn.size = 20.0;
    world.files[f1].radius = 10.0;
    world.dirs[d1].files.push(f1);
    world.dirs[d1].visible_count = 1;

    let f2 = world.files.insert(File::new(
        "/tests/test.rs",
        Vec3::ONE,
        Vec2::ZERO,
        2,
        8.0,
        4.0,
        false,
    ));
    world.files[f2].pawn.set_hidden(false);
    world.files[f2].pawn.size = 20.0;
    world.files[f2].radius = 10.0;
    world.dirs[d2].files.push(f2);
    world.dirs[d2].visible_count = 1;

    // Place d1 and d2 right on top of each other
    world.dirs[d1].pos = Vec2::ZERO;
    world.dirs[d2].pos = Vec2::new(1.0, 0.0);

    world.update_weighted_layout();

    // Verify d1 and d2 are pushed apart so their dir_radius circles do not overlap
    let r1 = world.dirs[d1].dir_radius;
    let r2 = world.dirs[d2].dir_radius;
    let dist = (world.dirs[d1].pos - world.dirs[d2].pos).length();
    assert!(
        dist >= (r1 + r2) - 1e-2,
        "directories should not overlap: dist = {dist}, r1 + r2 = {}",
        r1 + r2
    );
}

#[test]
fn test_multi_directory_cluster_rapier_resolution() {
    let mut world = World::new(42, 31);
    let mut dir_ids = Vec::new();

    // Create 4 overlapping child directories around root
    for name in ["/a/", "/b/", "/c/", "/d/"] {
        let d = world
            .dirs
            .insert(gource_sim::dirnode::DirNode::new(name, 8.0, 1.5));
        world.dir_map.insert(name.to_string(), d);
        world.add_node_to_dir(world.root, d);

        // Add file
        let f = world.files.insert(File::new(
            &format!("{name}file.rs"),
            Vec3::ONE,
            Vec2::ZERO,
            1,
            8.0,
            4.0,
            false,
        ));
        world.files[f].pawn.set_hidden(false);
        world.files[f].pawn.size = 24.0;
        world.files[f].radius = 12.0;
        world.dirs[d].files.push(f);
        world.dirs[d].visible_count = 1;

        // Position all clustered tightly at origin
        world.dirs[d].pos = Vec2::new(dir_ids.len() as f32 * 0.5, (dir_ids.len() % 2) as f32 * 0.5);
        dir_ids.push(d);
    }

    world.update_weighted_layout();

    // Verify root is still at origin
    assert_eq!(world.dirs[world.root].pos, Vec2::ZERO);

    // Verify all pairwise directory circles do not overlap
    for i in 0..dir_ids.len() {
        for j in (i + 1)..dir_ids.len() {
            let id_a = dir_ids[i];
            let id_b = dir_ids[j];
            let r_a = world.dirs[id_a].dir_radius;
            let r_b = world.dirs[id_b].dir_radius;
            let dist = (world.dirs[id_a].pos - world.dirs[id_b].pos).length();
            assert!(
                dist >= (r_a + r_b) - 1e-2,
                "directories {i} and {j} overlap: dist = {dist}, r_a + r_b = {}",
                r_a + r_b
            );
        }
    }
}

#[test]
fn test_tight_circle_packing_single_file_and_empty() {
    let mut files = SlotMap::with_key();
    let mut dir = gource_sim::dirnode::DirNode::new("/single/", 8.0, 1.5);

    // Empty dir
    dir.update_weighted_file_positions(8.0, &mut files);
    assert_eq!(dir.visible_count, 0);

    // 1 visible file
    let fid = files.insert(File::new(
        "/single/one.rs",
        Vec3::ONE,
        Vec2::ZERO,
        1,
        8.0,
        4.0,
        false,
    ));
    files[fid].pawn.set_hidden(false);
    files[fid].pawn.size = 12.0;
    files[fid].radius = 6.0;
    dir.files.push(fid);
    dir.visible_count = 1;

    dir.update_weighted_file_positions(8.0, &mut files);
    assert_eq!(files[fid].dest, Vec2::ZERO);
    assert_eq!(files[fid].distance, 0.0);

    dir.calc_weighted_radius(1.5, [], &files);
    assert!(dir.dir_radius >= 6.0 * 1.5 - 1e-3);
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
