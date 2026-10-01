use glam::{Vec2, Vec3};
use gource_history::{
    ChangeOp, ChangeRecord, CohortMode, History, IndexedCommit, LiveFileState, PathTable,
    TreeSnapshot, UserTable,
};
use gource_scene::files::pack;
use gource_scene::{Fx, ONE};
use gource_settings::{FileColourMode, FileSizeMetric, GourceSettings};
use gource_sim::file::File;
use gource_sim::view::{from_fx, to_fx};
use gource_sim::world::World;

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

    // Set weight target and verify target_size method increases
    file.set_weight_target(400, 100, 8.0);
    assert!(file.target_size() > base_size);

    let initial_timer = file.pulse_timer;
    file.pawn.set_hidden(false);
    file.logic(0.1, 10.0);
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
fn test_dirnode_weighted_radius_and_packing() {
    let mut world = World::new(42, 31);
    let settings = GourceSettings {
        file_size_metric: FileSizeMetric::Lines,
        ..Default::default()
    };
    world.weighted_mode = true;

    let f1 = world
        .add_file(
            &gource_vcs::CommitFile {
                filename: "/test/a.rs".to_string(),
                action: gource_vcs::FileAction::Add,
                colour: Vec3::ONE,
                lines_added: Some(100),
                ..Default::default()
            },
            &settings,
        )
        .unwrap();

    let f2 = world
        .add_file(
            &gource_vcs::CommitFile {
                filename: "/test/b.rs".to_string(),
                action: gource_vcs::FileAction::Add,
                colour: Vec3::ONE,
                lines_added: Some(400),
                ..Default::default()
            },
            &settings,
        )
        .unwrap();

    let dir_id = world.dir_map["/test/"];

    world.files[f1].pawn.set_hidden(false);
    world.files[f2].pawn.set_hidden(false);

    world.files[f1].set_weight_target(100, 100, world.tuning.file_diameter);
    world.files[f1].sim.size = world.files[f1].sim.target_size;
    world.files[f1].sim.radius = world.files[f1].sim.target_size / 2;

    world.files[f2].set_weight_target(400, 100, world.tuning.file_diameter);
    world.files[f2].sim.size = world.files[f2].sim.target_size;
    world.files[f2].sim.radius = world.files[f2].sim.target_size / 2;

    // Pack files and update weighted radii
    world.update_weighted_layout();
    world.sync_view(1.0);

    let dir = &world.dirs[dir_id];
    assert!(dir.dir_radius > 1.0);
    assert!(dir.parent_radius > 1.0);

    let p1 = world.files[f1].sim.pos;
    let p2 = world.files[f2].sim.pos;
    let dist = (p1 - p2).length();
    let min_dist = world.files[f1].sim.radius + world.files[f2].sim.radius;
    // With integer packing, circles do not overlap (allowing 1 unit tolerance)
    assert!(
        dist >= min_dist - 1,
        "two packed files must not overlap: dist {dist} vs min_dist {min_dist}"
    );
}

#[test]
fn test_tight_circle_packing_non_overlapping_and_tightness() {
    // 20 files with varying radii in Q8
    let radii_floats = [
        2.0, 16.0, 4.0, 12.0, 6.0, 14.0, 3.0, 10.0, 8.0, 5.0, 15.0, 7.0, 11.0, 9.0, 13.0, 2.5, 8.5,
        6.5, 11.5, 4.5,
    ];
    let radii: Vec<Fx> = radii_floats.iter().map(|&r| to_fx(r)).collect();

    let placed = pack(&radii);
    assert_eq!(placed.len(), radii.len());

    // 1. Non-overlapping guarantee: every pair (i, j) satisfies dist >= r_i + r_j - 1
    for i in 0..placed.len() {
        for j in i + 1..placed.len() {
            let dist = (placed[i] - placed[j]).length();
            let min_dist = radii[i] + radii[j];
            assert!(
                dist >= min_dist - 1,
                "overlap detected between {i} and {j}: dist = {dist}, min_dist = {min_dist}"
            );
        }
    }

    // 2. Tightness guarantee: each circle is tangent (within 2 Q8 units) to at least one neighbor
    for i in 0..placed.len() {
        let mut min_gap = i32::MAX;
        for j in 0..placed.len() {
            if i == j {
                continue;
            }
            let dist = (placed[i] - placed[j]).length();
            let gap = (dist - (radii[i] + radii[j])).abs();
            if gap < min_gap {
                min_gap = gap;
            }
        }
        assert!(
            min_gap <= 2,
            "circle {i} is not tangent to any neighbor: min gap = {min_gap}"
        );
    }

    // 3. Compactness: packing efficiency sum(pi * r^2) / (pi * R_bound^2) > 0.45
    let total_file_area: f32 = radii_floats
        .iter()
        .map(|&r| std::f32::consts::PI * r * r)
        .sum();
    let r_bound: f32 = placed
        .iter()
        .zip(&radii)
        .map(|(p, &r)| from_fx(p.length() + r))
        .fold(0.0f32, f32::max);
    let bound_area = std::f32::consts::PI * r_bound * r_bound;
    let efficiency = total_file_area / bound_area;
    assert!(
        efficiency > 0.45,
        "packing efficiency too low: {efficiency} (expected > 0.45)"
    );
}

