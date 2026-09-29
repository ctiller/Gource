//! Gource per-repository simulation and visualization state machine
//! (port of `src/gource.h`, `src/gource.cpp`).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use glam::{Vec2, Vec4};
use gource_core::StringHasher;
use gource_core::bounds::Bounds2D;
use gource_core::datetime;
use gource_draw::font::{FontId, TextStyle};
use gource_draw::list::DrawList;
use gource_draw::{Gfx, Projection, TextureId};
use gource_history::{
    ChangeOp, ChurnDecayModel, CohortMode, CommitInput, FileChangeInput, History, HistoryBuilder,
};
use gource_settings::{
    CameraMode, DashboardPanel as SettingsDashboardPanel, FileColourMode, FileSizeMetric,
    GourceSettings, SettingClass, SettingId, SettingValue, SettingsPatch, TuningSettings,
};
use gource_vcs::CommitLog;
use gource_vcs::commit::{Commit, FileAction};
use gource_vcs::logmill::LogMill;
use gource_widgets::caption::RCaption;
use gource_widgets::cursor::{MouseButtonKind, MouseCursor};
use gource_widgets::dashboard::{
    DashboardPanel, DashboardStack, EditorsLeaderboardPanel, SparklinePanel, StackedDiffBarsPanel,
    TheseusCohortAreaPanel, format_compact_u64,
};
use gource_widgets::key::FileKey;
use gource_widgets::slider::PositionSlider;
use gource_widgets::textbox::TextBox;
use gource_widgets::timeline_bar::{
    TimelineBarData, TimelineBarMarker, TimelineBarWidget, TimelineHit, TimelineHoverCard,
};
use gource_widgets::tuning_panel::{
    TuningHit, TuningPanelData, TuningPanelWidget, TuningRowView, TuningTab,
};

use crate::app::AppError;
use crate::camera::{STARTING_Z, ZoomCamera};
use crate::file::FileId;
use crate::input::{InputEvent, Key, MouseButton};
use crate::platform::{PlatformRequest, Viewport};
use crate::scrubber::{SeekOutcome, SimScrubber};
use crate::user::UserId;
use crate::world::{SceneFonts, SceneTextures, World};

/// Most commits read ahead of the current time (`commitqueue_max_size`).
const COMMITQUEUE_MAX_SIZE: usize = 100;

/// Fonts used by the Gource HUD and scene.
#[derive(Debug, Clone, Copy)]
pub struct GourceFonts {
    pub large: FontId,
    pub medium: FontId,
    pub caption: FontId,
    pub base: FontId,
    pub dirname: FontId,
    pub textbox: FontId,
    pub slider: FontId,
    pub scene: SceneFonts,
}

impl GourceFonts {
    /// Load all fonts needed by Gource for the given settings and font scale.
    pub fn load(gfx: &mut Gfx, settings: &GourceSettings) -> Result<Self, AppError> {
        let font_path = &settings.font_file;
        let face_id =
            if font_path.is_empty() || font_path == gource_draw::resources::DEFAULT_FONT_NAME {
                gfx.fonts.default_face()
            } else {
                gfx.fonts
                    .load_face_file(Path::new(font_path))
                    .map_err(|e| AppError(e.to_string()))?
            };

        let large_size = ((42.0 * settings.font_scale) as u32).max(1);
        let medium_size = (settings.scaled_font_size as u32).max(1);
        let caption_size = (settings.caption_size as u32).max(1);
        let base_size = ((14.0 * settings.font_scale) as u32).max(1);
        let dirname_size = (settings.scaled_dirname_font_size as u32).max(1);
        let textbox_size = ((18.0 * settings.font_scale) as u32).max(1);
        let slider_size = ((16.0 * settings.font_scale) as u32).max(1);
        let user_size = (settings.scaled_user_font_size as u32).max(1);
        let file_size = (settings.scaled_filename_font_size as u32).max(1);

        let selected_size = ((18.0 * settings.font_scale) as u32).max(1);

        let large = gfx.fonts.font(face_id, large_size);
        let medium = gfx.fonts.font(face_id, medium_size);
        let caption = gfx.fonts.font(face_id, caption_size);
        let base = gfx.fonts.font(face_id, base_size);
        let dirname = gfx.fonts.font(face_id, dirname_size);
        let textbox = gfx.fonts.font(face_id, textbox_size);
        let slider = gfx.fonts.font(face_id, slider_size);

        let scene = SceneFonts {
            file: gfx.fonts.font(face_id, file_size),
            file_selected: gfx.fonts.font(face_id, selected_size),
            user: gfx.fonts.font(face_id, user_size),
            user_selected: gfx.fonts.font(face_id, selected_size),
            dir: dirname,
        };

        Ok(Self {
            large,
            medium,
            caption,
            base,
            dirname,
            textbox,
            slider,
            scene,
        })
    }
}

/// Textures used by Gource scene and HUD.
#[derive(Debug, Clone, Copy)]
pub struct GourceTextures {
    pub scene: SceneTextures,
    pub logo: Option<TextureId>,
    pub background: Option<TextureId>,
}

impl GourceTextures {
    /// Load textures needed by Gource into Gfx. Options follow the C++
    /// `texturemanager.grab` calls: mipmapped and clamped, except the logo.
    pub fn load(gfx: &mut Gfx, settings: &GourceSettings) -> Result<Self, AppError> {
        let file_tex = gfx
            .textures
            .load_bytes(
                "file.png",
                gource_draw::resources::FILE_PNG,
                gource_draw::TextureOptions::default(),
            )
            .map_err(|e| AppError(e.to_string()))?;

        let beam_tex = gfx
            .textures
            .load_bytes(
                "beam.png",
                gource_draw::resources::BEAM_PNG,
                gource_draw::TextureOptions::default(),
            )
            .map_err(|e| AppError(e.to_string()))?;

        // `usertex`; `--default-user-image` and `--user-image-dir` images are
        // chosen per user (`Gource::assign_user_image`).
        let user_tex = gfx
            .textures
            .load_bytes(
                "user.png",
                gource_draw::resources::USER_PNG,
                gource_draw::TextureOptions::default(),
            )
            .map_err(|e| AppError(e.to_string()))?;

        let logo = if !settings.logo.is_empty() {
            Some(
                gfx.textures
                    .load_file(
                        Path::new(&settings.logo),
                        gource_draw::TextureOptions::plain(),
                    )
                    .map_err(|_| resource_error(&settings.logo))?,
            )
        } else {
            None
        };

        let background = if !settings.background_image.is_empty() {
            Some(
                gfx.textures
                    .load_file(
                        Path::new(&settings.background_image),
                        gource_draw::TextureOptions::default(),
                    )
                    .map_err(|_| resource_error(&settings.background_image))?,
            )
        } else {
            None
        };

        Ok(Self {
            scene: SceneTextures {
                file: file_tex,
                beam: beam_tex,
                default_user: user_tex,
            },
            logo,
            background,
        })
    }
}

/// The fatal error for an image that can't be loaded: C++ throws a
/// `TextureException`, which `main` reports as "failed to load resource".
fn resource_error(path: &str) -> AppError {
    AppError(format!("failed to load resource '{path}'"))
}

/// The x position of a caption `width` pixels wide: C++ stores it in an
/// `int` - centred with `(display.width / 2) - (width / 2)` (integer
/// halving of the display width) when `--caption-offset` is 0, right-aligned
/// with `display.width + offset - width` when it is negative - so the
/// result is truncated towards zero.
fn caption_offset_x(caption_offset: i32, display_width: u32, width: f32) -> i32 {
    let display_width = display_width as i32;
    if caption_offset == 0 {
        ((display_width / 2) as f32 - width / 2.0) as i32
    } else if caption_offset < 0 {
        ((display_width + caption_offset) as f32 - width) as i32
    } else {
        caption_offset
    }
}

/// The Gource state machine for a single repository.
pub struct Gource {
    pub settings: GourceSettings,
    pub world: World,
    pub camera: ZoomCamera,

    pub logmill: Option<LogMill>,
    pub commitlog: Option<CommitLog>,

    pub slider: PositionSlider,
    pub file_key: FileKey,
    pub textbox: TextBox,
    pub cursor: MouseCursor,

    pub fonts: GourceFonts,
    pub textures: GourceTextures,

    pub captions: VecDeque<RCaption>,
    pub active_captions: Vec<RCaption>,

    pub commitqueue: VecDeque<Commit>,
    pub ingested_commits: Vec<Commit>,

    pub track_users: bool,
    pub manual_camera: bool,
    pub manual_zoom: bool,
    pub manual_rotate: bool,
    pub rotation_remaining_angle: f32,
    pub rotate_angle: f32,
    pub cursor_move: Vec2,

    pub selected_user: Option<UserId>,
    pub hover_user: Option<UserId>,
    pub selected_file: Option<FileId>,
    pub hover_file: Option<FileId>,

    pub grab_mouse: bool,
    pub mouse_moved: bool,
    pub mouse_clicked: bool,
    pub mouse_dragged: bool,
    pub mouse_pos: Vec2,

    pub take_screenshot: bool,
    pub recolour: bool,
    pub paused: bool,
    pub first_read: bool,
    pub reloaded: bool,
    pub stop_position_reached: bool,
    pub is_finished: bool,

    pub last_percent: f32,
    pub idle_time: f32,
    pub currtime: i64,
    pub lasttime: i64,
    pub subseconds: f32,
    pub runtime: f32,

    pub max_tick_rate: f32,
    pub frameskip: usize,
    pub framecount: usize,
    pub recording: bool,

    pub splash: f32,
    pub message: String,
    pub message_timer: f32,
    pub display_date: String,
    pub date_x_offset: f32,
    pub starting_z: f32,

    pub debug: bool,
    pub trace_debug: bool,
    pub quadtree_debug: bool,
    pub fps: f32,

    pub commit_cursor: usize,

    pub pending_requests: Vec<PlatformRequest>,

    pub history_builder: HistoryBuilder,
    pub history_dirty: bool,
    pub history_preindexed: bool,
    pub history_cache: Option<std::sync::Arc<History>>,
    pub scrubber: SimScrubber,
    pub dashboards: DashboardStack,
    pub cached_dashboard_commit: Option<usize>,
    pub cached_dashboard_data: Option<gource_history::DashboardSeriesData>,
    pub timeline_bar: TimelineBarWidget,
    pub timeline_dragging: bool,
    pub slider_dragging: bool,
    pub tuning_panel: TuningPanelWidget,
    pub tuning_settings: TuningSettings,
    pub tuning_tab: TuningTab,
    pub tuning_scroll: usize,
    pub tuning_status: Option<String>,
}

