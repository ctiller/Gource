//! FileKey: the file extension legend (port of `key.h`, `key.cpp`).
//!
//! Maintains a legend of file types with per-extension counts, colours, sorting,
//! sliding/fade transitions, and interval-based periodic layout updates.

use glam::{Vec2, Vec3, Vec4};
use gource_core::StringHasher;
use gource_draw::font::TextStyle;
use gource_draw::list::QUAD_UVS;
use gource_draw::{DrawList, FontId, Gfx, TextureId};
use std::collections::BTreeMap;

/// Default shadow offset for key entries (vec2(3.0, 3.0)).
pub const DEFAULT_KEY_SHADOW: Vec2 = Vec2::new(3.0, 3.0);

/// An entry in the file extension key.
///
/// Port of C++ `FileKeyEntry`.
#[derive(Debug, Clone, PartialEq)]
pub struct FileKeyEntry {
    pub ext: String,
    pub display_ext: String,
    pub colour: Vec3,
    pub count: i32,
    pub alpha: f32,
    pub brightness: f32,
    pub pos: Vec2,
    pub pos_y: f32,
    pub src_y: f32,
    pub dest_y: f32,
    pub move_elapsed: f32,
    pub left_margin: f32,
    pub width: f32,
    pub height: f32,
    pub shadow: Vec2,
    pub show: bool,
}

impl FileKeyEntry {
    /// Create a new `FileKeyEntry`.
    ///
    /// Port of `FileKeyEntry::FileKeyEntry(const FXFont& font, const std::string& ext, const vec3& colour)`.
    ///
    /// In C++:
    /// - `width = 90.0f * font_scale`
    /// - `height = scaled_font_size + 4.0f`
    /// - `left_margin = scaled_font_size + 4.0f`
    /// - `display_ext` truncated if width > `width - 15.0f * font_scale`
    pub fn new(
        ext: impl Into<String>,
        colour: Vec3,
        scaled_font_size: f32,
        font_scale: f32,
        text_width_fn: impl Fn(&str) -> f32,
    ) -> Self {
        let ext = ext.into();
        let width = 90.0 * font_scale;
        let height = scaled_font_size + 4.0;
        let left_margin = scaled_font_size + 4.0;
        let shadow = DEFAULT_KEY_SHADOW;

        let max_text_width = width - 15.0 * font_scale;
        let mut display_ext = ext.clone();
        let mut truncated = false;

        while text_width_fn(&display_ext) > max_text_width && !display_ext.is_empty() {
            display_ext.pop();
            truncated = true;
        }

        if truncated {
            display_ext.push_str("...");
        }

        Self {
            ext,
            display_ext,
            colour,
            count: 0,
            alpha: 0.0,
            brightness: 1.0,
            pos: Vec2::ZERO,
            pos_y: -1.0,
            src_y: -1.0,
            dest_y: -1.0,
            move_elapsed: 1.0,
            left_margin,
            width,
            height,
            shadow,
            show: true,
        }
    }

    /// Colorize the entry using a `StringHasher` (or white for empty extension).
    ///
    /// Port of `FileKeyEntry::colourize()`.
    pub fn colourize(&mut self, hasher: &StringHasher) {
        self.colour = if self.ext.is_empty() {
            Vec3::ONE
        } else {
            hasher.colour_hash(&self.ext)
        };
    }

    /// Set entry destination y coordinate.
    ///
    /// Port of `FileKeyEntry::setDestY(float dest_y)`.
    pub fn set_dest_y(&mut self, dest_y: f32) {
        if dest_y == self.dest_y {
            return;
        }
        self.dest_y = dest_y;
        self.src_y = self.pos_y;
        self.move_elapsed = 0.0;
    }

    /// Increment entry count.
    ///
    /// Port of `FileKeyEntry::inc()`.
    pub fn inc(&mut self) {
        self.count += 1;
    }

    /// Decrement entry count.
    ///
    /// Port of `FileKeyEntry::dec()`.
    pub fn dec(&mut self) {
        self.count -= 1;
    }

    /// Set visibility flag.
    ///
    /// Port of `FileKeyEntry::setShow(bool show)`.
    pub fn set_show(&mut self, show: bool) {
        self.show = show;
    }

    /// Get count.
    ///
    /// Port of `FileKeyEntry::getCount()`.
    pub fn count(&self) -> i32 {
        self.count
    }

    /// Set count.
    ///
    /// Port of `FileKeyEntry::setCount(int count)`.
    pub fn set_count(&mut self, count: i32) {
        self.count = count;
    }

