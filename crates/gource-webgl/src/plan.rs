//! Pure rendering planning and GL mapping logic without GL context calls.
//!
//! This module handles:
//! 1. Diffing a `TextureStore` against the currently uploaded texture versions.
//! 2. Mapping `gource_draw::Material` to blend function parameters and shaders.
//! 3. Mapping texture options (Filter, Wrap, Mipmaps) to WebGL constant values.
//! 4. Packing vertices and indices into flat f32 and u32 buffers for GPU upload.

use gource_core::Vec2;
use gource_draw::{Filter, Material, TextureId, TextureStore, Vertex, Wrap};
use std::collections::{HashMap, HashSet};

// WebGL2 constant values defined as numeric constants for native portability.
pub const GL_COLOR_BUFFER_BIT: u32 = 0x00004000;
pub const GL_TRIANGLES: u32 = 0x0004;
pub const GL_FLOAT: u32 = 0x1406;
pub const GL_UNSIGNED_INT: u32 = 0x1405;
pub const GL_SRC_ALPHA: u32 = 0x0302;
pub const GL_ONE_MINUS_SRC_ALPHA: u32 = 0x0303;
pub const GL_ONE: u32 = 1;
pub const GL_ZERO: u32 = 0;
pub const GL_ARRAY_BUFFER: u32 = 0x8892;
pub const GL_ELEMENT_ARRAY_BUFFER: u32 = 0x8893;
pub const GL_STREAM_DRAW: u32 = 0x88E0;
pub const GL_STATIC_DRAW: u32 = 0x88E4;
pub const GL_DYNAMIC_DRAW: u32 = 0x88E8;
pub const GL_TEXTURE_2D: u32 = 0x0DE1;
pub const GL_TEXTURE_WRAP_S: u32 = 0x2802;
pub const GL_TEXTURE_WRAP_T: u32 = 0x2803;
pub const GL_TEXTURE_MAG_FILTER: u32 = 0x2800;
pub const GL_TEXTURE_MIN_FILTER: u32 = 0x2801;
pub const GL_NEAREST: u32 = 0x2600;
pub const GL_LINEAR: u32 = 0x2601;
pub const GL_NEAREST_MIPMAP_NEAREST: u32 = 0x2700;
pub const GL_LINEAR_MIPMAP_NEAREST: u32 = 0x2701;
pub const GL_NEAREST_MIPMAP_LINEAR: u32 = 0x2702;
pub const GL_LINEAR_MIPMAP_LINEAR: u32 = 0x2703;
pub const GL_CLAMP_TO_EDGE: u32 = 0x812F;
pub const GL_REPEAT: u32 = 0x2901;
pub const GL_RGBA8: u32 = 0x8058;
pub const GL_RGBA: u32 = 0x1908;
pub const GL_UNSIGNED_BYTE: u32 = 0x1401;
pub const GL_UNPACK_ALIGNMENT: u32 = 0x0CF5;

/// Number of f32 floats per interleaved vertex: pos (2) + uv (2) + colour (4) = 8.
pub const FLOATS_PER_VERTEX: usize = 8;
pub const VERTEX_STRIDE_BYTES: i32 = (FLOATS_PER_VERTEX * std::mem::size_of::<f32>()) as i32;

/// Blend parameters for glBlendFuncSeparate or glBlendFunc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlendMode {
    pub sfactor: u32,
    pub dfactor: u32,
}

/// Description of an action needed to synchronize GPU textures with a CPU `TextureStore`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextureAction {
    /// Upload a texture (either brand new or updated version).
    Upload { id: TextureId, version: u64 },
    /// Delete a GPU texture because its id is no longer present in `TextureStore`.
    Delete { id: TextureId },
}

/// Planned parameters for a single batch draw call.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchDrawPlan {
    pub material: Material,
    pub texture: TextureId,
    /// Offset in elements (vertices) within the frame's shared vertex buffer, or 0 if per-batch buffer.
    pub vertex_offset: usize,
    pub vertex_count: usize,
    /// Offset in indices within the index buffer.
    pub index_offset: usize,
    pub index_count: usize,
    pub blend: BlendMode,
}

/// Pure tracker for texture synchronisation and diffing.
#[derive(Debug, Default, Clone)]
pub struct TextureTracker {
    known_versions: HashMap<TextureId, u64>,
}

impl TextureTracker {
    pub fn new() -> Self {
        Self {
            known_versions: HashMap::new(),
        }
    }

