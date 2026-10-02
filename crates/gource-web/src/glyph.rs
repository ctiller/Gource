//! Pluggable Canvas2D glyph rasterisation for `gource-web`.
//!
//! On `wasm32`, glyphs are measured and rasterised into an offscreen 2D canvas,
//! removing the need for `ab_glyph` and embedded TTF font binaries in browser builds.

use gource_draw::RasterizedGlyph;

/// Extracts the tight bounding box and coverage bytes of a glyph from RGBA pixel data.
///
/// `rgba` contains `width * height * 4` bytes. `origin_x` is the horizontal position of the pen,
/// and `baseline_y` is the vertical position of the baseline within the canvas patch.
pub fn extract_tight_glyph_from_rgba(
    rgba: &[u8],
    width: u32,
    height: u32,
    origin_x: f32,
    baseline_y: f32,
) -> Option<RasterizedGlyph> {
    if width == 0 || height == 0 || rgba.len() < (width * height * 4) as usize {
        return None;
    }

    let mut min_x = u32::MAX;
    let mut max_x = 0;
    let mut min_y = u32::MAX;
    let mut max_y = 0;

    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 4 + 3) as usize;
            if rgba[idx] > 0 {
                if x < min_x {
                    min_x = x;
                }
                if x > max_x {
                    max_x = x;
                }
                if y < min_y {
                    min_y = y;
                }
                if y > max_y {
                    max_y = y;
                }
            }
        }
    }

    if min_x > max_x || min_y > max_y {
        return None;
    }

    let b_w = max_x - min_x + 1;
    let b_h = max_y - min_y + 1;
    let mut coverage = Vec::with_capacity((b_w * b_h) as usize);

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let idx = ((y * width + x) * 4 + 3) as usize;
            coverage.push(rgba[idx]);
        }
    }

    Some(RasterizedGlyph {
        width: b_w,
        height: b_h,
        min_x: min_x as f32 - origin_x,
        min_y: min_y as f32 - baseline_y,
        coverage,
    })
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use gource_draw::{FaceId, FontError, FontMetrics, GlyphRasterizer};
    use std::cell::RefCell;
    use wasm_bindgen::JsCast;
    use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

    struct CanvasState {
        canvas: HtmlCanvasElement,
        ctx: CanvasRenderingContext2d,
        current_size: u32,
    }

    impl CanvasState {
        fn prepare_font(&mut self, size: u32) {
            if self.current_size != size {
                self.ctx.set_font(&format!("{size}px sans-serif"));
                self.current_size = size;
            }
        }
    }

    thread_local! {
        static CANVAS_STATE: RefCell<Option<CanvasState>> = const { RefCell::new(None) };
    }

    fn with_canvas<R>(f: impl FnOnce(&mut CanvasState) -> R) -> Option<R> {
        CANVAS_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            if state.is_none() {
                let window = web_sys::window()?;
                let document = window.document()?;
                let canvas: HtmlCanvasElement =
                    document.create_element("canvas").ok()?.dyn_into().ok()?;
                canvas.set_width(256);
                canvas.set_height(256);
                let ctx: CanvasRenderingContext2d = canvas
                    .get_context("2d")
                    .ok()?
                    .and_then(|c| c.dyn_into().ok())?;
                *state = Some(CanvasState {
                    canvas,
                    ctx,
                    current_size: 0,
                });
            }
            state.as_mut().map(f)
        })
    }

    /// Canvas2D-backed glyph rasterizer for web builds.
    pub struct Canvas2dGlyphRasterizer {
        face_count: std::sync::atomic::AtomicU32,
    }

    impl Canvas2dGlyphRasterizer {
        pub fn new() -> Self {
            Self {
                face_count: std::sync::atomic::AtomicU32::new(0),
            }
        }
    }

    impl Default for Canvas2dGlyphRasterizer {
        fn default() -> Self {
            Self::new()
        }
    }

    impl GlyphRasterizer for Canvas2dGlyphRasterizer {
        fn load_face(&mut self, _name: &str, _data: &[u8]) -> Result<FaceId, FontError> {
            let id = self
                .face_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(FaceId(id))
        }

        fn metrics(&self, _face: FaceId, size: u32) -> FontMetrics {
            with_canvas(|state| {
                state.prepare_font(size);
                let (ascender, descender, max_advance) = if let Ok(tm) = state.ctx.measure_text("M")
                {
                    let max_adv = (tm.width() as f32).ceil();
                    let asc = tm.font_bounding_box_ascent();
                    let desc = tm.font_bounding_box_descent();
                    let (a, d) = if asc > 0.0 || desc > 0.0 {
                        (asc as f32, -(desc as f32))
                    } else {
                        let act_asc = tm.actual_bounding_box_ascent();
                        let act_desc = tm.actual_bounding_box_descent();
                        if act_asc > 0.0 || act_desc > 0.0 {
                            (act_asc as f32, -(act_desc as f32))
                        } else {
                            (size as f32 * 0.8, -(size as f32 * 0.2))
                        }
                    };
                    (a, d, max_adv)
                } else {
                    (
                        size as f32 * 0.8,
                        -(size as f32 * 0.2),
                        (size as f32 * 0.6).ceil(),
                    )
                };
                FontMetrics {
                    ascender,
                    descender,
                    max_advance,
                }
            })
            .unwrap_or_else(|| FontMetrics {
                ascender: size as f32 * 0.8,
                descender: -(size as f32 * 0.2),
                max_advance: (size as f32 * 0.6).ceil(),
            })
        }

        fn advance(&self, _face: FaceId, size: u32, ch: char) -> f32 {
            with_canvas(|state| {
                state.prepare_font(size);
                let mut buf = [0u8; 4];
                let s = ch.encode_utf8(&mut buf);
                state
                    .ctx
                    .measure_text(s)
                    .map(|tm| (tm.width() as f32).round())
                    .unwrap_or_else(|_| (size as f32 * 0.5).round())
            })
            .unwrap_or_else(|| (size as f32 * 0.5).round())
        }

        fn glyph_height(&self, _face: FaceId, size: u32, ch: char) -> f32 {
            if ch.is_whitespace() {
                return 0.0;
            }
            with_canvas(|state| {
                state.prepare_font(size);
                let mut buf = [0u8; 4];
                let s = ch.encode_utf8(&mut buf);
                if let Ok(tm) = state.ctx.measure_text(s) {
                    let h =
                        (tm.actual_bounding_box_ascent() + tm.actual_bounding_box_descent()) as f32;
                    if h > 0.0 {
                        return h.ceil();
                    }
                }
                (size as f32 * 0.7).ceil().max(1.0)
            })
            .unwrap_or_else(|| (size as f32 * 0.7).ceil().max(1.0))
        }

        fn rasterize(&self, _face: FaceId, size: u32, ch: char) -> Option<RasterizedGlyph> {
            if ch.is_whitespace() {
                return None;
            }
            with_canvas(|state| {
                state.prepare_font(size);
                let origin_x = size as f64;
                let baseline_y = (size as f64) * 1.5;
                let box_w = (size * 3).max(32);
                let box_h = (size * 3).max(32);

                if state.canvas.width() < box_w || state.canvas.height() < box_h {
                    state.canvas.set_width(box_w.max(state.canvas.width()));
                    state.canvas.set_height(box_h.max(state.canvas.height()));
                    state.current_size = 0;
                    state.prepare_font(size);
                }

                let ctx = &state.ctx;
                ctx.set_text_align("left");
                ctx.set_text_baseline("alphabetic");
                ctx.set_fill_style_str("white");
                ctx.clear_rect(0.0, 0.0, box_w as f64, box_h as f64);

                let mut buf = [0u8; 4];
                let s = ch.encode_utf8(&mut buf);
                let _ = ctx.fill_text(s, origin_x, baseline_y);

                let image_data = ctx
                    .get_image_data(0.0, 0.0, box_w as f64, box_h as f64)
                    .ok()?;
                let raw_data = image_data.data();
                extract_tight_glyph_from_rgba(
                    &raw_data,
                    box_w,
                    box_h,
                    origin_x as f32,
                    baseline_y as f32,
                )
            })
            .flatten()
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::Canvas2dGlyphRasterizer;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tight_glyph_empty() {
        let rgba = vec![0u8; 4 * 4 * 4];
        assert_eq!(extract_tight_glyph_from_rgba(&rgba, 4, 4, 0.0, 0.0), None);
    }

    #[test]
    fn test_extract_tight_glyph_zero_dimensions_or_short_buffer() {
        let rgba = vec![255u8; 16];
        assert_eq!(extract_tight_glyph_from_rgba(&rgba, 0, 4, 0.0, 0.0), None);
        assert_eq!(extract_tight_glyph_from_rgba(&rgba, 4, 0, 0.0, 0.0), None);
        assert_eq!(extract_tight_glyph_from_rgba(&rgba, 4, 4, 0.0, 0.0), None); // len 16 < 64
    }

    #[test]
    fn test_extract_tight_glyph_single_pixel() {
        let mut rgba = vec![0u8; 4 * 4 * 4];
        // Set pixel at (1, 2)
        let idx = (2 * 4 + 1) * 4;
        rgba[idx] = 255;
        rgba[idx + 1] = 255;
        rgba[idx + 2] = 255;
        rgba[idx + 3] = 180;

        let glyph = extract_tight_glyph_from_rgba(&rgba, 4, 4, 1.0, 2.0).expect("single pixel");
        assert_eq!(glyph.width, 1);
        assert_eq!(glyph.height, 1);
        assert_eq!(glyph.min_x, 0.0);
        assert_eq!(glyph.min_y, 0.0);
        assert_eq!(glyph.coverage, vec![180]);
    }

    #[test]
    fn test_extract_tight_glyph_multi_pixel_rect() {
        let mut rgba = vec![0u8; 5 * 5 * 4];
        // Pixels for x in 1..=3, y in 2..=3
        for y in 2..=3 {
            for x in 1..=3 {
                let idx = (y * 5 + x) * 4;
                rgba[idx + 3] = (x * 10 + y) as u8;
            }
        }

        let glyph = extract_tight_glyph_from_rgba(&rgba, 5, 5, 2.0, 3.0).expect("multi pixel");
        assert_eq!(glyph.width, 3);
        assert_eq!(glyph.height, 2);
        assert_eq!(glyph.min_x, 1.0 - 2.0); // -1.0
        assert_eq!(glyph.min_y, 2.0 - 3.0); // -1.0
        assert_eq!(glyph.coverage.len(), 6);
        // Row 2: (1,2)->12, (2,2)->22, (3,2)->32
        // Row 3: (1,3)->13, (2,3)->23, (3,3)->33
        assert_eq!(glyph.coverage, vec![12, 22, 32, 13, 23, 33]);
    }
}
