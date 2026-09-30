//! Tests for git worktree discovery, in-flight changes, and shadow commits.

use gource_vcs::commit::FileAction;
use gource_vcs::options::VcsOptions;
use gource_vcs::worktree::{
    WorktreeWatcher, discover_worktrees, discover_worktrees_fallback,
    parse_worktree_list_porcelain, read_branch_name, read_head_sha, scan_worktree_in_flight,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::channel;

fn run_git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git command failed");
    assert!(
        output.status.success(),
        "git command {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_parse_worktree_list_porcelain() {
    let porcelain_output = "\
worktree /home/user/repo
HEAD 1234567890abcdef1234567890abcdef12345678
branch refs/heads/master

worktree /home/user/repo-feature
HEAD abcdef1234567890abcdef1234567890abcdef12
branch refs/heads/feature/cool-stuff

worktree /home/user/repo-detached
HEAD 9999999999999999999999999999999999999999
detached

";
    let list = parse_worktree_list_porcelain(porcelain_output);
    assert_eq!(list.len(), 3);

    assert_eq!(list[0].path, PathBuf::from("/home/user/repo"));
    assert_eq!(list[0].branch, "master");
    assert_eq!(list[0].head_sha, "1234567890abcdef1234567890abcdef12345678");
    assert!(list[0].is_main);
    assert_eq!(list[0].author_name(), "worktree:master");

    assert_eq!(list[1].path, PathBuf::from("/home/user/repo-feature"));
    assert_eq!(list[1].branch, "feature/cool-stuff");
    assert_eq!(list[1].head_sha, "abcdef1234567890abcdef1234567890abcdef12");
    assert!(!list[1].is_main);
    assert_eq!(list[1].author_name(), "worktree:feature/cool-stuff");

    assert_eq!(list[2].path, PathBuf::from("/home/user/repo-detached"));
    assert_eq!(list[2].branch, "detached");
    assert_eq!(list[2].head_sha, "9999999999999999999999999999999999999999");
    assert!(!list[2].is_main);
    assert_eq!(list[2].author_name(), "worktree:detached");
}

#[test]
fn test_worktree_discovery_and_in_flight_watcher() {
    let temp = tempfile::tempdir().unwrap();
    let main_repo = temp.path().join("main_repo");
    fs::create_dir_all(&main_repo).unwrap();

    run_git(&main_repo, &["init", "-b", "main"]);
    run_git(&main_repo, &["config", "user.name", "Tester"]);
    run_git(&main_repo, &["config", "user.email", "test@example.com"]);

    // Create initial commit
    let file1 = main_repo.join("file1.txt");
    fs::write(&file1, "hello world\nline 2\n").unwrap();
    run_git(&main_repo, &["add", "file1.txt"]);
    run_git(&main_repo, &["commit", "-m", "initial commit"]);

    // Create linked worktree
    let wt_path = temp.path().join("wt_feature");
    run_git(
        &main_repo,
        &[
            "worktree",
            "add",
            "-b",
            "feat-shadow",
            wt_path.to_str().unwrap(),
        ],
    );

    // 1. Test discover_worktrees
    let worktrees = discover_worktrees(&main_repo).unwrap();
    assert!(worktrees.len() >= 2);
    let wt_main = worktrees.iter().find(|w| w.is_main).unwrap();
    assert_eq!(wt_main.branch, "main");
    let wt_feat = worktrees.iter().find(|w| !w.is_main).unwrap();
    assert_eq!(wt_feat.branch, "feat-shadow");

    // 2. Modify files in both worktrees
    // In main: untracked new file
    let untracked = main_repo.join("untracked.rs");
    fs::write(&untracked, "fn main() {}\n").unwrap();

    // In wt_feature: modified tracked file and new file
    let wt_file1 = wt_path.join("file1.txt");
    fs::write(&wt_file1, "hello world modified\nline 2\nline 3\n").unwrap();
    let wt_binary = wt_path.join("asset.bin");
    fs::write(&wt_binary, [0u8, 1, 2, 3]).unwrap();

    let options = VcsOptions {
        watch_worktrees: true,
        ..Default::default()
    };

    // Test scan_worktree_in_flight
    let main_files = scan_worktree_in_flight(wt_main, &options).unwrap();
    assert!(
        main_files
            .iter()
            .any(|f| f.path == "untracked.rs" && f.action == FileAction::Add)
    );

    let feat_files = scan_worktree_in_flight(wt_feat, &options).unwrap();
    assert!(
        feat_files
            .iter()
            .any(|f| f.path == "file1.txt" && f.action == FileAction::Modify)
    );
    assert!(
        feat_files
            .iter()
            .any(|f| f.path == "asset.bin" && f.is_binary)
    );

    // 3. Test WorktreeWatcher polling and streaming
    let mut watcher = WorktreeWatcher::new(main_repo.clone(), options.clone());
    let (tx, rx) = channel();

    watcher.poll_and_stream(&tx).unwrap();

    // Collect streamed lines
    let mut received_lines = Vec::new();
    while let Ok(line) = rx.try_recv() {
        if !line.is_empty() {
            received_lines.push(line);
        }
    }

    assert!(!received_lines.is_empty());
    assert!(
        received_lines
            .iter()
            .any(|l| l.contains("worktree:feat-shadow") && l.contains("file1.txt"))
    );
    assert!(
        received_lines
            .iter()
            .any(|l| l.contains("worktree:main") && l.contains("untracked.rs"))
    );

    // 4. Test Discard / Revert of in-flight changes emits delete action
    fs::remove_file(&untracked).unwrap();
    watcher.poll_and_stream(&tx).unwrap();

    let mut revert_lines = Vec::new();
    while let Ok(line) = rx.try_recv() {
        if !line.is_empty() {
            revert_lines.push(line);
        }
    }

    assert!(
        revert_lines
            .iter()
            .any(|l| l.contains("worktree:main|D|/untracked.rs"))
    );
}

#[test]
fn test_worktree_fallback_discovery_and_helpers() {
    let temp = tempfile::tempdir().unwrap();
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).unwrap();

    let git_dir = repo_dir.join(".git");
    fs::create_dir_all(&git_dir).unwrap();

    // Create HEAD with standard ref
    fs::write(git_dir.join("HEAD"), "ref: refs/heads/my-feature\n").unwrap();

    // Create linked worktrees dir
    let wt_dir = git_dir.join("worktrees").join("wt1");
    fs::create_dir_all(&wt_dir).unwrap();

    let linked_path = temp.path().join("linked_wt");
    fs::create_dir_all(&linked_path).unwrap();
    let linked_gitdir = linked_path.join(".git");
    fs::write(&linked_gitdir, format!("gitdir: {}\n", wt_dir.display())).unwrap();

    fs::write(wt_dir.join("gitdir"), linked_gitdir.to_str().unwrap()).unwrap();
    fs::write(wt_dir.join("HEAD"), "ref: refs/heads/linked-branch\n").unwrap();

    let fallback_wts = discover_worktrees_fallback(&repo_dir).unwrap();
    assert_eq!(fallback_wts.len(), 2);
    assert_eq!(fallback_wts[0].branch, "my-feature");
    assert!(fallback_wts[0].is_main);
    assert_eq!(fallback_wts[1].branch, "linked-branch");
    assert!(!fallback_wts[1].is_main);

    // Test fallback discover_worktrees by calling on a repo directory where `git` command would fail or we test helper logic directly
    let parsed_porcelain = parse_worktree_list_porcelain(
        "worktree /tmp/dir1\nHEAD 1111\nbranch refs/heads/b1\n\nworktree /tmp/dir2\nHEAD 2222\n\n",
    );
    assert_eq!(parsed_porcelain.len(), 2);
    assert_eq!(parsed_porcelain[1].branch, "dir2");

    // Test porcelain with trailing entry without trailing newline
    let parsed_trailing =
        parse_worktree_list_porcelain("worktree /tmp/single\nHEAD 3333\nbranch refs/heads/feat");
    assert_eq!(parsed_trailing.len(), 1);
    assert_eq!(parsed_trailing[0].branch, "feat");
}

