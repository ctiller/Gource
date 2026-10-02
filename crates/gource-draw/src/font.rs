//! Fonts and text rendering (replaces `core/fxfont.cpp`, which used FreeType).
//!
//! Glyphs are rasterised with pluggable [`GlyphRasterizer`] backends (e.g. `ab_glyph` or
//! browser Canvas2D) into shared atlas pages registered in the [`TextureStore`]
//! (RGBA8 with white RGB and coverage in alpha), and text is emitted as textured quads
//! into a [`DrawList`] with [`Material::Alpha`].
//!
//! Layout must follow FXFont so HUD elements line up like the original:
//! * a font instance is a face at an integer pixel size, meaning the em size in
//!   pixels (FreeType `FT_Set_Char_Size(64*size, 64*size, 72, 72)`), i.e. the
//!   ab_glyph scale is `size * height_unscaled / units_per_em`;
//! * advances are rounded down to whole pixels (`advance.x >> 6`);
//! * `width(text)` is the sum of advances (no kerning, no tab expansion);
//! * `draw(pos)`: `pos.x` is the pen start; if `align_right`, `x -= width`;
//!   if `align_top`, `y += height()` where `height() = ceil(ascender +
//!   descender)` (descender is negative); if `round`, x and y are rounded;
//!   then `y` is the baseline;
//! * each glyph quad is placed at `pen + (bitmap_left + 0.5, -bitmap_top - 0.5)`
//!   with size `(bitmap_w + 2, bitmap_h + 2)` and texture coordinates padded by
//!   half a texel outside/1.5 texels (see `FXGlyphPage::addGlyph`);
//! * `'\t'` advances by 4 × advance('M') (only when drawing);
//! * drop shadow: when `shadow` is set, first draw the whole string at
//!   `pos + shadow_offset` with colour `(0, 0, 0, colour.a * shadow_strength)`,
//!   then the string itself.
//!
//! [`Material::Alpha`]: crate::Material::Alpha

use crate::list::{DrawList, TextureId};
use crate::texture::TextureStore;
use gource_core::{Vec2, Vec4};
use std::collections::HashMap;
use std::path::Path;

/// A loaded font face (outline data).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FaceId(pub u32);

/// A face at a specific pixel size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(pub u32);

#[derive(Debug, thiserror::Error)]
pub enum FontError {
    #[error("failed to read font '{path}': {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid font data in '{0}'")]
    Invalid(String),
}

/// Metrics for a font face at a specific pixel size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontMetrics {
    pub ascender: f32,
    pub descender: f32,
    pub max_advance: f32,
}

/// Rasterized glyph coverage bitmap and bounding box relative to pen baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct RasterizedGlyph {
    pub width: u32,
    pub height: u32,
    /// Horizontal offset from pen position (`bounds.min.x`).
    pub min_x: f32,
    /// Vertical offset from baseline (`bounds.min.y`, negative above baseline).
    pub min_y: f32,
    /// Alpha coverage bytes (`width * height`, each `0..=255`).
    pub coverage: Vec<u8>,
}

/// Pluggable backend that measures and rasterises glyphs for [`FontStore`].
pub trait GlyphRasterizer: Send + Sync {
    /// Load and validate a font face from `data` (may be empty for system/Canvas2D fonts).
    fn load_face(&mut self, name: &str, data: &[u8]) -> Result<FaceId, FontError>;
    /// Return vertical and horizontal metrics for `face` at pixel `size`.
    fn metrics(&self, face: FaceId, size: u32) -> FontMetrics;
    /// Return rounded horizontal advance in pixels for `ch` at `(face, size)`.
    fn advance(&self, face: FaceId, size: u32, ch: char) -> f32;
    /// Return the tight pixel height of `ch` at `(face, size)` for `max_height` precaching.
    fn glyph_height(&self, face: FaceId, size: u32, ch: char) -> f32;
    /// Rasterise `ch` at `(face, size)` into a tight coverage buffer, or `None` if invisible.
    fn rasterize(&self, face: FaceId, size: u32, ch: char) -> Option<RasterizedGlyph>;
}

