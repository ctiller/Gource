//! PositionSlider: the seek bar (port of `slider.h`, `slider.cpp`).
//!
//! A seek bar displayed at the bottom of the viewport with mouse interaction
//! (hover, drag/click), bounds checking, fade in/out on hover or programmatically,
//! and an optional caption above the cursor.

use glam::{Vec2, Vec3, Vec4};
use gource_core::bounds::Bounds2D;
use gource_draw::font::TextStyle;
use gource_draw::{DrawList, FontId, Gfx};

/// Default margin gap from display edges (from `slider.cpp`: `int gap = 35;`).
pub const DEFAULT_SLIDER_GAP: f32 = 35.0;

/// Default line width for the slider box and progress bar in pixels.
pub const SLIDER_LINE_WIDTH: f32 = 2.0;

/// Default fade time in seconds.
pub const DEFAULT_FADE_TIME: f32 = 1.0;

/// PositionSlider widget.
///
/// Port of C++ `PositionSlider`.
#[derive(Debug, Clone, PartialEq)]
pub struct PositionSlider {
    pub font: Option<FontId>,
    pub bounds: Bounds2D,
    pub percent: f32,
    pub mouseover: f32,
    pub mouseover_elapsed: f32,
    pub fade_time: f32,
    pub alpha: f32,
    pub slidercol: Vec3,
    pub capwidth: f32,
    pub caption: String,
}

impl Default for PositionSlider {
    fn default() -> Self {
        Self::new(0.0)
    }
}

impl PositionSlider {
    /// Create a new `PositionSlider` with an initial percentage (0.0 .. 1.0).
    ///
    /// Port of `PositionSlider::PositionSlider(float percent)`.
    pub fn new(percent: f32) -> Self {
        Self {
            font: None,
            bounds: Bounds2D::new(),
            percent,
            mouseover: -1.0,
            mouseover_elapsed: DEFAULT_FADE_TIME,
            fade_time: DEFAULT_FADE_TIME,
            alpha: 0.0,
            slidercol: Vec3::ONE,
            capwidth: 0.0,
            caption: String::new(),
        }
    }

    /// Set font used for slider caption.
    ///
    /// In C++, this was loaded with `fontmanager.grab(gGourceSettings.font_file, 16 * gGourceSettings.font_scale)`.
    pub fn set_font(&mut self, font: FontId) {
        self.font = Some(font);
    }

    /// Set slider colour.
    ///
    /// Port of `PositionSlider::setColour(vec3 col)`.
    pub fn set_colour(&mut self, col: Vec3) {
        self.slidercol = col;
    }

    /// Set caption text and compute its width using the provided font size / width callback.
    ///
    /// Port of `PositionSlider::setCaption(const std::string& caption)`.
    pub fn set_caption(&mut self, caption: impl Into<String>, capwidth: f32) {
        self.caption = caption.into();
        self.capwidth = if self.caption.is_empty() {
            0.0
        } else {
            capwidth
        };
    }

    /// Set current progress percent (0.0 .. 1.0).
    ///
    /// Port of `PositionSlider::setPercent(float percent)`.
    pub fn set_percent(&mut self, percent: f32) {
        self.percent = percent;
    }

    /// Current progress percent.
    pub fn percent(&self) -> f32 {
        self.percent
    }

    /// Current alpha (0.0 .. 1.0).
    pub fn alpha(&self) -> f32 {
        self.alpha
    }

    /// Current bounds.
    ///
    /// Port of `PositionSlider::getBounds()`.
    pub fn bounds(&self) -> &Bounds2D {
        &self.bounds
    }

    /// Explicitly set slider bounds.
    pub fn set_bounds(&mut self, bounds: Bounds2D) {
        self.bounds = bounds;
    }

    /// Recalculate bounds given viewport dimensions (width, height) and margin gap.
    ///
    /// In C++:
    /// ```text
    /// int gap = 35;
    /// bounds.reset();
    /// bounds.update(vec2(gap, display.height - gap*2));
    /// bounds.update(vec2(display.width - gap, display.height - gap));
    /// ```
    pub fn resize(&mut self, display_width: f32, display_height: f32, gap: f32) {
        self.bounds.reset();
        self.bounds
            .update(Vec2::new(gap, display_height - gap * 2.0));
        self.bounds
            .update(Vec2::new(display_width - gap, display_height - gap));
    }

    /// Force show the slider (starts fading in or resets fade timer).
    ///
    /// Port of `PositionSlider::show()`.
    pub fn show(&mut self) {
        self.mouseover_elapsed = 0.0;
    }

