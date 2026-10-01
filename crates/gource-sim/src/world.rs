//! World state: arenas, scene bookkeeping, physics and drawing (port of gource.cpp).
//!
//! # C++ Call Mapping (src/gource.cpp -> crates/gource-sim/src/world.rs)
//!
//! | C++ Call / Global | Rust API Equivalent |
//! | :--- | :--- |
//! | `gGourceMinDirSize` | [`Tuning::min_dir_size`] |
//! | `gGourceForceGravity` | [`Tuning::force_gravity`] |
//! | `gGourceDirPadding` | [`Tuning::dir_padding`] |
//! | `gGourceGravity` | [`Tuning::gravity`] |
//! | `gGourceNodeDebug` | [`Tuning::node_debug`] |
//! | `gGourceFileDiameter` | [`Tuning::file_diameter`] |
//! | `gGourceBeamDist` | [`Tuning::beam_dist`] |
//! | `gGourceActionDist` | [`Tuning::action_dist`] |
//! | `gGourcePersonalSpaceDist` | [`Tuning::personal_space_dist`] |
//! | `gGourceShadowStrength` | [`Tuning::shadow_strength`] |
//! | `gGourceMaxQuadTreeDepth` | [`Tuning::max_quadtree_depth`] |
//! | `gGourceDirMap` | [`World::dir_map`] |
//! | `gGourceRemovedFiles` | [`World::removed_files`] |
//! | `root` (`RDirNode*`) | [`World::root`] |
//! | `files` (`std::map<std::string, RFile*>`) | [`World::files_by_path`], [`World::files`] |
//! | `users` (`std::map<std::string, RUser*>`) | [`World::users_by_name`], [`World::users`] |
//! | `tagusermap` (`std::map<int, RUser*>`) | [`World::users_by_tag`] |
//! | `user_bounds`, `active_user_bounds`, `dir_bounds` | [`World::user_bounds`], [`World::active_user_bounds`], [`World::dir_bounds`] |
//! | `userTree`, `dirNodeTree` | [`World::user_tree`], [`World::dir_tree`] |
//! | `Gource::addFile(cf)` | [`World::add_file`] |
//! | `Gource::deleteFile(file)` | [`World::delete_file`] |
//! | `Gource::addUser(username)` | [`World::add_user`] |
//! | `Gource::deleteUser(user)` | [`World::delete_user`] |
//! | `Gource::addFileAction(commit, cf, file, t)` | [`World::add_file_action`] |
//! | `Gource::updateBounds()` | [`World::update_sim_bounds`], [`World::sync_view`] |
//! | `Gource::interactUsers()` | [`World::interact_users`] |
//! | `Gource::interactDirs()` | [`World::sync_view`] (picking tree) |
//! | `Gource::updateDirs(dt)` | [`World::update_dirs`] |
//! | `Gource::updateUsers(t, dt)` (per-user simulation) | [`World::update_users`] |
//! | `Gource::changeColours()` | [`World::set_hash_seed`], [`World::change_colours`] |
//! | `Gource::updateVBOs()` + `drawScene()` parts | [`World::draw_scene`] |
//! | `Gource::drawNames()` / root->drawNames | [`World::draw_names`] |

use crate::action::{Action, ActionKind};
use crate::dirnode::DirNode;
use crate::file::{DirId, File, FileId};
use crate::step::SceneParams;
use crate::user::{User, UserId};
use crate::view::ViewTransform;
use glam::{Vec2, Vec3, Vec4};
use gource_core::{Bounds2D, QuadTree, StringHasher};
use gource_draw::font::{FontId, TextStyle};
use gource_draw::list::{DrawList, Material, TextureId, Vertex};
use gource_draw::{Gfx, Projection};
use gource_scene::{IVec2, ONE};
use gource_settings::GourceSettings;
use gource_vcs::commit::{Commit, CommitFile, FileAction};
use slotmap::SlotMap;
use std::collections::BTreeMap;

/// C++ tunable globals and constants with their defaults.
#[derive(Debug, Clone)]
pub struct Tuning {
    pub min_dir_size: f32,
    pub force_gravity: f32,
    pub dir_padding: f32,
    pub gravity: bool,
    pub node_debug: bool,
    pub file_diameter: f32,
    pub beam_dist: f32,
    pub action_dist: f32,
    pub personal_space_dist: f32,
    pub shadow_strength: f32,
    pub max_quadtree_depth: usize,
    pub quadtree_debug: bool,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            min_dir_size: 15.0,
            force_gravity: 10.0,
            dir_padding: 1.5,
            gravity: true,
            node_debug: false,
            file_diameter: 8.0,
            beam_dist: 100.0,
            action_dist: 50.0,
            personal_space_dist: 100.0,
            shadow_strength: 0.5,
            max_quadtree_depth: 6,
            quadtree_debug: false,
        }
    }
}

/// Textures required for drawing the scene.
#[derive(Debug, Clone, Copy)]
pub struct SceneTextures {
    pub file: TextureId,
    pub beam: TextureId,
    pub default_user: TextureId,
}

/// Font IDs required for scene name rendering.
/// Sizes / configuration from C++:
/// - `file`: font size `scaled_filename_font_size` (drop_shadow: true, round: false)
/// - `file_selected`: font size 18 (drop_shadow: true, round: false)
/// - `user`: font size `scaled_user_font_size` (drop_shadow: true, align_top: false)
/// - `user_selected`: font size 18 (drop_shadow: true, align_top: false)
/// - `dir`: font size `scaled_dirname_font_size` (round: false)
#[derive(Debug, Clone, Copy)]
pub struct SceneFonts {
    pub file: FontId,
    pub file_selected: FontId,
    pub user: FontId,
    pub user_selected: FontId,
    pub dir: FontId,
}

/// Information returned when a file is deleted from the simulation.
#[derive(Debug, Clone)]
pub struct DeletedFileInfo {
    pub file_id: FileId,
    pub fullpath: String,
    pub ext: String,
}

/// Information returned when a user is deleted from the simulation.
#[derive(Debug, Clone)]
pub struct DeletedUserInfo {
    pub user_id: UserId,
    pub name: String,
    pub tagid: i32,
}

/// The entire visualizer scene and simulation state.
#[derive(Clone)]
pub struct World {
    pub dirs: SlotMap<DirId, DirNode>,
    pub files: SlotMap<FileId, File>,
    pub users: SlotMap<UserId, User>,

    pub root: DirId,

    pub dir_map: BTreeMap<String, DirId>,
    pub files_by_path: BTreeMap<String, FileId>,
    pub users_by_name: BTreeMap<String, UserId>,
    pub users_by_tag: BTreeMap<i32, UserId>,

    pub user_bounds: Bounds2D,
    pub active_user_bounds: Bounds2D,
    pub dir_bounds: Bounds2D,

    pub user_tree: Option<QuadTree<UserId>>,
    pub dir_tree: Option<QuadTree<DirId>>,

    pub removed_files: Vec<FileId>,

    pub tag_seq: i32,
    pub hasher: StringHasher,
    /// Seed of the counter-based simulation RNG (`gource_scene::rng`).
    pub seed: u64,
    /// Simulation ticks run so far.
    pub tick: u64,
    pub tuning: Tuning,
    /// [`Tuning`] in fixed point.
    pub params: SceneParams,
    /// Simulation-space bounds of the visible directories.
    pub sim_dir_bounds: Option<(IVec2, IVec2)>,
    /// The user's view rotation (applied by [`World::sync_view`]).
    pub view: ViewTransform,
    /// Users created since the owner last drained this list. `Gource` assigns
    /// their images (`RUser::assignUserImage`), which needs the texture store.
    pub new_users: Vec<UserId>,
    pub weighted_mode: bool,
}

