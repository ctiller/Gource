//! Engine-agnostic Gource simulation.
//!
//! Owns all state (no globals): the directory tree, files, users, actions and
//! camera. The scene advances on a fixed integer tick (`gource-scene`); each
//! frame interpolates a float view of it, tessellated into a
//! [`gource_draw::DrawList`] each frame. Frontends feed [`input::InputEvent`]s
//! in and execute [`platform::PlatformRequest`]s coming out.
//!
//! Module map (C++ source in parentheses):
//! * [`camera`] — `ZoomCamera` (zoomcamera.cpp)
//! * [`pawn`] — shared pawn state (pawn.cpp)
//! * [`file`] — `RFile` (file.cpp)
//! * [`user`] — `RUser` (user.cpp)
//! * [`action`] — `RAction` and subclasses (action.cpp)
//! * [`spline`] — `SplineEdge` (spline.cpp)
//! * [`dirnode`] — `RDirNode` tree bookkeeping (dirnode.cpp)
//! * [`world`] — arenas tying dirs/files/users/actions together, the
//!   fixed-tick integer simulation step (`gource-scene` kernels) and drawing
//! * [`view`] — the float view of the integer scene (interpolation, rotation)
//! * [`app`] — [`GourceApp`], the frontend-facing API over the `Gource`
//!   state machine and `GourceShell` (gource.cpp, gource_shell.cpp).

pub mod app;
pub mod camera;
pub mod checkpoint;
pub mod gource;
pub mod input;
pub mod platform;
pub mod scene_draw;
pub mod scene_history;
pub mod scrubber;
pub mod shell;

// Re-export scene modules so downstream code and tests accessing gource_app::world etc. continue working
pub use gource_scene::{action, dirnode, file, kernel, pawn, profile, spline, step, user, view};

pub use action::{Action, ActionKind};
pub use app::{AppError, AppOptions, GourceApp, vcs_options};
pub use checkpoint::CheckpointStore;
pub use dirnode::DirNode;
pub use file::{DirId, File, FileId};
pub use gource::SimSnapshot;
pub use input::{InputEvent, Key, Modifiers, MouseButton};
pub use pawn::Pawn;
pub use platform::{PlatformRequest, Viewport};
pub use scene_draw::{SceneFonts, SceneTextures, WorldDraw};
pub use scene_history::WorldHistory;

pub mod world {
    pub use crate::scene_draw::{SceneFonts, SceneTextures, WorldDraw};
    pub use crate::scene_history::WorldHistory;
    pub use gource_scene::world::*;
}

pub use scrubber::{SeekOutcome, SimScrubber, TickRecord};
pub use spline::SplineEdge;
pub use user::{User, UserId, UserTexture};
pub use world::{DeletedFileInfo, DeletedUserInfo, Tuning, World};
