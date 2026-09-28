//! Directory node layout, physics and drawing (port of dirnode.cpp).

use crate::file::{DirId, File, FileId};
use crate::spline::SplineEdge;
use glam::{Vec2, Vec3, Vec4};
use gource_core::Bounds2D;
use gource_core::math::rotate_vec2;
use slotmap::SlotMap;
use std::f32::consts::PI;

/// A directory node in the visualizer tree.
/// Port of `RDirNode` in `dirnode.h` / `dirnode.cpp`.
#[derive(Debug, Clone)]
pub struct DirNode {
    pub abspath: String,
    pub path_token: String,
    pub path_token_offset: usize,

    pub parent: Option<DirId>,
    pub children: Vec<DirId>,
    pub files: Vec<FileId>,

    pub spline: SplineEdge,
    pub col: Vec4,

    pub spos: Vec2,
    pub projected_pos: Vec2,
    pub projected_spos: Vec2,

    pub pos: Vec2,
    pub vel: Vec2,
    pub accel: Vec2,
    pub prev_accel: Vec2,

    pub dir_area: f32,
    pub visible: bool,
    pub in_frustum: bool,
    pub position_initialized: bool,

    pub since_node_visible: f32,
    pub since_last_file_change: f32,
    pub since_last_node_change: f32,

    pub file_area: f32,
    pub dir_radius: f32,
    pub parent_radius: f32,

    pub depth: i32,
    pub visible_count: usize,

    pub screenpos: Vec3,
    pub node_normal: Vec2,
}

impl DirNode {
    /// Port of `RDirNode::RDirNode(RDirNode* parent, const std::string & abspath)`.
    pub fn new(abspath: &str, file_diameter: f32, dir_padding: f32) -> Self {
        let mut fixed_path = abspath.to_string();
        if fixed_path.is_empty() || !fixed_path.ends_with('/') {
            fixed_path.push('/');
        }

        let padded_file_radius = file_diameter * 0.5;
        let file_area = padded_file_radius * padded_file_radius * PI;

        let mut node = Self {
            // C++: the constructor's setParent(parent) calls adjustPath(),
            // which sets path_token_offset = abspath.size() even for the
            // root. Children get their token from that offset, so the root
            // must not stay at 0 (top-level dirs would be labelled "/src").
            path_token_offset: fixed_path.len(),
            abspath: fixed_path,
            path_token: String::new(),
            parent: None,
            children: Vec::new(),
            files: Vec::new(),
            spline: SplineEdge::new(),
            col: Vec4::ONE,
            spos: Vec2::ZERO,
            projected_pos: Vec2::ZERO,
            projected_spos: Vec2::ZERO,
            pos: Vec2::ZERO,
            vel: Vec2::ZERO,
            accel: Vec2::ZERO,
            prev_accel: Vec2::ZERO,
            dir_area: 0.0,
            visible: false,
            in_frustum: false,
            position_initialized: false,
            since_node_visible: 0.0,
            since_last_file_change: 0.0,
            since_last_node_change: 0.0,
            file_area,
            dir_radius: 1.0,
            parent_radius: 1.0,
            depth: 1,
            visible_count: 0,
            screenpos: Vec3::ZERO,
            node_normal: Vec2::ZERO,
        };
        node.calc_radius(dir_padding, 0.0);
        node.calc_colour(&SlotMap::with_key());
        node
    }

    /// Change path and update `abspath`.
    /// Port of `RDirNode::changePath`.
    pub fn change_path(&mut self, abspath: &str) {
        self.abspath = abspath.to_string();
        if self.abspath.is_empty() || !self.abspath.ends_with('/') {
            self.abspath.push('/');
        }
    }

    /// Port of `RDirNode::getTokenOffset()`.
    pub fn token_offset(&self) -> usize {
        self.path_token_offset
    }

    /// Port of `RDirNode::getPath()`.
    pub fn path(&self) -> &str {
        &self.abspath
    }

    /// Port of `RDirNode::getPos()`.
    pub fn pos(&self) -> Vec2 {
        self.pos
    }

    /// Port of `RDirNode::setPos(const vec2 & pos)`.
    pub fn set_pos(&mut self, pos: Vec2) {
        self.pos = pos;
    }

    /// Port of `RDirNode::getDepth()`.
    pub fn depth(&self) -> i32 {
        self.depth
    }

