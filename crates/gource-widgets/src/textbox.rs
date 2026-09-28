//! TextBox: hover tooltip with lines of text, clamping to screen, and background box
//! (port of `textbox.h`, `textbox.cpp`).
//!
//! Renders a multi-line tooltip with a drop shadow, coloured background quad,
//! and text placed within the box. Clamps position so the tooltip does not fall off
//! the screen edges.

use glam::{Vec2, Vec3, Vec4};
use gource_draw::font::TextStyle;
use gource_draw::{DrawList, FontId, Gfx};

/// Default background colour for TextBox (0.7, 0.7, 0.7 from C++ `colour = vec3(0.7f, 0.7f, 0.7f)`).
pub const DEFAULT_TEXTBOX_COLOUR: Vec3 = Vec3::new(0.7, 0.7, 0.7);

/// Default shadow offset (3.0, 3.0 from C++ `shadow = vec2(3.0f, 3.0f)`).
pub const DEFAULT_TEXTBOX_SHADOW: Vec2 = Vec2::new(3.0, 3.0);

/// TextBox widget.
///
/// Port of C++ `TextBox`.
#[derive(Debug, Clone, PartialEq)]
pub struct TextBox {
    pub content: Vec<String>,
    pub colour: Vec3,
    pub alpha: f32,
    pub brightness: f32,
    pub corner: Vec2,
    pub shadow: Vec2,
    pub font: Option<FontId>,
    pub font_size: f32,
    pub max_width_chars: usize,
    pub rect_width: i32,
    pub rect_height: i32,
    pub visible: bool,
}

impl Default for TextBox {
    fn default() -> Self {
        Self::new()
    }
}

impl TextBox {
    /// Create a new empty TextBox without a font assigned.
    ///
    /// Port of `TextBox::TextBox()`.
    pub fn new() -> Self {
        Self {
            content: Vec::new(),
            colour: DEFAULT_TEXTBOX_COLOUR,
            alpha: 1.0,
            brightness: 1.0,
            corner: Vec2::ZERO,
            shadow: DEFAULT_TEXTBOX_SHADOW,
            font: None,
            font_size: 18.0, // default in Gource for tooltip is 18 * font_scale
            max_width_chars: 1024,
            rect_width: 0,
            rect_height: 0,
            visible: false,
        }
    }

    /// Create a new TextBox with a specific font and font size.
    ///
    /// Port of `TextBox::TextBox(const FXFont& font)`.
    pub fn with_font(font: FontId, font_size: f32) -> Self {
        Self {
            font: Some(font),
            font_size,
            ..Self::new()
        }
    }

    /// Set font and size.
    pub fn set_font(&mut self, font: FontId, font_size: f32) {
        self.font = Some(font);
        self.font_size = font_size;
    }

    /// Hide tooltip.
    ///
    /// Port of `TextBox::hide()`.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Show tooltip.
    ///
    /// Port of `TextBox::show()`.
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Whether tooltip is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Set tooltip brightness multiplier.
    ///
    /// Port of `TextBox::setBrightness(float brightness)`.
    pub fn set_brightness(&mut self, brightness: f32) {
        self.brightness = brightness;
    }

    /// Set background colour.
    ///
    /// Port of `TextBox::setColour(const vec3& colour)`.
    pub fn set_colour(&mut self, colour: Vec3) {
        self.colour = colour;
    }

    /// Set alpha transparency.
    ///
    /// Port of `TextBox::setAlpha(float alpha)`.
    pub fn set_alpha(&mut self, alpha: f32) {
        self.alpha = alpha;
    }

    /// Clear all lines.
    ///
    /// Port of `TextBox::clear()`.
    pub fn clear(&mut self) {
        self.content.clear();
        self.rect_width = 0;
        self.rect_height = 2;
    }

    /// Add a line of text, measuring its width using `line_width`.
    ///
    /// Port of `TextBox::addLine(std::string str)`.
    pub fn add_line(&mut self, mut line: String, line_width: f32) {
        if self.max_width_chars > 0 && line.len() > self.max_width_chars {
            // Find character boundary to avoid slicing inside multi-byte UTF-8
            let mut end = self.max_width_chars;
            while end > 0 && !line.is_char_boundary(end) {
                end -= 1;
            }
            line.truncate(end);
        }

        let width = line_width as i32 + 6;
        if width > self.rect_width {
            self.rect_width = width;
        }

        self.rect_height += self.font_size as i32 + 4;
        self.content.push(line);
    }

    /// Set single string content.
    ///
    /// Port of `TextBox::setText(const std::string& str)`.
    pub fn set_text(&mut self, text: impl Into<String>, line_width: f32) {
        self.clear();
        self.add_line(text.into(), line_width);
    }

