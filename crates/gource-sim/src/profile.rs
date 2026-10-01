//! Per-frame timing of the phases of [`crate::gource::Gource::logic`].
//!
//! Cheap enough to leave on (a dozen clock reads per frame). On wasm32 there
//! is no monotonic clock in `std`, so every operation is a no-op there.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

/// A phase of `Gource::logic`, in execution order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicSpan {
    /// Widgets, rotation, recolouring: everything before playback.
    Prelude,
    /// Whole-simulation snapshot for the reverse-playback buffer.
    ReverseSnapshot,
    /// Scrubber checkpoint capture.
    Checkpoint,
    /// Reading commits from the log into the queue.
    ReadLog,
    /// Reaping deleted files and processing due commits.
    Commits,
    /// Caption layout.
    Captions,
    /// Bounds update and user quadtree/forces.
    InteractUsers,
    /// User movement and selection.
    UpdateUsers,
    /// Directory quadtree build.
    InteractDirs,
    /// Directory-directory repulsion (quadtree queries).
    DirForces,
    /// Directory springs, file layout within directories, file logic.
    DirLogic,
    /// Weighted (file size metric) layout and contact resolution.
    Weighted,
    /// Camera tracking and framing.
    Camera,
    /// Date label and playhead sync.
    Tail,
}

impl LogicSpan {
    /// Every span, in execution order.
    pub const ALL: [LogicSpan; 14] = [
        LogicSpan::Prelude,
        LogicSpan::ReverseSnapshot,
        LogicSpan::Checkpoint,
        LogicSpan::ReadLog,
        LogicSpan::Commits,
        LogicSpan::Captions,
        LogicSpan::InteractUsers,
        LogicSpan::UpdateUsers,
        LogicSpan::InteractDirs,
        LogicSpan::DirForces,
        LogicSpan::DirLogic,
        LogicSpan::Weighted,
        LogicSpan::Camera,
        LogicSpan::Tail,
    ];

    /// Short name for reports.
    pub fn name(self) -> &'static str {
        match self {
            LogicSpan::Prelude => "prelude",
            LogicSpan::ReverseSnapshot => "rev_snap",
            LogicSpan::Checkpoint => "checkpt",
            LogicSpan::ReadLog => "read_log",
            LogicSpan::Commits => "commits",
            LogicSpan::Captions => "captions",
            LogicSpan::InteractUsers => "int_users",
            LogicSpan::UpdateUsers => "upd_users",
            LogicSpan::InteractDirs => "int_dirs",
            LogicSpan::DirForces => "dir_force",
            LogicSpan::DirLogic => "dir_logic",
            LogicSpan::Weighted => "weighted",
            LogicSpan::Camera => "camera",
            LogicSpan::Tail => "tail",
        }
    }
}

/// Milliseconds spent in each [`LogicSpan`] during the last `logic` call.
#[derive(Debug, Clone, Default)]
pub struct LogicProfile {
    /// Indexed by `LogicSpan as usize`.
    pub ms: [f64; LogicSpan::ALL.len()],
    #[cfg(not(target_arch = "wasm32"))]
    last: Option<Instant>,
}

impl LogicProfile {
    /// Clears the timings and starts the clock for the first span.
    pub fn begin(&mut self) {
        self.ms = [0.0; LogicSpan::ALL.len()];
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.last = Some(Instant::now());
        }
    }

    /// Charges the time since the previous mark (or `begin`) to `span`.
    pub fn mark(&mut self, span: LogicSpan) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let now = Instant::now();
            if let Some(last) = self.last {
                self.ms[span as usize] += (now - last).as_secs_f64() * 1e3;
            }
            self.last = Some(now);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = span;
    }

    /// Milliseconds charged to `span`.
    pub fn get(&self, span: LogicSpan) -> f64 {
        self.ms[span as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_accumulate_into_spans() {
        let mut p = LogicProfile::default();
        p.begin();
        std::thread::sleep(std::time::Duration::from_millis(2));
        p.mark(LogicSpan::Commits);
        p.mark(LogicSpan::Commits);
        p.mark(LogicSpan::Camera);
        assert!(p.get(LogicSpan::Commits) >= 1.5);
        assert!(p.get(LogicSpan::Camera) < p.get(LogicSpan::Commits));
        assert_eq!(p.get(LogicSpan::Tail), 0.0);
        p.begin();
        assert_eq!(p.get(LogicSpan::Commits), 0.0);
        let names: Vec<_> = LogicSpan::ALL.iter().map(|s| s.name()).collect();
        assert_eq!(names.len(), 14);
        assert!(names.iter().all(|n| !n.is_empty()));
    }

    #[test]
    fn mark_before_begin_is_ignored() {
        let mut p = LogicProfile::default();
        p.mark(LogicSpan::Prelude);
        assert_eq!(p.get(LogicSpan::Prelude), 0.0);
    }
}
