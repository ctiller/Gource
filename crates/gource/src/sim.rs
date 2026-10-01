//! The interface between the Bevy shell and a simulation.

use std::sync::{Mutex, MutexGuard};

use bevy::prelude::Resource;
use gource_app::{InputEvent, PlatformRequest, Viewport};
use gource_draw::{DrawList, Gfx};

/// What the frontend drives every frame. Implemented by the Gource
/// simulation, and by test scenes.
///
/// All coordinates are physical pixels with the origin at the top-left of
/// the window (the draw list coordinate space).
pub trait Simulation: Send + 'static {
    /// Handle one input event.
    fn input(&mut self, event: &InputEvent);

    /// Advance by `dt` seconds of wall-clock time and tessellate the next
    /// frame into `list` (which the implementation resets).
    fn frame(&mut self, dt: f32, viewport: Viewport, list: &mut DrawList);

    /// Requests for the platform layer produced since the last call.
    fn take_requests(&mut self) -> Vec<PlatformRequest>;

    /// The textures and fonts the draw lists refer to.
    fn gfx(&self) -> &Gfx;
}

/// The running simulation. A mutex makes the `Send` simulation usable as a
/// Bevy resource; it is only ever locked from one system at a time.
#[derive(Resource)]
pub struct SimResource(Mutex<Box<dyn Simulation>>);

impl SimResource {
    pub fn new(sim: Box<dyn Simulation>) -> Self {
        Self(Mutex::new(sim))
    }

    pub fn lock(&self) -> MutexGuard<'_, Box<dyn Simulation>> {
        // A panic inside the simulation poisons the lock; keep going with the
        // inner value rather than cascading panics through every system.
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Simulation for gource_app::GourceApp {
    fn input(&mut self, event: &InputEvent) {
        gource_app::GourceApp::input(self, event);
    }

    fn frame(&mut self, dt: f32, viewport: Viewport, list: &mut DrawList) {
        gource_app::GourceApp::frame(self, dt, viewport, list);
    }

    fn take_requests(&mut self) -> Vec<PlatformRequest> {
        gource_app::GourceApp::take_requests(self)
    }

    fn gfx(&self) -> &Gfx {
        gource_app::GourceApp::gfx(self)
    }
}