impl World {
    /// Create a new World initialized with root directory "/".
    pub fn new(seed: u64, hash_seed: i32) -> Self {
        let mut dirs = SlotMap::with_key();
        let files = SlotMap::with_key();
        let users = SlotMap::with_key();

        let tuning = Tuning::default();
        let params = SceneParams::from_tuning(&tuning);
        let root_node = DirNode::new("/", params.file_area, params.padding);
        let root = dirs.insert(root_node);

        let mut dir_map = BTreeMap::new();
        dir_map.insert("/".to_string(), root);

        Self {
            dirs,
            files,
            users,
            root,
            dir_map,
            files_by_path: BTreeMap::new(),
            users_by_name: BTreeMap::new(),
            users_by_tag: BTreeMap::new(),
            user_bounds: Bounds2D::new(),
            active_user_bounds: Bounds2D::new(),
            dir_bounds: Bounds2D::new(),
            user_tree: None,
            dir_tree: None,
            removed_files: Vec::new(),
            tag_seq: 1,
            hasher: StringHasher::new(hash_seed),
            seed,
            tick: 0,
            tuning,
            params,
            sim_dir_bounds: None,
            view: ViewTransform::default(),
            new_users: Vec::new(),
            weighted_mode: false,
        }
    }

    /// Set a new hash seed and recolour existing files and users.
    /// Port of `Gource::changeColours`.
    pub fn change_colours(&mut self, new_seed: i32) {
        self.hasher.seed = new_seed;
        for (_, user) in &mut self.users {
            user.colourize(&self.hasher);
        }
        for (_, file) in &mut self.files {
            file.colourize(&self.hasher);
        }
    }

    /// Apply physics tuning parameters from [`gource_settings::TuningSettings`].
    pub fn apply_tuning(&mut self, tuning: &gource_settings::TuningSettings) {
        self.tuning.dir_padding = tuning.dir_padding;
        self.tuning.min_dir_size = tuning.min_dir_size;
        self.tuning.file_diameter = tuning.file_diameter;
        self.tuning.force_gravity = tuning.gravity.abs();
        self.tuning.beam_dist = tuning.beam_length;
        self.tuning.action_dist = tuning.action_distance;
        self.tuning.personal_space_dist = tuning.personal_space;
        self.tuning.shadow_strength = tuning.shadow_strength;
        self.params = SceneParams::from_tuning(&self.tuning);
    }

    /// Rotate the view of directories and users by angle (sin, cos),
    /// optionally around a centre point (view space). The simulation itself
    /// never rotates.
    pub fn rotate(&mut self, s: f32, c: f32, centre: Option<Vec2>) {
        self.view.rotate(s, c, centre);
    }

    /// Port of `Gource::addUser(const std::string& username)`.
    pub fn add_user(&mut self, username: &str, settings: &GourceSettings) -> UserId {
        let pos = crate::view::from_ivec(self.sim_dir_centre().unwrap_or(IVec2::ZERO));

        let tagid = self.tag_seq;
        self.tag_seq += 1;

        let mut user = User::new(
            username,
            pos,
            tagid,
            settings.max_user_speed,
            settings.user_scale,
            &self.hasher,
        );

        if settings.highlight_all_users {
            user.set_highlighted(true);
        } else {
            for hl in &settings.highlight_users {
                if !hl.is_empty() && user.name() == hl {
                    user.set_highlighted(true);
                    break;
                }
            }
        }

        let user_id = self.users.insert(user);
        self.users_by_name.insert(username.to_string(), user_id);
        self.users_by_tag.insert(tagid, user_id);
        self.new_users.push(user_id);

        user_id
    }

    /// Port of `Gource::deleteUser(RUser* user)`.
    /// Removes from arenas and mappings; returns metadata for orchestrator.
    pub fn delete_user(&mut self, user_id: UserId) -> Option<DeletedUserInfo> {
        let user = self.users.remove(user_id)?;
        self.users_by_name.remove(user.name());
        self.users_by_tag.remove(&user.pawn.tagid);
        Some(DeletedUserInfo {
            user_id,
            name: user.pawn.name,
            tagid: user.pawn.tagid,
        })
    }

    /// Port of `Gource::addFile(const RCommitFile& cf)`.
    /// Includes max_files check, is-a-directory check, and re-rooting.
    pub fn add_file(&mut self, cf: &CommitFile, settings: &GourceSettings) -> Option<FileId> {
        if settings.max_files > 0 && (self.files.len() as i32) >= settings.max_files {
            return None;
        }

        let mut file_as_dir = cf.filename.clone();
        if !file_as_dir.ends_with('/') {
            file_as_dir.push('/');
        }

        if self.is_dir(self.root, &file_as_dir) {
            return None;
        }

        let tagid = self.tag_seq;
        self.tag_seq += 1;

        let mut file = File::new(
            &cf.filename,
            Vec3::from(cf.colour),
            Vec2::ZERO,
            tagid,
            self.tuning.file_diameter,
            settings.filename_time,
            settings.file_extension_fallback,
        );
        file.created_timestamp = 0;
        file.is_shadow = cf.is_shadow;
        file.shadow_alpha = settings.shadow_alpha;

        if settings.file_size_metric != gource_settings::FileSizeMetric::None {
            file.weighted = true;
            file.pawn.size = 0.1;
            file.sim.size = ONE / 10;
            file.sim.radius = ONE / 20;
            file.sim.target_size = self.params.file_diameter;
            let dir = gource_scene::trig::direction(
                gource_scene::trig::GOLDEN_ANGLE.wrapping_mul(tagid as u16),
            );
            let init_pos = dir.unit_times(ONE as i64 / 2);
            file.sim.pos = init_pos;
            file.sim.prev_pos = init_pos;
            file.pawn.pos = crate::view::from_ivec(init_pos);
        }

        let file_id = self.files.insert(file);
        self.files_by_path.insert(cf.filename.clone(), file_id);

        // Add file to directory tree
        self.add_file_to_tree(self.root, file_id);

        // Re-root while root has a parent
        while let Some(parent_id) = self.dirs[self.root].parent {
            self.root = parent_id;
        }

        Some(file_id)
    }

    /// Port of `Gource::deleteFile(RFile* file)`.
    /// Removes file from dir hierarchy, users' action lists, and world arenas.
    pub fn delete_file(&mut self, file_id: FileId) -> Option<DeletedFileInfo> {
        let file = self.files.remove(file_id)?;
        self.files_by_path.remove(&file.fullpath);

        let was_visible = !file.pawn.is_hidden();

        // Remove from dir tree
        self.remove_file_from_tree(self.root, file_id, &file.path, was_visible);

        // Remove from any users with actions targeting this file
        for (_, user) in &mut self.users {
            user.file_removed(file_id);
        }

        // Clean up removed_files list
        self.removed_files.retain(|&f| f != file_id);

        Some(DeletedFileInfo {
            file_id,
            fullpath: file.fullpath,
            ext: file.ext,
        })
    }

