//! Integration and unit tests for live git mode and GitHub watch mode.

use gource_vcs::{
    DefaultHttpTransport, GitHubTarget, GitHubWatcher, LiveGitWatcher, LogMill, VcsOptions,
    resolve_github_token,
};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[test]
fn test_github_target_parse() {
    // Valid repository forms
    assert_eq!(
        GitHubTarget::parse("torvalds/linux").unwrap(),
        GitHubTarget::Repo {
            owner: "torvalds".to_string(),
            repo: "linux".to_string(),
        }
    );
    assert_eq!(
        GitHubTarget::parse("https://github.com/torvalds/linux.git").unwrap(),
        GitHubTarget::Repo {
            owner: "torvalds".to_string(),
            repo: "linux".to_string(),
        }
    );
    assert_eq!(
        GitHubTarget::parse("github:owner-name/repo_name.1").unwrap(),
        GitHubTarget::Repo {
            owner: "owner-name".to_string(),
            repo: "repo_name.1".to_string(),
        }
    );

    // Valid owner forms
    assert_eq!(
        GitHubTarget::parse("rust-lang").unwrap(),
        GitHubTarget::Owner {
            owner: "rust-lang".to_string(),
        }
    );
    assert_eq!(
        GitHubTarget::parse("rust-lang/*").unwrap(),
        GitHubTarget::Owner {
            owner: "rust-lang".to_string(),
        }
    );
    assert_eq!(
        GitHubTarget::parse("https://github.com/rust-lang/").unwrap(),
        GitHubTarget::Owner {
            owner: "rust-lang".to_string(),
        }
    );

    // Invalid forms
    assert!(GitHubTarget::parse("").is_err());
    assert!(GitHubTarget::parse("..").is_err());
    assert!(GitHubTarget::parse("../repo").is_err());
    assert!(GitHubTarget::parse("owner/..").is_err());
    assert!(GitHubTarget::parse("-flag/repo").is_err());
    assert!(GitHubTarget::parse("owner/-flag").is_err());
    assert!(GitHubTarget::parse("a/b/c").is_err());
    assert!(GitHubTarget::parse("invalid name with spaces/repo").is_err());

    // looks_like_github_path
    assert!(GitHubTarget::looks_like_github_path("github:foo/bar"));
    assert!(GitHubTarget::looks_like_github_path(
        "https://github.com/foo/bar"
    ));
    assert!(GitHubTarget::looks_like_github_path(
        "http://github.com/foo/bar"
    ));
    assert!(!GitHubTarget::looks_like_github_path("local/path"));
    assert!(!GitHubTarget::looks_like_github_path("."));
}

#[test]
fn test_resolve_github_token() {
    // Explicit token
    assert_eq!(
        resolve_github_token("  secret_token  "),
        Some("secret_token".to_string())
    );
}

