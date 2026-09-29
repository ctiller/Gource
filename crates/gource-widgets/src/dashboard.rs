//! HUD analytics and Git-of-Theseus dashboard widgets.
//!
//! Renders real-time analytics panels (sparklines, stacked diff bars,
//! Git-of-Theseus cohort survival area chart, and editor leaderboards) directly
//! into [`DrawList`] using [`Gfx`].

use glam::{Vec2, Vec3, Vec4};
use gource_draw::font::TextStyle;
use gource_draw::{DrawList, FontId, Gfx};

/// Format an integer into compact human-readable representation:
/// e.g. `950 -> "950"`, `12_400 -> "12.4k"`, `1_500_000 -> "1.5M"`.
pub fn format_compact_u64(n: u64) -> String {
    if n >= 1_000_000_000 {
        let v = n as f64 / 1_000_000_000.0;
        if v >= 100.0 {
            format!("{:.0}B", v)
        } else if v >= 10.0 {
            format!("{:.1}B", v)
        } else {
            format!("{:.2}B", v)
        }
    } else if n >= 1_000_000 {
        let v = n as f64 / 1_000_000.0;
        if v >= 100.0 {
            format!("{:.0}M", v)
        } else {
            format!("{:.1}M", v)
        }
    } else if n >= 10_000 {
        let v = n as f64 / 1_000.0;
        if v >= 100.0 {
            format!("{:.0}k", v)
        } else {
            format!("{:.1}k", v)
        }
    } else if n >= 1_000 {
        let v = n as f64 / 1_000.0;
        format!("{:.1}k", v)
    } else {
        format!("{n}")
    }
}

/// Helper to render the shared card background, drop shadow, and 1px border.
pub fn draw_card_base(list: &mut DrawList, pos: Vec2, size: Vec2, alpha: f32) {
    if alpha <= 0.0 || size.x <= 0.0 || size.y <= 0.0 {
        return;
    }
    // Subtle drop shadow at pos + (3.0, 3.0) with (0, 0, 0, 0.35 * alpha)
    let shadow_pos = pos + Vec2::new(3.0, 3.0);
    let shadow_col = Vec4::new(0.0, 0.0, 0.0, 0.35 * alpha);
    list.solid_rect(shadow_pos, size, shadow_col);

    // Semi-transparent dark card background (0.06, 0.07, 0.09, 0.78 * alpha)
    let bg_col = Vec4::new(0.06, 0.07, 0.09, 0.78 * alpha);
    list.solid_rect(pos, size, bg_col);

    // 1px border outline (0.3, 0.35, 0.45, 0.6 * alpha)
    let border_col = Vec4::new(0.3, 0.35, 0.45, 0.6 * alpha);
    list.rect_outline(pos, pos + size, 1.0, border_col);
}

/// Helper to draw a standard panel header with title on the left and value on the right.
#[allow(clippy::too_many_arguments)]
pub fn draw_panel_header(
    gfx: &mut Gfx,
    list: &mut DrawList,
    font: Option<FontId>,
    pos: Vec2,
    width: f32,
    title: &str,
    value: &str,
    alpha: f32,
    padding: f32,
) {
    let Some(font) = font else {
        return;
    };
    if alpha <= 0.0 {
        return;
    }

    let title_style = TextStyle::new(Vec4::new(0.75, 0.8, 0.88, alpha))
        .with_shadow(true)
        .with_align_top(true)
        .with_align_right(false);
    gfx.draw_text(
        list,
        font,
        pos + Vec2::new(padding, padding),
        title,
        &title_style,
    );

    if !value.is_empty() {
        let value_style = TextStyle::new(Vec4::new(0.95, 0.95, 0.98, alpha))
            .with_shadow(true)
            .with_align_top(true)
            .with_align_right(true);
        gfx.draw_text(
            list,
            font,
            pos + Vec2::new(width - padding, padding),
            value,
            &value_style,
        );
    }
}

/// SparklinePanel displays a title, a primary value, an optional signed delta badge,
/// and a polyline / filled area sparkline chart.
#[derive(Debug, Clone)]
pub struct SparklinePanel {
    pub title: String,
    pub value_str: String,
    pub delta_str: Option<String>,
    pub delta_positive: bool,
    pub values: Vec<f32>,
    pub line_colour: Vec3,
    pub fill_alpha: f32,
    pub height: f32,
}

