//! Git worktree discovery and uncommitted in-flight change watcher.
//!
//! Provides [`discover_worktrees`] to find linked worktrees,
//! [`scan_worktree_in_flight`] to extract modified/untracked files, and
//! [`WorktreeWatcher`] to poll for in-flight changes and stream shadow commits.

use crate::commit::{Commit, CommitFile, FileAction, file_colour};
use crate::in_process_git::is_binary_buffer;
use crate::options::VcsOptions;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Sender;
use std::time::UNIX_EPOCH;

/// Information about a Git worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeInfo {
    /// Absolute path to the worktree root directory.
    pub path: PathBuf,
    /// Worktree branch name or identifier (e.g. "feature-xyz").
    pub branch: String,
    /// Current HEAD commit hash of this worktree.
    pub head_sha: String,
    /// Whether this is the main repository worktree.
    pub is_main: bool,
}

impl WorktreeInfo {
    /// Formatted author username for this worktree in the simulation.
    pub fn author_name(&self) -> String {
        format!("worktree:{}", self.branch)
    }
}

/// Discover all worktrees for a repository (main worktree + any linked worktrees).
pub fn discover_worktrees(repo_dir: &Path) -> Result<Vec<WorktreeInfo>, String> {
    // 1. Try `git worktree list --porcelain`
    let cmd_res = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_dir)
        .output();

    if let Ok(output) = cmd_res
        && output.status.success()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed = parse_worktree_list_porcelain(&stdout);
        if !parsed.is_empty() {
            return Ok(parsed);
        }
    }

    discover_worktrees_fallback(repo_dir)
}

/// Fallback discovery inspecting `.git` metadata directly when `git worktree list` is not available.
pub fn discover_worktrees_fallback(repo_dir: &Path) -> Result<Vec<WorktreeInfo>, String> {
    let mut worktrees = Vec::new();
    let git_dir = if repo_dir.join(".git").is_dir() {
        repo_dir.join(".git")
    } else if let Ok(content) = fs::read_to_string(repo_dir.join(".git"))
        && let Some(rest) = content.strip_prefix("gitdir:")
    {
        PathBuf::from(rest.trim())
    } else {
        repo_dir.to_path_buf()
    };

    let main_branch = read_branch_name(&git_dir.join("HEAD")).unwrap_or_else(|| {
        repo_dir
            .file_name()
            .map_or("main".to_string(), |n| n.to_string_lossy().to_string())
    });
    let main_head = read_head_sha(&git_dir.join("HEAD")).unwrap_or_default();

    worktrees.push(WorktreeInfo {
        path: repo_dir.to_path_buf(),
        branch: main_branch,
        head_sha: main_head,
        is_main: true,
    });

    let linked_worktrees_dir = git_dir.join("worktrees");
    if linked_worktrees_dir.is_dir()
        && let Ok(entries) = fs::read_dir(linked_worktrees_dir)
    {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            let gitdir_file = p.join("gitdir");
            let head_file = p.join("HEAD");
            if let Ok(gitdir_content) = fs::read_to_string(&gitdir_file) {
                let wt_gitdir = PathBuf::from(gitdir_content.trim());
                let wt_path = wt_gitdir.parent().unwrap_or(&wt_gitdir).to_path_buf();
                let branch = read_branch_name(&head_file).unwrap_or_else(|| {
                    p.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "linked".to_string())
                });
                let head_sha = read_head_sha(&head_file).unwrap_or_default();
                if wt_path.exists() {
                    worktrees.push(WorktreeInfo {
                        path: wt_path,
                        branch,
                        head_sha,
                        is_main: false,
                    });
                }
            }
        }
    }

    Ok(worktrees)
}