    /// Whether entry is finished and can be removed (`count <= 0 && alpha <= 0.0`).
    ///
    /// Port of `FileKeyEntry::isFinished()`.
    pub fn is_finished(&self) -> bool {
        self.count <= 0 && self.alpha <= 0.0
    }

    /// Get extension string.
    ///
    /// Port of `FileKeyEntry::getExt()`.
    pub fn ext(&self) -> &str {
        &self.ext
    }

    /// Get colour.
    ///
    /// Port of `FileKeyEntry::getColour()`.
    pub fn colour(&self) -> Vec3 {
        self.colour
    }

    /// Advance entry logic and sliding animation.
    ///
    /// Port of `FileKeyEntry::logic(float dt)`.
    pub fn logic(&mut self, dt: f32) {
        if self.count <= 0 || !self.show {
            self.alpha = 0.0f32.max(self.alpha - dt);
        } else if self.alpha < 1.0 {
            self.alpha = 1.0f32.min(self.alpha + dt);
        }

        // Move towards dest
        if self.pos_y != self.dest_y {
            if self.pos_y < 0.0 {
                self.pos_y = self.dest_y;
            } else {
                self.move_elapsed += dt;
                if self.move_elapsed >= 1.0 {
                    self.pos_y = self.dest_y;
                } else {
                    self.pos_y = self.src_y + (self.dest_y - self.src_y) * self.move_elapsed;
                }
            }
        }

        self.pos = Vec2::new(self.alpha * self.left_margin, self.pos_y);
    }

    /// Draw entry into `DrawList`.
    ///
    /// Port of `FileKeyEntry::draw()`.
    ///
    /// In C++:
    /// 1. Shadow quad at `pos + shadow`, size `(width, height)`, colour `(0, 0, 0, alpha * 0.333f)`.
    /// 2. Gradient background quad:
    ///    - left edge vertices (top-left, bottom-left): `colour * 0.5f, alpha`
    ///    - right edge vertices (bottom-right, top-right): `colour, alpha`
    /// 3. Extension text: at `((int)pos.x + 2, (int)pos.y + 3)`, colour `(1, 1, 1, alpha)`, no shadow.
    /// 4. Count text: at `((int)pos.x + width + 4, (int)pos.y + 3)`, colour `(1, 1, 1, alpha)`, with drop shadow.
    pub fn draw(&self, font: FontId, gfx: &mut Gfx, list: &mut DrawList) {
        if self.is_finished() || self.alpha <= 0.0 {
            return;
        }

        let size = Vec2::new(self.width, self.height);

        // 1. Shadow quad
        let shadow_colour = Vec4::new(0.0, 0.0, 0.0, self.alpha * 0.333);
        list.solid_rect(self.pos + self.shadow, size, shadow_colour);

        // 2. Coloured quad with horizontal gradient:
        // Left vertices = colour * 0.5, Right vertices = colour
        let c_left = Vec4::new(
            self.colour.x * 0.5,
            self.colour.y * 0.5,
            self.colour.z * 0.5,
            self.alpha,
        );
        let c_right = Vec4::new(self.colour.x, self.colour.y, self.colour.z, self.alpha);
        let corners = [
            self.pos,
            Vec2::new(self.pos.x, self.pos.y + self.height),
            Vec2::new(self.pos.x + self.width, self.pos.y + self.height),
            Vec2::new(self.pos.x + self.width, self.pos.y),
        ];
        let colours = [c_left, c_left, c_right, c_right];
        list.quad_colours(TextureId::WHITE, corners, QUAD_UVS, colours);

        // 3. Extension text (no shadow)
        let text_style = TextStyle::new(Vec4::new(1.0, 1.0, 1.0, self.alpha)).with_shadow(false);
        let ext_pos = Vec2::new(
            (self.pos.x as i32 + 2) as f32,
            (self.pos.y as i32 + 3) as f32,
        );
        gfx.draw_text(list, font, ext_pos, &self.display_ext, &text_style);

        // 4. Count text (with shadow)
        let count_style = TextStyle::new(Vec4::new(1.0, 1.0, 1.0, self.alpha)).with_shadow(true);
        let count_str = self.count.to_string();
        let count_pos = Vec2::new(
            (self.pos.x as i32 + self.width as i32 + 4) as f32,
            (self.pos.y as i32 + 3) as f32,
        );
        gfx.draw_text(list, font, count_pos, &count_str, &count_style);
    }
}

