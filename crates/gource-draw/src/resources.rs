//! Default resources embedded in the binary, so Gource works without an
//! installed data directory.

/// Default file graphic (`data/file.png`).
pub const FILE_PNG: &[u8] = include_bytes!("../../../data/file.png");

/// Default user avatar (`data/user.png`).
pub const USER_PNG: &[u8] = include_bytes!("../../../data/user.png");

/// Beam texture used for tree edges and action beams (`data/beam.png`).
pub const BEAM_PNG: &[u8] = include_bytes!("../../../data/beam.png");

/// Default font (`data/fonts/FreeSans.ttf`, GPL with font exception).
pub const FREESANS_TTF: &[u8] = include_bytes!("../../../data/fonts/FreeSans.ttf");

/// The file name the C++ version uses for the default font. When the
/// configured font file equals this, the embedded font is used.
pub const DEFAULT_FONT_NAME: &str = "FreeSans.ttf";
