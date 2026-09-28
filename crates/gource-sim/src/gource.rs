//! Gource per-repository simulation and visualization state machine
//! (port of `src/gource.h`, `src/gource.cpp`).

use std::collections::VecDeque;
use std::f32::consts::PI;
use std::path::{Path, PathBuf};

use glam::{Vec2, Vec3, Vec4};
use gource_core::StringHasher;
use gource_core::bounds::Bounds2D;
use gource_core::datetime;
use gource_draw::font::{FontId, TextStyle};
use gource_draw::list::DrawList;
use gource_draw::{Gfx, Projection, TextureId};
use gource_settings::{CameraMode, GourceSettings};
use gource_vcs::CommitLog;
use gource_vcs::commit::{Commit, FileAction};
use gource_vcs::logmill::LogMill;
use gource_widgets::caption::RCaption;
use gource_widgets::cursor::{MouseButtonKind, MouseCursor};
use gource_widgets::key::FileKey;
use gource_widgets::slider::PositionSlider;
use gource_widgets::textbox::TextBox;

use crate::app::AppError;
use crate::camera::ZoomCamera;
use crate::file::FileId;
use crate::input::{InputEvent, Key, MouseButton};
use crate::platform::{PlatformRequest, Viewport};
use crate::user::UserId;
use crate::world::{SceneFonts, SceneTextures, World};

/// Fonts used by the Gource HUD and scene.
#[derive(Debug, Clone, Copy)]
pub struct GourceFonts {
    pub large: FontId,
    pub medium: FontId,
    pub caption: FontId,
    pub base: FontId,
    pub dirname: FontId,
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
        let user_size = (settings.scaled_user_font_size as u32).max(1);
        let file_size = (settings.scaled_filename_font_size as u32).max(1);

        let selected_size = ((18.0 * settings.font_scale) as u32).max(1);