    /// Port of `RDirNode::getRadius()`.
    pub fn radius(&self) -> f32 {
        self.dir_radius
    }

    /// Port of `RDirNode::getParentRadius()`.
    pub fn parent_radius(&self) -> f32 {
        self.parent_radius
    }

    /// Port of `RDirNode::getArea()`.
    pub fn area(&self) -> f32 {
        self.dir_area
    }

    /// Port of `RDirNode::getColour()`.
    pub fn colour(&self) -> Vec4 {
        self.col
    }

    /// Port of `RDirNode::getNodeNormal()`.
    pub fn node_normal(&self) -> Vec2 {
        self.node_normal
    }

    /// Port of `RDirNode::getSPos()`.
    pub fn spos(&self) -> Vec2 {
        self.spos
    }

    /// Port of `RDirNode::getProjectedPos()`.
    pub fn projected_pos(&self) -> Vec2 {
        self.projected_pos
    }

    /// Port of `RDirNode::prefixedBy(const std::string & path)`.
    pub fn prefixed_by(&self, path: &str) -> bool {
        if path.is_empty() {
            return false;
        }
        if !path.ends_with('/') {
            let mut p = path.to_string();
            p.push('/');
            self.abspath.starts_with(&p)
        } else {
            self.abspath.starts_with(path)
        }
    }

    /// Port of `RDirNode::commonPathPrefix(const std::string & str)`.
    pub fn common_path_prefix(&self, str: &str) -> String {
        let mut c = 0;
        let mut slash = -1;
        let ab_bytes = self.abspath.as_bytes();
        let str_bytes = str.as_bytes();

        while c < ab_bytes.len() && c < str_bytes.len() && ab_bytes[c] == str_bytes[c] {
            if ab_bytes[c] == b'/' {
                slash = c as i32;
            }
            c += 1;
        }

        if slash == -1 {
            String::new()
        } else {
            str[..=(slash as usize)].to_string()
        }
    }

    /// Port of `RDirNode::adjustPath`.
    pub fn adjust_path(&mut self, parent_token_offset: usize) {
        if self.parent.is_some() {
            if self.abspath.len() > parent_token_offset {
                self.path_token =
                    self.abspath[parent_token_offset..self.abspath.len() - 1].to_string();
            } else {
                self.path_token = String::new();
            }
        } else {
            self.path_token = String::new();
        }
        self.path_token_offset = self.abspath.len();
    }

    /// Port of `RDirNode::addVisible()`.
    pub fn add_visible(&mut self) {
        self.visible_count += 1;
        self.visible = true;
    }

    /// Port of `RDirNode::isVisible()`.
    pub fn is_visible(&self, dirs: &SlotMap<DirId, DirNode>) -> bool {
        if self.visible {
            return true;
        }
        for &child_id in &self.children {
            if let Some(child) = dirs.get(child_id)
                && child.is_visible(dirs)
            {
                return true;
            }
        }
        false
    }

    /// Port of `RDirNode::empty()`.
    pub fn is_empty(&self) -> bool {
        self.visible_count == 0 && self.children.is_empty()
    }

    /// Port of `RDirNode::calcRadius()`.
    pub fn calc_radius(&mut self, dir_padding: f32, children_area_sum: f32) {
        let total_file_area = self.file_area * (self.visible_count as f32);
        self.dir_area = total_file_area + children_area_sum;
        self.dir_radius = 1.0f32.max(self.dir_area.sqrt()) * dir_padding;
        self.parent_radius = 1.0f32.max(total_file_area.sqrt() * dir_padding);
    }

    /// Port of `RDirNode::calcColour()`.
    pub fn calc_colour(&mut self, files: &SlotMap<FileId, File>) {
        let brightness = 0.6f32.max(1.0 - (self.since_last_node_change / 3.0).min(1.0));
        self.col = Vec4::new(brightness, brightness, brightness, 1.0);

        let mut fcount = 0;
        for &file_id in &self.files {
            if let Some(file) = files.get(file_id) {
                if file.pawn.is_hidden() {
                    continue;
                }
                let filecol = file.colour() * brightness;
                let a = file.alpha();
                self.col += Vec4::new(filecol.x, filecol.y, filecol.z, a);
                fcount += 1;
            }
        }

        self.col /= (fcount as f32) + 1.0;
    }

