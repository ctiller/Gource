//! HUD analytics and Git-of-Theseus dashboard display models (`DashboardVM`, [`SparklineVM`],
//! [`StackedDiffBarsVM`], [`TheseusCohortAreaVM`], [`EditorsLeaderboardVM`]).

use gource_core::Vec3;

/// Format an integer into compact human-readable representation:
/// e.g. `950 -> "950"`, `12_400 -> "12.4k"`, `1_500_000 -> "1.5M"`.
pub fn format_compact_u64(n: u64) -> String {
    if n >= 1_000_000_000 {
        let v = n as f64 / 1_000_000_000.0;
        if v >= 100.0 {
            format!("{v:.0}B")
        } else if v >= 10.0 {
            format!("{v:.1}B")
        } else {
            format!("{v:.2}B")
        }
    } else if n >= 1_000_000 {
        let v = n as f64 / 1_000_000.0;
        if v >= 100.0 {
            format!("{v:.0}M")
        } else {
            format!("{v:.1}M")
        }
    } else if n >= 10_000 {
        let v = n as f64 / 1_000.0;
        if v >= 100.0 {
            format!("{v:.0}k")
        } else {
            format!("{v:.1}k")
        }
    } else if n >= 1_000 {
        let v = n as f64 / 1_000.0;
        format!("{v:.1}k")
    } else {
        format!("{n}")
    }
}

/// Sparkline panel view model.
#[derive(Debug, Clone, PartialEq)]
pub struct SparklineVM {
    pub title: String,
    pub value_str: String,
    pub delta_str: Option<String>,
    pub delta_positive: bool,
    pub values: Vec<f32>,
    pub line_colour: Vec3,
    pub fill_alpha: f32,
    pub height: f32,
}

impl SparklineVM {
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
}

/// Stacked diff bars (`+` vs `-` around midline) panel view model.
#[derive(Debug, Clone, PartialEq)]
pub struct StackedDiffBarsVM {
    pub title: String,
    pub summary_str: String,
    pub diffs: Vec<(u64, u64)>,
    pub height: f32,
}

impl StackedDiffBarsVM {
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
}

/// Git-of-Theseus cohort survival stacked area chart panel view model.
#[derive(Debug, Clone, PartialEq)]
pub struct TheseusCohortAreaVM {
    pub title: String,
    pub cohort_labels: Vec<String>,
    pub cohort_colours: Vec<Vec3>,
    pub samples: Vec<Vec<u64>>,
    pub half_life_days: Option<f32>,
    pub churn_rate: Option<f32>,
    pub height: f32,
}

impl TheseusCohortAreaVM {
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
}

/// Editors leaderboard panel view model.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorsLeaderboardVM {
    pub title: String,
    pub active_editors_count: usize,
    pub rows: Vec<(String, Vec3, u32, u64)>,
    pub max_rows: usize,
    pub row_height: f32,
}

impl EditorsLeaderboardVM {
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
}

/// Any panel in the analytics dashboard stack.
#[derive(Debug, Clone, PartialEq)]
pub enum DashboardPanelVM {
    Sparkline(SparklineVM),
    StackedDiffBars(StackedDiffBarsVM),
    TheseusCohort(TheseusCohortAreaVM),
    EditorsLeaderboard(EditorsLeaderboardVM),
}

impl DashboardPanelVM {
    pub fn height(&self, font_scale: f32) -> f32 {
        match self {
            Self::Sparkline(p) => p.height(font_scale),
            Self::StackedDiffBars(p) => p.height(font_scale),
            Self::TheseusCohort(p) => p.height(font_scale),
            Self::EditorsLeaderboard(p) => p.height(font_scale),
        }
    }
}

/// Top-level display model for the HUD analytics dashboard stack.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct DashboardVM {
    pub visible: bool,
    pub panels: Vec<DashboardPanelVM>,
}
