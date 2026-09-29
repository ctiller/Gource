//! Git-of-Theseus cohort survival, line age tracking, and churn decay models.

use crate::intern::CohortId;
use chrono::{DateTime, Datelike};

/// Cohort grouping mode for classifying lines of code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CohortMode {
    /// Annual cohorts (e.g. `"2024"`).
    #[default]
    Year,
    /// Quarterly cohorts (e.g. `"2024-Q1"`).
    Quarter,
    /// Author-based cohorts (grouped by user name).
    Author,
}

impl CohortMode {
    /// Formats the cohort label for a given commit timestamp and author username.
    pub fn cohort_label(&self, timestamp: i64, username: &str) -> String {
        match self {
            Self::Year => {
                let dt = DateTime::from_timestamp(timestamp, 0).unwrap_or_default();
                format!("{:04}", dt.year())
            }
            Self::Quarter => {
                let dt = DateTime::from_timestamp(timestamp, 0).unwrap_or_default();
                let q = (dt.month0() / 3) + 1;
                format!("{:04}-Q{}", dt.year(), q)
            }
            Self::Author => username.trim().to_string(),
        }
    }
}

/// Decay strategy when lines are removed from a file with multiple cohorts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChurnDecayModel {
    /// Drain newest lines first (default). Reflects that newly added code
    /// is most frequently edited, refactored, or replaced.
    #[default]
    LifoYoungestFirst,
    /// Scale all cohorts proportionally based on current line counts.
    /// Uses deterministic integer remainder distribution so line counts
    /// always match exact totals.
    Proportional,
}

/// Compact cohort line breakdown and churn tracking for a single file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileCohorts {
    /// List of `(cohort_id, surviving_lines)`.
    pub buckets: Vec<(CohortId, u32)>,
    /// Lines added in the most recent operation.
    pub recent_added: u32,
    /// Lines removed in the most recent operation.
    pub recent_removed: u32,
    /// Cumulative lines removed from this file over its entire lifetime.
    pub total_churn_removed: u64,
}

impl FileCohorts {
    /// Creates empty cohorts.
    pub fn new() -> Self {
        Self::default()
    }

    /// Total surviving lines in this file.
    pub fn total_lines(&self) -> u32 {
        self.buckets.iter().map(|(_, l)| *l).sum()
    }

    /// Adds lines associated with a cohort to this file.
    pub fn add_lines(&mut self, cohort: CohortId, lines: u32) {
        self.recent_added = lines;
        if lines == 0 {
            return;
        }
        if let Some(pos) = self.buckets.iter().position(|(c, _)| *c == cohort) {
            self.buckets[pos].1 = self.buckets[pos].1.saturating_add(lines);
        } else {
            self.buckets.push((cohort, lines));
        }
    }