#[test]
fn test_github_watcher_repo_mock_server() {
    // Spin up local mock server on 127.0.0.1:0
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://127.0.0.1:{}", addr.port());

    let poll_count = Arc::new(Mutex::new(0));
    let poll_count_server = Arc::clone(&poll_count);
    let server_shutdown = Arc::new(AtomicBool::new(false));
    let server_shutdown_clone = Arc::clone(&server_shutdown);

    let server_thread = thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        while !server_shutdown_clone.load(Ordering::SeqCst) {
            let (mut socket, _) = match listener.accept() {
                Ok(conn) => conn,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(_) => break,
            };

            let mut req_buf = [0u8; 4096];
            let n = socket.read(&mut req_buf).unwrap_or(0);
            let req_str = String::from_utf8_lossy(&req_buf[..n]);

            let mut count = poll_count_server.lock().unwrap();
            *count += 1;
            let current_poll = *count;
            drop(count);

            if req_str.contains("/commits?per_page=25") {
                if req_str.contains("If-None-Match: \"etag-v1\"") && current_poll == 2 {
                    // Poll 2: 304 Not Modified
                    let resp = "HTTP/1.1 304 Not Modified\r\nETag: \"etag-v1\"\r\nConnection: close\r\n\r\n";
                    let _ = socket.write_all(resp.as_bytes());
                } else if current_poll >= 3 {
                    // Poll 3: New commit arrives with new ETag
                    let body = r#"[
                        {"sha": "sha2", "commit": {"author": {"name": "Bob", "date": "2020-01-01T13:00:00Z"}}},
                        {"sha": "sha1", "commit": {"author": {"name": "Alice", "date": "2020-01-01T12:00:00Z"}}}
                    ]"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nETag: \"etag-v2\"\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes());
                } else {
                    // Poll 1: Initial commit
                    let body = r#"[
                        {"sha": "sha1", "commit": {"author": {"name": "Alice", "date": "2020-01-01T12:00:00Z"}}}
                    ]"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nETag: \"etag-v1\"\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes());
                }
            } else if req_str.contains("/commits/sha1") {
                let body = r#"{
                    "sha": "sha1",
                    "commit": {"author": {"name": "Alice", "date": "2020-01-01T12:00:00Z"}},
                    "files": [{"filename": "src/main.rs", "status": "added", "additions": 10, "deletions": 0}]
                }"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(resp.as_bytes());
            } else if req_str.contains("/commits/sha2") {
                let body = r#"{
                    "sha": "sha2",
                    "commit": {"author": {"name": "Bob", "date": "2020-01-01T13:00:00Z"}},
                    "files": [{"filename": "src/lib.rs", "status": "modified", "additions": 5, "deletions": 2}]
                }"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(resp.as_bytes());
            }
        }
    });

    let target = GitHubTarget::Repo {
        owner: "testowner".to_string(),
        repo: "testrepo".to_string(),
    };

    let opts = VcsOptions {
        live: true,
        live_interval_secs: 0.1,
        ..Default::default()
    };

    let abort_flag = Arc::new(AtomicBool::new(false));
    let transport = Arc::new(DefaultHttpTransport);

    let mut clog = GitHubWatcher::spawn_with_transport(
        target,
        base_url,
        transport,
        opts,
        Arc::clone(&abort_flag),
    )
    .unwrap();

    // Enable blocking on stream read
    clog.wait_for_input(true);

    // Commit 1 should arrive from initial poll
    let c1 = clog.next_commit().expect("expected commit 1");
    assert_eq!(c1.username, "Alice");
    assert_eq!(c1.files[0].filename, "/src/main.rs");
    assert_eq!(c1.files[0].lines_added, Some(10));

    // Commit 2 should arrive after poll 3
    let c2 = clog.next_commit().expect("expected commit 2");
    assert_eq!(c2.username, "Bob");
    assert_eq!(c2.files[0].filename, "/src/lib.rs");
    assert_eq!(c2.files[0].lines_added, Some(5));

    // Stop watcher & mock server
    abort_flag.store(true, Ordering::SeqCst);
    server_shutdown.store(true, Ordering::SeqCst);
    let _ = server_thread.join();
}

#[test]
fn test_github_watcher_owner_mock_server() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://127.0.0.1:{}", addr.port());

    let server_shutdown = Arc::new(AtomicBool::new(false));
    let server_shutdown_clone = Arc::clone(&server_shutdown);

    let server_thread = thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        while !server_shutdown_clone.load(Ordering::SeqCst) {
            let (mut socket, _) = match listener.accept() {
                Ok(conn) => conn,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(_) => break,
            };

            let mut req_buf = [0u8; 4096];
            let n = socket.read(&mut req_buf).unwrap_or(0);
            let req_str = String::from_utf8_lossy(&req_buf[..n]);

            if req_str.contains("/users/testowner/events") {
                let body = r#"[
                    {
                        "type": "PushEvent",
                        "repo": {"name": "testowner/proj1"},
                        "payload": {
                            "commits": [
                                {"sha": "sha_owner1"}
                            ]
                        }
                    }
                ]"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(resp.as_bytes());
            } else if req_str.contains("/repos/testowner/proj1/commits/sha_owner1") {
                let body = r#"{
                    "sha": "sha_owner1",
                    "commit": {"author": {"name": "Charlie", "date": "2020-01-01T15:00:00Z"}},
                    "files": [{"filename": "readme.md", "status": "added", "additions": 1, "deletions": 0}]
                }"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(resp.as_bytes());
            }
        }
    });

    let target = GitHubTarget::Owner {
        owner: "testowner".to_string(),
    };

    let opts = VcsOptions {
        live: false,
        ..Default::default()
    };

    let abort_flag = Arc::new(AtomicBool::new(false));
    let transport = Arc::new(DefaultHttpTransport);

    let mut clog = GitHubWatcher::spawn_with_transport(
        target,
        base_url,
        transport,
        opts,
        Arc::clone(&abort_flag),
    )
    .unwrap();

    clog.wait_for_input(true);
    let c = clog.next_commit().expect("expected owner commit");
    assert_eq!(c.username, "Charlie");
    // Verify file is prefixed with repo short name
    assert_eq!(c.files[0].filename, "/proj1/readme.md");

    abort_flag.store(true, Ordering::SeqCst);
    server_shutdown.store(true, Ordering::SeqCst);
    let _ = server_thread.join();
}

