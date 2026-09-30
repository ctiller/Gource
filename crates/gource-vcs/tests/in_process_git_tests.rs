use gource_vcs::options::VcsOptions;
use gource_vcs::{LogMill, in_process_git::generate_in_process_git_log};
use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn run_git(dir: &std::path::Path, args: &[&str]) {
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
fn test_in_process_git_empty_repo() {
    let temp = TempDir::new().unwrap();
    run_git(temp.path(), &["init", "--ref-format=files"]);

    let options = VcsOptions {
        git_backend: "in-process".to_string(),
        ..Default::default()
    };

    let log_file = generate_in_process_git_log(temp.path(), &options).unwrap();
    let content = fs::read_to_string(log_file.path()).unwrap();
    assert!(content.is_empty(), "empty repo should produce empty log");
}

#[test]
fn test_in_process_git_not_a_repo() {
    let temp = TempDir::new().unwrap();
    let options = VcsOptions {
        git_backend: "in-process".to_string(),
        ..Default::default()
    };
    let res = generate_in_process_git_log(temp.path(), &options);
    assert!(res.is_err(), "non-git dir should return Err");
}

#[test]
fn test_in_process_git_commits_and_numstat() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    run_git(dir, &["init", "--ref-format=files"]);
    run_git(dir, &["config", "user.name", "Test Author"]);
    run_git(dir, &["config", "user.email", "test@example.com"]);
    run_git(dir, &["config", "commit.gpgsign", "false"]);

    // Commit 1: Add file1.txt and binary.bin
    fs::write(dir.join("file1.txt"), "hello\nworld\n").unwrap();
    fs::write(dir.join("binary.bin"), b"binary\0content").unwrap();
    run_git(dir, &["add", "."]);
    run_git(dir, &["commit", "-m", "Initial commit"]);

    // Commit 2: Modify file1.txt and add sub/file2.txt
    fs::create_dir_all(dir.join("sub")).unwrap();
    fs::write(dir.join("file1.txt"), "hello\nbrave new\nworld\n").unwrap();
    fs::write(dir.join("sub/file2.txt"), "line1\nline2\n").unwrap();
    run_git(dir, &["add", "."]);
    run_git(dir, &["commit", "-m", "Second commit"]);

    // Commit 3: Delete binary.bin and modify sub/file2.txt
    fs::remove_file(dir.join("binary.bin")).unwrap();
    fs::write(dir.join("sub/file2.txt"), "line1\n").unwrap();
    run_git(dir, &["add", "."]);
    run_git(dir, &["commit", "-m", "Third commit"]);

    let mut options = VcsOptions {
        git_backend: "in-process".to_string(),
        include_numstat: true,
        author_time: true,
        ..Default::default()
    };

    let log_file = generate_in_process_git_log(dir, &options).unwrap();
    let content = fs::read_to_string(log_file.path()).unwrap();
    assert!(content.contains("user:Test Author"));
    assert!(content.contains("file1.txt"));
    assert!(content.contains("sub/file2.txt"));
    assert!(content.contains("binary.bin"));

    // Also test LogMill fetch_blocking with in-process git backend
    let mut commit_log = LogMill::fetch_blocking(dir.to_str().unwrap(), &options).unwrap();
    let mut commit_count = 0;
    while let Some(commit) = commit_log.next_commit() {
        commit_count += 1;
        assert_eq!(commit.username, "Test Author");
        assert!(!commit.files.is_empty());
    }
    assert_eq!(commit_count, 3, "expected 3 commits from in-process git");

    // Test with filters: ignore file1.txt
    options
        .filters
        .file_filters
        .push(fancy_regex::Regex::new("file1\\.txt").unwrap());
    let mut commit_log2 = LogMill::fetch_blocking(dir.to_str().unwrap(), &options).unwrap();
    while let Some(commit) = commit_log2.next_commit() {
        for f in commit.files {
            assert!(!f.filename.ends_with("file1.txt"));
        }
    }

    // Test with timestamp filtering
    let mut time_options = options.clone();
    time_options.start_timestamp = 2000000000; // Far future
    let log_empty = generate_in_process_git_log(dir, &time_options).unwrap();
    assert!(fs::read_to_string(log_empty.path()).unwrap().is_empty());

    time_options.start_timestamp = 0;
    time_options.stop_timestamp = 1000; // Far past
    let log_empty2 = generate_in_process_git_log(dir, &time_options).unwrap();
    assert!(fs::read_to_string(log_empty2.path()).unwrap().is_empty());

    // Test with user filter
    let mut user_options = options.clone();
    user_options
        .filters
        .user_filters
        .push(fancy_regex::Regex::new("Test Author").unwrap());
    let log_no_user = generate_in_process_git_log(dir, &user_options).unwrap();
    assert!(fs::read_to_string(log_no_user.path()).unwrap().is_empty());

    // Test explicit branch option
    let mut branch_options = options.clone();
    branch_options.git_branch = "HEAD".to_string();
    let log_branch = generate_in_process_git_log(dir, &branch_options).unwrap();
    assert!(!fs::read_to_string(log_branch.path()).unwrap().is_empty());

    // Test without numstat
    let mut no_numstat_options = options.clone();
    no_numstat_options.include_numstat = false;
    let log_no_numstat = generate_in_process_git_log(dir, &no_numstat_options).unwrap();
    let no_numstat_content = fs::read_to_string(log_no_numstat.path()).unwrap();
    assert!(no_numstat_content.contains(":100644 100644 0000000 0000000 A\tfile1.txt"));

    // Test author_time = false (commit time)
    let mut commit_time_options = options.clone();
    commit_time_options.author_time = false;
    let log_commit_time = generate_in_process_git_log(dir, &commit_time_options).unwrap();
    assert!(
        !fs::read_to_string(log_commit_time.path())
            .unwrap()
            .is_empty()
    );

    // Test invalid branch option
    branch_options.git_branch = "non_existent_branch_12345".to_string();
    assert!(generate_in_process_git_log(dir, &branch_options).is_err());
}