#[test]
fn test_tight_circle_packing_large_directory() {
    // 60 files to exercise k > 48 frontier pruning
    let radii: Vec<Fx> = (0..60)
        .map(|i| to_fx(2.0 + ((i * 7) % 15) as f32))
        .collect();

    let placed = pack(&radii);
    assert_eq!(placed.len(), 60);

    // Verify non-overlapping
    for i in 0..placed.len() {
        for j in i + 1..placed.len() {
            let dist = (placed[i] - placed[j]).length();
            let min_dist = radii[i] + radii[j];
            assert!(
                dist >= min_dist - 1,
                "overlap in 60-file cluster between {i} and {j}: dist = {dist}, min_dist = {min_dist}"
            );
        }
    }
}

#[test]
fn test_directory_contact_model_resolution() {
    let mut world = World::new(42, 31);
    let d1 = world.dirs.insert(gource_sim::dirnode::DirNode::new(
        "/src/",
        world.params.file_area,
        world.params.padding,
    ));
    world.dir_map.insert("/src/".to_string(), d1);
    world.add_node_to_dir(world.root, d1);

    let d2 = world.dirs.insert(gource_sim::dirnode::DirNode::new(
        "/tests/",
        world.params.file_area,
        world.params.padding,
    ));
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
    world.files[f1].sim.size = 20 * ONE;
    world.files[f1].sim.radius = 10 * ONE;
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
    world.files[f2].sim.size = 20 * ONE;
    world.files[f2].sim.radius = 10 * ONE;
    world.dirs[d2].files.push(f2);
    world.dirs[d2].visible_count = 1;

    // Place d1 and d2 right on top of each other
    world.dirs[d1].place(gource_scene::IVec2::ZERO);
    world.dirs[d2].place(gource_scene::IVec2::new(ONE, 0));

    world.update_weighted_layout();
    world.sync_view(1.0);

    // Verify d1 and d2 are pushed apart so their parent_radius circles do not overlap
    let r1 = world.dirs[d1].parent_radius;
    let r2 = world.dirs[d2].parent_radius;
    let dist = (world.dirs[d1].pos - world.dirs[d2].pos).length();
    assert!(
        dist >= (r1 + r2) - 1e-2,
        "directories should not overlap: dist = {dist}, r1 + r2 = {}",
        r1 + r2
    );
}

