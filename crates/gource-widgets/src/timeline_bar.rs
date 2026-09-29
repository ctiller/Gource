//! Bottom timeline scrubber, histogram, clip brackets, markers, and hover cards.
//!
//! Renders an interactive bottom timeline scrubber widget (`TimelineBarWidget`)
//! with commit-density & churn histogram, playhead, clip-in/clip-out range,
//! markers, and hover tooltip directly into [`DrawList`] using [`Gfx`].

use glam::{Vec2, Vec4};
use gource_core::bounds::Bounds2D;
use gource_draw::font::TextStyle;
use gource_draw::{DrawList, FontId, Gfx};

/// Default height of the timeline bar in virtual pixels before font scaling.
pub const TIMELINE_BAR_HEIGHT: f32 = 44.0;

/// Default margin from viewport left, right, and bottom edges.
pub const TIMELINE_BAR_MARGIN: f32 = 16.0;

/// Fade in/out speed in seconds.
pub const TIMELINE_FADE_TIME: f32 = 0.25;

/// Marker on the timeline bar.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineBarMarker {
    pub frac: f32,
    pub label: String,
    pub colour: Vec4,
}

impl TimelineBarMarker {
    pub fn new(frac: f32, label: impl Into<String>, colour: Vec4) -> Self {
        Self {
            frac: frac.clamp(0.0, 1.0),
            label: label.into(),
            colour,
        }
    }
}

/// Floating hover card detail when hovering over a timeline point.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineHoverCard {
    pub frac: f32,
    pub date: String,
    pub commits: u32,
    pub lines_added: u64,
    pub lines_removed: u64,
    pub top_editors: Vec<(String, Vec4, u32)>,
}

impl TimelineHoverCard {
    pub fn new(frac: f32, date: impl Into<String>, commits: u32) -> Self {
        Self {
            frac: frac.clamp(0.0, 1.0),
            date: date.into(),
            commits,
            lines_added: 0,
            lines_removed: 0,
            top_editors: Vec::new(),
        }
    }

    pub fn with_diff(mut self, added: u64, removed: u64) -> Self {
        self.lines_added = added;
        self.lines_removed = removed;
        self
    }

    pub fn with_top_editors(mut self, editors: &[(String, Vec4, u32)]) -> Self {
        self.top_editors = editors.to_vec();
        self
    }
}

/// Data payload passed into `TimelineBarWidget::draw`.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineBarData {
    /// Commit count and churn (lines changed) per histogram bucket.
    pub buckets: Vec<(u32, u32)>,
    /// Milestones/tags/markers along the timeline.
    pub markers: Vec<TimelineBarMarker>,
    /// Current playhead position in `[0.0, 1.0]`.
    pub playhead_frac: f32,
    /// Optional ghost seeking position when scrubbing or hovering.
    pub seeking_target_frac: Option<f32>,
    /// In-point clip bracket in `[0.0, 1.0]`.
    pub clip_in_frac: f32,
    /// Out-point clip bracket in `[0.0, 1.0]`.
    pub clip_out_frac: f32,
    /// Playback status badge (e.g. `"▶ 1.0x"`, `"⏸"`, `"◀ 2.0x"`).
    pub direction_label: String,
    /// Current simulation date string.
    pub current_date: String,
    /// Optional hover tooltip card.
    pub hover_info: Option<TimelineHoverCard>,
}

impl Default for TimelineBarData {
    fn default() -> Self {
        Self {
            buckets: Vec::new(),
            markers: Vec::new(),
            playhead_frac: 0.0,
            seeking_target_frac: None,
            clip_in_frac: 0.0,
            clip_out_frac: 1.0,
            direction_label: "▶ 1.0x".to_string(),
            current_date: String::new(),
            hover_info: None,
        }
    }
}

/// Result of hit-testing the timeline bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimelineHit {
    None,
    DirectionButton,
    ClipInHandle,
    ClipOutHandle,
    Marker(usize),
    Track(f32),
}

