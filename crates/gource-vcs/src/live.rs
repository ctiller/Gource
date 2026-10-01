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

struct RepoWatchTarget {
    repo_dir: PathBuf,
    prefix: Option<String>,
    target_ref: String,
    last_sha: String,
    worktree_watcher: Option<crate::worktree::WorktreeWatcher>,
}

impl LiveGitWatcher {
    /// Spawns a background thread polling `repo_dir` (or multiple `options.watch_paths`) for new commits.
    pub fn spawn(
        repo_dir: PathBuf,
        options: VcsOptions,
        abort_flag: Arc<AtomicBool>,
    ) -> Result<CommitLog, String> {
        // Determine list of repository directories to watch
        let repo_dirs: Vec<PathBuf> = if !options.watch_paths.is_empty() {
            options.watch_paths.clone()
        } else {
            vec![repo_dir]
        };

        // Verify git works in each repo_dir
        for dir in &repo_dirs {
            let git_check = Command::new("git")
                .args(["rev-parse", "--git-dir"])
                .current_dir(dir)
                .output();

            match git_check {
                Ok(output) if output.status.success() => {}
                _ => return Err("failed to generate log file".to_string()),
            }
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

        let is_multi = repo_dirs.len() > 1;
        let mut targets = Vec::new();
        for dir in repo_dirs {
            let prefix = if is_multi {
                let name = dir
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "repo".to_string());
                Some(name)
            } else {
                None
            };
            let wt_watcher = if options.watch_worktrees {
                let mut watcher =
                    crate::worktree::WorktreeWatcher::new(dir.clone(), options.clone());
                if let Some(ref p) = prefix {
                    watcher = watcher.with_prefix(p.clone());
                }
                Some(watcher)
            } else {
                None
            };
            targets.push(RepoWatchTarget {
                repo_dir: dir,
                prefix,
                target_ref: target_ref.clone(),
                last_sha: String::new(),
                worktree_watcher: wt_watcher,
            });
        }

        let worker_options = options.clone();
        let worker_abort = abort_flag;

        thread::Builder::new()
            .name("gource-live-git".to_string())
            .spawn(move || {
                Self::worker_loop(targets, worker_options, worker_abort, tx);
            })
            .map_err(|e| e.to_string())?;

        Ok(CommitLog::from_live_stream("custom", None, rx, options))
    }

    fn worker_loop(
        mut targets: Vec<RepoWatchTarget>,
        options: VcsOptions,
        abort_flag: Arc<AtomicBool>,
        tx: Sender<String>,
    ) {
        // 1. Initial backfill across all watched repositories
        for target in &mut targets {
            target.last_sha = resolve_rev(&target.repo_dir, &target.target_ref).unwrap_or_default();
            if !target.last_sha.is_empty() {
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
                cmd.arg(&target.last_sha);
                cmd.current_dir(&target.repo_dir);

                if let Ok(output) = cmd.output()
                    && output.status.success()
                {
                    let text = String::from_utf8_lossy(&output.stdout);
                    if !stream_git_log_commits(&text, target.prefix.as_deref(), &options, &tx) {
                        return;
                    }
                }
            }

            // Initial worktree poll
            if let Some(ref mut wt_watcher) = target.worktree_watcher {
                let _ = wt_watcher.poll_and_stream(&tx);
            }
        }

        // 2. Polling loop
        let interval_secs = if options.live_interval_secs > 0.0 {
            options.live_interval_secs
        } else {
            5.0
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

                if last_worktree_poll.elapsed() >= worktree_interval {
                    for target in &mut targets {
                        if let Some(ref mut wt_watcher) = target.worktree_watcher {
                            let _ = wt_watcher.poll_and_stream(&tx);
                        }
                    }
                    last_worktree_poll = std::time::Instant::now();
                }
            }

            if abort_flag.load(Ordering::SeqCst) {
                return;
            }

            for target in &mut targets {
                // Optional live fetch
                if options.live_fetch {
                    let _ = Command::new("git")
                        .args(["fetch", "--quiet"])
                        .current_dir(&target.repo_dir)
                        .status();
                }

                let new_sha = match resolve_rev(&target.repo_dir, &target.target_ref) {
                    Some(sha) if !sha.is_empty() => sha,
                    _ => continue,
                };

                if new_sha != target.last_sha {
                    let range = if !target.last_sha.is_empty()
                        && is_ancestor(&target.repo_dir, &target.last_sha, &new_sha)
                    {
                        format!("{}..{new_sha}", target.last_sha)
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
                    cmd.current_dir(&target.repo_dir);

                    if let Ok(output) = cmd.output()
                        && output.status.success()
                    {
                        let text = String::from_utf8_lossy(&output.stdout);
                        if !stream_git_log_commits(&text, target.prefix.as_deref(), &options, &tx) {
                            return;
                        }
                    }

                    target.last_sha = new_sha;
                }
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
fn stream_git_log_commits(
    git_log_text: &str,
    prefix: Option<&str>,
    options: &VcsOptions,
    tx: &Sender<String>,
) -> bool {
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
            let filename = match prefix {
                Some(p) => {
                    let raw = &file.filename;
                    if raw.starts_with('/') {
                        format!("/{p}{raw}")
                    } else {
                        format!("/{p}/{raw}")
                    }
                }
                None => {
                    if file.filename.starts_with('/') {
                        file.filename.clone()
                    } else {
                        format!("/{}", file.filename)
                    }
                }
            };

            let line = if file.is_binary {
                format!(
                    "{}|{}|{}|{}|-|-",
                    commit.timestamp,
                    commit.username,
                    file.action.code(),
                    filename
                )
            } else if let (Some(added), Some(removed)) = (file.lines_added, file.lines_removed) {
                format!(
                    "{}|{}|{}|{}|{}|{}",
                    commit.timestamp,
                    commit.username,
                    file.action.code(),
                    filename,
                    added,
                    removed
                )
            } else {
                format!(
                    "{}|{}|{}|{}",
                    commit.timestamp,
                    commit.username,
                    file.action.code(),
                    filename
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
