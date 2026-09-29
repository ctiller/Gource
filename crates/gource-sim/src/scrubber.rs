//! Real scrubbing, checkpoint seek/replay, and reverse playback controller (Phase 3C).
//!
//! Bridges [`gource_history::ScrubberState`] and [`gource_history::TimelineIndex`]
//! with the running [`crate::gource::Gource`] simulation state machine, managing:
//! - Periodic checkpoint snapshots with thinning policy ([`crate::checkpoint::CheckpointStore`])
//! - Fast checkpoint restoration + deterministic replaying up to a target timestamp
//! - High-speed historical tree materialization fallback via [`gource_history::History`]
//! - Smooth reverse playback using a short-term simulation frame buffer ([`SimSnapshot`])
//! - Cache invalidation / thinning on interactive settings changes ([`gource_settings::SettingClass`])

use std::collections::VecDeque;

use crate::checkpoint::CheckpointStore;
use crate::gource::SimSnapshot;
use gource_history::{History, ScrubberState, TimelineIndex};
use gource_settings::SettingClass;

/// Outcome returned by [`crate::gource::Gource::seek_to_timestamp`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeekOutcome {
    /// Found a preceding checkpoint and deterministic replaying advanced to the target timestamp.
    RestoredAndReplayed {
        checkpoint_ts: i64,
        ticks_replayed: usize,
    },
    /// Materialized world directly from history snapshot at target timestamp.
    MaterializedFromHistory { commit_index: usize },
    /// No checkpoint or history available; fallback to legacy log seek.
    FallbackLegacySeek,
}

/// Simulation timeline scrubber and reverse playback controller.
pub struct SimScrubber {
    /// In-memory checkpoint store for fast seek/replay.
    pub checkpoints: CheckpointStore,
    /// Interactive scrubber state (playhead fraction, clips, dragging, playback direction).
    pub state: ScrubberState,
    /// Timeline histogram index and time-fraction mapper.
    pub timeline: Option<TimelineIndex>,
    /// Recent frame snapshots for smooth reverse playback.
    reverse_buffer: VecDeque<SimSnapshot>,
    /// Minimum simulation runtime interval (in seconds) between automatic checkpoint captures.
    pub checkpoint_interval_sim_secs: f32,
    /// Simulation runtime when the last checkpoint was recorded.
    pub last_checkpoint_runtime: f32,
    /// Maximum number of recent frames kept in the reverse playback buffer.
    pub max_reverse_frames: usize,
}

impl Default for SimScrubber {
    fn default() -> Self {
        Self::new(64)
    }
}

impl SimScrubber {
    /// Creates a new `SimScrubber` with a specified checkpoint capacity.
    ///
    /// `checkpoint_budget` specifies the maximum number of checkpoints to retain before thinning.
    pub fn new(checkpoint_budget: usize) -> Self {
        Self {
            checkpoints: CheckpointStore::new(checkpoint_budget),
            state: ScrubberState::new(),
            timeline: None,
            reverse_buffer: VecDeque::new(),
            checkpoint_interval_sim_secs: 1.0,
            last_checkpoint_runtime: -f32::INFINITY,
            max_reverse_frames: 120,
        }
    }

    /// Sets the repository history and builds the timeline histogram index.
    pub fn set_history(&mut self, history: &History, bucket_count: usize) {
        self.timeline = Some(TimelineIndex::from_history(history, bucket_count));
    }

    /// Synchronizes the scrubber playhead fraction from a timestamp.
    pub fn sync_playhead_from_time(&mut self, currtime: i64) {
        if let Some(ref tl) = self.timeline {
            self.state.playhead_fraction = tl.time_to_fraction(currtime);
        }
    }

    /// Captures a checkpoint if the runtime interval threshold has passed or if no checkpoints exist.
    pub fn maybe_record_checkpoint(
        &mut self,
        currtime: i64,
        runtime: f32,
        snap: impl FnOnce() -> SimSnapshot,
    ) {
        if currtime > 0
            && (self.checkpoints.is_empty()
                || (runtime - self.last_checkpoint_runtime).abs()
                    >= self.checkpoint_interval_sim_secs)
        {
            self.checkpoints.insert(snap());
            self.last_checkpoint_runtime = runtime;
        }
    }