        let large = gfx.fonts.font(face_id, large_size);
        let medium = gfx.fonts.font(face_id, medium_size);
        let caption = gfx.fonts.font(face_id, caption_size);
        let base = gfx.fonts.font(face_id, base_size);
        let dirname = gfx.fonts.font(face_id, dirname_size);

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
    /// Load textures needed by Gource into Gfx.
    pub fn load(gfx: &mut Gfx, settings: &GourceSettings) -> Result<Self, AppError> {
        let file_tex = gfx
            .textures
            .load_bytes(
                "file.png",
                gource_draw::resources::FILE_PNG,
                gource_draw::TextureOptions::plain(),
            )
            .map_err(|e| AppError(e.to_string()))?;

        let beam_tex = gfx
            .textures
            .load_bytes(
                "beam.png",
                gource_draw::resources::BEAM_PNG,
                gource_draw::TextureOptions::plain(),
            )
            .map_err(|e| AppError(e.to_string()))?;

        let user_tex = if !settings.default_user_image.is_empty() {
            gfx.textures
                .load_file(
                    Path::new(&settings.default_user_image),
                    gource_draw::TextureOptions::plain(),
                )
                .map_err(|e| AppError(e.to_string()))?
        } else {
            gfx.textures
                .load_bytes(
                    "user.png",
                    gource_draw::resources::USER_PNG,
                    gource_draw::TextureOptions::plain(),
                )
                .map_err(|e| AppError(e.to_string()))?
        };

        let logo = if !settings.logo.is_empty() {
            Some(
                gfx.textures
                    .load_file(
                        Path::new(&settings.logo),
                        gource_draw::TextureOptions::plain(),
                    )
                    .map_err(|e| AppError(e.to_string()))?,
            )
        } else {
            None
        };

        let background = if !settings.background_image.is_empty() {
            Some(
                gfx.textures
                    .load_file(
                        Path::new(&settings.background_image),
                        gource_draw::TextureOptions::plain(),
                    )
                    .map_err(|e| AppError(e.to_string()))?,
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

    pub pending_requests: Vec<PlatformRequest>,
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

        let starting_z = -300.0f32;
        let mut camera = ZoomCamera::new(
            Vec3::new(0.0, 0.0, starting_z),
            Vec3::ZERO,
            settings.camera_zoom_min,
            settings.camera_zoom_max,
        );
        camera.set_distance(settings.camera_zoom_default);
        camera.set_padding(settings.padding);

        let track_users = settings.camera_mode == CameraMode::Track;

        let mut slider = PositionSlider::new(0.0);
        slider.set_font(fonts.caption);
        slider.resize(viewport.width as f32, viewport.height as f32, 35.0);

        if !recording && settings.repo_count <= 1 {
            slider.show();
        }

        let mut file_key = FileKey::new(1.0);
        file_key.set_font(
            fonts.base,
            settings.scaled_font_size as f32,
            settings.font_scale,
        );
        file_key.set_show(settings.show_key);

        let mut textbox = TextBox::new();
        textbox.set_font(fonts.large, 18.0 * settings.font_scale);
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

        let world = World::new(fastrand::u64(..), settings.hash_seed);

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
            pending_requests: Vec::new(),
        };

        if !g.settings.caption_file.is_empty() {
            g.load_captions();
        }

        Ok(g)
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

        self.world = World::new(fastrand::u64(..), self.settings.hash_seed);
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

    /// Check if seeking is possible.
    pub fn can_seek(&self) -> bool {
        if self.settings.hide_progress {
            return false;
        }
        if let Some(ref log) = self.commitlog {
            log.is_seekable()
        } else {
            false
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
        if percent < 1.0
            && let Some(ref mut log) = self.commitlog
            && let Some(commit) = log.commit_at(percent)
        {
            return datetime::format_local(commit.timestamp, "%A, %d %B, %Y");
        }
        String::new()
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
        let new_seed = fastrand::i32(1..=10000);
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

                if *key == Key::Return && modifiers.alt && !*repeat {
                    self.pending_requests
                        .push(PlatformRequest::ToggleFullscreen);
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

                let right_mouse = self.cursor.right_button_pressed();

                if self.mouse_dragged || right_mouse {
                    if right_mouse {
                        self.manual_rotate = true;
                        let angle = if delta.x.abs() > delta.y.abs() {
                            (delta.x.abs() / 10.0).min(1.0) * 5.0 * (PI / 180.0) * delta.x.signum()
                        } else {
                            (delta.y.abs() / 10.0).min(1.0) * 5.0 * (PI / 180.0) * delta.y.signum()
                        };
                        self.rotate_angle = angle;
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

                if !self.settings.hide_progress
                    && let Some(p) = self.slider.mouse_over(*pos)
                {
                    let date = self.date_at_position(p);
                    let cap_w = self.date_text_width(&date);
                    self.slider.set_caption(date, cap_w);
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

                        if !self.settings.hide_progress
                            && let Some(p) = self.slider.click(*pos)
                        {
                            self.seek_to(p);
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

    fn date_text_width(&self, date: &str) -> f32 {
        date.len() as f32 * 8.0 * self.settings.font_scale
    }

    /// Read available commits from the CommitLog into commitqueue.
    pub fn read_log(&mut self) -> Result<(), AppError> {
        let log = match self.commitlog.as_mut() {
            Some(l) => l,
            None => return Ok(()),
        };

        while self.commitqueue.len() < 100 {
            match log.next_commit() {
                Some(commit) => {
                    self.commitqueue.push_back(commit);
                }
                None => break,
            }
        }

        if self.first_read && self.commitqueue.is_empty() && log.is_finished() {
            return Err(AppError("no commits found".to_string()));
        }
        self.first_read = false;

        if !log.is_finished() && log.is_seekable() {
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
    pub fn process_commit(&mut self, commit: &Commit, t: f32) {
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
                        let col = self.world.files[fid].colour();
                        let font_scale = self.settings.font_scale;
                        self.file_key
                            .inc(&ext, col, |text| text.len() as f32 * 8.0 * font_scale);
                    }
                    fid_opt
                }
            };

            if let Some(fid) = file_id {
                self.world
                    .add_file_action(commit, cf, fid, t, &self.settings);
            }
        }
    }

    pub fn logic(&mut self, dt: f32, viewport: Viewport) -> Result<(), AppError> {
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

        if self.paused {
            self.world.update_bounds();
            self.world.interact_users();
            self.world.interact_dirs();
            self.update_camera(dt, viewport);
            return Ok(());
        }

        // Fetch commits
        if self.commitqueue.is_empty() {
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

        let time_inc = dt * 86400.0 * self.settings.days_per_second;
        let seconds = time_inc as i64;
        self.subseconds += time_inc - (seconds as f32);

        if self.subseconds >= 1.0 {
            self.currtime += self.subseconds as i64;
            self.subseconds -= self.subseconds.floor();
        }
        self.currtime += seconds;

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
            if self.settings.auto_skip_seconds >= 0.0
                && self.idle_time >= self.settings.auto_skip_seconds
                && !self.stop_position_reached
            {
                self.currtime = commit.timestamp;
                self.lasttime = commit.timestamp;
                self.idle_time = 0.0;
            }

            if commit.timestamp > self.currtime {
                break;
            }

            let commit = self.commitqueue.pop_front().unwrap();
            self.process_commit(&commit, t);

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

        // Captions logic
        let caption_height = 20.0 * self.settings.font_scale;
        let mut caption_start_y = if self.can_seek() {
            self.slider.bounds().min.y - 35.0
        } else {
            viewport.height as f32 - 40.0 * self.settings.font_scale
        };
        if !self.settings.title.is_empty() {
            caption_start_y =
                caption_start_y.min(viewport.height as f32 - 40.0 * self.settings.font_scale);
        }

        while let Some(cap) = self.captions.front() {
            if cap.timestamp > self.currtime {
                break;
            }
            let mut cap = self.captions.pop_front().unwrap();
            let mut y = caption_start_y;
            while self
                .active_captions
                .iter()
                .any(|c| (c.pos.y - y).abs() < 1.0)
            {
                y -= caption_height;
            }
            let mut offset_x = self.settings.caption_offset as f32;
            if offset_x == 0.0 {
                offset_x = (viewport.width as f32) * 0.5 - (cap.caption.len() as f32 * 4.0);
            } else if offset_x < 0.0 {
                offset_x = (viewport.width as f32) + offset_x - (cap.caption.len() as f32 * 8.0);
            }
            cap.set_pos(Vec2::new(offset_x, y));
            self.active_captions.push(cap);
        }

        self.active_captions.retain_mut(|cap| {
            cap.logic(dt);
            !cap.is_finished()
        });

        // World update
        self.world.update_bounds();
        self.world.interact_users();
        let inactive = self.world.update_users(t, dt, &self.settings);
        for uid in inactive {
            self.world.delete_user(uid);
        }

        if self.world.users.is_empty() && self.stop_position_reached {
            self.is_finished = true;
            self.pending_requests.push(PlatformRequest::Quit);
        }

        let idle_users = self.world.users.values().filter(|u| u.is_idle()).count();
        if idle_users == self.world.users.len() {
            self.idle_time += dt;
        } else {
            self.idle_time = 0.0;
        }

        self.world.interact_dirs();
        self.world.update_dirs(dt, self.settings.elasticity);

        self.update_camera(dt, viewport);

        let display_time = if !self.commitqueue.is_empty() {
            self.currtime
        } else {
            self.lasttime
        };
        if display_time > 0 {
            self.display_date = datetime::format_local(display_time, &self.settings.date_format);
            let target_offset = self.display_date.len() as f32 * 4.5 * self.settings.font_scale;
            if (self.date_x_offset - target_offset).abs() > 5.0 {
                self.date_x_offset = target_offset;
            }
        } else {
            self.display_date.clear();
        }

        Ok(())
    }

    /// Update camera tracking and framing.
    pub fn update_camera(&mut self, dt: f32, viewport: Viewport) {
        let auto_rotate = !self.manual_rotate && !self.settings.disable_auto_rotate;

        if self.manual_camera {
            if self.cursor_move.length_squared() > 0.0 {
                let cam_rate = (-self.camera.pos().z) / 5000.0;
                let mut pos = self.camera.pos();
                let delta = self.cursor_move * cam_rate * 10.0;
                pos.x += delta.x;
                pos.y += delta.y;
                self.camera.set_pos(pos, true);
                self.camera.stop();
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
                let angle_rate =
                    (dt.max(1.0 - ((self.rotation_remaining_angle / 90.0) - 0.5).abs() * 2.0)) * dt;
                let step = self.rotation_remaining_angle.min(90.0 * angle_rate);
                self.rotation_remaining_angle -= step;
                self.rotate_angle = step * (PI / 180.0);
            } else if !self.cursor.right_button_pressed() && self.world.dir_bounds.area() > 10000.0
            {
                let aspect = viewport.width as f32 / (viewport.height as f32).max(1.0);
                let w = self.world.dir_bounds.width();
                let h = self.world.dir_bounds.height();
                let ratio = if aspect > 1.0 {
                    w / h.max(1.0)
                } else {
                    h / w.max(1.0)
                };
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
        if self.commitlog.is_none() {
            let dots = match ((self.runtime * 3.0) as i32) % 4 {
                1 => ".",
                2 => "..",
                3 => "...",
                _ => "",
            };
            let action = if !self.is_finished {
                "Reading Log"
            } else {
                "Aborting"
            };
            let text = format!("{action}{dots}");
            let text_pos = Vec2::new(
                (viewport.width as f32) * 0.5 - 50.0,
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

            let style = TextStyle::new(Vec4::ONE).with_shadow(true);
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
        if self.can_seek() {
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
            let name_w = gfx.text_width(self.fonts.large, &file.pawn.name);
            self.textbox.set_text(&file.pawn.name, name_w);
            if !display_path.is_empty() {
                let path_w = gfx.text_width(self.fonts.large, &display_path);
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
            let name_w = gfx.text_width(self.fonts.large, user.name());
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
            let style = TextStyle::new(Vec4::ONE).with_shadow(true);
            gfx.draw_text(
                list,
                self.fonts.base,
                Vec2::new(1.0, 3.0),
                &self.message,
                &style,
            );
        }

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

        self.logic(scaled_dt, viewport)?;
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
}