impl SparklinePanel {
    pub fn new(title: impl Into<String>, value_str: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            value_str: value_str.into(),
            delta_str: None,
            delta_positive: true,
            values: Vec::new(),
            line_colour: Vec3::new(0.3, 0.7, 1.0),
            fill_alpha: 0.25,
            height: 80.0,
        }
    }

    pub fn with_values(mut self, values: &[f32]) -> Self {
        self.values = values.to_vec();
        self
    }

    pub fn with_delta(mut self, delta: impl Into<String>, positive: bool) -> Self {
        self.delta_str = Some(delta.into());
        self.delta_positive = positive;
        self
    }

    pub fn with_line_colour(mut self, colour: Vec3) -> Self {
        self.line_colour = colour;
        self
    }

    pub fn with_height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn height(&self, _font_scale: f32) -> f32 {
        self.height
    }

    pub fn draw(
        &self,
        gfx: &mut Gfx,
        list: &mut DrawList,
        font: Option<FontId>,
        pos: Vec2,
        width: f32,
        alpha: f32,
    ) {
        if alpha <= 0.0 || width <= 0.0 || self.height <= 0.0 {
            return;
        }
        let size = Vec2::new(width, self.height);
        draw_card_base(list, pos, size, alpha);

        let padding = 8.0;
        draw_panel_header(
            gfx,
            list,
            font,
            pos,
            width,
            &self.title,
            &self.value_str,
            alpha,
            padding,
        );

        // Subtitle / delta badge
        let header_h = 24.0;
        if let (Some(delta), Some(f)) = (&self.delta_str, font) {
            let delta_col = if self.delta_positive {
                Vec4::new(0.3, 0.9, 0.4, alpha)
            } else {
                Vec4::new(0.95, 0.35, 0.35, alpha)
            };
            let style = TextStyle::new(delta_col)
                .with_shadow(true)
                .with_align_top(true)
                .with_align_right(true);
            gfx.draw_text(
                list,
                f,
                pos + Vec2::new(width - padding, pos.y + header_h - pos.y),
                delta,
                &style,
            );
        }

        // Draw sparkline curve & fill
        if self.values.is_empty() {
            return;
        }

        let chart_top = pos.y + header_h + 8.0;
        let chart_bottom = pos.y + self.height - padding;
        let chart_h = chart_bottom - chart_top;
        if chart_h <= 2.0 {
            return;
        }

        let chart_left = pos.x + padding;
        let chart_right = pos.x + width - padding;
        let chart_w = chart_right - chart_left;
        if chart_w <= 0.0 {
            return;
        }

        let min_val = self
            .values
            .iter()
            .copied()
            .fold(f32::INFINITY, f32::min)
            .min(0.0);
        let max_val = self
            .values
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max)
            .max(min_val + 0.001);
        let val_range = max_val - min_val;

        let n = self.values.len();
        let compute_pt = |i: usize, v: f32| -> Vec2 {
            let x = if n <= 1 {
                chart_left + chart_w * 0.5
            } else {
                chart_left + (i as f32 / (n - 1) as f32) * chart_w
            };
            let norm = ((v - min_val) / val_range).clamp(0.0, 1.0);
            let y = chart_bottom - norm * chart_h;
            Vec2::new(x, y)
        };

        let fill_col = Vec4::new(
            self.line_colour.x,
            self.line_colour.y,
            self.line_colour.z,
            self.fill_alpha * alpha,
        );
        let line_col = Vec4::new(
            self.line_colour.x,
            self.line_colour.y,
            self.line_colour.z,
            0.95 * alpha,
        );

        if n == 1 {
            let pt = compute_pt(0, self.values[0]);
            list.solid_rect(
                Vec2::new(chart_left, pt.y),
                Vec2::new(chart_w, chart_bottom - pt.y),
                fill_col,
            );
            list.line(
                Vec2::new(chart_left, pt.y),
                Vec2::new(chart_right, pt.y),
                1.5,
                line_col,
            );
            return;
        }

        // Draw filled quads between consecutive points and the bottom
        for i in 0..n - 1 {
            let p0 = compute_pt(i, self.values[i]);
            let p1 = compute_pt(i + 1, self.values[i + 1]);

            let corners = [
                p0,
                p1,
                Vec2::new(p1.x, chart_bottom),
                Vec2::new(p0.x, chart_bottom),
            ];
            list.quad(
                gource_draw::TextureId::WHITE,
                corners,
                [
                    [0.0, 0.0].into(),
                    [1.0, 0.0].into(),
                    [1.0, 1.0].into(),
                    [0.0, 1.0].into(),
                ],
                fill_col,
            );
            list.line(p0, p1, 1.5, line_col);
        }
    }
}

/// StackedDiffBarsPanel displays lines added (`+`) vs lines removed (`-`)
/// over recent periods around a horizontal zero midline.
#[derive(Debug, Clone)]
pub struct StackedDiffBarsPanel {
    pub title: String,
    pub summary_str: String,
    pub diffs: Vec<(u64, u64)>, // (added, removed)
    pub height: f32,
}

