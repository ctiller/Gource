//! Directory nodes (port of dirnode.cpp): tree bookkeeping, the integer
//! simulation state ([`DirSim`]) and the float view fields derived from it.

use crate::file::{DirId, File, FileId};
use crate::spline::SplineEdge;
use glam::{Vec2, Vec3, Vec4};
use gource_core::Bounds2D;
use gource_scene::dirs::Radii;
use gource_scene::{Fx, IVec2};
use slotmap::SlotMap;

/// A directory's simulation state (fixed point; see `gource-scene`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DirSim {
    pub pos: IVec2,
    /// Position at the start of the current tick (view interpolation).
    pub prev_pos: IVec2,
    /// Spline control point.
    pub spos: IVec2,
    pub prev_spos: IVec2,
    /// Extra acceleration for the next tick (weighted-mode beam pushes).
    pub ext_accel: IVec2,
    pub radii: Radii,
    /// C++ `position_initialized`: placed next to its parent.
    pub initialized: bool,
}

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

    /// Simulation state; everything below `sim` that is a position or a
    /// radius is a float view derived from it.
    pub sim: DirSim,

    /// View: interpolated, rotated spline point.
    pub spos: Vec2,
    pub projected_pos: Vec2,
    pub projected_spos: Vec2,

    /// View: interpolated, rotated position.
    pub pos: Vec2,

    /// View of [`Radii::area`].
    pub dir_area: f32,
    pub visible: bool,
    pub in_frustum: bool,

    pub since_node_visible: f32,
    pub since_last_file_change: f32,
    pub since_last_node_change: f32,

    /// View of [`Radii::radius`].
    pub dir_radius: f32,
    /// View of [`Radii::parent_radius`].
    pub parent_radius: f32,

    pub depth: i32,
    pub visible_count: usize,

    pub screenpos: Vec3,
    pub node_normal: Vec2,

    /// View: the box `pos ± radius`, refreshed by
    /// [`DirNode::update_quad_item_bounds`] while the dir is visible, so a
    /// hidden dir keeps its last bounds. Picking and the frustum test use it.
    pub quad_item_bounds: Bounds2D,
}

impl DirNode {
    /// Port of `RDirNode::RDirNode(RDirNode* parent, const std::string & abspath)`.
    /// `file_area` is one file's area (Q16), `padding` the directory padding
    /// (Q8).
    pub fn new(abspath: &str, file_area: i64, padding: Fx) -> Self {
        let mut fixed_path = abspath.to_string();
        if fixed_path.is_empty() || !fixed_path.ends_with('/') {
            fixed_path.push('/');
        }

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
            sim: DirSim::default(),
            spos: Vec2::ZERO,
            projected_pos: Vec2::ZERO,
            projected_spos: Vec2::ZERO,
            pos: Vec2::ZERO,
            dir_area: 0.0,
            visible: false,
            in_frustum: false,
            since_node_visible: 0.0,
            since_last_file_change: 0.0,
            since_last_node_change: 0.0,
            dir_radius: 1.0,
            parent_radius: 1.0,
            depth: 1,
            visible_count: 0,
            screenpos: Vec3::ZERO,
            node_normal: Vec2::ZERO,
            quad_item_bounds: Bounds2D::new(),
        };
        node.calc_radius(file_area, padding, 0);
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

    /// Port of `RDirNode::setPos(const vec2 & pos)`: teleports the
    /// simulation position (rounded to fixed point) and the view.
    pub fn set_pos(&mut self, pos: Vec2) {
        self.place(crate::view::to_ivec(pos));
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

    /// Set the radii (C++ `calcRadius`, or the weighted variant), keeping the
    /// float view fields in step.
    pub fn set_radii(&mut self, radii: Radii) {
        self.sim.radii = radii;
        self.dir_area = crate::view::from_area(radii.area);
        self.dir_radius = crate::view::from_fx(radii.radius);
        self.parent_radius = crate::view::from_fx(radii.parent_radius);
    }

    /// Port of `RDirNode::calcRadius()` in fixed point: `file_area` is one
    /// file's area (Q16), `children_area` the sum of the children's areas.
    pub fn calc_radius(&mut self, file_area: i64, padding: Fx, children_area: i64) {
        let r =
            gource_scene::dirs::radii(file_area, self.visible_count as u32, children_area, padding);
        self.set_radii(r);
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

    /// Teleport the directory (simulation and view) to `pos`.
    pub fn place(&mut self, pos: IVec2) {
        self.sim.pos = pos;
        self.sim.prev_pos = pos;
        self.pos = crate::view::from_ivec(pos);
    }

    /// The box `RDirNode::updateQuadItemBounds()` computes: `pos ± radius`.
    pub fn bounds(&self) -> Bounds2D {
        let radoffset = Vec2::splat(self.dir_radius);
        Bounds2D::from_points(self.pos - radoffset, self.pos + radoffset)
    }

    /// Port of `RDirNode::updateQuadItemBounds()`: refresh the cached
    /// [`DirNode::quad_item_bounds`].
    pub fn update_quad_item_bounds(&mut self) {
        self.quad_item_bounds = self.bounds();
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
    use gource_scene::ONE;

    fn area() -> i64 {
        gource_scene::dirs::file_area(8 * ONE)
    }

    #[test]
    fn dirnode_basics() {
        let mut node = DirNode::new("/a/b", area(), 384);
        assert_eq!(node.path(), "/a/b/");
        assert_eq!(node.depth(), 1);
        assert!(node.prefixed_by("/a"));
        assert!(node.prefixed_by("/a/"));
        assert!(node.prefixed_by("/a/b"));
        assert!(!node.prefixed_by("/a/c"));
        assert!(!node.prefixed_by(""));
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
    }

    #[test]
    fn radii_and_placement() {
        let mut node = DirNode::new("/foo/", area(), 384);
        // An empty dir has the minimum radius: 1 unit * padding.
        assert_eq!(node.radius(), 1.5);
        assert_eq!(node.parent_radius(), 1.0);
        node.add_visible();
        node.add_visible();
        node.calc_radius(area(), 384, 0);
        // Two 8-unit files: area 2 * 16 pi, radius sqrt(32 pi) * 1.5.
        let expect = (32.0 * std::f32::consts::PI).sqrt() * 1.5;
        assert!((node.radius() - expect).abs() < 0.05, "{}", node.radius());
        assert!(node.area() > 100.0);

        node.set_pos(Vec2::new(10.0, -2.5));
        assert_eq!(node.pos(), Vec2::new(10.0, -2.5));
        assert_eq!(node.sim.pos, IVec2::new(10 * ONE, -640));
        assert_eq!(node.sim.prev_pos, node.sim.pos);
        node.update_quad_item_bounds();
        assert_eq!(node.quad_item_bounds, node.bounds());
        assert_eq!(node.spos(), Vec2::ZERO);
        assert_eq!(node.node_normal(), Vec2::ZERO);
        assert_eq!(node.projected_pos(), Vec2::ZERO);
        assert_eq!(node.colour(), node.col);
    }
}
