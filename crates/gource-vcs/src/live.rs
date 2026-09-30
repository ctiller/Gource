//! Live Git repository watcher (`--live`).
//!
//! Streams commits in real time from a local Git repository.

use crate::commit::Commit;
use crate::formats;
use crate::log::CommitLog;
use crate::options::VcsOptions;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::thread;
use std::time::Duration;

/// Live Git repository watcher that polls for new commits and streams them to a `CommitLog`.
pub struct LiveGitWatcher;

impl LiveGitWatcher {
    /// Spawns a background thread polling `repo_dir` for new commits.
    pub fn spawn(
        repo_dir: PathBuf,
        options: VcsOptions,
        abort_flag: Arc<AtomicBool>,
    ) -> Result<CommitLog, String> {
        // Verify git works in repo_dir
        let git_check = Command::new("git")
            .args(["rev-parse", "--git-dir"])
            .current_dir(&repo_dir)
            .output();

        match git_check {
            Ok(output) if output.status.success() => {}
            _ => return Err("failed to generate log file".to_string()),
        }

        // Validate git_branch to prevent flag injection (must not start with '-')
        if options.git_branch.starts_with('-') {
            return Err("failed to generate log file".to_string());
        }

        let (tx, rx) = channel::<String>();

        let target_ref = if options.git_branch.is_empty() {
            "HEAD".to_string()
        } else {
            options.git_branch.clone()
        };

        let worker_repo_dir = repo_dir;
        let worker_options = options.clone();
        let worker_abort = abort_flag;

        thread::Builder::new()
            .name("gource-live-git".to_string())
            .spawn(move || {
                Self::worker_loop(
                    worker_repo_dir,
                    target_ref,
                    worker_options,
                    worker_abort,
                    tx,
                );
            })
            .map_err(|e| e.to_string())?;

        Ok(CommitLog::from_live_stream("custom", None, rx, options))
    }

    fn worker_loop(
        repo_dir: PathBuf,
        target_ref: String,
        options: VcsOptions,
        abort_flag: Arc<AtomicBool>,
        tx: Sender<String>,
    ) {
        // 1. Resolve initial SHA
        let mut last_sha = resolve_rev(&repo_dir, &target_ref).unwrap_or_default();

        // 2. Initial backfill: run git log up to current SHA if repository has commits
        if !last_sha.is_empty() {
            let mut cmd = Command::new("git");
            cmd.args([
                "log",
                "--reverse",
                "--raw",
                "--encoding=UTF-8",
                "--no-renames",
            ]);
            if options.include_numstat {
                cmd.arg("--numstat");
            }
            cmd.arg("--no-show-signature");
            if options.author_time {
                cmd.arg("--pretty=format:user:%aN%n%at");
            } else {
                cmd.arg("--pretty=format:user:%aN%n%ct");
            }
            if options.start_timestamp != 0 {
                use chrono::TimeZone;
                if let Some(dt) = chrono::Local
                    .timestamp_opt(options.start_timestamp, 0)
                    .single()
                {
                    cmd.args(["--since", &dt.format("%Y-%m-%d").to_string()]);
                }
            }
            if options.stop_timestamp != 0 {
                use chrono::TimeZone;
                if let Some(dt) = chrono::Local
                    .timestamp_opt(options.stop_timestamp, 0)
                    .single()
                {
                    cmd.args(["--until", &dt.format("%Y-%m-%d").to_string()]);
                }
            }
            cmd.arg(&last_sha);
            cmd.current_dir(&repo_dir);

            if let Ok(output) = cmd.output()
                && output.status.success()
            {
                let text = String::from_utf8_lossy(&output.stdout);
                if !stream_git_log_commits(&text, &options, &tx) {
                    return;
                }
            }
        }

        // 3. Polling loop
        let interval_secs = if options.live_interval_secs > 0.0 {
            options.live_interval_secs
        } else {
            5.0
        };

        let mut worktree_watcher = if options.watch_worktrees {
            let mut watcher =
                crate::worktree::WorktreeWatcher::new(repo_dir.clone(), options.clone());
            // Initial poll on startup
            let _ = watcher.poll_and_stream(&tx);
            Some(watcher)
        } else {
            None
        };
        let worktree_interval = Duration::from_secs_f32(if options.worktree_poll_interval > 0.0 {
            options.worktree_poll_interval
        } else {
            0.25
        });
        let mut last_worktree_poll = std::time::Instant::now();

        let sleep_step = Duration::from_millis(50);
        let total_sleep = Duration::from_secs_f32(interval_secs);

        while !abort_flag.load(Ordering::SeqCst) {
            // Sleep in 50ms increments
            let mut elapsed = Duration::ZERO;
            while elapsed < total_sleep {
                if abort_flag.load(Ordering::SeqCst) {
                    return;
                }
                let sleep_duration = sleep_step.min(total_sleep - elapsed);
                thread::sleep(sleep_duration);
                elapsed += sleep_duration;

                if let Some(ref mut wt_watcher) = worktree_watcher
                    && last_worktree_poll.elapsed() >= worktree_interval
                {
                    let _ = wt_watcher.poll_and_stream(&tx);
                    last_worktree_poll = std::time::Instant::now();
                }
            }

            if abort_flag.load(Ordering::SeqCst) {
                return;
            }

            // Optional live fetch
            if options.live_fetch {
                let _ = Command::new("git")
                    .args(["fetch", "--quiet"])
                    .current_dir(&repo_dir)
                    .status();
            }

            let new_sha = match resolve_rev(&repo_dir, &target_ref) {
                Some(sha) if !sha.is_empty() => sha,
                _ => continue,
            };

            if new_sha != last_sha {
                let range = if !last_sha.is_empty() && is_ancestor(&repo_dir, &last_sha, &new_sha) {
                    format!("{last_sha}..{new_sha}")
                } else {
                    new_sha.clone()
                };

                let mut cmd = Command::new("git");
                cmd.args([
                    "log",
                    "--reverse",
                    "--raw",
                    "--encoding=UTF-8",
                    "--no-renames",
                ]);
                if options.include_numstat {
                    cmd.arg("--numstat");
                }
                cmd.arg("--no-show-signature");
                if options.author_time {
                    cmd.arg("--pretty=format:user:%aN%n%at");
                } else {
                    cmd.arg("--pretty=format:user:%aN%n%ct");
                }
                cmd.arg(&range);
                cmd.current_dir(&repo_dir);

                if let Ok(output) = cmd.output()
                    && output.status.success()
                {
                    let text = String::from_utf8_lossy(&output.stdout);
                    if !stream_git_log_commits(&text, &options, &tx) {
                        return;
                    }
                }

                last_sha = new_sha;
            }
        }
    }
}

