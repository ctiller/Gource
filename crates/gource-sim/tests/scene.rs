use glam::{Vec2, Vec3, Vec4};
use gource_draw::list::{DrawList, Material, TextureId};
use gource_draw::{Gfx, Projection};
use gource_settings::GourceSettings;
use gource_sim::action::{Action, ActionKind};
use gource_sim::dirnode::DirNode;
use gource_sim::file::{File, FileId};
use gource_sim::pawn::Pawn;
use gource_sim::spline::SplineEdge;
use gource_sim::user::User;
use gource_sim::world::{SceneFonts, SceneTextures, World};
use gource_vcs::commit::{Commit, CommitFile, FileAction};
use slotmap::SlotMap;

#[test]
fn test_pawn_all_methods() {
    let mut p = Pawn::new("test_pawn".to_string(), Vec2::new(10.0, 20.0), 99);
    assert_eq!(p.size(), 0.0);
    p.size = 20.0;
    assert_eq!(p.size(), 20.0);
    assert_eq!(p.pos(), Vec2::new(10.0, 20.0));
    p.set_pos(Vec2::new(30.0, 40.0));
    assert_eq!(p.pos(), Vec2::new(30.0, 40.0));
    assert_eq!(p.tag_id(), 99);
    assert_eq!(p.name(), "test_pawn");

    // Dimensions
    p.set_graphic_dimensions(200, 100);
    assert_eq!(p.graphic_ratio, 0.5);
    assert_eq!(p.dims, Vec2::new(20.0, 10.0));
    p.set_graphic_dimensions(0, 100);
    assert_eq!(p.graphic_ratio, 1.0);

    // Mouseover / Selected / Hidden
    p.set_mouseover(true);
    assert!(p.is_mouseover());
    p.set_selected(true);
    assert!(p.is_selected());
    p.set_hidden(true);
    assert!(p.is_hidden());
    assert!(!p.name_visible());

    p.set_hidden(false);
    p.set_selected(false);
    p.name_interval = 0.0;
    assert!(!p.name_visible());
    p.show_name();
    assert!(p.name_visible());
    p.show_name(); // second call when name_interval > 0 is no-op
    assert_eq!(p.name_interval, p.nametime);

    // Alpha with fadetime 0
    p.fadetime = 0.0;
    assert_eq!(p.alpha(), 1.0);
    p.fadetime = 2.0;
    p.elapsed = 1.0;
    assert_eq!(p.alpha(), 0.5);

    // Colour / name_colour
    assert_eq!(p.colour(), Vec3::ONE);
    assert_eq!(p.name_colour(), Vec3::ONE);

    // Bounds
    let b = p.bounds();
    assert_eq!(b.min, Vec2::new(20.0, 30.0));
    assert_eq!(b.max, Vec2::new(40.0, 50.0));

    // Logic
    p.logic(1.0);
    assert_eq!(p.elapsed, 2.0);
    assert_eq!(p.name_interval, 4.0);

    // Name alpha phases
    p.name_interval = 4.5; // done = 0.5 < 1.0
    assert_eq!(p.name_alpha(), 0.5);
    p.name_interval = 2.5; // done = 2.5 in (1.0..4.0)
    assert_eq!(p.name_alpha(), 1.0);
    p.name_interval = 0.5; // done = 4.5 > 4.0 -> nametime - done = 0.5
    assert_eq!(p.name_alpha(), 0.5);
    p.name_interval = -1.0;
    assert_eq!(p.name_alpha(), 0.0);
}

#[test]
fn test_spline_edge_coverage() {
    let mut spline = SplineEdge::new();
    assert_eq!(spline.label_pos(), Vec2::ZERO);

    // Collinear positions: mid and to are parallel -> ang = 0 -> detail = 1
    let p1 = Vec2::new(100.0, 0.0);
    let p2 = Vec2::new(0.0, 0.0);
    let spos = Vec2::new(50.0, 0.0);
    spline.update(p1, Vec4::ONE, p2, Vec4::ONE, spos, 0.5);
    assert_eq!(spline.spline_point.len(), 2); // min edge_detail = 1
    assert_eq!(spline.label_pos(), spos);

    // Big angle triggering max edge detail (10)
    let p1 = Vec2::new(100.0, 0.0);
    let p2 = Vec2::new(-100.0, 0.0);
    let spos = Vec2::new(0.0, 100.0);
    spline.update(p1, Vec4::ONE, p2, Vec4::ONE, spos, 0.2);
    assert_eq!(spline.spline_point.len(), 11); // detail 10 -> 11 points
}

