//! Renderer warm-up before recording.
//!
//! The C++ version draws with OpenGL straight away, so the first frame of a
//! `--output-ppm-stream` video already shows the scene. Bevy loads shaders,
//! compiles pipelines and creates the window surface asynchronously, which
//! left the first recorded frames blank. When recording, the frontend
//! therefore holds the simulation back and draws a probe (a white square and
//! a green glow in the top-left corner) until a capture of the window shows
//! both. At that point the camera renders into the capture target and the
//! pipelines of both materials are ready, so the simulation's first frame is
//! recorded correctly.

use std::time::Duration;

use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured},
    window::PrimaryWindow,
};
use gource_draw::DrawList;

use crate::{
    app::{ExitState, viewport_of},
    capture::{Deadline, Frame, frame_from_image},
    render::{FrameDrawList, FrameTextures, GpuTextures},
    sim::SimResource,
};

/// Give up waiting for the renderer after this long; the first video frames
/// may then be blank.
pub const WARMUP_TIMEOUT: Duration = Duration::from_secs(10);

/// A probe capture that has not arrived after this many frames is assumed
/// lost (e.g. taken before the window surface existed) and retried.
pub const PROBE_RETRY_FRAMES: u32 = 30;

const PROBE_SQUARE_SIZE: f32 = 4.0;
const PROBE_GLOW_CENTRE: Vec2 = Vec2::new(16.0, 4.0);
const PROBE_GLOW_RADIUS: f32 = 4.0;

/// Draw the probe: a white square (scene material) and a green glow (bloom
/// material) on black.
pub fn draw_probe(list: &mut DrawList, viewport: UVec2) {
    list.reset(viewport, Vec4::new(0.0, 0.0, 0.0, 1.0));
    list.solid_rect(Vec2::ZERO, Vec2::splat(PROBE_SQUARE_SIZE), Vec4::ONE);
    list.bloom(
        PROBE_GLOW_CENTRE,
        PROBE_GLOW_RADIUS,
        Vec4::new(0.0, 1.0, 0.0, 1.0),
    );
}

