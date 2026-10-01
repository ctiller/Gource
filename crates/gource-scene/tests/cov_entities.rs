//! Invariant and coverage tests for dirnode, pawn, user, and file components.

use glam::{UVec2, Vec2, Vec3};
use gource_core::StringHasher;
use gource_model::commit::{Commit, CommitFile, FileAction};
use gource_scene::ONE;
use gource_scene::dirnode::DirNode;
use gource_scene::file::{DirId, File, FileId};
use gource_scene::pawn::Pawn;
use gource_scene::user::{User, UserTexture};
use gource_scene::world::World;
use gource_settings::{FileColourMode, FileSizeMetric, GourceSettings};
use slotmap::SlotMap;

#[test]
fn test_dirnode_hierarchy_visibility_and_colours() {
    let mut dirs: SlotMap<DirId, DirNode> = SlotMap::with_key();
    let mut files: SlotMap<FileId, File> = SlotMap::with_key();

    let root_id = dirs.insert(DirNode::new("/", 1000, 384));
    let mut child_node = DirNode::new("/sub/", 1000, 384);
    child_node.parent = Some(root_id);
    let child_id = dirs.insert(child_node);

    dirs[root_id].children.push(child_id);

    // Test adjust_path corner cases:
    // 1. parent_token_offset >= abspath.len()
    dirs[child_id].adjust_path(100);
    assert_eq!(dirs[child_id].path_token, "");
    assert_eq!(dirs[child_id].token_offset(), dirs[child_id].path().len());

    // 2. parent is None
    dirs[root_id].adjust_path(0);
    assert_eq!(dirs[root_id].path_token, "");

    // Test is_visible with hidden and visible children
    assert!(!dirs[root_id].is_visible(&dirs));
    assert!(!dirs[child_id].is_visible(&dirs));

    dirs[child_id].add_visible();
    assert!(dirs[child_id].is_visible(&dirs));
    assert!(dirs[root_id].is_visible(&dirs));

    // Place dirnode
    dirs[child_id].place(gource_scene::IVec2::new(10 * ONE, 20 * ONE));
    assert_eq!(dirs[child_id].pos(), Vec2::new(10.0, 20.0));
    let bounds = dirs[child_id].bounds();
    assert!(bounds.contains(Vec2::new(10.0, 20.0)));

    // Create files in dirs
    let mut f1 = File::new(
        "/sub/file1.txt",
        Vec3::new(1.0, 0.0, 0.0),
        Vec2::ZERO,
        1,
        8.0,
        5.0,
        false,
    );
    f1.file_colour = Vec3::new(1.0, 0.0, 0.0);
    f1.pawn.elapsed = 10.0;
    f1.last_action = 0.0;
    f1.pawn.set_hidden(false);
    let f1_id = files.insert(f1);

    let mut f2 = File::new(
        "/sub/file2.rs",
        Vec3::new(0.0, 1.0, 0.0),
        Vec2::ZERO,
        2,
        8.0,
        5.0,
        false,
    );
    f2.file_colour = Vec3::new(0.0, 1.0, 0.0);
    f2.pawn.set_hidden(true); // hidden file
    let f2_id = files.insert(f2);

    dirs[child_id].files.push(f1_id);
    dirs[child_id].files.push(f2_id);

    // calc_colour with hidden and visible files
    dirs[child_id].calc_colour(&files);
    assert!(dirs[child_id].col.w > 0.0);

    // average_file_colour across child and root
    let avg_child = dirs[child_id].average_file_colour(&files, &dirs);
    assert_eq!(avg_child, Vec3::new(1.0, 0.0, 0.0));

    let avg_root = dirs[root_id].average_file_colour(&files, &dirs);
    assert_eq!(avg_root, avg_child);

    // Now add a visible file to root and check combined average
    let mut f3 = File::new(
        "/rootfile.c",
        Vec3::new(0.0, 0.0, 1.0),
        Vec2::ZERO,
        3,
        8.0,
        5.0,
        false,
    );
    f3.file_colour = Vec3::new(0.0, 0.0, 1.0);
    f3.pawn.elapsed = 10.0;
    f3.last_action = 0.0;
    f3.pawn.set_hidden(false);
    let f3_id = files.insert(f3);
    dirs[root_id].files.push(f3_id);

    let avg_root2 = dirs[root_id].average_file_colour(&files, &dirs);
    assert_ne!(avg_root2, Vec3::ZERO);
    assert_ne!(avg_root2, avg_child);

    // Test a directory with empty files list for average_file_colour count == 0
    let empty_dir_id = dirs.insert(DirNode::new("/empty/", 1000, 384));
    let avg_empty = dirs[empty_dir_id].average_file_colour(&files, &dirs);
    assert_eq!(avg_empty, Vec3::ZERO);
}