#[test]
fn test_file_full_coverage() {
    let mut f = File::new(
        "/a/b/test.rs",
        Vec3::new(0.2, 0.4, 0.6),
        Vec2::new(5.0, 5.0),
        77,
        10.0,
        3.0,
        true,
    );
    assert_eq!(f.fullpath, "/a/b/test.rs");
    assert_eq!(f.path, "/a/b/");
    assert_eq!(f.ext, "rs");

    // Overlaps
    let dir_pos = Vec2::new(100.0, 100.0);
    assert!(f.overlaps(dir_pos, Vec2::new(105.0, 105.0)));
    assert!(!f.overlaps(dir_pos, Vec2::new(200.0, 200.0)));

    // Name colour
    assert_eq!(f.name_colour(Vec3::X), f.pawn.namecol);
    f.pawn.selected = true;
    assert_eq!(f.name_colour(Vec3::X), Vec3::X);
    assert_eq!(f.colour(), Vec3::ONE);
    f.pawn.selected = false;

    // Colour with touched timestamp
    f.pawn.elapsed = 10.0;
    f.last_action = 9.5; // 0.5s ago -> blend
    f.touch_colour = Vec3::new(1.0, 0.0, 0.0);
    f.file_colour = Vec3::new(0.0, 1.0, 0.0);
    let c = f.colour();
    assert!((c.x - 0.5).abs() < 1e-4);
    assert!((c.y - 0.5).abs() < 1e-4);

    // Alpha when fade_start > 0
    f.fade_start = 9.0; // elapsed 10.0 -> fade elapsed 1.0 -> alpha 0.0
    assert_eq!(f.alpha(), 0.0);

    // Touch ignore cases
    f.forced_removal = true;
    assert!(!f.touch(100, Vec3::ONE));
    f.forced_removal = false;
    f.removing = true;
    f.removed_timestamp = 500;
    assert!(!f.touch(400, Vec3::ONE)); // touch timestamp < removed timestamp

    // Idle expiration logic
    f.removing = false;
    f.fade_start = -1.0;
    f.last_action = 0.0;
    f.pawn.elapsed = 10.0;
    let just_exp = f.logic(0.1, 5.0);
    assert!(!just_exp);
    assert!(f.fade_start > 0.0); // started fade

    // Hidden resetting elapsed
    f.pawn.set_hidden(true);
    f.logic(0.1, 0.0);
    assert_eq!(f.pawn.elapsed, 0.0);
}

#[test]
fn test_user_full_coverage() {
    let hasher = gource_core::StringHasher::default();
    let mut u = User::new("david", Vec2::new(50.0, 50.0), 12, 400.0, 1.5, &hasher);

    // A custom image without --colour-images: white, then the C++ blend.
    u.assign_graphic(&hasher, None, glam::UVec2::new(384, 512), true);
    assert_eq!(u.usercol, Vec3::splat(0.9));
    assert!((u.pawn.graphic_ratio - 512.0 / 384.0).abs() < 1e-6);
    assert_eq!(u.pawn.dims, Vec2::new(30.0, 40.0));

    // Recolouring (`changeColours`) uses the raw hash without the blend.
    u.colourize(&hasher);
    assert_eq!(u.usercol, hasher.colour_hash("david"));

    // Alpha with idle time
    u.pawn.elapsed = 10.0;
    u.last_action = 5.0;
    assert_eq!(u.alpha(4.0), 0.0); // 10 - 5 - 4 = 1.0 -> alpha = 1.0 - 1.0 = 0.0

    // Overlap forces
    let mut rng = gource_core::crand::CRand::new(999);
    u.apply_force_user(u.pawn.pos, 100.0, &mut rng); // dist < 0.001
    assert!(u.pawn.accel.length() > 0.0);
    u.pawn.accel = Vec2::ZERO;

    u.apply_force_action(u.pawn.pos, 50.0, 100.0, &mut rng); // dist < 0.001
    assert!(u.pawn.accel.length() > 0.0);
    u.pawn.accel = Vec2::ZERO;

    // Beam distance pull force
    u.apply_force_action(u.pawn.pos + Vec2::new(200.0, 0.0), 50.0, 100.0, &mut rng);
    assert!(u.pawn.accel.x > 0.0);

    // Accel clamping to max speed
    u.pawn.speed = 10.0;
    u.pawn.accel = Vec2::new(100.0, 0.0);
    u.logic(0.0, 0.1, 5.0, 100.0, 0.5, |_| None);
    assert!(u.pawn.pos.x > 50.0);

    // Active actions logic
    let mut sm: SlotMap<FileId, ()> = SlotMap::with_key();
    let f1 = sm.insert(());
    let mut act = Action::new(f1, 100, 0.0, ActionKind::Remove);
    act.rate = 10.0;
    u.active_actions.push(act);
    let events = u.logic(1.0, 0.5, 5.0, 100.0, 0.5, |_| None);
    assert!(!events.is_empty());
}

