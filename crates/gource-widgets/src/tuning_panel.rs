//! Interactive tuning side panel widget for live parameter adjustments.
//!
//! Renders an interactive parameter tuning drawer directly into [`DrawList`]
//! using [`Gfx`]. Supports tabs, numeric sliders, boolean toggles, enum cycle
//! buttons, per-row reset, and config saving/copying.

use gource_core::bounds::Bounds2D;
use gource_core::{Vec2, Vec4};
use gource_draw::font::TextStyle;
use gource_draw::{DrawList, FontId, Gfx};

/// Width of the tuning panel drawer before font scaling.
pub const TUNING_PANEL_WIDTH: f32 = 320.0;

/// Default margin from viewport top, bottom, and left edges.
pub const TUNING_PANEL_MARGIN: f32 = 16.0;

/// Fade in/out duration in seconds.
pub const TUNING_PANEL_FADE_TIME: f32 = 0.25;

/// Tabs in the tuning panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TuningTab {
    Visual,
    Dynamics,
    Timeline,
    Structural,
}

impl TuningTab {
    pub const ALL: [TuningTab; 4] = [
        TuningTab::Visual,
        TuningTab::Dynamics,
        TuningTab::Timeline,
        TuningTab::Structural,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Visual => "Visual",
            Self::Dynamics => "Dynamics",
            Self::Timeline => "Timeline",
            Self::Structural => "Structural",
        }
    }

    pub fn badge_colour(&self) -> Vec4 {
        match self {
            Self::Visual => Vec4::new(0.3, 0.7, 1.0, 1.0), // Cyan/Blue
            Self::Dynamics => Vec4::new(0.95, 0.65, 0.2, 1.0), // Orange/Amber
            Self::Timeline => Vec4::new(0.35, 0.85, 0.45, 1.0), // Green
            Self::Structural => Vec4::new(0.85, 0.4, 0.9, 1.0), // Purple/Magenta
        }
    }
}

/// View model for a single setting row.
#[derive(Debug, Clone, PartialEq)]
pub struct TuningRowView {
    pub setting_index: usize,
    pub label: String,
    pub cli_flag: String,
    pub value_text: String,
    pub slider_frac: Option<f32>,
    pub toggle_state: Option<bool>,
    pub is_modified: bool,
}

impl TuningRowView {
    pub fn new_slider(
        setting_index: usize,
        label: impl Into<String>,
        cli_flag: impl Into<String>,
        value_text: impl Into<String>,
        frac: f32,
        is_modified: bool,
    ) -> Self {
        Self {
            setting_index,
            label: label.into(),
            cli_flag: cli_flag.into(),
            value_text: value_text.into(),
            slider_frac: Some(frac.clamp(0.0, 1.0)),
            toggle_state: None,
            is_modified,
        }
    }

    pub fn new_toggle(
        setting_index: usize,
        label: impl Into<String>,
        cli_flag: impl Into<String>,
        value_text: impl Into<String>,
        enabled: bool,
        is_modified: bool,
    ) -> Self {
        Self {
            setting_index,
            label: label.into(),
            cli_flag: cli_flag.into(),
            value_text: value_text.into(),
            slider_frac: None,
            toggle_state: Some(enabled),
            is_modified,
        }
    }

    pub fn new_cycle(
        setting_index: usize,
        label: impl Into<String>,
        cli_flag: impl Into<String>,
        value_text: impl Into<String>,
        is_modified: bool,
    ) -> Self {
        Self {
            setting_index,
            label: label.into(),
            cli_flag: cli_flag.into(),
            value_text: value_text.into(),
            slider_frac: None,
            toggle_state: None,
            is_modified,
        }
    }
}

/// Data payload passed into `TuningPanelWidget::draw` and `hit_test`.
#[derive(Debug, Clone, PartialEq)]
pub struct TuningPanelData {
    pub active_tab: TuningTab,
    pub rows: Vec<TuningRowView>,
    pub scroll_offset: usize,
    pub status_message: Option<String>,
}

impl Default for TuningPanelData {
    fn default() -> Self {
        Self {
            active_tab: TuningTab::Visual,
            rows: Vec::new(),
            scroll_offset: 0,
            status_message: None,
        }
    }
}