#[test]
fn test_live_git_watcher_new_commit_detection() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_path = temp_dir.path();

    // 1. Initialize git repository
    let run_git = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(repo_path)
            .status()
            .expect("git execution failed");
        assert!(status.success(), "git {:?} failed", args);
    };

    run_git(&["init", "-b", "main"]);
    run_git(&["config", "user.name", "Tester"]);
    run_git(&["config", "user.email", "tester@example.com"]);
    run_git(&["config", "commit.gpgsign", "false"]);

    // Initial commit 1
    std::fs::write(repo_path.join("file1.txt"), "hello\n").unwrap();
    run_git(&["add", "file1.txt"]);
    run_git(&["commit", "-m", "Commit 1"]);

    // Initial commit 2
    std::fs::write(repo_path.join("file2.txt"), "world\n").unwrap();
    run_git(&["add", "file2.txt"]);
    run_git(&["commit", "-m", "Commit 2"]);

    let opts = VcsOptions {
        live: true,
        live_interval_secs: 0.1,
        include_numstat: true,
        ..Default::default()
    };

    let abort_flag = Arc::new(AtomicBool::new(false));

    // Spawn live watcher
    let mut clog =
        LiveGitWatcher::spawn(repo_path.to_path_buf(), opts, Arc::clone(&abort_flag)).unwrap();

    assert!(clog.is_live());
    clog.wait_for_input(true);

    // Read initial 2 commits
    let c1 = clog.next_commit().expect("expected commit 1");
    assert_eq!(c1.files[0].filename, "/file1.txt");

    let c2 = clog.next_commit().expect("expected commit 2");
    assert_eq!(c2.files[0].filename, "/file2.txt");

    // Add a 3rd commit to git repository dynamically
    std::fs::write(repo_path.join("file3.txt"), "live\n").unwrap();
    run_git(&["add", "file3.txt"]);
    run_git(&["commit", "-m", "Commit 3"]);

    // Poll commitlog - should automatically yield commit 3!
    let c3 = clog.next_commit().expect("expected live commit 3");
    assert_eq!(c3.files[0].filename, "/file3.txt");

    abort_flag.store(true, Ordering::SeqCst);
}

#[test]
fn test_logmill_dispatch_live_and_github() {
    let opts_github = VcsOptions {
        github: "testowner/testrepo".to_string(),
        ..Default::default()
    };
    // Test that LogMill::spawn recognizes options.github
    let mill = LogMill::spawn(".", opts_github);
    // LogMill spawned thread will attempt to fetch
    let status = mill.status();
    assert!(
        status == gource_vcs::LogMillStatus::Fetching
            || status == gource_vcs::LogMillStatus::Success
    );
}

