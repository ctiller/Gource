//! Backend-neutral 2D drawing for Gource.
//!
//! The simulation tessellates every frame into a [`DrawList`]: an ordered list
//! of textured, vertex-coloured triangle batches in **screen pixel
//! coordinates** (origin top-left, +y down). A frontend (the Bevy app, or a
//! headless renderer, or tests) only has to:
//!
//! 1. mirror the [`TextureStore`] to GPU textures (re-uploading an entry when
//!    its `version` changes),
//! 2. draw the batches in order with the two [`Material`]s,
//!
//! This is the same split egui uses (tessellate on the CPU, tiny backends),
//! and it keeps all of Gource's behaviour testable without a GPU.
//!
//! Colour convention: all colours and texel values are used *as-is* (gamma
//! encoded, like the original OpenGL renderer). Frontends must blend in sRGB
//! (gamma) space and must not apply sRGB decoding to textures.

pub mod font;
pub mod list;
pub mod ppm;
pub mod projection;
pub mod resources;
pub mod texture;
pub mod video;

pub use font::{FaceId, FontError, FontId, FontStore, TextStyle};
pub use list::{Batch, DrawList, Material, TextureId, Vertex};
pub use ppm::PpmExporter;
pub use projection::Projection;
pub use texture::{Filter, Texture, TextureError, TextureOptions, TextureStore, Wrap};
pub use video::{VideoCodec, VideoConfig, VideoExporter, VideoSink};

use glam::Vec2;

/// All persistent drawing resources: textures and fonts.
///
/// Owned by the simulation (which decides what to load), read by frontends
/// (which upload textures).
pub struct Gfx {
    pub textures: TextureStore,
    pub fonts: FontStore,
}

impl Default for Gfx {
    fn default() -> Self {
        Self::new()
    }
}

impl Gfx {
    pub fn new() -> Self {
        Self {
            textures: TextureStore::new(),
            fonts: FontStore::new(),
        }
    }

    /// Draw text (see [`FontStore::draw`]).
    pub fn draw_text(
        &mut self,
        list: &mut DrawList,
        font: FontId,
        pos: Vec2,
        text: &str,
        style: &TextStyle,
    ) {
        self.fonts
            .draw(&mut self.textures, list, font, pos, text, style);
    }

    /// Width of `text` in pixels (see [`FontStore::width`]).
    pub fn text_width(&mut self, font: FontId, text: &str) -> f32 {
        self.fonts.width(font, text)
    }
}
