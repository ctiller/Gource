use glam::{Vec2, Vec3};
use gource_draw::Projection;
use gource_draw::list::{DrawList, TextureId};
use gource_settings::GourceSettings;
use gource_sim::action::{Action, ActionKind};
use gource_sim::dirnode::DirNode;
use gource_sim::file::{File, FileId};
use gource_sim::user::User;
use gource_sim::world::{SceneTextures, World};
use gource_vcs::commit::{Commit, CommitFile, FileAction};
use slotmap::SlotMap;

#[test]
fn test_action_completed_coverage() {
    let mut sm: SlotMap<FileId, ()> = SlotMap::with_key();
    let fid = sm.insert(());
    let mut act = Action::new(fid, 100, 1.0, ActionKind::Create);
    // Finish action
    let _ = act.logic(10.0, 1);
    assert!(act.is_finished());

    // Calling logic on an already finished action hits action.rs:59 -> (false, false)
    let (needs_apply, finished_now) = act.logic(1.0, 1);
    assert!(!needs_apply);
    assert!(!finished_now);
}

#[test]
fn test_file_uncovered_branches() {
    let mut file = File::new(
        "no_ext_file",
        Vec3::new(0.5, 0.5, 0.5),
        Vec2::ZERO,
        1,
        8.0,
        5.0,
        false, // file_extension_fallback false -> ext is empty
    );
    assert_eq!(file.ext, "");

    // colourize() when ext is empty hits file.rs:141 -> Vec3::ONE
    let hasher = gource_core::StringHasher::default();
    file.colourize(&hasher);
    assert_eq!(file.file_colour, Vec3::ONE);

    // colour() when elapsed - last_action >= 1.0 hits file.rs:163 -> self.file_colour
    file.pawn.elapsed = 10.0;
    file.last_action = 5.0; // 5.0 >= 1.0
    assert_eq!(file.colour(), file.file_colour);
}

#[test]
fn test_user_uncovered_branches() {
    let hasher = gource_core::StringHasher::default();
    let mut user = User::new("user_branches", Vec2::ZERO, 1, 200.0, 1.0, &hasher);

    let mut sm: SlotMap<FileId, ()> = SlotMap::with_key();
    let f1 = sm.insert(());
    let f2 = sm.insert(());

    // user.rs:184 personal_space_dist * 0.5 (when active_actions is not empty)
    let a1 = Action::new(f1, 10, 1.0, ActionKind::Create);
    let a2 = Action::new(f2, 10, 1.0, ActionKind::Create);
    user.active_actions.push(a1);
    user.actions.push(a2);

    // user.rs: in_range false when get_file_pos returns None
    user.action_interval = 0.0;
    // Closure returns None for get_file_pos
    let _ = user.logic(0.0, 0.1, 5.0, 100 * gource_scene::ONE, |_| None);
}

#[test]
fn test_dirnode_uncovered_branches() {
    let mut dir = DirNode::new("/alpha/beta", 8, 1);
    // adjust_path when parent.is_none() and abspath has content
    dir.adjust_path(0);
    assert_eq!(dir.path_token, "");

    // adjust_path when parent is some, abspath.len() <= parent_token_offset (lines 213, 214)
    let mut sm: SlotMap<gource_sim::file::DirId, ()> = SlotMap::with_key();
    dir.parent = Some(sm.insert(()));
    dir.adjust_path(100);
    assert_eq!(dir.path_token, "");

    // calc_colour with hidden file (dirnode.rs:270)
    let mut files: SlotMap<FileId, File> = SlotMap::with_key();
    let mut hidden_f = File::new(
        "/alpha/beta/h.txt",
        Vec3::ONE,
        Vec2::ZERO,
        1,
        8.0,
        5.0,
        false,
    );
    hidden_f.pawn.set_hidden(true);
    let hfid = files.insert(hidden_f);
    dir.files.push(hfid);
    dir.calc_colour(&files);

    // average_file_colour with hidden file and child (dirnode.rs:453, 457, 478, 479)
    let mut dirs: SlotMap<gource_sim::file::DirId, DirNode> = SlotMap::with_key();
    let child_node = DirNode::new("/alpha/beta/child", 8, 1);
    let cid = dirs.insert(child_node);
    dir.children.push(cid);
    let avg = dir.average_file_colour(&files, &dirs);
    assert_eq!(avg, Vec3::ZERO);
}

