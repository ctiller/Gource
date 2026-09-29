//! CPU-side texture registry (replaces `core/texture.cpp`'s TextureManager).
//!
//! Frontends mirror entries to the GPU and re-upload whenever `version`
//! changes. Pixel data is RGBA8, straight alpha, and must be uploaded to a
//! *non-sRGB* texture format (values are used as-is, like the GL renderer).

use crate::list::TextureId;
use glam::UVec2;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Wrap {
    Clamp,
    Repeat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Filter {
    Nearest,
    Linear,
}

/// How a texture is sampled. With `mipmaps` the frontend should use
/// trilinear filtering (`GL_LINEAR_MIPMAP_LINEAR` in the C++ code).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureOptions {
    pub mipmaps: bool,
    pub wrap: Wrap,
    pub filter: Filter,
}

impl Default for TextureOptions {
    fn default() -> Self {
        Self {
            mipmaps: true,
            wrap: Wrap::Clamp,
            filter: Filter::Linear,
        }
    }
}

impl TextureOptions {
    /// Linear filtering, no mipmaps, clamped (e.g. glyph atlases, logos).
    pub const fn plain() -> Self {
        Self {
            mipmaps: false,
            wrap: Wrap::Clamp,
            filter: Filter::Linear,
        }
    }
}

/// A texture with its full mip chain.
#[derive(Debug, Clone, PartialEq)]
pub struct Texture {
    /// Path or logical name, for debugging and caching.
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub options: TextureOptions,
    /// RGBA8 pixel data, level 0 first. With `options.mipmaps` this holds the
    /// complete chain down to 1x1 (each level half the size of the previous,
    /// rounded down, min 1), generated with a box filter.
    pub levels: Vec<Vec<u8>>,
    /// Incremented whenever the pixel data changes.
    pub version: u64,
}