/// Glyph rasterizer backed by `ab_glyph`.
#[cfg(feature = "ab-glyph")]
#[derive(Default)]
pub struct AbGlyphRasterizer {
    faces: Vec<ab_glyph::FontArc>,
}

#[cfg(feature = "ab-glyph")]
impl GlyphRasterizer for AbGlyphRasterizer {
    fn load_face(&mut self, name: &str, data: &[u8]) -> Result<FaceId, FontError> {
        let font = ab_glyph::FontArc::try_from_vec(data.to_vec())
            .map_err(|_| FontError::Invalid(name.to_string()))?;
        let id = FaceId(self.faces.len() as u32);
        self.faces.push(font);
        Ok(id)
    }

    fn metrics(&self, face: FaceId, size: u32) -> FontMetrics {
        use ab_glyph::{Font, ScaleFont};
        let arc = &self.faces[face.0 as usize];
        let em_size = arc.units_per_em().unwrap_or(1000.0);
        let unit_scale_y = size as f32 / em_size;
        let ascender = arc.ascent_unscaled() * unit_scale_y;
        let descender = arc.descent_unscaled() * unit_scale_y;
        let scale = ab_glyph::PxScale::from(size as f32 * arc.height_unscaled() / em_size);
        let scaled_font = arc.as_scaled(scale);
        let max_advance = scaled_font.h_advance(scaled_font.glyph_id('M')).ceil();
        FontMetrics {
            ascender,
            descender,
            max_advance,
        }
    }

    fn advance(&self, face: FaceId, size: u32, ch: char) -> f32 {
        use ab_glyph::{Font, ScaleFont};
        let arc = &self.faces[face.0 as usize];
        let em_size = arc.units_per_em().unwrap_or(1000.0);
        let scale = ab_glyph::PxScale::from(size as f32 * arc.height_unscaled() / em_size);
        let scaled_font = arc.as_scaled(scale);
        let glyph_id = scaled_font.glyph_id(ch);
        let unhinted_adv = scaled_font.h_advance(glyph_id);
        unhinted_adv.round()
    }

    fn glyph_height(&self, face: FaceId, size: u32, ch: char) -> f32 {
        use ab_glyph::{Font, ScaleFont};
        let arc = &self.faces[face.0 as usize];
        let em_size = arc.units_per_em().unwrap_or(1000.0);
        let scale = ab_glyph::PxScale::from(size as f32 * arc.height_unscaled() / em_size);
        let scaled_font = arc.as_scaled(scale);
        if let Some(outlined) = scaled_font.outline_glyph(scaled_font.scaled_glyph(ch)) {
            let bounds = outlined.px_bounds();
            bounds.height().ceil()
        } else {
            0.0
        }
    }

