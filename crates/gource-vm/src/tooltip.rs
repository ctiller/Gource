//! Hover / selection tooltip display model (`TooltipVM`).

use gource_core::{Vec2, Vec3};

/// Display model for a hovered or selected scene entity tooltip.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct TooltipVM {
    pub visible: bool,
    pub screen_pos: Vec2,
    pub title: String,
    pub subtitle: Option<String>,
    pub accent_colour: Vec3,
}
