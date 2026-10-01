//! Settings patch and interactive tuning metadata table.
//!
//! Categorises every interactive setting by its simulation impact class:
//! - Visual: Immediate next-frame visual change (no history / checkpoint invalidation).
//! - Dynamics: Physics / sizing change (applied from playhead; later checkpoints dropped).
//! - Timeline: Time-to-tick mapping change (applied from playhead; later checkpoints dropped).
//! - Structural: Log filter changes (requires scene rematerialization at playhead).

use crate::gource::{CameraMode, DashboardPeriod, FileColourMode, FileSizeMetric, GourceSettings};
use glam::{Vec3, Vec4};

/// Simulation impact classification of a setting change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SettingClass {
    /// Immediate next-frame visual changes (colours, bloom, fonts, hide flags, dashboards, pulse, colour mode). History & checkpoints unaffected.
    Visual,
    /// Physics & sizing changes (elasticity, user friction/speed, file idle time, max file lag, Tuning constants, file size metric). Applied from playhead; later checkpoints dropped.
    Dynamics,
    /// Time mapping changes (seconds per day, auto-skip, time scale, loop). Later checkpoints dropped.
    Timeline,
    /// Structural filter changes (user/file regex filters, show filters, max files, max user speed). Scene rematerialized at playhead.
    Structural,
}

/// Bitflag set of visible elements that can be hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct HideFlags {
    pub date: bool,
    pub users: bool,
    pub tree: bool,
    pub files: bool,
    pub usernames: bool,
    pub filenames: bool,
    pub dirnames: bool,
    pub progress: bool,
    pub bloom: bool,
    pub mouse: bool,
    pub root: bool,
    pub dashboards: bool,
}

impl HideFlags {
    pub fn from_settings(s: &GourceSettings) -> Self {
        Self {
            date: s.hide_date,
            users: s.hide_users,
            tree: s.hide_tree,
            files: s.hide_files,
            usernames: s.hide_usernames,
            filenames: s.hide_filenames,
            dirnames: s.hide_dirnames,
            progress: s.hide_progress,
            bloom: s.hide_bloom,
            mouse: s.hide_mouse,
            root: s.hide_root,
            dashboards: s.hide_dashboards,
        }
    }

    pub fn apply_to_settings(&self, s: &mut GourceSettings) {
        s.hide_date = self.date;
        s.hide_users = self.users;
        s.hide_tree = self.tree;
        s.hide_files = self.files;
        s.hide_usernames = self.usernames;
        s.hide_filenames = self.filenames;
        s.hide_dirnames = self.dirnames;
        s.hide_progress = self.progress;
        s.hide_bloom = self.bloom;
        s.hide_mouse = self.mouse;
        s.hide_root = self.root;
        s.hide_dashboards = self.dashboards;
    }
}

/// Mirrors the simulation's physics tuning parameters so they can be inspected,
/// patched, reset, and serialized.
#[derive(Debug, Clone, PartialEq)]
pub struct TuningSettings {
    pub dir_padding: f32,
    pub min_dir_size: f32,
    pub file_diameter: f32,
    pub gravity: f32,
    pub parent_pull: f32,
    pub sibling_push: f32,
    pub beam_length: f32,
    pub action_distance: f32,
    pub personal_space: f32,
    pub shadow_strength: f32,
}

impl Default for TuningSettings {
    fn default() -> Self {
        Self {
            dir_padding: 1.5,
            min_dir_size: 10.0,
            file_diameter: 8.0,
            gravity: -10.0,
            parent_pull: 2.0,
            sibling_push: 100.0,
            beam_length: 100.0,
            action_distance: 30.0,
            personal_space: 30.0,
            shadow_strength: 0.5,
        }
    }
}

/// Dynamically typed setting value for reading, patching, and reset.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingValue {
    Bool(bool),
    F32(f32),
    U32(u32),
    Usize(usize),
    Vec3(Vec3),
    Vec4(Vec4),
    String(String),
    OptionalString(Option<String>),
    CameraMode(CameraMode),
    FileSizeMetric(FileSizeMetric),
    FileColourMode(FileColourMode),
    DashboardPeriod(DashboardPeriod),
    HideFlags(HideFlags),
}

