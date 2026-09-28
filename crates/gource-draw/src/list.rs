//! Draw lists: ordered batches of textured, vertex-coloured triangles.

use glam::{UVec2, Vec2, Vec4};

/// Identifies a texture in a [`crate::TextureStore`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct TextureId(pub u32);

impl TextureId {
    /// A 1x1 opaque white texture, always present. Used for untextured drawing.
    pub const WHITE: TextureId = TextureId(0);
}

/// How a batch is shaded and blended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Material {
    /// `out = texture(uv) * colour`, blended with `SRC_ALPHA, ONE_MINUS_SRC_ALPHA`.
    Alpha,
    /// The Gource bloom glow (port of `data/shaders/bloom.frag`), blended
    /// additively (`ONE, ONE`). The texture is ignored. `uv` holds the vertex
    /// position relative to the glow centre divided by the glow radius, i.e.
    /// it spans `-1..1` across the quad:
    ///
    /// ```text
    /// r         = noise in [0,1)
    /// intensity = min(1, cos(2 * length(uv) + (0.5 - r) * 0.045))
    /// gradient  = intensity * smoothstep(0, 2, intensity)
    ///           * smoothstep(1, 0.67 + r * 0.33, 1 - intensity)
    /// out       = colour * gradient
    /// ```
    Bloom,
}

/// A vertex in screen pixel coordinates (origin top-left, +y down).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vertex {
    pub pos: Vec2,
    pub uv: Vec2,
    /// Straight (non-premultiplied) RGBA.
    pub colour: Vec4,
}

impl Vertex {
    pub const fn new(pos: Vec2, uv: Vec2, colour: Vec4) -> Self {
        Self { pos, uv, colour }
    }
}

/// A run of triangles sharing a material and texture.
#[derive(Debug, Clone, PartialEq)]
pub struct Batch {
    pub material: Material,
    pub texture: TextureId,
    pub vertices: Vec<Vertex>,
    /// Triangle list indices into `vertices`.
    pub indices: Vec<u32>,
}

impl Batch {
    pub fn new(material: Material, texture: TextureId) -> Self {
        Self {
            material,
            texture,
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }
}

/// One frame worth of drawing, in painter's order.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawList {
    /// Size of the target in pixels.
    pub viewport: UVec2,
    /// Colour the target is cleared to before drawing (alpha 0 for a
    /// transparent window).
    pub clear_colour: Vec4,
    pub batches: Vec<Batch>,
}

impl Default for DrawList {
    fn default() -> Self {
        Self::new(UVec2::new(1, 1))
    }
}

/// Standard texture coordinates for a quad given as
/// `[min, (max.x, min.y), max, (min.x, max.y)]`.
pub const QUAD_UVS: [Vec2; 4] = [
    Vec2::new(0.0, 0.0),
    Vec2::new(1.0, 0.0),
    Vec2::new(1.0, 1.0),
    Vec2::new(0.0, 1.0),
];

impl DrawList {
    pub fn new(viewport: UVec2) -> Self {
        Self {
            viewport,
            clear_colour: Vec4::new(0.0, 0.0, 0.0, 1.0),
            batches: Vec::new(),
        }
    }

    /// Start a new frame.
    pub fn reset(&mut self, viewport: UVec2, clear_colour: Vec4) {
        self.viewport = viewport;
        self.clear_colour = clear_colour;
        self.batches.clear();
    }

    pub fn width(&self) -> f32 {
        self.viewport.x as f32
    }

    pub fn height(&self) -> f32 {
        self.viewport.y as f32
    }

    pub fn is_empty(&self) -> bool {
        self.batches.iter().all(|b| b.indices.is_empty())
    }

    pub fn vertex_count(&self) -> usize {
        self.batches.iter().map(|b| b.vertices.len()).sum()
    }

    /// The batch to append to: the last one if it has the same material and
    /// texture, otherwise a new one.
    pub fn batch_mut(&mut self, material: Material, texture: TextureId) -> &mut Batch {
        let reuse = matches!(self.batches.last(), Some(b) if b.material == material && b.texture == texture);
        if !reuse {
            self.batches.push(Batch::new(material, texture));
        }
        self.batches.last_mut().expect("batch just ensured")
    }

    /// Append raw triangles. `indices` are relative to `vertices`.
    pub fn triangles(
        &mut self,
        material: Material,
        texture: TextureId,
        vertices: &[Vertex],
        indices: &[u32],
    ) {
        if indices.is_empty() {
            return;
        }
        let batch = self.batch_mut(material, texture);
        let base = batch.vertices.len() as u32;
        batch.vertices.extend_from_slice(vertices);
        batch.indices.extend(indices.iter().map(|i| base + i));
    }

