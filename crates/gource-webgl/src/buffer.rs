//! GPU buffer batching and packing plan.
//!
//! Provides an abstraction for frame buffer management. Supports packing
//! all batches in a `DrawList` into a single interleaved vertex buffer (with byte offsets)
//! and a single index buffer, or preparing per-batch buffers.
//!
//! Here we pack the entire frame into a single contiguous vertex buffer and index buffer,
//! with per-batch element offsets. This minimizes buffer re-bindings and WebGL driver overhead.

use crate::plan::{BatchDrawPlan, blend_mode_for_material, pack_vertices};
use gource_draw::DrawList;

/// Frame buffers ready for upload to WebGL.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct FrameDrawData {
    /// Interleaved floats for all vertices in all batches.
    pub vertices: Vec<f32>,
    /// Indices for all batches (u32).
    pub indices: Vec<u32>,
    /// Draw call list in painter's order.
    pub batches: Vec<BatchDrawPlan>,
}

impl FrameDrawData {
    /// Prepares drawing data for all non-empty batches in the `DrawList`.
    ///
    /// Empty batches or batches with zero indices are omitted.
    pub fn from_draw_list(list: &DrawList) -> Self {
        let total_verts: usize = list.batches.iter().map(|b| b.vertices.len()).sum();
        let total_indices: usize = list.batches.iter().map(|b| b.indices.len()).sum();

        let mut vertices = Vec::with_capacity(total_verts * 8);
        let mut indices = Vec::with_capacity(total_indices);
        let mut batches = Vec::with_capacity(list.batches.len());

        let mut current_vert_offset = 0;
        let mut current_idx_offset = 0;

        for batch in &list.batches {
            if batch.indices.is_empty() || batch.vertices.is_empty() {
                continue;
            }

            pack_vertices(&batch.vertices, &mut vertices);

            // In DrawList, batch.indices are relative to batch.vertices (base 0).
            // When using glDrawElements with byte offset into the index buffer,
            // or base vertex offset, in WebGL2 we can add the vertex base to the index
            // or use drawElements with index offset if indices already reference the global vertex array.
            // Adjusting indices here so they refer directly to the global vertex buffer
            // makes glDrawElements call simple and robust across all WebGL2 implementations.
            let base_vertex = current_vert_offset as u32;
            for &idx in &batch.indices {
                indices.push(base_vertex + idx);
            }

            let vertex_count = batch.vertices.len();
            let index_count = batch.indices.len();

            batches.push(BatchDrawPlan {
                material: batch.material,
                texture: batch.texture,
                vertex_offset: current_vert_offset,
                vertex_count,
                index_offset: current_idx_offset,
                index_count,
                blend: blend_mode_for_material(batch.material),
            });

            current_vert_offset += vertex_count;
            current_idx_offset += index_count;
        }

        Self {
            vertices,
            indices,
            batches,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.batches.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gource_core::{UVec2, Vec2, Vec4};
    use gource_draw::{Batch, Material, TextureId, Vertex};

    #[test]
    fn test_empty_draw_list() {
        let list = DrawList::new(UVec2::new(800, 600));
        let data = FrameDrawData::from_draw_list(&list);
        assert!(data.is_empty());
        assert_eq!(data.vertices.len(), 0);
        assert_eq!(data.indices.len(), 0);
        assert_eq!(data.batches.len(), 0);
    }

    #[test]
    fn test_empty_batch_skipped() {
        let mut list = DrawList::new(UVec2::new(800, 600));
        // Add an empty batch explicitly
        list.batches
            .push(Batch::new(Material::Alpha, TextureId::WHITE));
        // Add a batch with vertices but no indices
        let mut batch_no_idx = Batch::new(Material::Alpha, TextureId::WHITE);
        batch_no_idx.vertices.push(Vertex::default());
        list.batches.push(batch_no_idx);

        let data = FrameDrawData::from_draw_list(&list);
        assert!(data.is_empty());
        assert_eq!(data.batches.len(), 0);
    }

    #[test]
    fn test_frame_draw_data_offsets_and_indices() {
        let mut list = DrawList::new(UVec2::new(800, 600));
        list.solid_rect(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0), Vec4::ONE);
        list.bloom(Vec2::new(50.0, 50.0), 20.0, Vec4::ONE);

        let data = FrameDrawData::from_draw_list(&list);
        assert_eq!(data.batches.len(), 2);

        // First batch: 4 vertices, 6 indices
        assert_eq!(data.batches[0].material, Material::Alpha);
        assert_eq!(data.batches[0].texture, TextureId::WHITE);
        assert_eq!(data.batches[0].vertex_offset, 0);
        assert_eq!(data.batches[0].vertex_count, 4);
        assert_eq!(data.batches[0].index_offset, 0);
        assert_eq!(data.batches[0].index_count, 6);

        // Second batch: 4 vertices, 6 indices
        assert_eq!(data.batches[1].material, Material::Bloom);
        assert_eq!(data.batches[1].vertex_offset, 4);
        assert_eq!(data.batches[1].vertex_count, 4);
        assert_eq!(data.batches[1].index_offset, 6);
        assert_eq!(data.batches[1].index_count, 6);

        // Total 8 vertices = 64 floats
        assert_eq!(data.vertices.len(), 64);
        // Total 12 indices
        assert_eq!(data.indices.len(), 12);

        // Check index offset for second batch
        assert_eq!(data.indices[0..6], [0, 1, 2, 0, 2, 3]);
        assert_eq!(data.indices[6..12], [4, 5, 6, 4, 6, 7]);
    }
}
