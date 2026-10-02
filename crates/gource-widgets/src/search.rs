//! Interactive HUD search widget for files, directories, and users.
//!
//! Provides a real-time auto-complete search bar activated by `/` or `Ctrl+F`.
//! Allows searching active repo files, directories, and authors with keyboard navigation,
//! highlighting matching items, and focusing/tracking camera on selection.

use gource_core::bounds::Bounds2D;
use gource_core::{Vec2, Vec4};
use gource_draw::font::TextStyle;
use gource_draw::{DrawList, FontId, Gfx};

/// Width of the search widget bar before font scaling.
pub const SEARCH_WIDGET_WIDTH: f32 = 460.0;

/// Default height of the search input bar.
pub const SEARCH_INPUT_HEIGHT: f32 = 36.0;

/// Height of each search result item row.
pub const SEARCH_ROW_HEIGHT: f32 = 26.0;

/// Maximum number of search results to display at once.
pub const MAX_DISPLAY_RESULTS: usize = 8;

/// Fade in/out duration in seconds.
pub const SEARCH_FADE_TIME: f32 = 0.15;

pub use gource_vm::search::{SearchItem, SearchItemKind};

/// Interactive Search Widget.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchWidget {
    pub bounds: Bounds2D,
    pub font: FontId,
    pub font_scale: f32,
    pub visible: bool,
    pub alpha: f32,
    pub fade_time: f32,
    pub query: String,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub results: Vec<SearchItem>,
    pub cursor_blink: f32,
}

impl SearchWidget {
    /// Create a new SearchWidget.
    pub fn new(font: FontId, font_scale: f32) -> Self {
        Self {
            bounds: Bounds2D::new(),
            font,
            font_scale: font_scale.max(0.1),
            visible: false,
            alpha: 0.0,
            fade_time: SEARCH_FADE_TIME,
            query: String::new(),
            selected_index: 0,
            scroll_offset: 0,
            results: Vec::new(),
            cursor_blink: 0.0,
        }
    }

    /// Resize search widget centered horizontally near the top of viewport.
    pub fn resize(&mut self, viewport_width: u32, _viewport_height: u32, font_scale: f32) {
        self.font_scale = font_scale.max(0.1);
        let w = (SEARCH_WIDGET_WIDTH * self.font_scale).min(viewport_width as f32 - 32.0);
        let x = ((viewport_width as f32) - w) * 0.5;
        let y = 48.0 * self.font_scale;
        let h = SEARCH_INPUT_HEIGHT * self.font_scale;
        self.bounds
            .set_points(Vec2::new(x, y), Vec2::new(x + w, y + h));
    }

    /// Show the search widget and focus input.
    pub fn show(&mut self) {
        self.visible = true;
        self.cursor_blink = 0.0;
    }

    /// Hide the search widget and clear input.
    pub fn hide(&mut self) {
        self.visible = false;
        self.query.clear();
        self.results.clear();
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    /// Toggle visibility.
    pub fn toggle(&mut self) {
        if self.visible {
            self.hide();
        } else {
            self.show();
        }
    }

    /// Whether the widget is currently active/visible.
    pub fn is_active(&self) -> bool {
        self.visible || self.alpha > 0.01
    }

    /// Advance fade animation and cursor blink.
    pub fn logic(&mut self, dt: f32) {
        let target = if self.visible { 1.0 } else { 0.0 };
        let step = (dt / self.fade_time.max(0.001)).clamp(0.0, 1.0);
        if self.alpha < target {
            self.alpha = (self.alpha + step).min(target);
        } else if self.alpha > target {
            self.alpha = (self.alpha - step).max(target);
        }

        self.cursor_blink = (self.cursor_blink + dt * 2.5) % 2.0;
    }

    /// Ensure the selected item is within the visible scrolled window.
    fn update_scroll(&mut self) {
        if self.results.is_empty() {
            self.scroll_offset = 0;
            return;
        }
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + MAX_DISPLAY_RESULTS {
            self.scroll_offset = self.selected_index - MAX_DISPLAY_RESULTS + 1;
        }
    }

    /// Push a character to query.
    pub fn push_char(&mut self, c: char) {
        self.query.push(c);
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.cursor_blink = 0.0;
    }

    /// Delete character backwards.
    pub fn backspace(&mut self) {
        self.query.pop();
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.cursor_blink = 0.0;
    }

    /// Move selection up.
    pub fn select_prev(&mut self) {
        if !self.results.is_empty() {
            if self.selected_index == 0 {
                self.selected_index = self.results.len() - 1;
            } else {
                self.selected_index -= 1;
            }
            self.update_scroll();
        }
    }

    /// Move selection down.
    pub fn select_next(&mut self) {
        if !self.results.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.results.len();
            self.update_scroll();
        }
    }