    fn rasterize(&self, face: FaceId, size: u32, ch: char) -> Option<RasterizedGlyph> {
        use ab_glyph::{Font, ScaleFont};
        let arc = &self.faces[face.0 as usize];
        let em_size = arc.units_per_em().unwrap_or(1000.0);
        let scale = ab_glyph::PxScale::from(size as f32 * arc.height_unscaled() / em_size);
        let scaled_font = arc.as_scaled(scale);
        let scaled_glyph = scaled_font.scaled_glyph(ch);
        let outlined = scaled_font.outline_glyph(scaled_glyph)?;
        let bounds = outlined.px_bounds();
        let b_w = bounds.width().round() as u32;
        let b_h = bounds.height().round() as u32;

        if b_w == 0 || b_h == 0 {
            return None;
        }

        let mut coverage = vec![0u8; (b_w * b_h) as usize];
        outlined.draw(|x, y, c| {
            if x < b_w && y < b_h {
                coverage[(y * b_w + x) as usize] = (c * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        });

        Some(RasterizedGlyph {
            width: b_w,
            height: b_h,
            min_x: bounds.min.x,
            min_y: bounds.min.y,
            coverage,
        })
    }
}

/// A fallback glyph rasterizer that accepts valid TTF/OTF or empty font data,
/// computes proportional metrics, and rasterises non-whitespace characters as simple filled boxes.
#[derive(Default)]
pub struct FallbackGlyphRasterizer {
    face_count: u32,
}

impl GlyphRasterizer for FallbackGlyphRasterizer {
    fn load_face(&mut self, name: &str, data: &[u8]) -> Result<FaceId, FontError> {
        if !data.is_empty() {
            let valid = data.len() >= 4
                && (data[0..4] == [0x00, 0x01, 0x00, 0x00]
                    || &data[0..4] == b"OTTO"
                    || &data[0..4] == b"true"
                    || &data[0..4] == b"ttcf");
            if !valid {
                return Err(FontError::Invalid(name.to_string()));
            }
        }
        let id = FaceId(self.face_count);
        self.face_count += 1;
        Ok(id)
    }

    fn metrics(&self, _face: FaceId, size: u32) -> FontMetrics {
        FontMetrics {
            ascender: size as f32 * 0.8,
            descender: -(size as f32 * 0.2),
            max_advance: (size as f32 * 0.6).ceil(),
        }
    }

    fn advance(&self, _face: FaceId, size: u32, _ch: char) -> f32 {
        (size as f32 * 0.5).round()
    }

    fn glyph_height(&self, _face: FaceId, size: u32, ch: char) -> f32 {
        if ch.is_whitespace() {
            0.0
        } else {
            (size as f32 * 0.7).round().max(1.0)
        }
    }

    fn rasterize(&self, _face: FaceId, size: u32, ch: char) -> Option<RasterizedGlyph> {
        if ch.is_whitespace() {
            return None;
        }
        let w = ((size as f32 * 0.4).round() as u32).max(1);
        let h = ((size as f32 * 0.7).round() as u32).max(1);
        Some(RasterizedGlyph {
            width: w,
            height: h,
            min_x: 0.0,
            min_y: -(h as f32),
            coverage: vec![255u8; (w * h) as usize],
        })
    }
}

/// Per-draw text settings (the mutable state of a C++ `FXFont`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    /// Text colour (`FXFont::setColour` / `setAlpha`).
    pub colour: Vec4,
    /// Draw a drop shadow (`FXFont::dropShadow`).
    pub shadow: bool,
    /// Shadow alpha relative to the text alpha (default 0.7).
    pub shadow_strength: f32,
    /// Shadow offset in pixels (default (1, 1)).
    pub shadow_offset: Vec2,
    /// Treat `pos.y` as the top of the text rather than the baseline (default true).
    pub align_top: bool,
    /// Treat `pos.x` as the right edge of the text (default false).
    pub align_right: bool,
    /// Round the final position to whole pixels (default false).
    pub round: bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            colour: Vec4::ONE,
            shadow: false,
            shadow_strength: 0.7,
            shadow_offset: Vec2::ONE,
            align_top: true,
            align_right: false,
            round: false,
        }
    }
}

impl TextStyle {
    pub fn new(colour: Vec4) -> Self {
        Self {
            colour,
            ..Self::default()
        }
    }

    pub fn with_colour(mut self, colour: Vec4) -> Self {
        self.colour = colour;
        self
    }

    pub fn with_alpha(mut self, alpha: f32) -> Self {
        self.colour.w = alpha;
        self
    }

    pub fn with_shadow(mut self, shadow: bool) -> Self {
        self.shadow = shadow;
        self
    }

    pub fn with_align_top(mut self, align_top: bool) -> Self {
        self.align_top = align_top;
        self
    }

    pub fn with_align_right(mut self, align_right: bool) -> Self {
        self.align_right = align_right;
        self
    }

