//! Settings / tuning panel display model (`SettingsVM`, [`TuningTab`], [`TuningRowView`]).

use gource_core::Vec4;

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
            Self::Visual => Vec4::new(0.3, 0.7, 1.0, 1.0),
            Self::Dynamics => Vec4::new(0.95, 0.65, 0.2, 1.0),
            Self::Timeline => Vec4::new(0.35, 0.85, 0.45, 1.0),
            Self::Structural => Vec4::new(0.85, 0.4, 0.9, 1.0),
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

/// Display model for the settings / tuning side panel.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsVM {
    pub visible: bool,
    pub active_tab: TuningTab,
    pub rows: Vec<TuningRowView>,
}

impl Default for SettingsVM {
    fn default() -> Self {
        Self {
            visible: false,
            active_tab: TuningTab::Visual,
            rows: Vec::new(),
        }
    }
}
