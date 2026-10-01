//! Drawing extension trait for World.

use glam::{Vec2, Vec3, Vec4};
use gource_core::Bounds2D;
use gource_draw::font::{FontId, TextStyle};
use gource_draw::list::{DrawList, Material, TextureId, Vertex};
use gource_draw::{Gfx, Projection};
use gource_scene::file::{DirId, FileId};
use gource_scene::user::UserId;
use gource_scene::world::World;
use gource_settings::GourceSettings;

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

pub trait WorldDraw {
    fn prepare_frame(&mut self, proj: &Projection, settings: &GourceSettings);
    fn draw_scene(
        &self,
        list: &mut DrawList,
        proj: &Projection,
        settings: &GourceSettings,
        textures: &SceneTextures,
    );
    fn draw_names(
        &self,
        list: &mut DrawList,
        gfx: &mut Gfx,
        settings: &GourceSettings,
        fonts: &SceneFonts,
        selected_user: Option<UserId>,
        selected_file: Option<FileId>,
    );
}

trait WorldDrawInternal {
    fn prepare_dirs_recursive(
        &mut self,
        dir_id: DirId,
        proj: &Projection,
        v_bounds: &Bounds2D,
        settings: &GourceSettings,
    );
    fn draw_edges_shadow_recursive(
        &self,
        dir_id: DirId,
        list: &mut DrawList,
        beam_tex: TextureId,
        shadow_strength: f32,
        settings: &GourceSettings,
    );
    fn draw_edges_recursive(
        &self,
        dir_id: DirId,
        list: &mut DrawList,
        beam_tex: TextureId,
        settings: &GourceSettings,
    );
    fn draw_files_shadow_recursive(
        &self,
        dir_id: DirId,
        list: &mut DrawList,
        proj: &Projection,
        file_tex: TextureId,
        offset_world: Vec2,
        shadow_strength: f32,
    );
    fn draw_user_actions(&self, list: &mut DrawList, proj: &Projection, beam_tex: TextureId);
    fn draw_files_recursive(
        &self,
        dir_id: DirId,
        list: &mut DrawList,
        proj: &Projection,
        file_tex: TextureId,
        settings: &GourceSettings,
    );
    fn draw_bloom_recursive(
        &self,
        dir_id: DirId,
        list: &mut DrawList,
        proj: &Projection,
        settings: &GourceSettings,
    );
    fn draw_dir_names_recursive(
        &self,
        dir_id: DirId,
        list: &mut DrawList,
        gfx: &mut Gfx,
        settings: &GourceSettings,
        dir_font: FontId,
    );
    fn draw_file_names_recursive(
        &self,
        dir_id: DirId,
        list: &mut DrawList,
        gfx: &mut Gfx,
        settings: &GourceSettings,
        file_font: FontId,
        selected_only: bool,
    );
}

impl WorldDraw for World {
    fn prepare_frame(&mut self, proj: &Projection, settings: &GourceSettings) {
        let v_bounds = proj.visible_bounds();
        self.prepare_dirs_recursive(self.root, proj, &v_bounds, settings);

        for (_, user) in &mut self.users {
            let top = Vec2::new(user.pawn.pos.x, user.pawn.pos.y - user.pawn.dims.y * 0.5);
            let screen = proj.to_screen(top);
            user.pawn.screenpos = Vec3::new(screen.x, screen.y, 0.0);
        }
    }

    fn draw_scene(
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
                let tex = user
                    .graphic
                    .map(|g| TextureId(g.0))
                    .unwrap_or(textures.default_user);
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
                let tex = user
                    .graphic
                    .map(|g| TextureId(g.0))
                    .unwrap_or(textures.default_user);
                list.rect(tex, screen_pos - dims * 0.5, dims, colour);
            }
        }

        // 7. Bloom (glow around directory nodes)
        if !settings.hide_bloom {
            self.draw_bloom_recursive(self.root, list, proj, settings);
        }
    }

    fn draw_names(
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
}

impl WorldDrawInternal for World {
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
                    ((5.0f32 - dir.since_last_node_change) / 5.0f32).max(0.0f32)
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
}
