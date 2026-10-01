//! Engine-agnostic utilities shared by all Gource crates.
//!
//! Nothing in this crate knows about windows, GPUs or the simulation; it only
//! provides small, well-tested building blocks (geometry, hashing, time, text).

#[cfg(feature = "glam")]
pub mod bounds;
pub mod crand;
pub mod datetime;
#[cfg(feature = "glam")]
pub mod math;
#[cfg(feature = "glam")]
pub mod quadtree;
pub mod resources;
pub mod stringhash;
pub mod utf8;

#[cfg(feature = "glam")]
pub use glam;
#[cfg(feature = "glam")]
pub use glam::{IVec2, UVec2, Vec2, Vec3, Vec4};

#[cfg(feature = "glam")]
pub use bounds::Bounds2D;
#[cfg(feature = "glam")]
pub use quadtree::QuadTree;
pub use stringhash::StringHasher;
