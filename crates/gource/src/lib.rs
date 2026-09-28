//! Bevy frontend for Gource.
//!
//! A thin shell around the engine-agnostic simulation in `gource-sim`:
//!
//! * [`cli`] — command line handling before the window opens (`main.cpp`),
//! * [`window`] — window configuration and window-level requests,
//! * [`input`] — Bevy/winit input to `gource_sim::InputEvent`,
//! * [`render`] — draws `gource_draw::DrawList`s with two `Material2d`s,
//! * [`capture`] — `--output-ppm-stream` frames and F12 screenshots,
//! * [`warmup`] — waits for the renderer before the first recorded frame,
//! * [`app`] — the Bevy app and frame loop around a [`sim::Simulation`].

pub mod app;
pub mod capture;
pub mod cli;
pub mod input;
pub mod render;
pub mod sim;
pub mod warmup;
pub mod window;
