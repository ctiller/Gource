//! The fixed-tick simulation step: drives the integer `gource-scene`
//! kernels over the [`World`] arenas, and derives the float view.
//!
//! One tick, in C++ `Gource::logic` order:
//! [`World::begin_tick`] → [`World::update_sim_bounds`] →
//! [`World::interact_users`] → [`World::update_users`] →
//! [`World::update_dirs`] → [`World::end_tick`]. Each frame then calls
//! [`World::sync_view`] to interpolate between the last two ticks.

use crate::action::ActionKind;
use crate::file::{DirId, FileId};
use crate::profile::{LogicProfile, LogicSpan};
use crate::user::UserId;
use crate::view::{from_fx, from_ivec, lerp_ivec, to_fx};
use crate::world::{Tuning, World};
use glam::Vec2;
use gource_core::{Bounds2D, QuadTree};
use gource_scene::dirs::{DirFrame, DirIn, DirParams};
use gource_scene::files::{Body, DirDisc};
use gource_scene::hash::StateHasher;
use gource_scene::users::{UserIn, UserParams};
use gource_scene::{Fx, IVec2, ONE, TICK_HZ, UNIT};
use gource_settings::GourceSettings;
use slotmap::{Key, SecondaryMap};

/// Seconds per simulation tick.
pub const TICK_DT: f32 = 1.0 / TICK_HZ as f32;

/// [`Tuning`] in fixed point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneParams {
    pub file_diameter: Fx,
    /// One file's area, Q16.
    pub file_area: i64,
    pub padding: Fx,
    pub gravity: Fx,
    pub gravity_on: bool,
    pub action_dist: Fx,
    pub beam_dist: Fx,
    pub personal_space: Fx,
}

impl SceneParams {
    pub fn from_tuning(t: &Tuning) -> Self {
        let file_diameter = to_fx(t.file_diameter);
        Self {
            file_diameter,
            file_area: gource_scene::dirs::file_area(file_diameter),
            padding: to_fx(t.dir_padding),
            gravity: to_fx(t.force_gravity),
            gravity_on: t.gravity,
            action_dist: to_fx(t.action_dist),
            beam_dist: to_fx(t.beam_dist),
            personal_space: to_fx(t.personal_space_dist),
        }
    }
}

/// The exact integer moves of one tick, for reverse playback: undoing them
/// restores every surviving entity's position bit for bit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TickDelta {
    /// `(dir, Δpos, Δspos)`.
    pub dirs: Vec<(DirId, IVec2, IVec2)>,
    pub files: Vec<(FileId, IVec2)>,
    pub users: Vec<(UserId, IVec2)>,
}

impl TickDelta {
    pub fn is_empty(&self) -> bool {
        self.dirs.is_empty() && self.files.is_empty() && self.users.is_empty()
    }

    /// Number of moved entities.
    pub fn len(&self) -> usize {
        self.dirs.len() + self.files.len() + self.users.len()
    }
}

/// A stable RNG key for a slotmap entity.
#[inline]
fn ent<K: Key>(k: K) -> u64 {
    k.data().as_ffi()
}

/// Threads for the directory force pass: `GOURCE_SIM_THREADS` if set,
/// otherwise one per 256 directories, at most 8.
#[cfg(not(target_arch = "wasm32"))]
fn dir_force_threads(n: usize) -> usize {
    use std::sync::OnceLock;
    static REQUESTED: OnceLock<Option<usize>> = OnceLock::new();
    let requested = *REQUESTED.get_or_init(|| {
        std::env::var("GOURCE_SIM_THREADS")
            .ok()
            .and_then(|v| v.parse().ok())
    });
    let hw = std::thread::available_parallelism().map_or(1, |n| n.get());
    dir_force_threads_for(n, requested, hw)
}

#[cfg(target_arch = "wasm32")]
fn dir_force_threads(_n: usize) -> usize {
    1
}

fn dir_force_threads_for(n: usize, requested: Option<usize>, hw: usize) -> usize {
    match requested {
        Some(t) => t.max(1),
        None => (n / 256).clamp(1, hw.clamp(1, 8)),
    }
}

