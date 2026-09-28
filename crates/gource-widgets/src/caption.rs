//! Captions: timed text overlays (port of `caption.h`, `caption.cpp`).
//!
//! An `RCaption` displays a text caption associated with a timestamp for a given duration.
//! During its lifetime it fades in and fades out based on elapsed time.

use glam::{Vec2, Vec3, Vec4};
use gource_draw::font::TextStyle;
use gource_draw::{DrawList, FontId, Gfx};

/// Default duration in seconds for a caption (from `gGourceSettings.caption_duration`).
pub const DEFAULT_CAPTION_DURATION: f32 = 10.0;

/// Default colour for captions (white).
pub const DEFAULT_CAPTION_COLOUR: Vec3 = Vec3::ONE;

/// Timed caption text overlay.
///
/// Port of C++ `RCaption`.
#[derive(Debug, Clone, PartialEq)]
pub struct RCaption {
    pub caption: String,
    pub timestamp: i64,
    pub font: FontId,
    pub pos: Vec2,
    pub colour: Vec3,
    pub alpha: f32,
    pub elapsed: f32,
    pub duration: f32,
}

impl RCaption {
    /// Create a new caption.
    ///
    /// Port of `RCaption::RCaption(const std::string& caption, time_t timestamp, const FXFont& font)`.
    pub fn new(caption: impl Into<String>, timestamp: i64, font: FontId) -> Self {
        Self::with_duration(caption, timestamp, font, DEFAULT_CAPTION_DURATION)
    }

    /// Create a new caption with an explicit duration and default colour.
    pub fn with_duration(
        caption: impl Into<String>,
        timestamp: i64,
        font: FontId,
        duration: f32,
    ) -> Self {
        Self {
            caption: caption.into(),
            timestamp,
            font,
            pos: Vec2::ZERO,
            colour: DEFAULT_CAPTION_COLOUR,
            alpha: 0.0,
            elapsed: 0.0,
            duration,
        }
    }

    /// Set position.
    ///
    /// Port of `RCaption::setPos(const vec2& pos)`.
    pub fn set_pos(&mut self, pos: Vec2) {
        self.pos = pos;
    }

    /// Get position.
    ///
    /// Port of `RCaption::getPos()`.
    pub fn pos(&self) -> Vec2 {
        self.pos
    }

    /// Get caption text.
    ///
    /// Port of `RCaption::getCaption()`.
    pub fn caption(&self) -> &str {
        &self.caption
    }

    /// Check if caption display duration has elapsed.
    ///
    /// Port of `RCaption::isFinished()`.
    pub fn is_finished(&self) -> bool {
        self.elapsed >= self.duration
    }

    /// Advance time and update fade alpha.
    ///
    /// Port of `RCaption::logic(float dt)`.
    pub fn logic(&mut self, dt: f32) {
        let fade_in = 2.0f32.min(self.duration / 3.0);
        self.elapsed += dt;
        let remaining = (self.duration - self.elapsed).max(0.0);
        if fade_in > 0.0 {
            self.alpha = 1.0f32.min(self.elapsed.min(remaining) / fade_in);
        } else {
            self.alpha = 1.0;
        }
    }