/// Comparison function matching C++ `file_key_entry_sort`.
///
/// In C++:
/// ```cpp
/// if(a->getCount() != b->getCount()) return a->getCount() > b->getCount();
/// return a->getExt().compare(b->getExt()) < 0;
/// ```
pub fn file_key_entry_cmp(a: &FileKeyEntry, b: &FileKeyEntry) -> std::cmp::Ordering {
    match b.count.cmp(&a.count) {
        std::cmp::Ordering::Equal => a.ext.cmp(&b.ext),
        other => other,
    }
}

/// File extension key legend manager.
///
/// Port of C++ `FileKey`.
#[derive(Debug, Clone, PartialEq)]
pub struct FileKey {
    pub keymap: BTreeMap<String, FileKeyEntry>,
    pub active_keys: Vec<String>,
    pub font: Option<FontId>,
    pub scaled_font_size: f32,
    pub font_scale: f32,
    pub update_interval: f32,
    pub interval_remaining: f32,
    pub show: bool,
}

impl Default for FileKey {
    fn default() -> Self {
        Self::new(1.0)
    }
}

impl FileKey {
    /// Create a new `FileKey` with a periodic update interval (in seconds).
    ///
    /// Port of `FileKey::FileKey(float update_interval)`.
    pub fn new(update_interval: f32) -> Self {
        Self {
            keymap: BTreeMap::new(),
            active_keys: Vec::new(),
            font: None,
            scaled_font_size: 16.0,
            font_scale: 1.0,
            update_interval,
            interval_remaining: 1.0,
            show: true,
        }
    }

    /// Configure font configuration.
    pub fn set_font(&mut self, font: FontId, scaled_font_size: f32, font_scale: f32) {
        self.font = Some(font);
        self.scaled_font_size = scaled_font_size;
        self.font_scale = font_scale;
    }

    /// Set visibility flag.
    ///
    /// Port of `FileKey::setShow(bool show)`.
    pub fn set_show(&mut self, show: bool) {
        self.show = show;
        for entry in self.keymap.values_mut() {
            entry.set_show(show);
        }
        self.interval_remaining = 0.0;
    }

    /// Recompute colours for all entries using the provided hasher.
    ///
    /// Port of `FileKey::colourize()`.
    pub fn colourize(&mut self, hasher: &StringHasher) {
        for entry in self.keymap.values_mut() {
            entry.colourize(hasher);
        }
    }

    /// Clear all file counts to 0 and trigger immediate refresh.
    ///
    /// Port of `FileKey::clear()`.
    pub fn clear(&mut self) {
        for entry in self.keymap.values_mut() {
            entry.set_count(0);
        }
        self.interval_remaining = 0.0;
    }

    /// Increment count for an extension, creating the entry if necessary.
    ///
    /// Port of `FileKey::inc(RFile* file)`.
    pub fn inc(&mut self, ext: &str, colour: Vec3, text_width_fn: impl Fn(&str) -> f32) {
        let scaled_font_size = self.scaled_font_size;
        let font_scale = self.font_scale;
        let entry = self.keymap.entry(ext.to_string()).or_insert_with(|| {
            FileKeyEntry::new(ext, colour, scaled_font_size, font_scale, text_width_fn)
        });
        entry.inc();
    }

    /// Increment with automatic colour lookup from `StringHasher`.
    pub fn inc_hashed(
        &mut self,
        ext: &str,
        hasher: &StringHasher,
        text_width_fn: impl Fn(&str) -> f32,
    ) {
        let colour = if ext.is_empty() {
            Vec3::ONE
        } else {
            hasher.colour_hash(ext)
        };
        self.inc(ext, colour, text_width_fn);
    }

    /// Decrement count for an extension.
    ///
    /// Port of `FileKey::dec(RFile* file)`.
    pub fn dec(&mut self, ext: &str) {
        if let Some(entry) = self.keymap.get_mut(ext) {
            entry.dec();
        }
    }