    /// Port of `RDirNode::calcFileDest(int max_files, int file_no)`.
    pub fn calc_file_dest(max_files: usize, file_no: usize) -> Vec2 {
        let arc = 1.0 / (max_files as f32);
        let frac = arc * 0.5 + arc * (file_no as f32);
        let angle = frac * PI * 2.0;
        Vec2::new(angle.sin(), angle.cos())
    }

    /// Port of `RDirNode::updateFilePositions()`.
    pub fn update_file_positions(&mut self, file_diameter: f32, files: &mut SlotMap<FileId, File>) {
        let mut max_files = 1;
        let mut diameter = 1;
        let mut file_no = 0;
        let mut d = 0.0f32;

        let mut files_left = self.visible_count;

        for &fid in &self.files {
            if let Some(file) = files.get_mut(fid) {
                if file.pawn.is_hidden() {
                    file.dest = Vec2::ZERO;
                    file.distance = 0.0;
                    continue;
                }

                let dest = Self::calc_file_dest(max_files, file_no);
                file.dest = dest;
                file.distance = d;

                files_left = files_left.saturating_sub(1);
                file_no += 1;

                if file_no >= max_files {
                    diameter += 1;
                    d += file_diameter;
                    max_files = (1.0f32.max((diameter as f32) * PI)) as usize;

                    if files_left < max_files {
                        max_files = files_left;
                    }

                    file_no = 0;
                }
            }
        }
    }

    /// Port of `RDirNode::distanceToParent()`.
    pub fn distance_to_parent(&self, parent: &DirNode) -> f32 {
        let posd = (parent.pos - self.pos).length();
        posd - (self.dir_radius + parent.parent_radius)
    }

    /// Port of `RDirNode::applyForceDir(RDirNode* node)`.
    pub fn apply_force_dir(&mut self, other_pos: Vec2, other_radius: f32, rng: &mut fastrand::Rng) {
        let dir = other_pos - self.pos;
        let posd2 = dir.length_squared();
        let myradius = self.dir_radius;
        let your_radius = other_radius;
        let sumradius = myradius + your_radius;

        let distance2 = posd2 - sumradius * sumradius;
        if distance2 > 0.0 {
            return;
        }

        let posd = posd2.sqrt();
        let distance = posd - myradius - your_radius;

        if posd < 0.00001 {
            let rx = (rng.i32(0..100) - 50) as f32;
            let ry = (rng.i32(0..100) - 50) as f32;
            let v = Vec2::new(rx, ry);
            let len = v.length();
            let norm = if len > 0.0 { v / len } else { Vec2::X };
            self.accel += norm;
            return;
        }

        self.accel += distance * (dir / posd);
    }

    /// Port of `RDirNode::updateSplinePoint(float dt)`.
    pub fn update_spline_point(&mut self, dt: f32, parent_pos: Vec2) {
        let td = (parent_pos - self.pos) * 0.5;
        let mid = self.pos + td;
        let delta = mid - self.spos;

        if delta.length_squared() > td.length_squared() {
            let delta_len = delta.length();
            let td_len = td.length();
            if delta_len > 0.0 {
                self.spos += (delta / delta_len) * (delta_len - td_len);
            }
        }

        self.spos += delta * (dt * 2.0).min(1.0);
    }

    /// Port of `RDirNode::setInitialPosition()`.
    pub fn set_initial_position(
        &mut self,
        parent_pos: Vec2,
        parent_parent_pos: Option<Vec2>,
        hasher: &gource_core::StringHasher,
    ) {
        self.pos = parent_pos;
        let h = hasher.vec2_hash(&self.abspath);

        if let Some(pp_pos) = parent_parent_pos {
            let p_edge = parent_pos - pp_pos;
            let p_edge_len = p_edge.length();
            let p_edge_norm = if p_edge_len > 0.0 {
                p_edge / p_edge_len
            } else {
                Vec2::ZERO
            };
            let combo = p_edge_norm * 2.0 + h;
            let clen = combo.length();
            self.pos += if clen > 0.0 { combo / clen } else { combo };
        } else {
            self.pos += h;
        }

        self.spos = self.pos - (parent_pos - self.pos) * 0.5;
        self.position_initialized = true;
    }

    /// Port of `RDirNode::move(float dt)`.
    pub fn move_step(&mut self, dt: f32, elasticity: f32) {
        if self.parent.is_none() {
            return;
        }

        self.pos += self.accel * dt;

        if elasticity > 0.0 {
            let diff = self.accel - self.prev_accel;
            let m = dt * elasticity;
            let accel3 = self.prev_accel * (1.0 - m) + diff * m;
            self.pos += accel3;
            self.prev_accel = accel3;
        }

        self.accel = Vec2::ZERO;
    }

