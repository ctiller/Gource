//! User representation (port of user.cpp).

use crate::action::Action;
use crate::file::FileId;
use crate::pawn::Pawn;
use glam::{Vec2, Vec3};
use slotmap::new_key_type;

new_key_type! {
    /// User identifier in [`crate::world::World`].
    pub struct UserId;
}

/// A committer/user avatar in the simulation.
/// Port of `RUser` in `user.h` / `user.cpp`.
#[derive(Debug, Clone)]
pub struct User {
    pub pawn: Pawn,

    pub actions: Vec<Action>,
    pub active_actions: Vec<Action>,

    pub action_interval: f32,
    pub action_dist: f32,

    pub last_action: f32,
    pub min_units_ps: f32,

    pub usercol: Vec3,
    pub highlighted: bool,
}

impl User {
    /// Port of `RUser::RUser(const std::string& name, vec2 pos, int tagid)`.
    pub fn new(
        name: &str,
        pos: Vec2,
        tagid: i32,
        max_user_speed: f32,
        user_scale: f32,
        hasher: &gource_core::StringHasher,
    ) -> Self {
        let mut pawn = Pawn::new(name.to_string(), pos, tagid);
        pawn.speed = max_user_speed;
        pawn.size = 20.0 * user_scale;
        pawn.shadow_offset = Vec2::new(2.0, 2.0) * user_scale;
        pawn.shadow = true;
        pawn.name_interval = 5.0;

        let mut user = Self {
            pawn,
            actions: Vec::new(),
            active_actions: Vec::new(),
            action_interval: 0.2,
            action_dist: 50.0,
            last_action: 0.0,
            min_units_ps: 100.0,
            usercol: Vec3::ONE,
            highlighted: false,
        };
        user.colourize(hasher, false);
        user
    }

    /// Port of `RUser::getName()`.
    pub fn name(&self) -> &str {
        self.pawn.name()
    }

    /// Port of `RUser::colourize()`.
    pub fn colourize(&mut self, hasher: &gource_core::StringHasher, custom_uncoloured: bool) {
        if custom_uncoloured {
            self.usercol = Vec3::ONE;
        } else {
            let base = hasher.colour_hash(&self.pawn.name);
            self.usercol = (base * 0.6 + Vec3::ONE * 0.4) * 0.9;
        }
    }

    /// Port of `RUser::addAction(RAction* action)`.
    pub fn add_action(&mut self, action: Action) {
        if self.is_idle() {
            self.pawn.show_name();
        }
        self.actions.push(action);
    }

    /// Port of `RUser::fileRemoved(RFile* f)`.
    pub fn file_removed(&mut self, file_id: FileId) {
        self.actions.retain(|a| a.target != file_id);
        self.active_actions.retain(|a| a.target != file_id);
    }

    /// Port of `RUser::getActionCount()`.
    pub fn action_count(&self) -> usize {
        self.actions.len() + self.active_actions.len()
    }

    /// Port of `RUser::getPendingActionCount()`.
    pub fn pending_action_count(&self) -> usize {
        self.actions.len()
    }

    /// Port of `RUser::isIdle()`.
    pub fn is_idle(&self) -> bool {
        self.actions.is_empty() && self.active_actions.is_empty()
    }

    /// Port of `RUser::isFading()`.
    pub fn is_fading(&self, user_idle_time: f32) -> bool {
        self.is_idle() && (self.pawn.elapsed - self.last_action) > user_idle_time
    }

    /// Port of `RUser::isInactive()`.
    pub fn is_inactive(&self) -> bool {
        self.is_idle() && (self.pawn.elapsed - self.last_action) > 10.0
    }

    /// Port of `RUser::setHighlighted(bool highlight)`.
    pub fn set_highlighted(&mut self, highlighted: bool) {
        self.highlighted = highlighted;
    }

    /// Port of `RUser::isHighlighted()`.
    pub fn is_highlighted(&self) -> bool {
        self.highlighted
    }

    /// Port of `RUser::setSelected(bool selected)`.
    pub fn set_selected(&mut self, selected: bool) {
        self.pawn.set_selected(selected);
    }