#[test]
fn test_pawn_full_state_and_methods() {
    let mut pawn = Pawn::new("Alice".to_string(), Vec2::ZERO, 42);
    assert_eq!(pawn.name(), "Alice");
    assert_eq!(pawn.tag_id(), 42);
    assert_eq!(pawn.size(), 0.0);
    assert_eq!(pawn.colour(), Vec3::ONE);
    assert_eq!(pawn.name_colour(), Vec3::ONE);

    pawn.set_pos(Vec2::new(15.0, 25.0));
    assert_eq!(pawn.pos(), Vec2::new(15.0, 25.0));

    // Mouse over & selected
    assert!(!pawn.is_mouseover());
    pawn.set_mouseover(true);
    assert!(pawn.is_mouseover());

    assert!(!pawn.is_selected());
    pawn.set_selected(true);
    assert!(pawn.is_selected());

    // set_graphic_dimensions with zero and non-zero
    pawn.set_graphic_dimensions(0, 50);
    assert_eq!(pawn.graphic_ratio, 1.0);
    pawn.set_graphic_dimensions(100, 50);
    assert_eq!(pawn.graphic_ratio, 0.5);

    // alpha with fadetime <= 0.0
    pawn.fadetime = 0.0;
    assert_eq!(pawn.alpha(), 1.0);
    pawn.fadetime = 2.0;
    pawn.elapsed = 1.0;
    assert_eq!(pawn.alpha(), 0.5);

    // name_alpha
    pawn.nametime = 5.0;
    pawn.name_interval = 0.0;
    pawn.set_selected(false);
    pawn.set_hidden(false);
    // name not visible when interval <= 0 and not selected
    assert_eq!(pawn.name_alpha(), 0.0);

    pawn.show_name();
    assert_eq!(pawn.name_interval, 5.0);
    // done = 0.0 -> < 1.0 -> 0.0
    assert_eq!(pawn.name_alpha(), 0.0);

    pawn.name_interval = 4.5; // done = 0.5
    assert_eq!(pawn.name_alpha(), 0.5);

    pawn.name_interval = 2.5; // done = 2.5 -> between 1.0 and nametime - 1.0 (4.0)
    assert_eq!(pawn.name_alpha(), 1.0);

    pawn.name_interval = 0.5; // done = 4.5 -> nametime - done = 0.5
    assert_eq!(pawn.name_alpha(), 0.5);

    pawn.set_hidden(true);
    assert_eq!(pawn.name_alpha(), 0.0);
}

#[test]
fn test_user_full_properties_and_actions() {
    let hasher = StringHasher::default();
    let mut user = User::new("bob", Vec2::new(1.0, 2.0), 10, 100.0, 1.0, &hasher);

    // User starts with name_interval = 5.0, so name is visible initially
    assert!(user.name_visible(false));
    user.pawn.name_interval = 0.0;
    assert!(!user.name_visible(false));
    assert!(user.name_visible(true));

    // UserTexture From / Into
    let tex = UserTexture::from(77);
    assert_eq!(tex.0, 77);
    let val: u32 = tex.into();
    assert_eq!(val, 77);

    // assign_graphic with uncoloured true/false
    user.assign_graphic(&hasher, Some(tex), UVec2::new(64, 64), true);
    assert_eq!(user.graphic, Some(tex));

    // Highlighting & selection
    assert!(!user.is_highlighted());
    let sel_col = Vec3::new(1.0, 0.0, 0.0);
    let hl_col = Vec3::new(0.0, 1.0, 0.0);
    // Unselected and unhighlighted name_colour:
    assert_eq!(user.name_colour(sel_col, hl_col), user.pawn.namecol);

    user.set_highlighted(true);
    assert!(user.is_highlighted());
    assert!(user.name_visible(false));

    assert_eq!(user.name_colour(sel_col, hl_col), hl_col);

    user.set_selected(true);
    assert_eq!(user.name_colour(sel_col, hl_col), sel_col);
    assert_eq!(user.colour(), Vec3::ONE);
    user.set_selected(false);
    assert_eq!(user.colour(), user.usercol);

    // Pending action count and action count
    assert_eq!(user.pending_action_count(), 0);
    let mut files: SlotMap<FileId, ()> = SlotMap::with_key();
    let fid1 = files.insert(());
    let fid2 = files.insert(());

    let a1 = gource_scene::Action::new(fid1, 100, 1.0, gource_scene::ActionKind::Create);
    let a2 = gource_scene::Action::new(
        fid2,
        100,
        1.0,
        gource_scene::ActionKind::Modify {
            modify_colour: Vec3::ONE,
        },
    );
    user.add_action(a1);
    user.add_action(a2);
    assert_eq!(user.pending_action_count(), 2);
    assert_eq!(user.action_count(), 2);

    // Personal space distance branches:
    // 1. Only pending actions
    let p_space = user.personal_space(100 * ONE);
    assert_eq!(p_space, 10 * ONE);

    // 2. Active actions
    let a = user.actions.remove(0);
    user.active_actions.push(a);
    let p_space_active = user.personal_space(100 * ONE);
    assert_eq!(p_space_active, 50 * ONE);

    // 3. File removed
    user.file_removed(fid1);
    assert_eq!(user.active_actions.len(), 0);
    assert_eq!(user.removed_active_count, 1);
    assert_eq!(user.action_count(), 2);

    user.file_removed(fid2);
    assert_eq!(user.actions.len(), 0);
    assert_eq!(user.pending_action_count(), 0);

    // 4. No actions left
    let p_space_none = user.personal_space(100 * ONE);
    assert_eq!(p_space_none, 50 * ONE); // removed_active_count > 0, so action_count != 0

    // alpha with user_idle_time
    user.pawn.elapsed = 10.0;
    user.last_action = 5.0;
    assert!(user.is_fading(3.0));
    assert!(!user.is_fading(6.0));
    assert!(!user.is_inactive());
    user.last_action = -1.0;
    assert!(user.is_inactive());

    let a_idle = user.alpha(4.0);
    assert!(a_idle < 1.0);
}