    /// Port of `RDirNode::rotate(float s, float c)`.
    pub fn rotate(&mut self, s: f32, c: f32) {
        self.pos = rotate_vec2(self.pos, s, c);
        self.spos = rotate_vec2(self.spos, s, c);
    }

    /// Port of `RDirNode::rotate(float s, float c, const vec2& centre)`.
    pub fn rotate_around(&mut self, s: f32, c: f32, centre: Vec2) {
        self.pos = rotate_vec2(self.pos - centre, s, c) + centre;
        self.spos = rotate_vec2(self.spos - centre, s, c) + centre;
    }

    /// Port of `RDirNode::updateQuadItemBounds()`.
    pub fn bounds(&self) -> Bounds2D {
        let radoffset = Vec2::splat(self.dir_radius);
        Bounds2D::from_points(self.pos - radoffset, self.pos + radoffset)
    }

    /// Port of `RDirNode::averageFileColour()`.
    pub fn average_file_colour(
        &self,
        files: &SlotMap<FileId, File>,
        dirs: &SlotMap<DirId, DirNode>,
    ) -> Vec3 {
        let mut av = Vec3::ZERO;
        let mut count = 0;

        for &fid in &self.files {
            if let Some(file) = files.get(fid) {
                if file.pawn.is_hidden() {
                    continue;
                }
                av += file.colour();
                count += 1;
            }
        }

        if count > 0 {
            av *= 1.0 / (count as f32);
        }

        let mut child_count = 0;
        let mut child_av = Vec3::ZERO;
        for &cid in &self.children {
            if let Some(child) = dirs.get(cid) {
                child_av += child.average_file_colour(files, dirs);
                child_count += 1;
            }
        }

        if child_count > 0 {
            child_av *= 1.0 / (child_count as f32);
            if count > 0 {
                av = (av + child_av) * 0.5;
            } else {
                av = child_av;
            }
        }

        av
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirnode_basics() {
        let mut node = DirNode::new("/a/b", 8.0, 1.5);
        assert_eq!(node.path(), "/a/b/");
        assert_eq!(node.depth(), 1);
        assert!(node.prefixed_by("/a"));
        assert!(node.prefixed_by("/a/"));
        assert!(node.prefixed_by("/a/b"));
        assert!(!node.prefixed_by("/a/c"));
        assert_eq!(node.common_path_prefix("/a/c/foo"), "/a/");
        assert_eq!(node.common_path_prefix("/x/y"), "/");
        assert_eq!(node.common_path_prefix("x/y"), "");

        node.change_path("/a/b/c");
        assert_eq!(node.path(), "/a/b/c/");

        let mut sm: SlotMap<DirId, ()> = SlotMap::with_key();
        node.parent = Some(sm.insert(()));
        node.adjust_path(3); // parent "/a/" offset = 3
        assert_eq!(node.path_token, "b/c");
        assert_eq!(node.token_offset(), 7);

        assert!(node.is_empty());
        node.add_visible();
        assert!(!node.is_empty());
        assert_eq!(node.visible_count, 1);

        let dest0 = DirNode::calc_file_dest(1, 0);
        assert_eq!(dest0, Vec2::new((PI).sin(), (PI).cos())); // frac = 0.5 -> angle = PI
    }

    #[test]
    fn dirnode_physics_and_forces() {
        let mut node = DirNode::new("/foo/", 8.0, 1.5);
        let mut rng = fastrand::Rng::with_seed(42);

        let mut sm: SlotMap<DirId, ()> = SlotMap::with_key();
        let dummy_parent = sm.insert(());
        node.parent = Some(dummy_parent);

        node.pos = Vec2::new(10.0, 0.0);
        node.dir_radius = 10.0;

        // Other node overlapping
        node.apply_force_dir(Vec2::new(15.0, 0.0), 10.0, &mut rng);
        assert!(node.accel.x != 0.0);

        // Move
        node.move_step(0.1, 0.5);
        assert_eq!(node.accel, Vec2::ZERO);

        // Rotations
        node.pos = Vec2::new(10.0, 0.0);
        node.rotate(1.0, 0.0); // 90 deg
        assert!((node.pos.y - 10.0).abs() < 1e-5);
    }
}