    /// Return the currently tracked texture IDs and versions.
    pub fn versions(&self) -> &HashMap<TextureId, u64> {
        &self.known_versions
    }

    /// Compare the provided store with tracked versions and return required actions.
    pub fn plan_sync(&self, store: &TextureStore) -> Vec<TextureAction> {
        let mut actions = Vec::new();
        let mut live_ids = HashSet::new();

        for (id, texture) in store.iter() {
            live_ids.insert(id);
            match self.known_versions.get(&id) {
                Some(&ver) if ver == texture.version => {
                    // Up-to-date, do nothing
                }
                _ => {
                    actions.push(TextureAction::Upload {
                        id,
                        version: texture.version,
                    });
                }
            }
        }

        // Check for deleted textures
        for &id in self.known_versions.keys() {
            if !live_ids.contains(&id) {
                actions.push(TextureAction::Delete { id });
            }
        }

        actions
    }

    /// Record that an upload action has succeeded.
    pub fn record_upload(&mut self, id: TextureId, version: u64) {
        self.known_versions.insert(id, version);
    }

    /// Record that a delete action has succeeded.
    pub fn record_delete(&mut self, id: TextureId) {
        self.known_versions.remove(&id);
    }
}

/// Map a `Material` to its WebGL blend factors.
pub fn blend_mode_for_material(material: Material) -> BlendMode {
    match material {
        Material::Alpha => BlendMode {
            sfactor: GL_SRC_ALPHA,
            dfactor: GL_ONE_MINUS_SRC_ALPHA,
        },
        Material::Bloom => BlendMode {
            sfactor: GL_ONE,
            dfactor: GL_ONE,
        },
    }
}

/// Map a `Wrap` mode to its GL constant.
pub fn gl_wrap(wrap: Wrap) -> u32 {
    match wrap {
        Wrap::Clamp => GL_CLAMP_TO_EDGE,
        Wrap::Repeat => GL_REPEAT,
    }
}

/// Map a `Filter` mode and mipmap setting to minification filter GL constant.
pub fn gl_min_filter(filter: Filter, mipmaps: bool) -> u32 {
    match (filter, mipmaps) {
        (Filter::Nearest, false) => GL_NEAREST,
        (Filter::Linear, false) => GL_LINEAR,
        // When mipmaps are enabled, trilinear filtering uses GL_LINEAR_MIPMAP_LINEAR,
        // or nearest mipmap for Nearest filter.
        (Filter::Nearest, true) => GL_NEAREST_MIPMAP_LINEAR,
        (Filter::Linear, true) => GL_LINEAR_MIPMAP_LINEAR,
    }
}

/// Map a `Filter` mode to magnification filter GL constant.
pub fn gl_mag_filter(filter: Filter) -> u32 {
    match filter {
        Filter::Nearest => GL_NEAREST,
        Filter::Linear => GL_LINEAR,
    }
}

/// Pack a slice of `Vertex` into an interleaved f32 buffer.
/// Layout per vertex: [pos.x, pos.y, uv.x, uv.y, col.r, col.g, col.b, col.a].
pub fn pack_vertices(vertices: &[Vertex], out: &mut Vec<f32>) {
    out.reserve(vertices.len() * FLOATS_PER_VERTEX);
    for v in vertices {
        out.push(v.pos.x);
        out.push(v.pos.y);
        out.push(v.uv.x);
        out.push(v.uv.y);
        out.push(v.colour.x);
        out.push(v.colour.y);
        out.push(v.colour.z);
        out.push(v.colour.w);
    }
}

/// Pack a slice of `Vertex` returning a newly allocated `Vec<f32>`.
pub fn pack_vertices_vec(vertices: &[Vertex]) -> Vec<f32> {
    let mut out = Vec::with_capacity(vertices.len() * FLOATS_PER_VERTEX);
    pack_vertices(vertices, &mut out);
    out
}

