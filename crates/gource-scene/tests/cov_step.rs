//! Invariant and coverage tests for step, actions, kernel edge cases, and simulation dynamics.

use gource_model::commit::{Commit, CommitFile, FileAction};
use gource_scene::action::Action;
use gource_scene::kernel::dirs::{DirIn, DirParams};
use gource_scene::kernel::files::{self, Body, DirDisc};
use gource_scene::profile::LogicProfile;
use gource_scene::step::{TICK_DT, fx_to_world};
use gource_scene::world::World;
use gource_scene::{ActionKind, Fx, IVec2, ONE};
use gource_settings::GourceSettings;

#[test]
fn test_action_logic_and_edge_cases() {
    let mut files: slotmap::SlotMap<gource_scene::file::FileId, ()> = slotmap::SlotMap::with_key();
    let fid = files.insert(());

    let mut action = Action::new(fid, 100, 1.0, ActionKind::Create);
    assert!(!action.is_finished());

    // First logic step: needs_apply = true
    let (needs_apply, finished) = action.logic(0.1, 1);
    assert!(needs_apply);
    assert!(!finished);
    assert!(action.progress > 0.0);

    // Second logic step: needs_apply = false
    let (needs_apply, finished) = action.logic(0.1, 1);
    assert!(!needs_apply);
    assert!(!finished);

    // Finish action
    let (_, finished) = action.logic(10.0, 10);
    assert!(finished);
    assert!(action.is_finished());

    // Post-finish logic step
    let (needs_apply, finished) = action.logic(0.1, 1);
    assert!(!needs_apply);
    assert!(!finished);
}

#[test]
fn test_kernel_fixed_and_files_edge_cases() {
    // fx_to_world
    let iv = IVec2::new(5 * ONE, -3 * ONE);
    let v = fx_to_world(iv);
    assert_eq!(v, gource_core::Vec2::new(5.0, -3.0));

    // pack with 0, 1, 2, 3 circles
    let p0 = files::pack(&[]);
    assert!(p0.is_empty());

    let p1 = files::pack(&[ONE]);
    assert_eq!(p1.len(), 1);
    assert_eq!(p1[0], IVec2::ZERO);

    let p2 = files::pack(&[ONE, 2 * ONE]);
    assert_eq!(p2.len(), 2);

    let p3 = files::pack(&[ONE, ONE, ONE]);
    assert_eq!(p3.len(), 3);

    // pack with many circles to hit fallback or triangle branch
    let radii: Vec<Fx> = (1..=15).map(|i| (i * ONE / 2) as Fx).collect();
    let packed = files::pack(&radii);
    assert_eq!(packed.len(), 15);

    // animate_size edge cases
    let s1 = files::animate_size(100, 100);
    assert_eq!(s1, 100);

    let s2 = files::animate_size(100, 200); // step up
    assert!(s2 > 100);

    let s3 = files::animate_size(200, 100); // step down
    assert!(s3 < 200);

    // step_weighted
    let mut bodies = vec![
        Body {
            pos: IVec2::new(0, 0),
            vel: IVec2::new(10 * ONE, 0),
            radius: ONE,
            id: 1,
        },
        Body {
            pos: IVec2::new(ONE / 2, 0), // overlapping
            vel: IVec2::new(-10 * ONE, 0),
            radius: ONE,
            id: 2,
        },
    ];
    files::step_weighted(&mut bodies, 42, 1);
    // overlap should push them apart
    assert!(bodies[1].pos.x > bodies[0].pos.x);

    // resolve_discs with fixed discs
    let discs = vec![
        DirDisc {
            pos: IVec2::ZERO,
            radius: 10 * ONE,
            fixed: true,
            empty: false,
            id: 1,
        },
        DirDisc {
            pos: IVec2::new(5 * ONE, 0),
            radius: 10 * ONE,
            fixed: false,
            empty: false,
            id: 2,
        },
        DirDisc {
            pos: IVec2::new(-5 * ONE, 0),
            radius: 10 * ONE,
            fixed: false,
            empty: false,
            id: 3,
        },
    ];
    let resolved = files::resolve_discs(&discs, 42, 1);
    assert_eq!(resolved.len(), 3);
    assert_eq!(resolved[0], IVec2::ZERO); // fixed remains at origin
    assert!(resolved[1].x > 5 * ONE);
    assert!(resolved[2].x < -5 * ONE);

    // Two fixed discs overlapping
    let discs_both_fixed = vec![
        DirDisc {
            pos: IVec2::ZERO,
            radius: 10 * ONE,
            fixed: true,
            empty: false,
            id: 1,
        },
        DirDisc {
            pos: IVec2::new(2 * ONE, 0),
            radius: 10 * ONE,
            fixed: true,
            empty: false,
            id: 2,
        },
    ];
    let resolved_fixed = files::resolve_discs(&discs_both_fixed, 42, 1);
    assert_eq!(resolved_fixed[0], IVec2::ZERO);
    assert_eq!(resolved_fixed[1], IVec2::new(2 * ONE, 0));
}

