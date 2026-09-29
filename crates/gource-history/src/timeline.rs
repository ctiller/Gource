//! Timeline histogram indexing, playhead scrubbing state, clip in/out markers,
//! and dashboard metrics series extraction.

use glam::Vec3;
use gource_core::StringHasher;
use std::collections::{HashMap, HashSet};

use crate::History;
use crate::intern::{CohortId, UserId};

/// Classification kind for timeline annotation markers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerKind {
    /// Git release or milestone tag.
    Tag,
    /// Explanatory caption text.
    Caption,
    /// Project milestone or lifecycle event.
    Milestone,
}

/// An annotation marker placed at a specific timestamp on the timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineMarker {
    /// Timestamp in Unix seconds.
    pub timestamp: i64,
    /// Human-readable label text.
    pub label: String,
    /// Category / marker kind.
    pub kind: MarkerKind,
}

/// Summary metrics aggregated for a single timeline time slice.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TimelineBucket {
    /// Start timestamp of bucket (inclusive).
    pub start_time: i64,
    /// End timestamp of bucket (inclusive).
    pub end_time: i64,
    /// Total commits falling into this bucket.
    pub commits: u32,
    /// Total lines added in this bucket.
    pub lines_added: u64,
    /// Total lines removed in this bucket.
    pub lines_removed: u64,
    /// Number of distinct active authors in this bucket.
    pub active_users: u32,
}

/// Detailed context for tooltip inspection when hovering over the timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineHoverInfo {
    /// Timestamp at the hovered fraction.
    pub timestamp: i64,
    /// Nearest commit index at or before the hovered timestamp.
    pub commit_idx: Option<usize>,
    /// Total commits in the hovered bucket.
    pub bucket_commits: u32,
    /// Lines added in the hovered bucket.
    pub bucket_lines_added: u64,
    /// Lines removed in the hovered bucket.
    pub bucket_lines_removed: u64,
    /// Up to 3 top editors in this bucket sorted descending by commits `(name, commits)`.
    pub top_editors: Vec<(String, u32)>,
}

/// Timeline histogram index dividing history into uniform time buckets.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineIndex {
    /// Earliest commit timestamp in history.
    pub min_time: i64,
    /// Latest commit timestamp in history.
    pub max_time: i64,
    /// Uniform time buckets across history.
    pub buckets: Vec<TimelineBucket>,
    /// Peak commit count across all buckets (for normalizing histogram heights).
    pub max_bucket_commits: u32,
    /// Registered timeline markers.
    pub markers: Vec<TimelineMarker>,
}

impl TimelineIndex {
    /// Constructs a timeline index by dividing the history duration into `num_buckets` equal bins.
    pub fn from_history(history: &History, num_buckets: usize) -> Self {
        if history.is_empty() {
            return Self {
                min_time: 0,
                max_time: 0,
                buckets: Vec::new(),
                max_bucket_commits: 0,
                markers: Vec::new(),
            };
        }

        let min_time = history.commits.first().map(|c| c.timestamp).unwrap_or(0);
        let max_time = history.commits.last().map(|c| c.timestamp).unwrap_or(0);
        let n = num_buckets.max(1);

        let time_span = (max_time - min_time).max(0) as f64;
        let mut buckets = Vec::with_capacity(n);
        let mut bucket_users: Vec<HashSet<UserId>> = Vec::with_capacity(n);

        for b in 0..n {
            let start = if time_span > 0.0 {
                min_time + ((b as f64 * time_span) / n as f64) as i64
            } else {
                min_time
            };
            let end = if b + 1 == n || time_span == 0.0 {
                max_time
            } else {
                min_time + (((b + 1) as f64 * time_span) / n as f64) as i64
            };
            buckets.push(TimelineBucket {
                start_time: start,
                end_time: end,
                commits: 0,
                lines_added: 0,
                lines_removed: 0,
                active_users: 0,
            });
            bucket_users.push(HashSet::new());
        }

        // Aggregate commits into buckets
        for (i, commit) in history.commits.iter().enumerate() {
            let b_idx = if time_span > 0.0 {
                let frac = (commit.timestamp - min_time) as f64 / time_span;
                ((frac * n as f64) as usize).min(n - 1)
            } else {
                0
            };

            let bucket = &mut buckets[b_idx];
            bucket.commits += 1;
            bucket_users[b_idx].insert(commit.user);

            for ch in history.commit_changes(i) {
                bucket.lines_added = bucket.lines_added.saturating_add(ch.lines_added as u64);
                bucket.lines_removed = bucket.lines_removed.saturating_add(ch.lines_removed as u64);
            }
        }

        for (b, users) in buckets.iter_mut().zip(bucket_users) {
            b.active_users = users.len() as u32;
        }

        let max_bucket_commits = buckets.iter().map(|b| b.commits).max().unwrap_or(0);

        Self {
            min_time,
            max_time,
            buckets,
            max_bucket_commits,
            markers: Vec::new(),
        }
    }