    /// Test if mouse position is within bounds, updating mouseover state and calculating percent.
    ///
    /// Port of `PositionSlider::mouseOver(vec2 pos, float* percent_ptr)`.
    pub fn mouse_over(&mut self, pos: Vec2) -> Option<f32> {
        if self.bounds.contains(pos) {
            self.mouseover_elapsed = 0.0;
            self.mouseover = pos.x;

            let denom = self.bounds.max.x - self.bounds.min.x;
            let p = if denom > 0.0 {
                (pos.x - self.bounds.min.x) / denom
            } else {
                0.0
            };
            Some(p)
        } else {
            self.mouseover = -1.0;
            None
        }
    }

    /// Handle click at `pos`. If inside bounds, updates slider percent and returns the new percent.
    ///
    /// Port of `PositionSlider::click(vec2 pos, float* percent_ptr)`.
    pub fn click(&mut self, pos: Vec2) -> Option<f32> {
        if let Some(p) = self.mouse_over(pos) {
            self.percent = p;
            Some(p)
        } else {
            None
        }
    }

    /// Advance fade logic.
    ///
    /// Port of `PositionSlider::logic(float dt)`.
    pub fn logic(&mut self, dt: f32) {
        if self.mouseover < 0.0 && self.mouseover_elapsed < self.fade_time {
            self.mouseover_elapsed += dt;
        }

        if self.mouseover_elapsed < self.fade_time && self.alpha < 1.0 {
            self.alpha = 1.0f32.min(self.alpha + dt);
        } else if self.mouseover_elapsed >= self.fade_time && self.alpha > 0.0 {
            self.alpha = 0.0f32.max(self.alpha - dt);
        }
    }

    /// Helper to draw slider bounds and the indicator line at `pos_x`.
    ///
    /// Port of `PositionSlider::drawSlider(float position)`.
    pub fn draw_slider_outline(
        bounds: &Bounds2D,
        pos_x: f32,
        offset: Vec2,
        line_width: f32,
        colour: Vec4,
        list: &mut DrawList,
    ) {
        let min = bounds.min + offset;
        let max = bounds.max + offset;
        list.rect_outline(min, max, line_width, colour);
        list.line(
            Vec2::new(pos_x + offset.x, min.y),
            Vec2::new(pos_x + offset.x, max.y),
            line_width,
            colour,
        );
    }

