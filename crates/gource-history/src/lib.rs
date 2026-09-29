//! High-performance repository history index, snapshots, metrics, and Git-of-Theseus tracking.

pub mod cache;
pub mod intern;
pub mod metrics;
pub mod snapshot;
pub mod theseus;
pub mod timeline;
pub mod worker;

pub use cache::{CACHE_MAGIC, CacheError, fnv1a_64};
pub use intern::{CohortId, CohortTable, PathEntry, PathId, PathTable, UserId, UserTable};
pub use metrics::{CommitMetricsPoint, EditorActivity, WindowSummary};
pub use snapshot::{LiveFileState, SNAPSHOT_STRIDE, TreeSnapshot};
pub use theseus::{ChurnDecayModel, CohortMode, FileCohorts, SurvivalPoint, estimate_half_life};
pub use timeline::{
    DashboardSeriesData, MarkerKind, PlaybackDirection, ScrubberState, TheseusDashboardData,
    TimelineBucket, TimelineHoverInfo, TimelineIndex, TimelineMarker,
};
pub use worker::{HistoryWorker, HistoryWorkerConfig, WorkerStatus};

/// Operation performed on a file in a commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeOp {
    /// New file created / added to the tree.
    Add,
    /// Existing file modified.
    Modify,
    /// File deleted / removed from the tree.
    Delete,
}

/// Compact change record stored in contiguous history arrays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeRecord {
    /// Interned path id.
    pub path: PathId,
    /// Operation performed.
    pub op: ChangeOp,
    /// Lines added in this change.
    pub lines_added: u32,
    /// Lines removed in this change.
    pub lines_removed: u32,
    /// Known byte size after this change, if available.
    pub byte_size: Option<u64>,
    /// True if the file was treated as binary.
    pub is_binary: bool,
}

/// Input format for a single file change when feeding commits into [`HistoryBuilder`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChangeInput {
    /// Path of the file (will be normalized and interned).
    pub path: String,
    /// Operation performed.
    pub op: ChangeOp,
    /// Lines added.
    pub lines_added: u32,
    /// Lines removed.
    pub lines_removed: u32,
    /// Byte size if known.
    pub byte_size: Option<u64>,
    /// Binary file flag.
    pub is_binary: bool,
}

/// Input format for feeding a commit into [`HistoryBuilder`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInput {
    /// Commit timestamp in Unix epoch seconds.
    pub timestamp: i64,
    /// Author / committer username.
    pub username: String,
    /// File changes in this commit.
    pub files: Vec<FileChangeInput>,
}

/// Index entry for a commit pointing to its slice in the contiguous change array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexedCommit {
    /// Commit timestamp in seconds.
    pub timestamp: i64,
    /// Interned user id.
    pub user: UserId,
    /// Interned cohort id.
    pub cohort: CohortId,
    /// Start index into `History::changes`.
    pub change_start: u32,
    /// Number of changes in this commit.
    pub change_len: u32,
}

/// Reusable helper that applies a commit's changes to a [`TreeSnapshot`].
fn apply_commit_changes(
    changes: &[ChangeRecord],
    commit: &IndexedCommit,
    state: &mut TreeSnapshot,
    decay_model: ChurnDecayModel,
) {
    for ch in changes {
        match ch.op {
            ChangeOp::Add => {
                let old_buckets = state.file(ch.path).map(|prev| prev.cohorts.buckets.clone());
                if let Some(buckets) = old_buckets {
                    for (cid, lines) in buckets {
                        state.deduct_cohort_lines(cid, lines as u64);
                    }
                }
                let initial_bytes = ch.byte_size.unwrap_or(ch.lines_added as u64 * 35);
                let new_file = LiveFileState::new(
                    ch.path,
                    ch.lines_added,
                    initial_bytes,
                    commit.user,
                    commit.timestamp,
                    commit.cohort,
                );
                state.insert_or_replace(new_file);
                state.add_cohort_lines(commit.cohort, ch.lines_added as u64);
            }
            ChangeOp::Modify => {
                let deducted = if let Some(file) = state.file_mut(ch.path) {
                    let deducted = file.cohorts.remove_lines(ch.lines_removed, decay_model);
                    file.cohorts.add_lines(commit.cohort, ch.lines_added);
                    file.lines = file.cohorts.total_lines();
                    if let Some(bytes) = ch.byte_size {
                        file.byte_size = bytes;
                    } else {
                        file.byte_size = file.lines as u64 * 35;
                    }
                    file.touch_count = file.touch_count.saturating_add(1);
                    file.last_user = commit.user;
                    file.last_timestamp = commit.timestamp;
                    Some(deducted)
                } else {
                    None
                };

                if let Some(deducted) = deducted {
                    for (cid, count) in deducted {
                        state.deduct_cohort_lines(cid, count as u64);
                    }
                    state.add_cohort_lines(commit.cohort, ch.lines_added as u64);
                } else {
                    // Implicit add if file was modified without preceding add
                    let initial_bytes = ch.byte_size.unwrap_or(ch.lines_added as u64 * 35);
                    let new_file = LiveFileState::new(
                        ch.path,
                        ch.lines_added,
                        initial_bytes,
                        commit.user,
                        commit.timestamp,
                        commit.cohort,
                    );
                    state.insert_or_replace(new_file);
                    state.add_cohort_lines(commit.cohort, ch.lines_added as u64);
                }
            }
            ChangeOp::Delete => {
                if let Some(removed_file) = state.remove_file(ch.path) {
                    for (cid, count) in removed_file.cohorts.buckets {
                        state.deduct_cohort_lines(cid, count as u64);
                    }
                }
            }
        }
    }
}