    /// Adds an annotation marker to the timeline.
    pub fn add_marker(&mut self, marker: TimelineMarker) {
        let pos = self
            .markers
            .binary_search_by_key(&marker.timestamp, |m| m.timestamp)
            .unwrap_or_else(|idx| idx);
        self.markers.insert(pos, marker);
    }

    /// Converts a timestamp to a normalized fraction in `0.0..=1.0`.
    pub fn time_to_fraction(&self, timestamp: i64) -> f32 {
        if self.max_time <= self.min_time {
            0.0
        } else {
            ((timestamp - self.min_time) as f32 / (self.max_time - self.min_time) as f32)
                .clamp(0.0, 1.0)
        }
    }

    /// Converts a normalized fraction in `0.0..=1.0` to a timestamp.
    pub fn fraction_to_time(&self, fraction: f32) -> i64 {
        if self.max_time <= self.min_time {
            self.min_time
        } else {
            self.min_time
                + (fraction.clamp(0.0, 1.0) * (self.max_time - self.min_time) as f32) as i64
        }
    }

    /// Extracts hover inspection information for a given timeline fraction.
    pub fn hover_summary(&self, history: &History, fraction: f32) -> TimelineHoverInfo {
        let ts = self.fraction_to_time(fraction);
        if self.buckets.is_empty() || history.is_empty() {
            return TimelineHoverInfo {
                timestamp: ts,
                commit_idx: None,
                bucket_commits: 0,
                bucket_lines_added: 0,
                bucket_lines_removed: 0,
                top_editors: Vec::new(),
            };
        }

        // Find bucket index
        let b_idx = if self.max_time > self.min_time {
            let frac = (ts - self.min_time) as f64 / (self.max_time - self.min_time) as f64;
            ((frac * self.buckets.len() as f64) as usize).min(self.buckets.len() - 1)
        } else {
            0
        };

        let bucket = &self.buckets[b_idx];

        // Find nearest commit at or before timestamp
        let commit_idx = match history.commits.binary_search_by_key(&ts, |c| c.timestamp) {
            Ok(idx) => {
                let mut last = idx;
                while last + 1 < history.commits.len() && history.commits[last + 1].timestamp == ts
                {
                    last += 1;
                }
                Some(last)
            }
            Err(idx) => {
                if idx == 0 {
                    Some(0)
                } else {
                    Some(idx - 1)
                }
            }
        };

        // Collect editor counts within this bucket's time window
        let mut user_commits: HashMap<UserId, u32> = HashMap::new();
        for commit in &history.commits {
            if commit.timestamp >= bucket.start_time && commit.timestamp <= bucket.end_time {
                *user_commits.entry(commit.user).or_insert(0) += 1;
            }
        }

        let mut sorted_editors: Vec<(String, u32)> = user_commits
            .into_iter()
            .map(|(uid, count)| {
                let name = history.users.get(uid).unwrap_or("unknown").to_string();
                (name, count)
            })
            .collect();

        sorted_editors.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        sorted_editors.truncate(3);

        TimelineHoverInfo {
            timestamp: ts,
            commit_idx,
            bucket_commits: bucket.commits,
            bucket_lines_added: bucket.lines_added,
            bucket_lines_removed: bucket.lines_removed,
            top_editors: sorted_editors,
        }
    }
}