impl StackedDiffBarsPanel {
    pub fn new(title: impl Into<String>, summary_str: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            summary_str: summary_str.into(),
            diffs: Vec::new(),
            height: 90.0,
        }
    }

    pub fn with_diffs(mut self, diffs: &[(u64, u64)]) -> Self {
        self.diffs = diffs.to_vec();
        self
    }

    pub fn with_height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn height(&self, _font_scale: f32) -> f32 {
        self.height
    }

    pub fn draw(
        &self,
        gfx: &mut Gfx,
        list: &mut DrawList,
        font: Option<FontId>,
        pos: Vec2,
        width: f32,
        alpha: f32,
    ) {
        if alpha <= 0.0 || width <= 0.0 || self.height <= 0.0 {
            return;
        }
        let size = Vec2::new(width, self.height);
        draw_card_base(list, pos, size, alpha);

        let padding = 8.0;
        draw_panel_header(
            gfx,
            list,
            font,
            pos,
            width,
            &self.title,
            &self.summary_str,
            alpha,
            padding,
        );

        let chart_top = pos.y + 26.0;
        let chart_bottom = pos.y + self.height - padding;
        let chart_h = chart_bottom - chart_top;
        if chart_h <= 4.0 || self.diffs.is_empty() {
            return;
        }

        let chart_left = pos.x + padding;
        let chart_right = pos.x + width - padding;
        let chart_w = chart_right - chart_left;
        if chart_w <= 0.0 {
            return;
        }

        let midline_y = chart_top + chart_h * 0.5;

        // Zero midline
        let midline_col = Vec4::new(0.4, 0.45, 0.55, 0.5 * alpha);
        list.line(
            Vec2::new(chart_left, midline_y),
            Vec2::new(chart_right, midline_y),
            1.0,
            midline_col,
        );

        let max_val = self
            .diffs
            .iter()
            .map(|&(a, r)| a.max(r))
            .max()
            .unwrap_or(0)
            .max(1) as f32;

        let half_h = chart_h * 0.46;
        let n = self.diffs.len();
        let slot_w = chart_w / n as f32;
        let bar_w = (slot_w * 0.75).max(1.0);

        let green_col = Vec4::new(0.25, 0.85, 0.45, 0.85 * alpha);
        let red_col = Vec4::new(0.95, 0.35, 0.35, 0.85 * alpha);

        for (i, &(added, removed)) in self.diffs.iter().enumerate() {
            let cx = chart_left + i as f32 * slot_w + (slot_w - bar_w) * 0.5;

            // Added bar (upwards from midline)
            if added > 0 {
                let bar_h = ((added as f32 / max_val) * half_h).max(1.0);
                list.solid_rect(
                    Vec2::new(cx, midline_y - bar_h),
                    Vec2::new(bar_w, bar_h),
                    green_col,
                );
            }

            // Removed bar (downwards from midline)
            if removed > 0 {
                let bar_h = ((removed as f32 / max_val) * half_h).max(1.0);
                list.solid_rect(Vec2::new(cx, midline_y), Vec2::new(bar_w, bar_h), red_col);
            }
        }
    }
}

/// TheseusCohortAreaPanel renders a classic Git-of-Theseus stacked area chart.
#[derive(Debug, Clone)]
pub struct TheseusCohortAreaPanel {
    pub title: String,
    pub cohort_labels: Vec<String>,
    pub cohort_colours: Vec<Vec3>,
    pub samples: Vec<Vec<u64>>,
    pub half_life_days: Option<f32>,
    pub churn_rate: Option<f32>,
    pub height: f32,
}