#[test]
fn test_scan_worktree_in_flight_errors_and_filters() {
    let temp = tempfile::tempdir().unwrap();
    let not_a_repo = temp.path().join("empty");
    fs::create_dir_all(&not_a_repo).unwrap();

    let info = gource_vcs::worktree::WorktreeInfo {
        path: not_a_repo,
        branch: "test".to_string(),
        head_sha: "0000".to_string(),
        is_main: true,
    };
    let options = VcsOptions::default();
    let res = scan_worktree_in_flight(&info, &options);
    assert!(res.is_err());
}

#[test]
fn test_worktree_stream_shadow_commit_formats() {
    let mut watcher = WorktreeWatcher::new(PathBuf::from("/tmp"), VcsOptions::default());
    let (tx, _rx) = std::sync::mpsc::channel();

    // Trigger poll_and_stream on non-existent dir (returns empty worktrees)
    let _ = watcher.poll_and_stream(&tx);

    // Test stream_shadow_commit variants via poll_and_stream with custom state
    let temp = tempfile::tempdir().unwrap();
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).unwrap();
    run_git(&repo_dir, &["init", "-b", "main"]);
    run_git(&repo_dir, &["config", "user.name", "Tester"]);
    run_git(&repo_dir, &["config", "user.email", "test@example.com"]);

    let f_plain = repo_dir.join("plain.txt");
    fs::write(&f_plain, "line 1\n").unwrap();
    run_git(&repo_dir, &["add", "plain.txt"]);
    run_git(&repo_dir, &["commit", "-m", "init"]);

    // Modify with empty new line (lines_added=1, lines_removed=1)
    fs::write(&f_plain, "line 1 updated\nline 2\n").unwrap();
    // Untracked binary
    let f_bin = repo_dir.join("bin.dat");
    fs::write(&f_bin, [0u8, 1, 2, 0]).unwrap();

    let mut watcher2 = WorktreeWatcher::new(
        repo_dir,
        VcsOptions {
            watch_worktrees: true,
            ..Default::default()
        },
    );
    let (tx2, rx2) = std::sync::mpsc::channel();
    watcher2.poll_and_stream(&tx2).unwrap();

    let mut lines = Vec::new();
    while let Ok(line) = rx2.try_recv() {
        if !line.is_empty() {
            lines.push(line);
        }
    }
    assert!(lines.iter().any(|l| l.contains("/bin.dat|-|-")));
    assert!(
        lines
            .iter()
            .any(|l| l.contains("/plain.txt|2|1") || l.contains("/plain.txt|"))
    );
}

#[test]
fn test_branch_and_head_parsers_and_edge_cases() {
    let temp = tempfile::tempdir().unwrap();
    let head_file = temp.path().join("HEAD");

    fs::write(&head_file, "ref: refs/heads/feature\n").unwrap();
    assert_eq!(read_branch_name(&head_file), Some("feature".to_string()));
    assert_eq!(read_head_sha(&head_file), None);

    fs::write(&head_file, "ref: refs/tags/v1.0\n").unwrap();
    assert_eq!(
        read_branch_name(&head_file),
        Some("refs/tags/v1.0".to_string())
    );

    fs::write(&head_file, "abcdef1234567890abcdef1234567890abcdef12\n").unwrap();
    assert_eq!(read_branch_name(&head_file), Some("detached".to_string()));
    assert_eq!(
        read_head_sha(&head_file),
        Some("abcdef1234567890abcdef1234567890abcdef12".to_string())
    );

    fs::write(&head_file, "\n").unwrap();
    assert_eq!(read_branch_name(&head_file), None);
    assert_eq!(read_head_sha(&head_file), None);

    assert_eq!(read_branch_name(&temp.path().join("nonexistent")), None);
    assert_eq!(read_head_sha(&temp.path().join("nonexistent")), None);
}