    /// Port of `Gource::addFileAction(const RCommit& commit, const RCommitFile& cf, RFile* file, float t)`.
    pub fn add_file_action(
        &mut self,
        commit: &Commit,
        cf: &CommitFile,
        file_id: FileId,
        t: f32,
        settings: &GourceSettings,
    ) {
        let user_id = match self.users_by_name.get(&commit.username).copied() {
            Some(uid) => uid,
            None => self.add_user(&commit.username, settings),
        };

        let kind = match cf.action {
            FileAction::Delete => ActionKind::Remove,
            FileAction::Add => ActionKind::Create,
            FileAction::Modify | FileAction::Other(_) => ActionKind::Modify {
                modify_colour: Vec3::from(cf.colour),
            },
        };

        let action = Action::new(file_id, commit.timestamp, t, kind);
        if let Some(user) = self.users.get_mut(user_id) {
            user.add_action(action);
        }

        if let Some(file) = self.files.get_mut(file_id) {
            if file.created_timestamp == 0 {
                file.created_timestamp = commit.timestamp;
            }
            if cf.lines_added.is_some() || cf.lines_removed.is_some() {
                file.apply_line_delta(cf.lines_added, cf.lines_removed, settings.file_pulse);
                file.byte_size = (file.lines as u64) * 35;
            }
            if settings.file_size_metric != gource_settings::FileSizeMetric::None {
                let (weight, ref_weight) = match settings.file_size_metric {
                    gource_settings::FileSizeMetric::Lines => (file.lines.max(1) as u64, 100),
                    gource_settings::FileSizeMetric::Size => (file.byte_size.max(1), 3500),
                    gource_settings::FileSizeMetric::Diff => (
                        (cf.lines_added.unwrap_or(0) as u64 + cf.lines_removed.unwrap_or(0) as u64)
                            .max(1),
                        50,
                    ),
                    gource_settings::FileSizeMetric::Churn => {
                        ((file.total_added + file.total_removed).max(1), 100)
                    }
                    gource_settings::FileSizeMetric::None => (1, 1),
                };
                file.set_weight_target(weight, ref_weight, self.tuning.file_diameter);
            }
            if settings.file_colour_mode == gource_settings::FileColourMode::Cohort
                && file.dominant_cohort_colour.is_none()
            {
                let cohort_label = gource_history::CohortMode::Year
                    .cohort_label(commit.timestamp, &commit.username);
                file.dominant_cohort_colour = Some(self.hasher.colour_hash(&cohort_label));
            }
        }
    }

    /// Recursive `isDir` check.
    fn is_dir(&self, dir_id: DirId, path: &str) -> bool {
        let dir = &self.dirs[dir_id];
        if dir.prefixed_by(path) {
            return true;
        }
        if !path.starts_with(dir.path()) {
            return false;
        }
        for &child_id in &dir.children {
            if self.is_dir(child_id, path) {
                return true;
            }
        }
        false
    }

    /// Find all directories matching prefix closest to root.
    pub fn find_dirs(&self, path: &str) -> Vec<DirId> {
        let mut out = Vec::new();
        self.find_dirs_recursive(self.root, path, &mut out);
        out
    }

    fn find_dirs_recursive(&self, dir_id: DirId, path: &str, out: &mut Vec<DirId>) {
        let dir = &self.dirs[dir_id];
        if dir.prefixed_by(path) {
            out.push(dir_id);
            return;
        }
        for &child_id in &dir.children {
            self.find_dirs_recursive(child_id, path, out);
        }
    }

    /// Get all file IDs under a directory recursively.
    pub fn get_files_recursive(&self, dir_id: DirId) -> Vec<FileId> {
        let mut out = Vec::new();
        self.get_files_recursive_inner(dir_id, &mut out);
        out
    }

    fn get_files_recursive_inner(&self, dir_id: DirId, out: &mut Vec<FileId>) {
        if let Some(dir) = self.dirs.get(dir_id) {
            out.extend_from_slice(&dir.files);
            for &child_id in &dir.children {
                self.get_files_recursive_inner(child_id, out);
            }
        }
    }

    /// Recursive tree addition of a file.
    /// Port of `RDirNode::addFile`.
    pub fn add_file_to_tree(&mut self, dir_id: DirId, file_id: FileId) -> bool {
        let file_path = self.files[file_id].path.clone();

        let dir_abspath = self.dirs[dir_id].abspath.clone();
        let is_root = self.dirs[dir_id].parent.is_none();

        // 1. Doesn't match this path at all
        if !file_path.starts_with(&dir_abspath) {
            if !is_root {
                return false;
            }

            // Fork root
            let mut common = self.dirs[dir_id].common_path_prefix(&file_path);
            if common.is_empty() {
                common = "/".to_string();
            }

            let newparent_node = DirNode::new(&common, self.params.file_area, self.params.padding);
            let newparent_id = self.dirs.insert(newparent_node);
            self.dir_map
                .insert(self.dirs[newparent_id].abspath.clone(), newparent_id);

            self.add_node_to_dir(newparent_id, dir_id);
            return self.add_file_to_tree(newparent_id, file_id);
        }

        // 2. Simply change path of root node if empty
        if is_root
            && dir_abspath == "/"
            && file_path != dir_abspath
            && self.dirs[dir_id].files.is_empty()
            && self.dirs[dir_id].children.is_empty()
        {
            self.dir_map.remove(&dir_abspath);
            self.dirs[dir_id].change_path(&file_path);
            self.dir_map
                .insert(self.dirs[dir_id].abspath.clone(), dir_id);
        }

        // 3. Exact directory match: add to this node
        let current_abspath = self.dirs[dir_id].abspath.clone();
        if file_path == current_abspath {
            self.dirs[dir_id].files.push(file_id);
            self.files[file_id].dir = Some(dir_id);
            if !self.files[file_id].pawn.is_hidden() {
                self.dirs[dir_id].visible_count += 1;
            }
            self.on_file_updated(dir_id);
            return true;
        }

        // 4. Try children
        let children = self.dirs[dir_id].children.clone();
        let mut added = false;
        for child_id in children {
            if self.add_file_to_tree(child_id, file_id) {
                added = true;
                break;
            }
        }

        if added && self.dirs[dir_id].parent.is_some() {
            return true;
        }

        // 5. Check if any file in this directory is actually a directory of file_id
        let mut to_remove = Vec::new();
        for &fid in &self.dirs[dir_id].files {
            let fp = &self.files[fid].fullpath;
            if file_path.starts_with(fp) {
                to_remove.push(fid);
            }
        }
        for fid in to_remove {
            self.files[fid].remove_forced();
        }

        if added {
            return true;
        }

        // 6. Create new child node
        let new_child = DirNode::new(&file_path, self.params.file_area, self.params.padding);
        let new_child_id = self.dirs.insert(new_child);
        self.dir_map
            .insert(self.dirs[new_child_id].abspath.clone(), new_child_id);

        self.add_file_to_tree(new_child_id, file_id);
        self.add_node_to_dir(dir_id, new_child_id);

        // 7. Check for common path element among children
        let mut commonpath = String::new();
        let mut common_pos = IVec2::ZERO;
        for &child_id in &self.dirs[dir_id].children {
            let child = &self.dirs[child_id];
            let common = child.common_path_prefix(&file_path);
            if common.len() > current_abspath.len() && common != file_path {
                commonpath = common;
                common_pos = child.sim.pos;
                break;
            }
        }

        if commonpath.len() > current_abspath.len() {
            let mut cnode = DirNode::new(&commonpath, self.params.file_area, self.params.padding);
            cnode.place(common_pos);
            let cnode_id = self.dirs.insert(cnode);
            self.dir_map
                .insert(self.dirs[cnode_id].abspath.clone(), cnode_id);

            let to_reparent: Vec<DirId> = self.dirs[dir_id]
                .children
                .iter()
                .copied()
                .filter(|&cid| cid != cnode_id && self.dirs[cid].prefixed_by(&commonpath))
                .collect();
            self.dirs[dir_id]
                .children
                .retain(|cid| !to_reparent.contains(cid));

            for cid in to_reparent {
                self.add_node_to_dir(cnode_id, cid);
            }

            self.add_node_to_dir(dir_id, cnode_id);
        }

        true
    }