    /// Append a quad given its four corners in order (either winding).
    pub fn push_quad(&mut self, material: Material, texture: TextureId, v: [Vertex; 4]) {
        let batch = self.batch_mut(material, texture);
        let base = batch.vertices.len() as u32;
        batch.vertices.extend_from_slice(&v);
        batch
            .indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Alpha-blended quad with a single colour.
    pub fn quad(&mut self, texture: TextureId, corners: [Vec2; 4], uvs: [Vec2; 4], colour: Vec4) {
        self.quad_colours(texture, corners, uvs, [colour; 4]);
    }

    /// Alpha-blended quad with per-vertex colours.
    pub fn quad_colours(
        &mut self,
        texture: TextureId,
        corners: [Vec2; 4],
        uvs: [Vec2; 4],
        colours: [Vec4; 4],
    ) {
        let v = std::array::from_fn(|i| Vertex::new(corners[i], uvs[i], colours[i]));
        self.push_quad(Material::Alpha, texture, v);
    }

    /// Axis aligned textured rectangle with texture coordinates `(0,0)..(1,1)`.
    pub fn rect(&mut self, texture: TextureId, pos: Vec2, size: Vec2, colour: Vec4) {
        self.rect_uv(texture, pos, size, Vec2::ZERO, Vec2::ONE, colour);
    }

    /// Axis aligned textured rectangle with explicit texture coordinates.
    pub fn rect_uv(
        &mut self,
        texture: TextureId,
        pos: Vec2,
        size: Vec2,
        uv_min: Vec2,
        uv_max: Vec2,
        colour: Vec4,
    ) {
        let corners = [
            pos,
            Vec2::new(pos.x + size.x, pos.y),
            pos + size,
            Vec2::new(pos.x, pos.y + size.y),
        ];
        let uvs = [
            uv_min,
            Vec2::new(uv_max.x, uv_min.y),
            uv_max,
            Vec2::new(uv_min.x, uv_max.y),
        ];
        self.quad(texture, corners, uvs, colour);
    }

    /// Untextured rectangle.
    pub fn solid_rect(&mut self, pos: Vec2, size: Vec2, colour: Vec4) {
        self.rect(TextureId::WHITE, pos, size, colour);
    }

    /// A bloom glow centred at `centre` covering `centre ± radius`.
    pub fn bloom(&mut self, centre: Vec2, radius: f32, colour: Vec4) {
        let r = Vec2::splat(radius);
        let min = centre - r;
        let max = centre + r;
        let corners = [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)];
        let uvs = [
            Vec2::new(-1.0, -1.0),
            Vec2::new(1.0, -1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(-1.0, 1.0),
        ];
        let v = std::array::from_fn(|i| Vertex::new(corners[i], uvs[i], colour));
        self.push_quad(Material::Bloom, TextureId::WHITE, v);
    }

    /// A line segment drawn as an untextured quad of the given width.
    pub fn line(&mut self, a: Vec2, b: Vec2, width: f32, colour: Vec4) {
        let dir = b - a;
        let len = dir.length();
        if len <= 0.0 {
            return;
        }
        let n = Vec2::new(-dir.y, dir.x) / len * (width * 0.5);
        self.quad(
            TextureId::WHITE,
            [a - n, b - n, b + n, a + n],
            QUAD_UVS,
            colour,
        );
    }

    /// Outline of an axis aligned rectangle.
    pub fn rect_outline(&mut self, min: Vec2, max: Vec2, width: f32, colour: Vec4) {
        let c = [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)];
        for i in 0..4 {
            self.line(c[i], c[(i + 1) % 4], width, colour);
        }
    }

    /// Append another list's batches with every vertex alpha multiplied by
    /// `alpha` (used to fade out the previous scene during transitions).
    pub fn append_faded(&mut self, other: &DrawList, alpha: f32) {
        for batch in &other.batches {
            let vertices: Vec<Vertex> = batch
                .vertices
                .iter()
                .map(|v| {
                    let mut v = *v;
                    match batch.material {
                        Material::Alpha => v.colour.w *= alpha,
                        // Additive: scale the contribution.
                        Material::Bloom => v.colour *= alpha,
                    }
                    v
                })
                .collect();
            self.triangles(batch.material, batch.texture, &vertices, &batch.indices);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_compatible_batches() {
        let mut list = DrawList::new(UVec2::new(100, 100));
        list.solid_rect(Vec2::ZERO, Vec2::ONE, Vec4::ONE);
        list.solid_rect(Vec2::ONE, Vec2::ONE, Vec4::ONE);
        assert_eq!(list.batches.len(), 1);
        assert_eq!(list.batches[0].vertices.len(), 8);
        assert_eq!(
            list.batches[0].indices,
            vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]
        );

        list.rect(TextureId(3), Vec2::ZERO, Vec2::ONE, Vec4::ONE);
        list.bloom(Vec2::ZERO, 1.0, Vec4::ONE);
        list.solid_rect(Vec2::ZERO, Vec2::ONE, Vec4::ONE);
        assert_eq!(list.batches.len(), 4);
        assert_eq!(list.batches[2].material, Material::Bloom);
    }

    #[test]
    fn rect_corners_and_uvs() {
        let mut list = DrawList::new(UVec2::new(100, 100));
        list.rect_uv(
            TextureId(1),
            Vec2::new(10.0, 20.0),
            Vec2::new(5.0, 6.0),
            Vec2::new(0.25, 0.5),
            Vec2::new(0.75, 1.0),
            Vec4::ONE,
        );
        let v = &list.batches[0].vertices;
        assert_eq!(v[0].pos, Vec2::new(10.0, 20.0));
        assert_eq!(v[2].pos, Vec2::new(15.0, 26.0));
        assert_eq!(v[1].uv, Vec2::new(0.75, 0.5));
        assert_eq!(v[3].uv, Vec2::new(0.25, 1.0));
    }

    #[test]
    fn faded_append_scales_alpha() {
        let mut a = DrawList::new(UVec2::new(10, 10));
        a.solid_rect(Vec2::ZERO, Vec2::ONE, Vec4::new(1.0, 1.0, 1.0, 0.5));
        let mut b = DrawList::new(UVec2::new(10, 10));
        b.append_faded(&a, 0.5);
        assert_eq!(b.batches[0].vertices[0].colour.w, 0.25);
    }
}