    /// Draw the slider into `DrawList`.
    ///
    /// Port of `PositionSlider::draw(float dt)`.
    ///
    /// Parameters:
    /// - `gfx`: drawing context for text rendering (if caption is present).
    /// - `list`: target draw list.
    /// - `display_width`: total screen width for clamping caption position.
    /// - `font_scale`: scaling factor for caption height offset (C++ uses `25 * font_scale`).
    pub fn draw(&self, gfx: &mut Gfx, list: &mut DrawList, display_width: f32, font_scale: f32) {
        if self.alpha <= 0.0 {
            return;
        }

        let pos_x = self.bounds.min.x + (self.bounds.max.x - self.bounds.min.x) * self.percent;

        // Shadow: offset by (2.0, 2.0), black with 0.7 * alpha
        let shadow_colour = Vec4::new(0.0, 0.0, 0.0, 0.7 * self.alpha);
        Self::draw_slider_outline(
            &self.bounds,
            pos_x,
            Vec2::new(2.0, 2.0),
            SLIDER_LINE_WIDTH,
            shadow_colour,
            list,
        );

        // Foreground: slidercol with alpha
        let main_colour = Vec4::new(
            self.slidercol.x,
            self.slidercol.y,
            self.slidercol.z,
            self.alpha,
        );
        Self::draw_slider_outline(
            &self.bounds,
            pos_x,
            Vec2::ZERO,
            SLIDER_LINE_WIDTH,
            main_colour,
            list,
        );

        // Caption text if present and mouseover >= 0
        if let (false, true, Some(font)) =
            (self.caption.is_empty(), self.mouseover >= 0.0, self.font)
        {
            let capwidth = if self.capwidth > 0.0 {
                self.capwidth
            } else {
                gfx.text_width(font, &self.caption)
            };
            let height_offset = 25.0 * font_scale;
            let min_x = 1.0f32;
            let max_x = (display_width - capwidth - 1.0).max(1.0);
            let ideal_x = self.mouseover - (capwidth / 2.0);
            let text_x = ideal_x.clamp(min_x, max_x);
            let text_y = self.bounds.min.y - height_offset;

            let style = TextStyle::new(Vec4::ONE).with_shadow(true);
            gfx.draw_text(list, font, Vec2::new(text_x, text_y), &self.caption, &style);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::UVec2;
    use gource_draw::TextureId;

    #[test]
    fn slider_resize_and_bounds() {
        let mut slider = PositionSlider::new(0.25);
        slider.resize(800.0, 600.0, 35.0);
        let b = slider.bounds();
        assert_eq!(b.min, Vec2::new(35.0, 600.0 - 70.0)); // (35, 530)
        assert_eq!(b.max, Vec2::new(800.0 - 35.0, 600.0 - 35.0)); // (765, 565)
        assert_eq!(slider.percent(), 0.25);
    }

    #[test]
    fn slider_mouse_over_and_click() {
        let mut slider = PositionSlider::new(0.0);
        slider.resize(800.0, 600.0, 35.0);

        // Point outside bounds
        assert_eq!(slider.mouse_over(Vec2::new(10.0, 10.0)), None);
        assert_eq!(slider.mouseover, -1.0);

        // Point inside bounds: min.x = 35.0, max.x = 765.0, width = 730.0.
        // pos.x = 400.0 -> (400 - 35) / 730 = 365 / 730 = 0.5.
        let pos = Vec2::new(400.0, 540.0);
        let p = slider.mouse_over(pos);
        assert_eq!(p, Some(0.5));
        assert_eq!(slider.mouseover, 400.0);
        assert_eq!(slider.mouseover_elapsed, 0.0);

        // Click outside
        assert_eq!(slider.click(Vec2::new(0.0, 0.0)), None);
        assert_eq!(slider.percent(), 0.0);

        // Click inside
        let click_p = slider.click(pos);
        assert_eq!(click_p, Some(0.5));
        assert_eq!(slider.percent(), 0.5);
    }

    #[test]
    fn slider_fade_logic() {
        let mut slider = PositionSlider::new(0.0);
        slider.resize(800.0, 600.0, 35.0);
        assert_eq!(slider.alpha(), 0.0);

        // When mouse is hovering, mouseover >= 0:
        slider.mouseover = 100.0;
        slider.mouseover_elapsed = 0.0;

        // Logic advances alpha: dt = 0.4 -> alpha = 0.4
        slider.logic(0.4);
        assert_eq!(slider.alpha(), 0.4);

        // Logic advances alpha: dt = 0.6 -> alpha = 1.0 (capped at 1.0)
        slider.logic(0.6);
        assert_eq!(slider.alpha(), 1.0);

        // Mouse leaves: mouseover becomes < 0
        slider.mouseover = -1.0;
        // elapsed was 0.0; after dt = 0.3, elapsed = 0.3 < fade_time (1.0)
        // alpha stays at 1.0
        slider.logic(0.3);
        assert_eq!(slider.mouseover_elapsed, 0.3);
        assert_eq!(slider.alpha(), 1.0);

        // after another dt = 0.7, elapsed reaches 1.0 >= fade_time
        // and since elapsed was already 1.0 at the second if-check, alpha decreases by 0.7!
        // alpha becomes 1.0 - 0.7 = 0.3.
        slider.logic(0.7);
        assert_eq!(slider.mouseover_elapsed, 1.0);
        assert!((slider.alpha() - 0.3).abs() < 1e-5);

        // Next tick: elapsed >= fade_time (1.0), so alpha decreases:
        slider.logic(0.3);
        assert_eq!(slider.alpha(), 0.0);
    }

    #[test]
    fn slider_draw_geometry() {
        let mut slider = PositionSlider::new(0.5);
        slider.resize(800.0, 600.0, 35.0);
        slider.mouseover = 100.0;
        slider.show();
        slider.logic(0.5); // alpha = 0.5

        let mut list = DrawList::new(UVec2::new(800, 600));
        let mut gfx = Gfx {
            textures: gource_draw::TextureStore::default(),
            fonts: gource_draw::FontStore::default(),
        };

        slider.draw(&mut gfx, &mut list, 800.0, 1.0);

        // Should have batches with lines for:
        // Shadow: 4 lines for outline + 1 line for progress = 5 lines (5 quads = 20 vertices)
        // Main: 4 lines for outline + 1 line for progress = 5 lines (5 quads = 20 vertices)
        // Total 40 vertices in batch(es)
        assert!(!list.is_empty());
        assert_eq!(list.vertex_count(), 40);

        // Alpha <= 0 should produce empty list
        let empty_slider = PositionSlider::default();
        let mut list2 = DrawList::new(UVec2::new(800, 600));
        empty_slider.draw(&mut gfx, &mut list2, 800.0, 1.0);
        assert!(list2.is_empty());
    }

    #[test]
    fn slider_draw_with_caption() {
        let mut gfx = Gfx::new();
        let face = gfx.fonts.default_face();
        let font = gfx.fonts.font(face, 16);

        let mut slider = PositionSlider::new(0.5);
        slider.set_font(font);
        slider.resize(800.0, 600.0, 35.0);
        let cap_width = gfx.text_width(font, "2026-09-28");
        slider.set_caption("2026-09-28", cap_width);

        // Hover active
        let mouse_pos = Vec2::new(400.0, 540.0);
        slider.mouse_over(mouse_pos);
        slider.logic(1.0); // alpha reaches 1.0

        let mut list = DrawList::new(UVec2::new(800, 600));
        slider.draw(&mut gfx, &mut list, 800.0, 1.0);
        assert!(!list.is_empty());
    }

    #[test]
    fn slider_set_bounds() {
        let mut slider = PositionSlider::new(0.0);
        let bounds = Bounds2D::from_points(Vec2::new(10.0, 20.0), Vec2::new(100.0, 50.0));
        slider.set_bounds(bounds);
        assert_eq!(*slider.bounds(), bounds);

        // Zero-width bounds test (denom <= 0 branch)
        let zero_bounds = Bounds2D::from_points(Vec2::new(50.0, 20.0), Vec2::new(50.0, 50.0));
        slider.set_bounds(zero_bounds);
        assert_eq!(slider.mouse_over(Vec2::new(50.0, 30.0)), Some(0.0));
    }

    #[test]
    fn slider_setters_and_empty_caption() {
        let mut slider = PositionSlider::new(0.1);
        slider.set_colour(Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(slider.slidercol, Vec3::new(1.0, 0.0, 0.0));

        slider.set_percent(0.8);
        assert_eq!(slider.percent(), 0.8);

        // Empty caption
        slider.set_caption("", 100.0);
        assert_eq!(slider.capwidth, 0.0);
        assert!(slider.caption.is_empty());
    }

    #[test]
    fn slider_exact_draw_geometry() {
        let mut slider = PositionSlider::new(0.5);
        slider.resize(800.0, 600.0, 35.0);
        // Bounds min = (35, 530), max = (765, 565). pos_x = 35 + (765 - 35) * 0.5 = 400.0
        slider.set_colour(Vec3::new(0.8, 0.9, 1.0));
        slider.mouseover = 400.0;
        slider.show();
        slider.logic(1.0); // alpha = 1.0

        let mut list = DrawList::new(UVec2::new(800, 600));
        let mut gfx = Gfx {
            textures: gource_draw::TextureStore::default(),
            fonts: gource_draw::FontStore::default(),
        };

        slider.draw(&mut gfx, &mut list, 800.0, 1.0);

        // Exactly 1 batch (TextureId::WHITE), 40 vertices (10 lines total = 20 triangles = 40 vertices)
        assert_eq!(list.batches.len(), 1);
        let batch = &list.batches[0];
        assert_eq!(batch.texture, TextureId::WHITE);
        assert_eq!(batch.vertices.len(), 40);

        // Check shadow outline colour: alpha * 0.7 = 0.7
        assert_eq!(batch.vertices[0].colour, Vec4::new(0.0, 0.0, 0.0, 0.7));
        // Check foreground outline colour: slidercol with alpha = 1.0
        // Shadow lines are 5 quads * 4 = 20 verts. Foreground starts at index 20:
        assert_eq!(batch.vertices[20].colour, Vec4::new(0.8, 0.9, 1.0, 1.0));
    }

    #[test]
    fn slider_caption_clamping() {
        let mut gfx = Gfx::new();
        let face = gfx.fonts.default_face();
        let font = gfx.fonts.font(face, 16);

        let mut slider = PositionSlider::new(0.5);
        slider.set_font(font);
        slider.resize(800.0, 600.0, 35.0);
        slider.set_caption("Test Caption", 100.0);

        // Test clamping left: mouseover at 10.0 (ideal_x = 10 - 50 = -40 -> clamped to 1.0)
        slider.mouseover = 10.0;
        slider.show();
        slider.logic(1.0); // alpha = 1.0
        let mut list_left = DrawList::new(UVec2::new(800, 600));
        slider.draw(&mut gfx, &mut list_left, 800.0, 1.0);
        assert!(!list_left.is_empty());

        // Test clamping right: mouseover at 790.0 (ideal_x = 790 - 50 = 740 -> clamped to 800 - 100 - 1 = 699.0)
        slider.mouseover = 790.0;
        slider.show();
        slider.logic(1.0); // alpha = 1.0
        let mut list_right = DrawList::new(UVec2::new(800, 600));
        slider.draw(&mut gfx, &mut list_right, 800.0, 1.0);
        assert!(!list_right.is_empty());
    }
}
