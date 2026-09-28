//! Gource settings (port of `GourceSettings` in gource_settings.{h,cpp}).

use crate::SettingsError;
use crate::conffile::{ConfEntry, ConfFile, ConfSection};
use glam::{Vec2, Vec3};
use std::collections::BTreeMap;
use std::path::Path;

/// `--log-level` (C++ `logger_level`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum LogLevel {
    #[default]
    Off,
    Error,
    Console,
    Info,
    Script,
    Debug,
    Warn,
    Pedantic,
}

impl LogLevel {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "warn" => Some(LogLevel::Warn),
            "debug" => Some(LogLevel::Debug),
            "info" => Some(LogLevel::Info),
            "error" => Some(LogLevel::Error),
            "pedantic" => Some(LogLevel::Pedantic),
            _ => None,
        }
    }
}

/// `--camera-mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CameraMode {
    #[default]
    Overview,
    Track,
}

impl CameraMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            CameraMode::Overview => "overview",
            CameraMode::Track => "track",
        }
    }
}

/// All `[gource]` section settings. Field names follow the C++ members.
///
/// Runtime state that the C++ code kept in the settings global (`shutdown`,
/// `file_graphic`, the string hash seed global) lives in the simulation
/// instead; `hash_seed` here is only the configured initial value.
#[derive(Debug, Clone)]
pub struct GourceSettings {
    /// Number of `[gource]` sections (repositories) in the conf.
    pub repo_count: usize,

    pub hide_date: bool,
    pub hide_users: bool,
    pub hide_tree: bool,
    pub hide_files: bool,
    pub hide_usernames: bool,
    pub hide_filenames: bool,
    pub hide_dirnames: bool,
    pub hide_progress: bool,
    pub hide_bloom: bool,
    pub hide_mouse: bool,
    pub hide_root: bool,

    pub disable_auto_rotate: bool,
    pub disable_input: bool,
    pub show_key: bool,

    pub load_config: String,
    pub save_config: String,
    /// Repository path, log file, or `-` for stdin.
    pub path: String,
    /// True if `path` was not given explicitly.
    pub default_path: bool,

    pub logo: String,
    pub logo_offset: Vec2,

    pub start_date: String,
    pub stop_date: String,
    /// Parsed `start_date` (0 = unset).
    pub start_timestamp: i64,
    /// Parsed `stop_date` (0 = unset).
    pub stop_timestamp: i64,
    pub start_position: f32,
    pub stop_position: f32,
    /// Seconds of runtime after which to stop (-1 = never).
    pub stop_at_time: f32,
    pub stop_on_idle: bool,
    pub stop_at_end: bool,
    pub dont_stop: bool,
    pub no_time_travel: bool,
    pub fixed_user_size: bool,
    pub author_time: bool,

    pub auto_skip_seconds: f32,
    pub days_per_second: f32,
    pub file_idle_time: f32,
    pub file_idle_time_at_end: f32,
    pub loop_delay_seconds: f32,
    /// `--loop` (`loop` in C++).
    pub looping: bool,
    /// `--disable-ffp`/fixed function pipeline flag; accepted for
    /// compatibility, has no effect on rendering.
    pub ffp: bool,

    pub colour_user_images: bool,
    pub default_user_image: String,
    pub user_image_dir: String,
    /// User name -> image path, built from `user_image_dir` on import.
    pub user_image_map: BTreeMap<String, String>,

    pub camera_zoom_min: f32,
    pub camera_zoom_max: f32,
    pub camera_zoom_default: f32,
    pub camera_mode: CameraMode,
    pub padding: f32,
    pub crop_vertical: bool,
    pub crop_horizontal: bool,

    pub bloom_multiplier: f32,
    pub bloom_intensity: f32,

    pub background_colour: Vec3,
    pub background_image: String,
    pub title: String,

    pub font_file: String,
    pub font_size: i32,
    pub filename_font_size: i32,
    pub dirname_font_size: i32,
    pub user_font_size: i32,
    pub font_colour: Vec3,
    pub font_scale: f32,
    /// True unless `--font-scale` was given (the simulation then picks a
    /// scale from the display size / DPI).
    pub default_font_scale: bool,
    pub scaled_font_size: i32,
    pub scaled_filename_font_size: i32,
    pub scaled_dirname_font_size: i32,
    pub scaled_user_font_size: i32,

    pub elasticity: f32,

    pub git_branch: String,
    pub log_format: String,
    pub date_format: String,