    /// Removes lines from this file according to [`ChurnDecayModel`].
    /// Returns a list of `(CohortId, removed_lines)` showing how many lines
    /// were deducted from each cohort.
    pub fn remove_lines(&mut self, lines: u32, model: ChurnDecayModel) -> Vec<(CohortId, u32)> {
        self.recent_removed = lines;
        if lines == 0 || self.buckets.is_empty() {
            return Vec::new();
        }

        let total = self.total_lines();
        let to_remove = lines.min(total);
        self.total_churn_removed = self.total_churn_removed.saturating_add(to_remove as u64);

        if to_remove >= total {
            let drained: Vec<(CohortId, u32)> = self.buckets.drain(..).collect();
            return drained;
        }

        let mut removed_per_cohort = Vec::new();

        match model {
            ChurnDecayModel::LifoYoungestFirst => {
                let mut remaining_to_remove = to_remove;
                while remaining_to_remove > 0 && !self.buckets.is_empty() {
                    let last_idx = self.buckets.len() - 1;
                    let (cid, cur_lines) = self.buckets[last_idx];
                    if cur_lines <= remaining_to_remove {
                        removed_per_cohort.push((cid, cur_lines));
                        remaining_to_remove -= cur_lines;
                        self.buckets.pop();
                    } else {
                        self.buckets[last_idx].1 -= remaining_to_remove;
                        removed_per_cohort.push((cid, remaining_to_remove));
                        remaining_to_remove = 0;
                    }
                }
            }
            ChurnDecayModel::Proportional => {
                let remaining_target = total - to_remove;
                let num_buckets = self.buckets.len();
                let mut new_counts = Vec::with_capacity(num_buckets);
                let mut fractions: Vec<(usize, u64)> = Vec::with_capacity(num_buckets);
                let mut sum_new: u32 = 0;

                for (idx, &(_, cur_lines)) in self.buckets.iter().enumerate() {
                    let num = (cur_lines as u64) * (remaining_target as u64);
                    let new_l = (num / (total as u64)) as u32;
                    let rem = num % (total as u64);
                    new_counts.push(new_l);
                    fractions.push((idx, rem));
                    sum_new += new_l;
                }

                let mut remainder = remaining_target.saturating_sub(sum_new) as usize;
                // Sort by remainder descending, tie-breaking by original index
                fractions.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                for (idx, _) in fractions {
                    if remainder == 0 {
                        break;
                    }
                    new_counts[idx] += 1;
                    remainder -= 1;
                }

                // Compute removed per cohort and update buckets
                let mut updated_buckets = Vec::with_capacity(num_buckets);
                for (idx, (cid, cur_lines)) in self.buckets.drain(..).enumerate() {
                    let new_lines = new_counts[idx];
                    let deducted = cur_lines.saturating_sub(new_lines);
                    if deducted > 0 {
                        removed_per_cohort.push((cid, deducted));
                    }
                    if new_lines > 0 {
                        updated_buckets.push((cid, new_lines));
                    }
                }
                self.buckets = updated_buckets;
            }
        }

        removed_per_cohort
    }

    /// File churn temperature in `0.0..=1.0`: ratio of lines removed to total lines touched.
    pub fn churn_temperature(&self) -> f32 {
        let live = self.total_lines() as u64;
        let total_touched = live + self.total_churn_removed;
        if total_touched == 0 {
            0.0
        } else {
            (self.total_churn_removed as f32 / total_touched as f32).clamp(0.0, 1.0)
        }
    }
}

/// A measurement of cohort survival at a specific age.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurvivalPoint {
    /// Age in seconds from initial cohort creation.
    pub age_seconds: i64,
    /// Lines surviving from this cohort at this age.
    pub surviving_lines: u64,
    /// Initial lines in this cohort when first introduced.
    pub initial_lines: u64,
}

impl SurvivalPoint {
    /// Survival fraction in `0.0..=1.0`.
    pub fn survival_fraction(&self) -> f32 {
        if self.initial_lines == 0 {
            0.0
        } else {
            (self.surviving_lines as f32 / self.initial_lines as f32).clamp(0.0, 1.0)
        }
    }
}

/// Estimates half-life (in seconds) from a series of survival points.
/// Returns `None` if the cohort has not yet decayed to 50% or if insufficient data.
pub fn estimate_half_life(points: &[SurvivalPoint]) -> Option<f64> {
    if points.len() < 2 {
        return None;
    }
    for window in points.windows(2) {
        let p0 = &window[0];
        let p1 = &window[1];
        let f0 = p0.survival_fraction();
        let f1 = p1.survival_fraction();

        if (f0 >= 0.5 && f1 <= 0.5) || (f0 <= 0.5 && f1 >= 0.5) {
            if (f0 - f1).abs() < 1e-6 {
                return Some(p0.age_seconds as f64);
            }
            // Linear interpolation to 0.5
            let t = (0.5 - f0) / (f1 - f0);
            let age = p0.age_seconds as f64 + t as f64 * (p1.age_seconds - p0.age_seconds) as f64;
            return Some(age.max(0.0));
        }
    }
    None
}