impl World {
    /// Directories in pre-order from the root.
    pub fn preorder_dirs(&self) -> Vec<DirId> {
        let mut out = Vec::with_capacity(self.dirs.len());
        let mut stack = vec![self.root];
        while let Some(d) = stack.pop() {
            if let Some(dir) = self.dirs.get(d) {
                out.push(d);
                stack.extend(dir.children.iter().rev().copied());
            }
        }
        out
    }

    /// C++ `isVisible()` for every directory at once (a directory is visible
    /// if it or any descendant has a visible file).
    pub fn visibility(&self, preorder: &[DirId]) -> SecondaryMap<DirId, bool> {
        let mut vis = SecondaryMap::with_capacity(preorder.len());
        for &d in preorder.iter().rev() {
            let dir = &self.dirs[d];
            let v = dir.visible
                || dir
                    .children
                    .iter()
                    .any(|c| vis.get(*c).copied().unwrap_or(false));
            vis.insert(d, v);
        }
        vis
    }

    /// The absolute simulation position of a file.
    pub fn file_sim_pos(&self, fid: FileId) -> Option<IVec2> {
        let f = self.files.get(fid)?;
        let dir_pos = f
            .dir
            .and_then(|d| self.dirs.get(d))
            .map_or(IVec2::ZERO, |d| d.sim.pos);
        Some(dir_pos + f.sim.pos)
    }

    /// Start a tick: remember every position for view interpolation and
    /// delta recording.
    pub fn begin_tick(&mut self) {
        for d in self.dirs.values_mut() {
            d.sim.prev_pos = d.sim.pos;
            d.sim.prev_spos = d.sim.spos;
        }
        for f in self.files.values_mut() {
            f.sim.prev_pos = f.sim.pos;
        }
        for u in self.users.values_mut() {
            u.sim.prev_pos = u.sim.pos;
        }
    }

    /// Finish a tick: advance the tick counter and return what moved.
    pub fn end_tick(&mut self) -> TickDelta {
        self.tick += 1;
        let mut delta = TickDelta::default();
        for (id, d) in &self.dirs {
            let dp = d.sim.pos - d.sim.prev_pos;
            let ds = d.sim.spos - d.sim.prev_spos;
            if dp != IVec2::ZERO || ds != IVec2::ZERO {
                delta.dirs.push((id, dp, ds));
            }
        }
        for (id, f) in &self.files {
            let dp = f.sim.pos - f.sim.prev_pos;
            if dp != IVec2::ZERO {
                delta.files.push((id, dp));
            }
        }
        for (id, u) in &self.users {
            let dp = u.sim.pos - u.sim.prev_pos;
            if dp != IVec2::ZERO {
                delta.users.push((id, dp));
            }
        }
        delta
    }

    /// Undo one tick's moves (reverse playback). The previous positions are
    /// set to the undone ones so the view interpolates backwards.
    pub fn undo_tick(&mut self, delta: &TickDelta) {
        for &(id, dp, ds) in &delta.dirs {
            if let Some(d) = self.dirs.get_mut(id) {
                d.sim.prev_pos = d.sim.pos;
                d.sim.prev_spos = d.sim.spos;
                d.sim.pos -= dp;
                d.sim.spos -= ds;
            }
        }
        for &(id, dp) in &delta.files {
            if let Some(f) = self.files.get_mut(id) {
                f.sim.prev_pos = f.sim.pos;
                f.sim.pos -= dp;
            }
        }
        for &(id, dp) in &delta.users {
            if let Some(u) = self.users.get_mut(id) {
                u.sim.prev_pos = u.sim.pos;
                u.sim.pos -= dp;
            }
        }
        self.tick = self.tick.saturating_sub(1);
    }

    /// The simulation-space bounds of the visible directories (C++
    /// `dir_bounds`, used to place new users).
    pub fn update_sim_bounds(&mut self) {
        let order = self.preorder_dirs();
        let vis = self.visibility(&order);
        let mut b: Option<(IVec2, IVec2)> = None;
        for d in order {
            if !vis[d] {
                continue;
            }
            let dir = &self.dirs[d];
            let r = IVec2::splat(dir.sim.radii.radius);
            let (lo, hi) = (dir.sim.pos - r, dir.sim.pos + r);
            b = Some(match b {
                Some((a, z)) => (a.min(lo), z.max(hi)),
                None => (lo, hi),
            });
        }
        self.sim_dir_bounds = b;
    }