    pub max_files: i32,
    pub max_user_speed: f32,
    pub max_file_lag: f32,
    pub user_idle_time: f32,
    pub user_friction: f32,
    pub user_scale: f32,
    pub time_scale: f32,

    pub highlight_dirs: bool,
    pub highlight_all_users: bool,
    pub dir_colour: Vec3,
    pub highlight_colour: Vec3,
    pub selection_colour: Vec3,
    pub dir_name_depth: i32,
    pub dir_name_position: f32,

    pub highlight_users: Vec<String>,
    pub follow_users: Vec<String>,

    pub file_filters: Vec<fancy_regex::Regex>,
    pub file_show_filters: Vec<fancy_regex::Regex>,
    pub user_filters: Vec<fancy_regex::Regex>,
    pub user_show_filters: Vec<fancy_regex::Regex>,

    pub file_extensions: bool,
    pub file_extension_fallback: bool,

    pub caption_file: String,
    pub caption_colour: Vec3,
    pub caption_duration: f32,
    pub caption_size: i32,
    pub caption_offset: i32,

    pub filename_colour: Vec3,
    pub filename_time: f32,

    pub output_custom_filename: String,

    /// `--hash-seed` (initial string hash seed, default 31).
    pub hash_seed: i32,

    pub log_level: LogLevel,
}

impl Default for GourceSettings {
    /// `GourceSettings()` constructor + `setGourceDefaults()`.
    fn default() -> Self {
        let mut s = Self {
            repo_count: 0,

            hide_date: false,
            hide_users: false,
            hide_tree: false,
            hide_files: false,
            hide_usernames: false,
            hide_filenames: false,
            hide_dirnames: false,
            hide_progress: false,
            hide_bloom: false,
            hide_mouse: false,
            hide_root: false,

            disable_auto_rotate: false,
            disable_input: false,
            show_key: false,

            load_config: String::new(),
            save_config: String::new(),
            path: ".".to_owned(),
            default_path: true,

            logo: String::new(),
            logo_offset: Vec2::new(20.0, 20.0),

            start_date: String::new(),
            stop_date: String::new(),
            start_timestamp: 0,
            stop_timestamp: 0,
            start_position: 0.0,
            stop_position: 0.0,
            stop_at_time: -1.0,
            stop_on_idle: false,
            stop_at_end: false,
            dont_stop: false,
            no_time_travel: false,
            fixed_user_size: false,
            author_time: false,

            auto_skip_seconds: 3.0,
            days_per_second: 0.1,
            file_idle_time: 0.0,
            file_idle_time_at_end: 0.0,
            loop_delay_seconds: 3.0,
            looping: false,
            ffp: false,

            colour_user_images: false,
            default_user_image: String::new(),
            user_image_dir: String::new(),
            user_image_map: BTreeMap::new(),

            camera_zoom_min: 50.0,
            camera_zoom_max: 10000.0,
            camera_zoom_default: 100.0,
            camera_mode: CameraMode::Overview,
            padding: 1.1,
            crop_vertical: false,
            crop_horizontal: false,

            bloom_multiplier: 1.0,
            bloom_intensity: 0.75,

            background_colour: Vec3::splat(0.1),
            background_image: String::new(),
            title: String::new(),

            font_file: gource_font_file_default(),
            font_size: 16,
            filename_font_size: 14,
            dirname_font_size: 14,
            user_font_size: 14,
            font_colour: Vec3::ONE,
            font_scale: 1.0,
            default_font_scale: true,
            scaled_font_size: 0,
            scaled_filename_font_size: 0,
            scaled_dirname_font_size: 0,
            scaled_user_font_size: 0,

            elasticity: 0.0,

            git_branch: String::new(),
            log_format: String::new(),
            date_format: "%A, %d %B, %Y %X".to_owned(),

            max_files: 0,
            max_user_speed: 500.0,
            max_file_lag: 5.0,
            user_idle_time: 3.0,
            user_friction: 1.0,
            user_scale: 1.0,
            time_scale: 1.0,

            highlight_dirs: false,
            highlight_all_users: false,
            dir_colour: Vec3::ONE,
            highlight_colour: Vec3::ONE,
            selection_colour: Vec3::new(1.0, 1.0, 0.3),
            dir_name_depth: 0,
            dir_name_position: 0.5,

            highlight_users: Vec::new(),
            follow_users: Vec::new(),

            file_filters: Vec::new(),
            file_show_filters: Vec::new(),
            user_filters: Vec::new(),
            user_show_filters: Vec::new(),

            file_extensions: false,
            file_extension_fallback: false,

            caption_file: String::new(),
            caption_colour: Vec3::ONE,
            caption_duration: 10.0,
            caption_size: 16,
            caption_offset: 0,

            filename_colour: Vec3::ONE,
            filename_time: 4.0,

            output_custom_filename: String::new(),

            hash_seed: 31,

            log_level: LogLevel::Off,
        };
        s.set_scaled_font_sizes();
        s
    }
}