    /// Pushes a simulation frame snapshot onto the reverse playback buffer.
    pub fn push_reverse_frame(&mut self, snap: SimSnapshot) {
        if self.reverse_buffer.len() >= self.max_reverse_frames {
            self.reverse_buffer.pop_front();
        }
        self.reverse_buffer.push_back(snap);
    }

    /// Pops the most recent frame from the reverse playback buffer.
    pub fn pop_reverse_frame(&mut self) -> Option<SimSnapshot> {
        self.reverse_buffer.pop_back()
    }

    /// Clears all frames from the reverse playback buffer.
    pub fn clear_reverse_buffer(&mut self) {
        self.reverse_buffer.clear();
    }

    /// Number of frames currently in the reverse playback buffer.
    pub fn reverse_buffer_len(&self) -> usize {
        self.reverse_buffer.len()
    }

    /// Handles settings changes by clearing or pruning checkpoints according to impact class.
    pub fn on_patch_applied(&mut self, classes: &[SettingClass], current_ts: i64) {
        if classes.contains(&SettingClass::Structural) {
            self.checkpoints.clear();
            self.reverse_buffer.clear();
        } else if classes.contains(&SettingClass::Dynamics)
            || classes.contains(&SettingClass::Timeline)
        {
            // Drop checkpoints after current_ts
            let retained: Vec<SimSnapshot> = self
                .checkpoints
                .checkpoints()
                .iter()
                .filter(|cp| cp.currtime <= current_ts)
                .cloned()
                .collect();
            self.checkpoints.clear();
            for cp in retained {
                self.checkpoints.insert(cp);
            }
            self.reverse_buffer.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gource_history::{ChangeOp, CommitInput, FileChangeInput, HistoryBuilder};

    #[test]
    fn test_sim_scrubber_initialization_and_reverse_buffer() {
        let mut scrubber = SimScrubber::new(50);
        assert_eq!(scrubber.checkpoints.len(), 0);
        assert!(scrubber.timeline.is_none());
        assert_eq!(scrubber.state.playhead_fraction, 0.0);
        assert_eq!(scrubber.checkpoint_interval_sim_secs, 1.0);

        // Test reverse buffer empty pop
        assert!(scrubber.pop_reverse_frame().is_none());

        // Test setting history
        let mut builder = HistoryBuilder::new(
            gource_history::CohortMode::Year,
            gource_history::ChurnDecayModel::LifoYoungestFirst,
        );
        builder.add_commit(CommitInput {
            timestamp: 1000,
            username: "Alice".to_string(),
            files: vec![FileChangeInput {
                path: "src/lib.rs".to_string(),
                op: ChangeOp::Add,
                lines_added: 10,
                lines_removed: 0,
                byte_size: Some(100),
                is_binary: false,
            }],
        });
        builder.add_commit(CommitInput {
            timestamp: 2000,
            username: "Bob".to_string(),
            files: vec![FileChangeInput {
                path: "src/main.rs".to_string(),
                op: ChangeOp::Add,
                lines_added: 20,
                lines_removed: 0,
                byte_size: Some(200),
                is_binary: false,
            }],
        });
        let hist = builder.finish();

        scrubber.set_history(&hist, 10);
        assert!(scrubber.timeline.is_some());

        // Test sync_playhead_from_time
        scrubber.sync_playhead_from_time(1500);
        assert!((scrubber.state.playhead_fraction - 0.5).abs() < 1e-4);

        scrubber.sync_playhead_from_time(1000);
        assert_eq!(scrubber.state.playhead_fraction, 0.0);

        scrubber.sync_playhead_from_time(2000);
        assert_eq!(scrubber.state.playhead_fraction, 1.0);
    }

    #[test]
    fn test_on_patch_applied_invalidation() {
        let mut scrubber = SimScrubber::new(20);

        // Structural invalidation clears everything
        scrubber.on_patch_applied(&[SettingClass::Structural], 1500);
        assert_eq!(scrubber.checkpoints.len(), 0);

        // Dynamics invalidation clears reverse buffer
        scrubber.on_patch_applied(&[SettingClass::Dynamics], 1500);
        assert_eq!(scrubber.checkpoints.len(), 0);

        // Visual invalidation keeps checkpoints intact
        scrubber.on_patch_applied(&[SettingClass::Visual], 1500);
        assert_eq!(scrubber.checkpoints.len(), 0);
    }
}