/// Convert pixel coordinate (screen space: origin top-left, +y down) to clip space (-1..1).
/// Matches vertex shader calculation:
/// clip_x = (x / viewport_w) * 2.0 - 1.0
/// clip_y = 1.0 - (y / viewport_h) * 2.0
pub fn pixel_to_clip(pos: Vec2, viewport: Vec2) -> Vec2 {
    Vec2::new(
        pos.x / viewport.x * 2.0 - 1.0,
        1.0 - pos.y / viewport.y * 2.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gource_core::Vec4;
    use gource_draw::TextureOptions;

    #[test]
    fn test_blend_modes() {
        assert_eq!(
            blend_mode_for_material(Material::Alpha),
            BlendMode {
                sfactor: GL_SRC_ALPHA,
                dfactor: GL_ONE_MINUS_SRC_ALPHA
            }
        );
        assert_eq!(
            blend_mode_for_material(Material::Bloom),
            BlendMode {
                sfactor: GL_ONE,
                dfactor: GL_ONE
            }
        );
    }

    #[test]
    fn test_wrap_and_filters() {
        assert_eq!(gl_wrap(Wrap::Clamp), GL_CLAMP_TO_EDGE);
        assert_eq!(gl_wrap(Wrap::Repeat), GL_REPEAT);

        assert_eq!(gl_mag_filter(Filter::Nearest), GL_NEAREST);
        assert_eq!(gl_mag_filter(Filter::Linear), GL_LINEAR);

        assert_eq!(gl_min_filter(Filter::Nearest, false), GL_NEAREST);
        assert_eq!(gl_min_filter(Filter::Linear, false), GL_LINEAR);
        assert_eq!(
            gl_min_filter(Filter::Nearest, true),
            GL_NEAREST_MIPMAP_LINEAR
        );
        assert_eq!(gl_min_filter(Filter::Linear, true), GL_LINEAR_MIPMAP_LINEAR);
    }

    #[test]
    fn test_texture_tracker_sync() {
        let mut store = TextureStore::new();
        let tracker = TextureTracker::new();

        // Initially store has white texture (id 0, version 1)
        let actions = tracker.plan_sync(&store);
        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0],
            TextureAction::Upload {
                id: TextureId::WHITE,
                version: 1
            }
        );

        let mut tracker = tracker;
        tracker.record_upload(TextureId::WHITE, 1);
        assert_eq!(tracker.plan_sync(&store).len(), 0);

        // Add a texture
        let t1 = store.create_rgba("custom", 2, 2, vec![255; 16], TextureOptions::plain());
        let actions = tracker.plan_sync(&store);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], TextureAction::Upload { id: t1, version: 1 });

        tracker.record_upload(t1, 1);
        assert_eq!(tracker.plan_sync(&store).len(), 0);

        // Update texture
        store.update_rgba(t1, 0, 0, 1, 1, &[0, 0, 0, 0]);
        let actions = tracker.plan_sync(&store);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], TextureAction::Upload { id: t1, version: 2 });
        tracker.record_upload(t1, 2);

        // Delete texture
        store.remove(t1);
        let actions = tracker.plan_sync(&store);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], TextureAction::Delete { id: t1 });

        tracker.record_delete(t1);
        assert_eq!(tracker.plan_sync(&store).len(), 0);
        assert_eq!(tracker.versions().len(), 1);
    }

    #[test]
    fn test_pack_vertices() {
        let v1 = Vertex::new(
            Vec2::new(10.0, 20.0),
            Vec2::new(0.0, 1.0),
            Vec4::new(0.2, 0.4, 0.6, 0.8),
        );
        let v2 = Vertex::new(
            Vec2::new(30.0, 40.0),
            Vec2::new(0.5, 0.5),
            Vec4::new(1.0, 1.0, 1.0, 1.0),
        );

        let packed = pack_vertices_vec(&[v1, v2]);
        assert_eq!(packed.len(), 16);
        assert_eq!(
            packed,
            vec![
                10.0, 20.0, 0.0, 1.0, 0.2, 0.4, 0.6, 0.8, 30.0, 40.0, 0.5, 0.5, 1.0, 1.0, 1.0, 1.0,
            ]
        );
    }

    #[test]
    fn test_pixel_to_clip() {
        let vp = Vec2::new(800.0, 600.0);
        // Top-left (0, 0) -> (-1, 1)
        let tl = pixel_to_clip(Vec2::ZERO, vp);
        assert!((tl.x - (-1.0)).abs() < 1e-6);
        assert!((tl.y - 1.0).abs() < 1e-6);

        // Bottom-right (800, 600) -> (1, -1)
        let br = pixel_to_clip(vp, vp);
        assert!((br.x - 1.0).abs() < 1e-6);
        assert!((br.y - (-1.0)).abs() < 1e-6);

        // Centre (400, 300) -> (0, 0)
        let centre = pixel_to_clip(Vec2::new(400.0, 300.0), vp);
        assert!(centre.length() < 1e-6);
    }
}