fn gource_font_file_default() -> String {
    "FreeSans.ttf".to_owned()
}

impl GourceSettings {
    /// `setScaledFontSizes`: `clamp((int)(size * font_scale), 1, 100)`.
    pub fn set_scaled_font_sizes(&mut self) {
        let scale = |size: i32| ((size as f32 * self.font_scale) as i32).clamp(1, 100);
        self.scaled_font_size = scale(self.font_size);
        self.scaled_user_font_size = scale(self.user_font_size);
        self.scaled_dirname_font_size = scale(self.dirname_font_size);
        self.scaled_filename_font_size = scale(self.filename_font_size);
    }

    /// `importGourceSettings(conf, section)`: reset to defaults, then apply the
    /// given `[gource]` section (or the first `[gource]` section if `None`),
    /// validating every value with the C++ error messages. Also sets
    /// `repo_count` from the number of `[gource]` sections.
    pub fn import(conf: &ConfFile, section: Option<&ConfSection>) -> Result<Self, SettingsError> {
        let mut settings = Self {
            repo_count: conf.count_sections("gource"),
            ..Self::default()
        };

        let default_sec;
        let gource_settings = match section {
            Some(s) => s,
            None => {
                if let Some(s) = conf.section("gource") {
                    s
                } else {
                    default_sec = ConfSection {
                        name: "gource".to_owned(),
                        entries: Vec::new(),
                        line: 0,
                    };
                    &default_sec
                }
            }
        };

        // hide flags
        let mut hide_fields: Vec<String> = Vec::new();

        if let Some(entry) = gource_settings.entry("hide") {
            if !entry.has_value() {
                return Err(conf.missing_value_error(entry));
            }

            let mut hide_string = entry.value.clone();
            while let Some(sep) = hide_string.find(',') {
                if sep == 0 && hide_string.len() == 1 {
                    break;
                }
                if sep == 0 {
                    hide_string = hide_string[1..].to_string();
                    continue;
                }
                let field = hide_string[..sep].to_string();
                hide_fields.push(field);
                hide_string = hide_string[sep + 1..].to_string();
            }
            if !hide_string.is_empty() && hide_string != "," {
                hide_fields.push(hide_string);
            }

            for hide_field in &hide_fields {
                match hide_field.as_str() {
                    "date" | "users" | "tree" | "files" | "usernames" | "filenames"
                    | "dirnames" | "bloom" | "progress" | "mouse" | "root" => {}
                    _ => {
                        return Err(conf.entry_error(
                            Some(entry),
                            format!("unknown option hide {hide_field}"),
                        ));
                    }
                }
            }
        }

        // check hide booleans
        for hide_bool_name in [
            "hide-date",
            "hide-files",
            "hide-users",
            "hide-tree",
            "hide-usernames",
            "hide-filenames",
            "hide-dirnames",
            "hide-progress",
            "hide-bloom",
            "hide-mouse",
            "hide-root",
        ] {
            if gource_settings.get_bool(hide_bool_name) {
                let hide_field = &hide_bool_name[5..];
                hide_fields.push(hide_field.to_owned());
            }
        }

        for hidestr in &hide_fields {
            match hidestr.as_str() {
                "date" => settings.hide_date = true,
                "users" => settings.hide_users = true,
                "tree" => settings.hide_tree = true,
                "files" => settings.hide_files = true,
                "usernames" => settings.hide_usernames = true,
                "filenames" => settings.hide_filenames = true,
                "dirnames" => settings.hide_dirnames = true,
                "bloom" => settings.hide_bloom = true,
                "progress" => settings.hide_progress = true,
                "root" => settings.hide_root = true,
                "mouse" => {
                    settings.hide_mouse = true;
                    settings.hide_progress = true;
                }
                _ => {}
            }
        }

        if let Some(entry) = gource_settings.entry("date-format") {
            if !entry.has_value() {
                return Err(conf.missing_value_error(entry));
            }
            settings.date_format = entry.value.clone();
        }

        if gource_settings.get_bool("disable-auto-rotate") {
            settings.disable_auto_rotate = true;
        }

        if gource_settings.get_bool("disable-auto-skip") {
            settings.auto_skip_seconds = -1.0;
        }

        if gource_settings.get_bool("disable-input") {
            settings.disable_input = true;
        }

        if gource_settings.get_bool("loop") {
            settings.looping = true;
        }

        if let Some(entry) = gource_settings.entry("loop-delay-seconds") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify loop-delay-seconds (float)"));
            }
            settings.loop_delay_seconds = entry.get_float();
            if settings.loop_delay_seconds <= 0.0 {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("git-branch") {
            if !entry.has_value() {
                return Err(conf.missing_value_error(entry));
            }
            let branch = &entry.value;
            if is_valid_branch_name(branch) {
                settings.git_branch = branch.clone();
            } else {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if gource_settings.get_bool("colour-images") {
            settings.colour_user_images = true;
        }

        if let Some(entry) = gource_settings.entry("crop") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify crop (vertical,horizontal)"));
            }
            match entry.value.as_str() {
                "vertical" => settings.crop_vertical = true,
                "horizontal" => settings.crop_horizontal = true,
                _ => return Err(conf.invalid_value_error(entry)),
            }
        }

        if let Some(entry) = gource_settings.entry("log-format") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify log-format (format)"));
            }
            let fmt = &entry.value;
            if fmt == "cvs" {
                return Err(
                    conf.entry_error(Some(entry), "please use either 'cvs2cl' or 'cvs-exp'")
                );
            }
            match fmt.as_str() {
                "git" | "cvs-exp" | "cvs2cl" | "svn" | "custom" | "hg" | "bzr" | "apache" => {
                    settings.log_format = fmt.clone();
                }
                _ => return Err(conf.invalid_value_error(entry)),
            }
        }

        if let Some(entry) = gource_settings.entry("default-user-image") {
            if !entry.has_value() {
                return Err(
                    conf.entry_error(Some(entry), "specify default-user-image (image path)")
                );
            }
            settings.default_user_image = entry.value.clone();
        }

        if let Some(entry) = gource_settings.entry("user-image-dir") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify user-image-dir (directory)"));
            }
            let mut dir_str = entry.value.clone();
            if !dir_str.ends_with('/') {
                dir_str.push('/');
            }
            settings.user_image_dir = dir_str.clone();
            settings.user_image_map.clear();

            let dir_path = Path::new(&dir_str);
            if !dir_path.is_dir() {
                return Err(
                    conf.entry_error(Some(entry), "specified user-image-dir is not a directory")
                );
            }

            let entries = std::fs::read_dir(dir_path).map_err(|_| {
                conf.entry_error(Some(entry), "error reading specified user-image-dir")
            })?;

            for dir_entry in entries {
                let dir_entry = match dir_entry {
                    Ok(e) => e,
                    Err(_) => {
                        return Err(
                            conf.entry_error(Some(entry), "error reading specified user-image-dir")
                        );
                    }
                };
                let file_path = dir_entry.path();
                let file_name = match file_path.file_name().and_then(|n| n.to_str()) {
                    Some(n) => n,
                    None => continue,
                };
                let lower_name = file_name.to_ascii_lowercase();
                let ext = if lower_name.ends_with(".png") {
                    ".png"
                } else if lower_name.ends_with(".jpg") {
                    ".jpg"
                } else if lower_name.ends_with(".jpeg") {
                    ".jpeg"
                } else {
                    continue;
                };

                let name = &file_name[..file_name.len() - ext.len()];
                let image_path = format!("{dir_str}{file_name}");
                settings.user_image_map.insert(name.to_owned(), image_path);
            }
        }

        if let Some(entry) = gource_settings.entry("caption-file") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify caption file (filename)"));
            }
            settings.caption_file = entry.value.clone();
            if !Path::new(&settings.caption_file).exists() {
                return Err(conf.entry_error(Some(entry), "caption file not found"));
            }
        }

        if let Some(entry) = gource_settings.entry("caption-duration") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify caption duration (seconds)"));
            }
            settings.caption_duration = entry.get_float();
            if settings.caption_duration <= 0.0 {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("caption-size") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify caption size"));
            }
            settings.caption_size = entry.get_int();
            if !(1..=100).contains(&settings.caption_size) {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("caption-offset") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify caption offset"));
            }
            settings.caption_offset = entry.get_int();
        }

        if let Some(entry) = gource_settings.entry("caption-colour") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify caption colour (FFFFFF)"));
            }
            if let Some(col) = parse_colour_entry(entry) {
                settings.caption_colour = col;
            } else {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("filename-colour") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify filename colour (FFFFFF)"));
            }
            if let Some(col) = parse_colour_entry(entry) {
                settings.filename_colour = col;
            } else {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("filename-time") {
            if !entry.has_value() {
                return Err(conf.entry_error(
                    Some(entry),
                    "specify duration to keep files on screen (float)",
                ));
            }
            settings.filename_time = entry.get_float();
            if settings.filename_time < 2.0 {
                return Err(conf.entry_error(Some(entry), "filename-time must be >= 2.0"));
            }
        }

        if let Some(entry) = gource_settings.entry("bloom-intensity") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify bloom-intensity (float)"));
            }
            settings.bloom_intensity = entry.get_float();
            if settings.bloom_intensity <= 0.0 {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("bloom-multiplier") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify bloom-multiplier (float)"));
            }
            settings.bloom_multiplier = entry.get_float();
            if settings.bloom_multiplier <= 0.0 {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("elasticity") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify elasticity (float)"));
            }
            settings.elasticity = entry.get_float();
            if settings.elasticity <= 0.0 {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("font-file") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify font file"));
            }
            let path = Path::new(&entry.value);
            if !path.exists() {
                return Err(conf.invalid_value_error(entry));
            }
            if let Ok(canon) = path.canonicalize() {
                let canon_str = canon.to_string_lossy().to_string();
                if canon_str.is_empty() {
                    return Err(conf.invalid_value_error(entry));
                }
                settings.font_file = canon_str;
            } else {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("font-size") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify font size"));
            }
            settings.font_size = entry.get_int();
            if !(1..=100).contains(&settings.font_size) {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("file-font-size") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify font size"));
            }
            settings.filename_font_size = entry.get_int();
            if !(1..=100).contains(&settings.filename_font_size) {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("dir-font-size") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify font size"));
            }
            settings.dirname_font_size = entry.get_int();
            if !(1..=100).contains(&settings.dirname_font_size) {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("user-font-size") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify font size"));
            }
            settings.user_font_size = entry.get_int();
            if !(1..=100).contains(&settings.user_font_size) {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("font-scale") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify font scale"));
            }
            settings.font_scale = entry.get_float();
            settings.default_font_scale = false;
            if settings.font_scale < 0.0 || settings.font_scale > 10.0 {
                return Err(conf.invalid_value_error(entry));
            }
            settings.set_scaled_font_sizes();
        }

        if let Some(entry) = gource_settings.entry("hash-seed") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify hash seed (integer)"));
            }
            settings.hash_seed = entry.get_int();
        }

        if let Some(entry) = gource_settings.entry("font-colour") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify font colour (FFFFFF)"));
            }
            if let Some(col) = parse_colour_entry(entry) {
                settings.font_colour = col;
            } else {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("background-colour") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify background colour (FFFFFF)"));
            }
            if let Some(col) = parse_colour_entry(entry) {
                settings.background_colour = col;
            } else {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("highlight-colour") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify highlight colour (FFFFFF)"));
            }
            if let Some(col) = parse_colour_entry(entry) {
                settings.highlight_colour = col;
            } else {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("selection-colour") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify selection colour (FFFFFF)"));
            }
            if let Some(col) = parse_colour_entry(entry) {
                settings.selection_colour = col;
            } else {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("dir-colour") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify dir colour (FFFFFF)"));
            }
            if let Some(col) = parse_colour_entry(entry) {
                settings.dir_colour = col;
            } else {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("background-image") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify background image (image path)"));
            }
            settings.background_image = entry.value.clone();
        }

        if let Some(entry) = gource_settings.entry("title") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify title"));
            }
            settings.title = entry.value.clone();
        }

        if let Some(entry) = gource_settings.entry("logo") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify logo (image path)"));
            }
            settings.logo = entry.value.clone();
        }

        if let Some(entry) = gource_settings.entry("logo-offset") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify logo-offset (XxY)"));
            }
            if let Some((x, y)) = crate::display::parse_rectangle(&entry.value) {
                settings.logo_offset = Vec2::new(x as f32, y as f32);
            } else {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("seconds-per-day") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify seconds-per-day (seconds)"));
            }
            let seconds_per_day = entry.get_float();
            if seconds_per_day <= 0.0 {
                return Err(conf.invalid_value_error(entry));
            }
            settings.days_per_second = 1.0 / seconds_per_day;
        }

        if let Some(entry) = gource_settings.entry("auto-skip-seconds") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify auto-skip-seconds (seconds)"));
            }
            settings.auto_skip_seconds = entry.get_float();
            if settings.auto_skip_seconds <= 0.0 {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("file-idle-time") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify file-idle-time (seconds)"));
            }
            let s = &entry.value;
            let val = entry.get_int() as f32;
            if val < 0.0 || (val == 0.0 && !s.starts_with('0')) {
                return Err(conf.invalid_value_error(entry));
            }
            settings.file_idle_time = val;
        }

        if let Some(entry) = gource_settings.entry("file-idle-time-at-end") {
            if !entry.has_value() {
                return Err(
                    conf.entry_error(Some(entry), "specify file-idle-time-at-end (seconds)")
                );
            }
            let s = &entry.value;
            let val = entry.get_int() as f32;
            if val < 0.0 || (val == 0.0 && !s.starts_with('0')) {
                return Err(conf.invalid_value_error(entry));
            }
            settings.file_idle_time_at_end = val;
        }

        if let Some(entry) = gource_settings.entry("user-idle-time") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify user-idle-time (seconds)"));
            }
            settings.user_idle_time = entry.get_float();
            if settings.user_idle_time < 0.0 {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("time-scale") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify time-scale (scale)"));
            }
            settings.time_scale = entry.get_float();
            if settings.time_scale <= 0.0 || settings.time_scale > 4.0 {
                return Err(conf.entry_error(Some(entry), "time-scale outside of range 0.0 - 4.0"));
            }
        }

        if let Some(entry) = gource_settings.entry("start-date") {
            if !entry.has_value() {
                return Err(
                    conf.entry_error(Some(entry), "specify start-date (YYYY-MM-DD hh:mm:ss)")
                );
            }
            if let Some(ts) = gource_core::datetime::parse_date_time(&entry.value) {
                settings.start_timestamp = ts;
                settings.start_date = gource_core::datetime::format_local(ts, "%Y-%m-%d");
            } else {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("stop-date") {
            if !entry.has_value() {
                return Err(
                    conf.entry_error(Some(entry), "specify stop-date (YYYY-MM-DD hh:mm:ss)")
                );
            }
            if let Some(ts) = gource_core::datetime::parse_date_time(&entry.value) {
                settings.stop_timestamp = ts;
                let time_str = gource_core::datetime::format_local(ts, "%H:%M:%S");
                let mut rounded = ts;
                if time_str != "00:00:00" {
                    rounded += 86400;
                }
                settings.stop_date = gource_core::datetime::format_local(rounded, "%Y-%m-%d");
            } else {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("start-position") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify start-position (float,random)"));
            }
            if entry.value == "random" {
                // In C++: srand(time(0)); start_position = (rand() % 1000) / 1000.0f;
                let seed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(0);
                settings.start_position = random_start_position(seed);
            } else {
                settings.start_position = entry.get_float();
                if settings.start_position <= 0.0 || settings.start_position >= 1.0 {
                    return Err(conf.entry_error(
                        Some(entry),
                        "start-position outside of range 0.0 - 1.0 (non-inclusive)",
                    ));
                }
            }
        }

        if let Some(entry) = gource_settings.entry("stop-position") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify stop-position (float)"));
            }
            settings.stop_position = entry.get_float();
            if settings.stop_position <= 0.0 || settings.stop_position > 1.0 {
                return Err(conf.entry_error(
                    Some(entry),
                    "stop-position outside of range 0.0 - 1.0 (inclusive)",
                ));
            }
        }

        if let Some(entry) = gource_settings.entry("stop-at-time") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify stop-at-time (seconds)"));
            }
            settings.stop_at_time = entry.get_float();
            if settings.stop_at_time <= 0.0 {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if gource_settings.get_bool("key") {
            settings.show_key = true;
        }

        if gource_settings.get_bool("ffp") {
            settings.ffp = true;
        }

        if gource_settings.get_bool("realtime") {
            settings.days_per_second = 1.0 / 86400.0;
        }

        if gource_settings.get_bool("no-time-travel") {
            settings.no_time_travel = true;
        }

        if gource_settings.get_bool("dont-stop") {
            settings.dont_stop = true;
        }

        if gource_settings.get_bool("stop-at-end") {
            settings.stop_at_end = true;
        }

        if gource_settings.get_bool("stop-on-idle") {
            settings.stop_on_idle = true;
        }

        if gource_settings.get_bool("fixed-user-size") {
            settings.fixed_user_size = true;
        }

        if gource_settings.get_bool("author-time") {
            settings.author_time = true;
        }

        if let Some(entry) = gource_settings.entry("max-files") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify max-files (number)"));
            }
            settings.max_files = entry.get_int();
            if settings.max_files < 0 || (settings.max_files == 0 && entry.value != "0") {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("max-file-lag") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify max-file-lag (seconds)"));
            }
            settings.max_file_lag = entry.get_float();
            if settings.max_file_lag == 0.0 {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("user-friction") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify user-friction (seconds)"));
            }
            let friction = entry.get_float();
            if friction <= 0.0 {
                return Err(conf.invalid_value_error(entry));
            }
            settings.user_friction = 1.0 / friction;
        }

        if let Some(entry) = gource_settings.entry("user-scale") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify user-scale (scale)"));
            }
            settings.user_scale = entry.get_float();
            if settings.user_scale <= 0.0 || settings.user_scale > 100.0 {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("max-user-speed") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify max-user-speed (units)"));
            }
            settings.max_user_speed = entry.get_float();
            if settings.max_user_speed <= 0.0 {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if gource_settings.get_bool("highlight-users")
            || gource_settings.get_bool("highlight-all-users")
        {
            settings.highlight_all_users = true;
        }

        if gource_settings.get_bool("highlight-dirs") {
            settings.highlight_dirs = true;
        }

        if let Some(entry) = gource_settings.entry("camera-mode") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify camera-mode (overview,track)"));
            }
            match entry.value.as_str() {
                "overview" => settings.camera_mode = CameraMode::Overview,
                "track" => settings.camera_mode = CameraMode::Track,
                _ => return Err(conf.invalid_value_error(entry)),
            }
        }

        if let Some(entry) = gource_settings.entry("padding") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify padding (float)"));
            }
            settings.padding = entry.get_float();
            if settings.padding <= 0.0 || settings.padding >= 2.0 {
                return Err(conf.invalid_value_error(entry));
            }
        }

        // multi-value entries
        for entry in gource_settings.entries_named("highlight-user") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify highlight-user (user)"));
            }
            settings.highlight_users.push(entry.value.clone());
        }

        for entry in gource_settings.entries_named("follow-user") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify follow-user (user)"));
            }
            settings.follow_users.push(entry.value.clone());
        }

        if gource_settings.get_bool("file-extensions") {
            settings.file_extensions = true;
        }

        if gource_settings.get_bool("file-extension-fallback") {
            settings.file_extension_fallback = true;
        }

        for entry in gource_settings.entries_named("file-filter") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify file-filter (regex)"));
            }
            let re = fancy_regex::Regex::new(&entry.value).map_err(|_| {
                conf.entry_error(Some(entry), "invalid file-filter regular expression")
            })?;
            settings.file_filters.push(re);
        }

        for entry in gource_settings.entries_named("file-show-filter") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify file-show-filter (regex)"));
            }
            let re = fancy_regex::Regex::new(&entry.value).map_err(|_| {
                conf.entry_error(Some(entry), "invalid file-show-filter regular expression")
            })?;
            settings.file_show_filters.push(re);
        }

        for entry in gource_settings.entries_named("user-filter") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify user-filter (regex)"));
            }
            let re = fancy_regex::Regex::new(&entry.value).map_err(|_| {
                conf.entry_error(Some(entry), "invalid user-filter regular expression")
            })?;
            settings.user_filters.push(re);
        }

        for entry in gource_settings.entries_named("user-show-filter") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify user-show-filter (regex)"));
            }
            let re = fancy_regex::Regex::new(&entry.value).map_err(|_| {
                conf.entry_error(Some(entry), "invalid user-show-filter regular expression")
            })?;
            settings.user_show_filters.push(re);
        }

        if let Some(entry) = gource_settings.entry("dir-name-depth") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify dir-name-depth (depth)"));
            }
            settings.dir_name_depth = entry.get_int();
            if settings.dir_name_depth <= 0 {
                return Err(conf.invalid_value_error(entry));
            }
        }

        if let Some(entry) = gource_settings.entry("dir-name-position") {
            if !entry.has_value() {
                return Err(conf.entry_error(Some(entry), "specify dir-name-position (float)"));
            }
            settings.dir_name_position = entry.get_float();
            if settings.dir_name_position < 0.1 || settings.dir_name_position > 1.0 {
                return Err(conf.entry_error(
                    Some(entry),
                    "dir-name-position outside of range 0.1 - 1.0 (inclusive)",
                ));
            }
        }

        // validate path
        if gource_settings.has_value("path") {
            settings.path = gource_settings.get_string("path");
            settings.default_path = false;
        }

        if settings.path == "-" {
            if settings.log_format.is_empty() {
                return Err(SettingsError(
                    "log-format required when reading from STDIN".to_owned(),
                ));
            }
        } else if !settings.path.is_empty() && settings.path != "." {
            let mut p = settings.path.clone();
            while p.ends_with('/') || p.ends_with('\\') {
                p.pop();
            }
            settings.path = p;
            if !Path::new(&settings.path).exists() {
                return Err(SettingsError(format!(
                    "'{}' does not appear to be a valid file or directory",
                    settings.path
                )));
            }
        }

        Ok(settings)
    }
}