    /// Port of `RUser::getAlpha()`.
    pub fn alpha(&self, user_idle_time: f32) -> f32 {
        let mut alpha = self.pawn.alpha();
        if self.pawn.elapsed - self.last_action > user_idle_time {
            let fade = (self.pawn.elapsed - self.last_action - user_idle_time).min(1.0);
            alpha = 1.0 - fade;
        }
        alpha
    }

    /// Port of `RUser::getColour()`.
    pub fn colour(&self) -> Vec3 {
        if self.pawn.selected {
            Vec3::ONE
        } else {
            self.usercol
        }
    }

    /// Port of `RUser::getNameColour()`.
    pub fn name_colour(&self, selection_colour: Vec3, highlight_colour: Vec3) -> Vec3 {
        if self.pawn.selected {
            selection_colour
        } else if self.highlighted {
            highlight_colour
        } else {
            self.pawn.namecol
        }
    }

    /// Port of `RUser::nameVisible()`.
    pub fn name_visible(&self, highlight_all_users: bool) -> bool {
        self.pawn.name_visible() || highlight_all_users || self.highlighted
    }

    /// Port of `RUser::applyForceUser(RUser* u)`.
    pub fn apply_force_user(
        &mut self,
        other_pos: Vec2,
        personal_space_dist: f32,
        rng: &mut fastrand::Rng,
    ) {
        let dir = other_pos - self.pawn.pos;
        let dist = dir.length();

        let desired_dist = if self.action_count() == 0 {
            personal_space_dist
        } else if !self.actions.is_empty() && self.active_actions.is_empty() {
            personal_space_dist * 0.1
        } else {
            personal_space_dist * 0.5
        };

        if dist < 0.001 {
            let rx = (rng.i32(0..100) - 50) as f32;
            let ry = (rng.i32(0..100) - 50) as f32;
            let v = Vec2::new(rx, ry);
            let len = v.length();
            let norm = if len > 0.0 { v / len } else { Vec2::X };
            self.pawn.accel += norm;
            return;
        }

        if dist < desired_dist {
            let norm = dir / dist;
            self.pawn.accel -= (desired_dist - dist) * norm;
        }
    }

    /// Port of `RUser::applyForceAction(RAction* action)`.
    pub fn apply_force_action(
        &mut self,
        target_pos: Vec2,
        action_dist: f32,
        beam_dist: f32,
        rng: &mut fastrand::Rng,
    ) {
        let dir = target_pos - self.pawn.pos;
        let dist = dir.length();
        let desired_dist = action_dist;

        if dist < 0.001 {
            let rx = (rng.i32(0..100) - 50) as f32;
            let ry = (rng.i32(0..100) - 50) as f32;
            let v = Vec2::new(rx, ry);
            let len = v.length();
            let norm = if len > 0.0 { v / len } else { Vec2::X };
            self.pawn.accel += norm;
            return;
        }

        let norm = dir / dist;
        if dist < desired_dist {
            self.pawn.accel -= (desired_dist - dist) * norm;
            return;
        }

        if dist > beam_dist {
            self.pawn.accel += (dist - beam_dist) * norm;
        }
    }