/// Result of hit-testing the tuning panel.
#[derive(Debug, Clone, PartialEq)]
pub enum TuningHit {
    None,
    Tab(TuningTab),
    RowSlider { setting_index: usize, frac: f32 },
    RowToggle { setting_index: usize },
    RowCycle { setting_index: usize },
    RowReset { setting_index: usize },
    SaveConfig,
    CopyCli,
    ResetAll,
    Close,
    PanelBackground,
}

/// Interactive tuning side panel widget.
#[derive(Debug, Clone)]
pub struct TuningPanelWidget {
    pub font_header: FontId,
    pub font_body: FontId,
    pub font_scale: f32,
    pub bounds: Bounds2D,
    pub visible: bool,
    pub alpha: f32,
    pub fade_time: f32,
    pub row_height: f32,
}

impl TuningPanelWidget {
    pub fn new(font_header: FontId, font_body: FontId, font_scale: f32) -> Self {
        Self {
            font_header,
            font_body,
            font_scale: font_scale.max(0.1),
            bounds: Bounds2D::new(),
            visible: true,
            alpha: 1.0,
            fade_time: TUNING_PANEL_FADE_TIME,
            row_height: 38.0,
        }
    }

    pub fn set_bounds(&mut self, pos: Vec2, size: Vec2) {
        self.bounds.set_points(pos, pos + size);
    }

    pub fn resize(
        &mut self,
        _viewport_width: u32,
        viewport_height: u32,
        font_header: FontId,
        font_body: FontId,
        font_scale: f32,
    ) {
        self.font_header = font_header;
        self.font_body = font_body;
        self.font_scale = font_scale.max(0.1);

        let w = TUNING_PANEL_WIDTH * self.font_scale;
        let margin = TUNING_PANEL_MARGIN;
        let h = (viewport_height as f32 - margin * 2.0).max(100.0);
        let x = margin;
        let y = margin;

        self.set_bounds(Vec2::new(x, y), Vec2::new(w, h));
    }

