//! Scene projection display model (`SceneFrame`).
//!
//! Holds the projected float positions, colours, alphas, and radii of directories,
//! files, users, spline edges, and action beams produced by projecting the integer
//! [`gource_scene`] world through the camera.

use gource_core::{Vec2, Vec3, Vec4};

/// Projected directory node ready for bloom and label rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct DirDrawItem {
    pub id: u64,
    pub pos: Vec2,
    pub radius: f32,
    pub bloom_colour: Vec4,
    pub label: Option<String>,
    pub label_alpha: f32,
}

/// Projected file node ready for icon and label rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct FileDrawItem {
    pub id: u64,
    pub pos: Vec2,
    pub size: f32,
    pub colour: Vec3,
    pub alpha: f32,
    pub touch_alpha: f32,
    pub label: Option<String>,
    pub label_alpha: f32,
}

/// Projected user node ready for avatar/icon and name rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct UserDrawItem {
    pub id: u64,
    pub pos: Vec2,
    pub size: f32,
    pub colour: Vec3,
    pub alpha: f32,
    pub name: String,
    pub label_alpha: f32,
}

/// Projected tree spline edge between two directory nodes.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeDrawItem {
    pub pos1: Vec2,
    pub col1: Vec4,
    pub pos2: Vec2,
    pub col2: Vec4,
    pub spos: Vec2,
}

/// Projected action beam from a user to a modified file.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionBeamDrawItem {
    pub source_pos: Vec2,
    pub target_pos: Vec2,
    pub colour: Vec4,
    pub progress: f32,
}

/// Complete projected scene view for one frame.
pub type SceneVM = SceneFrame;

/// Complete projected scene view for one frame.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SceneFrame {
    pub dirs: Vec<DirDrawItem>,
    pub files: Vec<FileDrawItem>,
    pub users: Vec<UserDrawItem>,
    pub edges: Vec<EdgeDrawItem>,
    pub actions: Vec<ActionBeamDrawItem>,
}