#[test]
fn test_dirnode_full_coverage() {
    let mut d = DirNode::new("/root/sub", 8.0, 1.2);
    // C++ RDirNode's constructor runs adjustPath() with no parent:
    // path_token_offset = abspath.size() ("/root/sub/").
    assert_eq!(d.token_offset(), 10);
    assert_eq!(d.area(), 0.0);
    assert_eq!(d.colour(), Vec4::ONE);
    assert_eq!(d.node_normal(), Vec2::ZERO);
    assert_eq!(d.spos(), Vec2::ZERO);
    assert_eq!(d.projected_pos(), Vec2::ZERO);

    // prefixed_by edge cases
    assert!(!d.prefixed_by(""));
    assert!(d.prefixed_by("/root"));
    assert!(!d.prefixed_by("/root/other"));

    // common_path_prefix edge cases
    assert_eq!(d.common_path_prefix("no_slash_prefix"), "");

    // adjust_path edge cases
    d.adjust_path(100); // offset > length
    assert_eq!(d.path_token, "");

    // Overlap forces: dist < 0.00001
    let mut rng = gource_core::crand::CRand::new(1234);
    d.apply_force_dir(d.pos, 10.0, &mut rng);
    assert!(d.accel.length() > 0.0);

    // Spline update
    let p_pos = Vec2::new(0.0, 100.0);
    d.update_spline_point(0.5, p_pos);
    assert!(d.spos.length() > 0.0);

    // Initial position
    let hasher = gource_core::StringHasher::default();
    d.set_initial_position(p_pos, Some(Vec2::new(0.0, 200.0)), &hasher);
    assert!(d.position_initialized);

    // Rotate around center
    d.pos = Vec2::new(10.0, 0.0);
    d.rotate_around(1.0, 0.0, Vec2::new(5.0, 0.0));
    assert!((d.pos.x - 5.0).abs() < 1e-4);
    assert!((d.pos.y - 5.0).abs() < 1e-4);

    // Bounds
    let b = d.bounds();
    assert!(b.width() > 0.0);

    // average_file_colour
    let files = SlotMap::with_key();
    let dirs = SlotMap::with_key();
    assert_eq!(d.average_file_colour(&files, &dirs), Vec3::ZERO);
}