/// Parse output of `git worktree list --porcelain`.
pub fn parse_worktree_list_porcelain(stdout: &str) -> Vec<WorktreeInfo> {
    let mut list = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_head = String::new();
    let mut current_branch = String::new();
    let mut is_first = true;

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if let Some(path) = current_path.take() {
                let branch = if current_branch.is_empty() {
                    path.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "worktree".to_string())
                } else {
                    current_branch.clone()
                };
                list.push(WorktreeInfo {
                    path,
                    branch,
                    head_sha: current_head.clone(),
                    is_main: is_first,
                });
                is_first = false;
            }
            current_head.clear();
            current_branch.clear();
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("worktree ") {
            current_path = Some(PathBuf::from(rest.trim()));
        } else if let Some(rest) = trimmed.strip_prefix("HEAD ") {
            current_head = rest.trim().to_string();
        } else if let Some(rest) = trimmed.strip_prefix("branch ") {
            let ref_name = rest.trim();
            let short = ref_name.strip_prefix("refs/heads/").unwrap_or(ref_name);
            current_branch = short.to_string();
        } else if trimmed == "detached" && current_branch.is_empty() {
            current_branch = "detached".to_string();
        }
    }

    if let Some(path) = current_path {
        let branch = if current_branch.is_empty() {
            path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "worktree".to_string())
        } else {
            current_branch
        };
        list.push(WorktreeInfo {
            path,
            branch,
            head_sha: current_head,
            is_main: is_first,
        });
    }

    list
}

pub fn read_branch_name(head_path: &Path) -> Option<String> {
    let content = fs::read_to_string(head_path).ok()?;
    let trimmed = content.trim();
    if let Some(ref_path) = trimmed.strip_prefix("ref: refs/heads/") {
        Some(ref_path.to_string())
    } else if let Some(ref_path) = trimmed.strip_prefix("ref: ") {
        Some(ref_path.to_string())
    } else if !trimmed.is_empty() {
        Some("detached".to_string())
    } else {
        None
    }
}

pub fn read_head_sha(head_path: &Path) -> Option<String> {
    let content = fs::read_to_string(head_path).ok()?;
    let trimmed = content.trim();
    if !trimmed.starts_with("ref:") && !trimmed.is_empty() {
        Some(trimmed.to_string())
    } else {
        None
    }
}

/// In-flight file change detected in a worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InFlightFile {
    pub path: String,
    pub action: FileAction,
    pub is_binary: bool,
    pub lines_added: u32,
    pub lines_removed: u32,
    pub mtime_nanos: u64,
}