#[test]
fn test_world_uncovered_branches() {
    let mut world = World::new(987, 654);
    let settings = GourceSettings {
        highlight_all_users: true,
        highlight_dirs: true,
        max_files: 100,
        ..Default::default()
    };

    // 1. world.rs:201, 202 - change_colours recolouring users and files
    let uid = world.add_user("test_recolour", &settings);
    let cf = CommitFile {
        filename: "/test/file.rs".to_string(),
        action: FileAction::Add,
        colour: <[f32; 3]>::from(Vec3::ONE),
        ..Default::default()
    };
    let fid = world.add_file(&cf, &settings).unwrap();
    world.change_colours(777);

    // 2. world.rs:272 - add_file returning None when file_as_dir is already a directory
    let cf_sub = CommitFile {
        filename: "/test/sub/f.rs".to_string(),
        action: FileAction::Add,
        colour: <[f32; 3]>::from(Vec3::ONE),
        ..Default::default()
    };
    let _ = world.add_file(&cf_sub, &settings);
    // Now "/test/sub/" is a dir! Trying to add a file named "/test/sub" should return None
    let cf_dir_conflict = CommitFile {
        filename: "/test/sub".to_string(),
        action: FileAction::Add,
        colour: <[f32; 3]>::from(Vec3::ONE),
        ..Default::default()
    };
    assert!(world.add_file(&cf_dir_conflict, &settings).is_none());

    // 3. world.rs:454 - add_file_to_tree visible_count increment when file is not hidden
    // We already added files, but let's add one directly to an existing dir where it's not hidden
    let dir_id = world.files[fid].dir.unwrap();
    let mut unhidden_f = File::new(
        "/test/unhidden.rs",
        Vec3::ONE,
        Vec2::ZERO,
        10,
        8.0,
        5.0,
        false,
    );
    unhidden_f.pawn.set_hidden(false);
    let unhidden_fid = world.files.insert(unhidden_f);
    world.add_file_to_tree(dir_id, unhidden_fid);

    // 4. world.rs:465, 466, 471 - add_file_to_tree recursive child match
    let cf_deep = CommitFile {
        filename: "/test/sub/deep/file.rs".to_string(),
        action: FileAction::Add,
        colour: <[f32; 3]>::from(Vec3::ONE),
        ..Default::default()
    };
    let deep_fid = world.add_file(&cf_deep, &settings).unwrap();
    assert!(world.files.contains_key(deep_fid));

    // 5. world.rs:487 - add_file_to_tree added true return
    let cf_deep2 = CommitFile {
        filename: "/test/sub/deep/file2.rs".to_string(),
        action: FileAction::Add,
        colour: <[f32; 3]>::from(Vec3::ONE),
        ..Default::default()
    };
    let deep_fid2 = world.add_file(&cf_deep2, &settings).unwrap();
    assert!(world.files.contains_key(deep_fid2));

    // 6. world.rs:564, 565 - add_node_to_dir reparenting
    // 7. world.rs:583 - adjust_depth_recursive root depth 1
    // 8. world.rs:612, 613, 614, 618, 624, 628, 629 - remove_file_from_tree when file was visible
    world.files[unhidden_fid].pawn.set_hidden(false);
    world.remove_file_from_tree(dir_id, unhidden_fid, "/test/", true);

    // 9. world.rs:653, 654, 655 - delete_dir_recursive with children
    // Delete file causing recursive dir deletion
    world.delete_file(deep_fid);
    world.delete_file(deep_fid2);

    // 10. Draw with hidden pawn to exercise continue branches in draw_scene (1128, 1145, 1269, 1352)
    world.users[uid].pawn.set_hidden(true);
    let proj = Projection::new(Vec3::new(0.0, 0.0, -500.0), Vec2::new(1280.0, 720.0));
    world.prepare_frame(&proj, &settings);
    let textures = SceneTextures {
        file: TextureId(1),
        beam: TextureId(2),
        default_user: TextureId(3),
    };
    let mut list = DrawList::new(glam::UVec2::new(1280, 720));
    world.draw_scene(&mut list, &proj, &settings, &textures);

    // 11. world.rs:339 - add_file_action with unknown username creates user
    let unknown_commit = Commit {
        timestamp: 200,
        username: "unknown_user".to_string(),
        files: vec![cf.clone()],
        ..Default::default()
    };
    world.add_file_action(&unknown_commit, &cf, fid, 1.0, &settings);
    assert!(world.users_by_name.contains_key("unknown_user"));
}
