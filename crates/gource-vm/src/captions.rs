//! Timed caption overlay display model (`CaptionsVM`, [`CaptionItemVM`]).

use gource_core::{Vec2, Vec3};

/// A single timed caption overlay item.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptionItemVM {
    pub caption: String,
    pub timestamp: i64,
    pub pos: Vec2,
    pub colour: Vec3,
    pub alpha: f32,
}

/// Display model for active captions.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct CaptionsVM {
    pub items: Vec<CaptionItemVM>,
}