    pub fn with_round(mut self, round: bool) -> Self {
        self.round = round;
        self
    }
}

/// Cached data for a single glyph in an instance.
#[derive(Debug, Clone)]
struct CachedGlyph {
    /// Advance in pixels (`advance.x >> 6`), matched to FreeType.
    advance: f32,
    /// Bounding box and atlas placement if the glyph has a visual representation.
    quad: Option<GlyphQuad>,
}

#[derive(Debug, Clone)]
struct GlyphQuad {
    /// Texture page in TextureStore.
    page_texture: TextureId,
    /// Offset from pen position to top-left of quad.
    /// `(bitmap_left + 0.5, -bitmap_top - 0.5)`
    corner: Vec2,
    /// Size of quad in screen pixels: `(bitmap_w + 2, bitmap_h + 2)`.
    dims: Vec2,
    /// Texture coordinates `[u_min, v_min, u_max, v_max]`.
    texcoords: Vec4,
}

/// An atlas page (512x512 RGBA8 texture in TextureStore).
struct AtlasPage {
    texture_id: TextureId,
    cursor_x: u32,
    cursor_y: u32,
    max_glyph_height: u32,
}

impl AtlasPage {
    const SIZE: u32 = 512;
    const PADDING: u32 = 3;

    fn new(textures: &mut TextureStore) -> Self {
        // Plain texture options: no mipmaps, clamp to edge, linear filtering
        let texture_id = textures.create_rgba(
            "font_atlas_page",
            Self::SIZE,
            Self::SIZE,
            vec![0u8; (Self::SIZE * Self::SIZE * 4) as usize],
            crate::texture::TextureOptions::plain(),
        );
        Self {
            texture_id,
            cursor_x: 1,
            cursor_y: 1,
            max_glyph_height: 1,
        }
    }

    /// Try to allocate space for a bitmap of size `(w, h)` and return `(corner_x, corner_y)`.
    fn try_allocate(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        let padding = Self::PADDING;
        if h > self.max_glyph_height {
            self.max_glyph_height = h;
        }

        let mut corner_x = self.cursor_x;
        let mut corner_y = self.cursor_y;

        if corner_x + w + padding > Self::SIZE {
            corner_x = 1;
            corner_y += self.max_glyph_height + padding;

            if corner_x + w + padding > Self::SIZE {
                return None;
            }
        }

        if corner_y + h + padding > Self::SIZE {
            return None;
        }

        self.cursor_x = corner_x + w + padding;
        self.cursor_y = corner_y;

        Some((corner_x, corner_y))
    }
}

struct FontInstance {
    face_id: FaceId,
    size: u32,
    ascender: f32,
    descender: f32,
    max_advance: f32,
    max_height: f32,
    glyphs: HashMap<char, CachedGlyph>,
}

/// Owns font faces, sized instances, glyph caches and atlas pages.
pub struct FontStore {
    rasterizer: Box<dyn GlyphRasterizer>,
    loaded_faces: Vec<(String, Vec<u8>)>,
    face_names: HashMap<String, FaceId>,
    default_face_id: Option<FaceId>,
    instances: Vec<FontInstance>,
    instance_map: HashMap<(FaceId, u32), FontId>,
    pages: Vec<AtlasPage>,
}

impl Default for FontStore {
    fn default() -> Self {
        #[cfg(feature = "ab-glyph")]
        let rasterizer: Box<dyn GlyphRasterizer> = Box::new(AbGlyphRasterizer::default());
        #[cfg(not(feature = "ab-glyph"))]
        let rasterizer: Box<dyn GlyphRasterizer> = Box::new(FallbackGlyphRasterizer::default());

        Self {
            rasterizer,
            loaded_faces: Vec::new(),
            face_names: HashMap::new(),
            default_face_id: None,
            instances: Vec::new(),
            instance_map: HashMap::new(),
            pages: Vec::new(),
        }
    }
}