    pub fn show(&mut self, visible: bool) {
        self.visible = visible;
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn alpha(&self) -> f32 {
        self.alpha
    }

    pub fn logic(&mut self, dt: f32) {
        let target = if self.visible { 1.0 } else { 0.0 };
        let step = (dt / self.fade_time.max(0.001)).clamp(0.0, 1.0);
        if self.alpha < target {
            self.alpha = (self.alpha + step).min(target);
        } else if self.alpha > target {
            self.alpha = (self.alpha - step).max(target);
        }
    }

    pub fn header_height(&self) -> f32 {
        32.0 * self.font_scale
    }

    pub fn tabs_height(&self) -> f32 {
        28.0 * self.font_scale
    }

    pub fn footer_height(&self) -> f32 {
        36.0 * self.font_scale
    }

    pub fn max_visible_rows(&self) -> usize {
        let available = self.bounds.height()
            - self.header_height()
            - self.tabs_height()
            - self.footer_height()
            - 16.0 * self.font_scale;
        let eff_row_h = self.row_height * self.font_scale;
        if eff_row_h <= 0.0 {
            0
        } else {
            (available / eff_row_h).max(0.0) as usize
        }
    }

    /// Hit-test mouse position against panel elements.
    pub fn hit_test(&self, mouse_pos: Vec2, data: &TuningPanelData) -> TuningHit {
        if !self.bounds.contains(mouse_pos) || self.alpha <= 0.05 {
            return TuningHit::None;
        }

        let p_min = self.bounds.min;
        let p_max = self.bounds.max;
        let scale = self.font_scale;

        // 1. Header close button [X] at top-right
        let close_size = 20.0 * scale;
        let close_min = Vec2::new(p_max.x - 8.0 * scale - close_size, p_min.y + 6.0 * scale);
        let close_max = close_min + Vec2::new(close_size, close_size);
        if mouse_pos.x >= close_min.x
            && mouse_pos.x <= close_max.x
            && mouse_pos.y >= close_min.y
            && mouse_pos.y <= close_max.y
        {
            return TuningHit::Close;
        }

        // 2. Tabs
        let tabs_y = p_min.y + self.header_height();
        let tabs_h = self.tabs_height();
        if mouse_pos.y >= tabs_y && mouse_pos.y <= tabs_y + tabs_h {
            let tab_w = (self.bounds.width() - 16.0 * scale) / TuningTab::ALL.len() as f32;
            let rel_x = mouse_pos.x - (p_min.x + 8.0 * scale);
            if rel_x >= 0.0 {
                let tab_idx = (rel_x / tab_w) as usize;
                if tab_idx < TuningTab::ALL.len() {
                    return TuningHit::Tab(TuningTab::ALL[tab_idx]);
                }
            }
        }

        // 3. Footer buttons
        let footer_y = p_max.y - self.footer_height();
        if mouse_pos.y >= footer_y && mouse_pos.y <= p_max.y {
            let padding = 8.0 * scale;
            let btn_gap = 6.0 * scale;
            let total_w = self.bounds.width() - padding * 2.0;
            let btn_w = (total_w - btn_gap * 2.0) / 3.0;

            let b1_x = p_min.x + padding;
            let b2_x = b1_x + btn_w + btn_gap;
            let b3_x = b2_x + btn_w + btn_gap;

            if mouse_pos.x >= b1_x && mouse_pos.x <= b1_x + btn_w {
                return TuningHit::SaveConfig;
            } else if mouse_pos.x >= b2_x && mouse_pos.x <= b2_x + btn_w {
                return TuningHit::CopyCli;
            } else if mouse_pos.x >= b3_x && mouse_pos.x <= b3_x + btn_w {
                return TuningHit::ResetAll;
            }
            return TuningHit::PanelBackground;
        }

        // 4. Content rows
        let content_start_y = tabs_y + tabs_h + 8.0 * scale;
        let eff_row_h = self.row_height * scale;
        let max_rows = self.max_visible_rows();
        let visible_rows = data.rows.iter().skip(data.scroll_offset).take(max_rows);

        for (i, row) in visible_rows.enumerate() {
            let ry = content_start_y + i as f32 * eff_row_h;
            if mouse_pos.y >= ry && mouse_pos.y <= ry + eff_row_h {
                let row_min_x = p_min.x + 8.0 * scale;
                let row_max_x = p_max.x - 8.0 * scale;

                // Right reset button [R] if row is modified
                let reset_w = 20.0 * scale;
                let reset_x = row_max_x - reset_w;
                if row.is_modified && mouse_pos.x >= reset_x && mouse_pos.x <= row_max_x {
                    return TuningHit::RowReset {
                        setting_index: row.setting_index,
                    };
                }

                // Interactive control area (sliders, toggles, cycle buttons)
                let control_right = if row.is_modified {
                    reset_x - 6.0 * scale
                } else {
                    row_max_x
                };
                let control_left = row_min_x + 140.0 * scale;

                if mouse_pos.x >= control_left && mouse_pos.x <= control_right {
                    if row.slider_frac.is_some() {
                        let span = (control_right - control_left).max(1.0);
                        let frac = ((mouse_pos.x - control_left) / span).clamp(0.0, 1.0);
                        return TuningHit::RowSlider {
                            setting_index: row.setting_index,
                            frac,
                        };
                    } else if row.toggle_state.is_some() {
                        return TuningHit::RowToggle {
                            setting_index: row.setting_index,
                        };
                    } else {
                        return TuningHit::RowCycle {
                            setting_index: row.setting_index,
                        };
                    }
                }
                return TuningHit::PanelBackground;
            }
        }

        TuningHit::PanelBackground
    }

    /// Draw the interactive tuning panel into [`DrawList`].
    pub fn draw(&self, data: &TuningPanelData, gfx: &mut Gfx, list: &mut DrawList) {
        if self.alpha <= 0.0 || self.bounds.width() <= 0.0 || self.bounds.height() <= 0.0 {
            return;
        }

        let a = self.alpha;
        let pos = self.bounds.min;
        let size = Vec2::new(self.bounds.width(), self.bounds.height());
        let scale = self.font_scale;

        // 1. Drop shadow & Dark backdrop card
        let shadow_pos = pos + Vec2::new(3.0, 3.0);
        list.solid_rect(shadow_pos, size, Vec4::new(0.0, 0.0, 0.0, 0.45 * a));

        let bg_col = Vec4::new(0.07, 0.08, 0.11, 0.92 * a);
        list.solid_rect(pos, size, bg_col);

        let border_col = Vec4::new(0.28, 0.32, 0.42, 0.7 * a);
        list.rect_outline(pos, pos + size, 1.0, border_col);

        // 2. Title bar
        let title_style = TextStyle::new(Vec4::new(0.95, 0.95, 0.98, a))
            .with_align_top(true)
            .with_align_right(false);
        gfx.draw_text(
            list,
            self.font_header,
            pos + Vec2::new(10.0 * scale, 8.0 * scale),
            "Live Tuning",
            &title_style,
        );

        // Active tab badge dot in title bar
        let badge_dot = data.active_tab.badge_colour() * Vec4::new(1.0, 1.0, 1.0, a);
        list.solid_rect(
            pos + Vec2::new(100.0 * scale, 12.0 * scale),
            Vec2::new(8.0 * scale, 8.0 * scale),
            badge_dot,
        );

        // Close button [X]
        let close_size = 18.0 * scale;
        let close_pos = Vec2::new(
            pos.x + size.x - 10.0 * scale - close_size,
            pos.y + 7.0 * scale,
        );
        list.solid_rect(
            close_pos,
            Vec2::new(close_size, close_size),
            Vec4::new(0.18, 0.20, 0.26, 0.8 * a),
        );
        list.rect_outline(
            close_pos,
            close_pos + Vec2::new(close_size, close_size),
            1.0,
            Vec4::new(0.4, 0.45, 0.55, 0.5 * a),
        );
        let x_style = TextStyle::new(Vec4::new(0.85, 0.85, 0.9, a))
            .with_align_top(true)
            .with_align_right(false);
        gfx.draw_text(
            list,
            self.font_body,
            close_pos + Vec2::new(4.0 * scale, 2.0 * scale),
            "x",
            &x_style,
        );

        // 3. Category Tabs
        let tabs_y = pos.y + self.header_height();
        let tab_w = (size.x - 16.0 * scale) / TuningTab::ALL.len() as f32;
        let tab_h = self.tabs_height() - 4.0 * scale;

        for (i, tab) in TuningTab::ALL.iter().enumerate() {
            let tx = pos.x + 8.0 * scale + i as f32 * tab_w;
            let is_active = *tab == data.active_tab;

            let tab_bg = if is_active {
                Vec4::new(0.16, 0.19, 0.26, 0.9 * a)
            } else {
                Vec4::new(0.10, 0.12, 0.16, 0.7 * a)
            };
            list.solid_rect(Vec2::new(tx, tabs_y), Vec2::new(tab_w - 2.0, tab_h), tab_bg);

            // Active underline indicator
            if is_active {
                let line_col = tab.badge_colour() * Vec4::new(1.0, 1.0, 1.0, a);
                list.solid_rect(
                    Vec2::new(tx, tabs_y + tab_h - 2.0),
                    Vec2::new(tab_w - 2.0, 2.0),
                    line_col,
                );
            }

            let text_col = if is_active {
                Vec4::new(0.95, 0.95, 1.0, a)
            } else {
                Vec4::new(0.65, 0.70, 0.78, 0.8 * a)
            };
            let tab_text_style = TextStyle::new(text_col)
                .with_align_top(true)
                .with_align_right(false);
            gfx.draw_text(
                list,
                self.font_body,
                Vec2::new(tx + 4.0 * scale, tabs_y + 4.0 * scale),
                tab.label(),
                &tab_text_style,
            );
        }

        // 4. Content rows
        let content_start_y = tabs_y + self.tabs_height() + 8.0 * scale;
        let eff_row_h = self.row_height * scale;
        let max_rows = self.max_visible_rows();
        let visible_rows = data.rows.iter().skip(data.scroll_offset).take(max_rows);

        let row_min_x = pos.x + 8.0 * scale;
        let row_max_x = pos.x + size.x - 8.0 * scale;

        for (i, row) in visible_rows.enumerate() {
            let ry = content_start_y + i as f32 * eff_row_h;

            // Highlight modified rows subtly
            if row.is_modified {
                let mod_bg = Vec4::new(0.2, 0.25, 0.35, 0.3 * a);
                list.solid_rect(
                    Vec2::new(row_min_x, ry),
                    Vec2::new(row_max_x - row_min_x, eff_row_h - 2.0),
                    mod_bg,
                );
            }

            // Left: Label + Flag hint
            let label_col = if row.is_modified {
                Vec4::new(1.0, 0.95, 0.6, a)
            } else {
                Vec4::new(0.85, 0.88, 0.95, a)
            };
            let label_style = TextStyle::new(label_col)
                .with_align_top(true)
                .with_align_right(false);
            gfx.draw_text(
                list,
                self.font_body,
                Vec2::new(row_min_x + 2.0, ry + 2.0),
                &row.label,
                &label_style,
            );

            let flag_style = TextStyle::new(Vec4::new(0.5, 0.55, 0.65, 0.7 * a))
                .with_align_top(true)
                .with_align_right(false);
            gfx.draw_text(
                list,
                self.font_body,
                Vec2::new(row_min_x + 2.0, ry + 18.0 * scale),
                &row.cli_flag,
                &flag_style,
            );

            // Right reset button [R]
            let reset_w = 20.0 * scale;
            let reset_x = row_max_x - reset_w;
            if row.is_modified {
                list.solid_rect(
                    Vec2::new(reset_x, ry + 8.0 * scale),
                    Vec2::new(reset_w, 20.0 * scale),
                    Vec4::new(0.3, 0.18, 0.2, 0.8 * a),
                );
                list.rect_outline(
                    Vec2::new(reset_x, ry + 8.0 * scale),
                    Vec2::new(reset_x + reset_w, ry + 28.0 * scale),
                    1.0,
                    Vec4::new(0.7, 0.3, 0.3, 0.7 * a),
                );
                let r_style = TextStyle::new(Vec4::new(0.95, 0.8, 0.8, a))
                    .with_align_top(true)
                    .with_align_right(false);
                gfx.draw_text(
                    list,
                    self.font_body,
                    Vec2::new(reset_x + 5.0 * scale, ry + 10.0 * scale),
                    "R",
                    &r_style,
                );
            }

            // Control widget (Slider / Toggle / Cycle Button)
            let control_right = if row.is_modified {
                reset_x - 6.0 * scale
            } else {
                row_max_x
            };
            let control_left = row_min_x + 130.0 * scale;
            let control_w = (control_right - control_left).max(10.0);

            if let Some(frac) = row.slider_frac {
                let track_y = ry + 12.0 * scale;
                let track_h = 6.0 * scale;

                // Groove
                list.solid_rect(
                    Vec2::new(control_left, track_y),
                    Vec2::new(control_w, track_h),
                    Vec4::new(0.12, 0.14, 0.18, 0.85 * a),
                );
                // Active fill
                let fill_w = control_w * frac;
                let fill_col = data.active_tab.badge_colour() * Vec4::new(1.0, 1.0, 1.0, 0.85 * a);
                list.solid_rect(
                    Vec2::new(control_left, track_y),
                    Vec2::new(fill_w, track_h),
                    fill_col,
                );

                // Slider thumb
                let thumb_x = control_left + fill_w;
                let thumb_h = 14.0 * scale;
                let thumb_w = 6.0 * scale;
                list.solid_rect(
                    Vec2::new(thumb_x - thumb_w * 0.5, track_y - 4.0 * scale),
                    Vec2::new(thumb_w, thumb_h),
                    Vec4::new(0.95, 0.95, 1.0, a),
                );

                // Value string on the right / above
                let val_style = TextStyle::new(Vec4::new(0.8, 0.85, 0.92, a))
                    .with_align_top(true)
                    .with_align_right(true);
                gfx.draw_text(
                    list,
                    self.font_body,
                    Vec2::new(control_right, ry + 20.0 * scale),
                    &row.value_text,
                    &val_style,
                );
            } else if let Some(enabled) = row.toggle_state {
                // Checkbox toggle pill
                let pill_w = 40.0 * scale;
                let pill_h = 18.0 * scale;
                let pill_x = control_right - pill_w;
                let pill_y = ry + 8.0 * scale;

                let pill_bg = if enabled {
                    Vec4::new(0.2, 0.7, 0.4, 0.85 * a)
                } else {
                    Vec4::new(0.2, 0.22, 0.28, 0.85 * a)
                };
                list.solid_rect(
                    Vec2::new(pill_x, pill_y),
                    Vec2::new(pill_w, pill_h),
                    pill_bg,
                );
                list.rect_outline(
                    Vec2::new(pill_x, pill_y),
                    Vec2::new(pill_x + pill_w, pill_y + pill_h),
                    1.0,
                    Vec4::new(0.4, 0.45, 0.55, 0.6 * a),
                );

                let dot_x = if enabled {
                    pill_x + pill_w - 14.0 * scale
                } else {
                    pill_x + 2.0 * scale
                };
                list.solid_rect(
                    Vec2::new(dot_x, pill_y + 2.0 * scale),
                    Vec2::new(12.0 * scale, 14.0 * scale),
                    Vec4::new(0.95, 0.95, 1.0, a),
                );
            } else {
                // Cycle button [ value ]
                let btn_h = 22.0 * scale;
                let btn_y = ry + 8.0 * scale;
                list.solid_rect(
                    Vec2::new(control_left, btn_y),
                    Vec2::new(control_w, btn_h),
                    Vec4::new(0.16, 0.18, 0.24, 0.85 * a),
                );
                list.rect_outline(
                    Vec2::new(control_left, btn_y),
                    Vec2::new(control_left + control_w, btn_y + btn_h),
                    1.0,
                    Vec4::new(0.35, 0.4, 0.5, 0.6 * a),
                );

                let cycle_style = TextStyle::new(Vec4::new(0.9, 0.92, 0.98, a))
                    .with_align_top(true)
                    .with_align_right(false);
                gfx.draw_text(
                    list,
                    self.font_body,
                    Vec2::new(control_left + 6.0 * scale, btn_y + 4.0 * scale),
                    &row.value_text,
                    &cycle_style,
                );
            }
        }

        // 5. Footer Bar with [Save .conf], [Copy CLI], [Reset All], and status message
        let footer_y = pos.y + size.y - self.footer_height();
        let padding = 8.0 * scale;
        let btn_gap = 6.0 * scale;
        let total_w = size.x - padding * 2.0;
        let btn_w = (total_w - btn_gap * 2.0) / 3.0;
        let btn_h = 24.0 * scale;

        let b1_x = pos.x + padding;
        let b2_x = b1_x + btn_w + btn_gap;
        let b3_x = b2_x + btn_w + btn_gap;

        let draw_footer_btn =
            |list: &mut DrawList, gfx: &mut Gfx, x: f32, label: &str, accent: Vec4| {
                list.solid_rect(
                    Vec2::new(x, footer_y + 4.0 * scale),
                    Vec2::new(btn_w, btn_h),
                    Vec4::new(0.18, 0.22, 0.30, 0.85 * a),
                );
                list.rect_outline(
                    Vec2::new(x, footer_y + 4.0 * scale),
                    Vec2::new(x + btn_w, footer_y + 4.0 * scale + btn_h),
                    1.0,
                    accent * Vec4::new(1.0, 1.0, 1.0, 0.6 * a),
                );
                let style = TextStyle::new(Vec4::new(0.9, 0.95, 1.0, a))
                    .with_align_top(true)
                    .with_align_right(false);
                gfx.draw_text(
                    list,
                    self.font_body,
                    Vec2::new(x + 6.0 * scale, footer_y + 8.0 * scale),
                    label,
                    &style,
                );
            };

        draw_footer_btn(list, gfx, b1_x, "Save .conf", Vec4::new(0.3, 0.7, 1.0, 1.0));
        draw_footer_btn(list, gfx, b2_x, "Copy CLI", Vec4::new(0.3, 0.8, 0.4, 1.0));
        draw_footer_btn(list, gfx, b3_x, "Reset All", Vec4::new(0.9, 0.4, 0.4, 1.0));

        // Optional status message (e.g. "Saved gource.conf")
        if let Some(status) = &data.status_message {
            let status_style = TextStyle::new(Vec4::new(0.3, 0.9, 0.5, a))
                .with_align_top(true)
                .with_align_right(true);
            gfx.draw_text(
                list,
                self.font_body,
                Vec2::new(pos.x + size.x - 12.0 * scale, footer_y - 12.0 * scale),
                status,
                &status_style,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gource_core::UVec2;

    #[test]
    fn test_tuning_tab_properties() {
        for tab in &TuningTab::ALL {
            assert!(!tab.label().is_empty());
            let col = tab.badge_colour();
            assert!(col.x >= 0.0 && col.y >= 0.0 && col.z >= 0.0);
        }
    }

    #[test]
    fn test_tuning_row_view_constructors() {
        let s = TuningRowView::new_slider(1, "Seconds/Day", "--seconds-per-day", "2.5s", 0.5, true);
        assert_eq!(s.setting_index, 1);
        assert_eq!(s.slider_frac, Some(0.5));
        assert!(s.is_modified);

        let t = TuningRowView::new_toggle(2, "Bloom", "--bloom", "On", true, false);
        assert_eq!(t.toggle_state, Some(true));
        assert!(!t.is_modified);

        let c = TuningRowView::new_cycle(3, "Camera", "--camera-mode", "Overview", false);
        assert!(c.slider_frac.is_none());
        assert!(c.toggle_state.is_none());
    }

    #[test]
    fn test_tuning_panel_widget_resize_and_visibility() {
        let mut widget = TuningPanelWidget::new(FontId(1), FontId(2), 1.0);
        assert!(widget.is_visible());
        assert_eq!(widget.alpha(), 1.0);

        widget.resize(1000, 800, FontId(1), FontId(2), 1.0);
        assert_eq!(widget.bounds.min.x, TUNING_PANEL_MARGIN);
        assert_eq!(widget.bounds.min.y, TUNING_PANEL_MARGIN);
        assert_eq!(widget.bounds.width(), TUNING_PANEL_WIDTH);
        assert_eq!(widget.bounds.height(), 800.0 - TUNING_PANEL_MARGIN * 2.0);

        assert!(widget.max_visible_rows() > 0);

        widget.show(false);
        assert!(!widget.is_visible());
        widget.logic(0.1);
        assert!(widget.alpha() < 1.0);

        widget.toggle();
        assert!(widget.is_visible());
        widget.logic(1.0);
        assert_eq!(widget.alpha(), 1.0);
    }

    #[test]
    fn test_tuning_panel_hit_tests() {
        let mut widget = TuningPanelWidget::new(FontId(1), FontId(2), 1.0);
        widget.resize(1000, 800, FontId(1), FontId(2), 1.0);

        let mut data = TuningPanelData {
            active_tab: TuningTab::Visual,
            rows: vec![
                TuningRowView::new_slider(10, "Speed", "--speed", "1.0", 0.5, true),
                TuningRowView::new_toggle(11, "Loop", "--loop", "On", true, false),
                TuningRowView::new_cycle(12, "Mode", "--mode", "Default", false),
            ],
            scroll_offset: 0,
            status_message: Some("OK".to_string()),
        };

        // 1. Outside panel
        assert_eq!(
            widget.hit_test(Vec2::new(500.0, 500.0), &data),
            TuningHit::None
        );

        // 2. Close button at top right
        let close_pos = widget.bounds.min + Vec2::new(widget.bounds.width() - 15.0, 15.0);
        assert_eq!(widget.hit_test(close_pos, &data), TuningHit::Close);

        // 3. Category Tab hit test
        let tabs_y = widget.bounds.min.y + widget.header_height() + 10.0;
        let tab1_x = widget.bounds.min.x + 20.0;
        assert_eq!(
            widget.hit_test(Vec2::new(tab1_x, tabs_y), &data),
            TuningHit::Tab(TuningTab::Visual)
        );

        // 4. Footer button hit tests
        let footer_y = widget.bounds.max.y - widget.footer_height() * 0.5;
        let b1_x = widget.bounds.min.x + 30.0;
        let b2_x = widget.bounds.min.x + 150.0;
        let b3_x = widget.bounds.min.x + 260.0;
        assert_eq!(
            widget.hit_test(Vec2::new(b1_x, footer_y), &data),
            TuningHit::SaveConfig
        );
        assert_eq!(
            widget.hit_test(Vec2::new(b2_x, footer_y), &data),
            TuningHit::CopyCli
        );
        assert_eq!(
            widget.hit_test(Vec2::new(b3_x, footer_y), &data),
            TuningHit::ResetAll
        );

        // 5. Row 0 Slider hit test & Reset hit test
        let row0_y =
            widget.bounds.min.y + widget.header_height() + widget.tabs_height() + 8.0 + 15.0;
        // Reset button on row 0 (since is_modified == true)
        let reset_btn_x = widget.bounds.max.x - 16.0;
        assert_eq!(
            widget.hit_test(Vec2::new(reset_btn_x, row0_y), &data),
            TuningHit::RowReset { setting_index: 10 }
        );
        // Slider on row 0
        let slider_x = widget.bounds.min.x + 200.0;
        match widget.hit_test(Vec2::new(slider_x, row0_y), &data) {
            TuningHit::RowSlider {
                setting_index,
                frac,
            } => {
                assert_eq!(setting_index, 10);
                assert!((0.0..=1.0).contains(&frac));
            }
            other => panic!("expected RowSlider hit, got {:?}", other),
        }

        // 6. Row 1 Toggle hit test
        let row1_y = row0_y + widget.row_height;
        let toggle_x = widget.bounds.max.x - 20.0;
        assert_eq!(
            widget.hit_test(Vec2::new(toggle_x, row1_y), &data),
            TuningHit::RowToggle { setting_index: 11 }
        );

        // 7. Row 2 Cycle hit test
        let row2_y = row1_y + widget.row_height;
        let cycle_x = widget.bounds.min.x + 200.0;
        assert_eq!(
            widget.hit_test(Vec2::new(cycle_x, row2_y), &data),
            TuningHit::RowCycle { setting_index: 12 }
        );

        // 8. Background hit inside panel
        let bg_pos = widget.bounds.min + Vec2::new(50.0, 50.0);
        let hit = widget.hit_test(bg_pos, &data);
        assert!(matches!(
            hit,
            TuningHit::PanelBackground | TuningHit::Tab(_)
        ));

        // Test with empty rows
        data.rows.clear();
        assert_eq!(
            widget.hit_test(Vec2::new(widget.bounds.min.x + 50.0, row0_y), &data),
            TuningHit::PanelBackground
        );
    }

    #[test]
    fn test_tuning_panel_drawing() {
        let mut gfx = Gfx::new();
        let face = gfx.fonts.default_face();
        let font_header = gfx.fonts.font(face, 14);
        let font_body = gfx.fonts.font(face, 11);

        let mut widget = TuningPanelWidget::new(font_header, font_body, 1.0);
        widget.resize(1000, 800, font_header, font_body, 1.0);

        let data = TuningPanelData {
            active_tab: TuningTab::Visual,
            rows: vec![
                TuningRowView::new_slider(1, "Elasticity", "--elasticity", "0.75", 0.75, true),
                TuningRowView::new_toggle(2, "Highlight", "--highlight-dirs", "Yes", true, false),
                TuningRowView::new_cycle(3, "Camera", "--camera-mode", "Track", true),
            ],
            scroll_offset: 0,
            status_message: Some("Configuration Saved".to_string()),
        };

        let mut list = DrawList::new(UVec2::new(1000, 800));
        widget.draw(&data, &mut gfx, &mut list);
        assert!(!list.is_empty());

        // Zero alpha skips
        widget.alpha = 0.0;
        let mut list2 = DrawList::new(UVec2::new(1000, 800));
        widget.draw(&data, &mut gfx, &mut list2);
        assert!(list2.is_empty());
    }
}
