//! Integer-only, deterministic scene simulation for Gource.
//!
//! Everything here is fixed point: positions are [`IVec2`] in Q24.8 world
//! units, the step is a fixed [`TICK_HZ`] tick, randomness comes from a
//! counter-based hash keyed by `(seed, tick, entity, salt)`, and the only
//! "transcendentals" are [`isqrt`](fixed::isqrt) and a const-generated sine
//! table. Integer addition is associative, so every force sum is independent
//! of evaluation order and thread count, and the same inputs give the same
//! state bit for bit on every platform (native and wasm).
//!
//! The float view (interpolation between ticks, rotation, camera, splines)
//! lives in `gource-sim`; nothing in this crate touches floating point.

#![deny(clippy::float_arithmetic)]

pub mod dirs;
pub mod files;
pub mod fixed;
pub mod grid;
pub mod hash;
pub mod rng;
pub mod trig;
pub mod users;

pub use fixed::{Fx, IVec2, ONE, TICK_HZ, UNIT};
