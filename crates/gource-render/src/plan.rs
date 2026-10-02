//! Pure rendering planning, vertex/index packing, texture version diffing, and
//! row-padding utilities for `wgpu`.

use std::collections::{HashMap, HashSet};

use bytemuck::{Pod, Zeroable};
use gource_draw::{DrawList, Filter, Material, TextureId, TextureStore, Vertex, Wrap};

/// Byte alignment required by `wgpu` for `copy_texture_to_buffer` rows.
pub const COPY_BYTES_PER_ROW_ALIGNMENT: u32 = 256;

/// Interleaved GPU vertex matching the layout expected by `scene.wgsl` and `bloom.wgsl`.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub colour: [f32; 4],
}

impl From<&Vertex> for GpuVertex {
    fn from(v: &Vertex) -> Self {
        Self {
            pos: [v.pos.x, v.pos.y],
            uv: [v.uv.x, v.uv.y],
            colour: [v.colour.x, v.colour.y, v.colour.z, v.colour.w],
        }
    }
}

/// Planned parameters for a single batch draw call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchDrawPlan {
    pub material: Material,
    pub texture: TextureId,
    pub index_offset: u32,
    pub index_count: u32,
}

/// Packed vertex/index buffers and batch plans for one frame.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct FrameDrawData {
    pub vertices: Vec<GpuVertex>,
    pub indices: Vec<u32>,
    pub batches: Vec<BatchDrawPlan>,
}

impl FrameDrawData {
    /// Pack all non-empty batches from `list` into contiguous vertex and index arrays.
    pub fn from_draw_list(list: &DrawList) -> Self {
        let total_verts: usize = list.batches.iter().map(|b| b.vertices.len()).sum();
        let total_indices: usize = list.batches.iter().map(|b| b.indices.len()).sum();

        let mut vertices = Vec::with_capacity(total_verts);
        let mut indices = Vec::with_capacity(total_indices);
        let mut batches = Vec::with_capacity(list.batches.len());

        for batch in &list.batches {
            if batch.vertices.is_empty() || batch.indices.is_empty() {
                continue;
            }
            let base_vertex = vertices.len() as u32;
            vertices.extend(batch.vertices.iter().map(GpuVertex::from));

            let index_offset = indices.len() as u32;
            indices.extend(batch.indices.iter().map(|&idx| base_vertex + idx));
            let index_count = batch.indices.len() as u32;

            batches.push(BatchDrawPlan {
                material: batch.material,
                texture: batch.texture,
                index_offset,
                index_count,
            });
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

/// Action required to synchronize GPU textures with a CPU [`TextureStore`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextureAction {
    Upload { id: TextureId, version: u64 },
    Delete { id: TextureId },
}

/// Tracks which [`TextureId`] versions are resident on the GPU.
#[derive(Debug, Default, Clone)]
pub struct TextureTracker {
    known_versions: HashMap<TextureId, u64>,
}

impl TextureTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn versions(&self) -> &HashMap<TextureId, u64> {
        &self.known_versions
    }

    pub fn plan_sync(&self, store: &TextureStore) -> Vec<TextureAction> {
        let mut actions = Vec::new();
        let mut live_ids = HashSet::new();

        for (id, texture) in store.iter() {
            live_ids.insert(id);
            match self.known_versions.get(&id) {
                Some(&ver) if ver == texture.version => {}
                _ => actions.push(TextureAction::Upload {
                    id,
                    version: texture.version,
                }),
            }
        }

        for &id in self.known_versions.keys() {
            if !live_ids.contains(&id) {
                actions.push(TextureAction::Delete { id });
            }
        }

        actions
    }

    pub fn record_upload(&mut self, id: TextureId, version: u64) {
        self.known_versions.insert(id, version);
    }

    pub fn record_delete(&mut self, id: TextureId) {
        self.known_versions.remove(&id);
    }
}

/// Map [`Wrap`] to `wgpu::AddressMode`.
pub fn wgpu_address_mode(wrap: Wrap) -> wgpu::AddressMode {
    match wrap {
        Wrap::Clamp => wgpu::AddressMode::ClampToEdge,
        Wrap::Repeat => wgpu::AddressMode::Repeat,
    }
}

/// Map [`Filter`] to `wgpu::FilterMode`.
pub fn wgpu_filter_mode(filter: Filter) -> wgpu::FilterMode {
    match filter {
        Filter::Nearest => wgpu::FilterMode::Nearest,
        Filter::Linear => wgpu::FilterMode::Linear,
    }
}

/// Map mipmap count to `wgpu::MipmapFilterMode`.
pub fn wgpu_mipmap_filter(mip_levels: usize) -> wgpu::MipmapFilterMode {
    if mip_levels > 1 {
        wgpu::MipmapFilterMode::Linear
    } else {
        wgpu::MipmapFilterMode::Nearest
    }
}

/// Blend state for `Material::Alpha`: `glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA)`.
pub const GL_ALPHA_BLEND: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::SrcAlpha,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::SrcAlpha,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
};

