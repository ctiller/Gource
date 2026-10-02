//! Timeline display model (`TimelineVM` / [`TimelineBarData`]).

use gource_core::Vec4;

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

/// Display model for the bottom timeline scrubber (`TimelineVM`).
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
    /// Whether live mode is enabled.
    pub is_live: bool,
    /// Whether the playhead is currently pinned to the live edge.
    pub at_live_edge: bool,
    /// Optional hover tooltip card.
    pub hover_info: Option<TimelineHoverCard>,
}

/// Canonical alias for [`TimelineBarData`].
pub type TimelineVM = TimelineBarData;

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
            is_live: false,
            at_live_edge: false,
            hover_info: None,
        }
    }
}