    /// Advance physics, queue active actions that are in range or overdue,
    /// and advance active actions.
    /// Port of `RUser::logic(float t, float dt)`.
    /// `get_file_pos`: closure mapping `FileId` to its absolute position.
    /// Returns a list of actions that were triggered (`(Action, needs_apply, finished_now)`).
    pub fn logic(
        &mut self,
        t: f32,
        dt: f32,
        max_file_lag: f32,
        beam_dist: f32,
        user_friction: f32,
        mut get_file_pos: impl FnMut(FileId) -> Option<Vec2>,
    ) -> Vec<(Action, bool, bool)> {
        self.pawn.logic(dt);
        self.action_interval -= dt;

        let find_nearby_action = !self.actions.is_empty() && self.action_interval <= 0.0;

        // Queue next active action
        let mut i = 0;
        while i < self.actions.len() {
            // Check overdue
            let is_overdue = max_file_lag >= 0.0 && self.actions[i].t < t - max_file_lag;
            if is_overdue {
                let mut action = self.actions.remove(i);
                action.rate = 2.0;
                self.active_actions.push(action);
                continue;
            }

            if !find_nearby_action {
                break;
            }

            let file_pos = get_file_pos(self.actions[i].target);
            let in_range = match file_pos {
                Some(pos) => (pos - self.pawn.pos).length() < beam_dist,
                None => false,
            };

            if in_range {
                let action = self.actions.remove(i);
                self.active_actions.push(action);
                break;
            }

            i += 1;
        }

        if self.action_interval <= 0.0 {
            let total_actions = self.action_count();
            self.action_interval = if total_actions > 0 {
                1.0 / (total_actions as f32)
            } else {
                1.0
            };
        }

        // Advance active actions
        let mut executed_events = Vec::new();
        let pending_count = self.actions.len();
        let mut a_idx = 0;
        while a_idx < self.active_actions.len() {
            let (needs_apply, finished_now) = self.active_actions[a_idx].logic(dt, pending_count);
            if needs_apply || finished_now {
                executed_events.push((
                    self.active_actions[a_idx].clone(),
                    needs_apply,
                    finished_now,
                ));
            }
            if self.active_actions[a_idx].is_finished() {
                self.active_actions.remove(a_idx);
            } else {
                a_idx += 1;
            }
        }

        // Move user
        let speed = self.pawn.speed;
        if self.pawn.accel.length_squared() > speed * speed {
            let len = self.pawn.accel.length();
            if len > 0.0 {
                self.pawn.accel = (self.pawn.accel / len) * speed;
            }
        }

        self.pawn.pos += self.pawn.accel * dt;
        self.pawn.accel *= (1.0 - user_friction * dt).max(0.0);

        executed_events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::ActionKind;
    use gource_core::StringHasher;
    use slotmap::SlotMap;

    #[test]
    fn user_action_handling_and_lifecycle() {
        let hasher = StringHasher::default();
        let mut u = User::new("bob", Vec2::ZERO, 10, 500.0, 1.0, &hasher);
        assert_eq!(u.name(), "bob");
        assert!(u.is_idle());

        let mut sm: SlotMap<FileId, ()> = SlotMap::with_key();
        let f1 = sm.insert(());
        let f2 = sm.insert(());

        let a1 = Action::new(f1, 100, 1.0, ActionKind::Create);
        let a2 = Action::new(
            f2,
            100,
            1.0,
            ActionKind::Modify {
                modify_colour: Vec3::ONE,
            },
        );

        u.add_action(a1);
        u.add_action(a2);
        assert_eq!(u.action_count(), 2);
        assert!(!u.is_idle());

        // File removal
        u.file_removed(f2);
        assert_eq!(u.action_count(), 1);

        // Forces
        let mut rng = fastrand::Rng::with_seed(123);
        u.apply_force_user(Vec2::new(5.0, 0.0), 100.0, &mut rng);
        assert!(u.pawn.accel.x < 0.0);

        u.apply_force_action(Vec2::new(200.0, 0.0), 50.0, 100.0, &mut rng);
        assert!(u.pawn.accel.x != 0.0);

        // Logic step: file in range (dt >= 0.2 so action_interval expires)
        let events = u.logic(2.0, 0.25, 5.0, 150.0, 1.0, |_| Some(Vec2::new(10.0, 0.0)));
        assert_eq!(u.active_actions.len(), 1);
        assert_eq!(events.len(), 1); // newly active action needs apply!
    }

    #[test]
    fn user_idle_and_fading() {
        let hasher = StringHasher::default();
        let mut u = User::new("carol", Vec2::ZERO, 11, 500.0, 1.0, &hasher);
        assert!(u.is_idle());
        assert!(!u.is_fading(3.0));
        assert!(!u.is_inactive());

        u.pawn.elapsed = 4.0;
        assert!(u.is_fading(3.0));
        assert!(!u.is_inactive());

        u.pawn.elapsed = 11.0;
        assert!(u.is_inactive());

        assert_eq!(u.colour(), u.usercol);
        u.set_selected(true);
        assert_eq!(u.colour(), Vec3::ONE);
        assert_eq!(u.name_colour(Vec3::X, Vec3::Y), Vec3::X);
        u.set_selected(false);
        u.set_highlighted(true);
        assert_eq!(u.name_colour(Vec3::X, Vec3::Y), Vec3::Y);
    }
}