/// All interactive settings available for inspection and live patching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingId {
    // Visual
    BackgroundColour,
    DirColour,
    TextColour,
    HighlightColour,
    SelectionColour,
    BloomMultiplier,
    BloomIntensity,
    FontScale,
    Title,
    CameraMode,
    HideFlags,
    FilePulse,
    FileColourMode,
    HideDashboards,
    DashboardPeriod,
    DashboardWindowDays,

    // Dynamics
    Elasticity,
    UserFriction,
    UserSpeed,
    UserScale,
    FileIdleTime,
    MaxFileLag,
    FileSizeMetric,
    TuningDirPadding,
    TuningMinDirSize,
    TuningFileDiameter,
    TuningGravity,
    TuningParentPull,
    TuningSiblingPush,
    TuningBeamLength,
    TuningActionDistance,
    TuningPersonalSpace,
    TuningShadowStrength,

    // Timeline
    SecondsPerDay,
    AutoSkipSeconds,
    TimeScale,
    Loop,

    // Structural
    MaxFiles,
    FileFilterRegex,
    FileShowFilterRegex,
    UserFilterRegex,
    UserShowFilterRegex,
}

impl SettingId {
    /// Array of all `SettingId` variants.
    pub const ALL: &'static [SettingId] = &[
        SettingId::BackgroundColour,
        SettingId::DirColour,
        SettingId::TextColour,
        SettingId::HighlightColour,
        SettingId::SelectionColour,
        SettingId::BloomMultiplier,
        SettingId::BloomIntensity,
        SettingId::FontScale,
        SettingId::Title,
        SettingId::CameraMode,
        SettingId::HideFlags,
        SettingId::FilePulse,
        SettingId::FileColourMode,
        SettingId::HideDashboards,
        SettingId::DashboardPeriod,
        SettingId::DashboardWindowDays,
        SettingId::Elasticity,
        SettingId::UserFriction,
        SettingId::UserSpeed,
        SettingId::UserScale,
        SettingId::FileIdleTime,
        SettingId::MaxFileLag,
        SettingId::FileSizeMetric,
        SettingId::TuningDirPadding,
        SettingId::TuningMinDirSize,
        SettingId::TuningFileDiameter,
        SettingId::TuningGravity,
        SettingId::TuningParentPull,
        SettingId::TuningSiblingPush,
        SettingId::TuningBeamLength,
        SettingId::TuningActionDistance,
        SettingId::TuningPersonalSpace,
        SettingId::TuningShadowStrength,
        SettingId::SecondsPerDay,
        SettingId::AutoSkipSeconds,
        SettingId::TimeScale,
        SettingId::Loop,
        SettingId::MaxFiles,
        SettingId::FileFilterRegex,
        SettingId::FileShowFilterRegex,
        SettingId::UserFilterRegex,
        SettingId::UserShowFilterRegex,
    ];

    pub fn all() -> &'static [SettingId] {
        Self::ALL
    }

    /// Classification of the setting.
    pub fn class(&self) -> SettingClass {
        match self {
            SettingId::BackgroundColour
            | SettingId::DirColour
            | SettingId::TextColour
            | SettingId::HighlightColour
            | SettingId::SelectionColour
            | SettingId::BloomMultiplier
            | SettingId::BloomIntensity
            | SettingId::FontScale
            | SettingId::Title
            | SettingId::CameraMode
            | SettingId::HideFlags
            | SettingId::FilePulse
            | SettingId::FileColourMode
            | SettingId::HideDashboards
            | SettingId::DashboardPeriod
            | SettingId::DashboardWindowDays => SettingClass::Visual,

            SettingId::Elasticity
            | SettingId::UserFriction
            | SettingId::UserSpeed
            | SettingId::UserScale
            | SettingId::FileIdleTime
            | SettingId::MaxFileLag
            | SettingId::FileSizeMetric
            | SettingId::TuningDirPadding
            | SettingId::TuningMinDirSize
            | SettingId::TuningFileDiameter
            | SettingId::TuningGravity
            | SettingId::TuningParentPull
            | SettingId::TuningSiblingPush
            | SettingId::TuningBeamLength
            | SettingId::TuningActionDistance
            | SettingId::TuningPersonalSpace
            | SettingId::TuningShadowStrength => SettingClass::Dynamics,

            SettingId::SecondsPerDay
            | SettingId::AutoSkipSeconds
            | SettingId::TimeScale
            | SettingId::Loop => SettingClass::Timeline,

            SettingId::MaxFiles
            | SettingId::FileFilterRegex
            | SettingId::FileShowFilterRegex
            | SettingId::UserFilterRegex
            | SettingId::UserShowFilterRegex => SettingClass::Structural,
        }
    }

    /// Human-readable setting name.
    pub fn name(&self) -> &'static str {
        match self {
            SettingId::BackgroundColour => "Background Colour",
            SettingId::DirColour => "Directory Colour",
            SettingId::TextColour => "Text Colour",
            SettingId::HighlightColour => "Highlight Colour",
            SettingId::SelectionColour => "Selection Colour",
            SettingId::BloomMultiplier => "Bloom Multiplier",
            SettingId::BloomIntensity => "Bloom Intensity",
            SettingId::FontScale => "Font Scale",
            SettingId::Title => "Title",
            SettingId::CameraMode => "Camera Mode",
            SettingId::HideFlags => "Hide Flags",
            SettingId::FilePulse => "File Pulse",
            SettingId::FileColourMode => "File Colour Mode",
            SettingId::HideDashboards => "Hide Dashboards",
            SettingId::DashboardPeriod => "Dashboard Period",
            SettingId::DashboardWindowDays => "Dashboard Window Days",

            SettingId::Elasticity => "Elasticity",
            SettingId::UserFriction => "User Friction",
            SettingId::UserSpeed => "Max User Speed",
            SettingId::UserScale => "User Scale",
            SettingId::FileIdleTime => "File Idle Time",
            SettingId::MaxFileLag => "Max File Lag",
            SettingId::FileSizeMetric => "File Size Metric",
            SettingId::TuningDirPadding => "Directory Padding",
            SettingId::TuningMinDirSize => "Minimum Directory Size",
            SettingId::TuningFileDiameter => "File Diameter",
            SettingId::TuningGravity => "Gravity",
            SettingId::TuningParentPull => "Parent Pull",
            SettingId::TuningSiblingPush => "Sibling Push",
            SettingId::TuningBeamLength => "Beam Length",
            SettingId::TuningActionDistance => "Action Distance",
            SettingId::TuningPersonalSpace => "Personal Space",
            SettingId::TuningShadowStrength => "Shadow Strength",

            SettingId::SecondsPerDay => "Seconds Per Day",
            SettingId::AutoSkipSeconds => "Auto Skip Seconds",
            SettingId::TimeScale => "Time Scale",
            SettingId::Loop => "Loop",

            SettingId::MaxFiles => "Max Files",
            SettingId::FileFilterRegex => "File Filter Regex",
            SettingId::FileShowFilterRegex => "File Show Filter Regex",
            SettingId::UserFilterRegex => "User Filter Regex",
            SettingId::UserShowFilterRegex => "User Show Filter Regex",
        }
    }

    /// Associated CLI flag name, if one exists.
    /// Associated CLI flag name, if one exists.
    pub fn cli_flag(&self) -> Option<&'static str> {
        crate::descriptor::cli_flag_for_setting_id(*self)
    }

    /// Numeric range (min, max, step) for sliders / spin controls in the UI.
    pub fn numeric_range(&self) -> Option<(f32, f32, f32)> {
        match self {
            SettingId::BloomMultiplier => Some((0.0, 5.0, 0.1)),
            SettingId::BloomIntensity => Some((0.0, 2.0, 0.05)),
            SettingId::FontScale => Some((0.1, 4.0, 0.05)),
            SettingId::FilePulse => Some((0.0, 10.0, 0.1)),
            SettingId::DashboardWindowDays => Some((1.0, 365.0, 1.0)),

            SettingId::Elasticity => Some((0.0, 1.0, 0.01)),
            SettingId::UserFriction => Some((0.1, 10.0, 0.1)),
            SettingId::UserSpeed => Some((10.0, 2000.0, 10.0)),
            SettingId::UserScale => Some((0.1, 10.0, 0.1)),
            SettingId::FileIdleTime => Some((0.0, 60.0, 0.5)),
            SettingId::MaxFileLag => Some((0.1, 60.0, 0.5)),

            SettingId::TuningDirPadding => Some((0.1, 10.0, 0.1)),
            SettingId::TuningMinDirSize => Some((1.0, 100.0, 1.0)),
            SettingId::TuningFileDiameter => Some((1.0, 40.0, 0.5)),
            SettingId::TuningGravity => Some((-100.0, 100.0, 1.0)),
            SettingId::TuningParentPull => Some((0.0, 20.0, 0.2)),
            SettingId::TuningSiblingPush => Some((0.0, 1000.0, 10.0)),
            SettingId::TuningBeamLength => Some((10.0, 500.0, 5.0)),
            SettingId::TuningActionDistance => Some((1.0, 200.0, 2.0)),
            SettingId::TuningPersonalSpace => Some((0.0, 200.0, 2.0)),
            SettingId::TuningShadowStrength => Some((0.0, 1.0, 0.05)),

            SettingId::SecondsPerDay => Some((0.01, 60.0, 0.1)),
            SettingId::AutoSkipSeconds => Some((0.0, 60.0, 0.5)),
            SettingId::TimeScale => Some((0.1, 4.0, 0.1)),

            SettingId::MaxFiles => Some((0.0, 100000.0, 100.0)),

            _ => None,
        }
    }

    /// Read the current value of this setting.
    pub fn read(&self, settings: &GourceSettings, tuning: &TuningSettings) -> SettingValue {
        match self {
            SettingId::BackgroundColour => SettingValue::Vec3(settings.background_colour),
            SettingId::DirColour => SettingValue::Vec3(settings.dir_colour),
            SettingId::TextColour => SettingValue::Vec3(settings.font_colour),
            SettingId::HighlightColour => SettingValue::Vec3(settings.highlight_colour),
            SettingId::SelectionColour => SettingValue::Vec3(settings.selection_colour),
            SettingId::BloomMultiplier => SettingValue::F32(settings.bloom_multiplier),
            SettingId::BloomIntensity => SettingValue::F32(settings.bloom_intensity),
            SettingId::FontScale => SettingValue::F32(settings.font_scale),
            SettingId::Title => SettingValue::String(settings.title.clone()),
            SettingId::CameraMode => SettingValue::CameraMode(settings.camera_mode),
            SettingId::HideFlags => SettingValue::HideFlags(HideFlags::from_settings(settings)),
            SettingId::FilePulse => SettingValue::F32(settings.file_pulse),
            SettingId::FileColourMode => SettingValue::FileColourMode(settings.file_colour_mode),
            SettingId::HideDashboards => SettingValue::Bool(settings.hide_dashboards),
            SettingId::DashboardPeriod => SettingValue::DashboardPeriod(settings.dashboard_period),
            SettingId::DashboardWindowDays => SettingValue::U32(settings.dashboard_window_days),

            SettingId::Elasticity => SettingValue::F32(settings.elasticity),
            SettingId::UserFriction => {
                let f = if settings.user_friction > 0.0 {
                    1.0 / settings.user_friction
                } else {
                    1.0
                };
                SettingValue::F32(f)
            }
            SettingId::UserSpeed => SettingValue::F32(settings.max_user_speed),
            SettingId::UserScale => SettingValue::F32(settings.user_scale),
            SettingId::FileIdleTime => SettingValue::F32(settings.file_idle_time),
            SettingId::MaxFileLag => SettingValue::F32(settings.max_file_lag),
            SettingId::FileSizeMetric => SettingValue::FileSizeMetric(settings.file_size_metric),
            SettingId::TuningDirPadding => SettingValue::F32(tuning.dir_padding),
            SettingId::TuningMinDirSize => SettingValue::F32(tuning.min_dir_size),
            SettingId::TuningFileDiameter => SettingValue::F32(tuning.file_diameter),
            SettingId::TuningGravity => SettingValue::F32(tuning.gravity),
            SettingId::TuningParentPull => SettingValue::F32(tuning.parent_pull),
            SettingId::TuningSiblingPush => SettingValue::F32(tuning.sibling_push),
            SettingId::TuningBeamLength => SettingValue::F32(tuning.beam_length),
            SettingId::TuningActionDistance => SettingValue::F32(tuning.action_distance),
            SettingId::TuningPersonalSpace => SettingValue::F32(tuning.personal_space),
            SettingId::TuningShadowStrength => SettingValue::F32(tuning.shadow_strength),

            SettingId::SecondsPerDay => {
                let spd = if settings.days_per_second > 0.0 {
                    1.0 / settings.days_per_second
                } else {
                    10.0
                };
                SettingValue::F32(spd)
            }
            SettingId::AutoSkipSeconds => SettingValue::F32(settings.auto_skip_seconds),
            SettingId::TimeScale => SettingValue::F32(settings.time_scale),
            SettingId::Loop => SettingValue::Bool(settings.looping),

            SettingId::MaxFiles => SettingValue::U32(settings.max_files.max(0) as u32),
            SettingId::FileFilterRegex => SettingValue::String(
                settings
                    .file_filters
                    .iter()
                    .map(|r| r.as_str().to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            SettingId::FileShowFilterRegex => SettingValue::String(
                settings
                    .file_show_filters
                    .iter()
                    .map(|r| r.as_str().to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            SettingId::UserFilterRegex => SettingValue::String(
                settings
                    .user_filters
                    .iter()
                    .map(|r| r.as_str().to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            SettingId::UserShowFilterRegex => SettingValue::String(
                settings
                    .user_show_filters
                    .iter()
                    .map(|r| r.as_str().to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        }
    }

    /// Apply a value to settings/tuning. Returns `true` if the value actually changed.
    pub fn apply(
        &self,
        val: &SettingValue,
        settings: &mut GourceSettings,
        tuning: &mut TuningSettings,
    ) -> bool {
        let cur = self.read(settings, tuning);
        if &cur == val {
            return false;
        }

        match (self, val) {
            (SettingId::BackgroundColour, SettingValue::Vec3(v)) => {
                settings.background_colour = *v;
            }
            (SettingId::DirColour, SettingValue::Vec3(v)) => {
                settings.dir_colour = *v;
            }
            (SettingId::TextColour, SettingValue::Vec3(v)) => {
                settings.font_colour = *v;
            }
            (SettingId::HighlightColour, SettingValue::Vec3(v)) => {
                settings.highlight_colour = *v;
            }
            (SettingId::SelectionColour, SettingValue::Vec3(v)) => {
                settings.selection_colour = *v;
            }
            (SettingId::BloomMultiplier, SettingValue::F32(v)) => {
                settings.bloom_multiplier = *v;
            }
            (SettingId::BloomIntensity, SettingValue::F32(v)) => {
                settings.bloom_intensity = *v;
            }
            (SettingId::FontScale, SettingValue::F32(v)) => {
                settings.font_scale = *v;
                settings.default_font_scale = false;
                settings.set_scaled_font_sizes();
            }
            (SettingId::Title, SettingValue::String(s)) => {
                settings.title = s.clone();
            }
            (SettingId::CameraMode, SettingValue::CameraMode(cm)) => {
                settings.camera_mode = *cm;
            }
            (SettingId::HideFlags, SettingValue::HideFlags(hf)) => {
                hf.apply_to_settings(settings);
            }
            (SettingId::FilePulse, SettingValue::F32(v)) => {
                settings.file_pulse = *v;
            }
            (SettingId::FileColourMode, SettingValue::FileColourMode(m)) => {
                settings.file_colour_mode = *m;
            }
            (SettingId::HideDashboards, SettingValue::Bool(b)) => {
                settings.hide_dashboards = *b;
            }
            (SettingId::DashboardPeriod, SettingValue::DashboardPeriod(dp)) => {
                settings.dashboard_period = *dp;
            }
            (SettingId::DashboardWindowDays, SettingValue::U32(days)) => {
                settings.dashboard_window_days = *days;
            }

            (SettingId::Elasticity, SettingValue::F32(v)) => {
                settings.elasticity = *v;
            }
            (SettingId::UserFriction, SettingValue::F32(v)) => {
                if *v > 0.0 {
                    settings.user_friction = 1.0 / *v;
                }
            }
            (SettingId::UserSpeed, SettingValue::F32(v)) => {
                settings.max_user_speed = *v;
            }
            (SettingId::UserScale, SettingValue::F32(v)) => {
                settings.user_scale = *v;
            }
            (SettingId::FileIdleTime, SettingValue::F32(v)) => {
                settings.file_idle_time = *v;
            }
            (SettingId::MaxFileLag, SettingValue::F32(v)) => {
                settings.max_file_lag = *v;
            }
            (SettingId::FileSizeMetric, SettingValue::FileSizeMetric(m)) => {
                settings.file_size_metric = *m;
            }
            (SettingId::TuningDirPadding, SettingValue::F32(v)) => {
                tuning.dir_padding = *v;
            }
            (SettingId::TuningMinDirSize, SettingValue::F32(v)) => {
                tuning.min_dir_size = *v;
            }
            (SettingId::TuningFileDiameter, SettingValue::F32(v)) => {
                tuning.file_diameter = *v;
            }
            (SettingId::TuningGravity, SettingValue::F32(v)) => {
                tuning.gravity = *v;
            }
            (SettingId::TuningParentPull, SettingValue::F32(v)) => {
                tuning.parent_pull = *v;
            }
            (SettingId::TuningSiblingPush, SettingValue::F32(v)) => {
                tuning.sibling_push = *v;
            }
            (SettingId::TuningBeamLength, SettingValue::F32(v)) => {
                tuning.beam_length = *v;
            }
            (SettingId::TuningActionDistance, SettingValue::F32(v)) => {
                tuning.action_distance = *v;
            }
            (SettingId::TuningPersonalSpace, SettingValue::F32(v)) => {
                tuning.personal_space = *v;
            }
            (SettingId::TuningShadowStrength, SettingValue::F32(v)) => {
                tuning.shadow_strength = *v;
            }

            (SettingId::SecondsPerDay, SettingValue::F32(v)) => {
                if *v > 0.0 {
                    settings.days_per_second = 1.0 / *v;
                }
            }
            (SettingId::AutoSkipSeconds, SettingValue::F32(v)) => {
                settings.auto_skip_seconds = *v;
            }
            (SettingId::TimeScale, SettingValue::F32(v)) => {
                settings.time_scale = *v;
            }
            (SettingId::Loop, SettingValue::Bool(b)) => {
                settings.looping = *b;
            }

            (SettingId::MaxFiles, SettingValue::U32(v)) => {
                settings.max_files = *v as i32;
            }
            (SettingId::FileFilterRegex, SettingValue::String(s)) => {
                settings.file_filters.clear();
                for part in s.split(',').filter(|p| !p.trim().is_empty()) {
                    if let Ok(re) = fancy_regex::Regex::new(part.trim()) {
                        settings.file_filters.push(re);
                    }
                }
            }
            (SettingId::FileShowFilterRegex, SettingValue::String(s)) => {
                settings.file_show_filters.clear();
                for part in s.split(',').filter(|p| !p.trim().is_empty()) {
                    if let Ok(re) = fancy_regex::Regex::new(part.trim()) {
                        settings.file_show_filters.push(re);
                    }
                }
            }
            (SettingId::UserFilterRegex, SettingValue::String(s)) => {
                settings.user_filters.clear();
                for part in s.split(',').filter(|p| !p.trim().is_empty()) {
                    if let Ok(re) = fancy_regex::Regex::new(part.trim()) {
                        settings.user_filters.push(re);
                    }
                }
            }
            (SettingId::UserShowFilterRegex, SettingValue::String(s)) => {
                settings.user_show_filters.clear();
                for part in s.split(',').filter(|p| !p.trim().is_empty()) {
                    if let Ok(re) = fancy_regex::Regex::new(part.trim()) {
                        settings.user_show_filters.push(re);
                    }
                }
            }

            _ => return false,
        }

        true
    }

    /// Reset this setting to its default value. Returns `true` if it changed.
    pub fn reset(&self, settings: &mut GourceSettings, tuning: &mut TuningSettings) -> bool {
        let def_settings = GourceSettings::default();
        let def_tuning = TuningSettings::default();
        let def_val = self.read(&def_settings, &def_tuning);
        self.apply(&def_val, settings, tuning)
    }
}

/// A collection of modifications to interactive settings.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SettingsPatch {
    pub changes: Vec<(SettingId, SettingValue)>,
}

impl SettingsPatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, id: SettingId, val: SettingValue) {
        self.changes.push((id, val));
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Returns the highest impact class among the changes, ordered:
    /// `Structural > Dynamics > Timeline > Visual`.
    pub fn highest_impact_class(&self) -> Option<SettingClass> {
        let mut highest: Option<SettingClass> = None;
        for (id, _) in &self.changes {
            let class = id.class();
            match (highest, class) {
                (None, c) => highest = Some(c),
                (Some(h), c) => {
                    let score = |cls: SettingClass| match cls {
                        SettingClass::Visual => 0,
                        SettingClass::Timeline => 1,
                        SettingClass::Dynamics => 2,
                        SettingClass::Structural => 3,
                    };
                    if score(c) > score(h) {
                        highest = Some(c);
                    }
                }
            }
        }
        highest
    }

    /// Returns true if applying this patch requires dropping checkpoints past the playhead.
    pub fn invalidates_checkpoints(&self) -> bool {
        matches!(
            self.highest_impact_class(),
            Some(SettingClass::Structural | SettingClass::Dynamics | SettingClass::Timeline)
        )
    }

    /// Returns true if applying this patch requires rematerializing the scene tree at playhead.
    pub fn requires_rematerialize(&self) -> bool {
        matches!(self.highest_impact_class(), Some(SettingClass::Structural))
    }

    /// Returns true if this patch remaps the timeline tick-to-time mapping.
    pub fn remaps_timeline(&self) -> bool {
        matches!(self.highest_impact_class(), Some(SettingClass::Timeline))
    }

    /// Applies all changes in the patch and returns deduplicated classes of settings that actually changed.
    pub fn apply(
        &self,
        settings: &mut GourceSettings,
        tuning: &mut TuningSettings,
    ) -> Vec<SettingClass> {
        let mut changed_classes = Vec::new();
        for (id, val) in &self.changes {
            if id.apply(val, settings, tuning) {
                let cls = id.class();
                if !changed_classes.contains(&cls) {
                    changed_classes.push(cls);
                }
            }
        }
        changed_classes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gource::DashboardPanel;

    #[test]
    fn test_setting_id_all() {
        let all = SettingId::all();
        assert_eq!(all.len(), 42);
        for id in all {
            assert!(!id.name().is_empty());
            let _ = id.class();
            let _ = id.cli_flag();
            let _ = id.numeric_range();
        }
    }

    #[test]
    fn test_read_apply_reset_all_settings() {
        let mut settings = GourceSettings::default();
        let mut tuning = TuningSettings::default();

        for id in SettingId::all() {
            let initial_val = id.read(&settings, &tuning);

            // Applying the same value should return false (no change)
            assert!(!id.apply(&initial_val, &mut settings, &mut tuning));

            // Generate an alternative test value
            let alt_val = match &initial_val {
                SettingValue::Bool(b) => SettingValue::Bool(!b),
                SettingValue::F32(f) => SettingValue::F32(f + 1.0),
                SettingValue::U32(u) => SettingValue::U32(u + 5),
                SettingValue::Usize(u) => SettingValue::Usize(u + 1),
                SettingValue::Vec3(v) => SettingValue::Vec3(*v + Vec3::splat(0.1)),
                SettingValue::Vec4(v) => SettingValue::Vec4(*v + Vec4::splat(0.1)),
                SettingValue::String(s) => SettingValue::String(format!("{s}_alt")),
                SettingValue::OptionalString(_) => {
                    SettingValue::OptionalString(Some("test".to_string()))
                }
                SettingValue::CameraMode(CameraMode::Overview) => {
                    SettingValue::CameraMode(CameraMode::Track)
                }
                SettingValue::CameraMode(CameraMode::Track) => {
                    SettingValue::CameraMode(CameraMode::Overview)
                }
                SettingValue::FileSizeMetric(FileSizeMetric::None) => {
                    SettingValue::FileSizeMetric(FileSizeMetric::Lines)
                }
                SettingValue::FileSizeMetric(_) => {
                    SettingValue::FileSizeMetric(FileSizeMetric::None)
                }
                SettingValue::FileColourMode(FileColourMode::Extension) => {
                    SettingValue::FileColourMode(FileColourMode::Churn)
                }
                SettingValue::FileColourMode(_) => {
                    SettingValue::FileColourMode(FileColourMode::Extension)
                }
                SettingValue::DashboardPeriod(DashboardPeriod::Day) => {
                    SettingValue::DashboardPeriod(DashboardPeriod::Year)
                }
                SettingValue::DashboardPeriod(_) => {
                    SettingValue::DashboardPeriod(DashboardPeriod::Day)
                }
                SettingValue::HideFlags(hf) => {
                    let mut modified = *hf;
                    modified.date = !modified.date;
                    SettingValue::HideFlags(modified)
                }
            };

            // Applying different value should return true
            assert!(id.apply(&alt_val, &mut settings, &mut tuning));
            let read_back = id.read(&settings, &tuning);
            assert_eq!(read_back, alt_val);

            // Resetting should return true (since it was changed from default)
            assert!(id.reset(&mut settings, &mut tuning));
            let reset_back = id.read(&settings, &tuning);
            assert_eq!(reset_back, initial_val);

            // Resetting again should return false (already at default)
            assert!(!id.reset(&mut settings, &mut tuning));
        }
    }

    #[test]
    fn test_patch_classification_and_impact() {
        let mut patch = SettingsPatch::new();
        assert!(patch.is_empty());
        assert_eq!(patch.highest_impact_class(), None);
        assert!(!patch.invalidates_checkpoints());
        assert!(!patch.requires_rematerialize());
        assert!(!patch.remaps_timeline());

        patch.push(
            SettingId::BackgroundColour,
            SettingValue::Vec3(Vec3::new(1.0, 0.0, 0.0)),
        );
        assert!(!patch.is_empty());
        assert_eq!(patch.highest_impact_class(), Some(SettingClass::Visual));
        assert!(!patch.invalidates_checkpoints());
        assert!(!patch.requires_rematerialize());
        assert!(!patch.remaps_timeline());

        patch.push(SettingId::SecondsPerDay, SettingValue::F32(20.0));
        assert_eq!(patch.highest_impact_class(), Some(SettingClass::Timeline));
        assert!(patch.invalidates_checkpoints());
        assert!(!patch.requires_rematerialize());
        assert!(patch.remaps_timeline());

        patch.push(SettingId::Elasticity, SettingValue::F32(0.5));
        assert_eq!(patch.highest_impact_class(), Some(SettingClass::Dynamics));
        assert!(patch.invalidates_checkpoints());
        assert!(!patch.requires_rematerialize());
        assert!(!patch.remaps_timeline());

        patch.push(SettingId::MaxFiles, SettingValue::U32(500));
        assert_eq!(patch.highest_impact_class(), Some(SettingClass::Structural));
        assert!(patch.invalidates_checkpoints());
        assert!(patch.requires_rematerialize());
        assert!(!patch.remaps_timeline());

        let mut settings = GourceSettings::default();
        let mut tuning = TuningSettings::default();
        let classes = patch.apply(&mut settings, &mut tuning);
        assert_eq!(
            classes,
            vec![
                SettingClass::Visual,
                SettingClass::Timeline,
                SettingClass::Dynamics,
                SettingClass::Structural
            ]
        );

        // Applying the same patch again produces no changes
        let classes2 = patch.apply(&mut settings, &mut tuning);
        assert!(classes2.is_empty());
    }

    #[test]
    fn test_gource_settings_to_cli_args() {
        let def = GourceSettings::default();
        assert!(def.to_cli_args().is_empty());

        let s = GourceSettings {
            background_colour: Vec3::new(1.0, 0.0, 0.0),
            bloom_multiplier: 2.0,
            bloom_intensity: 0.5,
            font_scale: 1.5,
            title: "My Project".to_string(),
            camera_mode: CameraMode::Track,
            hide_date: true,
            hide_users: true,
            file_pulse: 1.2,
            file_colour_mode: FileColourMode::Churn,
            dashboards: vec![DashboardPanel::Lines, DashboardPanel::Churn],
            dashboard_period: DashboardPeriod::Month,
            dashboard_window_days: 14,
            elasticity: 0.3,
            user_friction: 0.5,
            max_user_speed: 300.0,
            user_scale: 1.5,
            file_idle_time: 5.0,
            max_file_lag: 10.0,
            file_size_metric: FileSizeMetric::Lines,
            days_per_second: 0.2,
            auto_skip_seconds: 5.0,
            time_scale: 2.0,
            looping: true,
            max_files: 1000,
            output_stats_filename: "stats.json".to_string(),
            cache_dir: "/tmp/cache".to_string(),
            no_cache: true,
            seed: Some(999),
            path: "repo".to_string(),
            default_path: false,
            ..Default::default()
        };

        let args = s.to_cli_args();
        assert!(args.contains(&"--background-colour".to_string()));
        assert!(args.contains(&"ff0000".to_string()));
        assert!(args.contains(&"--bloom-multiplier".to_string()));
        assert!(args.contains(&"--bloom-intensity".to_string()));
        assert!(args.contains(&"--font-scale".to_string()));
        assert!(args.contains(&"--title".to_string()));
        assert!(args.contains(&"My Project".to_string()));
        assert!(args.contains(&"--camera-mode".to_string()));
        assert!(args.contains(&"track".to_string()));
        assert!(args.contains(&"--hide".to_string()));
        assert!(args.contains(&"date,users".to_string()));
        assert!(args.contains(&"--file-pulse".to_string()));
        assert!(args.contains(&"--file-colour-mode".to_string()));
        assert!(args.contains(&"--dashboard".to_string()));
        assert!(args.contains(&"lines,churn".to_string()));
        assert!(args.contains(&"--dashboard-period".to_string()));
        assert!(args.contains(&"month".to_string()));
        assert!(args.contains(&"--dashboard-window".to_string()));
        assert!(args.contains(&"14d".to_string()));
        assert!(args.contains(&"--elasticity".to_string()));
        assert!(args.contains(&"--user-friction".to_string()));
        assert!(args.contains(&"--max-user-speed".to_string()));
        assert!(args.contains(&"--user-scale".to_string()));
        assert!(args.contains(&"--file-idle-time".to_string()));
        assert!(args.contains(&"--max-file-lag".to_string()));
        assert!(args.contains(&"--file-size-metric".to_string()));
        assert!(args.contains(&"lines".to_string()));
        assert!(args.contains(&"--seconds-per-day".to_string()));
        assert!(args.contains(&"--auto-skip-seconds".to_string()));
        assert!(args.contains(&"--time-scale".to_string()));
        assert!(args.contains(&"--loop".to_string()));
        assert!(args.contains(&"--max-files".to_string()));
        assert!(args.contains(&"--output-stats".to_string()));
        assert!(args.contains(&"--cache-dir".to_string()));
        assert!(args.contains(&"--no-cache".to_string()));
        assert!(args.contains(&"--seed".to_string()));
        assert!(args.contains(&"999".to_string()));
        assert!(args.contains(&"repo".to_string()));
    }
}