fn resolve_rev(repo_dir: &Path, rev: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--verify", rev])
        .current_dir(repo_dir)
        .output()
        .ok()?;

    if output.status.success() {
        let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !sha.is_empty() {
            return Some(sha);
        }
    }
    None
}

fn is_ancestor(repo_dir: &Path, ancestor: &str, descendant: &str) -> bool {
    Command::new("git")
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .current_dir(repo_dir)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Parses git log commits from raw output and sends custom format lines followed by a sentinel line `""`.
/// Returns false if receiver has disconnected.
fn stream_git_log_commits(git_log_text: &str, options: &VcsOptions, tx: &Sender<String>) -> bool {
    let lines: Vec<&str> = git_log_text.lines().collect();
    let mut idx = 0;
    let total = lines.len();

    let mut get_line = |l: &mut String| -> bool {
        if idx < total {
            *l = lines[idx].to_string();
            idx += 1;
            true
        } else {
            false
        }
    };

    loop {
        let mut commit = Commit::default();
        if !formats::git::parse_commit(&mut get_line, &mut commit, options) {
            break;
        }

        // Format into custom log lines
        for file in &commit.files {
            let line = if file.is_binary {
                format!(
                    "{}|{}|{}|{}|-|-",
                    commit.timestamp,
                    commit.username,
                    file.action.code(),
                    file.filename
                )
            } else if let (Some(added), Some(removed)) = (file.lines_added, file.lines_removed) {
                format!(
                    "{}|{}|{}|{}|{}|{}",
                    commit.timestamp,
                    commit.username,
                    file.action.code(),
                    file.filename,
                    added,
                    removed
                )
            } else {
                format!(
                    "{}|{}|{}|{}",
                    commit.timestamp,
                    commit.username,
                    file.action.code(),
                    file.filename
                )
            };

            if tx.send(line).is_err() {
                return false;
            }
        }

        // Emit empty line sentinel to cleanly terminate the custom commit boundary
        if !commit.files.is_empty() && tx.send(String::new()).is_err() {
            return false;
        }
    }

    true
}