/// Timeline scrubber bar widget.
#[derive(Debug, Clone)]
pub struct TimelineBarWidget {
    pub font: FontId,
    pub font_scale: f32,
    pub bounds: Bounds2D,
    pub visible: bool,
    pub alpha: f32,
    pub fade_time: f32,
    pub hovered: bool,
    pub btn_width: f32,
    pub date_width: f32,
}

impl TimelineBarWidget {
    pub fn new(font: FontId, font_scale: f32) -> Self {
        Self {
            font,
            font_scale: font_scale.max(0.1),
            bounds: Bounds2D::new(),
            visible: true,
            alpha: 1.0,
            fade_time: TIMELINE_FADE_TIME,
            hovered: false,
            btn_width: 60.0,
            date_width: 90.0,
        }
    }

    pub fn set_bounds(&mut self, pos: Vec2, size: Vec2) {
        self.bounds.set_points(pos, pos + size);
    }

    pub fn resize(
        &mut self,
        viewport_width: u32,
        viewport_height: u32,
        font: FontId,
        font_scale: f32,
    ) {
        self.font = font;
        self.font_scale = font_scale.max(0.1);

        let h = TIMELINE_BAR_HEIGHT * self.font_scale;
        let margin = TIMELINE_BAR_MARGIN;
        let w = (viewport_width as f32 - margin * 2.0).max(10.0);
        let x = margin;
        let y = viewport_height as f32 - margin - h;

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
        let target = if self.visible || self.hovered {
            1.0
        } else {
            0.0
        };
        let step = (dt / self.fade_time.max(0.001)).clamp(0.0, 1.0);
        if self.alpha < target {
            self.alpha = (self.alpha + step).min(target);
        } else if self.alpha > target {
            self.alpha = (self.alpha - step).max(target);
        }
    }

    /// Calculate the horizontal track rectangle `(min_x, max_x, min_y, max_y)`.
    pub fn track_rect(&self) -> (f32, f32, f32, f32) {
        let left_badge_w = (self.btn_width + self.date_width + 16.0) * self.font_scale;
        let track_start_x = self.bounds.min.x + left_badge_w;
        let track_end_x = (self.bounds.max.x - 12.0 * self.font_scale).max(track_start_x + 10.0);
        let track_top_y = self.bounds.min.y + 8.0 * self.font_scale;
        let track_bottom_y = self.bounds.max.y - 8.0 * self.font_scale;
        (track_start_x, track_end_x, track_top_y, track_bottom_y)
    }

    /// Get normalized fraction `[0.0, 1.0]` corresponding to screen X coordinate.
    pub fn frac_at_x(&self, x: f32) -> f32 {
        let (track_start, track_end, _, _) = self.track_rect();
        let span = (track_end - track_start).max(1.0);
        ((x - track_start) / span).clamp(0.0, 1.0)
    }

    /// Get screen X coordinate for a normalized fraction `[0.0, 1.0]`.
    pub fn x_at_frac(&self, frac: f32) -> f32 {
        let (track_start, track_end, _, _) = self.track_rect();
        track_start + frac.clamp(0.0, 1.0) * (track_end - track_start)
    }