    /// The centre of the visible directories in simulation space, if they
    /// have any area (C++ `dir_bounds.centre()` in `addUser`).
    pub fn sim_dir_centre(&self) -> Option<IVec2> {
        let (lo, hi) = self.sim_dir_bounds?;
        if hi.x <= lo.x || hi.y <= lo.y {
            return None;
        }
        Some(IVec2::new(
            ((lo.x as i64 + hi.x as i64) / 2) as i32,
            ((lo.y as i64 + hi.y as i64) / 2) as i32,
        ))
    }

    /// C++ `interactUsers` + `applyForceToActions`: user-user personal space
    /// and the pull towards action targets, added to each user's `accel`.
    pub fn interact_users(&mut self) {
        let p = self.params;
        let ids: Vec<UserId> = self.users_by_name.values().copied().collect();
        let mut ins = Vec::with_capacity(ids.len());
        for &uid in &ids {
            let user = &mut self.users[uid];
            if !(user.active_actions.is_empty() && user.actions.is_empty()) {
                user.last_action = user.pawn.elapsed;
            }
            let user = &self.users[uid];
            let targets: Vec<IVec2> = if !user.active_actions.is_empty() {
                user.active_actions
                    .iter()
                    .take(3)
                    .filter_map(|a| self.file_sim_pos(a.target))
                    .collect()
            } else {
                user.actions
                    .first()
                    .and_then(|a| self.file_sim_pos(a.target))
                    .into_iter()
                    .collect()
            };
            let half = user.pawn.size * 0.5;
            ins.push(UserIn {
                id: ent(uid),
                pos: user.sim.pos,
                half: IVec2::new(to_fx(half), to_fx(half * user.pawn.graphic_ratio)),
                personal_space: user.personal_space(p.personal_space),
                targets,
            });
        }
        let params = UserParams {
            action_dist: p.action_dist,
            beam_dist: p.beam_dist,
            seed: self.seed,
        };
        let accels = gource_scene::users::user_accels(&ins, &params, self.tick);
        for (uid, a) in ids.into_iter().zip(accels) {
            self.users[uid].sim.accel += a;
        }
    }

    /// Advance users, execute actions against touched files, move users,
    /// and collect inactive user IDs. Port of `Gource::updateUsers`.
    pub fn update_users(&mut self, t: f32, dt: f32, settings: &GourceSettings) -> Vec<UserId> {
        let mut inactive_users = Vec::new();
        let user_ids: Vec<UserId> = self.users_by_name.values().copied().collect();
        let beam_dist = self.params.beam_dist;
        let friction = to_fx(settings.user_friction);

        for uid in user_ids {
            let events = {
                let files = &self.files;
                let dirs = &self.dirs;
                let user = &mut self.users[uid];
                user.logic(t, dt, settings.max_file_lag, beam_dist, |fid| {
                    files.get(fid).map(|f| {
                        f.dir
                            .and_then(|d| dirs.get(d))
                            .map_or(IVec2::ZERO, |d| d.sim.pos)
                            + f.sim.pos
                    })
                })
            };

            for (action, needs_apply, finished_now) in events {
                if needs_apply {
                    self.apply_action(uid, action.target, action.timestamp, action.colour);
                }
                if finished_now
                    && matches!(action.kind, ActionKind::Remove)
                    && let Some(file) = self.files.get_mut(action.target)
                {
                    file.remove_with_timestamp(action.timestamp);
                }
            }

            if self.weighted_mode {
                self.push_active_targets(uid);
            }

            let user = &mut self.users[uid];
            let speed = to_fx(user.pawn.speed);
            let (pos, accel) =
                gource_scene::users::move_user(user.sim.pos, user.sim.accel, speed, friction);
            user.sim.pos = pos;
            user.sim.accel = accel;

            if user.is_inactive() {
                inactive_users.push(uid);
            }
        }

        inactive_users
    }