impl Gource {
    /// Create a new Gource instance.
    pub fn new(
        mut settings: GourceSettings,
        gfx: &mut Gfx,
        options: &crate::app::AppOptions,
        viewport: Viewport,
        output_framerate: u32,
    ) -> Result<Self, AppError> {
        let recording = options.recording;

        if settings.default_font_scale {
            if viewport.dpi_ratio > 1.0 {
                settings.font_scale = viewport.dpi_ratio;
            } else {
                let threshold = 1600;
                settings.font_scale = (1 + (viewport.height as i32 / threshold).max(0)) as f32;
            }
            settings.set_scaled_font_sizes();
        }

        let fonts = GourceFonts::load(gfx, &settings)?;
        let textures = GourceTextures::load(gfx, &settings)?;

        let starting_z = STARTING_Z;
        let camera = ZoomCamera::from_settings(&settings);

        let track_users = settings.camera_mode == CameraMode::Track;

        let mut slider = PositionSlider::new(0.0);
        slider.set_font(fonts.slider);
        slider.resize(viewport.width as f32, viewport.height as f32, 35.0);

        if !recording && settings.repo_count <= 1 {
            slider.show();
        }

        let mut file_key = FileKey::new(1.0);
        file_key.set_font(
            fonts.medium,
            settings.scaled_font_size as f32,
            settings.font_scale,
        );
        file_key.set_show(settings.show_key);

        let mut textbox = TextBox::new();
        textbox.set_font(fonts.textbox, 18.0 * settings.font_scale);
        textbox.set_brightness(0.5);
        textbox.show();

        let mut cursor = MouseCursor::new();
        if settings.hide_mouse {
            cursor.show_cursor(false);
        }

        let vcs_opts = crate::app::vcs_options(&settings);
        let logmill = LogMill::spawn(&settings.path, vcs_opts);

        let mut max_tick_rate = 1.0 / 60.0;
        let mut frameskip = 0;

        if recording {
            let video_framerate = output_framerate.max(1);
            let mut gource_framerate = video_framerate;
            while gource_framerate < 60 {
                gource_framerate += video_framerate;
                frameskip += 1;
            }
            max_tick_rate = 1.0 / (gource_framerate as f32);
        }

        // Seed rand sequence: if settings.seed != 0 use settings.seed, else 1
        let seed = match settings.seed {
            Some(s) if s != 0 => s as u64,
            _ => 1,
        };
        let mut world = World::new(seed, settings.hash_seed);
        world.weighted_mode = settings.file_size_metric != FileSizeMetric::None;
        let tuning_settings = TuningSettings {
            gravity: world.tuning.force_gravity,
            min_dir_size: world.tuning.min_dir_size,
            action_distance: world.tuning.action_dist,
            personal_space: world.tuning.personal_space_dist,
            beam_length: world.tuning.beam_dist,
            dir_padding: world.tuning.dir_padding,
            file_diameter: world.tuning.file_diameter,
            shadow_strength: world.tuning.shadow_strength,
            ..Default::default()
        };

        let history_builder =
            HistoryBuilder::new(CohortMode::Year, ChurnDecayModel::LifoYoungestFirst);
        let scrubber = SimScrubber::new(64);
        let dashboards = DashboardStack::new();

        let mut timeline_bar = TimelineBarWidget::new(fonts.slider, settings.font_scale);
        timeline_bar.resize(
            viewport.width,
            viewport.height,
            fonts.slider,
            settings.font_scale,
        );
        timeline_bar.show(false);

        let mut tuning_panel =
            TuningPanelWidget::new(fonts.medium, fonts.slider, settings.font_scale);
        tuning_panel.resize(
            viewport.width,
            viewport.height,
            fonts.medium,
            fonts.slider,
            settings.font_scale,
        );
        tuning_panel.show(false);

        let mut g = Self {
            settings,
            world,
            camera,
            logmill: Some(logmill),
            commitlog: None,
            slider,
            file_key,
            textbox,
            cursor,
            fonts,
            textures,
            captions: VecDeque::new(),
            active_captions: Vec::new(),
            commitqueue: VecDeque::new(),
            ingested_commits: Vec::new(),
            track_users,
            manual_camera: false,
            manual_zoom: false,
            manual_rotate: false,
            rotation_remaining_angle: 0.0,
            rotate_angle: 0.0,
            cursor_move: Vec2::ZERO,
            selected_user: None,
            hover_user: None,
            selected_file: None,
            hover_file: None,
            grab_mouse: false,
            mouse_moved: false,
            mouse_clicked: false,
            mouse_dragged: false,
            mouse_pos: Vec2::ZERO,
            take_screenshot: false,
            recolour: false,
            paused: false,
            first_read: true,
            reloaded: false,
            stop_position_reached: false,
            is_finished: false,
            last_percent: 0.0,
            idle_time: 0.0,
            currtime: 0,
            lasttime: 0,
            subseconds: 0.0,
            runtime: 0.0,
            max_tick_rate,
            frameskip,
            framecount: 0,
            recording,
            splash: -1.0,
            message: String::new(),
            message_timer: 0.0,
            display_date: String::new(),
            date_x_offset: 0.0,
            starting_z,
            debug: false,
            trace_debug: false,
            quadtree_debug: false,
            fps: 60.0,
            commit_cursor: 0,
            pending_requests: Vec::new(),
            history_builder,
            history_dirty: false,
            history_preindexed: false,
            history_cache: None,
            scrubber,
            dashboards,
            cached_dashboard_commit: None,
            cached_dashboard_data: None,
            timeline_bar,
            timeline_dragging: false,
            slider_dragging: false,
            tuning_panel,
            tuning_settings,
            tuning_tab: TuningTab::Visual,
            tuning_scroll: 0,
            tuning_status: None,
        };

        if !g.settings.caption_file.is_empty() {
            g.load_captions();
        }

        Ok(g)
    }

    /// Port of `Gource::reload()`: the shell calls it after reloading its
    /// resources (F5) and when the display changes size; the next logic
    /// step repositions the active captions.
    pub fn reload(&mut self) {
        self.reloaded = true;
    }

    /// Resize display: re-evaluate font scaling on default font scale, reload fonts,
    /// resize widgets, and update caption fonts.
    pub fn resize(&mut self, viewport: Viewport, gfx: &mut Gfx) {
        self.reload();

        if self.settings.default_font_scale {
            if viewport.dpi_ratio > 1.0 {
                self.settings.font_scale = viewport.dpi_ratio;
            } else {
                let threshold = 1600;
                self.settings.font_scale = (1 + (viewport.height as i32 / threshold).max(0)) as f32;
            }
            self.settings.set_scaled_font_sizes();
        }

        if let Ok(fonts) = GourceFonts::load(gfx, &self.settings) {
            self.fonts = fonts;
        }

        self.slider.set_font(self.fonts.slider);
        self.slider
            .resize(viewport.width as f32, viewport.height as f32, 35.0);

        self.file_key.set_font(
            self.fonts.medium,
            self.settings.scaled_font_size as f32,
            self.settings.font_scale,
        );

        self.textbox
            .set_font(self.fonts.textbox, 18.0 * self.settings.font_scale);

        self.timeline_bar.resize(
            viewport.width,
            viewport.height,
            self.fonts.slider,
            self.settings.font_scale,
        );

        self.tuning_panel.resize(
            viewport.width,
            viewport.height,
            self.fonts.medium,
            self.fonts.slider,
            self.settings.font_scale,
        );

        for cap in &mut self.captions {
            cap.font = self.fonts.caption;
        }
        for cap in &mut self.active_captions {
            cap.font = self.fonts.caption;
        }
    }

    /// Reset simulation state for seeking / restarting.
    pub fn reset(&mut self) {
        self.camera.reset();
        self.commitqueue.clear();
        self.recolour = false;
        self.selected_file = None;
        self.hover_file = None;
        self.selected_user = None;
        self.hover_user = None;
        self.manual_rotate = false;
        self.manual_zoom = false;
        self.manual_camera = false;
        self.rotation_remaining_angle = 0.0;
        self.rotate_angle = 0.0;
        self.message_timer = 0.0;
        self.cursor_move = Vec2::ZERO;
        self.grab_mouse = false;
        self.mouse_clicked = false;
        self.mouse_moved = false;
        self.mouse_dragged = false;
        self.last_percent = 0.0;
        self.commit_cursor = 0;
        if !self.is_live_mode() {
            self.history_preindexed = false;
            self.ingested_commits.clear();
        }
        self.timeline_dragging = false;
        self.slider_dragging = false;
        self.cached_dashboard_commit = None;
        self.cached_dashboard_data = None;

        // The C++ rand() stream and string hash seed are globals that a
        // reset leaves alone.
        let rng = std::mem::take(&mut self.world.rng);
        let hash_seed = self.world.hasher.seed;
        let mut world = World::new(1, hash_seed);
        world.weighted_mode = self.settings.file_size_metric != FileSizeMetric::None;
        world.rng = rng;
        self.world = world;
        self.file_key.clear();

        self.captions.clear();
        self.active_captions.clear();

        self.idle_time = 0.0;
        self.currtime = 0;
        self.lasttime = 0;
        self.subseconds = 0.0;

        if !self.settings.caption_file.is_empty() {
            self.load_captions();
        }
    }