/// Git-of-Theseus cohort survival samples for dashboard visualization.
#[derive(Debug, Clone, PartialEq)]
pub struct TheseusDashboardData {
    /// Cohort labels (e.g. `["2021", "2022", "2023"]`).
    pub cohort_labels: Vec<String>,
    /// Line count samples across time up to playhead: `[sample_idx][cohort_idx]`.
    pub samples: Vec<Vec<u64>>,
    /// Estimated codebase half-life in days (if calculated).
    pub half_life_days: Option<f32>,
    /// Lifetime churn rate in `0.0..=1.0`: `removed / (added + removed)`.
    pub churn_rate: f32,
}

/// Extracted series and metrics ready for direct UI dashboard rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct DashboardSeriesData {
    /// Total live lines of code at the playhead commit.
    pub total_lines: u64,
    /// Net line delta in the sliding window (`added - removed`).
    pub lines_delta_in_window: i64,
    /// Sparkline samples of total lines up to playhead.
    pub lines_sparkline: Vec<f32>,
    /// Total active files at the playhead commit.
    pub total_files: u32,
    /// Sparkline samples of total files up to playhead.
    pub files_sparkline: Vec<f32>,
    /// `(lines_added, lines_removed)` per time period over the last `max_samples` periods.
    pub diff_bars: Vec<(f32, f32)>,
    /// Commits within the sliding window.
    pub commits_in_window: u32,
    /// Commits count per time period over the last `max_samples` periods.
    pub commits_per_period: Vec<f32>,
    /// Distinct active authors in the sliding window.
    pub active_editors_count: usize,
    /// Top editors in window: `(name, colour, commits, lines_added)`.
    pub top_editors: Vec<(String, Vec3, u32, u64)>,
    /// Git-of-Theseus cohort stacked breakdown and churn metrics.
    pub theseus_cohorts: TheseusDashboardData,
}