#[test]
fn test_multi_directory_cluster_resolution() {
    let mut world = World::new(42, 31);
    let mut dir_ids = Vec::new();

    // Create 4 overlapping child directories around root
    for name in ["/a/", "/b/", "/c/", "/d/"] {
        let d = world.dirs.insert(gource_sim::dirnode::DirNode::new(
            name,
            world.params.file_area,
            world.params.padding,
        ));
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
        world.files[f].sim.size = 24 * ONE;
        world.files[f].sim.radius = 12 * ONE;
        world.dirs[d].files.push(f);
        world.dirs[d].visible_count = 1;

        // Position all clustered tightly at origin
        let p = gource_scene::IVec2::new(
            (dir_ids.len() as i32 * ONE) / 2,
            ((dir_ids.len() % 2) as i32 * ONE) / 2,
        );
        world.dirs[d].place(p);
        dir_ids.push(d);
    }

    world.update_weighted_layout();
    world.sync_view(1.0);

    // Verify root is still at origin
    assert_eq!(world.dirs[world.root].pos, Vec2::ZERO);

    // Verify all pairwise directory circles do not overlap their parent_radius
    for i in 0..dir_ids.len() {
        for j in (i + 1)..dir_ids.len() {
            let id_a = dir_ids[i];
            let id_b = dir_ids[j];
            let r_a = world.dirs[id_a].parent_radius.max(10.0);
            let r_b = world.dirs[id_b].parent_radius.max(10.0);
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
    // Empty
    let empty: Vec<Fx> = Vec::new();
    assert!(pack(&empty).is_empty());

    // 1 circle placed at origin
    let single = vec![to_fx(6.0)];
    let placed = pack(&single);
    assert_eq!(placed, vec![gource_scene::IVec2::ZERO]);
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
        cached_cohort_0_half_life: None,
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
    assert!(world.files[main_fid].target_size() > world.tuning.file_diameter);
    assert!(world.files[main_fid].dominant_cohort_colour.is_some());

    // Verify users alice and bob were created and placed
    assert!(world.users_by_name.contains_key("alice"));
    assert!(world.users_by_name.contains_key("bob"));

    // Verify directory hierarchy was created and bounds computed
    assert!(!world.dir_bounds.is_empty());
    assert!(world.dir_map.contains_key("/src/"));
    assert!(world.dir_map.contains_key("/docs/"));
}

#[test]
fn test_no_jitter_on_file_edit_and_smooth_swirl() {
    let mut world = World::new(42, 31);
    let settings = GourceSettings {
        file_size_metric: FileSizeMetric::Lines,
        ..Default::default()
    };
    world.weighted_mode = true;

    // Add 10 files to a directory
    let mut fids = Vec::new();
    for i in 0..10 {
        let path = format!("/src/file_{i}.rs");
        let fid = world
            .add_file(
                &gource_vcs::CommitFile {
                    filename: path,
                    colour: Vec3::ONE,
                    action: gource_vcs::FileAction::Add,
                    lines_added: Some(50 + (i * 20) as u32),
                    lines_removed: None,
                    is_binary: false,
                    ..Default::default()
                },
                &settings,
            )
            .unwrap();
        world.files[fid].pawn.set_hidden(false);
        world.files[fid].apply_line_delta(Some(50 + (i * 20) as u32), None, 0.0);
        world.files[fid].set_weight_target(50 + i * 20, 100, 8.0);
        fids.push(fid);
    }

    // Settle layout
    world.update_weighted_layout();
    for _ in 0..30 {
        world.begin_tick();
        world.update_sim_bounds();
        let preorder = world.preorder_dirs();
        world.update_weighted_layout_step(&preorder);
        world.end_tick();
    }
    world.sync_view(1.0);

    // Now edit one file in the directory
    let edited_fid = fids[3];
    world.files[edited_fid].apply_line_delta(Some(500), None, 0.0);
    world.files[edited_fid].set_weight_target(600, 100, 8.0);

    // Step 10 ticks and verify per-tick position delta in view is small (< 5.0 px)
    for _ in 0..10 {
        let prev_positions: Vec<Vec2> = fids.iter().map(|&fid| world.files[fid].pawn.pos).collect();
        world.begin_tick();
        world.update_sim_bounds();
        let preorder = world.preorder_dirs();
        world.update_weighted_layout_step(&preorder);
        world.end_tick();
        world.sync_view(1.0);

        for (idx, &fid) in fids.iter().enumerate() {
            let delta = (world.files[fid].pawn.pos - prev_positions[idx]).length();
            assert!(
                delta < 5.0,
                "file {idx} jumped by {delta} px (expected smooth swirl < 5.0 px)"
            );
        }
    }

    // Test adding a new file starts near zero size and grows smoothly
    let new_fid = world
        .add_file(
            &gource_vcs::CommitFile {
                filename: "/src/new_file.rs".to_string(),
                colour: Vec3::ONE,
                action: gource_vcs::FileAction::Add,
                lines_added: Some(100),
                lines_removed: None,
                is_binary: false,
                ..Default::default()
            },
            &settings,
        )
        .unwrap();
    assert!(
        world.files[new_fid].pawn.size <= 0.2,
        "new file should start near zero size"
    );
    world.files[new_fid].pawn.set_hidden(false);
    let initial_size = world.files[new_fid].sim.size;
    let target = world.files[new_fid].size_goal();
    let next_size = gource_scene::files::animate_size(initial_size, target);
    assert!(next_size > initial_size, "new file should grow smoothly");

    // Test removing a file shrinks its size toward zero
    let removed_fid = fids[0];
    world.files[removed_fid].removing = true;
    let size_before = world.files[removed_fid].sim.size;
    let target_remove = world.files[removed_fid].size_goal();
    assert_eq!(target_remove, 0);
    let size_after = gource_scene::files::animate_size(size_before, target_remove);
    assert!(
        size_after < size_before,
        "removing file should shrink smoothly toward zero"
    );
}

#[test]
fn test_laser_touch_applies_push_force() {
    let mut world = World::new(42, 31);
    let settings = GourceSettings {
        file_size_metric: FileSizeMetric::Lines,
        ..Default::default()
    };
    world.weighted_mode = true;

    let uid = world.add_user("alice", &settings);
    let cf = gource_vcs::CommitFile {
        filename: "/src/laser_target.rs".to_string(),
        action: gource_vcs::FileAction::Modify,
        colour: Vec3::ONE,
        lines_added: Some(10),
        lines_removed: None,
        is_binary: false,
        ..Default::default()
    };
    let fid = world.add_file(&cf, &settings).expect("file added");
    world.files[fid].pawn.set_hidden(false);

    // Place user at (-50.0, 0.0) and file at (0.0, 0.0)
    world.users[uid].place(gource_scene::IVec2::new(-50 * ONE, 0));
    let dir_id = world.files[fid].dir.unwrap();
    world.dirs[dir_id].place(gource_scene::IVec2::ZERO);
    world.files[fid].sim.pos = gource_scene::IVec2::ZERO;
    world.files[fid].sim.vel = gource_scene::IVec2::ZERO;

    let commit = gource_vcs::Commit {
        timestamp: 1000,
        username: "alice".to_string(),
        files: vec![cf.clone()],
        ..Default::default()
    };
    world.add_file_action(&commit, &cf, fid, 1.0, &settings);

    // Initial velocity should be zero
    assert_eq!(world.files[fid].sim.vel, gource_scene::IVec2::ZERO);

    // Run update_users to trigger the laser touch action
    world.update_users(1.0, 0.25, &settings);

    // File should have received an impulse away from the author (i.e. positive X direction)
    assert!(
        world.files[fid].sim.vel.x > 0,
        "expected file velocity x > 0 away from user, got {:?}",
        world.files[fid].sim.vel
    );
}