impl FontStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_rasterizer(rasterizer: Box<dyn GlyphRasterizer>) -> Self {
        Self {
            rasterizer,
            ..Self::default()
        }
    }

    pub fn set_rasterizer(&mut self, mut rasterizer: Box<dyn GlyphRasterizer>) {
        for (name, data) in &self.loaded_faces {
            let _ = rasterizer.load_face(name, data);
        }
        self.rasterizer = rasterizer;
        for inst in &mut self.instances {
            let metrics = self.rasterizer.metrics(inst.face_id, inst.size);
            inst.ascender = metrics.ascender;
            inst.descender = metrics.descender;
            inst.max_advance = metrics.max_advance;
            inst.max_height = 0.0;
            inst.glyphs.clear();

            for ch in 32u8..=126u8 {
                let chr = ch as char;
                let adv = self.rasterizer.advance(inst.face_id, inst.size, chr);
                let glyph_h = self.rasterizer.glyph_height(inst.face_id, inst.size, chr);
                if glyph_h > inst.max_height {
                    inst.max_height = glyph_h;
                }
                inst.glyphs.insert(
                    chr,
                    CachedGlyph {
                        advance: adv,
                        quad: None,
                    },
                );
            }
        }
        self.pages.clear();
    }

    /// Load a face from memory. `name` is used in error messages and for
    /// caching (loading the same name twice returns the same face).
    pub fn load_face(&mut self, name: &str, data: Vec<u8>) -> Result<FaceId, FontError> {
        if let Some(&id) = self.face_names.get(name) {
            return Ok(id);
        }

        let id = self.rasterizer.load_face(name, &data)?;
        self.loaded_faces.push((name.to_string(), data));
        self.face_names.insert(name.to_string(), id);
        Ok(id)
    }

    /// Load a face from a font file (ttf/otf). Cached by path.
    pub fn load_face_file(&mut self, path: &Path) -> Result<FaceId, FontError> {
        let name = path.to_string_lossy().into_owned();
        if let Some(&id) = self.face_names.get(&name) {
            return Ok(id);
        }

        let data = std::fs::read(path).map_err(|e| FontError::Io {
            path: name.clone(),
            source: e,
        })?;

        self.load_face(&name, data)
    }

    /// The embedded default face (FreeSans, [`crate::resources::FREESANS_TTF`]).
    pub fn default_face(&mut self) -> FaceId {
        if let Some(id) = self.default_face_id {
            return id;
        }

        let id = self
            .load_face(
                crate::resources::DEFAULT_FONT_NAME,
                crate::resources::FREESANS_TTF.to_vec(),
            )
            .expect("embedded FreeSans.ttf is valid");
        self.default_face_id = Some(id);
        id
    }

    /// A face at a pixel size (clamped to >= 1). Cached: the same (face, size)
    /// always returns the same id.
    pub fn font(&mut self, face: FaceId, size: u32) -> FontId {
        let size = size.max(1);
        let key = (face, size);
        if let Some(&id) = self.instance_map.get(&key) {
            return id;
        }

        let metrics = self.rasterizer.metrics(face, size);
        let id = FontId(self.instances.len() as u32);
        let mut inst = FontInstance {
            face_id: face,
            size,
            ascender: metrics.ascender,
            descender: metrics.descender,
            max_advance: metrics.max_advance,
            max_height: 0.0,
            glyphs: HashMap::new(),
        };

        // Precache printable ASCII like FXGlyphSet does:
        // "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ;:'\",<.>/?-_=+!@#$%^&*()\\ "
        // Note: precache populates metrics (max_height) even without textures
        for ch in 32u8..=126u8 {
            let chr = ch as char;
            let adv = self.rasterizer.advance(face, size, chr);
            let glyph_h = self.rasterizer.glyph_height(face, size, chr);
            if glyph_h > inst.max_height {
                inst.max_height = glyph_h;
            }
            inst.glyphs.insert(
                chr,
                CachedGlyph {
                    advance: adv,
                    quad: None,
                },
            );
        }

        self.instances.push(inst);
        self.instance_map.insert(key, id);
        id
    }

    /// The pixel size of a font instance.
    pub fn size(&self, font: FontId) -> u32 {
        self.instances
            .get(font.0 as usize)
            .map(|i| i.size)
            .unwrap_or(0)
    }

    /// `FXFont::getWidth`: sum of rounded-down advances.
    pub fn width(&mut self, font: FontId, text: &str) -> f32 {
        let mut width = 0.0;
        for ch in text.chars() {
            width += self.glyph_advance(font, ch);
        }
        width
    }

    /// Helper to get or calculate advance for a character.
    fn glyph_advance(&mut self, font: FontId, ch: char) -> f32 {
        let inst = match self.instances.get_mut(font.0 as usize) {
            Some(i) => i,
            None => return 0.0,
        };

        if let Some(cg) = inst.glyphs.get(&ch) {
            return cg.advance;
        }

        let face_id = inst.face_id;
        let size = inst.size;
        let adv = self.rasterizer.advance(face_id, size, ch);

        let inst = self.instances.get_mut(font.0 as usize).unwrap();
        inst.glyphs.insert(
            ch,
            CachedGlyph {
                advance: adv,
                quad: None,
            },
        );
        adv
    }

    /// `FXFont::getHeight`: `ceil(ascender + descender)`.
    pub fn height(&self, font: FontId) -> f32 {
        self.instances
            .get(font.0 as usize)
            .map(|i| (i.ascender + i.descender).ceil())
            .unwrap_or(0.0)
    }

    /// Scaled ascender in pixels (positive).
    pub fn ascender(&self, font: FontId) -> f32 {
        self.instances
            .get(font.0 as usize)
            .map(|i| i.ascender)
            .unwrap_or(0.0)
    }

    /// Scaled descender in pixels (negative).
    pub fn descender(&self, font: FontId) -> f32 {
        self.instances
            .get(font.0 as usize)
            .map(|i| i.descender)
            .unwrap_or(0.0)
    }

    /// `FXFont::getMaxWidth`: the largest advance of any glyph in the face.
    pub fn max_width(&self, font: FontId) -> f32 {
        self.instances
            .get(font.0 as usize)
            .map(|i| i.max_advance)
            .unwrap_or(0.0)
    }

    /// `FXFont::getMaxHeight`: the tallest glyph (ceil of glyph height in
    /// pixels) rasterised so far for this instance (the C++ precaches ASCII).
    pub fn max_height(&self, font: FontId) -> f32 {
        self.instances
            .get(font.0 as usize)
            .map(|i| i.max_height)
            .unwrap_or(0.0)
    }

    /// Ensure a glyph is rasterised into an atlas page and return its quad info.
    fn ensure_glyph_quad(
        &mut self,
        textures: &mut TextureStore,
        font: FontId,
        ch: char,
    ) -> Option<GlyphQuad> {
        let inst = self.instances.get(font.0 as usize)?;
        if let Some(quad) = inst.glyphs.get(&ch).and_then(|cg| cg.quad.as_ref()) {
            return Some(quad.clone());
        }

        let face_id = inst.face_id;
        let size = inst.size;
        let advance = self.rasterizer.advance(face_id, size, ch);
        let rasterized = self.rasterizer.rasterize(face_id, size, ch);

        let quad = if let Some(glyph) = rasterized {
            let b_w = glyph.width;
            let b_h = glyph.height;

            if b_w == 0
                || b_h == 0
                || b_w + AtlasPage::PADDING + 1 > AtlasPage::SIZE
                || b_h + AtlasPage::PADDING + 1 > AtlasPage::SIZE
            {
                None
            } else {
                let glyph_h =
                    (glyph.height as f32).max(self.rasterizer.glyph_height(face_id, size, ch));
                let inst = self.instances.get_mut(font.0 as usize).unwrap();
                if glyph_h > inst.max_height {
                    inst.max_height = glyph_h;
                }

                // Convert coverage to RGBA8 (RGB = 255, A = coverage)
                let mut rgba = vec![0u8; (b_w * b_h * 4) as usize];
                for i in 0..(b_w * b_h) as usize {
                    rgba[i * 4] = 255;
                    rgba[i * 4 + 1] = 255;
                    rgba[i * 4 + 2] = 255;
                    rgba[i * 4 + 3] = glyph.coverage[i];
                }

                // Allocate in atlas page
                let (page_idx, corner_x, corner_y) = loop {
                    let last_alloc = self.pages.len().checked_sub(1).and_then(|idx| {
                        self.pages[idx]
                            .try_allocate(b_w, b_h)
                            .map(|c| (idx, c.0, c.1))
                    });
                    if let Some((idx, cx, cy)) = last_alloc {
                        break (idx, cx, cy);
                    }
                    // Need a new page
                    let new_page = AtlasPage::new(textures);
                    self.pages.push(new_page);
                };

                let page_tex_id = self.pages[page_idx].texture_id;
                textures.update_rgba(page_tex_id, corner_x, corner_y, b_w, b_h, &rgba);

                let page_size = AtlasPage::SIZE as f32;
                let texcoords = Vec4::new(
                    (corner_x as f32 - 0.5) / page_size,
                    (corner_y as f32 - 0.5) / page_size,
                    (corner_x as f32 + b_w as f32 + 1.5) / page_size,
                    (corner_y as f32 + b_h as f32 + 1.5) / page_size,
                );

                let dims = Vec2::new(b_w as f32 + 2.0, b_h as f32 + 2.0);
                // bounds.min.x corresponds to bitmap_left, bounds.min.y corresponds to -bitmap_top
                let corner = Vec2::new(glyph.min_x + 0.5, glyph.min_y - 0.5);

                Some(GlyphQuad {
                    page_texture: page_tex_id,
                    corner,
                    dims,
                    texcoords,
                })
            }
        } else {
            None
        };

        let inst = self.instances.get_mut(font.0 as usize).unwrap();
        inst.glyphs.insert(
            ch,
            CachedGlyph {
                advance,
                quad: quad.clone(),
            },
        );

        quad
    }

    /// `FXFont::draw`: emit glyph quads (and the optional drop shadow) into
    /// `list`, rasterising new glyphs into atlas pages as needed.
    pub fn draw(
        &mut self,
        textures: &mut TextureStore,
        list: &mut DrawList,
        font: FontId,
        pos: Vec2,
        text: &str,
        style: &TextStyle,
    ) {
        if text.is_empty() {
            return;
        }

        let mut origin = pos;
        if style.align_right {
            origin.x -= self.width(font, text);
        }
        if style.align_top {
            origin.y += self.height(font);
        }
        if style.round {
            origin.x = origin.x.round();
            origin.y = origin.y.round();
        }

        let tab_adv = self.glyph_advance(font, 'M') * 4.0;

        // First pass: collect glyph placements
        struct DrawGlyph {
            quad: GlyphQuad,
            pos: Vec2,
        }
        let mut draw_glyphs = Vec::new();
        let mut pen = origin;

        for ch in text.chars() {
            if ch == '\t' {
                pen.x += tab_adv;
                continue;
            }

            let adv = self.glyph_advance(font, ch);
            if let Some(quad) = self.ensure_glyph_quad(textures, font, ch) {
                draw_glyphs.push(DrawGlyph { quad, pos: pen });
            }
            pen.x += adv;
        }

        // Helper to push quads to list
        let emit_string = |list: &mut DrawList, offset: Vec2, colour: Vec4| {
            for dg in &draw_glyphs {
                let p = dg.pos + offset + dg.quad.corner;
                let dims = dg.quad.dims;
                let tc = dg.quad.texcoords;
                list.rect_uv(
                    dg.quad.page_texture,
                    p,
                    dims,
                    Vec2::new(tc.x, tc.y),
                    Vec2::new(tc.z, tc.w),
                    colour,
                );
            }
        };

        if style.shadow {
            let shadow_colour = Vec4::new(0.0, 0.0, 0.0, style.colour.w * style.shadow_strength);
            emit_string(list, style.shadow_offset, shadow_colour);
        }

        emit_string(list, Vec2::ZERO, style.colour);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_rasterizer_magic_bytes() {
        let mut r = FallbackGlyphRasterizer::default();
        // Empty data succeeds
        assert!(r.load_face("empty", &[]).is_ok());
        // Valid magic bytes succeed
        assert!(r.load_face("ttf", &[0x00, 0x01, 0x00, 0x00, 0, 0]).is_ok());
        assert!(r.load_face("otto", b"OTTO extra data").is_ok());
        assert!(r.load_face("true", b"true extra").is_ok());
        assert!(r.load_face("ttcf", b"ttcf extra").is_ok());
        // Invalid data fails
        assert!(r.load_face("bad1", &[1, 2, 3]).is_err());
        assert!(r.load_face("bad2", b"NOPE").is_err());
    }

    #[test]
    fn test_fallback_rasterizer_metrics_and_rasterize() {
        let mut r = FallbackGlyphRasterizer::default();
        let face = r.load_face("test", &[]).unwrap();
        let m = r.metrics(face, 20);
        assert_eq!(m.ascender, 16.0);
        assert_eq!(m.descender, -4.0);
        assert_eq!(m.max_advance, 12.0);

        assert_eq!(r.advance(face, 20, 'A'), 10.0);
        assert_eq!(r.glyph_height(face, 20, ' '), 0.0);
        assert!(r.glyph_height(face, 20, 'A') > 0.0);

        assert!(r.rasterize(face, 20, ' ').is_none());
        let glyph = r.rasterize(face, 20, 'A').unwrap();
        assert!(glyph.width > 0);
        assert!(glyph.height > 0);
        assert_eq!(glyph.coverage.len(), (glyph.width * glyph.height) as usize);
        assert!(glyph.coverage.iter().all(|&c| c == 255));
    }

    #[test]
    fn test_set_rasterizer() {
        let mut store = FontStore::with_rasterizer(Box::new(FallbackGlyphRasterizer::default()));
        let face = store.load_face("f1", vec![]).unwrap();
        let font = store.font(face, 20);
        assert_eq!(store.ascender(font), 16.0);
        assert_eq!(store.descender(font), -4.0);
        assert_eq!(store.width(font, "A"), 10.0);

        // Define a custom test rasterizer with different metrics
        struct CustomRasterizer {
            faces: u32,
        }
        impl GlyphRasterizer for CustomRasterizer {
            fn load_face(&mut self, _name: &str, _data: &[u8]) -> Result<FaceId, FontError> {
                let id = FaceId(self.faces);
                self.faces += 1;
                Ok(id)
            }
            fn metrics(&self, _face: FaceId, size: u32) -> FontMetrics {
                FontMetrics {
                    ascender: size as f32,
                    descender: 0.0,
                    max_advance: size as f32,
                }
            }
            fn advance(&self, _face: FaceId, size: u32, _ch: char) -> f32 {
                size as f32 * 2.0
            }
            fn glyph_height(&self, _face: FaceId, size: u32, _ch: char) -> f32 {
                size as f32
            }
            fn rasterize(&self, _face: FaceId, _size: u32, _ch: char) -> Option<RasterizedGlyph> {
                None
            }
        }

        store.set_rasterizer(Box::new(CustomRasterizer { faces: 0 }));
        // Instances should be recomputed
        assert_eq!(store.ascender(font), 20.0);
        assert_eq!(store.descender(font), 0.0);
        assert_eq!(store.width(font, "A"), 40.0);
    }
}