    /// A user's action reaching a file: touch it (C++ `RAction::apply`) and,
    /// in weighted mode, kick it away from the user.
    fn apply_action(&mut self, uid: UserId, fid: FileId, timestamp: i64, colour: glam::Vec3) {
        let user_pos = self.users[uid].sim.pos;
        let weighted = self.weighted_mode;
        let root = self.root;
        let Some(file_abs) = self.file_sim_pos(fid) else {
            return;
        };
        let Some(file) = self.files.get_mut(fid) else {
            return;
        };
        let was_hidden = file.pawn.is_hidden();
        if file.touch(timestamp, colour) {
            self.removed_files.retain(|&rf| rf != fid);
        }
        let file = &mut self.files[fid];
        if weighted {
            let beam = (file_abs - user_pos).unit();
            if was_hidden {
                file.sim.size = ONE / 10;
                file.sim.radius = ONE / 20;
                let nudge = match beam {
                    Some(b) => (-b).unit_times(ONE as i64 / 2),
                    None => gource_scene::trig::direction(
                        gource_scene::trig::GOLDEN_ANGLE.wrapping_mul(file.pawn.tagid as u16),
                    )
                    .unit_times(ONE as i64 / 2),
                };
                file.sim.pos = nudge;
                file.sim.prev_pos = nudge;
                file.sim.vel = IVec2::ZERO;
            }
            if let Some(b) = beam {
                file.sim.vel = (file.sim.vel + b.unit_times(6 * ONE as i64)).clamp_len(25 * ONE);
                if let Some(did) = file.dir
                    && did != root
                    && let Some(dir) = self.dirs.get_mut(did)
                {
                    dir.sim.ext_accel += b.unit_times(4 * ONE as i64);
                }
            }
        }
        let file = &self.files[fid];
        if let Some(did) = file.dir {
            if was_hidden {
                self.dirs[did].add_visible();
            }
            self.dirs[did].since_last_file_change = 0.0;
            self.on_node_updated(did, true);
        }
    }

    /// Weighted mode: a gentle continuous push on the files a user's beams
    /// are touching.
    fn push_active_targets(&mut self, uid: UserId) {
        let user_pos = self.users[uid].sim.pos;
        let root = self.root;
        let targets: Vec<FileId> = self.users[uid]
            .active_actions
            .iter()
            .map(|a| a.target)
            .collect();
        for fid in targets {
            let Some(abs) = self.file_sim_pos(fid) else {
                continue;
            };
            let Some(beam) = (abs - user_pos).unit() else {
                continue;
            };
            let file = &mut self.files[fid];
            if file.pawn.is_hidden() {
                continue;
            }
            // 18 units/s^2 over one tick.
            file.sim.vel = (file.sim.vel + beam.unit_times(18 * ONE as i64 / TICK_HZ as i64))
                .clamp_len(20 * ONE);
            if let Some(did) = file.dir
                && did != root
                && let Some(dir) = self.dirs.get_mut(did)
            {
                dir.sim.ext_accel += beam.unit_times(2 * ONE as i64);
            }
        }
    }

