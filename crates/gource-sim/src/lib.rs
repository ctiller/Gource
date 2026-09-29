//! Engine-agnostic Gource simulation.
//!
//! Owns all state (no globals): the directory tree, files, users, actions and
//! camera, advanced with `dt` and tessellated into a
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
//! * [`dirnode`] — `RDirNode` layout, physics and drawing (dirnode.cpp)
//! * [`world`] — arenas tying dirs/files/users/actions together
//! * [`app`] — [`GourceApp`], the frontend-facing API over the `Gource`
//!   state machine and `GourceShell` (gource.cpp, gource_shell.cpp).

pub mod action;
pub mod app;
pub mod camera;
pub mod checkpoint;
pub mod dirnode;
pub mod file;
pub mod gource;
pub mod input;
pub mod pawn;
pub mod platform;
pub mod scrubber;
pub mod shell;
pub mod spline;
pub mod user;
pub mod world;

pub use action::{Action, ActionKind};
pub use app::{AppError, AppOptions, GourceApp, vcs_options};
pub use checkpoint::CheckpointStore;
pub use dirnode::DirNode;
pub use file::{DirId, File, FileId};
pub use gource::SimSnapshot;
pub use input::{InputEvent, Key, Modifiers, MouseButton};
pub use pawn::Pawn;
pub use platform::{PlatformRequest, Viewport};
pub use scrubber::{SeekOutcome, SimScrubber};
pub use spline::SplineEdge;
pub use user::{User, UserId};
pub use world::{DeletedFileInfo, DeletedUserInfo, SceneFonts, SceneTextures, Tuning, World};