impl DashboardSeriesData {
    /// Extracts comprehensive dashboard series and metrics up to `playhead_commit_idx`.
    pub fn extract(
        history: &History,
        playhead_commit_idx: usize,
        period_seconds: i64,
        window_seconds: i64,
        max_samples: usize,
    ) -> Self {
        if history.is_empty() || playhead_commit_idx >= history.commits.len() {
            return Self::empty();
        }

        let playhead_ts = history.commits[playhead_commit_idx].timestamp;
        let metrics = &history.metrics[playhead_commit_idx];
        let total_lines = metrics.total_lines;
        let total_files = metrics.total_files;

        // 1. Sliding window activity
        let win = history.window_summary(playhead_commit_idx, window_seconds);
        let lines_delta_in_window = win.net_lines;
        let commits_in_window = win.commits_in_window;
        let active_editors_count = win.distinct_active_editors as usize;

        let hasher = StringHasher::default();
        let top_editors: Vec<(String, Vec3, u32, u64)> = win
            .top_editors
            .iter()
            .take(5)
            .map(|e| {
                let name = history.users.get(e.user).unwrap_or("unknown").to_string();
                let col = hasher.colour_hash(&name);
                (name, col, e.commits, e.lines_added)
            })
            .collect();

        // 2. Sparklines (lines and files sampled across 0..=playhead_commit_idx)
        let sample_count = max_samples.max(1);
        let total_steps = playhead_commit_idx + 1;
        let mut lines_sparkline = Vec::with_capacity(sample_count);
        let mut files_sparkline = Vec::with_capacity(sample_count);
        let mut sampled_indices = Vec::with_capacity(sample_count);

        for s in 0..sample_count {
            let idx = if sample_count > 1 {
                ((s as f64 * (total_steps - 1) as f64) / (sample_count - 1) as f64).round() as usize
            } else {
                playhead_commit_idx
            };
            sampled_indices.push(idx);
            lines_sparkline.push(history.metrics[idx].total_lines as f32);
            files_sparkline.push(history.metrics[idx].total_files as f32);
        }

        // 3. Diff bars and commits per period (over the last max_samples periods up to playhead)
        let period = period_seconds.max(1);
        let mut diff_bars = Vec::with_capacity(sample_count);
        let mut commits_per_period = Vec::with_capacity(sample_count);

        for s in 0..sample_count {
            let offset_start = (sample_count - s) as i64 * period;
            let offset_end = (sample_count - s - 1) as i64 * period;
            let p_start = playhead_ts - offset_start;
            let p_end = playhead_ts - offset_end;

            let mut added: u64 = 0;
            let mut removed: u64 = 0;
            let mut period_commits: u32 = 0;

            for i in 0..=playhead_commit_idx {
                let ts = history.commits[i].timestamp;
                if ts >= p_start && (ts < p_end || (s + 1 == sample_count && ts <= p_end)) {
                    period_commits += 1;
                    for ch in history.commit_changes(i) {
                        added = added.saturating_add(ch.lines_added as u64);
                        removed = removed.saturating_add(ch.lines_removed as u64);
                    }
                }
            }

            diff_bars.push((added as f32, removed as f32));
            commits_per_period.push(period_commits as f32);
        }

        // 4. Git-of-Theseus cohorts
        let num_cohorts = history.cohorts.len();
        let cohort_labels: Vec<String> = (0..num_cohorts)
            .map(|cid| {
                history
                    .cohorts
                    .get(CohortId(cid as u16))
                    .unwrap_or("")
                    .to_string()
            })
            .collect();

        let mut samples = Vec::with_capacity(sampled_indices.len());
        for &c_idx in &sampled_indices {
            let snap = history.state_at_commit(c_idx);
            let mut row = vec![0u64; num_cohorts];
            for (cid, lines) in snap.stacked_cohorts() {
                if (cid.0 as usize) < num_cohorts {
                    row[cid.0 as usize] = lines;
                }
            }
            samples.push(row);
        }

        let total_touched = metrics.cum_lines_added + metrics.cum_lines_removed;
        let churn_rate = if total_touched == 0 {
            0.0
        } else {
            (metrics.cum_lines_removed as f32 / total_touched as f32).clamp(0.0, 1.0)
        };

        // Half life estimate from cohort 0 if available
        let half_life_days = history
            .cohort_half_life(CohortId(0))
            .map(|sec| (sec / 86400.0) as f32);

        Self {
            total_lines,
            lines_delta_in_window,
            lines_sparkline,
            total_files,
            files_sparkline,
            diff_bars,
            commits_in_window,
            commits_per_period,
            active_editors_count,
            top_editors,
            theseus_cohorts: TheseusDashboardData {
                cohort_labels,
                samples,
                half_life_days,
                churn_rate,
            },
        }
    }

    /// Empty placeholder dashboard data.
    pub fn empty() -> Self {
        Self {
            total_lines: 0,
            lines_delta_in_window: 0,
            lines_sparkline: Vec::new(),
            total_files: 0,
            files_sparkline: Vec::new(),
            diff_bars: Vec::new(),
            commits_in_window: 0,
            commits_per_period: Vec::new(),
            active_editors_count: 0,
            top_editors: Vec::new(),
            theseus_cohorts: TheseusDashboardData {
                cohort_labels: Vec::new(),
                samples: Vec::new(),
                half_life_days: None,
                churn_rate: 0.0,
            },
        }
    }
}

/// Playback direction for scrubbed timeline traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackDirection {
    /// Normal forward chronological playback.
    #[default]
    Forward,
    /// Reverse chronological rewinding.
    Reverse,
}