    /// Add a child directory node to parent.
    /// Port of `RDirNode::addNode`.
    pub fn add_node_to_dir(&mut self, parent_id: DirId, node_id: DirId) {
        let node_path = self.dirs[node_id].abspath.clone();

        // Reparent any existing children that are prefixed by node_path
        let to_reparent: Vec<DirId> = self.dirs[parent_id]
            .children
            .iter()
            .copied()
            .filter(|&cid| cid != node_id && self.dirs[cid].prefixed_by(&node_path))
            .collect();
        self.dirs[parent_id]
            .children
            .retain(|cid| !to_reparent.contains(cid));

        for cid in to_reparent {
            self.add_node_to_dir(node_id, cid);
        }

        self.dirs[parent_id].children.push(node_id);
        self.set_parent_dir(node_id, parent_id);
        self.on_node_updated(parent_id, false);
    }

    /// Set parent pointer and update depths / tokens.
    pub fn set_parent_dir(&mut self, child_id: DirId, parent_id: DirId) {
        self.dirs[child_id].parent = Some(parent_id);
        let parent_token_offset = self.dirs[parent_id].path_token_offset;
        self.dirs[child_id].adjust_path(parent_token_offset);
        self.adjust_depth_recursive(child_id);
    }

    fn adjust_depth_recursive(&mut self, dir_id: DirId) {
        let depth = match self.dirs[dir_id].parent {
            Some(pid) => self.dirs[pid].depth + 1,
            None => 1,
        };
        self.dirs[dir_id].depth = depth;
        let children = self.dirs[dir_id].children.clone();
        for cid in children {
            self.adjust_depth_recursive(cid);
        }
    }

    /// Remove a file from the directory hierarchy.
    /// Port of `RDirNode::removeFile`.
    pub fn remove_file_from_tree(
        &mut self,
        dir_id: DirId,
        file_id: FileId,
        file_path: &str,
        was_visible: bool,
    ) -> bool {
        let abspath = self.dirs[dir_id].abspath.clone();
        if !file_path.starts_with(&abspath) {
            return false;
        }

        if file_path == abspath {
            let mut removed = false;
            self.dirs[dir_id].files.retain(|&fid| {
                if fid == file_id {
                    removed = true;
                    false
                } else {
                    true
                }
            });

            if removed {
                if was_visible && self.dirs[dir_id].visible_count > 0 {
                    self.dirs[dir_id].visible_count -= 1;
                }
                self.on_file_updated(dir_id);
                return true;
            }
            return false;
        }

        // Try children
        let children = self.dirs[dir_id].children.clone();
        for child_id in children {
            if self.remove_file_from_tree(child_id, file_id, file_path, was_visible) {
                // Reap the child if it is now empty: C++ checks
                // `noFiles() && noDirs()`, not `empty()` (which only counts
                // visible files) - hidden files still belong to the dir.
                let child = &self.dirs[child_id];
                if child.files.is_empty() && child.children.is_empty() {
                    self.dirs[dir_id].children.retain(|&cid| cid != child_id);
                    self.delete_dir_recursive(child_id);
                    self.on_node_updated(dir_id, false);
                }
                return true;
            }
        }

        false
    }

    fn delete_dir_recursive(&mut self, dir_id: DirId) {
        if let Some(dir) = self.dirs.remove(dir_id) {
            self.dir_map.remove(&dir.abspath);
            for cid in dir.children {
                self.delete_dir_recursive(cid);
            }
        }
    }

    /// The sum of the children's areas (Q16).
    pub(crate) fn children_area(&self, dir_id: DirId) -> i64 {
        self.dirs[dir_id]
            .children
            .iter()
            .filter_map(|&cid| self.dirs.get(cid))
            .map(|c| c.sim.radii.area)
            .sum()
    }

    fn recalc_radius(&mut self, dir_id: DirId) {
        if self.weighted_mode {
            self.recalc_weighted_radius(dir_id);
        } else {
            let children_area = self.children_area(dir_id);
            let (fa, pad) = (self.params.file_area, self.params.padding);
            self.dirs[dir_id].calc_radius(fa, pad, children_area);
        }
    }

    fn on_file_updated(&mut self, dir_id: DirId) {
        self.recalc_radius(dir_id);
        self.dirs[dir_id].since_last_file_change = 0.0;
        self.on_node_updated(dir_id, false);
    }

    pub(crate) fn on_node_updated(&mut self, dir_id: DirId, user_initiated: bool) {
        if user_initiated {
            self.dirs[dir_id].since_last_node_change = 0.0;
        }
        self.recalc_radius(dir_id);
        if !self.weighted_mode {
            self.assign_ring_slots(dir_id);
        }
        if self.dirs[dir_id].visible
            && self.dirs[dir_id].children.is_empty()
            && self.dirs[dir_id].files.is_empty()
        {
            self.dirs[dir_id].visible = false;
        }

        if let Some(parent_id) = self.dirs[dir_id].parent {
            self.on_node_updated(parent_id, true);
        }
    }

    /// Calculate edge geometry and screen positions.
    /// Port of `root->calcEdges()`, `calcScreenPos` and frustum checking.
    pub fn prepare_frame(&mut self, proj: &Projection, settings: &GourceSettings) {
        let v_bounds = proj.visible_bounds();
        self.prepare_dirs_recursive(self.root, proj, &v_bounds, settings);

        for (_, user) in &mut self.users {
            let top = Vec2::new(user.pawn.pos.x, user.pawn.pos.y - user.pawn.dims.y * 0.5);
            let screen = proj.to_screen(top);
            user.pawn.screenpos = Vec3::new(screen.x, screen.y, 0.0);
        }
    }

    fn prepare_dirs_recursive(
        &mut self,
        dir_id: DirId,
        proj: &Projection,
        v_bounds: &Bounds2D,
        settings: &GourceSettings,
    ) {
        let parent_data = self.dirs[dir_id]
            .parent
            .map(|pid| (self.dirs[pid].projected_pos, self.dirs[pid].col));

        let dir = &mut self.dirs[dir_id];
        dir.in_frustum = v_bounds.overlaps(&dir.quad_item_bounds);
        dir.projected_pos = proj.to_screen(dir.pos);
        dir.projected_spos = proj.to_screen(dir.spos);

        if let Some((p_pos, p_col)) = parent_data {
            let c_pos = dir.projected_pos;
            let c_col = dir.col;
            let spos = dir.projected_spos;
            dir.spline
                .update(p_pos, p_col, c_pos, c_col, spos, settings.dir_name_position);
        }

        for &fid in &dir.files {
            if let Some(file) = self.files.get_mut(fid) {
                let abs_pos = file.absolute_pos(dir.pos);
                let offset = if file.pawn.is_selected() {
                    Vec2::new(5.5, -2.0)
                } else {
                    Vec2::new(5.5, -1.0)
                };
                let screen = proj.to_screen(abs_pos + offset);
                file.pawn.screenpos = Vec3::new(screen.x, screen.y, 0.0);
            }
        }

        let children = dir.children.clone();
        for cid in children {
            self.prepare_dirs_recursive(cid, proj, v_bounds, settings);
        }
    }