    /// Set multi-line content.
    ///
    /// Port of `TextBox::setText(const std::vector<std::string>& content)`.
    pub fn set_text_lines<I>(&mut self, lines: I)
    where
        I: IntoIterator<Item = (String, f32)>,
    {
        self.clear();
        for (line, width) in lines {
            self.add_line(line, width);
        }
    }

    /// Set position, optionally adjusting (clamping) to screen bounds.
    ///
    /// Port of `TextBox::setPos(const vec2& pos, bool adjust = false)`.
    ///
    /// In C++:
    /// ```text
    /// corner = pos;
    /// if(!adjust) return;
    /// int fontheight = font.getFontSize() + 4;
    /// corner.y -= rect_height;
    /// if((corner.x + rect_width) > display.width) {
    ///     if((corner.x - rect_width - fontheight) > 0) {
    ///         corner.x -= rect_width;
    ///     } else {
    ///         corner.x = display.width - rect_width;
    ///     }
    /// }
    /// if(corner.y < 0) corner.y += rect_height + fontheight;
    /// if(corner.y + rect_height > display.height) corner.y -= rect_height;
    /// ```
    pub fn set_pos(&mut self, pos: Vec2, adjust: bool, display_width: f32, display_height: f32) {
        self.corner = pos;
        if !adjust {
            return;
        }

        let fontheight = self.font_size as i32 + 4;
        self.corner.y -= self.rect_height as f32;

        if (self.corner.x + self.rect_width as f32) > display_width {
            if (self.corner.x - self.rect_width as f32 - fontheight as f32) > 0.0 {
                self.corner.x -= self.rect_width as f32;
            } else {
                self.corner.x = display_width - self.rect_width as f32;
            }
        }

        if self.corner.y < 0.0 {
            self.corner.y += (self.rect_height + fontheight) as f32;
        }
        if self.corner.y + self.rect_height as f32 > display_height {
            self.corner.y -= self.rect_height as f32;
        }
    }