/// Scan a worktree directory for uncommitted changes (tracked dirty + untracked files).
pub fn scan_worktree_in_flight(
    worktree: &WorktreeInfo,
    options: &VcsOptions,
) -> Result<Vec<InFlightFile>, String> {
    let output = Command::new("git")
        .args(["status", "--porcelain=v1", "-uall"])
        .current_dir(&worktree.path)
        .output()
        .map_err(|e| {
            format!(
                "failed to run git status in {}: {e}",
                worktree.path.display()
            )
        })?;

    if !output.status.success() {
        return Err(format!(
            "git status failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut files = Vec::new();

    for line in stdout.lines() {
        if line.len() < 3 {
            continue;
        }
        let status = &line[0..2];
        let file_path = line[3..].trim();
        let target_path = if let Some((_, new_p)) = file_path.split_once(" -> ") {
            new_p.trim()
        } else {
            file_path
        };

        if !options.filters.allows_file(target_path) {
            continue;
        }

        let full_disk_path = worktree.path.join(target_path);
        let mtime_nanos = full_disk_path
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);

        let action = if status.contains('D') {
            FileAction::Delete
        } else if status == "??" || status.starts_with('A') {
            FileAction::Add
        } else {
            FileAction::Modify
        };

        let mut is_binary = false;
        let mut lines_added = 0;
        let mut lines_removed = 0;

        if action != FileAction::Delete && full_disk_path.is_file() {
            if let Ok(bytes) = fs::read(&full_disk_path) {
                if is_binary_buffer(&bytes) {
                    is_binary = true;
                } else {
                    let count = bytes.iter().filter(|&&b| b == b'\n').count() as u32;
                    lines_added = count.max(1);
                    if action == FileAction::Modify {
                        lines_removed = 1;
                    }
                }
            }
        } else if action == FileAction::Delete {
            lines_removed = 1;
        }

        files.push(InFlightFile {
            path: target_path.to_string(),
            action,
            is_binary,
            lines_added,
            lines_removed,
            mtime_nanos,
        });
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// Watches worktrees for real-time in-flight changes and emits shadow commit events.
pub struct WorktreeWatcher {
    repo_dir: PathBuf,
    options: VcsOptions,
    /// Known worktrees indexed by path.
    worktrees: Vec<WorktreeInfo>,
    /// State of in-flight files per worktree: `(worktree_path, file_path) -> (mtime, action)`.
    in_flight_state: BTreeMap<(PathBuf, String), InFlightFile>,
    /// Last seen HEAD per worktree path.
    worktree_heads: BTreeMap<PathBuf, String>,
}

impl WorktreeWatcher {
    /// Create a new worktree watcher for a repository root.
    pub fn new(repo_dir: PathBuf, options: VcsOptions) -> Self {
        Self {
            repo_dir,
            options,
            worktrees: Vec::new(),
            in_flight_state: BTreeMap::new(),
            worktree_heads: BTreeMap::new(),
        }
    }

    /// Refresh worktrees and poll for in-flight changes, streaming any new shadow commits to `tx`.
    pub fn poll_and_stream(&mut self, tx: &Sender<String>) -> Result<(), String> {
        // 1. Refresh worktrees list
        if let Ok(discovered) = discover_worktrees(&self.repo_dir) {
            self.worktrees = discovered;
        }

        let now = chrono::Utc::now().timestamp();
        let mut current_keys = BTreeSet::new();

        for wt in &self.worktrees {
            let files = match scan_worktree_in_flight(wt, &self.options) {
                Ok(f) => f,
                Err(_) => continue,
            };

            let author_name = wt.author_name();
            let mut shadow_files = Vec::new();

            for file in files {
                let key = (wt.path.clone(), file.path.clone());
                current_keys.insert(key.clone());

                let changed = match self.in_flight_state.get(&key) {
                    Some(prev) => {
                        prev.mtime_nanos != file.mtime_nanos || prev.action != file.action
                    }
                    None => true,
                };

                if changed {
                    let cf = CommitFile {
                        filename: if file.path.starts_with('/') {
                            file.path.clone()
                        } else {
                            format!("/{}", file.path)
                        },
                        action: file.action.clone(),
                        colour: file_colour(&file.path, &self.options.hasher),
                        lines_added: Some(file.lines_added),
                        lines_removed: Some(file.lines_removed),
                        is_binary: file.is_binary,
                        is_shadow: true,
                    };
                    shadow_files.push(cf);
                    self.in_flight_state.insert(key, file);
                }
            }

            // If changes were detected in this worktree, emit a shadow commit
            if !shadow_files.is_empty() {
                let commit = Commit {
                    timestamp: now,
                    username: author_name.clone(),
                    files: shadow_files,
                    is_shadow: true,
                };
                self.stream_shadow_commit(&commit, tx);
            }

            // Update recorded HEAD for worktree
            self.worktree_heads
                .insert(wt.path.clone(), wt.head_sha.clone());
        }

        // 2. Check for discarded / reverted in-flight files
        let discarded_keys: Vec<(PathBuf, String)> = self
            .in_flight_state
            .keys()
            .filter(|k| !current_keys.contains(k))
            .cloned()
            .collect();

        for key in discarded_keys {
            if let Some(prev) = self.in_flight_state.remove(&key) {
                // If HEAD changed, it was committed (handled by real commit flow)
                // If HEAD didn't change, the file was discarded / reverted!
                let wt_head_changed =
                    self.worktrees
                        .iter()
                        .find(|w| w.path == key.0)
                        .is_some_and(|w| {
                            self.worktree_heads
                                .get(&w.path)
                                .is_some_and(|old_h| *old_h != w.head_sha)
                        });

                if !wt_head_changed && prev.action != FileAction::Delete {
                    let branch = self
                        .worktrees
                        .iter()
                        .find(|w| w.path == key.0)
                        .map(|w| w.branch.as_str())
                        .unwrap_or("worktree");

                    let discard_commit = Commit {
                        timestamp: now,
                        username: format!("worktree:{branch}"),
                        files: vec![CommitFile {
                            filename: if prev.path.starts_with('/') {
                                prev.path
                            } else {
                                format!("/{}", prev.path)
                            },
                            action: FileAction::Delete,
                            colour: file_colour(&key.1, &self.options.hasher),
                            lines_added: None,
                            lines_removed: None,
                            is_binary: prev.is_binary,
                            is_shadow: true,
                        }],
                        is_shadow: true,
                    };
                    self.stream_shadow_commit(&discard_commit, tx);
                }
            }
        }

        Ok(())
    }

    fn stream_shadow_commit(&self, commit: &Commit, tx: &Sender<String>) {
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
            let _ = tx.send(line);
        }
        let _ = tx.send(String::new()); // commit separator
    }
}