    /// Port of `Gource::drawScene`:
    /// Emits all batches in exact C++ order into `DrawList`:
    /// 1. updateAndDrawEdges (edge shadows then edges)
    /// 2. drawFileShadows
    /// 3. drawActions
    /// 4. drawFiles
    /// 5. drawUserShadows
    /// 6. drawUsers
    /// 7. drawBloom
    pub fn draw_scene(
        &self,
        list: &mut DrawList,
        proj: &Projection,
        settings: &GourceSettings,
        textures: &SceneTextures,
    ) {
        let shadow_mult = self.tuning.shadow_strength;

        // 1. Edges
        if !settings.hide_tree {
            // Edge shadows (offset by (2, 2) screen pixels)
            self.draw_edges_shadow_recursive(self.root, list, textures.beam, shadow_mult, settings);
            // Edges
            self.draw_edges_recursive(self.root, list, textures.beam, settings);
        }

        // 2. File shadows (offset (2, 2) world units)
        if !settings.hide_files {
            let offset_world = Vec2::new(2.0, 2.0);
            self.draw_files_shadow_recursive(
                self.root,
                list,
                proj,
                textures.file,
                offset_world,
                shadow_mult,
            );
        }

        // 3. Actions (drawn with beam texture)
        if !settings.hide_users {
            self.draw_user_actions(list, proj, textures.beam);
        }

        // 4. Files
        if !settings.hide_files {
            self.draw_files_recursive(self.root, list, proj, textures.file, settings);
        }

        // C++ `updateVBOs`: `--fixed-user-size` keeps users the same size on
        // screen by scaling their world size with the camera distance.
        let user_scale_factor = if settings.fixed_user_size {
            proj.distance / -crate::camera::STARTING_Z
        } else {
            1.0
        };

        // 5. User shadows (offset (2, 2) * user_scale world units)
        if !settings.hide_users {
            let offset_world = Vec2::new(2.0, 2.0) * settings.user_scale;
            for user_id in self.users_by_name.values() {
                let user = &self.users[*user_id];
                if user.pawn.is_hidden() || !user.pawn.shadow {
                    continue;
                }
                let alpha = user.alpha(settings.user_idle_time) * shadow_mult;
                let world_pos = user.pawn.pos + offset_world;
                let screen_pos = proj.to_screen(world_pos);
                let screen_size = proj.to_screen_len(user.pawn.size * user_scale_factor);
                let dims = Vec2::new(screen_size, screen_size * user.pawn.graphic_ratio);
                let col = Vec4::new(0.0, 0.0, 0.0, alpha);
                let tex = user.graphic.unwrap_or(textures.default_user);
                list.rect(tex, screen_pos - dims * 0.5, dims, col);
            }
        }

        // 6. Users
        if !settings.hide_users {
            for user_id in self.users_by_name.values() {
                let user = &self.users[*user_id];
                if user.pawn.is_hidden() {
                    continue;
                }
                let alpha = user.alpha(settings.user_idle_time);
                let col = user.colour();
                let screen_pos = proj.to_screen(user.pawn.pos);
                let screen_size = proj.to_screen_len(user.pawn.size * user_scale_factor);
                let dims = Vec2::new(screen_size, screen_size * user.pawn.graphic_ratio);
                let colour = Vec4::new(col.x, col.y, col.z, alpha);
                let tex = user.graphic.unwrap_or(textures.default_user);
                list.rect(tex, screen_pos - dims * 0.5, dims, colour);
            }
        }

        // 7. Bloom (glow around directory nodes)
        if !settings.hide_bloom {
            self.draw_bloom_recursive(self.root, list, proj, settings);
        }
    }

    fn draw_edges_shadow_recursive(
        &self,
        dir_id: DirId,
        list: &mut DrawList,
        beam_tex: TextureId,
        shadow_strength: f32,
        settings: &GourceSettings,
    ) {
        let dir = &self.dirs[dir_id];
        if dir.parent.is_some()
            && (!settings.hide_root || self.dirs[dir.parent.unwrap()].parent.is_some())
        {
            let offset = Vec2::new(2.0, 2.0);
            let count = dir.spline.spline_point.len();
            if count >= 2 {
                for i in 0..(count - 1) {
                    let p1 = dir.spline.spline_point[i] + offset;
                    let p2 = dir.spline.spline_point[i + 1] + offset;
                    let dir_vec = p1 - p2;
                    let len = dir_vec.length();
                    let perp = if len > 0.0 {
                        Vec2::new(-dir_vec.y, dir_vec.x) / len * 2.5
                    } else {
                        Vec2::ZERO
                    };
                    let col = Vec4::new(0.0, 0.0, 0.0, shadow_strength);
                    let corners = [p1 + perp, p1 - perp, p2 - perp, p2 + perp];
                    let uvs = [
                        Vec2::new(1.0, 0.0),
                        Vec2::new(0.0, 0.0),
                        Vec2::new(0.0, 0.0),
                        Vec2::new(1.0, 0.0),
                    ];
                    list.quad(beam_tex, corners, uvs, col);
                }
            }
        }

        for &cid in &dir.children {
            if self.dirs[cid].is_visible(&self.dirs) {
                self.draw_edges_shadow_recursive(cid, list, beam_tex, shadow_strength, settings);
            }
        }
    }

    fn draw_edges_recursive(
        &self,
        dir_id: DirId,
        list: &mut DrawList,
        beam_tex: TextureId,
        settings: &GourceSettings,
    ) {
        let dir = &self.dirs[dir_id];
        if dir.parent.is_some()
            && (!settings.hide_root || self.dirs[dir.parent.unwrap()].parent.is_some())
        {
            let count = dir.spline.spline_point.len();
            if count >= 2 {
                for i in 0..(count - 1) {
                    let p1 = dir.spline.spline_point[i];
                    let p2 = dir.spline.spline_point[i + 1];
                    let c1 = dir.spline.spline_colour[i];
                    let c2 = dir.spline.spline_colour[i + 1];

                    let dir_vec = p1 - p2;
                    let len = dir_vec.length();
                    let perp = if len > 0.0 {
                        Vec2::new(-dir_vec.y, dir_vec.x) / len * 2.5
                    } else {
                        Vec2::ZERO
                    };

                    let corners = [p1 + perp, p1 - perp, p2 - perp, p2 + perp];
                    let uvs = [
                        Vec2::new(1.0, 0.0),
                        Vec2::new(0.0, 0.0),
                        Vec2::new(0.0, 0.0),
                        Vec2::new(1.0, 0.0),
                    ];
                    let colours = [c1, c1, c2, c2];
                    list.quad_colours(beam_tex, corners, uvs, colours);
                }
            }
        }

        for &cid in &dir.children {
            if self.dirs[cid].is_visible(&self.dirs) {
                self.draw_edges_recursive(cid, list, beam_tex, settings);
            }
        }
    }

    fn draw_files_shadow_recursive(
        &self,
        dir_id: DirId,
        list: &mut DrawList,
        proj: &Projection,
        file_tex: TextureId,
        offset_world: Vec2,
        shadow_strength: f32,
    ) {
        let dir = &self.dirs[dir_id];
        if dir.in_frustum {
            for &fid in &dir.files {
                let file = &self.files[fid];
                if file.pawn.is_hidden() {
                    continue;
                }
                let world_pos = file.absolute_pos(dir.pos) + offset_world;
                let screen_pos = proj.to_screen(world_pos);
                let screen_size = proj.to_screen_len(file.pawn.size);
                let dims = Vec2::new(screen_size, screen_size * file.pawn.graphic_ratio);
                let alpha = file.alpha() * shadow_strength;
                let col = Vec4::new(0.0, 0.0, 0.0, alpha);
                list.rect(file_tex, screen_pos - dims * 0.5, dims, col);
            }
        }

        for &cid in &dir.children {
            self.draw_files_shadow_recursive(
                cid,
                list,
                proj,
                file_tex,
                offset_world,
                shadow_strength,
            );
        }
    }