/// Interactive timeline scrubber controller state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScrubberState {
    /// Current playhead commit index.
    pub playhead_commit_idx: usize,
    /// Current playhead position in normalized fraction `0.0..=1.0`.
    pub playhead_fraction: f32,
    /// Playback direction (Forward or Reverse).
    pub playback_direction: PlaybackDirection,
    /// True while the user is actively dragging the scrubber thumb.
    pub dragging: bool,
    /// Optional in-point clipping fraction `0.0..=1.0`.
    pub clip_in: Option<f32>,
    /// Optional out-point clipping fraction `0.0..=1.0`.
    pub clip_out: Option<f32>,
    /// Target fraction during smooth seeking / animated transitions.
    pub target_fraction: Option<f32>,
}

impl ScrubberState {
    /// Creates a default initial scrubber state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the clip-in boundary fraction.
    pub fn set_clip_in(&mut self, fraction: f32) {
        let clamped = fraction.clamp(0.0, 1.0);
        let max_val = self.clip_out.unwrap_or(1.0);
        self.clip_in = Some(clamped.min(max_val));
        if self.playhead_fraction < clamped {
            self.playhead_fraction = clamped;
        }
    }

    /// Sets the clip-out boundary fraction.
    pub fn set_clip_out(&mut self, fraction: f32) {
        let clamped = fraction.clamp(0.0, 1.0);
        let min_val = self.clip_in.unwrap_or(0.0);
        self.clip_out = Some(clamped.max(min_val));
        if self.playhead_fraction > clamped {
            self.playhead_fraction = clamped;
        }
    }

    /// Clears both in and out clip boundaries.
    pub fn clear_clips(&mut self) {
        self.clip_in = None;
        self.clip_out = None;
    }

    /// Clamps an arbitrary fraction within configured clip boundaries.
    pub fn clamp_fraction(&self, frac: f32) -> f32 {
        let min_f = self.clip_in.unwrap_or(0.0);
        let max_f = self.clip_out.unwrap_or(1.0);
        frac.clamp(min_f, max_f)
    }

    /// Updates the playhead fraction and calculates the corresponding commit index.
    pub fn set_fraction(&mut self, fraction: f32, total_commits: usize) {
        let clamped = self.clamp_fraction(fraction);
        self.playhead_fraction = clamped;
        self.playhead_commit_idx = if total_commits == 0 {
            0
        } else {
            let idx = (clamped * (total_commits - 1) as f32).round() as usize;
            idx.min(total_commits - 1)
        };
    }

    /// Advances playhead by one commit forward, respecting clip boundaries.
    pub fn step_forward(&mut self, total_commits: usize) {
        if total_commits == 0 || self.playhead_commit_idx + 1 >= total_commits {
            return;
        }
        let next_idx = self.playhead_commit_idx + 1;
        let frac = next_idx as f32 / (total_commits - 1) as f32;
        if let Some(out_point) = self.clip_out
            && frac > out_point + 1e-4
        {
            return;
        }
        self.playhead_commit_idx = next_idx;
        self.playhead_fraction = frac.clamp(0.0, 1.0);
    }

    /// Rewinds playhead by one commit backward, respecting clip boundaries.
    pub fn step_backward(&mut self, total_commits: usize) {
        if total_commits == 0 || self.playhead_commit_idx == 0 {
            return;
        }
        let prev_idx = self.playhead_commit_idx - 1;
        let frac = prev_idx as f32 / (total_commits - 1) as f32;
        if let Some(in_point) = self.clip_in
            && frac < in_point - 1e-4
        {
            return;
        }
        self.playhead_commit_idx = prev_idx;
        self.playhead_fraction = frac.clamp(0.0, 1.0);
    }

    /// Toggles playback direction between Forward and Reverse.
    pub fn toggle_direction(&mut self) {
        self.playback_direction = match self.playback_direction {
            PlaybackDirection::Forward => PlaybackDirection::Reverse,
            PlaybackDirection::Reverse => PlaybackDirection::Forward,
        };
    }
}