    /// Get the currently highlighted search result.
    pub fn selected_result(&self) -> Option<&SearchItem> {
        self.results.get(self.selected_index)
    }

    /// Update search results given candidate files, dirs, and users.
    pub fn update_filter<'a, F, D, U>(&mut self, files: F, dirs: D, users: U)
    where
        F: IntoIterator<Item = (u64, &'a str, &'a str)>,
        D: IntoIterator<Item = (u64, &'a str)>,
        U: IntoIterator<Item = (u64, &'a str)>,
    {
        self.results.clear();
        let q = self.query.trim().to_lowercase();
        if q.is_empty() {
            return;
        }

        // Search Users
        for (id, name) in users {
            if name.to_lowercase().contains(&q) {
                self.results.push(SearchItem::new_user(id, name));
                if self.results.len() >= MAX_DISPLAY_RESULTS {
                    return;
                }
            }
        }

        // Search Dirs
        for (id, path) in dirs {
            if path.to_lowercase().contains(&q) {
                self.results.push(SearchItem::new_dir(id, path));
                if self.results.len() >= MAX_DISPLAY_RESULTS {
                    return;
                }
            }
        }

        // Search Files
        for (id, name, path) in files {
            if name.to_lowercase().contains(&q) || path.to_lowercase().contains(&q) {
                self.results.push(SearchItem::new_file(id, name, path));
                if self.results.len() >= MAX_DISPLAY_RESULTS {
                    return;
                }
            }
        }

        if self.selected_index >= self.results.len() {
            self.selected_index = 0;
        }
    }

    /// Draw the search HUD into `DrawList`.
    pub fn draw(&self, gfx: &mut Gfx, list: &mut DrawList) {
        if self.alpha <= 0.01 {
            return;
        }

        let a = self.alpha;
        let pos = self.bounds.min;
        let w = self.bounds.width();
        let input_h = self.bounds.height();

        let row_h = SEARCH_ROW_HEIGHT * self.font_scale;
        let displayed_count = self.results.len().min(MAX_DISPLAY_RESULTS);
        let total_h = input_h + displayed_count as f32 * row_h;
        let full_size = Vec2::new(w, total_h);

        // 1. Drop Shadow
        list.solid_rect(
            pos + Vec2::new(3.0, 3.0),
            full_size,
            Vec4::new(0.0, 0.0, 0.0, 0.5 * a),
        );

        // 2. Main Background Quad
        let bg_col = Vec4::new(0.08, 0.10, 0.14, 0.95 * a);
        list.solid_rect(pos, full_size, bg_col);

        // 3. Border
        let border_col = Vec4::new(0.30, 0.50, 0.85, 0.8 * a);
        list.rect_outline(pos, pos + full_size, 1.5, border_col);

        // 4. Input Box Divider if results present
        if displayed_count > 0 {
            list.line(
                Vec2::new(pos.x, pos.y + input_h),
                Vec2::new(pos.x + w, pos.y + input_h),
                1.0,
                Vec4::new(0.20, 0.25, 0.35, 0.7 * a),
            );
        }

        // 5. Search Icon / Prompt
        let icon_style = TextStyle::new(Vec4::new(0.5, 0.7, 1.0, a));
        let prompt_pos = Vec2::new(
            pos.x + 10.0 * self.font_scale,
            pos.y + 8.0 * self.font_scale,
        );
        gfx.draw_text(list, self.font, prompt_pos, "🔍", &icon_style);

        // 6. Search Query / Placeholder Text
        let text_x = pos.x + 36.0 * self.font_scale;
        let text_y = pos.y + 8.0 * self.font_scale;
        if self.query.is_empty() {
            let placeholder_style = TextStyle::new(Vec4::new(0.45, 0.50, 0.60, 0.8 * a));
            gfx.draw_text(
                list,
                self.font,
                Vec2::new(text_x, text_y),
                "Search files, directories, or users... (Esc to close)",
                &placeholder_style,
            );
        } else {
            let query_style = TextStyle::new(Vec4::new(0.95, 0.95, 0.95, a));
            gfx.draw_text(
                list,
                self.font,
                Vec2::new(text_x, text_y),
                &self.query,
                &query_style,
            );

            // Cursor
            if self.cursor_blink < 1.0 {
                let text_w = gfx.text_width(self.font, &self.query);
                let cx = text_x + text_w + 2.0;
                list.line(
                    Vec2::new(cx, text_y),
                    Vec2::new(cx, text_y + 18.0 * self.font_scale),
                    1.5,
                    Vec4::new(0.4, 0.7, 1.0, a),
                );
            }
        }

        // 7. Results List
        let mut curr_y = pos.y + input_h;
        let start_idx = self.scroll_offset;
        for (idx, item) in self
            .results
            .iter()
            .enumerate()
            .skip(start_idx)
            .take(MAX_DISPLAY_RESULTS)
        {
            let is_selected = idx == self.selected_index;
            let row_pos = Vec2::new(pos.x, curr_y);
            let row_size = Vec2::new(w, row_h);

            // Row highlight if selected
            if is_selected {
                list.solid_rect(row_pos, row_size, Vec4::new(0.18, 0.32, 0.52, 0.75 * a));
                list.line(
                    row_pos,
                    Vec2::new(row_pos.x, row_pos.y + row_h),
                    3.0,
                    Vec4::new(0.35, 0.75, 1.0, a),
                );
            }

            // Badge
            let badge_bg = item.kind.badge_colour() * Vec4::new(1.0, 1.0, 1.0, 0.85 * a);
            let badge_w = 40.0 * self.font_scale;
            let badge_h = 16.0 * self.font_scale;
            let badge_pos = Vec2::new(
                pos.x + 8.0 * self.font_scale,
                curr_y + 5.0 * self.font_scale,
            );
            list.solid_rect(badge_pos, Vec2::new(badge_w, badge_h), badge_bg);

            let badge_text_style = TextStyle::new(Vec4::new(0.05, 0.05, 0.05, a));
            gfx.draw_text(
                list,
                self.font,
                badge_pos + Vec2::new(3.0 * self.font_scale, 1.0 * self.font_scale),
                item.kind.badge_label(),
                &badge_text_style,
            );

            // Item Name / Path Detail
            let name_style = if is_selected {
                TextStyle::new(Vec4::new(1.0, 1.0, 1.0, a))
            } else {
                TextStyle::new(Vec4::new(0.85, 0.85, 0.85, 0.9 * a))
            };
            let name_pos = Vec2::new(
                pos.x + 56.0 * self.font_scale,
                curr_y + 4.0 * self.font_scale,
            );

            let max_w = w - (68.0 * self.font_scale);
            let mut display_buf = String::new();
            let mut display_text = item.detail.as_str();
            if gfx.text_width(self.font, display_text) > max_w {
                let suffix_w = gfx.text_width(self.font, "...");
                let avail = (max_w - suffix_w).max(0.0);
                for c in display_text.chars() {
                    display_buf.push(c);
                    if gfx.text_width(self.font, &display_buf) > avail {
                        display_buf.pop();
                        break;
                    }
                }
                display_buf.push_str("...");
                display_text = &display_buf;
            }

            gfx.draw_text(list, self.font, name_pos, display_text, &name_style);

            curr_y += row_h;
        }
    }
}
