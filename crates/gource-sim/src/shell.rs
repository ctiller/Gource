//! GourceShell: multi-repository sequencing, transitions, and window-level keys
//! (port of `src/gource_shell.h`, `src/gource_shell.cpp`).

use std::collections::VecDeque;

use gource_draw::Gfx;
use gource_draw::list::DrawList;
use gource_settings::{ConfSection, Config, GourceSettings};

use crate::app::{AppError, AppOptions};
use crate::gource::Gource;
use crate::input::{InputEvent, Key};
use crate::platform::{PlatformRequest, Viewport};

/// The GourceShell orchestrator.
pub struct GourceShell {
    pub config: Config,
    pub options: AppOptions,
    pub gource: Option<Gource>,
    pub current_repo_idx: usize,
    pub repo_count: usize,
    pub next: bool,
    pub is_finished: bool,
    pub transition_interval: f32,
    pub last_frame_list: Option<DrawList>,
    pub toggle_delay: f32,
    pub requests: VecDeque<PlatformRequest>,
    pub gfx: Gfx,
    /// The viewport of the previous frame: a change is the C++
    /// `GourceShell::resize`, which reloads (see [`GourceShell::reload`]).
    pub last_viewport: Option<Viewport>,
}

impl GourceShell {
    /// Create a new GourceShell from configuration and options.
    pub fn new(config: Config, options: AppOptions) -> Result<Self, AppError> {
        let repo_count = config.conf.count_sections("gource").max(1);
        let gfx = Gfx::new();

        let mut shell = Self {
            config,
            options,
            gource: None,
            current_repo_idx: 0,
            repo_count,
            next: false,
            is_finished: false,
            transition_interval: 0.0,
            last_frame_list: None,
            toggle_delay: 0.0,
            requests: VecDeque::new(),
            gfx,
            last_viewport: None,
        };

        // Prepare the first repository
        let initial_viewport = Viewport::new(
            shell.config.display.display_width.max(1) as u32,
            shell.config.display.display_height.max(1) as u32,
        );
        let gource = shell.get_next(initial_viewport)?;
        shell.gource = gource;

        Ok(shell)
    }

    /// Advance to the next repository in sequence (`getNext`).
    pub fn get_next(&mut self, viewport: Viewport) -> Result<Option<Gource>, AppError> {
        if self.gource.is_some() {
            self.transition_interval = 1.0;
            self.gource = None;
        }

        let gource_sections: Vec<&ConfSection> =
            self.config.conf.sections_named("gource").collect();

        if self.current_repo_idx >= gource_sections.len() {
            if self.repo_count > 1 && !self.options.recording {
                self.current_repo_idx = 0;
            } else {
                return Ok(None);
            }
        }

        let section = gource_sections.get(self.current_repo_idx).copied();
        let mut settings =
            GourceSettings::import(&self.config.conf, section).map_err(|e| AppError(e.0))?;

        if self.options.recording
            && !(settings.dont_stop || settings.looping || settings.path == "-")
        {
            settings.stop_at_end = true;
        }

        if settings.repo_count > 1 && settings.stop_at_time <= 0.0 && settings.stop_position <= 0.0
        {
            settings.stop_at_time = 60.0;
        }

        self.current_repo_idx += 1;
        self.next = false;

        let output_framerate = self.config.display.output_framerate.max(1) as u32;
        let gource = Gource::new(
            settings,
            &mut self.gfx,
            &self.options,
            viewport,
            output_framerate,
        )?;
        Ok(Some(gource))
    }

