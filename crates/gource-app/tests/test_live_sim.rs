//! End-to-end integration tests for Live Mode and GitHub Watch Mode (Phase 5).
//!
//! Tests:
//! 1. Local Git Live Mode streaming, new commit arrival, history growth, and DVR scrubbing.
//! 2. Live edge pinning and LiveBadge rewind/catch-up.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use gource_app::app::{AppOptions, GourceApp};
use gource_app::platform::Viewport;
use gource_core::UVec2;
use gource_draw::DrawList;
use gource_settings::{CliAction, parse_command_line};
use gource_widgets::timeline_bar::TimelineHit;
use tempfile::TempDir;

fn run_git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Alice")
        .env("GIT_AUTHOR_EMAIL", "alice@example.com")
        .env("GIT_COMMITTER_NAME", "Alice")
        .env("GIT_COMMITTER_EMAIL", "alice@example.com")
        .status()
        .expect("git command failed to spawn");
    assert!(status.success(), "git {:?} failed in {:?}", args, dir);
}

fn run_git_with_date(dir: &Path, args: &[&str], timestamp: i64) {
    let ts_str = timestamp.to_string();
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Alice")
        .env("GIT_AUTHOR_EMAIL", "alice@example.com")
        .env("GIT_AUTHOR_DATE", &ts_str)
        .env("GIT_COMMITTER_NAME", "Alice")
        .env("GIT_COMMITTER_EMAIL", "alice@example.com")
        .env("GIT_COMMITTER_DATE", &ts_str)
        .status()
        .expect("git command failed to spawn");
    assert!(status.success(), "git {:?} failed in {:?}", args, dir);
}

fn create_test_git_repo() -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let repo_path = temp_dir.path().to_path_buf();

    run_git(&repo_path, &["init"]);
    run_git(&repo_path, &["config", "user.name", "Alice"]);
    run_git(&repo_path, &["config", "user.email", "alice@example.com"]);

    // Commit 1 (timestamp 1700000000)
    fs::write(repo_path.join("file1.txt"), "hello world\n").unwrap();
    run_git(&repo_path, &["add", "file1.txt"]);
    run_git_with_date(&repo_path, &["commit", "-m", "commit 1"], 1700000000);

    // Commit 2 (timestamp 1700001000)
    fs::write(repo_path.join("file2.rs"), "fn main() {}\n").unwrap();
    run_git(&repo_path, &["add", "file2.rs"]);
    run_git_with_date(&repo_path, &["commit", "-m", "commit 2"], 1700001000);

    (temp_dir, repo_path)
}