#[test]
fn test_multi_tree_live_watching() {
    let temp_parent = tempfile::tempdir().unwrap();
    let repo_a = temp_parent.path().join("repo-alpha");
    let repo_b = temp_parent.path().join("repo-beta");
    std::fs::create_dir(&repo_a).unwrap();
    std::fs::create_dir(&repo_b).unwrap();

    let init_repo = |path: &std::path::Path, file: &str, content: &str, msg: &str| {
        let run = |args: &[&str]| {
            let status = Command::new("git")
                .args(args)
                .current_dir(path)
                .status()
                .expect("git execution failed");
            assert!(status.success());
        };
        run(&["init", "-b", "main"]);
        run(&["config", "user.name", "AlphaAuthor"]);
        run(&["config", "user.email", "alpha@example.com"]);
        run(&["config", "commit.gpgsign", "false"]);
        std::fs::write(path.join(file), content).unwrap();
        run(&["add", file]);
        run(&["commit", "-m", msg]);
    };

    init_repo(&repo_a, "main.rs", "fn main() {}\n", "Initial commit alpha");
    init_repo(
        &repo_b,
        "lib.rs",
        "pub fn lib() {}\n",
        "Initial commit beta",
    );

    let opts = VcsOptions {
        live: true,
        live_interval_secs: 0.1,
        include_numstat: true,
        watch_paths: vec![repo_a.clone(), repo_b.clone()],
        ..Default::default()
    };

    let abort_flag = Arc::new(AtomicBool::new(false));
    let mut clog = LiveGitWatcher::spawn(repo_a.clone(), opts, Arc::clone(&abort_flag)).unwrap();
    clog.wait_for_input(true);

    // Initial backfill should read from both repos with directory prefix!
    let mut files_seen = Vec::new();
    for _ in 0..2 {
        let c = clog
            .next_commit()
            .expect("expected commit from initial backfill");
        for f in &c.files {
            files_seen.push(f.filename.clone());
        }
    }

    assert!(
        files_seen.contains(&"/repo-alpha/main.rs".to_string()),
        "expected /repo-alpha/main.rs in {files_seen:?}"
    );
    assert!(
        files_seen.contains(&"/repo-beta/lib.rs".to_string()),
        "expected /repo-beta/lib.rs in {files_seen:?}"
    );

    // Now commit to repo_b dynamically
    std::fs::write(repo_b.join("extra.rs"), "pub fn extra() {}\n").unwrap();
    let status = Command::new("git")
        .args(["add", "extra.rs"])
        .current_dir(&repo_b)
        .status()
        .unwrap();
    assert!(status.success());
    let status = Command::new("git")
        .args(["commit", "-m", "Dynamic commit beta"])
        .current_dir(&repo_b)
        .status()
        .unwrap();
    assert!(status.success());

    // Next commit from stream should be from repo-beta with prefix!
    let live_c = clog.next_commit().expect("expected dynamic live commit");
    assert_eq!(live_c.files[0].filename, "/repo-beta/extra.rs");

    // Test in-flight worktree change in repo_a with watch_worktrees = true
    let opts_wt = VcsOptions {
        live: true,
        live_interval_secs: 0.1,
        watch_worktrees: true,
        worktree_poll_interval: 0.05,
        watch_paths: vec![repo_a.clone(), repo_b.clone()],
        ..Default::default()
    };
    let abort_wt = Arc::new(AtomicBool::new(false));
    let mut clog_wt =
        LiveGitWatcher::spawn(repo_a.clone(), opts_wt, Arc::clone(&abort_wt)).unwrap();
    clog_wt.wait_for_input(true);

    // Consume all backfill commits (repo_a has 1 commit, repo_b has 2 commits = total 3 commits)
    let c_bf1 = clog_wt.next_commit().expect("expected backfill commit 1");
    let c_bf2 = clog_wt.next_commit().expect("expected backfill commit 2");
    let c_bf3 = clog_wt.next_commit().expect("expected backfill commit 3");
    assert!(!c_bf1.is_shadow);
    assert!(!c_bf2.is_shadow);
    assert!(!c_bf3.is_shadow);

    // Create uncommitted in-flight file in repo_a
    std::fs::write(repo_a.join("uncommitted.rs"), "fn inflight() {}\n").unwrap();

    // WorktreeWatcher will poll and stream shadow commit with prefix /repo-alpha/uncommitted.rs!
    let shadow_c = clog_wt
        .next_commit()
        .expect("expected in-flight shadow commit");
    assert!(shadow_c.is_shadow);
    assert_eq!(shadow_c.files[0].filename, "/repo-alpha/uncommitted.rs");
    assert!(shadow_c.files[0].is_shadow);

    abort_wt.store(true, Ordering::SeqCst);
    abort_flag.store(true, Ordering::SeqCst);
}