#[test]
fn test_file_colour_modes_shadow_and_pulse() {
    let hasher = StringHasher::default();
    let mut file = File::new(
        "/dir/test.rs",
        Vec3::new(0.5, 0.5, 0.5),
        Vec2::ZERO,
        1,
        8.0,
        5.0,
        false,
    );
    file.colourize(&hasher);
    file.pawn.elapsed = 10.0;
    file.last_action = 0.0;

    // File name colour
    assert_eq!(file.name_colour(Vec3::X), file.pawn.namecol);
    file.pawn.set_selected(true);
    assert_eq!(file.name_colour(Vec3::X), Vec3::X);
    assert_eq!(file.colour(), Vec3::ONE);
    file.pawn.set_selected(false);

    // File display name
    assert_eq!(file.display_name(true), "rs");
    assert_eq!(file.display_name(false), "test.rs");

    // File colour without extension
    let mut file_no_ext = File::new("/dir/noext", Vec3::ONE, Vec2::ZERO, 2, 8.0, 5.0, false);
    file_no_ext.colourize(&hasher);
    assert_eq!(file_no_ext.file_colour, Vec3::ONE);
    assert_eq!(file_no_ext.display_name(true), "noext");

    // File display colours across modes
    file.created_timestamp = 1_000_000;
    let col_ext = file.display_colour(FileColourMode::Extension, 1_000_000);
    assert_eq!(col_ext, file.file_colour);

    // churn_ratio when total == 0
    file.total_added = 0;
    file.total_removed = 0;
    assert_eq!(file.churn_ratio(), 0.0);

    file.total_added = 100;
    file.total_removed = 20;
    assert!((file.churn_ratio() - (20.0 / 120.0)).abs() < 1e-4);
    let col_churn = file.display_colour(FileColourMode::Churn, 1_000_000);
    assert_ne!(col_churn, col_ext);

    let col_age = file.display_colour(FileColourMode::Age, 1_000_000 + 86400 * 180);
    assert_ne!(col_age, col_ext);

    file.dominant_cohort_colour = Some(Vec3::new(0.12, 0.34, 0.56));
    let col_cohort = file.display_colour(FileColourMode::Cohort, 1_000_000);
    assert_eq!(col_cohort, Vec3::new(0.12, 0.34, 0.56));

    // Selected display_colour returns Vec3::ONE
    file.pawn.set_selected(true);
    assert_eq!(
        file.display_colour(FileColourMode::Extension, 1_000_000),
        Vec3::ONE
    );
    file.pawn.set_selected(false);

    // display_colour with recent touch (elapsed - last_action < 1.0)
    file.pawn.elapsed = 0.5;
    file.last_action = 0.0;
    let col_touched = file.display_colour(FileColourMode::Extension, 1_000_000);
    assert_ne!(col_touched, file.file_colour);
    file.pawn.elapsed = 10.0;

    // Shadow file lifecycle and solidification
    file.is_shadow = true;
    file.shadow_alpha = 0.3;
    let shadow_a = file.alpha();
    assert!((shadow_a - 0.3).abs() < 1e-4);

    file.solidify();
    assert!(!file.is_shadow);
    assert!(file.solidifying);
    assert_eq!(file.solidify_timer, 0.5);

    // Pulse visual
    assert!(file.pulse_visual().is_some());
    let (ring_size, pulse_col) = file.pulse_visual().unwrap();
    assert!(ring_size >= file.pawn.size);
    assert!(pulse_col.w > 0.0);

    // Step logic during solidification with file_idle_time > 0 and fade triggered
    file.pawn.elapsed = 20.0;
    file.last_action = 5.0;
    let just_exp = file.logic(0.25, 10.0);
    assert!(!just_exp);
    assert!(file.fade_start > 0.0);

    let just_exp2 = file.logic(0.3, 0.0);
    assert!(!just_exp2);
    assert!(!file.solidifying);
    assert_eq!(file.solidify_timer, 0.0);

    // Line delta with removal pulse
    file.apply_line_delta(Some(10), Some(50), 1.0);
    assert_eq!(file.pulse_delta, -40);
    let visual = file.pulse_visual().unwrap();
    // delta < 0 should give red pulse (x > 0.9)
    assert!(visual.1.x > 0.9);

    // Weighted size goal and target size
    file.pawn.set_hidden(false);
    file.set_weight_target(100, 100, 8.0);
    assert!(file.target_size() > 0.0);
    assert_eq!(file.size_goal(), 0); // fade_start > 0 -> size_goal is 0
    file.fade_start = -1.0;
    assert_eq!(file.size_goal(), file.sim.target_size);

    file.removing = true;
    assert_eq!(file.size_goal(), 0);
}