#[test]
fn test_world_drawing_and_frustum() {
    let mut world = World::new(777, 31);
    let settings = GourceSettings {
        highlight_all_users: true,
        highlight_dirs: true,
        ..Default::default()
    };

    // Add user and file
    let uid = world.add_user("testuser", &settings);
    let cf = CommitFile {
        filename: "/sub/file.rs".to_string(),
        action: FileAction::Add,
        colour: Vec3::new(0.8, 0.2, 0.1),
        ..Default::default()
    };
    let fid = world.add_file(&cf, &settings).unwrap();

    let commit = Commit {
        timestamp: 100,
        username: "testuser".to_string(),
        files: vec![cf.clone()],
    };
    world.add_file_action(&commit, &cf, fid, 0.0, &settings);

    // Update users and tree so file is touched and active
    world.update_users(0.5, 0.3, &settings);
    world.update_bounds();
    world.interact_users();
    world.interact_dirs();
    world.update_dirs(0.1, 0.2, 0.0);

    let proj = Projection::new(Vec3::new(0.0, 0.0, -500.0), Vec2::new(1280.0, 720.0));
    world.prepare_frame(&proj, &settings);

    let textures = SceneTextures {
        file: TextureId(1),
        beam: TextureId(2),
        default_user: TextureId(3),
    };

    let mut list = DrawList::new(glam::UVec2::new(1280, 720));
    world.draw_scene(&mut list, &proj, &settings, &textures);
    assert!(!list.is_empty());

    // Check batches emitted in drawScene:
    // Has bloom batch, alpha batches
    let has_bloom = list.batches.iter().any(|b| b.material == Material::Bloom);
    assert!(has_bloom);

    // Drawing with hide flags
    let mut hide_settings = settings.clone();
    hide_settings.hide_tree = true;
    hide_settings.hide_files = true;
    hide_settings.hide_users = true;
    hide_settings.hide_bloom = true;
    let mut empty_list = DrawList::new(glam::UVec2::new(1280, 720));
    world.draw_scene(&mut empty_list, &proj, &hide_settings, &textures);
    assert!(empty_list.is_empty());

    // Font rendering via Gfx
    let mut gfx = Gfx::new();
    let default_face = gfx.fonts.default_face();
    let scene_fonts = SceneFonts {
        file: gfx.fonts.font(default_face, 14),
        file_selected: gfx.fonts.font(default_face, 18),
        user: gfx.fonts.font(default_face, 14),
        user_selected: gfx.fonts.font(default_face, 18),
        dir: gfx.fonts.font(default_face, 14),
    };

    let mut text_list = DrawList::new(glam::UVec2::new(1280, 720));
    world.draw_names(
        &mut text_list,
        &mut gfx,
        &settings,
        &scene_fonts,
        Some(uid),
        Some(fid),
    );
    assert!(!text_list.is_empty());
}

#[test]
fn test_world_edge_cases() {
    let mut world = World::new(999, 42);
    let mut settings = GourceSettings {
        max_files: 1,
        ..Default::default()
    };
    let cf1 = CommitFile {
        filename: "/a.txt".to_string(),
        action: FileAction::Add,
        colour: Vec3::ONE,
        ..Default::default()
    };
    let cf2 = CommitFile {
        filename: "/b.txt".to_string(),
        action: FileAction::Add,
        colour: Vec3::ONE,
        ..Default::default()
    };
    assert!(world.add_file(&cf1, &settings).is_some());
    assert!(world.add_file(&cf2, &settings).is_none());

    // File that is directory
    let _cf_dir = CommitFile {
        filename: "/a.txt".to_string(),
        action: FileAction::Add,
        colour: Vec3::ONE,
        ..Default::default()
    };
    // Already exists as file, but is_dir checks if filename + "/" is dir
    let cf_sub = CommitFile {
        filename: "/a.txt/nested.txt".to_string(),
        action: FileAction::Add,
        colour: Vec3::ONE,
        ..Default::default()
    };
    settings.max_files = 10;
    let sub_id = world.add_file(&cf_sub, &settings).unwrap();
    assert_eq!(world.files[sub_id].fullpath, "/a.txt/nested.txt");

    // find_dirs and get_files_recursive
    let found = world.find_dirs("/a.txt");
    assert!(!found.is_empty());
    let rec_files = world.get_files_recursive(world.root);
    assert!(!rec_files.is_empty());

    // Recolour
    world.change_colours(101);
    assert_eq!(world.hasher.seed, 101);
}

#[test]
fn test_platform_viewport_and_requests() {
    use gource_sim::platform::{PlatformRequest, Viewport};
    let vp = Viewport::new(1920, 1080);
    assert_eq!(vp.size(), glam::Vec2::new(1920.0, 1080.0));
    assert_eq!(vp.dpi_ratio, 1.0);

    let reqs = [
        PlatformRequest::Quit,
        PlatformRequest::ToggleFullscreen,
        PlatformRequest::ToggleFrameless,
    ];
    assert_eq!(reqs.len(), 3);
}