    fn draw_user_actions(&self, list: &mut DrawList, proj: &Projection, beam_tex: TextureId) {
        for user_id in self.users_by_name.values() {
            let user = &self.users[*user_id];
            for a in &user.active_actions {
                if a.is_finished() {
                    continue;
                }
                let file = match self.files.get(a.target) {
                    Some(f) => f,
                    None => continue,
                };
                let dir_pos = file
                    .dir
                    .and_then(|d| self.dirs.get(d))
                    .map(|d| d.pos)
                    .unwrap_or(Vec2::ZERO);
                let src_world = user.pawn.pos;
                let dest_world = file.absolute_pos(dir_pos);

                let src = proj.to_screen(src_world);
                let dest = proj.to_screen(dest_world);

                let dir_vec = dest - src;
                let len = dir_vec.length();
                let n = if len > 0.0 { dir_vec / len } else { Vec2::ZERO };
                let perp = Vec2::new(-n.y, n.x);

                let target_screen_size = proj.to_screen_len(file.pawn.size);
                let offset = perp * target_screen_size * 0.5;
                let offset_src = offset * 0.3;

                let is_shadow_action = file.is_shadow || user.name().starts_with("worktree:");
                let alpha_mult = if is_shadow_action { 0.45 } else { 1.0 };
                let alpha = (1.0 - a.progress) * alpha_mult;
                // C++: `alpha * 0.1` (a double literal).
                let alpha2 = (alpha as f64 * 0.1) as f32;

                let beam_col = if is_shadow_action {
                    a.colour.lerp(Vec3::new(0.6, 0.85, 1.0), 0.45)
                } else {
                    a.colour
                };

                let col1 = Vec4::new(beam_col.x, beam_col.y, beam_col.z, alpha);
                let col2 = Vec4::new(beam_col.x, beam_col.y, beam_col.z, alpha2);

                let v1 = Vertex::new(src - offset_src, Vec2::new(0.0, 0.0), col2);
                let v2 = Vertex::new(src + offset_src, Vec2::new(0.0, 1.0), col2);
                let v3 = Vertex::new(dest + offset, Vec2::new(1.0, 1.0), col1);
                let v4 = Vertex::new(dest - offset, Vec2::new(1.0, 0.0), col1);

                list.push_quad(Material::Alpha, beam_tex, [v1, v2, v3, v4]);
            }
        }
    }

    fn draw_files_recursive(
        &self,
        dir_id: DirId,
        list: &mut DrawList,
        proj: &Projection,
        file_tex: TextureId,
        settings: &GourceSettings,
    ) {
        let dir = &self.dirs[dir_id];
        if dir.in_frustum {
            for &fid in &dir.files {
                let file = &self.files[fid];
                if file.pawn.is_hidden() {
                    continue;
                }
                let world_pos = file.absolute_pos(dir.pos);
                let screen_pos = proj.to_screen(world_pos);

                // Render pulse ring if active
                if let Some((ring_size, ring_col)) = file.pulse_visual() {
                    let rdims = Vec2::splat(proj.to_screen_len(ring_size));
                    list.rect(file_tex, screen_pos - rdims * 0.5, rdims, ring_col);
                }

                let screen_size = proj.to_screen_len(file.pawn.size);
                let dims = Vec2::new(screen_size, screen_size * file.pawn.graphic_ratio);
                let mut c =
                    if settings.file_colour_mode == gource_settings::FileColourMode::Extension {
                        file.colour()
                    } else {
                        file.display_colour(settings.file_colour_mode, 0)
                    };
                if file.is_shadow {
                    // Blend towards ethereal cyan
                    c = c.lerp(Vec3::new(0.6, 0.85, 1.0), 0.45);
                }
                let alpha = file.alpha();
                let col = Vec4::new(c.x, c.y, c.z, alpha);
                list.rect(file_tex, screen_pos - dims * 0.5, dims, col);
            }
        }

        for &cid in &dir.children {
            self.draw_files_recursive(cid, list, proj, file_tex, settings);
        }
    }

    fn draw_bloom_recursive(
        &self,
        dir_id: DirId,
        list: &mut DrawList,
        proj: &Projection,
        settings: &GourceSettings,
    ) {
        let dir = &self.dirs[dir_id];
        if dir.in_frustum && dir.is_visible(&self.dirs) {
            let bloom_radius = dir.dir_radius * 2.0 * settings.bloom_multiplier;
            let screen_radius = proj.to_screen_len(bloom_radius);
            let screen_centre = proj.to_screen(dir.pos);
            let bloom_col = dir.col * settings.bloom_intensity;
            list.bloom(
                screen_centre,
                screen_radius,
                Vec4::new(bloom_col.x, bloom_col.y, bloom_col.z, 1.0),
            );
        }

        for &cid in &dir.children {
            self.draw_bloom_recursive(cid, list, proj, settings);
        }
    }

    /// Port of `Gource::drawNames`: renders directory, user and file names.
    pub fn draw_names(
        &self,
        list: &mut DrawList,
        gfx: &mut Gfx,
        settings: &GourceSettings,
        fonts: &SceneFonts,
        selected_user: Option<UserId>,
        selected_file: Option<FileId>,
    ) {
        // 1. Directory names
        if !settings.hide_dirnames {
            self.draw_dir_names_recursive(self.root, list, gfx, settings, fonts.dir);
        }

        // 2. Unselected file names
        if !settings.hide_filenames && !settings.hide_files {
            self.draw_file_names_recursive(self.root, list, gfx, settings, fonts.file, false);
        }

        // 3. Unselected user names
        if !settings.hide_usernames && !settings.hide_users {
            for user_id in self.users_by_name.values() {
                if Some(*user_id) == selected_user {
                    continue;
                }
                let user = &self.users[*user_id];
                let is_visible = user.name_visible(settings.highlight_all_users);
                if !is_visible {
                    continue;
                }
                let u_alpha = user.alpha(settings.user_idle_time);
                let alpha = if user.is_highlighted() || settings.highlight_all_users {
                    u_alpha
                } else {
                    user.pawn.name_alpha()
                };
                if alpha <= 0.01 {
                    continue;
                }
                let col = user.name_colour(settings.selection_colour, settings.highlight_colour);
                let font_id = fonts.user;
                let text_w = gfx.text_width(font_id, user.name());
                let font_h = gfx.fonts.max_height(font_id);
                let pos = Vec2::new(
                    user.pawn.screenpos.x - text_w * 0.5,
                    user.pawn.screenpos.y - font_h,
                );
                let style = TextStyle::default()
                    .with_colour(Vec4::new(col.x, col.y, col.z, alpha))
                    .with_shadow(true)
                    .with_align_top(false);
                gfx.draw_text(list, font_id, pos, user.name(), &style);
            }
        }

        // 4. Selected user drawn on top
        if let Some(uid) = selected_user
            && let Some(user) = self.users.get(uid)
            && !user.pawn.is_hidden()
            && !settings.hide_usernames
            && !settings.hide_users
        {
            let alpha = user.alpha(settings.user_idle_time);
            if alpha > 0.01 {
                let font_id = fonts.user_selected;
                let text_w = gfx.text_width(font_id, user.name());
                let font_h = gfx.fonts.max_height(font_id);
                let pos = Vec2::new(
                    user.pawn.screenpos.x - text_w * 0.5,
                    user.pawn.screenpos.y - font_h,
                );
                let col = settings.selection_colour;
                let style = TextStyle::default()
                    .with_colour(Vec4::new(col.x, col.y, col.z, alpha))
                    .with_shadow(true)
                    .with_align_top(false);
                gfx.draw_text(list, font_id, pos, user.name(), &style);
            }
        }

        // 5. Selected file drawn on top
        if let Some(fid) = selected_file
            && let Some(file) = self.files.get(fid)
        {
            let font_id = fonts.file_selected;
            let text_pos = file.pawn.screenpos.truncate();
            let label = file.display_name(settings.file_extensions);
            let col = settings.selection_colour;
            let style = TextStyle::default()
                .with_colour(Vec4::new(col.x, col.y, col.z, 1.0))
                .with_shadow(true)
                .with_round(false);
            gfx.draw_text(list, font_id, text_pos, label, &style);
        }
    }