    /// The directory force pass, per-directory motion and file logic, and
    /// (weighted mode) the file solver. `dt` is the tick length (timers);
    /// `file_idle_time` is `gGourceSettings.file_idle_time`.
    pub fn update_dirs(&mut self, dt: f32, file_idle_time: f32, profile: &mut LogicProfile) {
        let order = self.preorder_dirs();
        let vis = self.visibility(&order);
        let mut index: SecondaryMap<DirId, u32> = SecondaryMap::with_capacity(order.len());
        for (i, &d) in order.iter().enumerate() {
            index.insert(d, i as u32);
        }
        let ins: Vec<DirIn> = order
            .iter()
            .map(|&d| {
                let dir = &self.dirs[d];
                DirIn {
                    id: ent(d),
                    pos: dir.sim.pos,
                    radius: dir.sim.radii.radius,
                    parent_radius: dir.sim.radii.parent_radius,
                    parent: dir.parent.and_then(|p| index.get(p).copied()),
                    visible: vis[d],
                    empty: dir.is_empty(),
                }
            })
            .collect();
        let frame = DirFrame::from_preorder(ins);
        let params = DirParams {
            gravity: self.params.gravity,
            gravity_on: self.params.gravity_on,
            seed: self.seed,
        };
        let accels = gource_scene::dirs::dir_accels(
            &frame,
            &params,
            self.tick,
            dir_force_threads(order.len()),
        );
        profile.mark(LogicSpan::DirForces);

        let weighted = self.weighted_mode;
        for (i, &d) in order.iter().enumerate() {
            let parent = self.dirs[d].parent;
            let (ppos, gpos) = match parent {
                Some(p) => {
                    let pd = &self.dirs[p];
                    (Some(pd.sim.pos), pd.parent.map(|g| self.dirs[g].sim.pos))
                }
                None => (None, None),
            };
            let hash = self.hasher.hash(&self.dirs[d].abspath);
            let dir = &mut self.dirs[d];
            if let Some(pp) = ppos {
                if !dir.is_empty() && !dir.sim.initialized {
                    let h = gource_scene::dirs::hash_direction(hash);
                    let pos = gource_scene::dirs::initial_position(pp, gpos, h);
                    dir.sim.pos = pos;
                    dir.sim.prev_pos = pos;
                    // C++: spos = pos - (parent - pos) / 2.
                    let spos = pos - (pp - pos).scale(1, 2);
                    dir.sim.spos = spos;
                    dir.sim.prev_spos = spos;
                    dir.sim.initialized = true;
                }
                let accel = accels[i] + dir.sim.ext_accel;
                dir.sim.pos = gource_scene::dirs::integrate(dir.sim.pos, accel);
                dir.sim.spos = gource_scene::dirs::spline_point(dir.sim.spos, dir.sim.pos, pp);
            }
            dir.sim.ext_accel = IVec2::ZERO;

            let file_ids = dir.files.clone();
            for fid in file_ids {
                let Some(file) = self.files.get_mut(fid) else {
                    continue;
                };
                if weighted {
                    file.sim.size =
                        gource_scene::files::animate_size(file.sim.size, file.size_goal());
                    file.sim.radius = (file.sim.size / 2).max(ONE / 20);
                } else {
                    let target = file.sim.dest.unit_times(file.sim.distance as i64);
                    file.sim.pos = gource_scene::files::approach(file.sim.pos, target);
                }
                if file.logic(dt, file_idle_time) {
                    self.removed_files.push(fid);
                }
            }

            let dir = &mut self.dirs[d];
            dir.calc_colour(&self.files);
            if dir.visible {
                dir.since_node_visible += dt;
            }
            dir.since_last_file_change += dt;
            dir.since_last_node_change += dt;
        }
        profile.mark(LogicSpan::DirLogic);

        if weighted {
            self.update_weighted_layout_step(&order);
        }
        profile.mark(LogicSpan::Weighted);
    }

    /// One tick of the weighted layout: each directory's file solver
    /// (post-order), weighted radii, then directory contact resolution.
    pub fn update_weighted_layout_step(&mut self, preorder: &[DirId]) {
        for &d in preorder.iter().rev() {
            let fids: Vec<FileId> = self.dirs[d]
                .files
                .iter()
                .copied()
                .filter(|f| self.files.get(*f).is_some_and(|f| !f.pawn.is_hidden()))
                .collect();
            let mut bodies: Vec<Body> = fids
                .iter()
                .map(|&f| {
                    let file = &self.files[f];
                    Body {
                        pos: file.sim.pos,
                        vel: file.sim.vel,
                        radius: file.sim.radius.max(ONE / 20),
                        id: ent(f),
                    }
                })
                .collect();
            gource_scene::files::step_weighted(&mut bodies, self.seed, self.tick);
            for (f, b) in fids.iter().zip(&bodies) {
                let file = &mut self.files[*f];
                file.sim.pos = b.pos;
                file.sim.vel = b.vel;
            }
            self.recalc_weighted_radius(d);
        }
        self.resolve_directory_contacts();
    }

    /// Weighted-mode radii of one directory from its visible files.
    pub(crate) fn recalc_weighted_radius(&mut self, d: DirId) {
        let children_area = self.children_area(d);
        let files: Vec<(Fx, Fx)> = self.dirs[d]
            .files
            .iter()
            .filter_map(|f| self.files.get(*f))
            .filter(|f| !f.pawn.is_hidden())
            .map(|f| {
                (
                    f.sim.pos.length().saturating_add(f.sim.radius),
                    f.sim.radius,
                )
            })
            .collect();
        let r = gource_scene::files::weighted_radii(&files, children_area, self.params.padding);
        self.dirs[d].set_radii(r);
    }