impl TheseusCohortAreaPanel {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            cohort_labels: Vec::new(),
            cohort_colours: Vec::new(),
            samples: Vec::new(),
            half_life_days: None,
            churn_rate: None,
            height: 140.0,
        }
    }

    pub fn with_cohorts(
        mut self,
        labels: &[String],
        colours: &[Vec3],
        samples: &[Vec<u64>],
    ) -> Self {
        self.cohort_labels = labels.to_vec();
        self.cohort_colours = colours.to_vec();
        self.samples = samples.to_vec();
        self
    }

    pub fn with_analytics(mut self, half_life_days: Option<f32>, churn_rate: Option<f32>) -> Self {
        self.half_life_days = half_life_days;
        self.churn_rate = churn_rate;
        self
    }

    pub fn with_height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn height(&self, _font_scale: f32) -> f32 {
        self.height
    }

    pub fn draw(
        &self,
        gfx: &mut Gfx,
        list: &mut DrawList,
        font: Option<FontId>,
        pos: Vec2,
        width: f32,
        alpha: f32,
    ) {
        if alpha <= 0.0 || width <= 0.0 || self.height <= 0.0 {
            return;
        }
        let size = Vec2::new(width, self.height);
        draw_card_base(list, pos, size, alpha);

        let padding = 8.0;

        // Subtitle text for half-life and churn
        let mut stats = Vec::new();
        if let Some(hl) = self.half_life_days {
            stats.push(format!("t½: {:.0}d", hl));
        }
        if let Some(cr) = self.churn_rate {
            stats.push(format!("churn: {:.1}%/m", cr * 100.0));
        }
        let header_val = stats.join("  ");

        draw_panel_header(
            gfx,
            list,
            font,
            pos,
            width,
            &self.title,
            &header_val,
            alpha,
            padding,
        );

        let legend_h = 16.0;
        let chart_top = pos.y + 24.0;
        let chart_bottom = pos.y + self.height - padding - legend_h;
        let chart_h = chart_bottom - chart_top;
        if chart_h <= 4.0 || self.samples.is_empty() {
            return;
        }

        let chart_left = pos.x + padding;
        let chart_right = pos.x + width - padding;
        let chart_w = chart_right - chart_left;
        if chart_w <= 0.0 {
            return;
        }

        // Compute max total lines across all time slices
        let max_total = self
            .samples
            .iter()
            .map(|slice| slice.iter().sum::<u64>())
            .max()
            .unwrap_or(0)
            .max(1) as f32;

        let num_slices = self.samples.len();
        let num_cohorts = self.cohort_labels.len();

        // Helper to get normalized cumulative y coordinate for slice s and cohort boundary c
        let get_y = |s: usize, c: usize| -> f32 {
            let slice = &self.samples[s];
            let sum_up_to: u64 = slice.iter().take(c).sum();
            let frac = (sum_up_to as f32 / max_total).clamp(0.0, 1.0);
            chart_bottom - frac * chart_h
        };

        let get_x = |s: usize| -> f32 {
            if num_slices <= 1 {
                chart_left
            } else {
                chart_left + (s as f32 / (num_slices - 1) as f32) * chart_w
            }
        };

        if num_slices == 1 {
            let s = 0;
            for c in 0..num_cohorts {
                let y_bot = get_y(s, c);
                let y_top = get_y(s, c + 1);
                let col = self
                    .cohort_colours
                    .get(c)
                    .copied()
                    .unwrap_or(Vec3::new(0.5, 0.5, 0.5));
                let quad_col = Vec4::new(col.x, col.y, col.z, 0.85 * alpha);
                list.solid_rect(
                    Vec2::new(chart_left, y_top),
                    Vec2::new(chart_w, y_bot - y_top),
                    quad_col,
                );
            }
        } else {
            // Draw stacked area trapezoids for each slice interval (s, s+1) and cohort c
            for s in 0..num_slices - 1 {
                let x0 = get_x(s);
                let x1 = get_x(s + 1);

                for c in 0..num_cohorts {
                    let y0_bot = get_y(s, c);
                    let y0_top = get_y(s, c + 1);
                    let y1_bot = get_y(s + 1, c);
                    let y1_top = get_y(s + 1, c + 1);

                    // Skip drawing degenerate band
                    if (y0_bot - y0_top).abs() < 1e-4 && (y1_bot - y1_top).abs() < 1e-4 {
                        continue;
                    }

                    let col = self
                        .cohort_colours
                        .get(c)
                        .copied()
                        .unwrap_or(Vec3::new(0.5, 0.5, 0.5));
                    let quad_col = Vec4::new(col.x, col.y, col.z, 0.85 * alpha);

                    let corners = [
                        Vec2::new(x0, y0_top),
                        Vec2::new(x1, y1_top),
                        Vec2::new(x1, y1_bot),
                        Vec2::new(x0, y0_bot),
                    ];
                    list.quad(
                        gource_draw::TextureId::WHITE,
                        corners,
                        [
                            [0.0, 0.0].into(),
                            [1.0, 0.0].into(),
                            [1.0, 1.0].into(),
                            [0.0, 1.0].into(),
                        ],
                        quad_col,
                    );
                }
            }
        }

        // Draw legend at the bottom
        let legend_y = pos.y + self.height - legend_h + 2.0;
        let mut cur_x = chart_left;
        let swatch_size = Vec2::new(8.0, 8.0);

        for (i, label) in self.cohort_labels.iter().enumerate().take(5) {
            let col = self
                .cohort_colours
                .get(i)
                .copied()
                .unwrap_or(Vec3::new(0.5, 0.5, 0.5));
            let swatch_col = Vec4::new(col.x, col.y, col.z, alpha);

            list.solid_rect(Vec2::new(cur_x, legend_y), swatch_size, swatch_col);
            cur_x += swatch_size.x + 3.0;

            if let Some(f) = font {
                let label_style = TextStyle::new(Vec4::new(0.7, 0.75, 0.8, alpha))
                    .with_align_top(true)
                    .with_align_right(false);
                gfx.draw_text(
                    list,
                    f,
                    Vec2::new(cur_x, legend_y - 1.0),
                    label,
                    &label_style,
                );
                let text_w = gfx.text_width(f, label);
                cur_x += text_w + 10.0;
            } else {
                cur_x += 25.0;
            }

            if cur_x > chart_right - 10.0 {
                break;
            }
        }
    }
}