#[test]
fn test_dirnode_additional_coverage() {
    let mut d = DirNode::new("/alpha/beta", 10.0, 2.0);
    assert_eq!(d.pos(), Vec2::ZERO);
    d.set_pos(Vec2::new(15.0, 25.0));
    assert_eq!(d.pos(), Vec2::new(15.0, 25.0));
    assert!(d.radius() >= 0.0);
    assert_eq!(d.parent_radius(), 1.0);

    let mut parent = DirNode::new("/alpha", 10.0, 2.0);
    parent.set_pos(Vec2::new(50.0, 50.0));
    let dist = d.distance_to_parent(&parent);
    assert!(dist > 0.0);

    // rotate
    d.rotate(0.5, 0.2);

    let mut dirs: SlotMap<gource_sim::file::DirId, DirNode> = SlotMap::with_key();
    let child = DirNode::new("/alpha/beta/gamma", 10.0, 2.0);
    let cid = dirs.insert(child);
    d.children.push(cid);

    // move_step with elasticity > 0.0
    d.parent = Some(cid);
    d.pos = Vec2::new(10.0, 10.0);
    d.accel = Vec2::new(10.0, 5.0);
    d.move_step(0.1, 0.5);
    assert!(d.pos.x > 10.0);

    // is_visible with visible child
    assert!(!d.is_visible(&dirs));
    dirs[cid].visible = true;
    assert!(d.is_visible(&dirs));

    // average_file_colour with active files and child dirs
    let mut files: SlotMap<FileId, File> = SlotMap::with_key();
    let mut file1 = File::new(
        "/alpha/beta/file1.rs",
        Vec3::new(1.0, 0.0, 0.0),
        Vec2::ZERO,
        1,
        8.0,
        5.0,
        false,
    );
    file1.pawn.set_hidden(false);
    let mut file2 = File::new(
        "/alpha/beta/gamma/file2.rs",
        Vec3::new(0.0, 1.0, 0.0),
        Vec2::ZERO,
        2,
        8.0,
        5.0,
        false,
    );
    file2.pawn.set_hidden(false);
    let f1 = files.insert(file1);
    let f2 = files.insert(file2);
    d.files.push(f1);
    dirs[cid].files.push(f2);
    let avg = d.average_file_colour(&files, &dirs);
    assert!(avg.x > 0.0 || avg.y > 0.0);

    // calc_colour
    d.calc_colour(&files);
    assert!(d.colour().x > 0.0 || d.colour().y > 0.0);
}

#[test]
fn test_user_additional_coverage() {
    let hasher = gource_core::StringHasher::default();
    let mut u = User::new("alice", Vec2::new(10.0, 10.0), 5, 200.0, 1.0, &hasher);

    assert_eq!(u.pending_action_count(), 0);
    assert!(!u.is_highlighted());
    // In User::new, pawn.name_interval is initialized to 5.0 so name_visible is true initially
    assert!(u.name_visible(false));
    u.pawn.name_interval = 0.0;
    assert!(!u.name_visible(false));
    u.set_highlighted(true);
    assert!(u.is_highlighted());
    assert!(u.name_visible(false));

    // Name colour with selection / highlight
    let ncol_hl = u.name_colour(Vec3::new(1.0, 1.0, 0.0), Vec3::new(0.0, 1.0, 1.0));
    assert_eq!(ncol_hl, Vec3::new(0.0, 1.0, 1.0));
    u.pawn.set_selected(true);
    let ncol_sel = u.name_colour(Vec3::new(1.0, 1.0, 0.0), Vec3::new(0.0, 1.0, 1.0));
    assert_eq!(ncol_sel, Vec3::new(1.0, 1.0, 0.0));

    // Desired dist force pull in apply_force_action
    let mut rng = gource_core::crand::CRand::new(123);
    u.apply_force_action(Vec2::new(15.0, 10.0), 20.0, 50.0, &mut rng);

    // Action queue with overdue max_file_lag
    let mut sm: SlotMap<FileId, ()> = SlotMap::with_key();
    let f1 = sm.insert(());
    let act1 = Action::new(f1, 10, 0.0, ActionKind::Create);
    let act2 = Action::new(
        f1,
        20,
        10.0,
        ActionKind::Modify {
            modify_colour: Vec3::ONE,
        },
    );
    u.add_action(act1);
    u.add_action(act2);
    assert_eq!(u.pending_action_count(), 2);

    // Logic with max_file_lag causing action to accelerate
    u.logic(50.0, 0.1, 5.0, 100.0, 0.5, |_| Some(Vec2::new(10.0, 10.0)));
    assert!(!u.active_actions.is_empty());
}