/// Fully indexed repository history with snapshot checkpoints and metrics.
#[derive(Debug, Clone)]
pub struct History {
    /// Interned path table.
    pub paths: PathTable,
    /// Interned user table.
    pub users: UserTable,
    /// Interned cohort table.
    pub cohorts: CohortTable,
    /// Cohort classification mode.
    pub cohort_mode: CohortMode,
    /// Line churn decay strategy.
    pub decay_model: ChurnDecayModel,
    /// Contiguous array of all file change records across all commits.
    pub changes: Vec<ChangeRecord>,
    /// Contiguous array of indexed commits.
    pub commits: Vec<IndexedCommit>,
    /// Prefix-sum cumulative metrics at each commit.
    pub metrics: Vec<CommitMetricsPoint>,
    /// Periodic snapshot checkpoints spaced by [`SNAPSHOT_STRIDE`].
    pub snapshots: Vec<TreeSnapshot>,
}

impl History {
    /// Number of commits in the history.
    pub fn commit_count(&self) -> usize {
        self.commits.len()
    }

    /// True if no commits are indexed.
    pub fn is_empty(&self) -> bool {
        self.commits.is_empty()
    }

    /// Change records for a specific commit index.
    pub fn commit_changes(&self, commit_idx: usize) -> &[ChangeRecord] {
        if let Some(c) = self.commits.get(commit_idx) {
            let start = c.change_start as usize;
            let end = start + c.change_len as usize;
            &self.changes[start..end]
        } else {
            &[]
        }
    }

    /// Cumulative metrics point at a commit index.
    pub fn metrics_at_commit(&self, commit_idx: usize) -> Option<&CommitMetricsPoint> {
        self.metrics.get(commit_idx)
    }

    /// Reconstructs the exact [`TreeSnapshot`] at `commit_idx` by replaying from the nearest
    /// preceding checkpoint (replaying at most [`SNAPSHOT_STRIDE`] commits).
    pub fn state_at_commit(&self, commit_idx: usize) -> TreeSnapshot {
        if self.commits.is_empty() || commit_idx >= self.commits.len() {
            return TreeSnapshot::new();
        }

        // Binary search nearest snapshot <= commit_idx
        let snap_idx = match self
            .snapshots
            .binary_search_by_key(&commit_idx, |s| s.commit_index)
        {
            Ok(exact) => exact,
            Err(next) => next.saturating_sub(1),
        };

        let mut state = if snap_idx < self.snapshots.len()
            && self.snapshots[snap_idx].commit_index <= commit_idx
        {
            self.snapshots[snap_idx].clone()
        } else {
            TreeSnapshot::new()
        };

        let replay_start = if snap_idx < self.snapshots.len()
            && self.snapshots[snap_idx].commit_index <= commit_idx
        {
            self.snapshots[snap_idx].commit_index + 1
        } else {
            0
        };

        for i in replay_start..=commit_idx {
            let commit = &self.commits[i];
            let changes = self.commit_changes(i);
            apply_commit_changes(changes, commit, &mut state, self.decay_model);
            state.commit_index = i;
            state.timestamp = commit.timestamp;
        }

        state
    }

    /// Binary-searches the latest commit at or before `timestamp` and materializes its state.
    pub fn state_at_timestamp(&self, timestamp: i64) -> Option<TreeSnapshot> {
        if self.commits.is_empty() || self.commits[0].timestamp > timestamp {
            return None;
        }
        let pos = match self
            .commits
            .binary_search_by_key(&timestamp, |c| c.timestamp)
        {
            Ok(idx) => {
                // Advance to the last commit with identical timestamp
                let mut last = idx;
                while last + 1 < self.commits.len() && self.commits[last + 1].timestamp == timestamp
                {
                    last += 1;
                }
                last
            }
            Err(idx) => idx.saturating_sub(1),
        };
        Some(self.state_at_commit(pos))
    }