/// EditorsLeaderboardPanel displays the active editors count and a bar chart leaderboard of top contributors.
#[derive(Debug, Clone)]
pub struct EditorsLeaderboardPanel {
    pub title: String,
    pub active_editors_count: usize,
    pub rows: Vec<(String, Vec3, u32, u64)>, // (name, colour, commits, lines_added)
    pub max_rows: usize,
    pub row_height: f32,
}

impl EditorsLeaderboardPanel {
    pub fn new(title: impl Into<String>, active_editors: usize) -> Self {
        Self {
            title: title.into(),
            active_editors_count: active_editors,
            rows: Vec::new(),
            max_rows: 5,
            row_height: 16.0,
        }
    }

    pub fn with_rows(mut self, rows: &[(String, Vec3, u32, u64)]) -> Self {
        self.rows = rows.to_vec();
        self
    }

    pub fn with_max_rows(mut self, max: usize) -> Self {
        self.max_rows = max;
        self
    }

    pub fn height(&self, _font_scale: f32) -> f32 {
        let display_count = self.rows.len().min(self.max_rows);
        32.0 + display_count as f32 * self.row_height + 8.0
    }

    pub fn draw(
        &self,
        gfx: &mut Gfx,
        list: &mut DrawList,
        font: Option<FontId>,
        pos: Vec2,
        width: f32,
        alpha: f32,
    ) {
        if alpha <= 0.0 || width <= 0.0 {
            return;
        }
        let total_h = self.height(1.0);
        let size = Vec2::new(width, total_h);
        draw_card_base(list, pos, size, alpha);

        let padding = 8.0;
        let count_str = format!("Active: {}", self.active_editors_count);
        draw_panel_header(
            gfx,
            list,
            font,
            pos,
            width,
            &self.title,
            &count_str,
            alpha,
            padding,
        );

        let max_commits = self.rows.iter().map(|r| r.2).max().unwrap_or(0).max(1) as f32;

        let content_w = width - padding * 2.0;
        let mut y = pos.y + 26.0;

        for (name, colour, commits, lines) in self.rows.iter().take(self.max_rows) {
            let frac = (*commits as f32 / max_commits).clamp(0.0, 1.0);
            let bar_w = content_w * frac;

            // Bar background
            let bar_bg = Vec4::new(colour.x, colour.y, colour.z, 0.25 * alpha);
            list.solid_rect(
                Vec2::new(pos.x + padding, y),
                Vec2::new(content_w, self.row_height - 2.0),
                Vec4::new(0.12, 0.14, 0.18, 0.4 * alpha),
            );
            list.solid_rect(
                Vec2::new(pos.x + padding, y),
                Vec2::new(bar_w, self.row_height - 2.0),
                bar_bg,
            );

            // Left: Name
            if let Some(f) = font {
                let name_style = TextStyle::new(Vec4::new(0.9, 0.9, 0.95, alpha))
                    .with_shadow(true)
                    .with_align_top(true)
                    .with_align_right(false);
                gfx.draw_text(
                    list,
                    f,
                    Vec2::new(pos.x + padding + 4.0, y),
                    name,
                    &name_style,
                );

                // Right: commits / lines
                let right_text = format!("{}c +{}", commits, format_compact_u64(*lines));
                let right_style = TextStyle::new(Vec4::new(0.7, 0.75, 0.8, alpha))
                    .with_shadow(true)
                    .with_align_top(true)
                    .with_align_right(true);
                gfx.draw_text(
                    list,
                    f,
                    Vec2::new(pos.x + width - padding - 4.0, y),
                    &right_text,
                    &right_style,
                );
            }

            y += self.row_height;
        }
    }
}

/// Dynamic panel enum that can be held by [`DashboardStack`].
#[derive(Debug, Clone)]
pub enum DashboardPanel {
    Sparkline(SparklinePanel),
    StackedDiffBars(StackedDiffBarsPanel),
    TheseusCohort(TheseusCohortAreaPanel),
    EditorsLeaderboard(EditorsLeaderboardPanel),
}

impl DashboardPanel {
    pub fn height(&self, font_scale: f32) -> f32 {
        match self {
            Self::Sparkline(p) => p.height(font_scale),
            Self::StackedDiffBars(p) => p.height(font_scale),
            Self::TheseusCohort(p) => p.height(font_scale),
            Self::EditorsLeaderboard(p) => p.height(font_scale),
        }
    }