#[test]
fn test_world_comprehensive_coverage() {
    let mut world = World::new(1234, 5678);
    let settings = GourceSettings {
        highlight_users: vec!["bob".to_string()],
        selection_colour: Vec3::new(1.0, 1.0, 0.0),
        highlight_colour: Vec3::new(0.0, 1.0, 1.0),
        dir_name_depth: 2,
        dir_name_position: 0.5,
        ..Default::default()
    };

    // Tuning default check
    assert_eq!(world.tuning.file_diameter, 8.0);

    // Add highlighted user
    let uid_bob = world.add_user("bob", &settings);
    assert!(world.users[uid_bob].is_highlighted());

    // Add unhighlighted user
    let uid_charlie = world.add_user("charlie", &settings);
    assert!(!world.users[uid_charlie].is_highlighted());
    world.users[uid_charlie].pawn.show_name();

    // Add files across several directories to create sibling directories and multi-level depth
    let cf1 = CommitFile {
        filename: "/src/core/engine.rs".to_string(),
        action: FileAction::Add,
        colour: Vec3::new(1.0, 0.0, 0.0),
        ..Default::default()
    };
    let cf2 = CommitFile {
        filename: "/src/core/render.rs".to_string(),
        action: FileAction::Add,
        colour: Vec3::new(0.0, 1.0, 0.0),
        ..Default::default()
    };
    let cf3 = CommitFile {
        filename: "/src/net/socket.rs".to_string(),
        action: FileAction::Add,
        colour: Vec3::new(0.0, 0.0, 1.0),
        ..Default::default()
    };
    let fid1 = world.add_file(&cf1, &settings).unwrap();
    let fid2 = world.add_file(&cf2, &settings).unwrap();
    let fid3 = world.add_file(&cf3, &settings).unwrap();

    // Commit actions: delete, modify, add
    let commit_bob = Commit {
        timestamp: 100,
        username: "bob".to_string(),
        files: vec![
            cf1.clone(),
            CommitFile {
                filename: cf2.filename.clone(),
                action: FileAction::Modify,
                colour: Vec3::new(0.5, 0.5, 0.0),
                ..Default::default()
            },
            CommitFile {
                filename: cf3.filename.clone(),
                action: FileAction::Delete,
                colour: Vec3::new(1.0, 0.0, 0.0),
                ..Default::default()
            },
        ],
    };
    world.add_file_action(&commit_bob, &commit_bob.files[0], fid1, 0.0, &settings);
    world.add_file_action(&commit_bob, &commit_bob.files[1], fid2, 0.0, &settings);
    world.add_file_action(&commit_bob, &commit_bob.files[2], fid3, 0.0, &settings);

    // Initial position & logic update on dirs & users
    for did in world.dir_map.values().copied().collect::<Vec<_>>() {
        world.dirs[did].visible = true;
    }
    world.update_bounds();
    world.interact_users();
    world.interact_dirs();
    world.update_dirs(0.1, 0.5, 0.0);

    // Finish actions in update_users
    for _ in 0..15 {
        world.update_users(1.0, 0.2, &settings);
    }

    // Prepare frame and draw
    let proj = Projection::new(Vec3::new(0.0, 0.0, -1000.0), Vec2::new(1920.0, 1080.0));
    world.prepare_frame(&proj, &settings);

    let textures = SceneTextures {
        file: TextureId(1),
        beam: TextureId(2),
        default_user: TextureId(3),
    };
    let mut list = DrawList::new(glam::UVec2::new(1920, 1080));
    world.draw_scene(&mut list, &proj, &settings, &textures);
    assert!(!list.is_empty());

    // Draw names with selected file and unselected users
    let mut gfx = Gfx::new();
    let default_face = gfx.fonts.default_face();
    let scene_fonts = SceneFonts {
        file: gfx.fonts.font(default_face, 14),
        file_selected: gfx.fonts.font(default_face, 18),
        user: gfx.fonts.font(default_face, 14),
        user_selected: gfx.fonts.font(default_face, 18),
        dir: gfx.fonts.font(default_face, 14),
    };
    let mut text_list = DrawList::new(glam::UVec2::new(1920, 1080));
    world.files[fid1].pawn.show_name();
    world.draw_names(
        &mut text_list,
        &mut gfx,
        &settings,
        &scene_fonts,
        None,
        Some(fid1),
    );
    assert!(!text_list.is_empty());

    // File deletion and recursive dir cleanup
    let dinfo = world.delete_file(fid3);
    assert!(dinfo.is_some());
    let uinfo = world.delete_user(uid_bob);
    assert!(uinfo.is_some());
}

