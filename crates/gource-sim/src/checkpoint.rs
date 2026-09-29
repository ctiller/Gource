//! Deterministic checkpoint store with geometric thinning (Phase 1C).
//!
//! Stores snapshots of simulation state at regular intervals to enable efficient
//! rewinding, seeking, and replaying. When the store reaches its maximum capacity,
//! it thins older checkpoints geometrically (keeping every second checkpoint in the
//! older half) to maintain temporal coverage across the entire run while conserving memory.

use crate::gource::SimSnapshot;

/// In-memory store of simulation snapshots with thinning policy.
#[derive(Clone)]
pub struct CheckpointStore {
    max_checkpoints: usize,
    checkpoints: Vec<SimSnapshot>,
}

impl CheckpointStore {
    /// Create a new `CheckpointStore` with capacity `max_checkpoints`.
    pub fn new(max_checkpoints: usize) -> Self {
        Self {
            max_checkpoints: max_checkpoints.max(2),
            checkpoints: Vec::new(),
        }
    }

    /// Number of checkpoints currently stored.
    pub fn len(&self) -> usize {
        self.checkpoints.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.checkpoints.is_empty()
    }

    /// Remove all checkpoints.
    pub fn clear(&mut self) {
        self.checkpoints.clear();
    }

    /// Insert a new checkpoint snapshot.
    ///
    /// If `len() >= max_checkpoints`, the older half is thinned (keeping every 2nd checkpoint)
    /// before inserting the new snapshot.
    pub fn insert(&mut self, snapshot: SimSnapshot) {
        if self.checkpoints.len() >= self.max_checkpoints {
            self.thin();
        }
        self.checkpoints.push(snapshot);
    }

    /// Apply geometric thinning: keep every 2nd checkpoint in the older half,
    /// keeping all checkpoints in the newer half.
    pub fn thin(&mut self) {
        let n = self.checkpoints.len();
        if n < 2 {
            return;
        }
        let mid = n / 2;
        let mut idx = 0;
        self.checkpoints.retain(|_| {
            let keep = if idx < mid { idx % 2 == 0 } else { true };
            idx += 1;
            keep
        });
    }

    /// Find the nearest checkpoint whose `currtime` is <= `timestamp`.
    pub fn nearest_before_time(&self, timestamp: i64) -> Option<&SimSnapshot> {
        self.checkpoints
            .iter()
            .filter(|s| s.currtime <= timestamp)
            .max_by_key(|s| s.currtime)
    }

    /// Find the nearest checkpoint whose `runtime` is <= `runtime`.
    pub fn nearest_before_runtime(&self, runtime: f32) -> Option<&SimSnapshot> {
        self.checkpoints
            .iter()
            .filter(|s| s.runtime <= runtime)
            .max_by(|a, b| {
                a.runtime
                    .partial_cmp(&b.runtime)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// Return all stored checkpoints as a slice.
    pub fn checkpoints(&self) -> &[SimSnapshot] {
        &self.checkpoints
    }
}
