//! Invariant and coverage tests for World methods and tree manipulations.

use gource_model::commit::{Commit, CommitFile, FileAction};
use gource_scene::world::World;
use gource_settings::GourceSettings;

#[test]
fn test_world_user_highlight_list_and_queries() {
    let mut world = World::new(123, 31);
    let settings = GourceSettings {
        highlight_all_users: false,
        highlight_users: vec!["dave".to_string(), "".to_string()],
        ..Default::default()
    };

    let uid1 = world.add_user("alice", &settings);
    assert!(!world.users[uid1].is_highlighted());

    let uid2 = world.add_user("dave", &settings);
    assert!(world.users[uid2].is_highlighted());

    let cf = CommitFile {
        filename: "/a/b/c/file.rs".to_string(),
        action: FileAction::Add,
        colour: [1.0, 1.0, 1.0],
        ..Default::default()
    };
    let fid = world.add_file(&cf, &settings).unwrap();

    // Query non-existent dir and files recursive on nested dir
    let dirs_a = world.find_dirs("/a/");
    assert_eq!(dirs_a.len(), 1);
    let files_a = world.get_files_recursive(dirs_a[0]);
    assert_eq!(files_a, vec![fid]);

    // Query prefix that does not match
    let dirs_none = world.find_dirs("/z/");
    assert!(dirs_none.is_empty());
}

#[test]
fn test_world_action_kinds_and_colour_modes() {
    let mut world = World::new(456, 31);
    let settings = GourceSettings::default();

    let uid = world.add_user("eve", &settings);

    // Test Modify action kind
    let cf_mod = CommitFile {
        filename: "/dir/code.rs".to_string(),
        action: FileAction::Modify,
        colour: [0.5, 0.6, 0.7],
        ..Default::default()
    };
    let fid = world.add_file(&cf_mod, &settings).unwrap();

    let commit_mod = Commit {
        timestamp: 1000,
        username: "eve".to_string(),
        files: vec![cf_mod.clone()],
        ..Default::default()
    };
    world.add_file_action(&commit_mod, &cf_mod, fid, 0.0, &settings);
    assert_eq!(world.users[uid].action_count(), 1);

    // Test Other action kind
    let cf_other = CommitFile {
        filename: "/dir/other.txt".to_string(),
        action: FileAction::Other("copy".to_string()),
        colour: [0.1, 0.2, 0.3],
        ..Default::default()
    };
    let fid_other = world.add_file(&cf_other, &settings).unwrap();
    let commit_other = Commit {
        timestamp: 1001,
        username: "eve".to_string(),
        files: vec![cf_other.clone()],
        ..Default::default()
    };
    world.add_file_action(&commit_other, &cf_other, fid_other, 0.0, &settings);

    // Test None file_size_metric in add_file_action
    let settings_none = GourceSettings {
        file_size_metric: gource_settings::FileSizeMetric::None,
        ..Default::default()
    };
    world.add_file_action(&commit_other, &cf_other, fid_other, 0.0, &settings_none);
}

#[test]
fn test_world_nested_dir_reparenting_and_tree_removal() {
    let mut world = World::new(789, 31);
    let settings = GourceSettings::default();

    // Create /common/sub1/f1.rs
    let cf1 = CommitFile {
        filename: "/common/sub1/f1.rs".to_string(),
        action: FileAction::Add,
        colour: [1.0, 1.0, 1.0],
        ..Default::default()
    };
    let fid1 = world.add_file(&cf1, &settings).unwrap();

    // Create /common/sub2/f2.rs -> will create /common/ fork or node
    let cf2 = CommitFile {
        filename: "/common/sub2/f2.rs".to_string(),
        action: FileAction::Add,
        colour: [1.0, 1.0, 1.0],
        ..Default::default()
    };
    let fid2 = world.add_file(&cf2, &settings).unwrap();

    // Create /common/sub1/nested/f3.rs
    let cf3 = CommitFile {
        filename: "/common/sub1/nested/f3.rs".to_string(),
        action: FileAction::Add,
        colour: [1.0, 1.0, 1.0],
        ..Default::default()
    };
    let fid3 = world.add_file(&cf3, &settings).unwrap();

    // Remove fid3 from tree directly with /common/sub1/nested/ path
    let removed3 = world.remove_file_from_tree(world.root, fid3, "/common/sub1/nested/", true);
    assert!(removed3);

    // Remove with non-matching path returns false
    let not_removed = world.remove_file_from_tree(world.root, fid1, "/nonexistent/", true);
    assert!(!not_removed);

    // Remove fid1 from /common/sub1/
    let removed1 = world.remove_file_from_tree(world.root, fid1, "/common/sub1/", true);
    assert!(removed1);

    // Remove fid2 from /common/sub2/
    let removed2 = world.remove_file_from_tree(world.root, fid2, "/common/sub2/", true);
    assert!(removed2);
}