#[test]
fn test_in_process_git_deep_trees_and_deletions() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    run_git(dir, &["init", "--ref-format=files"]);
    run_git(dir, &["config", "user.name", "Deep Author"]);
    run_git(dir, &["config", "user.email", "deep@example.com"]);
    run_git(dir, &["config", "commit.gpgsign", "false"]);

    // Commit 1: Deep tree with multiple subdirectories
    fs::create_dir_all(dir.join("a/b/c")).unwrap();
    fs::write(dir.join("a/b/c/nested.txt"), "line1\nline2\nline3\n").unwrap();
    fs::write(dir.join("a/top.txt"), "top\n").unwrap();
    run_git(dir, &["add", "."]);
    run_git(dir, &["commit", "-m", "Nested commit"]);

    // Commit 2: Modify deep file with lines replaced, add new file, delete top
    fs::write(
        dir.join("a/b/c/nested.txt"),
        "line1\nmodified\nline3\nline4\n",
    )
    .unwrap();
    fs::remove_file(dir.join("a/top.txt")).unwrap();
    fs::write(dir.join("a/b/c/other.bin"), b"data\0byte").unwrap();
    run_git(dir, &["add", "."]);
    run_git(dir, &["commit", "-m", "Modified nested and deleted top"]);

    // Commit 3: Delete the binary file
    fs::remove_file(dir.join("a/b/c/other.bin")).unwrap();
    run_git(dir, &["add", "."]);
    run_git(dir, &["commit", "-m", "Deleted other.bin"]);

    let options = VcsOptions {
        git_backend: "in-process".to_string(),
        include_numstat: true,
        ..Default::default()
    };

    let log_file = generate_in_process_git_log(dir, &options).unwrap();
    let content = fs::read_to_string(log_file.path()).unwrap();
    assert!(content.contains("a/b/c/nested.txt"));
    assert!(content.contains("a/top.txt"));
    assert!(content.contains("a/b/c/other.bin"));

    let mut commit_log = LogMill::fetch_blocking(dir.to_str().unwrap(), &options).unwrap();
    let mut count = 0;
    while let Some(c) = commit_log.next_commit() {
        count += 1;
        assert_eq!(c.username, "Deep Author");
    }
    assert_eq!(count, 3);
}

#[test]
fn test_in_process_git_empty_commit_and_detached_head() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();

    run_git(dir, &["init", "--ref-format=files"]);
    run_git(dir, &["config", "user.name", "Detached Author"]);
    run_git(dir, &["config", "user.email", "detached@example.com"]);
    run_git(dir, &["config", "commit.gpgsign", "false"]);

    fs::write(dir.join("file.txt"), "hello\n").unwrap();
    run_git(dir, &["add", "."]);
    run_git(dir, &["commit", "-m", "Commit 1"]);

    // Add empty commit (has changes.is_empty())
    run_git(dir, &["commit", "--allow-empty", "-m", "Empty commit"]);

    // Detach HEAD
    let rev_output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .unwrap();
    let head_sha = String::from_utf8(rev_output.stdout)
        .unwrap()
        .trim()
        .to_string();
    run_git(dir, &["checkout", &head_sha]);

    let options = VcsOptions {
        git_backend: "in-process".to_string(),
        include_numstat: true,
        ..Default::default()
    };

    let log_file = generate_in_process_git_log(dir, &options).unwrap();
    let content = fs::read_to_string(log_file.path()).unwrap();
    assert!(content.contains("file.txt"));
}