    /// Draw tooltip into DrawList.
    ///
    /// Port of `TextBox::draw() const`.
    ///
    /// In C++:
    /// - Shadow quad: `corner + shadow`, size `(rect_width, rect_height)`, colour `(0, 0, 0, alpha * 0.333f)`.
    /// - Background quad: `corner`, size `(rect_width, rect_height)`, colour `(colour * brightness, alpha)`.
    /// - Lines of text: drawn at `(corner.x + 2, corner.y + yinc)` with text colour `(1, 1, 1, alpha)` and
    ///   `yinc` starting at 3 and incrementing by `font_size + 4`.
    pub fn draw(&self, gfx: &mut Gfx, list: &mut DrawList) {
        if !self.visible || self.alpha <= 0.0 {
            return;
        }

        let size = Vec2::new(self.rect_width as f32, self.rect_height as f32);

        // Drop shadow quad
        let shadow_colour = Vec4::new(0.0, 0.0, 0.0, self.alpha * 0.333);
        list.solid_rect(self.corner + self.shadow, size, shadow_colour);

        // Coloured background quad
        let bg_rgb = self.colour * self.brightness;
        let bg_colour = Vec4::new(bg_rgb.x, bg_rgb.y, bg_rgb.z, self.alpha);
        list.solid_rect(self.corner, size, bg_colour);

        // Content lines
        if let Some(font) = self.font {
            let mut yinc = 3.0f32;
            let font_step = self.font_size + 4.0;
            let text_colour = Vec4::new(1.0, 1.0, 1.0, self.alpha);
            let style = TextStyle::new(text_colour);

            for line in &self.content {
                let text_pos = Vec2::new(
                    (self.corner.x as i32 + 2) as f32,
                    (self.corner.y as i32 + yinc as i32) as f32,
                );
                gfx.draw_text(list, font, text_pos, line, &style);
                yinc += font_step;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::UVec2;
    use gource_draw::TextureId;

    #[test]
    fn textbox_clear_and_add_lines() {
        let mut box_ = TextBox::with_font(FontId(1), 18.0);
        assert_eq!(box_.rect_height, 0);
        assert_eq!(box_.rect_width, 0);

        box_.add_line("First".to_string(), 40.0);
        // rect_width = max(0, 40 + 6) = 46
        // rect_height = 0 + (18 + 4) = 22
        assert_eq!(box_.rect_width, 46);
        assert_eq!(box_.rect_height, 22);

        box_.add_line("Second line which is longer".to_string(), 150.0);
        // rect_width = max(46, 150 + 6) = 156
        // rect_height = 22 + 22 = 44
        assert_eq!(box_.rect_width, 156);
        assert_eq!(box_.rect_height, 44);
        assert_eq!(box_.content.len(), 2);

        box_.clear();
        assert_eq!(box_.rect_width, 0);
        assert_eq!(box_.rect_height, 2);
        assert!(box_.content.is_empty());
    }

    #[test]
    fn textbox_set_text_and_lines() {
        let mut box_ = TextBox::new();
        box_.font_size = 18.0;

        box_.set_text("Hello", 50.0);
        assert_eq!(box_.content, vec!["Hello"]);
        assert_eq!(box_.rect_width, 56);
        assert_eq!(box_.rect_height, 24);

        let lines = vec![("A".to_string(), 10.0), ("B".to_string(), 20.0)];
        box_.set_text_lines(lines);
        assert_eq!(box_.content.len(), 2);
        assert_eq!(box_.rect_width, 26);
        assert_eq!(box_.rect_height, 46);
    }

    #[test]
    fn textbox_position_and_adjust() {
        let mut box_ = TextBox::new();
        box_.font_size = 18.0;
        box_.set_text("Tooltip text", 100.0); // clear sets rect_height = 2, + 22 = 24. rect_width = 106.

        // Without adjust: corner = pos exactly
        box_.set_pos(Vec2::new(50.0, 50.0), false, 800.0, 600.0);
        assert_eq!(box_.corner, Vec2::new(50.0, 50.0));

        // With adjust: standard case where pos is inside screen
        // corner.y = 50.0 - 24 = 26.0
        box_.set_pos(Vec2::new(50.0, 50.0), true, 800.0, 600.0);
        assert_eq!(box_.corner, Vec2::new(50.0, 26.0));

        // Clamping right edge:
        // corner.x = 750, rect_width = 106 -> corner.x + rect_width = 856 > 800.
        // fontheight = 22. corner.x - rect_width - fontheight = 750 - 106 - 22 = 622 > 0.
        // So corner.x -= 106 -> 644.
        box_.set_pos(Vec2::new(750.0, 100.0), true, 800.0, 600.0);
        assert_eq!(box_.corner.x, 644.0);

        // Clamping right edge when not enough room to flip left:
        // corner.x = 100, display_width = 150, rect_width = 106.
        // 100 + 106 = 206 > 150.
        // 100 - 106 - 22 = -28 <= 0 -> corner.x = 150 - 106 = 44.
        box_.set_pos(Vec2::new(100.0, 100.0), true, 150.0, 600.0);
        assert_eq!(box_.corner.x, 44.0);

        // Clamping top edge:
        // pos.y = 10 -> corner.y -= 24 -> -14 < 0.
        // corner.y += rect_height + fontheight = -14 + 24 + 22 = 32.
        box_.set_pos(Vec2::new(50.0, 10.0), true, 800.0, 600.0);
        assert_eq!(box_.corner.y, 32.0);

        // Clamping bottom edge:
        // pos.y = 600 -> corner.y = 600 - 24 = 576.
        // corner.y + rect_height = 576 + 24 = 600 <= 600 (ok).
        // If pos.y = 620 -> corner.y = 620 - 24 = 596.
        // 596 + 24 = 620 > 600 -> corner.y -= 24 = 572.
        box_.set_pos(Vec2::new(50.0, 620.0), true, 800.0, 600.0);
        assert_eq!(box_.corner.y, 572.0);
    }

    #[test]
    fn textbox_draw_geometry() {
        let mut box_ = TextBox::with_font(FontId(1), 18.0);
        box_.set_text("Line 1", 50.0);
        box_.show();

        let mut list = DrawList::new(UVec2::new(800, 600));
        let mut gfx = Gfx {
            textures: gource_draw::TextureStore::default(),
            fonts: gource_draw::FontStore::default(),
        };

        box_.draw(&mut gfx, &mut list);

        // Invisible when hidden
        assert!(!list.is_empty());
        // Shadow quad (4 verts) + Background quad (4 verts) = 8 solid rect verts
        assert_eq!(list.batches.len(), 1);
        assert_eq!(list.batches[0].vertices.len(), 8);

        // When hidden
        box_.hide();
        list.reset(UVec2::new(800, 600), Vec4::ZERO);
        box_.draw(&mut gfx, &mut list);
        assert!(list.is_empty());
    }

    #[test]
    fn textbox_max_width_chars() {
        let mut box_ = TextBox::new();
        box_.max_width_chars = 5;
        box_.add_line("123456789".to_string(), 100.0);
        assert_eq!(box_.content[0], "12345");
    }

    #[test]
    fn textbox_styling_properties() {
        let mut box_ = TextBox::new();
        box_.set_colour(Vec3::new(0.2, 0.4, 0.6));
        box_.set_alpha(0.8);
        box_.set_brightness(1.5);
        assert_eq!(box_.colour, Vec3::new(0.2, 0.4, 0.6));
        assert_eq!(box_.alpha, 0.8);
        assert_eq!(box_.brightness, 1.5);
    }

    #[test]
    fn textbox_draw_with_real_font() {
        let mut gfx = Gfx::new();
        let face = gfx.fonts.default_face();
        let font = gfx.fonts.font(face, 18);

        let mut box_ = TextBox::with_font(font, 18.0);
        let w = gfx.text_width(font, "Hello Tooltip");
        box_.set_text("Hello Tooltip", w);
        box_.show();

        let mut list = DrawList::new(UVec2::new(800, 600));
        box_.draw(&mut gfx, &mut list);
        assert!(!list.is_empty());

        // Zero alpha should skip draw
        box_.set_alpha(0.0);
        let mut list2 = DrawList::new(UVec2::new(800, 600));
        box_.draw(&mut gfx, &mut list2);
        assert!(list2.is_empty());
    }

    #[test]
    fn textbox_unicode_truncation() {
        let mut box_ = TextBox::new();
        box_.max_width_chars = 3; // "🦀" is 4 bytes
        box_.add_line("🦀🦀🦀".to_string(), 100.0);
        // Truncate to <= 3 bytes without panicking at char boundary
        assert!(box_.content[0].len() <= 3);
    }

    #[test]
    fn textbox_default_and_set_font_and_visibility() {
        let mut box_ = TextBox::default();
        assert!(!box_.is_visible());
        box_.show();
        assert!(box_.is_visible());

        box_.set_font(FontId(5), 24.0);
        assert_eq!(box_.font, Some(FontId(5)));
        assert_eq!(box_.font_size, 24.0);
    }

    #[test]
    fn textbox_exact_draw_geometry() {
        let mut box_ = TextBox::new();
        box_.set_colour(Vec3::new(0.6, 0.7, 0.8));
        box_.set_brightness(0.5);
        box_.set_alpha(0.9);
        box_.set_pos(Vec2::new(100.0, 200.0), false, 800.0, 600.0);
        box_.add_line("Hello".to_string(), 50.0); // rect_width = 56, rect_height = 0 + 22 = 22
        box_.show();

        let mut list = DrawList::new(UVec2::new(800, 600));
        let mut gfx = Gfx {
            textures: gource_draw::TextureStore::default(),
            fonts: gource_draw::FontStore::default(),
        };

        box_.draw(&mut gfx, &mut list);

        // Batches: exactly 1 batch (TextureId::WHITE) since font is None
        assert_eq!(list.batches.len(), 1);
        let b = &list.batches[0];
        assert_eq!(b.texture, TextureId::WHITE);
        assert_eq!(b.vertices.len(), 8);

        // Shadow quad: pos = corner + shadow = (100 + 3, 200 + 3) = (103, 203), size (56, 22)
        // colour = (0, 0, 0, 0.9 * 0.333) = (0, 0, 0, 0.2997)
        assert_eq!(b.vertices[0].pos, Vec2::new(103.0, 203.0));
        assert!((b.vertices[0].colour.w - 0.9 * 0.333).abs() < 1e-5);

        // Background quad: pos = corner = (100, 200), size (56, 22)
        // colour = colour * brightness = (0.6 * 0.5, 0.7 * 0.5, 0.8 * 0.5, 0.9) = (0.3, 0.35, 0.4, 0.9)
        assert_eq!(b.vertices[4].pos, Vec2::new(100.0, 200.0));
        assert!((b.vertices[4].colour.x - 0.3).abs() < 1e-5);
        assert!((b.vertices[4].colour.y - 0.35).abs() < 1e-5);
        assert!((b.vertices[4].colour.z - 0.4).abs() < 1e-5);
        assert_eq!(b.vertices[4].colour.w, 0.9);

        // Test with font set to ensure font.draw offsets (x + 2, y + 3) are hit
        let face = gfx.fonts.default_face();
        let font = gfx.fonts.font(face, 18);
        box_.set_font(font, 18.0);
        let mut list_font = DrawList::new(UVec2::new(800, 600));
        box_.draw(&mut gfx, &mut list_font);
        // With font, there will be textured quad batches added for text
        assert!(list_font.vertex_count() > 8);

        // Hidden textbox should not draw
        box_.hide();
        let mut list_hidden = DrawList::new(UVec2::new(800, 600));
        box_.draw(&mut gfx, &mut list_hidden);
        assert!(list_hidden.is_empty());
    }
}