impl Texture {
    pub fn size(&self) -> UVec2 {
        UVec2::new(self.width, self.height)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TextureError {
    #[error("failed to read '{path}': {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to decode image '{name}': {message}")]
    Decode { name: String, message: String },
}

/// Generate mip levels starting from level 0 (width x height, RGBA8).
/// Each level is half the size of the previous (rounded down, minimum 1).
/// Uses a 2x2 box filter with edge clamping for odd dimensions.
fn generate_mipmaps(width: u32, height: u32, base_rgba: &[u8]) -> Vec<Vec<u8>> {
    let mut levels = Vec::new();
    levels.push(base_rgba.to_vec());

    let mut cur_w = width;
    let mut cur_h = height;
    let mut cur_pixels = base_rgba.to_vec();

    while cur_w > 1 || cur_h > 1 {
        let next_w = (cur_w / 2).max(1);
        let next_h = (cur_h / 2).max(1);
        let mut next_pixels = vec![0u8; (next_w * next_h * 4) as usize];

        for dst_y in 0..next_h {
            for dst_x in 0..next_w {
                let src_x0 = dst_x * 2;
                let src_y0 = dst_y * 2;
                let src_x1 = (src_x0 + 1).min(cur_w - 1);
                let src_y1 = (src_y0 + 1).min(cur_h - 1);

                let p00_idx = ((src_y0 * cur_w + src_x0) * 4) as usize;
                let p10_idx = ((src_y0 * cur_w + src_x1) * 4) as usize;
                let p01_idx = ((src_y1 * cur_w + src_x0) * 4) as usize;
                let p11_idx = ((src_y1 * cur_w + src_x1) * 4) as usize;

                let dst_idx = ((dst_y * next_w + dst_x) * 4) as usize;

                for c in 0..4 {
                    let sum = cur_pixels[p00_idx + c] as u32
                        + cur_pixels[p10_idx + c] as u32
                        + cur_pixels[p01_idx + c] as u32
                        + cur_pixels[p11_idx + c] as u32;
                    next_pixels[dst_idx + c] = ((sum + 2) / 4) as u8;
                }
            }
        }

        levels.push(next_pixels.clone());
        cur_w = next_w;
        cur_h = next_h;
        cur_pixels = next_pixels;
    }

    levels
}

/// Registry of textures addressed by [`TextureId`].
///
/// `TextureId::WHITE` (id 0) is always a 1x1 opaque white texture.
#[derive(Debug)]
pub struct TextureStore {
    textures: Vec<Option<Texture>>,
    cache: HashMap<(String, TextureOptions), TextureId>,
    /// Textures loaded with [`TextureStore::load_file`] (their name is the
    /// path), for [`TextureStore::reload_files`].
    files: BTreeSet<TextureId>,
}

impl Default for TextureStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Read and decode an image file (png, jpeg, bmp, gif, tga) to RGBA8.
fn decode_file(path: &Path) -> Result<(u32, u32, Vec<u8>), TextureError> {
    let name = path.to_string_lossy().into_owned();
    let reader = image::ImageReader::open(path).map_err(|e| TextureError::Io {
        path: name.clone(),
        source: e,
    })?;

    let reader = reader.with_guessed_format().map_err(|e| TextureError::Io {
        path: name.clone(),
        source: e,
    })?;

    let img = reader.decode().map_err(|e| TextureError::Decode {
        name,
        message: e.to_string(),
    })?;

    let rgba = img.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    Ok((width, height, rgba.into_raw()))
}

impl TextureStore {
    /// Create a store containing only the white texture.
    pub fn new() -> Self {
        let white_rgba = vec![255, 255, 255, 255];
        let white_texture = Texture {
            name: "WHITE".to_string(),
            width: 1,
            height: 1,
            options: TextureOptions::plain(),
            levels: vec![white_rgba],
            version: 1,
        };
        Self {
            textures: vec![Some(white_texture)],
            cache: HashMap::new(),
            files: BTreeSet::new(),
        }
    }

    /// Load and decode an image file (png, jpeg, bmp, gif, tga), converting to
    /// RGBA8 and generating mipmaps if requested. Cached by (path, options):
    /// loading the same file twice returns the same id.
    pub fn load_file(
        &mut self,
        path: &Path,
        options: TextureOptions,
    ) -> Result<TextureId, TextureError> {
        let key = (path.to_string_lossy().into_owned(), options);
        if let Some(&id) = self.cache.get(&key) {
            return Ok(id);
        }

        let (width, height, raw) = decode_file(path)?;
        let id = self.create_rgba(&key.0, width, height, raw, options);
        self.cache.insert(key, id);
        self.files.insert(id);
        Ok(id)
    }

    /// Re-read every texture loaded with [`TextureStore::load_file`] from
    /// its file, keeping its id and options and bumping its version (the
    /// C++ `TextureManager::unload()` + `reload()` of F5). A texture whose
    /// file can no longer be read keeps its old pixels; the errors are
    /// returned.
    pub fn reload_files(&mut self) -> Vec<TextureError> {
        let mut errors = Vec::new();
        let ids: Vec<TextureId> = self.files.iter().copied().collect();
        for id in ids {
            let Some(Some(tex)) = self.textures.get(id.0 as usize) else {
                continue;
            };
            match decode_file(Path::new(&tex.name)) {
                Ok((width, height, rgba)) => {
                    let tex = self.textures[id.0 as usize]
                        .as_mut()
                        .expect("checked above");
                    tex.levels = if tex.options.mipmaps {
                        generate_mipmaps(width, height, &rgba)
                    } else {
                        vec![rgba]
                    };
                    tex.width = width;
                    tex.height = height;
                    tex.version = tex.version.wrapping_add(1);
                }
                Err(e) => errors.push(e),
            }
        }
        errors
    }

    /// Decode an in-memory image (e.g. an embedded resource). Cached by
    /// (name, options).
    pub fn load_bytes(
        &mut self,
        name: &str,
        bytes: &[u8],
        options: TextureOptions,
    ) -> Result<TextureId, TextureError> {
        let key = (name.to_string(), options);
        if let Some(&id) = self.cache.get(&key) {
            return Ok(id);
        }

        let id = self.load_bytes_impl(name, bytes, options)?;
        self.cache.insert(key, id);
        Ok(id)
    }

    fn load_bytes_impl(
        &mut self,
        name: &str,
        bytes: &[u8],
        options: TextureOptions,
    ) -> Result<TextureId, TextureError> {
        let img = image::load_from_memory(bytes).map_err(|e| TextureError::Decode {
            name: name.to_string(),
            message: e.to_string(),
        })?;

        let rgba = img.to_rgba8();
        let width = rgba.width();
        let height = rgba.height();
        let raw = rgba.into_raw();

        Ok(self.create_rgba(name, width, height, raw, options))
    }

    /// Register raw RGBA8 pixels (`rgba.len() == width * height * 4`).
    /// Not cached: always creates a new texture.
    pub fn create_rgba(
        &mut self,
        name: &str,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        options: TextureOptions,
    ) -> TextureId {
        let expected_len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .expect("image dimensions overflow");
        assert_eq!(
            rgba.len(),
            expected_len,
            "buffer length {} does not match width * height * 4 = {} ({}x{})",
            rgba.len(),
            expected_len,
            width,
            height
        );

        let levels = if options.mipmaps {
            generate_mipmaps(width, height, &rgba)
        } else {
            vec![rgba]
        };

        let texture = Texture {
            name: name.to_string(),
            width,
            height,
            options,
            levels,
            version: 1,
        };

        let id = TextureId(self.textures.len() as u32);
        self.textures.push(Some(texture));
        id
    }

    /// Replace a rectangle of level 0 (`rgba.len() == w * h * 4`), regenerate
    /// the mip chain if the texture has one, and bump the version.
    /// Out-of-range rectangles are clipped.
    pub fn update_rgba(&mut self, id: TextureId, x: u32, y: u32, w: u32, h: u32, rgba: &[u8]) {
        let expected_len = (w as usize)
            .checked_mul(h as usize)
            .and_then(|n| n.checked_mul(4))
            .expect("update dimensions overflow");
        assert_eq!(
            rgba.len(),
            expected_len,
            "update_rgba: buffer length {} does not match w * h * 4 = {} ({}x{})",
            rgba.len(),
            expected_len,
            w,
            h
        );

        let Some(Some(tex)) = self.textures.get_mut(id.0 as usize) else {
            return;
        };

        if w == 0 || h == 0 || x >= tex.width || y >= tex.height {
            return;
        }

        // Clip rectangle to texture boundaries
        let clip_w = w.min(tex.width - x);
        let clip_h = h.min(tex.height - y);

        let level0 = &mut tex.levels[0];
        let tex_w = tex.width;

        for row in 0..clip_h {
            let src_start = ((row * w) * 4) as usize;
            let src_end = src_start + (clip_w * 4) as usize;
            let dst_start = (((y + row) * tex_w + x) * 4) as usize;
            let dst_end = dst_start + (clip_w * 4) as usize;
            level0[dst_start..dst_end].copy_from_slice(&rgba[src_start..src_end]);
        }

        if tex.options.mipmaps {
            tex.levels = generate_mipmaps(tex.width, tex.height, level0);
        }

        tex.version = tex.version.wrapping_add(1);
    }

    pub fn get(&self, id: TextureId) -> Option<&Texture> {
        self.textures.get(id.0 as usize).and_then(|t| t.as_ref())
    }

    /// Size of a texture, or zero if it does not exist.
    pub fn size(&self, id: TextureId) -> UVec2 {
        self.get(id).map(|t| t.size()).unwrap_or(UVec2::ZERO)
    }

    /// All live textures.
    pub fn iter(&self) -> impl Iterator<Item = (TextureId, &Texture)> {
        self.textures
            .iter()
            .enumerate()
            .filter_map(|(i, t)| t.as_ref().map(|t| (TextureId(i as u32), t)))
    }

    /// Remove a texture (the white texture cannot be removed). Ids are not
    /// reused.
    pub fn remove(&mut self, id: TextureId) {
        if id == TextureId::WHITE {
            return;
        }
        if let Some(tex) = self
            .textures
            .get_mut(id.0 as usize)
            .and_then(|slot| slot.take())
        {
            self.cache.remove(&(tex.name, tex.options));
            self.files.remove(&id);
        }
    }
}
