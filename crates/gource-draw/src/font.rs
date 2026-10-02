//! Fonts and text rendering (replaces `core/fxfont.cpp`, which used FreeType).
//!
//! Glyphs are rasterised with `ab_glyph` into shared atlas pages registered in
//! the [`TextureStore`] (RGBA8 with white RGB and coverage in alpha), and text
//! is emitted as textured quads into a [`DrawList`] with [`Material::Alpha`].
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

use crate::list::DrawList;
use crate::texture::TextureStore;
use gource_core::{Vec2, Vec4};
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

use crate::list::TextureId;
use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use std::collections::HashMap;

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
    scale: PxScale,
    ascender: f32,
    descender: f32,
    max_advance: f32,
    max_height: f32,
    glyphs: HashMap<char, CachedGlyph>,
}

/// Owns font faces, sized instances, glyph caches and atlas pages.
#[derive(Default)]
pub struct FontStore {
    faces: Vec<FontArc>,
    face_names: HashMap<String, FaceId>,
    default_face_id: Option<FaceId>,
    instances: Vec<FontInstance>,
    instance_map: HashMap<(FaceId, u32), FontId>,
    pages: Vec<AtlasPage>,
}

impl FontStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load a face from memory. `name` is used in error messages and for
    /// caching (loading the same name twice returns the same face).
    pub fn load_face(&mut self, name: &str, data: Vec<u8>) -> Result<FaceId, FontError> {
        if let Some(&id) = self.face_names.get(name) {
            return Ok(id);
        }

        let font = FontArc::try_from_vec(data).map_err(|_| FontError::Invalid(name.to_string()))?;
        let id = FaceId(self.faces.len() as u32);
        self.faces.push(font);
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

        let arc = self.faces.get(face.0 as usize).expect("valid FaceId");
        let em_size = arc.units_per_em().unwrap_or(1000.0);
        let scale = PxScale::from(size as f32 * arc.height_unscaled() / em_size);
        let scaled_font = arc.as_scaled(scale);

        let unit_scale_y = size as f32 / em_size;
        let ascender = arc.ascent_unscaled() * unit_scale_y;
        let descender = arc.descent_unscaled() * unit_scale_y;

        // FreeType max_advance / 64.0
        let max_advance = (scaled_font.h_advance(scaled_font.glyph_id('M'))).ceil();

        let id = FontId(self.instances.len() as u32);
        let mut inst = FontInstance {
            face_id: face,
            size,
            scale,
            ascender,
            descender,
            max_advance,
            max_height: 0.0,
            glyphs: HashMap::new(),
        };

        // Precache printable ASCII like FXGlyphSet does:
        // "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ;:'\",<.>/?-_=+!@#$%^&*()\\ "
        // Note: precache populates metrics (max_height) even without textures
        for ch in 32u8..=126u8 {
            let chr = ch as char;
            let glyph_id = scaled_font.glyph_id(chr);
            let unhinted_adv = scaled_font.h_advance(glyph_id);
            let adv = unhinted_adv.round();
            if let Some(outlined) = scaled_font.outline_glyph(scaled_font.scaled_glyph(chr)) {
                let bounds = outlined.px_bounds();
                let glyph_h = (bounds.height()).ceil();
                if glyph_h > inst.max_height {
                    inst.max_height = glyph_h;
                }
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
        let scale = inst.scale;
        let arc = &self.faces[face_id.0 as usize];
        let scaled_font = arc.as_scaled(scale);

        let glyph_id = scaled_font.glyph_id(ch);
        let unhinted_adv = scaled_font.h_advance(glyph_id);
        let adv = unhinted_adv.round();

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
        let scale = inst.scale;
        let arc = self.faces.get(face_id.0 as usize)?.clone();
        let scaled_font = arc.as_scaled(scale);

        let glyph_id = scaled_font.glyph_id(ch);
        let unhinted_adv = scaled_font.h_advance(glyph_id);
        let advance = unhinted_adv.round();

        // Outline glyph
        let scaled_glyph = scaled_font.scaled_glyph(ch);
        let outlined = scaled_font.outline_glyph(scaled_glyph);

        let quad = if let Some(outlined) = outlined {
            let bounds = outlined.px_bounds();
            let b_w = bounds.width().round() as u32;
            let b_h = bounds.height().round() as u32;

            if b_w == 0
                || b_h == 0
                || b_w + AtlasPage::PADDING + 1 > AtlasPage::SIZE
                || b_h + AtlasPage::PADDING + 1 > AtlasPage::SIZE
            {
                None
            } else {
                let glyph_h = (bounds.height()).ceil();
                let inst = self.instances.get_mut(font.0 as usize).unwrap();
                if glyph_h > inst.max_height {
                    inst.max_height = glyph_h;
                }

                // Rasterise glyph into coverage buffer
                let mut coverage = vec![0u8; (b_w * b_h) as usize];
                outlined.draw(|x, y, c| {
                    if x < b_w && y < b_h {
                        coverage[(y * b_w + x) as usize] =
                            (c * 255.0).round().clamp(0.0, 255.0) as u8;
                    }
                });

                // Convert coverage to RGBA8 (RGB = 255, A = coverage)
                let mut rgba = vec![0u8; (b_w * b_h * 4) as usize];
                for i in 0..(b_w * b_h) as usize {
                    rgba[i * 4] = 255;
                    rgba[i * 4 + 1] = 255;
                    rgba[i * 4 + 2] = 255;
                    rgba[i * 4 + 3] = coverage[i];
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
                let corner = Vec2::new(bounds.min.x + 0.5, bounds.min.y - 0.5);

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