#[test]
fn test_local_git_live_mode_and_dvr_scrubbing() {
    let (_temp_dir, repo_path) = create_test_git_repo();

    let args = [
        "gource".to_string(),
        "--live".to_string(),
        "--live-interval".to_string(),
        "0.05".to_string(),
        "--seconds-per-day".to_string(),
        "0.1".to_string(),
        "--file-idle-time".to_string(),
        "100.0".to_string(),
        repo_path.to_str().unwrap().to_string(),
    ];

    let CliAction::Run(config) = parse_command_line(&args[1..]).unwrap() else {
        panic!("expected CliAction::Run");
    };

    let mut app = GourceApp::new(config, AppOptions::default()).expect("GourceApp::new");
    let viewport = Viewport {
        width: 1280,
        height: 720,
        dpi_ratio: 1.0,
    };
    let mut draw_list = DrawList::new(UVec2::new(1280, 720));

    // Wait until commitlog is ready and step frames until Gource has replayed both initial commits
    let mut reached_live_edge = false;
    for _ in 0..120 {
        app.frame(0.05, viewport, &mut draw_list);
        if let Some(g) = app.shell_mut().gource.as_mut()
            && g.is_live_mode()
            && g.ingested_commits.len() >= 2
            && g.commit_cursor >= g.ingested_commits.len()
            && g.commitqueue.is_empty()
        {
            reached_live_edge = true;
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        reached_live_edge,
        "Failed to reach live edge after initial commits"
    );

    assert!(!app.is_finished());
    {
        let g = app.shell_mut().gource.as_mut().unwrap();
        assert!(g.is_live_mode());
        assert!(g.is_at_live_edge());

        // Both file1.txt and file2.rs should exist in the world
        let filenames: Vec<String> = g
            .world
            .files
            .values()
            .map(|f| f.pawn.name.clone())
            .collect();
        assert!(
            filenames.iter().any(|n| n == "file1.txt"),
            "file1.txt not found in world: {:?}",
            filenames
        );
        assert!(
            filenames.iter().any(|n| n == "file2.rs"),
            "file2.rs not found in world: {:?}",
            filenames
        );

        // History commit count should be 2
        let hist = g.ensure_history();
        assert_eq!(hist.commit_count(), 2);
    }

    // Step additional frames at the live edge: verify simulation does NOT quit and currtime holds
    let currtime_at_edge = app.shell().gource.as_ref().unwrap().currtime;
    for _ in 0..10 {
        app.frame(0.05, viewport, &mut draw_list);
    }
    assert!(!app.is_finished());
    let g = app.shell().gource.as_ref().unwrap();
    assert_eq!(g.currtime, currtime_at_edge);
    assert!(g.is_at_live_edge());

    // Commit 3 in the git repo (timestamp 1700002000)
    fs::write(repo_path.join("file3.go"), "package main\n").unwrap();
    run_git(&repo_path, &["add", "file3.go"]);
    run_git_with_date(&repo_path, &["commit", "-m", "commit 3"], 1700002000);

    // Allow background watcher thread to poll (interval 0.05s)
    thread::sleep(Duration::from_millis(150));

    // Step frames and verify commit 3 is ingested and animated into world.files
    let mut ingested_third_commit = false;
    for _ in 0..100 {
        app.frame(0.05, viewport, &mut draw_list);
        if let Some(g) = app.shell_mut().gource.as_mut()
            && g.ingested_commits.len() >= 3
            && g.commit_cursor >= g.ingested_commits.len()
            && g.commitqueue.is_empty()
        {
            ingested_third_commit = true;
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert!(ingested_third_commit, "Failed to ingest third live commit");

    {
        let g = app.shell_mut().gource.as_mut().unwrap();
        assert_eq!(g.ensure_history().commit_count(), 3);
        let filenames: Vec<String> = g
            .world
            .files
            .values()
            .map(|f| f.pawn.name.clone())
            .collect();
        assert!(
            filenames.iter().any(|n| n == "file3.go"),
            "file3.go not found in world: {:?}",
            filenames
        );
    }

    // Test DVR scrubbing: scrub backward to fraction 0.1
    {
        let g = app.shell_mut().gource.as_mut().unwrap();
        g.paused = true;
        g.handle_timeline_hit(TimelineHit::Track(0.1));
        assert!(
            !g.is_at_live_edge(),
            "Should not be at live edge while DVR scrubbed back"
        );
    }

    // Advance 5 frames in DVR mode
    for _ in 0..5 {
        app.frame(0.05, viewport, &mut draw_list);
    }
    assert!(!app.shell().gource.as_ref().unwrap().is_at_live_edge());

    // Click LiveBadge to jump back to live edge
    {
        let g = app.shell_mut().gource.as_mut().unwrap();
        g.handle_timeline_hit(TimelineHit::LiveBadge);
        assert!(
            g.is_at_live_edge(),
            "LiveBadge should jump back to live edge"
        );

        // Verify world state has all 3 files
        let filenames: Vec<String> = g
            .world
            .files
            .values()
            .map(|f| f.pawn.name.clone())
            .collect();
        assert!(
            filenames.iter().any(|n| n == "file3.go"),
            "file3.go should be present after jumping back to live edge"
        );
    }
}