fn is_valid_branch_name(s: &str) -> bool {
    // Regex: ^(?!-)[/\w.,;_=+{}\[\]-]+$
    if s.is_empty() || s.starts_with('-') {
        return false;
    }
    s.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || c == '_'
            || matches!(
                c,
                '/' | '.' | ',' | ';' | '=' | '+' | '{' | '}' | '[' | ']' | '-'
            )
    })
}

/// Helper to compute random start position from a seed/timestamp, matching C++
/// `srand(time(0)); (rand() % 1000) / 1000.0f`.
/// Value is guaranteed to be in [0.0, 1.0) with 3-decimal granularity.
pub fn random_start_position(seed: u64) -> f32 {
    ((seed % 1000) as f32) / 1000.0
}

fn parse_colour_entry(entry: &ConfEntry) -> Option<Vec3> {
    if entry.is_vec3() {
        return Some(entry.get_vec3());
    }
    let s = &entry.value;
    if s.len() == 6 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        return Some(Vec3::new(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_scaled_font_sizes() {
        let s = GourceSettings::default();
        assert_eq!(s.scaled_font_size, 16);
        assert_eq!(s.scaled_user_font_size, 14);
        assert_eq!(s.scaled_dirname_font_size, 14);
        assert_eq!(s.scaled_filename_font_size, 14);
    }

    #[test]
    fn scaled_font_sizes_clamp() {
        let mut s = GourceSettings {
            font_scale: 10.0,
            ..Default::default()
        };
        s.set_scaled_font_sizes();
        assert_eq!(s.scaled_font_size, 100);
        s.font_scale = 0.01;
        s.set_scaled_font_sizes();
        assert_eq!(s.scaled_font_size, 1);
    }

    #[test]
    fn test_import_defaults() {
        let conf = ConfFile::new();
        let s = GourceSettings::import(&conf, None).unwrap();
        assert_eq!(s.path, ".");
        assert!(s.default_path);
        assert_eq!(s.repo_count, 0);
        assert_eq!(s.camera_mode, CameraMode::Overview);
    }

    #[test]
    fn test_import_hide() {
        let mut conf = ConfFile::new();
        conf.set_entry("gource", "hide", "date,users,mouse");
        let s = GourceSettings::import(&conf, None).unwrap();
        assert!(s.hide_date);
        assert!(s.hide_users);
        assert!(s.hide_mouse);
        assert!(s.hide_progress);
    }

    #[test]
    fn test_import_branch_valid() {
        let mut conf = ConfFile::new();
        conf.set_entry("gource", "git-branch", "feature/awesome-123");
        let s = GourceSettings::import(&conf, None).unwrap();
        assert_eq!(s.git_branch, "feature/awesome-123");
    }

    #[test]
    fn test_import_branch_invalid() {
        let conf = ConfFile::parse("[gource]\ngit-branch=-invalid\n", "test.conf").unwrap();
        let err = GourceSettings::import(&conf, None).unwrap_err();
        assert_eq!(err.0, "test.conf, line 2: invalid 'git-branch' value");
    }

    #[test]
    fn test_random_start_position() {
        assert_eq!(random_start_position(0), 0.0);
        assert_eq!(random_start_position(500), 0.5);
        assert_eq!(random_start_position(999), 0.999);
        assert_eq!(random_start_position(1000), 0.0);
        assert_eq!(random_start_position(1234), 0.234);

        let mut conf = ConfFile::new();
        conf.set_entry("gource", "start-position", "random");
        let s = GourceSettings::import(&conf, None).unwrap();
        assert!(s.start_position >= 0.0 && s.start_position < 1.0);
    }
}