    fn draw_dir_names_recursive(
        &self,
        dir_id: DirId,
        list: &mut DrawList,
        gfx: &mut Gfx,
        settings: &GourceSettings,
        dir_font: FontId,
    ) {
        let dir = &self.dirs[dir_id];
        if dir.parent.is_some() && dir.is_visible(&self.dirs) {
            let depth_ok =
                settings.dir_name_depth <= 0 || (dir.depth - 1) <= settings.dir_name_depth;
            let time_ok = settings.highlight_dirs || dir.since_last_node_change <= 5.0;

            if depth_ok && time_ok {
                let alpha = if settings.highlight_dirs {
                    1.0
                } else {
                    ((5.0 - dir.since_last_node_change) / 5.0).max(0.0)
                };
                let col = settings.dir_colour;
                let label_pos = dir.spline.label_pos();
                let style = TextStyle::default()
                    .with_colour(Vec4::new(col.x, col.y, col.z, alpha))
                    .with_shadow(true)
                    .with_round(false);
                gfx.draw_text(list, dir_font, label_pos, &dir.path_token, &style);
            }
        }

        for &cid in &dir.children {
            self.draw_dir_names_recursive(cid, list, gfx, settings, dir_font);
        }
    }

    fn draw_file_names_recursive(
        &self,
        dir_id: DirId,
        list: &mut DrawList,
        gfx: &mut Gfx,
        settings: &GourceSettings,
        file_font: FontId,
        _selected_only: bool,
    ) {
        let dir = &self.dirs[dir_id];
        if dir.in_frustum {
            for &fid in &dir.files {
                let file = &self.files[fid];
                if file.pawn.is_selected() {
                    continue;
                }
                let alpha = file.pawn.name_alpha();
                if alpha <= 0.01 {
                    continue;
                }
                let col = settings.filename_colour;
                let text_pos = file.pawn.screenpos.truncate();
                let style = TextStyle::default()
                    .with_colour(Vec4::new(col.x, col.y, col.z, alpha))
                    .with_shadow(true)
                    .with_round(false);
                let label = file.display_name(settings.file_extensions);
                gfx.draw_text(list, file_font, text_pos, label, &style);
            }
        }

        for &cid in &dir.children {
            self.draw_file_names_recursive(cid, list, gfx, settings, file_font, _selected_only);
        }
    }