    pub fn draw(
        &self,
        gfx: &mut Gfx,
        list: &mut DrawList,
        font: Option<FontId>,
        pos: Vec2,
        width: f32,
        alpha: f32,
    ) {
        match self {
            Self::Sparkline(p) => p.draw(gfx, list, font, pos, width, alpha),
            Self::StackedDiffBars(p) => p.draw(gfx, list, font, pos, width, alpha),
            Self::TheseusCohort(p) => p.draw(gfx, list, font, pos, width, alpha),
            Self::EditorsLeaderboard(p) => p.draw(gfx, list, font, pos, width, alpha),
        }
    }
}

/// DashboardStack container for managing and drawing HUD analytics panels.
#[derive(Debug, Clone)]
pub struct DashboardStack {
    pub panels: Vec<DashboardPanel>,
    pub margin_right: f32,
    pub top_y: f32,
    pub panel_width: f32,
    pub panel_gap: f32,
    pub alpha: f32,
}

impl Default for DashboardStack {
    fn default() -> Self {
        Self::new()
    }
}

impl DashboardStack {
    pub fn new() -> Self {
        Self {
            panels: Vec::new(),
            margin_right: 16.0,
            top_y: 48.0,
            panel_width: 240.0,
            panel_gap: 10.0,
            alpha: 1.0,
        }
    }

    pub fn add_panel(&mut self, panel: DashboardPanel) {
        self.panels.push(panel);
    }

    pub fn clear(&mut self) {
        self.panels.clear();
    }

    /// Palette of 12 distinct, pleasant colours for Git-of-Theseus cohorts.
    pub fn cohort_palette(index: usize) -> Vec3 {
        const PALETTE: [Vec3; 12] = [
            Vec3::new(0.24, 0.52, 0.88), // Soft Blue
            Vec3::new(0.30, 0.76, 0.46), // Emerald
            Vec3::new(0.95, 0.62, 0.22), // Warm Amber
            Vec3::new(0.85, 0.32, 0.38), // Crimson Rose
            Vec3::new(0.58, 0.40, 0.86), // Purple
            Vec3::new(0.20, 0.72, 0.76), // Teal
            Vec3::new(0.92, 0.80, 0.28), // Golden Yellow
            Vec3::new(0.88, 0.45, 0.65), // Pink Orchid
            Vec3::new(0.42, 0.65, 0.32), // Olive Green
            Vec3::new(0.70, 0.45, 0.28), // Bronze
            Vec3::new(0.38, 0.60, 0.72), // Steel Blue
            Vec3::new(0.55, 0.58, 0.65), // Slate Gray
        ];
        PALETTE[index % PALETTE.len()]
    }