/// Blend state for `Material::Bloom`: `glBlendFunc(GL_ONE, GL_ONE)`.
pub const GL_ADDITIVE_BLEND: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    },
};

/// Compute the 256-byte aligned row pitch required by `wgpu` buffer-texture copies.
pub fn padded_bytes_per_row(width: u32) -> u32 {
    let unpadded = width.max(1) * 4;
    let align = COPY_BYTES_PER_ROW_ALIGNMENT;
    unpadded.div_ceil(align) * align
}

/// Strip row padding from a mapped `wgpu` readback buffer into tightly packed RGBA8 rows.
pub fn unpad_rgba_rows(padded: &[u8], width: u32, height: u32, padded_row_bytes: u32) -> Vec<u8> {
    let row_bytes = (width * 4) as usize;
    let stride = padded_row_bytes as usize;
    let mut out = Vec::with_capacity(row_bytes * height as usize);
    for y in 0..height as usize {
        let start = y * stride;
        let end = start + row_bytes;
        if let Some(slice) = padded.get(start..end) {
            out.extend_from_slice(slice);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use gource_core::{UVec2, Vec2, Vec4};
    use gource_draw::{Batch, TextureOptions};

    #[test]
    fn frame_draw_data_packs_and_skips_empty() {
        let mut list = DrawList::new(UVec2::new(640, 480));
        list.batches
            .push(Batch::new(Material::Alpha, TextureId::WHITE));
        list.solid_rect(
            Vec2::new(1.0, 2.0),
            Vec2::new(10.0, 20.0),
            Vec4::new(0.5, 0.25, 0.75, 1.0),
        );
        list.bloom(Vec2::new(30.0, 40.0), 8.0, Vec4::ONE);

        let data = FrameDrawData::from_draw_list(&list);
        assert!(!data.is_empty());
        assert_eq!(data.batches.len(), 2);
        assert_eq!(data.vertices.len(), 8);
        assert_eq!(data.indices.len(), 12);
        assert_eq!(data.batches[0].material, Material::Alpha);
        assert_eq!(data.batches[0].index_offset, 0);
        assert_eq!(data.batches[0].index_count, 6);
        assert_eq!(data.batches[1].material, Material::Bloom);
        assert_eq!(data.batches[1].index_offset, 6);
        assert_eq!(data.batches[1].index_count, 6);
        assert_eq!(data.indices[6..12], [4, 5, 6, 4, 6, 7]);
    }

    #[test]
    fn texture_tracker_sync_lifecycle() {
        let mut store = TextureStore::new();
        let mut tracker = TextureTracker::new();

        let actions = tracker.plan_sync(&store);
        assert_eq!(
            actions,
            vec![TextureAction::Upload {
                id: TextureId::WHITE,
                version: 1
            }]
        );
        tracker.record_upload(TextureId::WHITE, 1);
        assert!(tracker.plan_sync(&store).is_empty());

        let custom = store.create_rgba("c", 2, 2, vec![255; 16], TextureOptions::plain());
        assert_eq!(
            tracker.plan_sync(&store),
            vec![TextureAction::Upload {
                id: custom,
                version: 1
            }]
        );
        tracker.record_upload(custom, 1);

        store.update_rgba(custom, 0, 0, 1, 1, &[0, 0, 0, 0]);
        assert_eq!(
            tracker.plan_sync(&store),
            vec![TextureAction::Upload {
                id: custom,
                version: 2
            }]
        );
        tracker.record_upload(custom, 2);

        store.remove(custom);
        assert_eq!(
            tracker.plan_sync(&store),
            vec![TextureAction::Delete { id: custom }]
        );
        tracker.record_delete(custom);
        assert_eq!(tracker.versions().len(), 1);
    }

    #[test]
    fn sampler_mappings_and_row_padding() {
        assert_eq!(
            wgpu_address_mode(Wrap::Clamp),
            wgpu::AddressMode::ClampToEdge
        );
        assert_eq!(wgpu_address_mode(Wrap::Repeat), wgpu::AddressMode::Repeat);
        assert_eq!(wgpu_filter_mode(Filter::Nearest), wgpu::FilterMode::Nearest);
        assert_eq!(wgpu_filter_mode(Filter::Linear), wgpu::FilterMode::Linear);
        assert_eq!(wgpu_mipmap_filter(1), wgpu::MipmapFilterMode::Nearest);
        assert_eq!(wgpu_mipmap_filter(3), wgpu::MipmapFilterMode::Linear);

        assert_eq!(padded_bytes_per_row(1), 256);
        assert_eq!(padded_bytes_per_row(64), 256);
        assert_eq!(padded_bytes_per_row(65), 512);

        let mut padded = vec![0u8; 256 * 2];
        padded[0..4].copy_from_slice(&[1, 2, 3, 4]);
        padded[256..260].copy_from_slice(&[5, 6, 7, 8]);
        let unpadded = unpad_rgba_rows(&padded, 1, 2, 256);
        assert_eq!(unpadded, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }
}
