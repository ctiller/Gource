//! Integer-only layout kernels (Q24.8 fixed point).
//!
//! Deterministic across threads and targets; nothing in this module touches floating point.

#![deny(clippy::float_arithmetic)]

pub mod dirs;
pub mod files;
pub mod fixed;
pub mod grid;
pub mod hash;
pub mod rng;
pub mod trig;
pub mod users;