/// Whether a capture shows the probe drawn by [`draw_probe`].
pub fn probe_visible(frame: &Frame) -> bool {
    let pixel = |x: u32, y: u32| -> Option<&[u8]> {
        if x >= frame.width || y >= frame.height {
            return None;
        }
        let i = (y as usize * frame.width as usize + x as usize) * 4;
        frame.rgba.get(i..i + 4)
    };
    let square = pixel(1, 1).is_some_and(|p| p[..3].iter().all(|&c| c >= 192));
    let glow = pixel(PROBE_GLOW_CENTRE.x as u32, PROBE_GLOW_CENTRE.y as u32)
        .is_some_and(|p| p[1] >= 32 && p[0] < 32 && p[2] < 32);
    square && glow
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Phase {
    /// No probe in flight.
    #[default]
    Idle,
    /// A probe capture was requested this many frames ago.
    Probing(u32),
    /// The simulation may run.
    Ready,
}

/// Holds the simulation back until the renderer is ready (see the module
/// documentation).
#[derive(Resource, Debug, Default)]
pub struct Warmup {
    phase: Phase,
    /// A probe capture showed the probe; [`run_warmup`] finishes the warm-up
    /// at the start of the next frame's produce step, so the simulation never
    /// starts in a frame that also spawned a probe capture (only one capture
    /// per window and frame is possible).
    passed: bool,
    probes: u32,
    deadline: Option<Deadline>,
}

impl Warmup {
    /// A warm-up that has already finished.
    pub fn ready() -> Self {
        Self {
            phase: Phase::Ready,
            ..default()
        }
    }

    pub fn is_ready(&self) -> bool {
        self.phase == Phase::Ready
    }

    /// Number of probe captures requested so far.
    pub fn probes(&self) -> u32 {
        self.probes
    }

    /// Record what a probe capture showed.
    pub fn record(&mut self, visible: bool) {
        if visible {
            self.passed = true;
        } else if matches!(self.phase, Phase::Probing(_)) {
            self.phase = Phase::Idle;
        }
    }

    /// Advance by one frame. Returns whether a probe capture should be
    /// requested now.
    fn step(&mut self) -> bool {
        match self.phase {
            Phase::Ready => false,
            _ if self.passed => {
                self.phase = Phase::Ready;
                false
            }
            Phase::Probing(frames) if frames < PROBE_RETRY_FRAMES => {
                self.phase = Phase::Probing(frames + 1);
                false
            }
            Phase::Idle | Phase::Probing(_) => {
                self.phase = Phase::Probing(0);
                self.probes += 1;
                true
            }
        }
    }
}

/// While warming up: draw the probe instead of running the simulation and
/// capture it, one capture at a time, until a capture shows it.
#[allow(clippy::too_many_arguments)]
pub fn run_warmup(
    mut commands: Commands,
    mut warmup: ResMut<Warmup>,
    sim: Res<SimResource>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut list: ResMut<FrameDrawList>,
    mut textures: ResMut<FrameTextures>,
    gpu: Res<GpuTextures>,
    exit: Res<ExitState>,
) {
    if warmup.is_ready() || exit.is_quitting() {
        return;
    }
    let deadline = *warmup
        .deadline
        .get_or_insert_with(|| Deadline::after(WARMUP_TIMEOUT));
    if deadline.passed() && !warmup.passed {
        warn!(
            "the renderer was not ready after {}s; the first video frames may be blank",
            WARMUP_TIMEOUT.as_secs()
        );
        warmup.phase = Phase::Ready;
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let viewport = viewport_of(window);
    if viewport.width == 0 || viewport.height == 0 {
        return;
    }
    if !warmup.step() {
        if warmup.is_ready() {
            return;
        }
    } else {
        commands.spawn(Screenshot::primary_window()).observe(
            |captured: On<ScreenshotCaptured>, mut warmup: ResMut<Warmup>| {
                let visible = frame_from_image(&captured.image).is_some_and(|f| probe_visible(&f));
                warmup.record(visible);
            },
        );
    }
    // Meanwhile upload the textures the simulation loaded at start-up.
    textures.collect(&sim.lock().gfx().textures, &gpu);
    draw_probe(&mut list.0, UVec2::new(viewport.width, viewport.height));
}

#[cfg(test)]
mod tests {
    use super::*;
    use gource_draw::Material;

    fn black(width: u32, height: u32) -> Frame {
        let mut rgba = vec![0; width as usize * height as usize * 4];
        for px in rgba.chunks_exact_mut(4) {
            px[3] = 255;
        }
        Frame {
            width,
            height,
            rgba,
        }
    }

    fn set(frame: &mut Frame, x: u32, y: u32, rgb: [u8; 3]) {
        let i = (y * frame.width + x) as usize * 4;
        frame.rgba[i..i + 3].copy_from_slice(&rgb);
    }

    #[test]
    fn probe_uses_both_materials() {
        let mut list = DrawList::default();
        draw_probe(&mut list, UVec2::new(64, 32));
        assert_eq!(list.clear_colour, Vec4::new(0.0, 0.0, 0.0, 1.0));
        let materials: Vec<Material> = list.batches.iter().map(|b| b.material).collect();
        assert!(materials.contains(&Material::Alpha));
        assert!(materials.contains(&Material::Bloom));
    }

    #[test]
    fn probe_detection() {
        let mut frame = black(32, 8);
        assert!(!probe_visible(&frame), "blank frame");
        set(&mut frame, 1, 1, [255, 255, 255]);
        assert!(!probe_visible(&frame), "square without glow");
        set(&mut frame, 16, 4, [0, 108, 0]);
        assert!(probe_visible(&frame));
        set(&mut frame, 16, 4, [255, 255, 255]);
        assert!(!probe_visible(&frame), "glow must be green");
        set(&mut frame, 16, 4, [0, 108, 0]);
        set(&mut frame, 1, 1, [25, 25, 25]);
        assert!(!probe_visible(&frame), "clear colour only");
        assert!(!probe_visible(&black(4, 4)), "too small to hold the probe");
    }

    #[test]
    fn warmup_probes_one_at_a_time_until_visible() {
        let mut warmup = Warmup::default();
        assert!(!warmup.is_ready());
        assert!(warmup.step(), "first frame requests a probe");
        assert!(!warmup.step(), "one probe in flight at a time");
        warmup.record(false);
        assert!(warmup.step(), "a failed probe is retried");
        assert_eq!(warmup.probes(), 2);
        warmup.record(true);
        assert!(!warmup.is_ready(), "ready only at the next step");
        assert!(!warmup.step());
        assert!(warmup.is_ready());
        assert!(!warmup.step());
        warmup.record(false);
        assert!(warmup.is_ready(), "late results are ignored once ready");
    }

    #[test]
    fn lost_probes_are_retried() {
        let mut warmup = Warmup::default();
        assert!(warmup.step());
        for _ in 0..PROBE_RETRY_FRAMES {
            assert!(!warmup.step());
        }
        assert!(warmup.step(), "retry after PROBE_RETRY_FRAMES frames");
        assert_eq!(warmup.probes(), 2);
        assert!(Warmup::ready().is_ready());
    }
}
