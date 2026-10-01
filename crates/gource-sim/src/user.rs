//! User representation (port of user.cpp).

use crate::action::Action;
use crate::file::FileId;
use crate::pawn::Pawn;
use glam::{UVec2, Vec2, Vec3};
use gource_draw::TextureId;
use gource_scene::{Fx, IVec2};
use slotmap::new_key_type;

new_key_type! {
    /// User identifier in [`crate::world::World`].
    pub struct UserId;
}

/// A user's simulation state (fixed point).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UserSim {
    pub pos: IVec2,
    /// Position at the start of the current tick (view interpolation).
    pub prev_pos: IVec2,
    /// C++ `accel`: a decaying velocity, Q8 units/s.
    pub accel: IVec2,
}

/// A committer/user avatar in the simulation.
/// Port of `RUser` in `user.h` / `user.cpp`.
#[derive(Debug, Clone)]
pub struct User {
    /// `pawn.pos` is a float view of [`User::sim`].
    pub pawn: Pawn,
    pub sim: UserSim,

    pub actions: Vec<Action>,
    pub active_actions: Vec<Action>,
    /// Active actions dropped by [`User::file_removed`]. C++ `fileRemoved`
    /// erases them without decrementing `activeCount`, so they keep counting
    /// towards `getActionCount()` (the user's personal space and action
    /// interval) for the rest of the user's life. Kept for parity.
    pub removed_active_count: usize,

    pub action_interval: f32,
    pub action_dist: f32,

    pub last_action: f32,
    pub min_units_ps: f32,

    pub usercol: Vec3,
    pub highlighted: bool,

    /// The user's image (`--user-image-dir` / `--default-user-image`);
    /// `None` draws the built-in `user.png`.
    pub graphic: Option<TextureId>,
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

        let p = crate::view::to_ivec(pos);
        let mut user = Self {
            pawn,
            sim: UserSim {
                pos: p,
                prev_pos: p,
                accel: IVec2::ZERO,
            },
            actions: Vec::new(),
            active_actions: Vec::new(),
            removed_active_count: 0,
            action_interval: 0.2,
            action_dist: 50.0,
            last_action: 0.0,
            min_units_ps: 100.0,
            usercol: Vec3::ONE,
            highlighted: false,
            graphic: None,
        };
        // The built-in-image path of `assignUserImage`; `Gource` calls
        // `assign_graphic` again with the user's real image and its size.
        user.assign_graphic(hasher, None, UVec2::ONE, false);
        user
    }

    /// Port of `RUser::getName()`.
    pub fn name(&self) -> &str {
        self.pawn.name()
    }

    /// Port of `RUser::colourize()`: the raw colour hash of the name. C++
    /// `Gource::changeColours` calls only this, so recoloured users lose the
    /// blend `assignUserImage` applies.
    pub fn colourize(&mut self, hasher: &gource_core::StringHasher) {
        self.usercol = hasher.colour_hash(&self.pawn.name);
    }

    /// Port of `RUser::assignUserImage()` once the image is chosen:
    /// `graphic` is the user's own image (`None` = the built-in `user.png`),
    /// `size` the chosen texture's size in pixels (sets the aspect ratio), and
    /// `uncoloured` is true for a custom image without `--colour-images`.
    pub fn assign_graphic(
        &mut self,
        hasher: &gource_core::StringHasher,
        graphic: Option<TextureId>,
        size: UVec2,
        uncoloured: bool,
    ) {
        self.colourize(hasher);
        if uncoloured {
            self.usercol = Vec3::ONE;
        }
        self.graphic = graphic;
        self.pawn.set_graphic_dimensions(size.x, size.y);
        self.usercol = self.usercol * 0.6 + Vec3::ONE * 0.4;
        self.usercol *= 0.9;
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
        let active = self.active_actions.len();
        self.active_actions.retain(|a| a.target != file_id);
        self.removed_active_count += active - self.active_actions.len();
    }

    /// Port of `RUser::getActionCount()` (`actionCount + activeCount`),
    /// including the active actions C++ never uncounts (see
    /// [`User::removed_active_count`]).
    pub fn action_count(&self) -> usize {
        self.actions.len() + self.active_actions.len() + self.removed_active_count
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

    /// The personal space this user wants (C++ `applyForceUser`): the full
    /// distance when it has no actions, a tenth while it only has pending
    /// ones, half while it is working.
    pub fn personal_space(&self, personal_space_dist: Fx) -> Fx {
        if self.action_count() == 0 {
            personal_space_dist
        } else if !self.actions.is_empty() && self.active_actions.is_empty() {
            personal_space_dist / 10
        } else {
            personal_space_dist / 2
        }
    }

    /// Teleport the user (simulation and view) to `pos`.
    pub fn place(&mut self, pos: IVec2) {
        self.sim.pos = pos;
        self.sim.prev_pos = pos;
        self.pawn.pos = crate::view::from_ivec(pos);
    }

    /// Queue active actions that are in range or overdue, and advance
    /// active actions. Port of `RUser::logic(float t, float dt)` minus the
    /// motion (the integer simulation moves users).
    /// `get_file_pos`: closure mapping `FileId` to its absolute simulation
    /// position; `beam_dist` is Q8.
    /// Returns a list of actions that were triggered (`(Action, needs_apply, finished_now)`).
    pub fn logic(
        &mut self,
        t: f32,
        dt: f32,
        max_file_lag: f32,
        beam_dist: Fx,
        mut get_file_pos: impl FnMut(FileId) -> Option<IVec2>,
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
                Some(pos) => {
                    (pos - self.sim.pos).len_sq() < (beam_dist as i64) * (beam_dist as i64)
                }
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

        // Personal space: pending only -> a tenth.
        assert_eq!(u.personal_space(1000), 100);

        // Logic step: file in range (dt >= 0.2 so action_interval expires)
        let one = gource_scene::ONE;
        let events = u.logic(2.0, 0.25, 5.0, 150 * one, |_| Some(IVec2::new(10 * one, 0)));
        assert_eq!(u.active_actions.len(), 1);
        assert_eq!(events.len(), 1); // newly active action needs apply!
        // Working -> half.
        assert_eq!(u.personal_space(1000), 500);

        u.place(IVec2::new(one, -one));
        assert_eq!(u.pawn.pos, Vec2::new(1.0, -1.0));
        assert_eq!(u.sim.prev_pos, u.sim.pos);
    }

    #[test]
    fn user_idle_and_fading() {
        let hasher = StringHasher::default();
        let mut u = User::new("carol", Vec2::ZERO, 11, 500.0, 1.0, &hasher);
        assert!(u.is_idle());
        assert_eq!(u.personal_space(1000), 1000);
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
