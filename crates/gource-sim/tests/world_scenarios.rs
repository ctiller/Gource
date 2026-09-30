use glam::{Vec2, Vec3};
use gource_draw::list::{DrawList, Material, TextureId};
use gource_draw::{Gfx, Projection};
use gource_settings::GourceSettings;
use gource_sim::world::{SceneFonts, SceneTextures, World};
use gource_vcs::commit::{Commit, CommitFile, FileAction};

#[test]
fn test_world_scenarios_and_harden() {
    let mut world = World::new(12345, 54321);
    let settings = GourceSettings {
        highlight_users: vec!["bob".to_string()],
        highlight_dirs: true,
        selection_colour: Vec3::new(1.0, 1.0, 0.0),
        highlight_colour: Vec3::new(0.0, 1.0, 1.0),
        dir_name_depth: 3,
        dir_name_position: 0.5,
        max_file_lag: 0.5,
        ..Default::default()
    };

    // 1. Root re-parenting / prefix refactoring / deep paths / deleting files & reaping empty dirs
    let cf1 = CommitFile {
        filename: "/sub1/dirA/deep/file1.txt".to_string(),
        action: FileAction::Add,
        colour: Vec3::new(0.1, 0.2, 0.3),
        ..Default::default()
    };
    let cf2 = CommitFile {
        filename: "/sub2/dirB/deep/file2.txt".to_string(),
        action: FileAction::Add,
        colour: Vec3::new(0.4, 0.5, 0.6),
        ..Default::default()
    };
    let cf3 = CommitFile {
        filename: "/sub1/dirA/deep/file3.txt".to_string(),
        action: FileAction::Add,
        colour: Vec3::new(0.7, 0.8, 0.9),
        ..Default::default()
    };

    let fid1 = world.add_file(&cf1, &settings).expect("file 1 added");
    let fid2 = world.add_file(&cf2, &settings).expect("file 2 added");
    let fid3 = world.add_file(&cf3, &settings).expect("file 3 added");

    // Root should be "/"
    assert_eq!(world.dirs[world.root].path(), "/");
    assert!(world.dirs[world.root].children.len() >= 2);

    // 2. Users with actions over time (queues, beams, idle/reap)
    let uid_alice = world.add_user("alice", &settings);
    let uid_bob = world.add_user("bob", &settings); // highlighted

    let commit_alice = Commit {
        timestamp: 100,
        username: "alice".to_string(),
        files: vec![cf1.clone(), cf2.clone()],
        ..Default::default()
    };
    world.add_file_action(&commit_alice, &cf1, fid1, 1.0, &settings);
    world.add_file_action(&commit_alice, &cf2, fid2, 1.0, &settings);

    let commit_bob = Commit {
        timestamp: 105,
        username: "bob".to_string(),
        files: vec![cf3.clone()],
        ..Default::default()
    };
    world.add_file_action(&commit_bob, &cf3, fid3, 1.0, &settings);

    // Initial action count
    assert_eq!(world.users[uid_alice].action_count(), 2);
    assert_eq!(world.users[uid_bob].action_count(), 1);

    // Make dirs visible
    for did in world.dir_map.values().copied().collect::<Vec<_>>() {
        world.dirs[did].visible = true;
    }

    // Run simulation steps to trigger action execution, touch files, update bounds
    for step in 0..25 {
        let t = 2.0 + (step as f32) * 0.2;
        world.update_users(t, 0.25, &settings);
        world.update_bounds();
        world.interact_users();
        world.interact_dirs();
        world.update_dirs(0.1, 0.1, 0.0);
    }

    // Now actions should be executed / files touched
    assert!(world.files[fid1].touch_colour.y > 0.0);
    assert!(!world.files[fid1].pawn.is_hidden());

    // 3. QuadTree picking / spatial tree verification
    assert!(world.user_tree.is_some());
    assert!(world.dir_tree.is_some());
    let u_tree = world.user_tree.as_ref().unwrap();
    let d_tree = world.dir_tree.as_ref().unwrap();
    let picked_users = u_tree.items_in_bounds(&world.user_bounds);
    assert!(!picked_users.is_empty());
    let picked_dirs = d_tree.items_in_bounds(&world.dir_bounds);
    assert!(!picked_dirs.is_empty());

    // 4. prepare_frame + draw_scene + draw_names
    let proj = Projection::new(Vec3::new(0.0, 0.0, -1000.0), Vec2::new(1920.0, 1080.0));
    world.prepare_frame(&proj, &settings);

    let textures = SceneTextures {
        file: TextureId(10),
        beam: TextureId(20),
        default_user: TextureId(30),
    };

    let mut list = DrawList::new(glam::UVec2::new(1920, 1080));
    world.draw_scene(&mut list, &proj, &settings, &textures);
    assert!(!list.is_empty());

    // Check material batches: Alpha and Bloom
    let has_alpha = list.batches.iter().any(|b| b.material == Material::Alpha);
    let has_bloom = list.batches.iter().any(|b| b.material == Material::Bloom);
    assert!(has_alpha);
    assert!(has_bloom);

    // Draw names
    let mut gfx = Gfx::new();
    let default_face = gfx.fonts.default_face();
    let fonts = SceneFonts {
        file: gfx.fonts.font(default_face, 12),
        file_selected: gfx.fonts.font(default_face, 18),
        user: gfx.fonts.font(default_face, 12),
        user_selected: gfx.fonts.font(default_face, 18),
        dir: gfx.fonts.font(default_face, 12),
    };

    let mut name_list = DrawList::new(glam::UVec2::new(1920, 1080));
    world.draw_names(
        &mut name_list,
        &mut gfx,
        &settings,
        &fonts,
        Some(uid_alice),
        Some(fid1),
    );
    assert!(!name_list.is_empty());

    // 5. Change colours
    let orig_seed = world.hasher.seed;
    world.change_colours(orig_seed + 999);
    assert_eq!(world.hasher.seed, orig_seed + 999);

    let del1 = world.delete_file(fid1);
    assert!(del1.is_some());
    let del3 = world.delete_file(fid3);
    assert!(del3.is_some());

    // When both files in /sub1/dirA/deep/ are deleted, the directory becomes empty and is reaped:
    assert!(!world.dir_map.contains_key("/sub1/dirA/deep/"));

    // Delete remaining file
    let del2 = world.delete_file(fid2);
    assert!(del2.is_some());
    assert!(!world.dir_map.contains_key("/sub2/dirB/deep/"));
}