    /// Advance time, update active keys, sort entries, and place them on screen.
    ///
    /// Port of `FileKey::logic(float dt)`.
    pub fn logic(&mut self, dt: f32, display_height: f32) {
        self.interval_remaining -= dt;

        if self.interval_remaining <= 0.0 {
            if self.show {
                self.active_keys.clear();
                let mut finished_keys = Vec::new();

                for (ext, entry) in &self.keymap {
                    if !entry.is_finished() {
                        self.active_keys.push(ext.clone());
                    } else {
                        finished_keys.push(ext.clone());
                    }
                }

                // Sort active keys using C++ file_key_entry_sort
                let keymap = &self.keymap;
                self.active_keys.sort_by(|a, b| {
                    let ea = &keymap[a];
                    let eb = &keymap[b];
                    file_key_entry_cmp(ea, eb)
                });

                // Limit to entries that can fit on screen:
                // int max_visible_entries = std::max(0, (int)((display.height - 150.0f) / 20.0f));
                let max_visible = ((display_height - 150.0) / 20.0).max(0.0) as usize;
                if self.active_keys.len() > max_visible {
                    self.active_keys.truncate(max_visible);
                }

                // Set destination y positions:
                // float offset_y = gGourceSettings.scaled_font_size + 6.0f;
                let offset_y = self.scaled_font_size + 6.0;
                let mut key_y = offset_y;

                for ext in &self.active_keys {
                    if let Some(entry) = self.keymap.get_mut(ext) {
                        if entry.count() > 0 {
                            entry.set_dest_y(key_y);
                        }
                        key_y += offset_y;
                    }
                }

                // Remove finished entries
                for ext in finished_keys {
                    self.keymap.remove(&ext);
                }
            }

            self.interval_remaining = self.update_interval;
        }

        // Step logic on active entries
        for ext in &self.active_keys {
            if let Some(entry) = self.keymap.get_mut(ext) {
                entry.logic(dt);
            }
        }
    }