#[test]
fn test_kernel_dirs_accels_empty_and_discs() {
    let params = DirParams {
        gravity: 5 * ONE,
        gravity_on: true,
        seed: 42,
    };

    // 0 dirs
    let frame_empty = gource_scene::kernel::dirs::DirFrame::from_preorder(Vec::new());
    let accels0 = gource_scene::kernel::dirs::dir_accels(&frame_empty, &params, 1, 1);
    assert!(accels0.is_empty());

    // Dirs where some are empty
    let dirs = vec![
        DirIn {
            id: 1,
            pos: IVec2::ZERO,
            radius: 10 * ONE,
            parent_radius: 10 * ONE,
            parent: None,
            visible: true,
            empty: false,
        },
        DirIn {
            id: 2,
            pos: IVec2::new(5 * ONE, 5 * ONE),
            radius: 10 * ONE,
            parent_radius: 10 * ONE,
            parent: Some(0),
            visible: false,
            empty: true, // empty!
        },
        DirIn {
            id: 3,
            pos: IVec2::new(-5 * ONE, -5 * ONE),
            radius: 10 * ONE,
            parent_radius: 10 * ONE,
            parent: Some(0),
            visible: true,
            empty: false,
        },
    ];

    let frame = gource_scene::kernel::dirs::DirFrame::from_preorder(dirs);
    let accels = gource_scene::kernel::dirs::dir_accels(&frame, &params, 1, 2);
    assert_eq!(accels.len(), 3);
}

#[test]
fn test_world_settle_place_user_and_weighted_simulation() {
    let mut world = World::new(777, 31);
    let settings = GourceSettings {
        user_idle_time: 0.1,
        ..Default::default()
    };

    // Test sim_dir_centre when bounds is None and when bounds has area
    assert_eq!(world.sim_dir_centre(), None);
    world.sim_dir_bounds = Some((
        IVec2::new(-10 * ONE, -10 * ONE),
        IVec2::new(10 * ONE, 10 * ONE),
    ));
    assert_eq!(world.sim_dir_centre(), Some(IVec2::ZERO));
    world.sim_dir_bounds = None;

    let uid = world.add_user("tester", &settings);
    let cf1 = CommitFile {
        filename: "/dir1/file1.txt".to_string(),
        action: FileAction::Add,
        colour: [1.0, 1.0, 1.0],
        ..Default::default()
    };
    let fid1 = world.add_file(&cf1, &settings).unwrap();

    let cf2 = CommitFile {
        filename: "/dir2/file2.txt".to_string(),
        action: FileAction::Add,
        colour: [1.0, 1.0, 1.0],
        ..Default::default()
    };
    let fid2 = world.add_file(&cf2, &settings).unwrap();

    // Settle layout
    world.settle(10);
    assert_eq!(world.tick, 10);

    // Place user near fid1
    world.place_user_near(uid, IVec2::new(10 * ONE, 20 * ONE));
    assert_ne!(world.users[uid].sim.pos, IVec2::ZERO);

    // Add remove action for fid1 to trigger action finished remove branch
    let commit = Commit {
        timestamp: 500,
        username: "tester".to_string(),
        files: vec![cf1.clone()],
        ..Default::default()
    };
    let cf_remove = CommitFile {
        filename: "/dir1/file1.txt".to_string(),
        action: FileAction::Delete,
        colour: [1.0, 0.0, 0.0],
        ..Default::default()
    };
    world.add_file_action(&commit, &cf_remove, fid1, 0.0, &settings);

    // Enable weighted mode to exercise weighted action push and contacts
    world.weighted_mode = true;
    world.resolve_directory_contacts();

    let mut profile = LogicProfile::default();
    for tick in 10..30 {
        world.begin_tick();
        world.update_sim_bounds();
        let inactives = world.update_users(tick as f32 * TICK_DT, TICK_DT, &settings);
        world.update_dirs(TICK_DT, 0.0, &mut profile);
        world.end_tick();

        // User should eventually become inactive
        if !inactives.is_empty() {
            assert_eq!(inactives[0], uid);
        }
    }

    // Now test file removal from tree and empty dir cleanup
    let was_removed = world.remove_file_from_tree(world.root, fid1, "/dir1/", true);
    assert!(was_removed);

    // Test tree file replacement if a file path is actually a directory of another file
    let cf_subdir_file = CommitFile {
        filename: "/dir2/file2.txt/inner.txt".to_string(),
        action: FileAction::Add,
        colour: [1.0, 1.0, 1.0],
        ..Default::default()
    };
    let fid3 = world.add_file(&cf_subdir_file, &settings);
    assert!(fid3.is_some());
    assert!(world.files[fid2].removing);
}