    /// Push overlapping directory discs apart (weighted mode).
    pub fn resolve_directory_contacts(&mut self) {
        let ids: Vec<DirId> = self.dir_map.values().copied().collect();
        if ids.len() < 2 {
            return;
        }
        let discs: Vec<DirDisc> = ids
            .iter()
            .map(|&d| {
                let dir = &self.dirs[d];
                DirDisc {
                    pos: dir.sim.pos,
                    radius: dir.sim.radii.parent_radius.max(10 * ONE),
                    fixed: d == self.root,
                    empty: dir.is_empty(),
                    id: ent(d),
                }
            })
            .collect();
        let resolved = gource_scene::files::resolve_discs(&discs, self.seed, self.tick);
        for (&d, p) in ids.iter().zip(resolved) {
            let dir = &mut self.dirs[d];
            let delta = p - dir.sim.pos;
            dir.sim.spos += delta;
            dir.sim.pos = p;
        }
    }

    /// Weighted mode: pack every directory's files from scratch and resolve
    /// directory contacts (used when materialising a snapshot).
    pub fn update_weighted_layout(&mut self) {
        let order = self.preorder_dirs();
        for &d in order.iter().rev() {
            let fids: Vec<FileId> = self.dirs[d]
                .files
                .iter()
                .copied()
                .filter(|f| self.files.get(*f).is_some_and(|f| !f.pawn.is_hidden()))
                .collect();
            let radii: Vec<Fx> = fids
                .iter()
                .map(|&f| {
                    let file = &self.files[f];
                    (file.sim.size / 2).max(self.params.file_diameter / 4)
                })
                .collect();
            let placed = gource_scene::files::pack(&radii);
            for (f, p) in fids.iter().zip(placed) {
                let file = &mut self.files[*f];
                file.sim.pos = p;
                file.sim.prev_pos = p;
                file.sim.vel = IVec2::ZERO;
            }
            self.recalc_weighted_radius(d);
        }
        self.resolve_directory_contacts();
    }

    /// Assign ring slots to a directory's visible files (C++
    /// `updateFilePositions`).
    pub(crate) fn assign_ring_slots(&mut self, d: DirId) {
        let dir = &self.dirs[d];
        let slots =
            gource_scene::files::ring_slots(dir.visible_count as u32, self.params.file_diameter);
        let mut it = slots.into_iter();
        let fids = dir.files.clone();
        for fid in fids {
            let Some(file) = self.files.get_mut(fid) else {
                continue;
            };
            if file.pawn.is_hidden() {
                file.sim.dest = IVec2::ZERO;
                file.sim.distance = 0;
                continue;
            }
            let slot = it.next().unwrap_or_default();
            file.sim.dest = slot.dest;
            file.sim.distance = slot.distance;
        }
    }

    /// A 64-bit hash of the whole simulation state, in a canonical order
    /// (paths and names), for determinism checks.
    pub fn state_hash(&self) -> u64 {
        let mut h = StateHasher::new();
        h.write_u64(self.tick);
        for (path, &d) in &self.dir_map {
            let dir = &self.dirs[d];
            h.write_u64(path.len() as u64);
            h.write_vec(dir.sim.pos);
            h.write_vec(dir.sim.spos);
            h.write_i32(dir.sim.radii.radius);
            h.write_i32(dir.sim.radii.parent_radius);
        }
        for &f in self.files_by_path.values() {
            let file = &self.files[f];
            h.write_vec(file.sim.pos);
            h.write_vec(file.sim.vel);
            h.write_i32(file.sim.size);
        }
        for &u in self.users_by_name.values() {
            let user = &self.users[u];
            h.write_vec(user.sim.pos);
            h.write_vec(user.sim.accel);
        }
        h.finish()
    }

    /// Derive the float view: interpolate positions `alpha` of the way from
    /// the previous tick to the current one, apply the view rotation, then
    /// refresh the float bounds and the picking quadtrees.
    pub fn sync_view(&mut self, alpha: f32) {
        let alpha = alpha.clamp(0.0, 1.0);
        let view = self.view;
        let order = self.preorder_dirs();
        for &d in &order {
            let parent_view = self.dirs[d].parent.map(|p| self.dirs[p].pos);
            let dir = &mut self.dirs[d];
            dir.pos = view.point(lerp_ivec(dir.sim.prev_pos, dir.sim.pos, alpha));
            dir.spos = view.point(lerp_ivec(dir.sim.prev_spos, dir.sim.spos, alpha));
            dir.dir_radius = from_fx(dir.sim.radii.radius);
            dir.parent_radius = from_fx(dir.sim.radii.parent_radius);
            dir.node_normal =
                parent_view.map_or(Vec2::ZERO, |pp| (dir.pos - pp).normalize_or_zero());
        }
        let weighted = self.weighted_mode;
        for f in self.files.values_mut() {
            f.pawn.pos = lerp_ivec(f.sim.prev_pos, f.sim.pos, alpha);
            if weighted && f.weighted {
                f.pawn.size = from_fx(f.sim.size);
                f.pawn.dims = Vec2::splat(f.pawn.size);
            }
        }
        for u in self.users.values_mut() {
            u.pawn.pos = view.point(lerp_ivec(u.sim.prev_pos, u.sim.pos, alpha));
        }
        self.update_view_bounds(&order);
    }

