//! Plain-`wgpu` [`gource_draw::DrawList`] renderer for Gource.
//!
//! Supports offscreen rendering to an RGBA8 texture with synchronous CPU readback
//! (used by `gource-cli`, `gource-serve` video streaming, and pixel tests) as well
//! as rendering directly to a `wgpu::TextureView`.

pub mod plan;
pub mod renderer;
pub mod shader;

pub use plan::{
    BatchDrawPlan, COPY_BYTES_PER_ROW_ALIGNMENT, FrameDrawData, GL_ADDITIVE_BLEND, GL_ALPHA_BLEND,
    GpuVertex, TextureAction, TextureTracker, padded_bytes_per_row, unpad_rgba_rows,
};
pub use renderer::{OffscreenTarget, RenderError, WgpuRenderer};
pub use shader::{BLOOM_WGSL, SCENE_WGSL};

#[cfg(test)]
mod tests {
    use super::*;
    use gource_core::{UVec2, Vec2, Vec4};
    use gource_draw::{DrawList, TextureOptions, TextureStore};

    fn pixel(rgba: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let idx = ((y * width + x) * 4) as usize;
        [rgba[idx], rgba[idx + 1], rgba[idx + 2], rgba[idx + 3]]
    }

    #[test]
    fn render_error_display() {
        assert_eq!(
            RenderError::NoAdapter.to_string(),
            "no compatible wgpu adapter found"
        );
        assert_eq!(
            RenderError::Device("oops".into()).to_string(),
            "failed to create wgpu device: oops"
        );
        assert_eq!(
            RenderError::MapFailed.to_string(),
            "failed to map offscreen readback buffer"
        );
    }

    #[test]
    fn offscreen_renders_alpha_and_bloom_materials() {
        let mut renderer = WgpuRenderer::new_headless(false).expect("wgpu headless adapter");
        assert_eq!(renderer.sample_count(), 1);

        let mut store = TextureStore::new();
        let mut target = OffscreenTarget::new(renderer.device(), 32, 32, 1);
        assert_eq!((target.width(), target.height()), (32, 32));

        // 1. Empty draw list clears to clear_colour.
        let mut list = DrawList::new(UVec2::new(64, 32));
        list.reset(UVec2::new(64, 32), Vec4::new(0.0, 0.0, 0.0, 1.0));
        let rgba = renderer
            .render_offscreen(&list, &store, &mut target)
            .expect("render empty");
        assert_eq!((target.width(), target.height()), (64, 32));
        assert_eq!(rgba.len(), 64 * 32 * 4);
        assert_eq!(pixel(&rgba, 64, 1, 1), [0, 0, 0, 255]);

        // 2. White solid rect (Material::Alpha) + green glow (Material::Bloom).
        list.solid_rect(Vec2::ZERO, Vec2::new(8.0, 8.0), Vec4::ONE);
        list.bloom(Vec2::new(32.0, 16.0), 8.0, Vec4::new(0.0, 1.0, 0.0, 1.0));

        // Also exercise texture upload/delete and buffer growth.
        let tex_id = store.create_rgba("t", 2, 2, vec![255; 16], TextureOptions::default());
        let rgba = renderer
            .render_offscreen(&list, &store, &mut target)
            .expect("render scene+bloom");
        let sq = pixel(&rgba, 64, 2, 2);
        assert!(sq[0] >= 250 && sq[1] >= 250 && sq[2] >= 250, "sq={sq:?}");
        let glow = pixel(&rgba, 64, 32, 16);
        assert!(
            glow[1] >= 40 && glow[0] < 16 && glow[2] < 16,
            "glow={glow:?}"
        );

        // Remove custom texture, exercise buffer growth (>1024 vertices), and render to view.
        let _ = renderer.queue();
        store.remove(tex_id);
        for i in 0..300 {
            let x = (i % 16) as f32 * 2.0;
            let y = (i / 16) as f32 * 2.0;
            list.solid_rect(Vec2::new(x, y), Vec2::new(1.0, 1.0), Vec4::ONE);
        }
        renderer.render_to_view(&list, &store, target.color_view(), None);

        let mut msaa_renderer = WgpuRenderer::new_headless(true).expect("wgpu msaa adapter");
        assert_eq!(msaa_renderer.sample_count(), 4);
        let msaa_rgba = msaa_renderer
            .render_offscreen(&list, &store, &mut target)
            .expect("render msaa");
        assert_eq!(msaa_rgba.len(), 64 * 32 * 4);
        let sq_msaa = pixel(&msaa_rgba, 64, 2, 2);
        assert!(sq_msaa[0] >= 200, "sq_msaa={sq_msaa:?}");
    }
}
