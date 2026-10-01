//! Directory labels (`path_token`) must match C++ `RDirNode::adjustPath`.

use glam::Vec3;
use gource_settings::GourceSettings;
use gource_sim::world::World;
use gource_vcs::commit::{CommitFile, FileAction};

fn add(world: &mut World, settings: &GourceSettings, path: &str) {
    let cf = CommitFile {
        filename: path.to_string(),
        action: FileAction::Add,
        colour: <[f32; 3]>::from(Vec3::ONE),
        ..Default::default()
    };
    world.add_file(&cf, settings).expect("file added");
}

fn token(world: &World, dir: &str) -> String {
    let did = world.dir_map[dir];
    world.dirs[did].path_token.clone()
}

#[test]
fn test_dir_labels_have_no_leading_slash() {
    let mut world = World::new(1, 1);
    let settings = GourceSettings::default();
    add(&mut world, &settings, "/README");
    add(&mut world, &settings, "/src/main.cpp");
    add(&mut world, &settings, "/src/core/display.cpp");
    add(&mut world, &settings, "/data/fonts/FreeSans.ttf");

    // C++ gives the root path_token_offset = 1 ("/".size()), so a top-level
    // dir "/src/" is labelled "src", not "/src" (src/dirnode.cpp adjustPath).
    assert_eq!(world.dirs[world.root].token_offset(), 1);
    assert_eq!(token(&world, "/src/"), "src");
    assert_eq!(token(&world, "/src/core/"), "core");
    assert_eq!(token(&world, "/data/fonts/"), "data/fonts");
}

#[test]
fn test_dir_labels_after_root_rename_match_cpp() {
    // C++ quirk: when the first file is in a subdirectory, the empty root is
    // renamed to that directory (RDirNode::addFile -> changePath) but keeps
    // the token offset of "/". Its children are labelled from offset 1, and
    // adjustPath isn't recursive, so they keep that label after the root is
    // forked back to "/".
    let mut world = World::new(1, 1);
    let settings = GourceSettings::default();
    add(&mut world, &settings, "/src/main.cpp");
    add(&mut world, &settings, "/src/core/display.cpp");
    add(&mut world, &settings, "/data/x.txt");

    assert_eq!(token(&world, "/src/"), "src");
    assert_eq!(token(&world, "/src/core/"), "src/core");
    assert_eq!(token(&world, "/data/"), "data");
}