    /// Hit-test mouse position against interactive components.
    /// Hit-test mouse position against interactive components.
    pub fn hit_test(&self, mouse_pos: Vec2, data: Option<&TimelineBarData>) -> TimelineHit {
        if !self.bounds.contains(mouse_pos) || self.alpha <= 0.05 {
            return TimelineHit::None;
        }

        let (track_start, track_end, track_top, track_bottom) = self.track_rect();

        // 1. Check Direction Button badge
        let btn_w = self.btn_width * self.font_scale;
        let btn_min = self.bounds.min + Vec2::new(8.0 * self.font_scale, 6.0 * self.font_scale);
        let btn_max = Vec2::new(btn_min.x + btn_w, self.bounds.max.y - 6.0 * self.font_scale);
        if mouse_pos.x >= btn_min.x
            && mouse_pos.x <= btn_max.x
            && mouse_pos.y >= btn_min.y
            && mouse_pos.y <= btn_max.y
        {
            return TimelineHit::DirectionButton;
        }

        // 2. Check track area (handles, markers, or track scrub)
        if mouse_pos.x >= track_start - 6.0
            && mouse_pos.x <= track_end + 6.0
            && mouse_pos.y >= track_top - 6.0
            && mouse_pos.y <= track_bottom + 6.0
        {
            if let Some(d) = data {
                let in_x = self.x_at_frac(d.clip_in_frac);
                if (mouse_pos.x - in_x).abs() <= 6.0 {
                    return TimelineHit::ClipInHandle;
                }
                let out_x = self.x_at_frac(d.clip_out_frac);
                if (mouse_pos.x - out_x).abs() <= 6.0 {
                    return TimelineHit::ClipOutHandle;
                }
                for (i, m) in d.markers.iter().enumerate() {
                    let mx = self.x_at_frac(m.frac);
                    if (mouse_pos.x - mx).abs() <= 6.0 && mouse_pos.y <= track_top + 8.0 {
                        return TimelineHit::Marker(i);
                    }
                }
            }

            let frac = self.frac_at_x(mouse_pos.x);
            return TimelineHit::Track(frac);
        }

        TimelineHit::None
    }