    /// Computes activity statistics across a sliding time window ending at `end_commit_idx`.
    pub fn window_summary(&self, end_commit_idx: usize, window_seconds: i64) -> WindowSummary {
        if self.commits.is_empty() || end_commit_idx >= self.commits.len() {
            return WindowSummary {
                start_timestamp: 0,
                end_timestamp: 0,
                commits_in_window: 0,
                lines_added: 0,
                lines_removed: 0,
                net_lines: 0,
                distinct_active_editors: 0,
                top_editors: Vec::new(),
            };
        }

        let end_ts = self.commits[end_commit_idx].timestamp;
        let start_ts = end_ts.saturating_sub(window_seconds);

        // Find first commit >= start_ts
        let start_idx = match self.commits[..=end_commit_idx]
            .binary_search_by_key(&start_ts, |c| c.timestamp)
        {
            Ok(idx) => {
                let mut first = idx;
                while first > 0 && self.commits[first - 1].timestamp == start_ts {
                    first -= 1;
                }
                first
            }
            Err(idx) => idx,
        };

        let mut window_data = Vec::with_capacity(end_commit_idx - start_idx + 1);
        for i in start_idx..=end_commit_idx {
            let commit = &self.commits[i];
            let changes = self.commit_changes(i);
            let mut added: u32 = 0;
            let mut removed: u32 = 0;
            for ch in changes {
                added = added.saturating_add(ch.lines_added);
                removed = removed.saturating_add(ch.lines_removed);
            }
            window_data.push((commit.user, added, removed));
        }

        WindowSummary::compute(start_ts, end_ts, &window_data)
    }

    /// Calculates the survival trajectory over time for lines introduced by `cohort`.
    pub fn cohort_survival(&self, cohort: CohortId) -> Vec<SurvivalPoint> {
        let mut initial_lines: u64 = 0;
        let mut cohort_birth_ts: Option<i64> = None;

        // Determine initial lines added in this cohort and its first appearance
        for (i, commit) in self.commits.iter().enumerate() {
            if commit.cohort == cohort {
                if cohort_birth_ts.is_none() {
                    cohort_birth_ts = Some(commit.timestamp);
                }
                for ch in self.commit_changes(i) {
                    initial_lines = initial_lines.saturating_add(ch.lines_added as u64);
                }
            }
        }

        let Some(birth_ts) = cohort_birth_ts else {
            return Vec::new();
        };

        if initial_lines == 0 {
            return Vec::new();
        }

        let mut points = Vec::new();
        for (i, commit) in self.commits.iter().enumerate() {
            if commit.timestamp < birth_ts {
                continue;
            }
            let snap = self.state_at_commit(i);
            let surviving = snap
                .cohort_totals
                .get(cohort.0 as usize)
                .copied()
                .unwrap_or(0);
            points.push(SurvivalPoint {
                age_seconds: commit.timestamp - birth_ts,
                surviving_lines: surviving,
                initial_lines,
            });
        }

        points
    }

    /// Estimates the half-life in seconds of code written in `cohort`.
    pub fn cohort_half_life(&self, cohort: CohortId) -> Option<f64> {
        let points = self.cohort_survival(cohort);
        estimate_half_life(&points)
    }

    /// Formats history statistics as CSV (`commit,timestamp,total_files,total_lines,lines_added,lines_removed,active_editors_30d,dominant_cohort`).
    pub fn export_csv(&self) -> String {
        let mut csv = String::from(
            "commit,timestamp,total_files,total_lines,lines_added,lines_removed,active_editors_30d,dominant_cohort\n",
        );
        let thirty_days_sec = 30 * 86_400;

        for (i, commit) in self.commits.iter().enumerate() {
            let m = &self.metrics[i];
            let changes = self.commit_changes(i);
            let added: u32 = changes.iter().map(|c| c.lines_added).sum();
            let removed: u32 = changes.iter().map(|c| c.lines_removed).sum();

            let summary_30d = self.window_summary(i, thirty_days_sec);
            let snap = self.state_at_commit(i);
            let dominant_label = snap
                .dominant_cohort()
                .and_then(|cid| self.cohorts.get(cid))
                .unwrap_or("none");

            use std::fmt::Write;
            let _ = writeln!(
                csv,
                "{},{},{},{},{},{},{},{}",
                i,
                commit.timestamp,
                m.total_files,
                m.total_lines,
                added,
                removed,
                summary_30d.distinct_active_editors,
                dominant_label
            );
        }

        csv
    }
}

