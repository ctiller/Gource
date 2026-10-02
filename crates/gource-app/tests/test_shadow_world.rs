//! Tests for shadow files and solidification in Gource simulation.

use gource_app::WorldDraw;
use gource_app::world::World;
use gource_core::{Vec2, Vec3};
use gource_draw::DrawList;
use gource_settings::GourceSettings;
use gource_vcs::commit::{Commit, CommitFile, FileAction};

#[test]
fn test_shadow_file_creation_and_solidification() {
    let mut world = World::new(42, 101);
    let settings = GourceSettings {
        shadow_alpha: 0.4,
        ..Default::default()
    };

    // 1. Add shadow file from worktree author
    let cf_shadow = CommitFile {
        filename: "/src/feature.rs".to_string(),
        action: FileAction::Add,
        colour: <[f32; 3]>::from(Vec3::new(0.2, 0.8, 0.3)),
        is_shadow: true,
        ..Default::default()
    };

    let fid = world
        .add_file(&cf_shadow, &settings)
        .expect("shadow file added");
    world.files[fid].pawn.set_hidden(false);
    world.files[fid].pawn.elapsed = 1.0;
    assert!(world.files[fid].is_shadow);
    assert_eq!(world.files[fid].shadow_alpha, 0.4);

    let shadow_commit = Commit {
        timestamp: 1000,
        username: "worktree:feat-branch".to_string(),
        files: vec![cf_shadow.clone()],
        is_shadow: true,
    };

    world.add_file_action(&shadow_commit, &cf_shadow, fid, 0.0, &settings);
    let alpha_initial = world.files[fid].alpha();
    assert!(
        alpha_initial <= 0.45,
        "shadow file initial alpha should be <= 0.45, got {alpha_initial}"
    );

    // Step logic: file should remain shadow
    world.files[fid].logic(0.1, 0.0);
    assert!(world.files[fid].is_shadow);

    // 2. Real commit solidifies the shadow file
    world.files[fid].solidify();
    assert!(!world.files[fid].is_shadow);
    assert!(world.files[fid].solidifying);
    assert!(world.files[fid].pulse_timer > 0.0, "pulse burst triggered");

    // Advance logic through solidification timer (0.5s)
    world.files[fid].logic(0.6, 0.0);
    assert!(!world.files[fid].solidifying);
    assert_eq!(world.files[fid].solidify_timer, 0.0);
    assert!(
        world.files[fid].alpha() >= 0.95,
        "solidified file alpha should be fully opaque"
    );
}

#[test]
fn test_shadow_file_revert_dissolves_file() {
    let mut world = World::new(42, 101);
    let settings = GourceSettings::default();

    let cf_shadow = CommitFile {
        filename: "/src/abandoned.rs".to_string(),
        action: FileAction::Add,
        colour: <[f32; 3]>::from(Vec3::ONE),
        is_shadow: true,
        ..Default::default()
    };

    let fid = world.add_file(&cf_shadow, &settings).unwrap();
    world.files[fid].pawn.set_hidden(false);
    world.files[fid].pawn.elapsed = 1.0;
    assert!(world.files[fid].is_shadow);

    // When revert occurs, remove_forced is called
    world.files[fid].remove_forced();
    assert!(world.files[fid].removing);
    assert!(world.files[fid].forced_removal);
    assert!(world.files[fid].fade_start >= 0.0);

    // Step logic 1.1s to expire
    let just_expired = world.files[fid].logic(1.1, 0.0);
    assert!(just_expired);
    assert!(world.files[fid].expired);
}

#[test]
fn test_draw_scene_with_shadow_file_and_beam() {
    let mut world = World::new(42, 101);
    let settings = GourceSettings {
        shadow_alpha: 0.4,
        ..Default::default()
    };

    let cf = CommitFile {
        filename: "/src/mod.rs".to_string(),
        action: FileAction::Add,
        colour: <[f32; 3]>::from(Vec3::new(0.5, 0.8, 0.2)),
        is_shadow: true,
        ..Default::default()
    };

    let fid = world.add_file(&cf, &settings).unwrap();
    world.files[fid].pawn.set_hidden(false);

    let commit = Commit {
        timestamp: 1000,
        username: "worktree:main".to_string(),
        files: vec![cf.clone()],
        is_shadow: true,
    };
    world.add_file_action(&commit, &cf, fid, 0.0, &settings);

    let mut list = DrawList::new(gource_core::UVec2::new(1280, 720));
    let proj = gource_draw::Projection::new(Vec3::new(0.0, 0.0, -500.0), Vec2::new(1280.0, 720.0));

    let textures = gource_app::world::SceneTextures {
        file: gource_draw::TextureId(1),
        beam: gource_draw::TextureId(2),
        default_user: gource_draw::TextureId(3),
    };

    world.prepare_frame(&proj, &settings);
    world.draw_scene(&mut list, &proj, &settings, &textures);
    assert!(!list.is_empty());
}