    /// Draw the timeline bar widget.
    pub fn draw(&self, data: &TimelineBarData, gfx: &mut Gfx, list: &mut DrawList) {
        if self.alpha <= 0.0 || self.bounds.width() <= 0.0 || self.bounds.height() <= 0.0 {
            return;
        }

        let a = self.alpha;
        let pos = self.bounds.min;
        let size = Vec2::new(self.bounds.width(), self.bounds.height());

        // 1. Backdrop card with shadow and border
        let shadow_pos = pos + Vec2::new(2.0, 2.0);
        let shadow_col = Vec4::new(0.0, 0.0, 0.0, 0.45 * a);
        list.solid_rect(shadow_pos, size, shadow_col);

        let bg_col = Vec4::new(0.07, 0.08, 0.11, 0.88 * a);
        list.solid_rect(pos, size, bg_col);

        let border_col = Vec4::new(0.28, 0.32, 0.42, 0.65 * a);
        list.rect_outline(pos, pos + size, 1.0, border_col);

        // 2. Left badge: Playback status button
        let btn_w = self.btn_width * self.font_scale;
        let btn_pos = pos + Vec2::new(8.0 * self.font_scale, 6.0 * self.font_scale);
        let btn_h = size.y - 12.0 * self.font_scale;
        let btn_bg = Vec4::new(0.18, 0.22, 0.30, 0.85 * a);
        list.solid_rect(btn_pos, Vec2::new(btn_w, btn_h), btn_bg);
        list.rect_outline(
            btn_pos,
            btn_pos + Vec2::new(btn_w, btn_h),
            1.0,
            Vec4::new(0.4, 0.48, 0.65, 0.5 * a),
        );

        let btn_style = TextStyle::new(Vec4::new(0.95, 0.95, 1.0, a))
            .with_align_top(false)
            .with_align_right(false);
        let btn_text_pos = btn_pos + Vec2::new(6.0 * self.font_scale, btn_h * 0.7);
        gfx.draw_text(
            list,
            self.font,
            btn_text_pos,
            &data.direction_label,
            &btn_style,
        );

        // 3. Current Date text
        let date_pos = Vec2::new(
            btn_pos.x + btn_w + 10.0 * self.font_scale,
            btn_pos.y + btn_h * 0.7,
        );
        let date_style = TextStyle::new(Vec4::new(0.75, 0.82, 0.92, a))
            .with_align_top(false)
            .with_align_right(false);
        gfx.draw_text(list, self.font, date_pos, &data.current_date, &date_style);

        // 4. Histogram Track
        let (track_start, track_end, track_top, track_bottom) = self.track_rect();
        let track_w = track_end - track_start;
        let track_h = track_bottom - track_top;

        // Track background groove
        let groove_col = Vec4::new(0.04, 0.05, 0.07, 0.9 * a);
        list.solid_rect(
            Vec2::new(track_start, track_top),
            Vec2::new(track_w, track_h),
            groove_col,
        );
        list.rect_outline(
            Vec2::new(track_start, track_top),
            Vec2::new(track_end, track_bottom),
            1.0,
            Vec4::new(0.2, 0.24, 0.32, 0.5 * a),
        );

        // Dimmed area outside clips
        let in_x = self.x_at_frac(data.clip_in_frac);
        let out_x = self.x_at_frac(data.clip_out_frac);
        if in_x > track_start {
            list.solid_rect(
                Vec2::new(track_start, track_top),
                Vec2::new(in_x - track_start, track_h),
                Vec4::new(0.0, 0.0, 0.0, 0.45 * a),
            );
        }
        if out_x < track_end {
            list.solid_rect(
                Vec2::new(out_x, track_top),
                Vec2::new(track_end - out_x, track_h),
                Vec4::new(0.0, 0.0, 0.0, 0.45 * a),
            );
        }

        // Draw Histogram Bars
        if !data.buckets.is_empty() {
            let max_density = data.buckets.iter().map(|b| b.0).max().unwrap_or(0).max(1) as f32;
            let n = data.buckets.len();
            let bucket_w = track_w / n as f32;

            for (i, &(commits, _churn)) in data.buckets.iter().enumerate() {
                if commits == 0 {
                    continue;
                }
                let bx = track_start + i as f32 * bucket_w;
                let bar_h = ((commits as f32 / max_density) * track_h).max(1.0);
                let by = track_bottom - bar_h;

                let is_inside_clip = bx >= in_x - bucket_w && bx <= out_x;
                let bar_col = if is_inside_clip {
                    Vec4::new(0.25, 0.65, 0.95, 0.8 * a)
                } else {
                    Vec4::new(0.25, 0.65, 0.95, 0.25 * a)
                };

                list.solid_rect(
                    Vec2::new(bx, by),
                    Vec2::new(bucket_w.max(1.0), bar_h),
                    bar_col,
                );
            }
        }

        // 5. Markers along top edge of track
        for marker in &data.markers {
            let mx = self.x_at_frac(marker.frac);
            list.line(
                Vec2::new(mx, track_top),
                Vec2::new(mx, track_top + 6.0),
                1.5,
                marker.colour * Vec4::new(1.0, 1.0, 1.0, a),
            );
        }

        // 6. Clip-In and Clip-Out Brackets `[` and `]`
        let bracket_col = Vec4::new(0.95, 0.75, 0.25, 0.95 * a);
        // Clip-In '['
        list.line(
            Vec2::new(in_x, track_top - 2.0),
            Vec2::new(in_x, track_bottom + 2.0),
            2.0,
            bracket_col,
        );
        list.line(
            Vec2::new(in_x, track_top - 2.0),
            Vec2::new(in_x + 4.0, track_top - 2.0),
            1.5,
            bracket_col,
        );
        list.line(
            Vec2::new(in_x, track_bottom + 2.0),
            Vec2::new(in_x + 4.0, track_bottom + 2.0),
            1.5,
            bracket_col,
        );

        // Clip-Out ']'
        list.line(
            Vec2::new(out_x, track_top - 2.0),
            Vec2::new(out_x, track_bottom + 2.0),
            2.0,
            bracket_col,
        );
        list.line(
            Vec2::new(out_x - 4.0, track_top - 2.0),
            Vec2::new(out_x, track_top - 2.0),
            1.5,
            bracket_col,
        );
        list.line(
            Vec2::new(out_x - 4.0, track_bottom + 2.0),
            Vec2::new(out_x, track_bottom + 2.0),
            1.5,
            bracket_col,
        );

        // 7. Seeking Ghost Line
        if let Some(seek_frac) = data.seeking_target_frac {
            let sx = self.x_at_frac(seek_frac);
            list.line(
                Vec2::new(sx, track_top - 3.0),
                Vec2::new(sx, track_bottom + 3.0),
                1.0,
                Vec4::new(1.0, 1.0, 1.0, 0.45 * a),
            );
        }

        // 8. Bright Playhead Needle & Handle
        let px = self.x_at_frac(data.playhead_frac);
        let playhead_col = Vec4::new(1.0, 0.28, 0.28, a);
        list.line(
            Vec2::new(px, track_top - 4.0),
            Vec2::new(px, track_bottom + 4.0),
            2.0,
            playhead_col,
        );
        // Playhead top cap triangle/diamond
        let cap_size = 5.0 * self.font_scale;
        list.solid_rect(
            Vec2::new(px - cap_size * 0.5, track_top - 5.0 - cap_size),
            Vec2::new(cap_size, cap_size),
            playhead_col,
        );

        // 9. Floating Hover Card
        if let Some(card) = &data.hover_info {
            let hx = self.x_at_frac(card.frac);
            let card_w = 160.0 * self.font_scale;
            let card_h = 60.0 * self.font_scale;
            let card_x = (hx - card_w * 0.5).clamp(pos.x, pos.x + size.x - card_w);
            let card_y = pos.y - card_h - 8.0;

            // Card background & outline
            list.solid_rect(
                Vec2::new(card_x + 2.0, card_y + 2.0),
                Vec2::new(card_w, card_h),
                Vec4::new(0.0, 0.0, 0.0, 0.4 * a),
            );
            list.solid_rect(
                Vec2::new(card_x, card_y),
                Vec2::new(card_w, card_h),
                Vec4::new(0.08, 0.09, 0.12, 0.95 * a),
            );
            list.rect_outline(
                Vec2::new(card_x, card_y),
                Vec2::new(card_x + card_w, card_y + card_h),
                1.0,
                Vec4::new(0.4, 0.45, 0.55, 0.7 * a),
            );

            // Card text: Date
            let card_style = TextStyle::new(Vec4::new(0.9, 0.95, 1.0, a))
                .with_align_top(true)
                .with_align_right(false);
            gfx.draw_text(
                list,
                self.font,
                Vec2::new(card_x + 6.0, card_y + 4.0),
                &card.date,
                &card_style,
            );

            // Commits & changes
            let stats_text = format!("{} commits", card.commits);
            let stats_style = TextStyle::new(Vec4::new(0.7, 0.75, 0.85, a))
                .with_align_top(true)
                .with_align_right(false);
            gfx.draw_text(
                list,
                self.font,
                Vec2::new(card_x + 6.0, card_y + 20.0),
                &stats_text,
                &stats_style,
            );

            let diff_text = format!("+{} -{}", card.lines_added, card.lines_removed);
            let diff_style = TextStyle::new(Vec4::new(0.3, 0.85, 0.45, a))
                .with_align_top(true)
                .with_align_right(false);
            gfx.draw_text(
                list,
                self.font,
                Vec2::new(card_x + 6.0, card_y + 36.0),
                &diff_text,
                &diff_style,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::UVec2;

    #[test]
    fn test_timeline_bar_widget_resize_and_bounds() {
        let font = FontId(1);
        let mut widget = TimelineBarWidget::new(font, 1.0);
        assert!(widget.is_visible());
        assert_eq!(widget.alpha(), 1.0);

        widget.resize(1000, 800, font, 1.0);
        assert_eq!(widget.bounds.min.x, TIMELINE_BAR_MARGIN);
        assert_eq!(widget.bounds.max.y, 800.0 - TIMELINE_BAR_MARGIN);
        assert_eq!(widget.bounds.height(), TIMELINE_BAR_HEIGHT);

        let (t_start, t_end, t_top, t_bottom) = widget.track_rect();
        assert!(t_start > widget.bounds.min.x);
        assert!(t_end <= widget.bounds.max.x);
        assert!(t_top < t_bottom);
    }

    #[test]
    fn test_timeline_bar_widget_visibility_and_fade() {
        let mut widget = TimelineBarWidget::new(FontId(1), 1.0);
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
    fn test_timeline_bar_hit_test() {
        let mut widget = TimelineBarWidget::new(FontId(1), 1.0);
        widget.resize(1000, 800, FontId(1), 1.0);

        let data = TimelineBarData {
            clip_in_frac: 0.2,
            clip_out_frac: 0.8,
            markers: vec![TimelineBarMarker::new(0.5, "Tag", Vec4::ONE)],
            ..Default::default()
        };

        // Outside bounds
        assert_eq!(
            widget.hit_test(Vec2::new(10.0, 10.0), None),
            TimelineHit::None
        );

        // Direction button
        let btn_pos = widget.bounds.min + Vec2::new(15.0, 15.0);
        assert_eq!(widget.hit_test(btn_pos, None), TimelineHit::DirectionButton);

        // Clip-In Handle
        let in_x = widget.x_at_frac(0.2);
        let (_, _, t_top, t_bottom) = widget.track_rect();
        assert_eq!(
            widget.hit_test(Vec2::new(in_x, (t_top + t_bottom) * 0.5), Some(&data)),
            TimelineHit::ClipInHandle
        );

        // Clip-Out Handle
        let out_x = widget.x_at_frac(0.8);
        assert_eq!(
            widget.hit_test(Vec2::new(out_x, (t_top + t_bottom) * 0.5), Some(&data)),
            TimelineHit::ClipOutHandle
        );

        // Marker Hit
        let mx = widget.x_at_frac(0.5);
        assert_eq!(
            widget.hit_test(Vec2::new(mx, t_top + 2.0), Some(&data)),
            TimelineHit::Marker(0)
        );

        // Track area
        let track_mid = Vec2::new(widget.x_at_frac(0.4), (t_top + t_bottom) * 0.5);
        match widget.hit_test(track_mid, Some(&data)) {
            TimelineHit::Track(frac) => {
                assert!((frac - 0.4).abs() < 0.05);
            }
            other => panic!("expected Track hit, got {:?}", other),
        }
    }

    #[test]
    fn test_timeline_bar_drawing() {
        let mut gfx = Gfx::new();
        let face = gfx.fonts.default_face();
        let font = gfx.fonts.font(face, 12);

        let mut widget = TimelineBarWidget::new(font, 1.0);
        widget.resize(800, 600, font, 1.0);

        let data = TimelineBarData {
            buckets: vec![(10, 100), (0, 0), (25, 400), (5, 50)],
            markers: vec![TimelineBarMarker::new(0.25, "v1.0", Vec4::ONE)],
            playhead_frac: 0.4,
            seeking_target_frac: Some(0.45),
            clip_in_frac: 0.1,
            clip_out_frac: 0.9,
            current_date: "2024-03-15".to_string(),
            hover_info: Some(
                TimelineHoverCard::new(0.4, "2024-03-15", 42)
                    .with_diff(120, 30)
                    .with_top_editors(&[("Alice".to_string(), Vec4::ONE, 10)]),
            ),
            ..Default::default()
        };

        let mut list = DrawList::new(UVec2::new(800, 600));
        widget.draw(&data, &mut gfx, &mut list);
        assert!(!list.is_empty());

        // Zero alpha skips draw
        widget.alpha = 0.0;
        let mut list2 = DrawList::new(UVec2::new(800, 600));
        widget.draw(&data, &mut gfx, &mut list2);
        assert!(list2.is_empty());
    }
}