    /// Reconstructs the complete world state from a historical [`gource_history::TreeSnapshot`].
    /// Clears existing tree and files, reconstructs active directories and files with their metrics,
    /// positions users near their active working areas, and runs settle steps of simulation physics.
    pub fn materialize_from_snapshot(
        &mut self,
        snap: &gource_history::TreeSnapshot,
        history: &gource_history::History,
        settings: &GourceSettings,
        settle_steps: usize,
    ) {
        // Reset dirs and files, keeping only the root
        self.files.clear();
        self.files_by_path.clear();
        self.removed_files.clear();
        self.users.clear();
        self.users_by_name.clear();
        self.users_by_tag.clear();
        self.new_users.clear();
        self.user_tree = None;
        self.dir_tree = None;

        self.weighted_mode = settings.file_size_metric != gource_settings::FileSizeMetric::None;

        self.dirs.clear();
        self.dir_map.clear();
        let root_node = DirNode::new("/", self.params.file_area, self.params.padding);
        self.root = self.dirs.insert(root_node);
        self.dir_map.insert("/".to_string(), self.root);

        // Track last-touched file position for each active user to place them accurately
        let mut user_last_file_path: BTreeMap<String, String> = BTreeMap::new();

        // Materialize live files
        for live in &snap.files {
            let fullpath = match history.paths.resolve(live.path) {
                Some(p) => p.to_string(),
                None => continue,
            };

            let colour = history
                .paths
                .get(live.path)
                .map(|e| e.colour)
                .unwrap_or(gource_vcs::commit::WHITE);

            let cf = CommitFile {
                filename: fullpath.clone(),
                action: FileAction::Add,
                colour,
                ..Default::default()
            };

            let fid = match self.add_file(&cf, settings) {
                Some(id) => id,
                None => continue,
            };

            // Find dominant cohort color if present
            let dominant_cohort_colour = live
                .cohorts
                .buckets
                .iter()
                .max_by_key(|(_, lines)| *lines)
                .and_then(|(cid, _)| history.cohorts.get(*cid))
                .map(|label| self.hasher.colour_hash(label));

            let (dir_id, was_hidden) = {
                let file = &mut self.files[fid];
                file.lines = live.lines;
                file.byte_size = live.byte_size;
                file.touch_count = live.touch_count;
                file.created_timestamp = live.created_timestamp;
                file.dominant_cohort_colour = dominant_cohort_colour;

                // Touch to make the file visible in the simulation
                let was_hidden = file.pawn.is_hidden();
                file.touch(live.last_timestamp, Vec3::from(colour));

                // If a file size metric is enabled, set target weight and snap initial size
                let weight: u64 = match settings.file_size_metric {
                    gource_settings::FileSizeMetric::None => 0,
                    gource_settings::FileSizeMetric::Size => live.byte_size,
                    gource_settings::FileSizeMetric::Lines => live.lines as u64,
                    gource_settings::FileSizeMetric::Diff => live.lines.max(1) as u64,
                    gource_settings::FileSizeMetric::Churn => live.cohorts.total_churn_removed,
                };
                if weight > 0 {
                    file.weighted = true;
                    let ref_weight = match settings.file_size_metric {
                        gource_settings::FileSizeMetric::Size => 1024,
                        gource_settings::FileSizeMetric::Lines => 100,
                        gource_settings::FileSizeMetric::Diff => 50,
                        gource_settings::FileSizeMetric::Churn => 100,
                        _ => 100,
                    };
                    file.set_weight_target(weight, ref_weight, self.tuning.file_diameter);
                    file.sim.size = file.sim.target_size;
                    file.sim.radius = file.sim.target_size / 2;
                    file.pawn.size = file.target_size();
                    file.pawn.dims = Vec2::splat(file.pawn.size);
                }

                (file.dir, was_hidden)
            };

            if let Some(did) = dir_id {
                if was_hidden {
                    self.dirs[did].add_visible();
                }
                self.dirs[did].since_last_file_change = 0.0;
                self.on_node_updated(did, true);
            }

            if let Some(user_name) = history.users.get(live.last_user) {
                user_last_file_path.insert(user_name.to_string(), fullpath);
            }
        }

        // Apply weighted layout if a file size metric is active
        if settings.file_size_metric != gource_settings::FileSizeMetric::None {
            self.update_weighted_layout();
        }

        // Materialize active users and position them near their touched files
        for (username, path) in user_last_file_path {
            let uid = self.add_user(&username, settings);
            if let Some(&fid) = self.files_by_path.get(&path)
                && let Some(file_pos) = self.file_sim_pos(fid)
            {
                self.place_user_near(uid, file_pos);
            }
        }

        // Settle the layout
        self.settle(settle_steps);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::LogicProfile;
    use crate::step::{TICK_DT, TickDelta};

    /// A world with a few hundred files over a nested tree.
    fn nested_world(seed: u64) -> World {
        let mut world = World::new(seed, 31);
        let settings = GourceSettings::default();
        world.add_user("alice", &settings);
        world.add_user("bob", &settings);
        let mut cfs = Vec::new();
        for a in 0..6 {
            for b in 0..5 {
                for f in 0..4 {
                    cfs.push(CommitFile {
                        filename: format!("/a{a}/b{b}/c{}/f{f}.rs", (a + b) % 3),
                        action: FileAction::Add,
                        colour: gource_vcs::commit::WHITE,
                        ..Default::default()
                    });
                }
            }
        }
        for (i, cf) in cfs.iter().enumerate() {
            let commit = Commit {
                timestamp: 1000,
                username: if i % 2 == 0 { "alice" } else { "bob" }.to_string(),
                files: vec![cf.clone()],
                ..Default::default()
            };
            let fid = world.add_file(cf, &settings).expect("file added");
            world.add_file_action(&commit, cf, fid, 0.0, &settings);
        }
        world
    }

    fn tick(world: &mut World, n: u64, settings: &GourceSettings) -> TickDelta {
        let mut profile = LogicProfile::default();
        world.begin_tick();
        world.update_sim_bounds();
        world.interact_users();
        world.update_users(n as f32 * TICK_DT, TICK_DT, settings);
        world.update_dirs(TICK_DT, 0.0, &mut profile);
        world.end_tick()
    }

    #[test]
    fn simulation_is_deterministic_and_spreads() {
        let settings = GourceSettings::default();
        let mut a = nested_world(7);
        let mut b = nested_world(7);
        for n in 0..600 {
            tick(&mut a, n, &settings);
            tick(&mut b, n, &settings);
        }
        assert_eq!(a.state_hash(), b.state_hash());
        assert_eq!(a.tick, 600);
        // A different seed diverges only through coincident tie-breaks, so
        // the hashes may match; the layout must have spread either way.
        a.sync_view(1.0);
        assert!(a.dir_bounds.area() > 10000.0, "{:?}", a.dir_bounds);
        assert!(a.dirs.len() > 30);
        // Every file is visible and sits near its ring slot.
        for f in a.files.values() {
            assert!(!f.pawn.is_hidden());
            let target = f.sim.dest.unit_times(f.sim.distance as i64);
            assert!(
                (f.sim.pos - target).length() < ONE,
                "{:?} vs {target:?}",
                f.sim.pos
            );
        }
        assert!(a.user_tree.is_some() && a.dir_tree.is_some());
    }

    #[test]
    fn undo_restores_positions_exactly() {
        let settings = GourceSettings::default();
        let mut world = nested_world(3);
        for n in 0..120 {
            tick(&mut world, n, &settings);
        }
        let before: Vec<_> = world
            .dirs
            .values()
            .map(|d| (d.sim.pos, d.sim.spos))
            .collect();
        let files_before: Vec<_> = world.files.values().map(|f| f.sim.pos).collect();
        let users_before: Vec<_> = world.users.values().map(|u| u.sim.pos).collect();
        let mut deltas = Vec::new();
        for n in 120..180 {
            deltas.push(tick(&mut world, n, &settings));
        }
        assert!(deltas.iter().any(|d| !d.is_empty()));
        for d in deltas.iter().rev() {
            world.undo_tick(d);
        }
        assert_eq!(world.tick, 120);
        let after: Vec<_> = world
            .dirs
            .values()
            .map(|d| (d.sim.pos, d.sim.spos))
            .collect();
        assert_eq!(before, after);
        assert_eq!(
            files_before,
            world.files.values().map(|f| f.sim.pos).collect::<Vec<_>>()
        );
        assert_eq!(
            users_before,
            world.users.values().map(|u| u.sim.pos).collect::<Vec<_>>()
        );
    }

    #[test]
    fn view_interpolates_and_rotates() {
        let settings = GourceSettings::default();
        let mut world = nested_world(5);
        for n in 0..30 {
            tick(&mut world, n, &settings);
        }
        let d = *world.dir_map.values().find(|&&d| d != world.root).unwrap();
        let (p0, p1) = (world.dirs[d].sim.prev_pos, world.dirs[d].sim.pos);
        world.sync_view(0.5);
        let mid = (crate::view::from_ivec(p0) + crate::view::from_ivec(p1)) * 0.5;
        assert!((world.dirs[d].pos - mid).length() < 1e-3);
        world.rotate(1.0, 0.0, None);
        world.sync_view(1.0);
        let p = crate::view::from_ivec(p1);
        assert!((world.dirs[d].pos - Vec2::new(-p.y, p.x)).length() < 1e-3);
        // Rotation never touches the simulation.
        assert_eq!(world.dirs[d].sim.pos, p1);
    }

    #[test]
    fn weighted_mode_runs_and_packs() {
        let settings = GourceSettings {
            file_size_metric: gource_settings::FileSizeMetric::Lines,
            ..Default::default()
        };
        let mut world = World::new(9, 31);
        world.weighted_mode = true;
        for i in 0..20 {
            let cf = CommitFile {
                filename: format!("/w/f{i}.rs"),
                action: FileAction::Add,
                colour: gource_vcs::commit::WHITE,
                lines_added: Some(10 * (i + 1)),
                ..Default::default()
            };
            let commit = Commit {
                timestamp: 10,
                username: "u".to_string(),
                files: vec![cf.clone()],
                ..Default::default()
            };
            let fid = world.add_file(&cf, &settings).unwrap();
            world.add_file_action(&commit, &cf, fid, 0.0, &settings);
        }
        for n in 0..400 {
            tick(&mut world, n, &settings);
        }
        world.update_weighted_layout();
        world.sync_view(1.0);
        let files: Vec<_> = world
            .files
            .values()
            .filter(|f| !f.pawn.is_hidden())
            .collect();
        assert!(files.len() > 10);
        assert!(files.iter().all(|f| f.sim.size > ONE));
    }

    #[test]
    fn world_file_and_user_lifecycle() {
        let mut world = World::new(42, 31);
        let settings = GourceSettings::default();

        let uid = world.add_user("alice", &settings);
        assert_eq!(world.users[uid].name(), "alice");

        let cf = CommitFile {
            filename: "/src/main.rs".to_string(),
            action: FileAction::Add,
            colour: gource_vcs::commit::WHITE,
            ..Default::default()
        };

        let fid = world.add_file(&cf, &settings).expect("file added");
        assert_eq!(world.files[fid].fullpath, "/src/main.rs");

        let commit = Commit {
            timestamp: 1000,
            username: "alice".to_string(),
            files: vec![cf.clone()],
            ..Default::default()
        };

        world.add_file_action(&commit, &cf, fid, 1.0, &settings);
        assert_eq!(world.users[uid].action_count(), 1);

        // Run update_users with dt >= 0.2 to execute action and touch file (making it visible)
        let inactives = world.update_users(1.0, 0.25, &settings);
        assert!(inactives.is_empty());

        world.update_sim_bounds();
        assert!(world.sim_dir_bounds.is_some());
        world.interact_users();
        world.update_dirs(0.1, 0.0, &mut LogicProfile::default());
        world.sync_view(1.0);
        assert!(!world.dir_bounds.is_empty());
        assert!(world.user_tree.is_some());
        assert!(world.dir_tree.is_some());

        let inactives = world.update_users(1.1, 0.1, &settings);
        assert!(inactives.is_empty());

        // Deleting file
        let del_info = world.delete_file(fid).expect("deleted file");
        assert_eq!(del_info.fullpath, "/src/main.rs");
        // The action was active: C++ keeps counting it (see
        // `User::removed_active_count`), though the user is now idle.
        assert!(world.users[uid].is_idle());
        assert_eq!(world.users[uid].removed_active_count, 1);
        assert_eq!(world.users[uid].action_count(), 1);

        // Deleting user
        let udel_info = world.delete_user(uid).expect("deleted user");
        assert_eq!(udel_info.name, "alice");
    }

    #[test]
    fn world_rerooting_and_tree_reparenting() {
        let mut world = World::new(100, 31);
        let settings = GourceSettings::default();

        let cf1 = CommitFile {
            filename: "/repo1/a.rs".to_string(),
            action: FileAction::Add,
            colour: gource_vcs::commit::WHITE,
            ..Default::default()
        };
        let _ = world.add_file(&cf1, &settings);

        let cf2 = CommitFile {
            filename: "/repo2/b.rs".to_string(),
            action: FileAction::Add,
            colour: gource_vcs::commit::WHITE,
            ..Default::default()
        };
        let _ = world.add_file(&cf2, &settings);

        // Common parent should be "/"
        assert_eq!(world.dirs[world.root].path(), "/");
        assert!(world.dirs[world.root].children.len() >= 2);
    }
}