/// Builder for constructing [`History`] incrementally from commit streams.
#[derive(Debug, Clone)]
pub struct HistoryBuilder {
    cohort_mode: CohortMode,
    decay_model: ChurnDecayModel,
    paths: PathTable,
    users: UserTable,
    cohorts: CohortTable,
    changes: Vec<ChangeRecord>,
    commits: Vec<IndexedCommit>,
    metrics: Vec<CommitMetricsPoint>,
    snapshots: Vec<TreeSnapshot>,
    current_state: TreeSnapshot,
    cum_lines_added: u64,
    cum_lines_removed: u64,
}

impl HistoryBuilder {
    /// Creates a new builder with the specified cohort grouping and decay model.
    pub fn new(cohort_mode: CohortMode, decay_model: ChurnDecayModel) -> Self {
        Self {
            cohort_mode,
            decay_model,
            paths: PathTable::new(),
            users: UserTable::new(),
            cohorts: CohortTable::new(),
            changes: Vec::new(),
            commits: Vec::new(),
            metrics: Vec::new(),
            snapshots: Vec::new(),
            current_state: TreeSnapshot::new(),
            cum_lines_added: 0,
            cum_lines_removed: 0,
        }
    }

    /// Ingests a commit into the history model.
    pub fn add_commit(&mut self, commit: CommitInput) {
        let user_id = self.users.intern(&commit.username);
        let cohort_label = self
            .cohort_mode
            .cohort_label(commit.timestamp, &commit.username);
        let cohort_id = self.cohorts.intern(&cohort_label);

        let change_start = self.changes.len() as u32;
        let mut commit_records = Vec::with_capacity(commit.files.len());

        for f in commit.files {
            let path_id = self.paths.intern(&f.path);
            let rec = ChangeRecord {
                path: path_id,
                op: f.op,
                lines_added: f.lines_added,
                lines_removed: f.lines_removed,
                byte_size: f.byte_size,
                is_binary: f.is_binary,
            };
            self.cum_lines_added = self.cum_lines_added.saturating_add(f.lines_added as u64);
            self.cum_lines_removed = self
                .cum_lines_removed
                .saturating_add(f.lines_removed as u64);
            commit_records.push(rec);
        }

        let change_len = commit_records.len() as u32;
        let commit_idx = self.commits.len();
        let indexed = IndexedCommit {
            timestamp: commit.timestamp,
            user: user_id,
            cohort: cohort_id,
            change_start,
            change_len,
        };

        // Apply changes to current state
        apply_commit_changes(
            &commit_records,
            &indexed,
            &mut self.current_state,
            self.decay_model,
        );
        self.current_state.commit_index = commit_idx;
        self.current_state.timestamp = commit.timestamp;

        self.changes.extend(commit_records);
        self.commits.push(indexed);

        // Record metrics point
        self.metrics.push(CommitMetricsPoint {
            timestamp: commit.timestamp,
            total_files: self.current_state.total_files() as u32,
            total_lines: self.current_state.total_lines(),
            total_bytes: self.current_state.total_bytes(),
            cum_lines_added: self.cum_lines_added,
            cum_lines_removed: self.cum_lines_removed,
            cum_commits: self.commits.len() as u32,
        });

        // Store periodic checkpoint
        if commit_idx.is_multiple_of(SNAPSHOT_STRIDE) {
            self.snapshots.push(self.current_state.clone());
        }
    }

    /// Produces a point-in-time [`History`] snapshot from current builder state
    /// without consuming the builder.
    pub fn snapshot(&self) -> History {
        let mut snapshots = self.snapshots.clone();
        if snapshots.is_empty()
            || snapshots
                .last()
                .is_none_or(|s| s.commit_index != self.current_state.commit_index)
        {
            snapshots.push(self.current_state.clone());
        }

        History {
            paths: self.paths.clone(),
            users: self.users.clone(),
            cohorts: self.cohorts.clone(),
            cohort_mode: self.cohort_mode,
            decay_model: self.decay_model,
            changes: self.changes.clone(),
            commits: self.commits.clone(),
            metrics: self.metrics.clone(),
            snapshots,
        }
    }

    /// Finalizes and returns the complete [`History`].
    pub fn finish(mut self) -> History {
        // Ensure at least the final state is preserved if not already on a checkpoint
        if self.snapshots.is_empty()
            || self
                .snapshots
                .last()
                .is_none_or(|s| s.commit_index != self.current_state.commit_index)
        {
            self.snapshots.push(self.current_state.clone());
        }

        History {
            paths: self.paths,
            users: self.users,
            cohorts: self.cohorts,
            cohort_mode: self.cohort_mode,
            decay_model: self.decay_model,
            changes: self.changes,
            commits: self.commits,
            metrics: self.metrics,
            snapshots: self.snapshots,
        }
    }
}
