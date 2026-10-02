//! Action representation: commit actions affecting files (port of action.cpp).

use crate::file::FileId;
use gource_core::Vec3;

/// Action kind (Create, Modify, Remove).
/// Port of `RAction` subclasses (`CreateAction`, `ModifyAction`, `RemoveAction`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActionKind {
    Create,
    Modify { modify_colour: Vec3 },
    Remove,
}

/// An action performed by a user on a file.
/// Port of `RAction` in `action.h` / `action.cpp`.
#[derive(Debug, Clone)]
pub struct Action {
    pub target: FileId,
    pub timestamp: i64,
    pub t: f32,
    pub progress: f32,
    pub rate: f32,
    pub kind: ActionKind,
    pub colour: Vec3,
    pub applied: bool,
}

impl Action {
    /// Create a new Action with appropriate default colour and rate.
    pub fn new(target: FileId, timestamp: i64, t: f32, kind: ActionKind) -> Self {
        let colour = match kind {
            ActionKind::Create => Vec3::new(0.0, 1.0, 0.0),
            ActionKind::Modify { .. } => Vec3::new(1.0, 0.7, 0.3),
            ActionKind::Remove => Vec3::new(1.0, 0.0, 0.0),
        };
        Self {
            target,
            timestamp,
            t,
            progress: 0.0,
            rate: 0.5,
            kind,
            colour,
            applied: false,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.progress >= 1.0
    }

    /// Advance action progress given `pending_actions_count`.
    /// Returns `(needs_apply, finished_now)`:
    /// - `needs_apply`: true on the very first logic step (`progress == 0.0`)
    /// - `finished_now`: true if progress transitioned to >= 1.0 on this step
    pub fn logic(&mut self, dt: f32, pending_actions_count: usize) -> (bool, bool) {
        if self.progress >= 1.0 {
            return (false, false);
        }

        let needs_apply = !self.applied;
        if needs_apply {
            self.applied = true;
        }

        let action_rate = (self.rate * (pending_actions_count as f32).max(1.0)).min(10.0);

        let old_progress = self.progress;
        self.progress = (self.progress + action_rate * dt).min(1.0);
        let finished_now = old_progress < 1.0 && self.progress >= 1.0;

        (needs_apply, finished_now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotmap::SlotMap;

    #[test]
    fn action_lifecycle() {
        let mut sm: SlotMap<FileId, ()> = SlotMap::with_key();
        let fid = sm.insert(());

        let mut create = Action::new(fid, 100, 1.0, ActionKind::Create);
        assert_eq!(create.colour, Vec3::new(0.0, 1.0, 0.0));
        assert!(!create.is_finished());

        let (apply1, fin1) = create.logic(0.1, 1);
        assert!(apply1);
        assert!(!fin1);
        assert!(create.progress > 0.0);

        // Next step shouldn't need apply
        let (apply2, fin2) = create.logic(0.1, 1);
        assert!(!apply2);
        assert!(!fin2);

        // Finish action
        let (apply3, fin3) = create.logic(10.0, 1);
        assert!(!apply3);
        assert!(fin3);
        assert!(create.is_finished());

        let remove = Action::new(fid, 200, 2.0, ActionKind::Remove);
        assert_eq!(remove.colour, Vec3::new(1.0, 0.0, 0.0));

        let modify = Action::new(
            fid,
            300,
            3.0,
            ActionKind::Modify {
                modify_colour: Vec3::splat(0.5),
            },
        );
        assert_eq!(modify.colour, Vec3::new(1.0, 0.7, 0.3));
    }
}