#[test]
fn test_world_metrics_tuning_and_cohorts() {
    let mut world = World::new(999, 31);
    let mut settings = GourceSettings {
        file_size_metric: FileSizeMetric::Churn,
        file_colour_mode: FileColourMode::Cohort,
        file_pulse: 1.0,
        highlight_all_users: true,
        ..Default::default()
    };

    let uid = world.add_user("carol", &settings);
    assert!(world.users[uid].is_highlighted());

    let cf = CommitFile {
        filename: "/src/lib.rs".to_string(),
        action: FileAction::Add,
        colour: [1.0, 1.0, 1.0],
        lines_added: Some(25),
        lines_removed: Some(5),
        ..Default::default()
    };

    let commit = Commit {
        timestamp: 1_600_000_000,
        username: "carol".to_string(),
        files: vec![cf.clone()],
        ..Default::default()
    };

    let fid = world.add_file(&cf, &settings).unwrap();
    world.add_file_action(&commit, &cf, fid, 1.0, &settings);

    let file = &world.files[fid];
    assert_eq!(file.created_timestamp, commit.timestamp);
    assert!(file.dominant_cohort_colour.is_some());
    assert_eq!(file.lines, 20);
    assert_eq!(file.byte_size, 20 * 35);
    assert!(file.weighted);

    // Test other FileSizeMetrics
    settings.file_size_metric = FileSizeMetric::Size;
    world.add_file_action(&commit, &cf, fid, 1.0, &settings);
    assert!(world.files[fid].sim.target_size > 0);

    settings.file_size_metric = FileSizeMetric::Diff;
    world.add_file_action(&commit, &cf, fid, 1.0, &settings);
    assert!(world.files[fid].sim.target_size > 0);

    // Apply tuning
    let tuning = gource_settings::TuningSettings {
        dir_padding: 2.0,
        min_dir_size: 10.0,
        file_diameter: 12.0,
        gravity: -5.0,
        beam_length: 50.0,
        action_distance: 30.0,
        personal_space: 40.0,
        shadow_strength: 0.5,
        ..Default::default()
    };
    world.apply_tuning(&tuning);
    assert_eq!(world.tuning.dir_padding, 2.0);
    assert_eq!(world.tuning.force_gravity, 5.0);

    // Change colours
    world.change_colours(12345);
    assert_eq!(world.hasher.seed, 12345);

    // Recursive directory queries
    let root_dirs = world.find_dirs("/src");
    assert_eq!(root_dirs.len(), 1);
    let files_rec = world.get_files_recursive(root_dirs[0]);
    assert_eq!(files_rec, vec![fid]);

    // Max files check
    settings.max_files = 1;
    let cf2 = CommitFile {
        filename: "/src/other.rs".to_string(),
        action: FileAction::Add,
        colour: [1.0, 1.0, 1.0],
        ..Default::default()
    };
    assert!(world.add_file(&cf2, &settings).is_none());

    // File as dir check
    let cf3 = CommitFile {
        filename: "/src".to_string(),
        action: FileAction::Add,
        colour: [1.0, 1.0, 1.0],
        ..Default::default()
    };
    assert!(world.add_file(&cf3, &settings).is_none());
}