    /// Read caption file and populate captions deque.
    pub fn load_captions(&mut self) {
        let path = Path::new(&self.settings.caption_file);
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return,
        };

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some((ts_str, text)) = trimmed.split_once('|')
                && let Some(timestamp) = datetime::parse_date_time(ts_str.trim())
            {
                let mut cap = RCaption::with_duration(
                    text.trim(),
                    timestamp,
                    self.fonts.caption,
                    self.settings.caption_duration,
                );
                cap.colour = self.settings.caption_colour;
                self.captions.push_back(cap);
            }
        }
    }

    /// Check whether live mode or GitHub watch mode is active.
    pub fn is_live_mode(&self) -> bool {
        self.settings.live
            || !self.settings.github.is_empty()
            || self.commitlog.as_ref().is_some_and(|l| l.is_live())
    }

    /// Check whether the playhead is currently pinned to the live edge.
    pub fn is_at_live_edge(&self) -> bool {
        self.is_live_mode()
            && self.scrubber.state.playback_direction == gource_history::PlaybackDirection::Forward
            && (self.ingested_commits.is_empty()
                || (self.commit_cursor >= self.ingested_commits.len()
                    && self.commitqueue.is_empty())
                || self.scrubber.state.playhead_fraction >= 0.99)
    }

    /// Drain newly arrived commits from the live log into ingested_commits and history_builder.
    pub fn drain_live_commits(&mut self) {
        if !self.is_live_mode() {
            return;
        }
        let Some(ref mut log) = self.commitlog else {
            return;
        };
        let mut got_any = false;
        while let Some(commit) = log.next_commit() {
            self.history_builder
                .add_commit(Self::commit_to_input(&commit));
            self.history_dirty = true;
            self.history_preindexed = true;
            self.ingested_commits.push(commit);
            got_any = true;
        }
        if got_any || self.history_dirty {
            let _ = self.ensure_history();
        }
    }

    /// Check if seeking is possible.
    pub fn can_seek(&self) -> bool {
        if self.settings.hide_progress {
            return false;
        }
        if self.commitlog.as_ref().is_some_and(|l| l.is_seekable()) {
            true
        } else {
            self.is_live_mode() && !self.ingested_commits.is_empty()
        }
    }

    /// Seek to a percentage (0.0 .. 1.0).
    pub fn seek_to(&mut self, percent: f32) {
        if self.commitlog.as_ref().is_some_and(|l| l.is_seekable()) {
            if self.paused {
                self.paused = false;
            }
            self.reset();
            if let Some(ref mut log) = self.commitlog {
                log.seek_to(percent);
            }
        }
    }

    /// Peek date string under mouse cursor at position percent.
    pub fn date_at_position(&mut self, percent: f32) -> String {
        if self.history_preindexed
            || self.world.weighted_mode
            || !self.settings.dashboards.is_empty()
            || !self.settings.cache_dir.is_empty()
        {
            let _ = self.ensure_history();
            if let Some(ref tl) = self.scrubber.timeline {
                return datetime::format_local(tl.fraction_to_time(percent), "%A, %d %B, %Y");
            }
        }
        if percent < 1.0
            && let Some(ref mut log) = self.commitlog
            && let Some(commit) = log.commit_at(percent)
        {
            return datetime::format_local(commit.timestamp, "%A, %d %B, %Y");
        }
        String::new()
    }

    /// Convert a VCS Commit to History CommitInput.
    pub(crate) fn commit_to_input(commit: &Commit) -> CommitInput {
        let files = commit
            .files
            .iter()
            .map(|cf| {
                let op = match cf.action {
                    FileAction::Add => ChangeOp::Add,
                    FileAction::Modify => ChangeOp::Modify,
                    FileAction::Delete => ChangeOp::Delete,
                    _ => ChangeOp::Modify,
                };
                FileChangeInput {
                    path: cf.filename.clone(),
                    op,
                    lines_added: cf.lines_added.unwrap_or(0),
                    lines_removed: cf.lines_removed.unwrap_or(0),
                    byte_size: None,
                    is_binary: false,
                }
            })
            .collect();
        CommitInput {
            timestamp: commit.timestamp,
            username: commit.username.clone(),
            files,
        }
    }

    /// Returns a shared reference-counted historical index snapshot, re-indexing if changes arrived.
    pub fn ensure_history(&mut self) -> std::sync::Arc<History> {
        if !self.history_preindexed && self.commitlog.as_ref().is_some_and(|l| l.is_seekable()) {
            let cache_path = if !self.settings.no_cache && !self.settings.cache_dir.is_empty() {
                let _ = std::fs::create_dir_all(&self.settings.cache_dir);
                let hash = gource_history::cache::fnv1a_64(self.settings.path.as_bytes());
                Some(
                    Path::new(&self.settings.cache_dir)
                        .join(format!("gource-history-{hash:016x}.bin")),
                )
            } else {
                None
            };

            let mut loaded_from_cache = false;
            if let Some(ref path) = cache_path
                && let Ok(cached_hist) = History::load_from_path(path, &self.settings.path)
            {
                let snap = std::sync::Arc::new(cached_hist);
                self.scrubber.set_history(&snap, 64);
                self.history_cache = Some(snap);
                self.history_preindexed = true;
                self.history_dirty = false;
                loaded_from_cache = true;
            }

            if !loaded_from_cache {
                self.history_builder =
                    HistoryBuilder::new(CohortMode::Year, ChurnDecayModel::LifoYoungestFirst);
                if let Some(ref mut log) = self.commitlog {
                    log.seek_to(0.0);
                    while let Some(c) = log.next_commit() {
                        self.history_builder.add_commit(Self::commit_to_input(&c));
                    }
                    log.seek_to(0.0);
                    for _ in 0..self.commit_cursor {
                        let _ = log.next_commit();
                    }
                }
                let snap = std::sync::Arc::new(self.history_builder.snapshot());
                if let Some(ref path) = cache_path {
                    let _ = snap.save_to_path(path, &self.settings.path);
                }
                self.scrubber.set_history(&snap, 64);
                self.history_cache = Some(snap);
                self.history_preindexed = true;
                self.history_dirty = false;
            }
        }

        if !self.history_dirty
            && let Some(ref snap) = self.history_cache
        {
            snap.clone()
        } else {
            let snap = std::sync::Arc::new(self.history_builder.snapshot());
            self.scrubber.set_history(&snap, 64);
            self.history_cache = Some(snap.clone());
            self.history_dirty = false;
            self.cached_dashboard_commit = None;
            self.cached_dashboard_data = None;
            snap
        }
    }

    /// Build the view model data payload for the tuning panel.
    pub fn build_tuning_panel_data(&self) -> TuningPanelData {
        let def_settings = GourceSettings::default();
        let def_tuning = TuningSettings::default();
        let mut rows = Vec::new();

        for (idx, &id) in SettingId::all().iter().enumerate() {
            let class_match = match self.tuning_tab {
                TuningTab::Visual => id.class() == SettingClass::Visual,
                TuningTab::Dynamics => id.class() == SettingClass::Dynamics,
                TuningTab::Timeline => id.class() == SettingClass::Timeline,
                TuningTab::Structural => id.class() == SettingClass::Structural,
            };
            if !class_match {
                continue;
            }

            let val = id.read(&self.settings, &self.tuning_settings);
            let def_val = id.read(&def_settings, &def_tuning);
            let is_modified = val != def_val;
            let cli_flag = id.cli_flag().unwrap_or("").to_string();
            let label = id.name().to_string();

            let row = if let Some((min, max, _)) = id.numeric_range() {
                let cur_f32 = match val {
                    SettingValue::F32(f) => f,
                    SettingValue::U32(u) => u as f32,
                    SettingValue::Usize(u) => u as f32,
                    _ => 0.0,
                };
                let frac = if max > min {
                    ((cur_f32 - min) / (max - min)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let val_str = format!("{cur_f32:.2}");
                TuningRowView::new_slider(idx, label, cli_flag, val_str, frac, is_modified)
            } else if let SettingValue::Bool(b) = val {
                let val_str = if b {
                    "true".to_string()
                } else {
                    "false".to_string()
                };
                TuningRowView::new_toggle(idx, label, cli_flag, val_str, b, is_modified)
            } else {
                let val_str = match val {
                    SettingValue::Vec3(v) => format!("{:.2},{:.2},{:.2}", v.x, v.y, v.z),
                    SettingValue::Vec4(v) => format!("{:.2},{:.2},{:.2},{:.2}", v.x, v.y, v.z, v.w),
                    SettingValue::String(s) => s,
                    SettingValue::OptionalString(s) => s.unwrap_or_default(),
                    SettingValue::CameraMode(cm) => cm.as_str().to_string(),
                    SettingValue::FileSizeMetric(m) => m.as_str().to_string(),
                    SettingValue::FileColourMode(m) => m.as_str().to_string(),
                    SettingValue::DashboardPeriod(p) => p.as_str().to_string(),
                    _ => String::new(),
                };
                TuningRowView::new_cycle(idx, label, cli_flag, val_str, is_modified)
            };

            rows.push(row);
        }

        TuningPanelData {
            active_tab: self.tuning_tab,
            rows,
            scroll_offset: self.tuning_scroll,
            status_message: self.tuning_status.clone(),
        }
    }

    /// Build the view model data payload for the timeline bar.
    pub fn build_timeline_bar_data(&mut self) -> TimelineBarData {
        let hist = self.ensure_history();
        let mut buckets = Vec::new();
        let mut markers = Vec::new();

        if let Some(ref tl) = self.scrubber.timeline {
            for b in &tl.buckets {
                buckets.push((
                    b.commits,
                    (b.lines_added.saturating_add(b.lines_removed)) as u32,
                ));
            }
            for m in &tl.markers {
                let frac = tl.time_to_fraction(m.timestamp);
                let col = match m.kind {
                    gource_history::MarkerKind::Tag => Vec4::new(0.95, 0.75, 0.2, 1.0),
                    gource_history::MarkerKind::Caption => Vec4::new(0.3, 0.7, 1.0, 1.0),
                    gource_history::MarkerKind::Milestone => Vec4::new(0.85, 0.4, 0.9, 1.0),
                };
                markers.push(TimelineBarMarker::new(frac, &m.label, col));
            }
        }

        let direction_label = match self.scrubber.state.playback_direction {
            gource_history::PlaybackDirection::Forward => {
                if self.paused {
                    "⏸".to_string()
                } else {
                    format!("▶ {:.1}x", self.settings.time_scale)
                }
            }
            gource_history::PlaybackDirection::Reverse => {
                if self.paused {
                    "⏸".to_string()
                } else {
                    format!("◀ {:.1}x", self.settings.time_scale)
                }
            }
        };

        let hover_info = if self.timeline_bar.hovered {
            let mouse_x = self.mouse_pos.x;
            let (t_min, t_max, _, _) = self.timeline_bar.track_rect();
            if mouse_x >= t_min && mouse_x <= t_max {
                let frac = self.timeline_bar.frac_at_x(mouse_x);
                self.scrubber.timeline.as_ref().map(|tl| {
                    let info = tl.hover_summary(&hist, frac);
                    let card = TimelineHoverCard::new(
                        frac,
                        datetime::format_local(info.timestamp, "%Y-%m-%d"),
                        info.bucket_commits,
                    )
                    .with_diff(info.bucket_lines_added, info.bucket_lines_removed);

                    let top_eds: Vec<(String, Vec4, u32)> = info
                        .top_editors
                        .iter()
                        .map(|(name, count)| {
                            let c = self.world.hasher.colour_hash(name);
                            (name.clone(), Vec4::new(c.x, c.y, c.z, 1.0), *count)
                        })
                        .collect();
                    card.with_top_editors(&top_eds)
                })
            } else {
                None
            }
        } else {
            None
        };

        let is_live = self.is_live_mode();
        let at_live_edge = self.is_at_live_edge();
        self.timeline_bar.set_live(is_live, at_live_edge);

        TimelineBarData {
            buckets,
            markers,
            playhead_frac: self.scrubber.state.playhead_fraction,
            seeking_target_frac: self.scrubber.state.target_fraction,
            clip_in_frac: self.scrubber.state.clip_in.unwrap_or(0.0),
            clip_out_frac: self.scrubber.state.clip_out.unwrap_or(1.0),
            direction_label,
            current_date: self.display_date.clone(),
            is_live,
            at_live_edge,
            hover_info,
        }
    }

    /// Handle an interactive click/action event from the tuning panel.
    pub fn handle_tuning_hit(&mut self, hit: TuningHit) {
        match hit {
            TuningHit::Close => {
                self.tuning_panel.show(false);
            }
            TuningHit::Tab(tab) => {
                self.tuning_tab = tab;
                self.tuning_scroll = 0;
            }
            TuningHit::RowReset { setting_index } => {
                if let Some(&id) = SettingId::all().get(setting_index) {
                    let def_s = GourceSettings::default();
                    let def_t = TuningSettings::default();
                    let def_val = id.read(&def_s, &def_t);
                    let mut patch = SettingsPatch::new();
                    patch.push(id, def_val);
                    let _ = patch.apply(&mut self.settings, &mut self.tuning_settings);
                    self.world.apply_tuning(&self.tuning_settings);
                }
            }
            TuningHit::RowSlider {
                setting_index,
                frac,
            } => {
                if let Some(&id) = SettingId::all().get(setting_index)
                    && let Some((min, max, _)) = id.numeric_range()
                {
                    let new_f32 = min + frac * (max - min);
                    let val = match id.read(&self.settings, &self.tuning_settings) {
                        SettingValue::U32(_) => SettingValue::U32(new_f32.round() as u32),
                        SettingValue::Usize(_) => SettingValue::Usize(new_f32.round() as usize),
                        _ => SettingValue::F32(new_f32),
                    };
                    let mut patch = SettingsPatch::new();
                    patch.push(id, val);
                    let _ = patch.apply(&mut self.settings, &mut self.tuning_settings);
                    self.world.apply_tuning(&self.tuning_settings);
                }
            }
            TuningHit::RowToggle { setting_index } => {
                if let Some(&id) = SettingId::all().get(setting_index) {
                    let cur = match id.read(&self.settings, &self.tuning_settings) {
                        SettingValue::Bool(b) => b,
                        _ => false,
                    };
                    let mut patch = SettingsPatch::new();
                    patch.push(id, SettingValue::Bool(!cur));
                    let _ = patch.apply(&mut self.settings, &mut self.tuning_settings);
                    self.world.apply_tuning(&self.tuning_settings);
                }
            }
            TuningHit::RowCycle { setting_index } => {
                if let Some(&id) = SettingId::all().get(setting_index) {
                    let next_val = match id {
                        SettingId::CameraMode => {
                            let next = match self.settings.camera_mode {
                                CameraMode::Overview => CameraMode::Track,
                                CameraMode::Track => CameraMode::Overview,
                            };
                            Some(SettingValue::CameraMode(next))
                        }
                        SettingId::FileColourMode => {
                            let next = match self.settings.file_colour_mode {
                                FileColourMode::Extension => FileColourMode::Age,
                                FileColourMode::Age => FileColourMode::Churn,
                                FileColourMode::Churn => FileColourMode::Cohort,
                                FileColourMode::Cohort => FileColourMode::Extension,
                            };
                            Some(SettingValue::FileColourMode(next))
                        }
                        SettingId::FileSizeMetric => {
                            let next = match self.settings.file_size_metric {
                                FileSizeMetric::None => FileSizeMetric::Size,
                                FileSizeMetric::Size => FileSizeMetric::Lines,
                                FileSizeMetric::Lines => FileSizeMetric::Diff,
                                FileSizeMetric::Diff => FileSizeMetric::Churn,
                                FileSizeMetric::Churn => FileSizeMetric::None,
                            };
                            Some(SettingValue::FileSizeMetric(next))
                        }
                        SettingId::DashboardPeriod => {
                            let next = match self.settings.dashboard_period {
                                gource_settings::DashboardPeriod::Day => {
                                    gource_settings::DashboardPeriod::Week
                                }
                                gource_settings::DashboardPeriod::Week => {
                                    gource_settings::DashboardPeriod::Month
                                }
                                gource_settings::DashboardPeriod::Month => {
                                    gource_settings::DashboardPeriod::Year
                                }
                                gource_settings::DashboardPeriod::Year => {
                                    gource_settings::DashboardPeriod::Day
                                }
                            };
                            Some(SettingValue::DashboardPeriod(next))
                        }
                        _ => None,
                    };
                    if let Some(val) = next_val {
                        let mut patch = SettingsPatch::new();
                        patch.push(id, val);
                        let _ = patch.apply(&mut self.settings, &mut self.tuning_settings);
                        self.world.apply_tuning(&self.tuning_settings);
                    }
                }
            }
            TuningHit::SaveConfig => {
                self.tuning_status = Some("Saved configuration".to_string());
            }
            TuningHit::CopyCli => {
                let flags = self.settings.to_cli_args().join(" ");
                self.tuning_status = Some(format!("Flags: {flags}"));
            }
            TuningHit::ResetAll => {
                self.settings = GourceSettings::default();
                self.tuning_settings = TuningSettings::default();
                self.world.apply_tuning(&self.tuning_settings);
                self.tuning_status = Some("Reset all settings".to_string());
            }
            TuningHit::PanelBackground | TuningHit::None => {}
        }
    }

    /// Handle an interactive click/scrub event on the timeline bar widget.
    pub fn handle_timeline_hit(&mut self, hit: TimelineHit) {
        match hit {
            TimelineHit::DirectionButton => {
                self.scrubber.state.toggle_direction();
            }
            TimelineHit::LiveBadge => {
                self.scrubber.state.playback_direction = gource_history::PlaybackDirection::Forward;
                self.paused = false;
                self.handle_timeline_hit(TimelineHit::Track(1.0));
            }
            TimelineHit::Track(frac) => {
                let frac = frac.clamp(0.0, 1.0);
                let hist = self.ensure_history();
                let total_commits = hist.commit_count();
                self.scrubber.state.set_fraction(frac, total_commits);
                if let Some(ref tl) = self.scrubber.timeline {
                    let min_commit_ts = hist.commits.first().map(|c| c.timestamp).unwrap_or(0);
                    let target_ts = tl.fraction_to_time(frac).max(min_commit_ts);
                    if let Some(tree_snap) = hist.state_at_timestamp(target_ts) {
                        self.world
                            .materialize_from_snapshot(&tree_snap, &hist, &self.settings, 30);
                        self.currtime = target_ts;
                        self.lasttime = target_ts;
                        self.subseconds = 0.0;
                        self.scrubber.sync_playhead_from_time(self.currtime);
                        self.commit_cursor = tree_snap.commit_index + 1;
                        self.commitqueue.clear();
                        self.stop_position_reached = false;
                        self.idle_time = 0.0;
                        if let Some(ref mut log) = self.commitlog
                            && log.is_seekable()
                        {
                            log.seek_to(0.0);
                            for _ in 0..self.commit_cursor {
                                let _ = log.next_commit();
                            }
                        }
                        self.last_percent = frac;
                        self.slider.set_percent(frac);
                    }
                }
            }
            TimelineHit::ClipInHandle => {
                let frac = self.timeline_bar.frac_at_x(self.mouse_pos.x);
                self.scrubber.state.set_clip_in(frac);
            }
            TimelineHit::ClipOutHandle => {
                let frac = self.timeline_bar.frac_at_x(self.mouse_pos.x);
                self.scrubber.state.set_clip_out(frac);
            }
            TimelineHit::Marker(idx) => {
                let timeline_data = self.build_timeline_bar_data();
                if let Some(m) = timeline_data.markers.get(idx) {
                    let frac = m.frac.clamp(0.0, 1.0);
                    let hist = self.ensure_history();
                    let total_commits = hist.commit_count();
                    self.scrubber.state.set_fraction(frac, total_commits);
                    if let Some(ref tl) = self.scrubber.timeline {
                        let min_commit_ts = hist.commits.first().map(|c| c.timestamp).unwrap_or(0);
                        let target_ts = tl.fraction_to_time(frac).max(min_commit_ts);
                        if let Some(tree_snap) = hist.state_at_timestamp(target_ts) {
                            self.world.materialize_from_snapshot(
                                &tree_snap,
                                &hist,
                                &self.settings,
                                30,
                            );
                            self.currtime = target_ts;
                            self.lasttime = target_ts;
                            self.subseconds = 0.0;
                            self.scrubber.sync_playhead_from_time(self.currtime);
                            self.commit_cursor = tree_snap.commit_index + 1;
                            self.commitqueue.clear();
                            self.stop_position_reached = false;
                            self.idle_time = 0.0;
                            if let Some(ref mut log) = self.commitlog
                                && log.is_seekable()
                            {
                                log.seek_to(0.0);
                                for _ in 0..self.commit_cursor {
                                    let _ = log.next_commit();
                                }
                            }
                            self.last_percent = frac;
                            self.slider.set_percent(frac);
                        }
                    }
                }
            }
            TimelineHit::None => {}
        }
    }

    /// Renders the analytics dashboard stack if enabled.
    pub fn draw_dashboards(&mut self, gfx: &mut Gfx, list: &mut DrawList, viewport: Viewport) {
        if !self.settings.hide_dashboards && !self.settings.dashboards.is_empty() {
            self.dashboards.clear();
            let hist = self.ensure_history();
            let playhead_commit_idx = if hist.is_empty() {
                0
            } else {
                match hist
                    .commits
                    .binary_search_by_key(&self.currtime, |c| c.timestamp)
                {
                    Ok(idx) => idx,
                    Err(idx) => {
                        if idx == 0 {
                            0
                        } else {
                            idx - 1
                        }
                    }
                }
            };

            let period_secs = match self.settings.dashboard_period {
                gource_settings::DashboardPeriod::Day => 86400,
                gource_settings::DashboardPeriod::Week => 7 * 86400,
                gource_settings::DashboardPeriod::Month => 30 * 86400,
                gource_settings::DashboardPeriod::Year => 365 * 86400,
            };
            let window_secs = (self.settings.dashboard_window_days as i64) * 86400;
            let series_data = if let (Some(cached_idx), Some(cached_data)) =
                (self.cached_dashboard_commit, &self.cached_dashboard_data)
            {
                if cached_idx == playhead_commit_idx {
                    cached_data.clone()
                } else {
                    let data = gource_history::DashboardSeriesData::extract(
                        &hist,
                        playhead_commit_idx,
                        period_secs,
                        window_secs,
                        20,
                    );
                    self.cached_dashboard_commit = Some(playhead_commit_idx);
                    self.cached_dashboard_data = Some(data.clone());
                    data
                }
            } else {
                let data = gource_history::DashboardSeriesData::extract(
                    &hist,
                    playhead_commit_idx,
                    period_secs,
                    window_secs,
                    20,
                );
                self.cached_dashboard_commit = Some(playhead_commit_idx);
                self.cached_dashboard_data = Some(data.clone());
                data
            };

            for panel_kind in &self.settings.dashboards {
                match panel_kind {
                    SettingsDashboardPanel::Lines => {
                        let p = SparklinePanel::new(
                            "Lines of Code",
                            format_compact_u64(series_data.total_lines),
                        )
                        .with_values(&series_data.lines_sparkline)
                        .with_delta(
                            format!("{:+}", series_data.lines_delta_in_window),
                            series_data.lines_delta_in_window >= 0,
                        );
                        self.dashboards.add_panel(DashboardPanel::Sparkline(p));
                    }
                    SettingsDashboardPanel::Diff => {
                        let diffs: Vec<(u64, u64)> = series_data
                            .diff_bars
                            .iter()
                            .map(|&(a, r)| (a as u64, r as u64))
                            .collect();
                        let p = StackedDiffBarsPanel::new(
                            "Code Churn",
                            format!(
                                "+{} -{}",
                                format_compact_u64(diffs.iter().map(|d| d.0).sum()),
                                format_compact_u64(diffs.iter().map(|d| d.1).sum())
                            ),
                        )
                        .with_diffs(&diffs);
                        self.dashboards
                            .add_panel(DashboardPanel::StackedDiffBars(p));
                    }
                    SettingsDashboardPanel::Theseus => {
                        let cohort_colors: Vec<glam::Vec3> =
                            (0..series_data.theseus_cohorts.cohort_labels.len())
                                .map(DashboardStack::cohort_palette)
                                .collect();
                        let p = TheseusCohortAreaPanel::new("Git-of-Theseus")
                            .with_cohorts(
                                &series_data.theseus_cohorts.cohort_labels,
                                &cohort_colors,
                                &series_data.theseus_cohorts.samples,
                            )
                            .with_analytics(
                                series_data.theseus_cohorts.half_life_days,
                                Some(series_data.theseus_cohorts.churn_rate),
                            );
                        self.dashboards.add_panel(DashboardPanel::TheseusCohort(p));
                    }
                    SettingsDashboardPanel::Editors => {
                        let p = EditorsLeaderboardPanel::new(
                            "Top Contributors",
                            series_data.active_editors_count,
                        )
                        .with_rows(&series_data.top_editors);
                        self.dashboards
                            .add_panel(DashboardPanel::EditorsLeaderboard(p));
                    }
                    SettingsDashboardPanel::Commits => {
                        let p = SparklinePanel::new(
                            "Commits",
                            format_compact_u64(series_data.commits_in_window as u64),
                        )
                        .with_values(&series_data.commits_per_period)
                        .with_line_colour(glam::Vec3::new(0.95, 0.65, 0.2));
                        self.dashboards.add_panel(DashboardPanel::Sparkline(p));
                    }
                    SettingsDashboardPanel::Churn => {
                        let p = SparklinePanel::new(
                            "Files",
                            format_compact_u64(series_data.total_files as u64),
                        )
                        .with_values(&series_data.files_sparkline)
                        .with_line_colour(glam::Vec3::new(0.85, 0.4, 0.9));
                        self.dashboards.add_panel(DashboardPanel::Sparkline(p));
                    }
                }
            }

            self.dashboards.draw(
                gfx,
                list,
                Some(self.fonts.base),
                viewport.width as f32,
                self.settings.font_scale,
            );
        }
    }

    /// Renders the timeline bar widget if visible.
    pub fn draw_timeline_bar(&mut self, gfx: &mut Gfx, list: &mut DrawList) {
        if self.timeline_bar.is_visible() {
            let timeline_data = self.build_timeline_bar_data();
            self.timeline_bar.draw(&timeline_data, gfx, list);
        }
    }

    /// Renders the live tuning panel widget if visible.
    pub fn draw_tuning_panel(&mut self, gfx: &mut Gfx, list: &mut DrawList) {
        if self.tuning_panel.is_visible() {
            let tuning_data = self.build_tuning_panel_data();
            self.tuning_panel.draw(&tuning_data, gfx, list);
        }
    }

    /// Apply an interactive settings patch to the simulation, updating physics tuning,
    /// dropping invalidated checkpoints, and rematerializing the scene tree if structural.
    pub fn apply_settings_patch(
        &mut self,
        patch: &SettingsPatch,
        viewport: Viewport,
        gfx: &mut Gfx,
    ) -> Vec<SettingClass> {
        let classes = patch.apply(&mut self.settings, &mut self.tuning_settings);
        self.world.apply_tuning(&self.tuning_settings);
        self.scrubber.on_patch_applied(&classes, self.currtime);
        if classes.contains(&SettingClass::Structural) {
            let _ = self.seek_to_timestamp(self.currtime, 0, viewport, gfx);
        }
        classes
    }

    /// Select or deselect a user.
    pub fn select_user(&mut self, user_id: Option<UserId>) {
        if user_id.is_some() && self.selected_user == user_id {
            return;
        }
        if let Some(fid) = self.selected_file.take()
            && let Some(file) = self.world.files.get_mut(fid)
        {
            file.pawn.selected = false;
        }
        if let Some(uid) = self.selected_user.take()
            && let Some(user) = self.world.users.get_mut(uid)
        {
            user.pawn.selected = false;
        }

        self.selected_user = user_id;

        if let Some(uid) = user_id {
            if let Some(user) = self.world.users.get_mut(uid) {
                user.pawn.selected = true;
            }
            if self.track_users {
                self.camera.lock_on(true);
            }
        } else {
            self.camera.lock_on(false);
        }
    }

    /// Select or deselect a file.
    pub fn select_file(&mut self, file_id: Option<FileId>) {
        if file_id.is_some() && self.selected_file == file_id {
            return;
        }
        if let Some(uid) = self.selected_user.take()
            && let Some(user) = self.world.users.get_mut(uid)
        {
            user.pawn.selected = false;
        }
        if let Some(fid) = self.selected_file.take()
            && let Some(file) = self.world.files.get_mut(fid)
        {
            file.pawn.selected = false;
        }

        self.selected_file = file_id;

        if let Some(fid) = file_id {
            if let Some(file) = self.world.files.get_mut(fid) {
                file.pawn.selected = true;
            }
            if self.track_users {
                self.camera.lock_on(true);
            }
        } else {
            self.camera.lock_on(false);
        }
    }

    /// Select next user in sequence (Tab key).
    pub fn select_next_user(&mut self) {
        if self.world.users.is_empty() {
            return;
        }
        let uids: Vec<UserId> = self.world.users_by_name.values().copied().collect();
        let next_uid = match self.selected_user {
            None => uids.first().copied(),
            Some(curr) => {
                let idx = uids.iter().position(|&u| u == curr).unwrap_or(0);
                Some(uids[(idx + 1) % uids.len()])
            }
        };
        self.select_user(next_uid);
    }

    /// Toggle camera tracking mode.
    pub fn toggle_camera_mode(&mut self) {
        self.track_users = !self.track_users;
        self.manual_rotate = false;
        self.manual_zoom = false;
        self.manual_camera = false;
        if self.selected_user.is_some() {
            self.camera.lock_on(self.track_users);
        }
        self.settings.camera_mode = if self.track_users {
            CameraMode::Track
        } else {
            CameraMode::Overview
        };
    }

    /// Zoom in / out.
    pub fn zoom(&mut self, zoom_in: bool) {
        self.manual_zoom = true;
        let mut dest = self.camera.dest();
        let step = (-dest.z) * 0.2;
        if zoom_in {
            dest.z += step;
        } else {
            dest.z -= step;
        }
        dest.z = dest.z.clamp(
            -self.settings.camera_zoom_max,
            -self.settings.camera_zoom_min,
        );
        self.camera.set_distance(-dest.z);
    }

    /// Grab or release mouse.
    pub fn set_grab_mouse(&mut self, grab: bool) {
        self.grab_mouse = grab;
        self.pending_requests
            .push(PlatformRequest::SetCursorGrab(grab));
        self.cursor.show_cursor(!grab);
        self.pending_requests
            .push(PlatformRequest::SetCursorVisible(!grab));
    }

    /// Change string hasher seed and recolour world & file key.
    pub fn change_colours(&mut self) {
        let new_seed = (self.world.rng.rand() % 10000) + 1;
        self.world.change_colours(new_seed);
        let hasher = StringHasher::new(new_seed);
        self.file_key.colourize(&hasher);
    }

    /// Set HUD message.
    pub fn set_message(&mut self, msg: impl Into<String>) {
        self.message = msg.into();
        self.message_timer = 5.0;
    }

    /// Take screenshot (F12).
    pub fn trigger_screenshot(&mut self) {
        let mut pngno = 1;
        let mut path = PathBuf::from(format!("gource-{pngno:04}.png"));
        while path.exists() && pngno < 10000 {
            pngno += 1;
            path = PathBuf::from(format!("gource-{pngno:04}.png"));
        }
        self.pending_requests.push(PlatformRequest::Screenshot {
            path: path.clone(),
            with_alpha: false,
        });
        self.set_message(format!("Wrote screenshot {}", path.display()));
    }

    /// Handle user input events.
    pub fn input(&mut self, event: &InputEvent) {
        if self.settings.disable_input {
            if let InputEvent::KeyDown {
                key: Key::Escape, ..
            } = event
            {
                self.is_finished = true;
                self.pending_requests.push(PlatformRequest::Quit);
            }
            return;
        }

        match event {
            InputEvent::KeyDown {
                key,
                modifiers,
                repeat,
            } => {
                if *key == Key::Escape && !*repeat {
                    self.is_finished = true;
                    self.pending_requests.push(PlatformRequest::Quit);
                    return;
                }

                if self.commitlog.is_none() {
                    return;
                }

                if *key == Key::Return && modifiers.alt && !*repeat {
                    self.pending_requests
                        .push(PlatformRequest::ToggleFullscreen);
                    return;
                }

                if *key == Key::F1 && !*repeat {
                    self.tuning_panel.toggle();
                    return;
                }

                if *key == Key::F2 && !*repeat {
                    self.timeline_bar.toggle();
                    return;
                }

                if *key == Key::F3 && !*repeat {
                    self.settings.hide_dashboards = !self.settings.hide_dashboards;
                    return;
                }

                if *key == Key::F4 && !*repeat {
                    self.scrubber.state.toggle_direction();
                    return;
                }

                if *key == Key::F11 && !*repeat {
                    self.pending_requests.push(PlatformRequest::ToggleFrameless);
                    return;
                }

                if *key == Key::F12 && !*repeat {
                    self.take_screenshot = true;
                    return;
                }

                if *repeat {
                    return;
                }

                match key {
                    Key::Char('q') => self.debug = !self.debug,
                    Key::Char('w') => self.trace_debug = !self.trace_debug,
                    Key::Char('m') => {
                        if !self.mouse_dragged
                            && !self.mouse_clicked
                            && !self.cursor.left_button_pressed()
                        {
                            let hidden = self.cursor.is_hidden();
                            self.cursor.show_cursor(hidden);
                            self.settings.hide_mouse = !hidden;
                            self.pending_requests
                                .push(PlatformRequest::SetCursorVisible(hidden));
                        }
                    }
                    Key::Char('n') => self.idle_time = self.settings.auto_skip_seconds,
                    Key::Char('y') => self.quadtree_debug = !self.quadtree_debug,
                    Key::Char('t') => self.settings.hide_tree = !self.settings.hide_tree,
                    Key::Char('g') => self.settings.hide_users = !self.settings.hide_users,
                    Key::Char('u') => {
                        if self.settings.hide_usernames && !self.settings.highlight_all_users {
                            self.settings.hide_usernames = false;
                            self.settings.highlight_all_users = true;
                        } else if self.settings.highlight_all_users && !self.settings.hide_usernames
                        {
                            self.settings.hide_usernames = false;
                            self.settings.highlight_all_users = false;
                        } else {
                            self.settings.hide_usernames = true;
                            self.settings.highlight_all_users = false;
                        }
                    }
                    Key::Char('d') => {
                        if self.settings.hide_dirnames && !self.settings.highlight_dirs {
                            self.settings.hide_dirnames = false;
                            self.settings.highlight_dirs = true;
                        } else if self.settings.highlight_dirs && !self.settings.hide_dirnames {
                            self.settings.hide_dirnames = false;
                            self.settings.highlight_dirs = false;
                        } else {
                            self.settings.hide_dirnames = true;
                            self.settings.highlight_dirs = false;
                        }
                    }
                    Key::Char('f') => {
                        if self.settings.hide_filenames && !self.settings.file_extensions {
                            self.settings.hide_filenames = false;
                        } else if !self.settings.hide_filenames && self.settings.file_extensions {
                            self.settings.file_extensions = false;
                            self.settings.hide_filenames = true;
                        } else {
                            self.settings.file_extensions = true;
                            self.settings.hide_filenames = false;
                        }
                    }
                    Key::Char('r') => self.settings.hide_root = !self.settings.hide_root,
                    Key::Char('k') => self.settings.show_key = !self.settings.show_key,
                    Key::Char('c') => self.splash = 15.0,
                    Key::Char('v') => self.toggle_camera_mode(),
                    Key::Char('s') => self.recolour = true,
                    Key::Char('z') => self.world.tuning.gravity = !self.world.tuning.gravity,
                    Key::Tab => self.select_next_user(),
                    Key::Space => self.paused = !self.paused,
                    Key::Char('=') | Key::Char('+') => {
                        if self.settings.days_per_second >= 1.0 {
                            self.settings.days_per_second =
                                (self.settings.days_per_second.floor() + 1.0).min(30.0);
                        } else {
                            self.settings.days_per_second =
                                (self.settings.days_per_second * 2.0).min(1.0);
                        }
                    }
                    Key::Char('-') => {
                        if self.settings.days_per_second > 1.0 {
                            self.settings.days_per_second =
                                (self.settings.days_per_second.floor() - 1.0).max(0.0);
                        } else {
                            self.settings.days_per_second =
                                (self.settings.days_per_second * 0.5).max(0.0);
                        }
                    }
                    Key::KeypadPlus => self.zoom(false),
                    Key::KeypadMinus => self.zoom(true),
                    Key::Char('[') => self.world.tuning.force_gravity /= 1.1,
                    Key::Char(']') => self.world.tuning.force_gravity *= 1.1,
                    Key::Char('.') => {
                        if self.settings.time_scale >= 1.0 {
                            self.settings.time_scale =
                                (self.settings.time_scale.floor() + 1.0).min(4.0);
                        } else {
                            self.settings.time_scale = (self.settings.time_scale * 2.0).min(1.0);
                        }
                    }
                    Key::Char(',') => {
                        if self.settings.time_scale > 1.0 {
                            self.settings.time_scale =
                                (self.settings.time_scale.floor() - 1.0).max(0.0);
                        } else {
                            self.settings.time_scale = (self.settings.time_scale * 0.5).max(0.25);
                        }
                    }
                    Key::Char('/') => self.settings.time_scale = 1.0,
                    Key::Right => {
                        self.cursor_move.x = 10.0;
                        self.manual_camera = true;
                    }
                    Key::Left => {
                        self.cursor_move.x = -10.0;
                        self.manual_camera = true;
                    }
                    Key::Up => {
                        self.cursor_move.y = -10.0;
                        self.manual_camera = true;
                    }
                    Key::Down => {
                        self.cursor_move.y = 10.0;
                        self.manual_camera = true;
                    }
                    _ => {}
                }
            }
            InputEvent::KeyUp { .. } => {}
            InputEvent::MouseMove { pos, delta } => {
                if self.commitlog.is_none() || self.settings.hide_mouse {
                    return;
                }

                if self.timeline_bar.is_visible() && self.timeline_dragging {
                    self.mouse_pos = *pos;
                    self.cursor.update_pos(*pos);
                    let frac = self.timeline_bar.frac_at_x(pos.x);
                    self.handle_timeline_hit(TimelineHit::Track(frac));
                    return;
                }

                if !self.timeline_bar.is_visible() && self.slider_dragging {
                    self.mouse_pos = *pos;
                    self.cursor.update_pos(*pos);
                    let b = self.slider.bounds();
                    let denom = (b.max.x - b.min.x).max(1.0);
                    let p = ((pos.x - b.min.x) / denom).clamp(0.0, 1.0);
                    self.handle_timeline_hit(TimelineHit::Track(p));
                    return;
                }

                let right_mouse = self.cursor.right_button_pressed();

                if self.mouse_dragged || right_mouse {
                    if right_mouse {
                        self.manual_rotate = true;
                        let mag = if delta.x.abs() > delta.y.abs() {
                            delta.x
                        } else {
                            delta.y
                        };
                        // C++: min(1, |mag| / 10) * 5 * DEGREES_TO_RADIANS
                        // (a double), negated for a negative drag.
                        let angle = ((1.0f32.min(mag.abs() / 10.0) * 5.0) as f64
                            * gource_core::math::CPP_DEGREES_TO_RADIANS)
                            as f32;
                        self.rotate_angle = if mag < 0.0 { -angle } else { angle };
                        return;
                    }

                    self.cursor_move += *delta;
                    return;
                }

                if self.grab_mouse {
                    return;
                }

                self.mouse_pos = *pos;
                self.mouse_moved = true;
                self.cursor.update_pos(*pos);

                self.timeline_bar.hovered = self.timeline_bar.bounds.contains(*pos);

                if !self.timeline_bar.is_visible()
                    && !self.settings.hide_progress
                    && let Some(p) = self.slider.mouse_over(*pos)
                {
                    let date = self.date_at_position(p);
                    self.slider.set_caption(date, 0.0);
                }
            }
            InputEvent::MouseButton {
                button,
                pressed,
                pos,
            } => {
                if self.commitlog.is_none() || self.settings.hide_mouse {
                    return;
                }

                let kind = match button {
                    MouseButton::Left => MouseButtonKind::Left,
                    MouseButton::Right => MouseButtonKind::Right,
                    MouseButton::Middle => MouseButtonKind::Middle,
                };
                self.cursor.set_button_down(kind, *pressed);

                if *pressed {
                    if *button == MouseButton::Left {
                        self.cursor.set_left_click(true);
                        self.mouse_clicked = true;

                        // 1. Tuning Panel hit test
                        if self.tuning_panel.is_visible() {
                            let tuning_data = self.build_tuning_panel_data();
                            let hit = self.tuning_panel.hit_test(*pos, &tuning_data);
                            if !matches!(hit, TuningHit::None) {
                                self.handle_tuning_hit(hit);
                                return;
                            }
                        }

                        // 2. Timeline Bar hit test
                        if self.timeline_bar.is_visible() {
                            let timeline_data = self.build_timeline_bar_data();
                            let hit = self.timeline_bar.hit_test(*pos, Some(&timeline_data));
                            if !matches!(hit, TimelineHit::None) {
                                if matches!(hit, TimelineHit::Track(_)) {
                                    self.timeline_dragging = true;
                                }
                                self.handle_timeline_hit(hit);
                                return;
                            }
                        }

                        if !self.timeline_bar.is_visible()
                            && !self.settings.hide_progress
                            && let Some(p) = self.slider.click(*pos)
                        {
                            if self.world.weighted_mode
                                || !self.settings.dashboards.is_empty()
                                || !self.settings.cache_dir.is_empty()
                                || self.history_preindexed
                                || self.is_live_mode()
                            {
                                self.slider_dragging = true;
                                self.handle_timeline_hit(TimelineHit::Track(p));
                            } else {
                                self.seek_to(p);
                            }
                            return;
                        }

                        // Trace click
                        if let Some(uid) = self.hover_user {
                            self.select_user(Some(uid));
                        } else if let Some(fid) = self.hover_file {
                            self.select_file(Some(fid));
                        } else {
                            // Select background
                            self.select_user(None);
                            self.manual_camera = true;
                            self.mouse_dragged = true;
                            self.set_grab_mouse(true);
                        }
                    } else if *button == MouseButton::Right {
                        self.cursor.set_right_click(true);
                    }
                } else if *button == MouseButton::Left {
                    self.timeline_dragging = false;
                    self.slider_dragging = false;
                    self.mouse_dragged = false;
                    self.set_grab_mouse(false);
                }
            }
            InputEvent::MouseWheel { delta } => {
                if self.commitlog.is_none() {
                    return;
                }
                self.zoom(*delta > 0.0);
            }
            InputEvent::Focus(focused) => {
                self.cursor.set_focus(*focused);
                if !focused && self.grab_mouse {
                    self.mouse_dragged = false;
                    self.set_grab_mouse(false);
                }
            }
        }
    }

    /// Read commits from the log into the queue (`Gource::readLog`): until
    /// the last queued commit is ahead of the current time, or the queue is
    /// full.
    pub fn read_log(&mut self) -> Result<(), AppError> {
        if self.stop_position_reached {
            return Ok(());
        }

        if self.is_live_mode() {
            self.drain_live_commits();

            while self.commit_cursor < self.ingested_commits.len()
                && self.commitqueue.back().is_none_or(|last| {
                    last.timestamp <= self.currtime && self.commitqueue.len() < COMMITQUEUE_MAX_SIZE
                })
            {
                let commit = self.ingested_commits[self.commit_cursor].clone();
                if self.settings.stop_timestamp != 0
                    && commit.timestamp > self.settings.stop_timestamp
                {
                    self.stop_position_reached = true;
                    break;
                }
                self.commit_cursor += 1;
                self.commitqueue.push_back(commit);
            }

            if !self.commitqueue.is_empty() {
                self.first_read = false;
            }
            return Ok(());
        }

        let Some(log) = self.commitlog.as_mut() else {
            return Ok(());
        };

        while (log.has_buffered_commit() || !log.is_finished())
            && self.commitqueue.back().is_none_or(|last| {
                last.timestamp <= self.currtime && self.commitqueue.len() < COMMITQUEUE_MAX_SIZE
            })
        {
            let Some(commit) = log.next_commit() else {
                if !log.is_seekable() {
                    break;
                }
                continue;
            };
            if self.settings.stop_timestamp != 0 && commit.timestamp > self.settings.stop_timestamp
            {
                self.stop_position_reached = true;
                break;
            }
            if !self.history_preindexed {
                self.history_builder
                    .add_commit(Self::commit_to_input(&commit));
                self.history_dirty = true;
            }
            self.commit_cursor += 1;
            self.commitqueue.push_back(commit);
        }

        if self.first_read && self.commitqueue.is_empty() {
            return Err(AppError("no commits found".to_string()));
        }
        self.first_read = false;

        if !self.history_preindexed && !log.is_finished() && log.is_seekable() {
            self.last_percent = log.percent();
            self.slider.set_percent(self.last_percent);
        }

        let is_finished = log.is_finished();
        if (self.settings.stop_at_end && is_finished)
            || (self.settings.stop_position > 0.0
                && log.is_seekable()
                && (is_finished || self.last_percent >= self.settings.stop_position))
        {
            self.stop_position_reached = true;
        }

        if (is_finished || self.stop_position_reached) && self.settings.file_idle_time_at_end > 0.0
        {
            self.settings.file_idle_time = self.settings.file_idle_time_at_end;
        }

        Ok(())
    }

    /// Process a single commit by updating the directory tree and actions.
    pub fn process_commit(
        &mut self,
        commit: &Commit,
        t: f32,
        gfx: &mut Gfx,
    ) -> Result<(), AppError> {
        for cf in &commit.files {
            if !cf.filename.is_empty() && cf.filename.ends_with('/') {
                if cf.action != FileAction::Delete {
                    continue;
                }
                let dirs = self.world.find_dirs(&cf.filename);
                for dir_id in dirs {
                    let dir_files = self.world.get_files_recursive(dir_id);
                    for fid in dir_files {
                        self.world
                            .add_file_action(commit, cf, fid, t, &self.settings);
                    }
                }
                continue;
            }

            let file_id = match self.world.files_by_path.get(&cf.filename).copied() {
                Some(fid) => Some(fid),
                None => {
                    let fid_opt = self.world.add_file(cf, &self.settings);
                    if let Some(fid) = fid_opt {
                        let ext = self.world.files[fid].ext.clone();
                        let col = self.world.files[fid].file_colour;
                        let font_id = self.fonts.medium;
                        self.file_key
                            .inc(&ext, col, |text| gfx.text_width(font_id, text));
                    }
                    fid_opt
                }
            };

            if let Some(fid) = file_id {
                self.world
                    .add_file_action(commit, cf, fid, t, &self.settings);
            }
        }

        for uid in std::mem::take(&mut self.world.new_users) {
            self.assign_user_image(uid, gfx)?;
        }
        Ok(())
    }

    /// Port of `RUser::assignUserImage`: the image from `--user-image-dir`
    /// matching the user's name, else `--default-user-image`, else the
    /// built-in `user.png`. Custom images are drawn uncoloured unless
    /// `--colour-images` is set.
    pub fn assign_user_image(&mut self, uid: UserId, gfx: &mut Gfx) -> Result<(), AppError> {
        let Some(user) = self.world.users.get(uid) else {
            return Ok(());
        };
        let mut image = None;
        if !self.settings.user_image_dir.is_empty() {
            image = self.settings.user_image_map.get(user.name());
        }
        if image.is_none() && !self.settings.default_user_image.is_empty() {
            image = Some(&self.settings.default_user_image);
        }
        let graphic = match image {
            Some(path) => Some(
                gfx.textures
                    .load_file(Path::new(path), gource_draw::TextureOptions::default())
                    .map_err(|_| resource_error(path))?,
            ),
            None => None,
        };
        let uncoloured = graphic.is_some() && !self.settings.colour_user_images;
        let size = gfx
            .textures
            .size(graphic.unwrap_or(self.textures.scene.default_user));
        let hasher = &self.world.hasher;
        if let Some(user) = self.world.users.get_mut(uid) {
            user.assign_graphic(hasher, graphic, size, uncoloured);
        }
        Ok(())
    }

    pub fn logic(&mut self, dt: f32, viewport: Viewport, gfx: &mut Gfx) -> Result<(), AppError> {
        let pending_uids = std::mem::take(&mut self.world.new_users);
        for uid in pending_uids {
            let _ = self.assign_user_image(uid, gfx);
        }

        if self.is_finished {
            return Ok(());
        }

        if self.message_timer > 0.0 {
            self.message_timer -= dt;
        }
        if self.splash > 0.0 {
            self.splash -= dt;
        }

        // Initialize log from logmill
        if self.commitlog.is_none() {
            if self.recording {
                // C++ polls the log mill, which nearly always finishes during
                // the first frame. A recording makes that deterministic, so
                // its runtime and exported frames don't depend on how long
                // the log takes to read: the first frame always waits, and
                // the second blocks until the log is ready.
                if self.framecount == 0 {
                    return Ok(());
                }
                if let Some(m) = self.logmill.as_mut() {
                    m.wait();
                }
            }
            let finished = self
                .logmill
                .as_ref()
                .map(|m| m.is_finished())
                .unwrap_or(false);
            if !finished {
                return Ok(());
            }

            let res = self.logmill.as_mut().and_then(|m| m.take_result());
            match res {
                Some(Ok(log)) => {
                    self.commitlog = Some(log);
                    if self.settings.start_position > 0.0 {
                        self.seek_to(self.settings.start_position);
                    }
                }
                Some(Err(err)) => {
                    if !err.is_empty() {
                        return Err(AppError(err));
                    } else if self.settings.default_path {
                        self.pending_requests.push(PlatformRequest::ShowHelpAndExit);
                        return Ok(());
                    } else {
                        return Err(AppError("failed to generate log file".to_string()));
                    }
                }
                None => return Ok(()),
            }
        }

        self.file_key.logic(dt, viewport.height as f32);
        self.slider.logic(dt);
        self.timeline_bar.logic(dt);
        self.tuning_panel.logic(dt);

        // Apply tree rotation
        if self.rotate_angle != 0.0 {
            let s = self.rotate_angle.sin();
            let c = self.rotate_angle.cos();

            if self.manual_rotate {
                let centre = self.camera.pos().truncate();
                self.world.rotate(s, c, Some(centre));
            } else {
                self.world.rotate(s, c, None);
            }
            self.rotate_angle = 0.0;
        }

        if self.recolour {
            self.change_colours();
            self.recolour = false;
        }

        self.world.weighted_mode = self.settings.file_size_metric != FileSizeMetric::None;

        if self.paused {
            self.world.update_bounds();
            self.world.interact_users();
            self.world.interact_dirs();
            self.update_camera(dt, viewport);
            return Ok(());
        }

        // Reverse playback branch
        if self.scrubber.state.playback_direction == gource_history::PlaybackDirection::Reverse {
            let _ = self.step_reverse(viewport, gfx);
            return Ok(());
        }

        self.scrubber.push_reverse_frame(self.snapshot());
        self.maybe_record_checkpoint();
        self.scrubber.sync_playhead_from_time(self.currtime);

        // Fetch commits
        if self.is_live_mode() {
            self.drain_live_commits();
            if self.commitqueue.is_empty() && self.commit_cursor < self.ingested_commits.len() {
                self.read_log()?;
            }
        } else if self.commitqueue.is_empty() {
            self.read_log()?;
        }

        if self.settings.looping
            && self.commitqueue.is_empty()
            && self.commitlog.as_ref().is_some_and(|l| l.is_seekable())
            && self.idle_time >= self.settings.loop_delay_seconds
        {
            self.first_read = true;
            self.seek_to(0.0);
            self.read_log()?;
        }

        if self.currtime == 0 && !self.commitqueue.is_empty() {
            self.currtime = self.commitqueue[0].timestamp;
            self.lasttime = self.currtime;
            self.subseconds = 0.0;
        }

        if self.is_live_mode()
            && self.commitqueue.is_empty()
            && self.commit_cursor >= self.ingested_commits.len()
        {
            // At the live edge waiting for new commits: hold currtime at lasttime
            if self.lasttime > 0 {
                self.currtime = self.lasttime;
            }
            self.subseconds = 0.0;
        } else {
            // C++: `float time_inc = (dt * 86400.0 * days_per_second)`: 86400.0
            // is a double, so the product is evaluated in double precision.
            let time_inc = (dt as f64 * 86400.0 * self.settings.days_per_second as f64) as f32;
            let seconds = time_inc as i64;
            self.subseconds += time_inc - (seconds as f32);

            if self.subseconds >= 1.0 {
                self.currtime += self.subseconds as i64;
                self.subseconds -= (self.subseconds as i64) as f32;
            }
            self.currtime += seconds;
        }

        // Delete reaped files
        let removed = std::mem::take(&mut self.world.removed_files);
        for fid in removed {
            if let Some(del) = self.world.delete_file(fid) {
                self.file_key.dec(&del.ext);
            }
        }

        // Process commits up to currtime
        let t = self.runtime;
        while let Some(commit) = self.commitqueue.front() {
            let auto_skip = self.settings.auto_skip_seconds >= 0.0
                && self.idle_time >= self.settings.auto_skip_seconds
                && !self.stop_position_reached;
            let live_catchup =
                self.is_live_mode() && self.currtime < commit.timestamp && self.idle_time > 0.0;
            if auto_skip || live_catchup {
                self.currtime = commit.timestamp;
                self.lasttime = commit.timestamp;
                self.idle_time = 0.0;
            }

            if commit.timestamp > self.currtime {
                break;
            }

            let commit = self.commitqueue.pop_front().unwrap();
            self.process_commit(&commit, t, gfx)?;

            if self.settings.no_time_travel {
                if commit.timestamp > self.lasttime {
                    self.lasttime = commit.timestamp;
                }
            } else {
                if self.lasttime > commit.timestamp {
                    self.currtime = commit.timestamp;
                }
                self.lasttime = commit.timestamp;
            }
            self.subseconds = 0.0;
        }

        if self.is_live_mode()
            && self.commitqueue.is_empty()
            && self.commit_cursor >= self.ingested_commits.len()
            && self.lasttime > 0
        {
            self.currtime = self.lasttime;
            self.subseconds = 0.0;
        }

        // C++ resizes the slider to the display every tick.
        self.slider
            .resize(viewport.width as f32, viewport.height as f32, 35.0);
        self.timeline_bar.resize(
            viewport.width,
            viewport.height,
            self.fonts.slider,
            self.settings.font_scale,
        );

        // Captions logic
        let caption_height = gfx.fonts.max_height(self.fonts.caption);
        let medium_height = gfx.fonts.max_height(self.fonts.medium);
        let display_height = viewport.height as f32;
        let mut caption_start_y = if self.can_seek() {
            self.slider.bounds().min.y - 35.0
        } else {
            display_height - medium_height - 20.0
        };
        if !self.settings.title.is_empty() {
            caption_start_y = caption_start_y.min(display_height - 20.0 - medium_height);
        }
        let caption_start_y = caption_start_y.floor();

        if self.reloaded {
            // Reposition the active captions (the display or fonts changed).
            let mut y = caption_start_y;
            for cap in &mut self.active_captions {
                let width = gfx.text_width(self.fonts.caption, &cap.caption);
                let x = caption_offset_x(self.settings.caption_offset, viewport.width, width);
                cap.set_pos(Vec2::new(x as f32, y));
                y -= caption_height;
            }
            self.reloaded = false;
        }

        while let Some(cap) = self.captions.front() {
            if cap.timestamp > self.currtime {
                break;
            }
            let mut cap = self.captions.pop_front().unwrap();
            // Stack below the lowest free row (C++ compares rows exactly).
            let mut y = caption_start_y;
            while self.active_captions.iter().any(|c| c.pos.y == y) {
                y -= caption_height;
            }
            let width = gfx.text_width(self.fonts.caption, &cap.caption);
            let x = caption_offset_x(self.settings.caption_offset, viewport.width, width);
            cap.set_pos(Vec2::new(x as f32, y));
            self.active_captions.push(cap);
        }

        self.active_captions.retain_mut(|cap| {
            cap.logic(dt);
            !cap.is_finished()
        });

        // World update
        self.world.update_bounds();
        self.world.interact_users();
        self.update_users(t, dt);

        self.world.interact_dirs();
        self.world
            .update_dirs(dt, self.settings.elasticity, self.settings.file_idle_time);

        self.update_camera(dt, viewport);

        let display_time = if !self.commitqueue.is_empty() {
            self.currtime
        } else {
            self.lasttime
        };
        if display_time > 0 {
            self.display_date = datetime::format_local(display_time, &self.settings.date_format);
            let w = gfx.text_width(self.fonts.medium, &self.display_date);
            let date_offset = (((w as i32) as f64) * 0.5) as i32;
            if (self.date_x_offset as i32 - date_offset).abs() > 5 {
                self.date_x_offset = date_offset as f32;
            }
        } else {
            self.display_date.clear();
        }

        self.scrubber.sync_playhead_from_time(self.currtime);
        if self.history_preindexed {
            self.last_percent = self.scrubber.state.playhead_fraction;
            self.slider.set_percent(self.last_percent);
        }

        Ok(())
    }

    /// Port of `Gource::updateUsers`: move users, then (in name order)
    /// deselect a fading selected user and select the first active
    /// `--follow-user` when nothing is selected. The finish check and the
    /// idle count run before inactive users are deleted, as in C++.
    fn update_users(&mut self, t: f32, dt: f32) {
        let inactive = self.world.update_users(t, dt, &self.settings);

        let user_ids: Vec<UserId> = self.world.users_by_name.values().copied().collect();
        let mut idle_users = 0;
        for uid in user_ids {
            let user = &self.world.users[uid];
            let fading = user.is_fading(self.settings.user_idle_time);
            let idle = user.is_idle();

            if fading && self.selected_user == Some(uid) {
                self.select_user(None);
            }

            if idle {
                idle_users += 1;
            } else if self.selected_user.is_none() && self.selected_file.is_none() {
                let name = self.world.users[uid].name();
                let followed = self
                    .settings
                    .follow_users
                    .iter()
                    .any(|f| !f.is_empty() && f == name);
                if followed {
                    self.select_user(Some(uid));
                }
            }
        }

        if !self.is_live_mode() && self.world.users.is_empty() && self.stop_position_reached {
            self.is_finished = true;
            self.pending_requests.push(PlatformRequest::Quit);
        }

        if idle_users == self.world.users.len() {
            self.idle_time += dt;
        } else {
            self.idle_time = 0.0;
        }

        for uid in inactive {
            self.delete_user(uid);
        }
    }

    /// Port of `Gource::deleteUser`.
    fn delete_user(&mut self, uid: UserId) {
        if self.hover_user == Some(uid) {
            self.hover_user = None;
        }
        if self.selected_user == Some(uid) {
            self.select_user(None);
        }
        self.world.delete_user(uid);
    }

    /// Update camera tracking and framing.
    pub fn update_camera(&mut self, dt: f32, viewport: Viewport) {
        let mut auto_rotate = !self.manual_rotate && !self.settings.disable_auto_rotate;

        if self.manual_camera {
            if self.cursor_move.length_squared() > 0.0 {
                let cam_rate = (-self.camera.pos().z) / 5000.0;
                let mut pos = self.camera.pos();
                let delta = self.cursor_move * cam_rate * 10.0;
                pos.x += delta.x;
                pos.y += delta.y;
                self.camera.set_pos(pos, true);
                self.camera.stop();
                auto_rotate = false;
                self.cursor_move = Vec2::ZERO;
            }
        } else {
            let cambounds = if self.track_users
                && (self.selected_file.is_some() || self.selected_user.is_some())
            {
                let mut focus = Bounds2D::new();
                if let Some(uid) = self.selected_user
                    && let Some(u) = self.world.users.get(uid)
                {
                    focus.update(u.pawn.pos);
                }
                if let Some(fid) = self.selected_file
                    && let Some(f) = self.world.files.get(fid)
                {
                    let dir_pos = f
                        .dir
                        .and_then(|d| self.world.dirs.get(d))
                        .map(|d| d.pos)
                        .unwrap_or(Vec2::ZERO);
                    focus.update(f.absolute_pos(dir_pos));
                }
                focus
            } else if self.track_users && self.idle_time == 0.0 {
                self.world.active_user_bounds
            } else {
                self.world.dir_bounds
            };

            self.camera.adjust_with_settings(
                &cambounds,
                !self.manual_zoom,
                viewport.size(),
                &self.settings,
            );
        }

        self.camera.logic(dt);

        if auto_rotate {
            if self.rotation_remaining_angle > 0.0 {
                // C++: `max(dt, (float)(1.0f - fabs((r / 90.0f) - 0.5) *
                // 2.0f)) * dt`, where `- 0.5` makes the inner part double.
                let inner = (1.0f64
                    - (((self.rotation_remaining_angle / 90.0) as f64) - 0.5).abs() * 2.0)
                    as f32;
                let angle_rate = dt.max(inner) * dt;
                let step = self.rotation_remaining_angle.min(90.0 * angle_rate);
                self.rotation_remaining_angle -= step;
                self.rotate_angle =
                    ((step as f64) * gource_core::math::CPP_DEGREES_TO_RADIANS) as f32;
            } else if !self.cursor.right_button_pressed() && self.world.dir_bounds.area() > 10000.0
            {
                let aspect = viewport.width as f32 / viewport.height as f32;
                let w = self.world.dir_bounds.width();
                let h = self.world.dir_bounds.height();
                let ratio = if aspect > 1.0 { w / h } else { h / w };
                if ratio < 0.67 {
                    self.rotation_remaining_angle = 90.0;
                }
            }
        } else {
            self.rotation_remaining_angle = 0.0;
        }
    }

    /// Mouse picking / tracing for hover tooltips.
    pub fn mousetrace(&mut self, proj: &Projection) {
        let mouse_world = proj.to_world(self.mouse_pos);

        let mut user_selection = None;
        if !self.settings.hide_users
            && let Some(tree) = &self.world.user_tree
        {
            tree.visit_items_at(mouse_world, |uid| {
                if user_selection.is_none()
                    && let Some(user) = self.world.users.get(uid)
                    && !user.is_fading(self.settings.user_idle_time)
                    && user.pawn.bounds().contains(mouse_world)
                {
                    user_selection = Some(uid);
                }
            });
        }

        let mut file_selection = None;
        if user_selection.is_none()
            && !self.settings.hide_files
            && let Some(tree) = &self.world.dir_tree
        {
            tree.visit_items_at(mouse_world, |did| {
                if file_selection.is_none()
                    && let Some(dir) = self.world.dirs.get(did)
                {
                    for &fid in &dir.files {
                        if let Some(file) = self.world.files.get(fid)
                            && !file.pawn.is_hidden()
                            && file.overlaps(dir.pos, mouse_world)
                        {
                            file_selection = Some(fid);
                            break;
                        }
                    }
                }
            });
        }

        if let Some(fid) = file_selection {
            if let Some(uid) = self.hover_user.take()
                && let Some(u) = self.world.users.get_mut(uid)
            {
                u.pawn.mouseover = false;
            }
            if self.hover_file != Some(fid) {
                if let Some(old_fid) = self.hover_file.take()
                    && let Some(f) = self.world.files.get_mut(old_fid)
                {
                    f.pawn.mouseover = false;
                }
                if let Some(f) = self.world.files.get_mut(fid) {
                    f.pawn.mouseover = true;
                }
                self.hover_file = Some(fid);
            }
        } else if let Some(uid) = user_selection {
            if let Some(fid) = self.hover_file.take()
                && let Some(f) = self.world.files.get_mut(fid)
            {
                f.pawn.mouseover = false;
            }
            if self.hover_user != Some(uid) {
                if let Some(old_uid) = self.hover_user.take()
                    && let Some(u) = self.world.users.get_mut(old_uid)
                {
                    u.pawn.mouseover = false;
                }
                if let Some(u) = self.world.users.get_mut(uid) {
                    u.pawn.mouseover = true;
                }
                self.hover_user = Some(uid);
            }
        } else {
            if let Some(fid) = self.hover_file.take()
                && let Some(f) = self.world.files.get_mut(fid)
            {
                f.pawn.mouseover = false;
            }
            if let Some(uid) = self.hover_user.take()
                && let Some(u) = self.world.users.get_mut(uid)
            {
                u.pawn.mouseover = false;
            }
        }
    }

    /// Render scene and HUD into DrawList.
    pub fn draw(&mut self, _dt: f32, viewport: Viewport, gfx: &mut Gfx, list: &mut DrawList) {
        // Background
        let bg_col = Vec4::new(
            self.settings.background_colour.x,
            self.settings.background_colour.y,
            self.settings.background_colour.z,
            1.0,
        );
        list.reset(glam::UVec2::new(viewport.width, viewport.height), bg_col);
        list.solid_rect(Vec2::ZERO, viewport.size(), bg_col);

        if let Some(bg_tex) = self.textures.background {
            list.rect(bg_tex, Vec2::ZERO, viewport.size(), Vec4::ONE);
        }

        // Loading screen
        if self.commitlog.is_none()
            || (self.is_live_mode()
                && self.ingested_commits.is_empty()
                && self.commitqueue.is_empty()
                && self.world.files.is_empty())
        {
            let dots = match ((self.runtime * 3.0) as i32) % 4 {
                1 => ".",
                2 => "..",
                3 => "...",
                _ => "",
            };
            let action = if self.is_finished {
                "Aborting".to_string()
            } else if !self.settings.github.is_empty() {
                format!("Connecting to GitHub ({})", self.settings.github)
            } else if self.is_live_mode() {
                "Waiting for Commits".to_string()
            } else {
                "Reading Log".to_string()
            };
            let text = format!("{action}{dots}");
            let text_width = gfx.text_width(self.fonts.medium, &text);
            let text_pos = Vec2::new(
                (viewport.width as f32) * 0.5 - (text_width * 0.5),
                (viewport.height as f32) * 0.5 - 10.0,
            );
            let style = TextStyle::new(Vec4::ONE).with_shadow(true);
            gfx.draw_text(list, self.fonts.medium, text_pos, &text, &style);
            return;
        }

        let proj = self.camera.projection(viewport.size());

        // Mouse trace
        if !self.settings.hide_mouse && self.cursor.is_visible() {
            self.mousetrace(&proj);
        }

        // Prepare frame
        self.world.prepare_frame(&proj, &self.settings);

        // Draw scene
        self.world
            .draw_scene(list, &proj, &self.settings, &self.textures.scene);

        // Draw names
        self.world.draw_names(
            list,
            gfx,
            &self.settings,
            &self.fonts.scene,
            self.selected_user,
            self.selected_file,
        );

        // Debug bounds
        if self.debug {
            let cambounds = if self.track_users {
                &self.world.active_user_bounds
            } else {
                &self.world.dir_bounds
            };
            if !cambounds.is_empty() {
                let min_screen = proj.to_screen(cambounds.min);
                let max_screen = proj.to_screen(cambounds.max);
                list.rect_outline(min_screen, max_screen, 2.0, Vec4::ONE);
            }
        }

        // Logo
        if let Some(logo_tex) = self.textures.logo {
            let logo_size = Vec2::new(128.0, 128.0);
            let pos = viewport.size() - logo_size - self.settings.logo_offset;
            list.rect(logo_tex, pos, logo_size, Vec4::ONE);
        }

        // Splash screen
        if self.splash > 0.0 {
            let logo_h = 100.0 * self.settings.font_scale;
            let splash_col = Vec4::new(0.0, 0.5, 1.0, self.splash * 0.015);
            let y_start = (viewport.height as f32) * 0.5 - 40.0 * self.settings.font_scale;
            list.solid_rect(
                Vec2::new(0.0, y_start),
                Vec2::new(viewport.width as f32, logo_h),
                splash_col,
            );

            let style = TextStyle::new(Vec4::ONE).with_shadow(true).with_round(true);
            let title_pos = Vec2::new(
                (viewport.width as f32) * 0.5 - 80.0 * self.settings.font_scale,
                (viewport.height as f32) * 0.5 - 30.0 * self.settings.font_scale,
            );
            gfx.draw_text(list, self.fonts.large, title_pos, "Gource", &style);

            let sub_pos = Vec2::new(
                (viewport.width as f32) * 0.5 - 150.0 * self.settings.font_scale,
                (viewport.height as f32) * 0.5 + 10.0 * self.settings.font_scale,
            );
            gfx.draw_text(
                list,
                self.fonts.base,
                sub_pos,
                "Software Version Control Visualization",
                &style,
            );

            let copy_pos = Vec2::new(
                (viewport.width as f32) * 0.5 - 90.0 * self.settings.font_scale,
                (viewport.height as f32) * 0.5 + 30.0 * self.settings.font_scale,
            );
            gfx.draw_text(
                list,
                self.fonts.base,
                copy_pos,
                "(C) 2009 Andrew Caudwell",
                &style,
            );
        }

        // Date text
        if !self.settings.hide_date && !self.display_date.is_empty() {
            let date_pos = Vec2::new((viewport.width as f32) * 0.5 - self.date_x_offset, 20.0);
            let style = TextStyle::new(Vec4::new(
                self.settings.font_colour.x,
                self.settings.font_colour.y,
                self.settings.font_colour.z,
                1.0,
            ))
            .with_shadow(true);
            gfx.draw_text(
                list,
                self.fonts.medium,
                date_pos,
                &self.display_date,
                &style,
            );
        }

        // Title
        if !self.settings.title.is_empty() {
            let style = TextStyle::new(Vec4::new(
                self.settings.font_colour.x,
                self.settings.font_colour.y,
                self.settings.font_colour.z,
                1.0,
            ))
            .with_shadow(true)
            .with_align_top(false);
            let title_pos = Vec2::new(10.0, viewport.height as f32 - 10.0);
            gfx.draw_text(
                list,
                self.fonts.medium,
                title_pos,
                &self.settings.title,
                &style,
            );
        }

        // Active captions
        for cap in &self.active_captions {
            cap.draw(gfx, list);
        }

        // File key
        self.file_key.set_show(self.settings.show_key);
        self.file_key.draw(gfx, list);

        // Position slider
        if self.can_seek() && !self.timeline_bar.is_visible() {
            self.slider
                .draw(gfx, list, viewport.width as f32, self.settings.font_scale);
        }

        // Tooltip text box
        if let Some(fid) = self.hover_file
            && self.hover_file != self.selected_file
            && let Some(file) = self.world.files.get(fid)
        {
            let mut display_path = file.path.clone();
            if display_path.starts_with('/') {
                display_path.remove(0);
            }
            self.textbox.clear();
            let name_w = gfx.text_width(self.fonts.textbox, &file.pawn.name);
            self.textbox.set_text(&file.pawn.name, name_w);
            if !display_path.is_empty() {
                let path_w = gfx.text_width(self.fonts.textbox, &display_path);
                self.textbox.add_line(display_path, path_w);
            }
            self.textbox.set_colour(file.colour());
            self.textbox.set_pos(
                self.mouse_pos,
                true,
                viewport.width as f32,
                viewport.height as f32,
            );
            self.textbox.draw(gfx, list);
        } else if let Some(uid) = self.hover_user
            && self.hover_user != self.selected_user
            && let Some(user) = self.world.users.get(uid)
        {
            self.textbox.clear();
            let name_w = gfx.text_width(self.fonts.textbox, user.name());
            self.textbox.set_text(user.name(), name_w);
            self.textbox.set_colour(user.colour());
            self.textbox.set_pos(
                self.mouse_pos,
                true,
                viewport.width as f32,
                viewport.height as f32,
            );
            self.textbox.draw(gfx, list);
        }

        // HUD message
        if self.message_timer > 0.0 && !self.message.is_empty() {
            let style = TextStyle::new(Vec4::ONE).with_shadow(true).with_round(true);
            gfx.draw_text(
                list,
                self.fonts.base,
                Vec2::new(1.0, 3.0),
                &self.message,
                &style,
            );
        }

        // Dashboards stack
        self.draw_dashboards(gfx, list, viewport);

        // Timeline Bar
        self.draw_timeline_bar(gfx, list);

        // Tuning Panel
        self.draw_tuning_panel(gfx, list);

        self.mouse_moved = false;
        self.mouse_clicked = false;

        if self.take_screenshot {
            self.trigger_screenshot();
            self.take_screenshot = false;
        }
    }

    /// Advance one frame: logic and tessellation.
    pub fn update(
        &mut self,
        dt: f32,
        viewport: Viewport,
        gfx: &mut Gfx,
        list: &mut DrawList,
    ) -> Result<(), AppError> {
        let mut scaled_dt = dt.min(self.max_tick_rate);
        if self.recording {
            scaled_dt = self.max_tick_rate;
        }
        scaled_dt *= self.settings.time_scale;

        if !self.paused {
            self.runtime += scaled_dt;
        }

        if self.settings.stop_at_time > 0.0 && self.runtime >= self.settings.stop_at_time {
            self.stop_position_reached = true;
        }

        self.logic(scaled_dt, viewport, gfx)?;
        self.draw(scaled_dt, viewport, gfx, list);

        // Frame capture for recording
        if self.recording
            && self.commitlog.is_some()
            && !self.is_finished
            && self.framecount.is_multiple_of(self.frameskip + 1)
        {
            self.pending_requests.push(PlatformRequest::CaptureFrame);
        }

        if !self.settings.hide_mouse {
            self.cursor.logic(dt);
        }

        self.framecount += 1;
        Ok(())
    }

    /// Capture a deterministic snapshot of the simulation state.
    pub fn snapshot(&self) -> SimSnapshot {
        SimSnapshot {
            settings: self.settings.clone(),
            world: self.world.clone(),
            camera: self.camera.clone(),
            slider: self.slider.clone(),
            file_key: self.file_key.clone(),
            textbox: self.textbox.clone(),
            cursor: self.cursor.clone(),
            captions: self.captions.clone(),
            active_captions: self.active_captions.clone(),
            commitqueue: self.commitqueue.clone(),
            track_users: self.track_users,
            manual_camera: self.manual_camera,
            manual_zoom: self.manual_zoom,
            manual_rotate: self.manual_rotate,
            rotation_remaining_angle: self.rotation_remaining_angle,
            rotate_angle: self.rotate_angle,
            cursor_move: self.cursor_move,
            selected_user: self.selected_user,
            hover_user: self.hover_user,
            selected_file: self.selected_file,
            hover_file: self.hover_file,
            grab_mouse: self.grab_mouse,
            mouse_moved: self.mouse_moved,
            mouse_clicked: self.mouse_clicked,
            mouse_dragged: self.mouse_dragged,
            mouse_pos: self.mouse_pos,
            take_screenshot: self.take_screenshot,
            recolour: self.recolour,
            paused: self.paused,
            first_read: self.first_read,
            reloaded: self.reloaded,
            stop_position_reached: self.stop_position_reached,
            is_finished: self.is_finished,
            last_percent: self.last_percent,
            idle_time: self.idle_time,
            currtime: self.currtime,
            lasttime: self.lasttime,
            subseconds: self.subseconds,
            runtime: self.runtime,
            max_tick_rate: self.max_tick_rate,
            frameskip: self.frameskip,
            framecount: self.framecount,
            recording: self.recording,
            splash: self.splash,
            message: self.message.clone(),
            message_timer: self.message_timer,
            display_date: self.display_date.clone(),
            date_x_offset: self.date_x_offset,
            starting_z: self.starting_z,
            debug: self.debug,
            trace_debug: self.trace_debug,
            quadtree_debug: self.quadtree_debug,
            fps: self.fps,
            commit_cursor: self.commit_cursor,
        }
    }

    /// Restore simulation state from a snapshot.
    pub fn restore(&mut self, snapshot: &SimSnapshot) {
        self.settings = snapshot.settings.clone();
        self.world = snapshot.world.clone();
        self.camera = snapshot.camera.clone();
        self.slider = snapshot.slider.clone();
        self.file_key = snapshot.file_key.clone();
        self.textbox = snapshot.textbox.clone();
        self.cursor = snapshot.cursor.clone();
        self.captions = snapshot.captions.clone();
        self.active_captions = snapshot.active_captions.clone();
        self.commitqueue = snapshot.commitqueue.clone();
        self.track_users = snapshot.track_users;
        self.manual_camera = snapshot.manual_camera;
        self.manual_zoom = snapshot.manual_zoom;
        self.manual_rotate = snapshot.manual_rotate;
        self.rotation_remaining_angle = snapshot.rotation_remaining_angle;
        self.rotate_angle = snapshot.rotate_angle;
        self.cursor_move = snapshot.cursor_move;
        self.selected_user = snapshot.selected_user;
        self.hover_user = snapshot.hover_user;
        self.selected_file = snapshot.selected_file;
        self.hover_file = snapshot.hover_file;
        self.grab_mouse = snapshot.grab_mouse;
        self.mouse_moved = snapshot.mouse_moved;
        self.mouse_clicked = snapshot.mouse_clicked;
        self.mouse_dragged = snapshot.mouse_dragged;
        self.mouse_pos = snapshot.mouse_pos;
        self.take_screenshot = snapshot.take_screenshot;
        self.recolour = snapshot.recolour;
        self.paused = snapshot.paused;
        self.first_read = snapshot.first_read;
        self.reloaded = snapshot.reloaded;
        self.stop_position_reached = snapshot.stop_position_reached;
        self.is_finished = snapshot.is_finished;
        self.last_percent = snapshot.last_percent;
        self.idle_time = snapshot.idle_time;
        self.currtime = snapshot.currtime;
        self.lasttime = snapshot.lasttime;
        self.subseconds = snapshot.subseconds;
        self.runtime = snapshot.runtime;
        self.max_tick_rate = snapshot.max_tick_rate;
        self.frameskip = snapshot.frameskip;
        self.framecount = snapshot.framecount;
        self.recording = snapshot.recording;
        self.splash = snapshot.splash;
        self.message = snapshot.message.clone();
        self.message_timer = snapshot.message_timer;
        self.display_date = snapshot.display_date.clone();
        self.date_x_offset = snapshot.date_x_offset;
        self.starting_z = snapshot.starting_z;
        self.debug = snapshot.debug;
        self.trace_debug = snapshot.trace_debug;
        self.quadtree_debug = snapshot.quadtree_debug;
        self.fps = snapshot.fps;
        self.commit_cursor = snapshot.commit_cursor;

        if let Some(ref mut log) = self.commitlog
            && log.is_seekable()
        {
            log.seek_to(0.0);
            for _ in 0..snapshot.commit_cursor {
                let _ = log.next_commit();
            }
        }
    }

    /// Captures a checkpoint if the runtime interval threshold has passed or if no checkpoints exist.
    pub fn maybe_record_checkpoint(&mut self) {
        if self.currtime > 0
            && (self.scrubber.checkpoints.is_empty()
                || (self.runtime - self.scrubber.last_checkpoint_runtime).abs()
                    >= self.scrubber.checkpoint_interval_sim_secs)
        {
            self.scrubber.checkpoints.insert(self.snapshot());
            self.scrubber.last_checkpoint_runtime = self.runtime;
        }
    }

    /// Seeks the simulation to the specified timestamp.
    ///
    /// 1. Clears the reverse buffer.
    /// 2. If a checkpoint exists before `target_ts` within `max_replay_ticks` of simulation logic,
    ///    restores the checkpoint and fast-forwards through simulation ticks up to `target_ts`.
    /// 3. Otherwise, if `history` is available, materializes the complete scene tree from a
    ///    [`gource_history::TreeSnapshot`], clears commit queues, rebuilds file counts, and
    ///    assigns user textures.
    /// 4. Otherwise, falls back to legacy seek.
    pub fn seek_to_timestamp(
        &mut self,
        target_ts: i64,
        max_replay_ticks: usize,
        viewport: Viewport,
        gfx: &mut Gfx,
    ) -> Result<SeekOutcome, AppError> {
        self.scrubber.clear_reverse_buffer();

        // 1. Try restoring from nearest checkpoint before target_ts
        if let Some(cp) = self.scrubber.checkpoints.nearest_before_time(target_ts) {
            let cp_ts = cp.currtime;
            let days_per_second = self.settings.days_per_second.max(0.0001);
            let time_diff = (target_ts - cp_ts).max(0);
            let sim_secs = time_diff as f32 / (days_per_second * 86400.0);
            let tick_rate = self.max_tick_rate.max(1.0 / 60.0);
            let estimated_ticks = (sim_secs / tick_rate).ceil() as usize;

            if cp_ts <= target_ts && estimated_ticks <= max_replay_ticks {
                let cp_snapshot = cp.clone();
                self.restore(&cp_snapshot);

                let mut ticks_replayed = 0;
                while self.currtime < target_ts
                    && !self.is_finished
                    && ticks_replayed < max_replay_ticks
                {
                    self.scrubber.push_reverse_frame(self.snapshot());
                    self.logic(self.max_tick_rate, viewport, gfx)?;
                    self.runtime += self.max_tick_rate;
                    ticks_replayed += 1;
                }

                self.scrubber.sync_playhead_from_time(self.currtime);
                return Ok(SeekOutcome::RestoredAndReplayed {
                    checkpoint_ts: cp_ts,
                    ticks_replayed,
                });
            }
        }

        // 2. Fall back to materializing from History TreeSnapshot
        let hist = self.ensure_history();
        if !hist.is_empty()
            && let Some(tree_snap) = hist.state_at_timestamp(target_ts)
        {
            let commit_index = tree_snap.commit_index;
            self.world
                .materialize_from_snapshot(&tree_snap, &hist, &self.settings, 30);

            self.currtime = target_ts;
            self.lasttime = target_ts;
            self.subseconds = 0.0;
            self.commit_cursor = commit_index + 1;
            self.commitqueue.clear();
            self.stop_position_reached = false;
            self.idle_time = 0.0;
            if let Some(ref mut log) = self.commitlog
                && log.is_seekable()
            {
                log.seek_to(0.0);
                for _ in 0..self.commit_cursor {
                    let _ = log.next_commit();
                }
                self.last_percent = log.percent();
                self.slider.set_percent(self.last_percent);
            }

            // Rebuild file_key counts from world.files
            self.file_key.clear();
            for file in self.world.files.values() {
                if !file.pawn.is_hidden() {
                    let ext = file.ext.clone();
                    let col = file.file_colour;
                    let font_id = self.fonts.medium;
                    self.file_key
                        .inc(&ext, col, |text| gfx.text_width(font_id, text));
                }
            }

            // Assign textures for newly materialized users
            for uid in std::mem::take(&mut self.world.new_users) {
                self.assign_user_image(uid, gfx)?;
            }

            let snap = self.snapshot();
            self.scrubber.checkpoints.insert(snap);
            self.scrubber.last_checkpoint_runtime = self.runtime;
            self.scrubber.sync_playhead_from_time(self.currtime);

            return Ok(SeekOutcome::MaterializedFromHistory { commit_index });
        }

        // 3. Fallback
        Ok(SeekOutcome::FallbackLegacySeek)
    }

    /// Steps playback backward by one frame or seeks slightly backward.
    pub fn step_reverse(&mut self, viewport: Viewport, gfx: &mut Gfx) -> Result<bool, AppError> {
        if let Some(snap) = self.scrubber.pop_reverse_frame() {
            self.restore(&snap);
            self.scrubber.sync_playhead_from_time(self.currtime);
            return Ok(true);
        }

        if self.currtime > 0 {
            let days_per_second = self.settings.days_per_second.max(0.0001);
            let time_delta =
                ((self.max_tick_rate as f64 * 86400.0 * days_per_second as f64) as i64).max(1);
            let target_ts = (self.currtime - time_delta).max(0);

            let outcome = self.seek_to_timestamp(target_ts, 120, viewport, gfx)?;
            let moved = !matches!(outcome, SeekOutcome::FallbackLegacySeek);
            return Ok(moved);
        }

        Ok(false)
    }
}

/// A deterministic snapshot of simulation state for rewind, fast-forward, and replay.
#[derive(Clone)]
pub struct SimSnapshot {
    pub settings: GourceSettings,
    pub world: World,
    pub camera: ZoomCamera,
    pub slider: PositionSlider,
    pub file_key: FileKey,
    pub textbox: TextBox,
    pub cursor: MouseCursor,
    pub captions: VecDeque<RCaption>,
    pub active_captions: Vec<RCaption>,
    pub commitqueue: VecDeque<Commit>,
    pub track_users: bool,
    pub manual_camera: bool,
    pub manual_zoom: bool,
    pub manual_rotate: bool,
    pub rotation_remaining_angle: f32,
    pub rotate_angle: f32,
    pub cursor_move: Vec2,
    pub selected_user: Option<UserId>,
    pub hover_user: Option<UserId>,
    pub selected_file: Option<FileId>,
    pub hover_file: Option<FileId>,
    pub grab_mouse: bool,
    pub mouse_moved: bool,
    pub mouse_clicked: bool,
    pub mouse_dragged: bool,
    pub mouse_pos: Vec2,
    pub take_screenshot: bool,
    pub recolour: bool,
    pub paused: bool,
    pub first_read: bool,
    pub reloaded: bool,
    pub stop_position_reached: bool,
    pub is_finished: bool,
    pub last_percent: f32,
    pub idle_time: f32,
    pub currtime: i64,
    pub lasttime: i64,
    pub subseconds: f32,
    pub runtime: f32,
    pub max_tick_rate: f32,
    pub frameskip: usize,
    pub framecount: usize,
    pub recording: bool,
    pub splash: f32,
    pub message: String,
    pub message_timer: f32,
    pub display_date: String,
    pub date_x_offset: f32,
    pub starting_z: f32,
    pub debug: bool,
    pub trace_debug: bool,
    pub quadtree_debug: bool,
    pub fps: f32,
    pub commit_cursor: usize,
}
