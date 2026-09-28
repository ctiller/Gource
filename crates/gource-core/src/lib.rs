//! Engine-agnostic utilities shared by all Gource crates.
//!
//! Nothing in this crate knows about windows, GPUs or the simulation; it only
//! provides small, well-tested building blocks (geometry, hashing, time, text).

pub mod bounds;
pub mod datetime;
pub mod math;
pub mod quadtree;
pub mod resources;
pub mod stringhash;
pub mod utf8;

pub use glam;
pub use glam::{IVec2, UVec2, Vec2, Vec3, Vec4};

pub use bounds::Bounds2D;
pub use quadtree::QuadTree;
pub use stringhash::StringHasher;
