//! WebGL2 renderer backend for Gource [`gource_draw::DrawList`].
//!
//! This crate provides a lightweight WebGL2 renderer over `web-sys`/`wasm-bindgen`
//! targeting `wasm32-unknown-unknown`.
//!
//! # Architecture
//!
//! - [`plan`]: Pure rendering planning and GL mapping logic (diffing texture stores,
//!   mapping material blend modes, packing vertices, pixel to clip coordinate conversion).
//!   This module has zero DOM or GL dependencies and is 100% unit-tested on native targets.
//! - [`buffer`]: Per-frame vertex and index batch planning. Interleaves all vertices
//!   across batches into a single contiguous buffer with painter's order batch offsets.
//! - [`shader`]: GLSL ES 3.00 shader sources for Alpha and Bloom materials.
//! - [`gl`]: Wasm32-only GL dispatching layer implementing [`WebGlRenderer`].
//!
//! # Premultiplied Alpha Note
//!
//! Gource colours and textures are straight (non-premultiplied) RGBA and blended in
//! gamma space with `SRC_ALPHA, ONE_MINUS_SRC_ALPHA` (Alpha) and `ONE, ONE` (Bloom).
//! Canvases in HTML5 default to `premultipliedAlpha: true`, which would cause the browser
//! compositor to treat straight alpha framebuffer output incorrectly. Therefore,
//! the WebGL2 context should be created with `{ "premultipliedAlpha": false }`:
//!
//! ```javascript
//! const gl = canvas.getContext('webgl2', { premultipliedAlpha: false });
//! ```

pub mod buffer;
pub mod plan;
pub mod shader;

#[cfg(target_arch = "wasm32")]
pub mod gl;

#[cfg(target_arch = "wasm32")]
pub use gl::WebGlRenderer;

#[cfg(all(target_arch = "wasm32", feature = "demo"))]
pub mod demo;
