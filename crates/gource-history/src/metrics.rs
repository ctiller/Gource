//! Cumulative time-series metrics, sliding window summaries, and CSV export.

use crate::intern::UserId;
use std::collections::HashMap;

/// Prefix-sum time-series data point captured at each commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitMetricsPoint {
    /// Commit timestamp in seconds.
    pub timestamp: i64,
    /// Total live files after this commit.
    pub total_files: u32,
    /// Total live lines of code after this commit.
    pub total_lines: u64,
    /// Total live bytes after this commit.
    pub total_bytes: u64,
    /// Cumulative lines added across history up to this commit.
    pub cum_lines_added: u64,
    /// Cumulative lines removed across history up to this commit.
    pub cum_lines_removed: u64,
    /// 1-based cumulative commit count.
    pub cum_commits: u32,
}

/// Activity statistics for an individual editor within a sliding window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorActivity {
    /// Interned user id.
    pub user: UserId,
    /// Number of commits made by this user in the window.
    pub commits: u32,
    /// Lines added by this user in the window.
    pub lines_added: u64,
    /// Lines removed by this user in the window.
    pub lines_removed: u64,
}

/// Aggregated activity summary across a time window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowSummary {
    /// Window start timestamp (inclusive).
    pub start_timestamp: i64,
    /// Window end timestamp (inclusive).
    pub end_timestamp: i64,
    /// Number of commits within this window.
    pub commits_in_window: u32,
    /// Total lines added in this window.
    pub lines_added: u64,
    /// Total lines removed in this window.
    pub lines_removed: u64,
    /// Net line delta (`lines_added as i64 - lines_removed as i64`).
    pub net_lines: i64,
    /// Count of distinct active editors in this window.
    pub distinct_active_editors: u32,
    /// Editor activity breakdown sorted descending by commit count, then lines added.
    pub top_editors: Vec<EditorActivity>,
}

impl WindowSummary {
    /// Aggregates activity across a range of commits.
    pub fn compute(
        start_ts: i64,
        end_ts: i64,
        commits: &[(UserId, u32, u32)], // (user, lines_added, lines_removed)
    ) -> Self {
        let mut total_added: u64 = 0;
        let mut total_removed: u64 = 0;
        let mut by_user: HashMap<UserId, (u32, u64, u64)> = HashMap::new();

        for &(user, added, removed) in commits {
            total_added += added as u64;
            total_removed += removed as u64;
            let entry = by_user.entry(user).or_insert((0, 0, 0));
            entry.0 += 1;
            entry.1 += added as u64;
            entry.2 += removed as u64;
        }

        let mut top_editors: Vec<EditorActivity> = by_user
            .into_iter()
            .map(|(user, (c, a, r))| EditorActivity {
                user,
                commits: c,
                lines_added: a,
                lines_removed: r,
            })
            .collect();

        // Sort descending by commits, then lines_added
        top_editors.sort_by(|a, b| {
            b.commits
                .cmp(&a.commits)
                .then_with(|| b.lines_added.cmp(&a.lines_added))
                .then_with(|| a.user.0.cmp(&b.user.0))
        });

        Self {
            start_timestamp: start_ts,
            end_timestamp: end_ts,
            commits_in_window: commits.len() as u32,
            lines_added: total_added,
            lines_removed: total_removed,
            net_lines: (total_added as i64) - (total_removed as i64),
            distinct_active_editors: top_editors.len() as u32,
            top_editors,
        }
    }
}