    /// Handle one input event.
    pub fn input(&mut self, event: &InputEvent) {
        if let InputEvent::KeyDown {
            key,
            modifiers,
            repeat,
        } = event
            && !*repeat
        {
            if *key == Key::Escape {
                self.is_finished = true;
                self.requests.push_back(PlatformRequest::Quit);
                return;
            }

            let disable_input = self
                .gource
                .as_ref()
                .is_some_and(|g| g.settings.disable_input);
            if disable_input {
                return;
            }

            if *key == Key::F5 {
                self.reload();
                return;
            }

            if *key == Key::F11 {
                if self.toggle_delay <= 0.0 && !self.options.recording {
                    self.requests.push_back(PlatformRequest::ToggleFrameless);
                    self.toggle_delay = 0.25;
                    if let Some(g) = self.gource.as_mut() {
                        g.reload();
                    }
                }
                return;
            }

            if *key == Key::Return {
                if modifiers.alt {
                    if !self.options.recording {
                        self.requests.push_back(PlatformRequest::ToggleFullscreen);
                        if let Some(g) = self.gource.as_mut() {
                            g.reload();
                        }
                    }
                } else if self.repo_count > 1 {
                    self.next = true;
                }
                return;
            }
        }

        if let Some(ref mut g) = self.gource {
            g.input(event);
        }
    }

    /// Port of `GourceShell::reload()` (F5): re-read the textures from their
    /// files, then let the visualization reposition its captions. Like a
    /// texture that fails to load at startup, one that can no longer be read
    /// is fatal (C++ throws a `TextureException`).
    pub fn reload(&mut self) {
        if let Some(err) = self.gfx.textures.reload_files().into_iter().next() {
            let path = match &err {
                gource_draw::TextureError::Io { path, .. } => path.clone(),
                gource_draw::TextureError::Decode { name, .. } => name.clone(),
            };
            self.is_finished = true;
            self.requests.push_back(PlatformRequest::Fatal(format!(
                "failed to load resource '{path}'"
            )));
            self.requests.push_back(PlatformRequest::Quit);
            return;
        }
        if let Some(g) = self.gource.as_mut() {
            g.reload();
        }
    }

    /// Advance one displayed frame and tessellate into list.
    pub fn frame(&mut self, dt: f32, viewport: Viewport, list: &mut DrawList) {
        if self.is_finished {
            return;
        }

        // A new display size is the C++ `GourceShell::resize`, which reloads.
        if self.last_viewport.is_some_and(|v| v != viewport)
            && let Some(g) = self.gource.as_mut()
        {
            g.reload();
        }
        self.last_viewport = Some(viewport);

        if self.toggle_delay > 0.0 {
            self.toggle_delay -= dt;
        }

        let needs_next = match &self.gource {
            Some(g) => g.is_finished || self.next,
            None => true,
        };

        if needs_next {
            // Save last frame for transition fading if switching
            if self.gource.is_some() {
                self.last_frame_list = Some(list.clone());
            }

            match self.get_next(viewport) {
                Ok(Some(next_gource)) => {
                    self.gource = Some(next_gource);
                }
                Ok(None) => {
                    self.is_finished = true;
                    self.requests.push_back(PlatformRequest::Quit);
                    return;
                }
                Err(err) => {
                    self.is_finished = true;
                    self.requests.push_back(PlatformRequest::Fatal(err.0));
                    self.requests.push_back(PlatformRequest::Quit);
                    return;
                }
            }
        }

        if let Some(ref mut g) = self.gource {
            if let Err(err) = g.update(dt, viewport, &mut self.gfx, list) {
                self.is_finished = true;
                self.requests.push_back(PlatformRequest::Fatal(err.0));
                self.requests.push_back(PlatformRequest::Quit);
                return;
            }

            // Drain requests from gource
            for req in g.pending_requests.drain(..) {
                if req == PlatformRequest::Quit {
                    self.is_finished = true;
                }
                self.requests.push_back(req);
            }

            // Transition blending: fade previous frame over current frame
            if self.transition_interval > 0.0 {
                if let Some(ref prev_list) = self.last_frame_list {
                    list.append_faded(prev_list, self.transition_interval);
                }
                self.transition_interval -= dt;
            } else {
                self.last_frame_list = None;
            }
        }
    }
}