    pub fn draw(
        &self,
        gfx: &mut Gfx,
        list: &mut DrawList,
        font: Option<FontId>,
        viewport_w: f32,
        font_scale: f32,
    ) {
        if self.alpha <= 0.0 || self.panels.is_empty() {
            return;
        }

        let effective_width = self.panel_width * font_scale;
        let effective_gap = self.panel_gap * font_scale;
        let x = (viewport_w - self.margin_right - effective_width).max(0.0);
        let mut y = self.top_y;

        for panel in &self.panels {
            let h = panel.height(font_scale);
            panel.draw(
                gfx,
                list,
                font,
                Vec2::new(x, y),
                effective_width,
                self.alpha,
            );
            y += h + effective_gap;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::UVec2;

    #[test]
    fn test_format_compact_u64() {
        assert_eq!(format_compact_u64(0), "0");
        assert_eq!(format_compact_u64(950), "950");
        assert_eq!(format_compact_u64(1_000), "1.0k");
        assert_eq!(format_compact_u64(12_400), "12.4k");
        assert_eq!(format_compact_u64(150_000), "150k");
        assert_eq!(format_compact_u64(1_500_000), "1.5M");
        assert_eq!(format_compact_u64(25_000_000), "25.0M");
        assert_eq!(format_compact_u64(450_000_000), "450M");
        assert_eq!(format_compact_u64(1_500_000_000), "1.50B");
        assert_eq!(format_compact_u64(25_000_000_000), "25.0B");
        assert_eq!(format_compact_u64(300_000_000_000), "300B");
    }

    #[test]
    fn test_cohort_palette() {
        for i in 0..24 {
            let col = DashboardStack::cohort_palette(i);
            assert!(col.x >= 0.0 && col.x <= 1.0);
            assert!(col.y >= 0.0 && col.y <= 1.0);
            assert!(col.z >= 0.0 && col.z <= 1.0);
        }
    }

    #[test]
    fn test_sparkline_panel_drawing() {
        let mut gfx = Gfx::new();
        let face = gfx.fonts.default_face();
        let font = gfx.fonts.font(face, 12);
        let mut list = DrawList::new(UVec2::new(800, 600));

        // 1. Empty values
        let panel_empty = SparklinePanel::new("Empty Series", "0");
        panel_empty.draw(
            &mut gfx,
            &mut list,
            Some(font),
            Vec2::new(10.0, 10.0),
            200.0,
            1.0,
        );
        assert!(!list.is_empty());

        // 2. Single value
        let mut list2 = DrawList::new(UVec2::new(800, 600));
        let panel_single = SparklinePanel::new("Single", "100")
            .with_values(&[42.0])
            .with_delta("+5%", true)
            .with_line_colour(Vec3::ONE)
            .with_height(90.0);
        assert_eq!(panel_single.height(1.0), 90.0);
        panel_single.draw(
            &mut gfx,
            &mut list2,
            Some(font),
            Vec2::new(10.0, 10.0),
            200.0,
            1.0,
        );
        assert!(!list2.is_empty());

        // 3. Multi values with negative delta
        let mut list3 = DrawList::new(UVec2::new(800, 600));
        let panel_multi = SparklinePanel::new("Multi", "42k")
            .with_values(&[10.0, 15.0, 12.0, 25.0, 30.0, 28.0])
            .with_delta("-12%", false);
        panel_multi.draw(
            &mut gfx,
            &mut list3,
            Some(font),
            Vec2::new(10.0, 10.0),
            200.0,
            1.0,
        );
        assert!(!list3.is_empty());

        // 4. Zero alpha
        let mut list4 = DrawList::new(UVec2::new(800, 600));
        panel_multi.draw(
            &mut gfx,
            &mut list4,
            Some(font),
            Vec2::new(10.0, 10.0),
            200.0,
            0.0,
        );
        assert!(list4.is_empty());

        // 5. Degenerate size / height
        let mut list5 = DrawList::new(UVec2::new(800, 600));
        let panel_tiny = SparklinePanel::new("Tiny", "0")
            .with_height(10.0)
            .with_values(&[1.0, 2.0]);
        panel_tiny.draw(
            &mut gfx,
            &mut list5,
            Some(font),
            Vec2::new(10.0, 10.0),
            200.0,
            1.0,
        );
    }

    #[test]
    fn test_stacked_diff_bars_panel_drawing() {
        let mut gfx = Gfx::new();
        let face = gfx.fonts.default_face();
        let font = gfx.fonts.font(face, 12);
        let mut list = DrawList::new(UVec2::new(800, 600));

        let diffs = vec![(100, 50), (200, 0), (0, 150), (300, 200)];
        let panel = StackedDiffBarsPanel::new("Diffs", "+450 -400")
            .with_diffs(&diffs)
            .with_height(100.0);
        assert_eq!(panel.height(1.0), 100.0);

        panel.draw(
            &mut gfx,
            &mut list,
            Some(font),
            Vec2::new(10.0, 10.0),
            220.0,
            1.0,
        );
        assert!(!list.is_empty());

        // Empty diffs
        let mut list2 = DrawList::new(UVec2::new(800, 600));
        let panel_empty = StackedDiffBarsPanel::new("Diffs Empty", "0");
        panel_empty.draw(
            &mut gfx,
            &mut list2,
            Some(font),
            Vec2::new(10.0, 10.0),
            220.0,
            1.0,
        );
        assert!(!list2.is_empty());

        // Zero alpha
        let mut list3 = DrawList::new(UVec2::new(800, 600));
        panel.draw(
            &mut gfx,
            &mut list3,
            Some(font),
            Vec2::new(10.0, 10.0),
            220.0,
            0.0,
        );
        assert!(list3.is_empty());
    }

    #[test]
    fn test_theseus_cohort_area_panel_drawing() {
        let mut gfx = Gfx::new();
        let face = gfx.fonts.default_face();
        let font = gfx.fonts.font(face, 12);
        let mut list = DrawList::new(UVec2::new(800, 600));

        let labels = vec!["2022".to_string(), "2023".to_string(), "2024".to_string()];
        let colours = vec![
            Vec3::new(0.2, 0.5, 0.9),
            Vec3::new(0.3, 0.8, 0.4),
            Vec3::new(0.9, 0.6, 0.2),
        ];
        let samples = vec![vec![1000, 0, 0], vec![900, 500, 0], vec![800, 450, 700]];

        let panel = TheseusCohortAreaPanel::new("Git-of-Theseus Cohorts")
            .with_cohorts(&labels, &colours, &samples)
            .with_analytics(Some(450.0), Some(0.045))
            .with_height(150.0);
        assert_eq!(panel.height(1.0), 150.0);

        panel.draw(
            &mut gfx,
            &mut list,
            Some(font),
            Vec2::new(10.0, 10.0),
            240.0,
            1.0,
        );
        assert!(!list.is_empty());

        // Single sample slice
        let mut list_single = DrawList::new(UVec2::new(800, 600));
        let panel_single = TheseusCohortAreaPanel::new("Single Slice").with_cohorts(
            &labels,
            &colours,
            &samples[..1],
        );
        panel_single.draw(
            &mut gfx,
            &mut list_single,
            Some(font),
            Vec2::new(10.0, 10.0),
            240.0,
            1.0,
        );
        assert!(!list_single.is_empty());

        // Without font
        let mut list_no_font = DrawList::new(UVec2::new(800, 600));
        panel.draw(
            &mut gfx,
            &mut list_no_font,
            None,
            Vec2::new(10.0, 10.0),
            240.0,
            1.0,
        );
        assert!(!list_no_font.is_empty());

        // Zero alpha
        let mut list_zero = DrawList::new(UVec2::new(800, 600));
        panel.draw(
            &mut gfx,
            &mut list_zero,
            Some(font),
            Vec2::new(10.0, 10.0),
            240.0,
            0.0,
        );
        assert!(list_zero.is_empty());

        // Empty samples
        let mut list_empty = DrawList::new(UVec2::new(800, 600));
        let panel_empty = TheseusCohortAreaPanel::new("Empty");
        panel_empty.draw(
            &mut gfx,
            &mut list_empty,
            Some(font),
            Vec2::new(10.0, 10.0),
            240.0,
            1.0,
        );
        assert!(!list_empty.is_empty());
    }

    #[test]
    fn test_editors_leaderboard_panel_drawing() {
        let mut gfx = Gfx::new();
        let face = gfx.fonts.default_face();
        let font = gfx.fonts.font(face, 12);
        let mut list = DrawList::new(UVec2::new(800, 600));

        let rows = vec![
            ("Alice".to_string(), Vec3::new(0.2, 0.5, 0.9), 120, 15000),
            ("Bob".to_string(), Vec3::new(0.9, 0.4, 0.3), 85, 9400),
            ("Carol".to_string(), Vec3::new(0.3, 0.8, 0.4), 42, 3200),
        ];

        let panel = EditorsLeaderboardPanel::new("Active Editors", 15)
            .with_rows(&rows)
            .with_max_rows(3);
        assert_eq!(panel.height(1.0), 32.0 + 3.0 * 16.0 + 8.0);

        panel.draw(
            &mut gfx,
            &mut list,
            Some(font),
            Vec2::new(10.0, 10.0),
            240.0,
            1.0,
        );
        assert!(!list.is_empty());

        // Without font
        let mut list2 = DrawList::new(UVec2::new(800, 600));
        panel.draw(
            &mut gfx,
            &mut list2,
            None,
            Vec2::new(10.0, 10.0),
            240.0,
            1.0,
        );
        assert!(!list2.is_empty());

        // Zero alpha
        let mut list3 = DrawList::new(UVec2::new(800, 600));
        panel.draw(
            &mut gfx,
            &mut list3,
            Some(font),
            Vec2::new(10.0, 10.0),
            240.0,
            0.0,
        );
        assert!(list3.is_empty());
    }

    #[test]
    fn test_dashboard_stack_lifecycle_and_drawing() {
        let mut gfx = Gfx::new();
        let face = gfx.fonts.default_face();
        let font = gfx.fonts.font(face, 12);
        let mut list = DrawList::new(UVec2::new(1000, 800));

        let mut stack = DashboardStack::default();
        assert!(stack.panels.is_empty());

        let spark = SparklinePanel::new("Lines", "12k").with_values(&[1.0, 2.0, 3.0]);
        let diffs = StackedDiffBarsPanel::new("Diffs", "+100 -50").with_diffs(&[(100, 50)]);
        let theseus = TheseusCohortAreaPanel::new("Cohorts");
        let editors = EditorsLeaderboardPanel::new("Editors", 5);

        stack.add_panel(DashboardPanel::Sparkline(spark));
        stack.add_panel(DashboardPanel::StackedDiffBars(diffs));
        stack.add_panel(DashboardPanel::TheseusCohort(theseus));
        stack.add_panel(DashboardPanel::EditorsLeaderboard(editors));
        assert_eq!(stack.panels.len(), 4);

        stack.draw(&mut gfx, &mut list, Some(font), 1000.0, 1.0);
        assert!(!list.is_empty());

        // Zero alpha skips
        stack.alpha = 0.0;
        let mut list2 = DrawList::new(UVec2::new(1000, 800));
        stack.draw(&mut gfx, &mut list2, Some(font), 1000.0, 1.0);
        assert!(list2.is_empty());

        // Clear stack
        stack.clear();
        assert!(stack.panels.is_empty());
    }
}
