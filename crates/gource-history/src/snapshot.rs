//! Live file repository state and periodic tree snapshots.

use crate::intern::{CohortId, PathId, UserId};
use crate::theseus::FileCohorts;

/// Number of commits between stored periodic snapshots.
pub const SNAPSHOT_STRIDE: usize = 64;

/// Live state of an active file in the repository tree.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveFileState {
    /// Interned path id.
    pub path: PathId,
    /// Current surviving line count.
    pub lines: u32,
    /// File size in bytes.
    pub byte_size: u64,
    /// Number of times touched / modified across commits.
    pub touch_count: u32,
    /// User who last touched the file.
    pub last_user: UserId,
    /// Timestamp when last modified.
    pub last_timestamp: i64,
    /// Timestamp when originally created (added).
    pub created_timestamp: i64,
    /// Per-file Git-of-Theseus cohort line breakdown.
    pub cohorts: FileCohorts,
}

impl LiveFileState {
    /// Creates a newly added file state.
    pub fn new(
        path: PathId,
        lines: u32,
        byte_size: u64,
        user: UserId,
        timestamp: i64,
        cohort: CohortId,
    ) -> Self {
        let mut cohorts = FileCohorts::new();
        cohorts.add_lines(cohort, lines);
        Self {
            path,
            lines,
            byte_size,
            touch_count: 1,
            last_user: user,
            last_timestamp: timestamp,
            created_timestamp: timestamp,
            cohorts,
        }
    }
}

/// Point-in-time snapshot of all live files and cohort totals in the repository.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TreeSnapshot {
    /// Zero-based commit index at which this snapshot was taken.
    pub commit_index: usize,
    /// Timestamp of this commit.
    pub timestamp: i64,
    /// Live files sorted by [`PathId`] for $O(\log N)$ binary search lookup.
    pub files: Vec<LiveFileState>,
    /// Global line counts indexed by `CohortId.0`.
    pub cohort_totals: Vec<u64>,
}

impl TreeSnapshot {
    /// Creates an empty tree snapshot.
    pub fn new() -> Self {
        Self::default()
    }

    /// Looks up a live file by [`PathId`].
    pub fn file(&self, path: PathId) -> Option<&LiveFileState> {
        self.files
            .binary_search_by_key(&path, |f| f.path)
            .ok()
            .map(|idx| &self.files[idx])
    }

    /// Looks up a live file mutably by [`PathId`].
    pub fn file_mut(&mut self, path: PathId) -> Option<&mut LiveFileState> {
        match self.files.binary_search_by_key(&path, |f| f.path) {
            Ok(idx) => Some(&mut self.files[idx]),
            Err(_) => None,
        }
    }

    /// Inserts or replaces a file in the sorted list.
    pub fn insert_or_replace(&mut self, file: LiveFileState) {
        match self.files.binary_search_by_key(&file.path, |f| f.path) {
            Ok(idx) => self.files[idx] = file,
            Err(idx) => self.files.insert(idx, file),
        }
    }

    /// Removes a file by [`PathId`], returning its previous state if it was live.
    pub fn remove_file(&mut self, path: PathId) -> Option<LiveFileState> {
        match self.files.binary_search_by_key(&path, |f| f.path) {
            Ok(idx) => Some(self.files.remove(idx)),
            Err(_) => None,
        }
    }

    /// Number of active files.
    pub fn total_files(&self) -> usize {
        self.files.len()
    }

    /// Total line count across all active files.
    pub fn total_lines(&self) -> u64 {
        self.files.iter().map(|f| f.lines as u64).sum()
    }

    /// Total byte size across all active files.
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.byte_size).sum()
    }

    /// Returns non-zero stacked cohort line counts `(CohortId, lines)`.
    pub fn stacked_cohorts(&self) -> Vec<(CohortId, u64)> {
        self.cohort_totals
            .iter()
            .enumerate()
            .filter_map(|(cid, &lines)| {
                if lines > 0 {
                    Some((CohortId(cid as u16), lines))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Identifies the dominant cohort (the cohort with the most surviving lines).
    pub fn dominant_cohort(&self) -> Option<CohortId> {
        self.cohort_totals
            .iter()
            .enumerate()
            .max_by_key(|entry| *entry.1)
            .and_then(|(cid, &lines)| {
                if lines > 0 {
                    Some(CohortId(cid as u16))
                } else {
                    None
                }
            })
    }

    /// Adds lines to a cohort total.
    pub fn add_cohort_lines(&mut self, cohort: CohortId, lines: u64) {
        let idx = cohort.0 as usize;
        if idx >= self.cohort_totals.len() {
            self.cohort_totals.resize(idx + 1, 0);
        }
        self.cohort_totals[idx] = self.cohort_totals[idx].saturating_add(lines);
    }

    /// Deducts lines from a cohort total.
    pub fn deduct_cohort_lines(&mut self, cohort: CohortId, lines: u64) {
        let idx = cohort.0 as usize;
        if let Some(total) = self.cohort_totals.get_mut(idx) {
            *total = total.saturating_sub(lines);
        }
    }
}