#[test]
fn test_world_deep_tree_forces_and_reparenting() {
    let mut world = World::new(9999, 1111);
    let mut settings = GourceSettings::default();

    // Force area > 10000 by setting positions spread out
    let cf_a = CommitFile {
        filename: "/a/b/c/d/file1.txt".to_string(),
        action: FileAction::Add,
        colour: Vec3::new(1.0, 0.0, 0.0),
        ..Default::default()
    };
    let cf_b = CommitFile {
        filename: "/a/b/c/e/file2.txt".to_string(),
        action: FileAction::Add,
        colour: Vec3::new(0.0, 1.0, 0.0),
        ..Default::default()
    };
    let cf_c = CommitFile {
        filename: "/a/b/x/y/file3.txt".to_string(),
        action: FileAction::Add,
        colour: Vec3::new(0.0, 0.0, 1.0),
        ..Default::default()
    };
    let f1 = world.add_file(&cf_a, &settings).unwrap();
    let f2 = world.add_file(&cf_b, &settings).unwrap();
    let f3 = world.add_file(&cf_c, &settings).unwrap();

    // Spread directory positions to trigger max quadtree depth (>10000 area)
    for (i, dir) in world.dirs.values_mut().enumerate() {
        dir.pos = Vec2::new((i as f32) * 500.0, (i as f32) * 500.0);
        dir.visible = true;
    }

    world.update_bounds();
    assert!(world.dir_bounds.area() > 10000.0);

    // interact_dirs with deep quadtree
    world.interact_dirs();

    // User interactions with area > 10000
    let u1 = world.add_user("user1", &settings);
    let u2 = world.add_user("user2", &settings);
    world.users[u1].pawn.set_pos(Vec2::new(100.0, 100.0));
    world.users[u2].pawn.set_pos(Vec2::new(120.0, 100.0));
    world.update_bounds();
    world.interact_users();

    // Update dirs to exercise parent's parent push force, sibling repulsion, and nearby repulsion
    world.update_dirs(0.1, 0.5, 0.0);

    // Test file deletion on middle directory triggering deletion of empty dir
    let d1 = world.delete_file(f1);
    assert!(d1.is_some());
    let d2 = world.delete_file(f2);
    assert!(d2.is_some());
    let d3 = world.delete_file(f3);
    assert!(d3.is_some());

    // Test user becoming inactive
    world.users[u1].pawn.elapsed = 1000.0;
    world.users[u1].last_action = 0.0;
    settings.user_idle_time = 10.0;
    let inactive = world.update_users(100.0, 0.1, &settings);
    assert!(inactive.contains(&u1));
}

#[test]
fn test_world_common_path_refactoring_and_root_files() {
    let mut world = World::new(555, 666);
    let settings = GourceSettings::default();

    // Adding file directly at root
    let cf_root = CommitFile {
        filename: "/README.md".to_string(),
        action: FileAction::Add,
        colour: Vec3::ONE,
        ..Default::default()
    };
    let rf = world.add_file(&cf_root, &settings).unwrap();
    assert_eq!(world.files[rf].path, "/");

    // Add two files sharing a subfolder to trigger commonpath refactoring
    let cf1 = CommitFile {
        filename: "/shared/sub1/a.txt".to_string(),
        action: FileAction::Add,
        colour: Vec3::ONE,
        ..Default::default()
    };
    let cf2 = CommitFile {
        filename: "/shared/sub2/b.txt".to_string(),
        action: FileAction::Add,
        colour: Vec3::ONE,
        ..Default::default()
    };
    let f1 = world.add_file(&cf1, &settings).unwrap();
    let f2 = world.add_file(&cf2, &settings).unwrap();
    assert!(world.files.contains_key(f1));
    assert!(world.files.contains_key(f2));

    // Check find_dirs
    let dirs = world.find_dirs("/shared");
    assert!(!dirs.is_empty());
}