    /// Draw caption into the draw list.
    ///
    /// Port of `RCaption::draw()`.
    pub fn draw(&self, gfx: &mut Gfx, list: &mut DrawList) {
        if self.is_finished() || self.alpha <= 0.0 {
            return;
        }
        let text_colour = Vec4::new(self.colour.x, self.colour.y, self.colour.z, self.alpha);
        let style = TextStyle::new(text_colour);
        gfx.draw_text(list, self.font, self.pos, &self.caption, &style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caption_lifecycle_and_fade() {
        let font = FontId(1);
        let mut cap = RCaption::with_duration("Hello World", 123456, font, 6.0);
        assert_eq!(cap.caption(), "Hello World");
        assert_eq!(cap.timestamp, 123456);
        assert_eq!(cap.font, font);
        assert_eq!(cap.pos(), Vec2::ZERO);
        assert!(!cap.is_finished());
        assert_eq!(cap.alpha, 0.0);

        cap.set_pos(Vec2::new(10.0, 20.0));
        assert_eq!(cap.pos(), Vec2::new(10.0, 20.0));

        // fade_in = min(2.0, 6.0/3.0) = 2.0.
        // At dt = 1.0: elapsed = 1.0, remaining = 5.0. min(1.0, 5.0) / 2.0 = 0.5.
        cap.logic(1.0);
        assert_eq!(cap.elapsed, 1.0);
        assert_eq!(cap.alpha, 0.5);
        assert!(!cap.is_finished());

        // At dt = 1.0 (elapsed = 2.0): min(2.0, 4.0) / 2.0 = 1.0.
        cap.logic(1.0);
        assert_eq!(cap.elapsed, 2.0);
        assert_eq!(cap.alpha, 1.0);
        assert!(!cap.is_finished());

        // At dt = 2.0 (elapsed = 4.0): remaining = 2.0. min(4.0, 2.0) / 2.0 = 1.0.
        cap.logic(2.0);
        assert_eq!(cap.elapsed, 4.0);
        assert_eq!(cap.alpha, 1.0);
        assert!(!cap.is_finished());

        // At dt = 1.0 (elapsed = 5.0): remaining = 1.0. min(5.0, 1.0) / 2.0 = 0.5.
        cap.logic(1.0);
        assert_eq!(cap.elapsed, 5.0);
        assert_eq!(cap.alpha, 0.5);
        assert!(!cap.is_finished());

        // At dt = 1.0 (elapsed = 6.0): remaining = 0.0. alpha = 0.0. Finished!
        cap.logic(1.0);
        assert_eq!(cap.elapsed, 6.0);
        assert_eq!(cap.alpha, 0.0);
        assert!(cap.is_finished());
    }

    #[test]
    fn caption_short_duration() {
        let font = FontId(1);
        let mut cap = RCaption::with_duration("Short", 100, font, 1.5);
        // fade_in = min(2.0, 1.5/3.0) = 0.5
        cap.logic(0.25);
        assert_eq!(cap.alpha, 0.25 / 0.5); // 0.5
        cap.logic(0.25);
        assert_eq!(cap.alpha, 1.0);
    }

    #[test]
    fn caption_zero_duration() {
        let font = FontId(1);
        let mut cap = RCaption::with_duration("Instant", 100, font, 0.0);
        assert!(cap.is_finished());
        cap.logic(0.1);
        assert!(cap.is_finished());
    }

    #[test]
    fn caption_new_default_duration() {
        let font = FontId(2);
        let cap = RCaption::new("Test", 100, font);
        assert_eq!(cap.duration, DEFAULT_CAPTION_DURATION);
    }

    #[test]
    fn caption_draw() {
        let mut gfx = Gfx::new();
        let face = gfx.fonts.default_face();
        let font = gfx.fonts.font(face, 16);

        let mut cap = RCaption::with_duration("Caption Draw Test", 100, font, 10.0);
        cap.set_pos(Vec2::new(10.0, 20.0));
        cap.logic(1.0); // alpha > 0

        let mut list = DrawList::new(glam::UVec2::new(800, 600));
        cap.draw(&mut gfx, &mut list);
        assert!(!list.is_empty());
        // Verify batch and text vertices produced
        let total_verts = list.vertex_count();
        assert!(total_verts > 0);
        // Alpha at 1.0s is 1.0 / 2.0 = 0.5. Verify text quad vertex alpha is 0.5
        assert!((list.batches[0].vertices[0].colour.w - 0.5).abs() < 1e-4);

        // Alpha <= 0 caption should not draw
        let cap_zero = RCaption::with_duration("ZeroAlpha", 100, font, 10.0);
        assert_eq!(cap_zero.alpha, 0.0);
        let mut list0 = DrawList::new(glam::UVec2::new(800, 600));
        cap_zero.draw(&mut gfx, &mut list0);
        assert!(list0.is_empty());

        // Finished caption should not draw even if alpha was set > 0
        let mut cap_finished = RCaption::with_duration("Done", 100, font, 1.0);
        cap_finished.logic(2.0); // is_finished() is true
        cap_finished.alpha = 1.0;
        let mut list2 = DrawList::new(glam::UVec2::new(800, 600));
        cap_finished.draw(&mut gfx, &mut list2);
        assert!(list2.is_empty());
    }

    #[test]
    fn caption_fade_alpha_formula_precision() {
        // C++:
        // fade_in = min(2.0f, duration / 3.0f);
        // alpha = min(1.0f, min(elapsed, max(0.0f, duration - elapsed)) / fade_in);
        // Test with duration = 9.0s -> fade_in = min(2.0, 3.0) = 2.0s
        let font = FontId(1);
        let mut cap = RCaption::with_duration("Fade Test", 500, font, 9.0);

        // At t = 0.5s: elapsed = 0.5, remaining = 8.5 -> min(0.5, 8.5) / 2.0 = 0.25
        cap.logic(0.5);
        assert!((cap.alpha - 0.25).abs() < 1e-6);

        // At t = 2.0s: elapsed = 2.0, remaining = 7.0 -> min(2.0, 7.0) / 2.0 = 1.0
        cap.logic(1.5);
        assert_eq!(cap.alpha, 1.0);

        // At t = 7.0s: elapsed = 7.0, remaining = 2.0 -> min(7.0, 2.0) / 2.0 = 1.0
        cap.logic(5.0);
        assert_eq!(cap.alpha, 1.0);

        // At t = 8.0s: elapsed = 8.0, remaining = 1.0 -> min(8.0, 1.0) / 2.0 = 0.5
        cap.logic(1.0);
        assert!((cap.alpha - 0.5).abs() < 1e-6);

        // At t = 8.9s: elapsed = 8.9, remaining = 0.1 -> min(8.9, 0.1) / 2.0 = 0.05
        cap.logic(0.9);
        assert!((cap.alpha - 0.05).abs() < 1e-6);

        // At t = 9.0s: elapsed = 9.0, remaining = 0.0 -> alpha = 0.0, is_finished = true
        cap.logic(0.1);
        assert_eq!(cap.alpha, 0.0);
        assert!(cap.is_finished());
    }
}