    /// Draw all active entries.
    ///
    /// Port of `FileKey::draw()`.
    pub fn draw(&self, gfx: &mut Gfx, list: &mut DrawList) {
        if let Some(font) = self.font {
            for ext in &self.active_keys {
                if let Some(entry) = self.keymap.get(ext) {
                    entry.draw(font, gfx, list);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::UVec2;

    fn mock_text_width(s: &str) -> f32 {
        s.len() as f32 * 8.0
    }

    #[test]
    fn file_key_entry_creation_and_truncation() {
        // width = 90 * 1.0 = 90. max_text_width = 90 - 15 = 75.
        // mock_text_width: 8px per char. 75 / 8 = 9.375 chars max.
        let entry = FileKeyEntry::new("cpp", Vec3::ONE, 16.0, 1.0, mock_text_width);
        assert_eq!(entry.ext(), "cpp");
        assert_eq!(entry.display_ext, "cpp");
        assert_eq!(entry.width, 90.0);
        assert_eq!(entry.height, 20.0);
        assert_eq!(entry.left_margin, 20.0);

        let long_entry = FileKeyEntry::new(
            "a_very_long_file_extension",
            Vec3::ONE,
            16.0,
            1.0,
            mock_text_width,
        );
        assert_eq!(long_entry.ext(), "a_very_long_file_extension");
        assert!(long_entry.display_ext.ends_with("..."));
    }

    #[test]
    fn file_key_entry_counting_and_sliding() {
        let mut entry = FileKeyEntry::new("rs", Vec3::ONE, 16.0, 1.0, mock_text_width);
        assert_eq!(entry.count(), 0);
        assert!(entry.is_finished());

        entry.inc();
        assert_eq!(entry.count(), 1);
        assert!(!entry.is_finished());

        entry.set_dest_y(50.0);
        assert_eq!(entry.dest_y, 50.0);
        assert_eq!(entry.move_elapsed, 0.0);

        // Calling set_dest_y again with same value is no-op
        entry.set_dest_y(50.0);
        assert_eq!(entry.dest_y, 50.0);

        // Logic advances: pos_y was -1.0, so first logic initializes pos_y to dest_y
        entry.logic(0.5);
        assert_eq!(entry.pos_y, 50.0);
        assert_eq!(entry.alpha, 0.5);
        assert_eq!(entry.pos.x, 0.5 * entry.left_margin);

        // Move to a new dest_y
        entry.set_dest_y(100.0);
        assert_eq!(entry.src_y, 50.0);
        assert_eq!(entry.dest_y, 100.0);

        entry.logic(0.5);
        assert_eq!(entry.pos_y, 75.0); // 50 + (100 - 50) * 0.5

        entry.logic(0.6); // move_elapsed >= 1.0 -> pos_y = 100.0
        assert_eq!(entry.pos_y, 100.0);

        entry.dec();
        assert_eq!(entry.count(), 0);
        // alpha fades out
        entry.logic(0.5);
        assert_eq!(entry.alpha, 0.5);
        entry.logic(0.5);
        assert_eq!(entry.alpha, 0.0);
        assert!(entry.is_finished());
    }

    #[test]
    fn file_key_sorting() {
        let e1 = FileKeyEntry::new("cpp", Vec3::ONE, 16.0, 1.0, mock_text_width);
        let mut e2 = FileKeyEntry::new("rs", Vec3::ONE, 16.0, 1.0, mock_text_width);
        let mut e3 = FileKeyEntry::new("c", Vec3::ONE, 16.0, 1.0, mock_text_width);

        e2.set_count(10);
        e3.set_count(10);
        // e2 and e3 both count 10 -> tiebreaker is extension alphabetical ("c" < "rs")
        assert_eq!(file_key_entry_cmp(&e3, &e2), std::cmp::Ordering::Less);
        // e2 (count 10) vs e1 (count 0) -> e2 comes before e1 (greater count)
        assert_eq!(file_key_entry_cmp(&e2, &e1), std::cmp::Ordering::Less);
    }

    #[test]
    fn file_key_manager_lifecycle() {
        let mut key = FileKey::new(1.0);
        key.set_font(FontId(1), 16.0, 1.0);

        let hasher = StringHasher::new(31);
        key.inc_hashed("rs", &hasher, mock_text_width);
        key.inc_hashed("rs", &hasher, mock_text_width);
        key.inc_hashed("cpp", &hasher, mock_text_width);

        assert_eq!(key.keymap.len(), 2);
        assert_eq!(key.keymap["rs"].count(), 2);
        assert_eq!(key.keymap["cpp"].count(), 1);

        // Advance logic so update triggers (dt >= interval_remaining = 1.0)
        key.logic(1.0, 600.0);
        assert_eq!(key.active_keys, vec!["rs".to_string(), "cpp".to_string()]);
        // rs dest_y should be offset_y = 16 + 6 = 22.0
        assert_eq!(key.keymap["rs"].dest_y, 22.0);
        // cpp dest_y should be 22.0 + 22.0 = 44.0
        assert_eq!(key.keymap["cpp"].dest_y, 44.0);

        // Colourize recomputes colours
        key.colourize(&hasher);

        // Clear resets counts
        key.clear();
        assert_eq!(key.keymap["rs"].count(), 0);
        assert_eq!(key.keymap["cpp"].count(), 0);

        // setShow
        key.set_show(false);
        assert!(!key.show);
        assert!(!key.keymap["rs"].show);
    }

    #[test]
    fn file_key_draw_batches() {
        let mut key = FileKey::new(1.0);
        let font = FontId(1);
        key.set_font(font, 16.0, 1.0);

        let hasher = StringHasher::new(31);
        key.inc_hashed("rs", &hasher, mock_text_width);

        // Trigger layout & fade in
        key.logic(1.0, 600.0);
        key.logic(1.0, 600.0); // alpha reaches 1.0

        let mut list = DrawList::new(UVec2::new(800, 600));
        let mut gfx = Gfx {
            textures: gource_draw::TextureStore::default(),
            fonts: gource_draw::FontStore::default(),
        };

        key.draw(&mut gfx, &mut list);

        // Draw produces:
        // Shadow quad (solid_rect = 4 verts)
        // Coloured gradient quad (quad_colours = 4 verts)
        assert_eq!(list.batches.len(), 1);
        assert_eq!(list.batches[0].vertices.len(), 8);
    }

    #[test]
    fn file_key_draw_with_real_font() {
        let mut gfx = Gfx::new();
        let face = gfx.fonts.default_face();
        let font = gfx.fonts.font(face, 16);

        let mut key = FileKey::new(1.0);
        key.set_font(font, 16.0, 1.0);
        let hasher = StringHasher::new(31);
        let w_fn = |s: &str| 8.0 * s.len() as f32;

        key.inc_hashed("rs", &hasher, w_fn);
        key.inc_hashed("cpp", &hasher, w_fn);
        key.logic(1.0, 600.0);
        key.logic(1.0, 600.0);

        let mut list = DrawList::new(UVec2::new(800, 600));
        key.draw(&mut gfx, &mut list);
        assert!(!list.is_empty());

        // Finished entries should be removed
        key.dec("rs");
        key.dec("cpp");
        // Unknown extension dec should not panic
        key.dec("unknown");
        key.logic(1.0, 600.0); // alpha drops to 0
        key.logic(1.0, 600.0); // finished_keys removed from keymap
        assert!(key.keymap.is_empty());
    }

    #[test]
    fn file_key_max_visible_entries() {
        let mut key = FileKey::new(1.0);
        let hasher = StringHasher::new(31);
        let w_fn = |s: &str| 8.0 * s.len() as f32;

        for i in 0..20 {
            let ext = format!("ext{}", i);
            key.inc_hashed(&ext, &hasher, w_fn);
        }

        // display_height = 250 -> max_visible = (250 - 150) / 20 = 5
        key.logic(1.0, 250.0);
        assert_eq!(key.active_keys.len(), 5);

        // When show = false, active_keys is not updated in that branch
        key.set_show(false);
        key.logic(1.0, 600.0);
        assert!(!key.show);
    }

    #[test]
    fn file_key_default_and_empty_ext() {
        let key_default = FileKey::default();
        assert_eq!(key_default.update_interval, 1.0);

        let hasher = StringHasher::new(31);
        let mut entry_empty = FileKeyEntry::new("", Vec3::ZERO, 16.0, 1.0, mock_text_width);
        entry_empty.colourize(&hasher);
        // C++: ext.empty() ? vec3(1.0f, 1.0f, 1.0f) : colourHash(ext)
        assert_eq!(entry_empty.colour(), Vec3::ONE);

        let mut key = FileKey::new(1.0);
        key.inc_hashed("", &hasher, mock_text_width);
        assert_eq!(key.keymap[""].colour(), Vec3::ONE);
    }

    #[test]
    fn file_key_exact_draw_geometry() {
        let mut gfx = Gfx::new();
        let face = gfx.fonts.default_face();
        let font = gfx.fonts.font(face, 16);

        let mut entry = FileKeyEntry::new("cpp", Vec3::new(1.0, 0.5, 0.0), 16.0, 1.0, |s| {
            s.len() as f32 * 8.0
        });
        entry.inc();
        entry.set_dest_y(22.0);
        entry.logic(1.0); // alpha = 1.0, pos_y = 22.0, pos.x = 20.0

        assert_eq!(entry.pos, Vec2::new(20.0, 22.0));
        assert_eq!(entry.width, 90.0);
        assert_eq!(entry.height, 20.0);

        let mut list = DrawList::new(UVec2::new(800, 600));
        entry.draw(font, &mut gfx, &mut list);

        // Batches:
        // Batch 0: TextureId::WHITE (Solid shadow rect + horizontal gradient quad)
        assert_eq!(list.batches.len(), 2);
        let b0 = &list.batches[0];
        assert_eq!(b0.texture, TextureId::WHITE);
        // Shadow quad: at pos + shadow = (20 + 3, 22 + 3) = (23, 25), size (90, 20)
        assert_eq!(b0.vertices[0].pos, Vec2::new(23.0, 25.0));
        assert_eq!(b0.vertices[0].colour, Vec4::new(0.0, 0.0, 0.0, 0.333));
        assert_eq!(b0.vertices[1].pos, Vec2::new(23.0 + 90.0, 25.0));
        assert_eq!(b0.vertices[2].pos, Vec2::new(23.0 + 90.0, 25.0 + 20.0));
        assert_eq!(b0.vertices[3].pos, Vec2::new(23.0, 25.0 + 20.0));

        // Coloured gradient quad: corners pos = (20, 22), (20, 42), (110, 42), (110, 22)
        // Left vertices (idx 4, 5): colour * 0.5 = (0.5, 0.25, 0.0, 1.0)
        // Right vertices (idx 6, 7): colour = (1.0, 0.5, 0.0, 1.0)
        assert_eq!(b0.vertices[4].pos, Vec2::new(20.0, 22.0));
        assert_eq!(b0.vertices[4].colour, Vec4::new(0.5, 0.25, 0.0, 1.0));
        assert_eq!(b0.vertices[5].pos, Vec2::new(20.0, 42.0));
        assert_eq!(b0.vertices[5].colour, Vec4::new(0.5, 0.25, 0.0, 1.0));
        assert_eq!(b0.vertices[6].pos, Vec2::new(110.0, 42.0));
        assert_eq!(b0.vertices[6].colour, Vec4::new(1.0, 0.5, 0.0, 1.0));
        assert_eq!(b0.vertices[7].pos, Vec2::new(110.0, 22.0));
        assert_eq!(b0.vertices[7].colour, Vec4::new(1.0, 0.5, 0.0, 1.0));

        // When alpha <= 0 or is_finished, entry.draw does nothing
        entry.dec();
        entry.logic(1.0); // alpha drops to 0.0
        let mut list2 = DrawList::new(UVec2::new(800, 600));
        entry.draw(font, &mut gfx, &mut list2);
        assert!(list2.is_empty());
    }
}