    /// Float bounds and picking quadtrees from the view positions (C++
    /// `updateBounds`, `interactUsers`/`interactDirs` tree building).
    fn update_view_bounds(&mut self, order: &[DirId]) {
        self.user_bounds.reset();
        self.active_user_bounds.reset();
        for user in self.users.values() {
            let b = user.pawn.bounds();
            self.user_bounds.update_bounds(&b);
            if !user.is_idle() {
                self.active_user_bounds.update_bounds(&b);
            }
        }

        let vis = self.visibility(order);
        self.dir_bounds.reset();
        for &d in order {
            if vis[d] {
                let dir = &mut self.dirs[d];
                dir.update_quad_item_bounds();
                self.dir_bounds.update_bounds(&dir.quad_item_bounds);
            }
        }

        let max_depth = if self.dir_bounds.area() > 10000.0 {
            self.tuning.max_quadtree_depth
        } else {
            1
        };
        let mut ub = self.user_bounds;
        ub.min -= Vec2::ONE;
        ub.max += Vec2::ONE;
        let mut utree = QuadTree::new(ub, max_depth, 1);
        for &uid in self.users_by_name.values() {
            utree.insert(uid, self.users[uid].pawn.bounds());
        }
        self.user_tree = Some(utree);

        let mut db: Bounds2D = self.dir_bounds;
        db.min -= Vec2::ONE;
        db.max += Vec2::ONE;
        let mut dtree = QuadTree::new(db, max_depth, 1);
        for &d in self.dir_map.values() {
            if !self.dirs[d].is_empty() {
                dtree.insert(d, self.dirs[d].quad_item_bounds);
            }
        }
        self.dir_tree = Some(dtree);
    }

    /// Run `ticks` directory-only ticks (settling a materialised layout).
    pub fn settle(&mut self, ticks: usize) {
        let mut profile = LogicProfile::default();
        for _ in 0..ticks {
            self.begin_tick();
            self.update_sim_bounds();
            self.update_dirs(TICK_DT, 0.0, &mut profile);
            self.end_tick();
        }
        self.sync_view(1.0);
    }

    /// Place a user near `target` (simulation space) at half the action
    /// distance in a pseudo-random direction.
    pub(crate) fn place_user_near(&mut self, uid: UserId, target: IVec2) {
        let dir = gource_scene::rng::random_dir(
            self.seed,
            self.tick,
            ent(uid),
            gource_scene::rng::salt::USER_ACTION,
        );
        let nudge = dir.unit_times(self.params.action_dist as i64 / 2);
        self.users[uid].place(target + nudge);
    }
}

/// Q8 world units → float, for tests and tools.
pub fn fx_to_world(v: IVec2) -> Vec2 {
    from_ivec(v)
}

#[allow(dead_code)]
const _: () = assert!(UNIT > 0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_heuristic() {
        assert_eq!(dir_force_threads_for(10, None, 96), 1);
        assert_eq!(dir_force_threads_for(1000, None, 96), 3);
        assert_eq!(dir_force_threads_for(100_000, None, 96), 8);
        assert_eq!(dir_force_threads_for(100_000, None, 2), 2);
        assert_eq!(dir_force_threads_for(10, Some(0), 96), 1);
        assert_eq!(dir_force_threads_for(10, Some(5), 96), 5);
        assert!(dir_force_threads(10) >= 1);
    }

    #[test]
    fn delta_bookkeeping() {
        let d = TickDelta::default();
        assert!(d.is_empty());
        assert_eq!(d.len(), 0);
    }
}
