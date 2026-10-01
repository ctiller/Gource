//! Shared pawn state and behaviour (port of pawn.cpp).

use glam::{Vec2, Vec3};
use gource_core::Bounds2D;

/// Default shadow strength multiplier (`gGourceShadowStrength`).
pub const DEFAULT_SHADOW_STRENGTH: f32 = 0.5;

/// Base pawn structure representing common attributes of files, users, etc.
/// Port of C++ `Pawn` in `pawn.h` / `pawn.cpp`.
#[derive(Debug, Clone)]
pub struct Pawn {
    pub name: String,
    pub pos: Vec2,
    pub shadow_offset: Vec2,

    pub namewidth: f32,
    pub speed: f32,

    pub elapsed: f32,
    pub fadetime: f32,

    pub nametime: f32,
    pub name_interval: f32,
    pub namecol: Vec3,

    pub shadow: bool,
    pub hidden: bool,
    pub tagid: i32,

    pub mouseover: bool,
    pub selected: bool,

    pub size: f32,
    pub graphic_ratio: f32,
    pub screenpos: Vec3,
    pub dims: Vec2,
}

impl Pawn {
    /// Port of `Pawn::Pawn(const std::string& name, vec2 pos, int tagid)`.
    pub fn new(name: String, pos: Vec2, tagid: i32) -> Self {
        Self {
            name,
            pos,
            tagid,
            hidden: false,
            speed: 1.0,
            selected: false,
            mouseover: false,
            shadow: false,
            namewidth: 0.0,
            shadow_offset: Vec2::new(2.0, 2.0),
            elapsed: 0.0,
            fadetime: 1.0,
            nametime: 5.0,
            name_interval: 0.0,
            namecol: Vec3::ONE,
            graphic_ratio: 1.0,
            size: 0.0,
            screenpos: Vec3::ZERO,
            dims: Vec2::ZERO,
        }
    }

    /// Port of `Pawn::getSize()`.
    pub fn size(&self) -> f32 {
        self.size
    }

    /// Port of `Pawn::getPos()`.
    pub fn pos(&self) -> Vec2 {
        self.pos
    }

    /// Port of `Pawn::setPos(vec2 pos)`.
    pub fn set_pos(&mut self, pos: Vec2) {
        self.pos = pos;
    }

    /// Port of `Pawn::getTagID()`.
    pub fn tag_id(&self) -> i32 {
        self.tagid
    }

    /// Port of `Pawn::getName()`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Port of `Pawn::showName()`.
    pub fn show_name(&mut self) {
        if self.name_interval <= 0.0 {
            self.name_interval = self.nametime;
        }
    }

    /// Port of `Pawn::updateQuadItemBounds()`.
    pub fn bounds(&self) -> Bounds2D {
        let halfsize_x = self.size * 0.5;
        let halfsize = Vec2::new(halfsize_x, halfsize_x * self.graphic_ratio);
        Bounds2D::from_points(self.pos - halfsize, self.pos + halfsize)
    }

    /// Port of `Pawn::logic(float dt)`.
    pub fn logic(&mut self, dt: f32) {
        self.elapsed += dt;
        if !self.hidden && self.name_interval > 0.0 {
            self.name_interval -= dt;
        }
    }

    /// Port of `Pawn::setGraphic`: updates `graphic_ratio` (h / w) and `dims`.
    pub fn set_graphic_dimensions(&mut self, width: u32, height: u32) {
        if width > 0 {
            self.graphic_ratio = height as f32 / width as f32;
        } else {
            self.graphic_ratio = 1.0;
        }
        self.dims = Vec2::new(self.size, self.size * self.graphic_ratio);
    }

    /// Port of `Pawn::setMouseOver(bool over)`.
    pub fn set_mouseover(&mut self, over: bool) {
        self.mouseover = over;
    }

    /// Port of `Pawn::isMouseOver()`.
    pub fn is_mouseover(&self) -> bool {
        self.mouseover
    }

    /// Port of `Pawn::setSelected(bool selected)`.
    pub fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }

    /// Port of `Pawn::isSelected()`.
    pub fn is_selected(&self) -> bool {
        self.selected
    }

    /// Port of `Pawn::setHidden(bool hidden)`.
    pub fn set_hidden(&mut self, hidden: bool) {
        self.hidden = hidden;
    }

    /// Port of `Pawn::isHidden()`.
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }

    /// Port of `Pawn::getAlpha()`.
    pub fn alpha(&self) -> f32 {
        if self.fadetime <= 0.0 {
            1.0
        } else {
            (self.elapsed / self.fadetime).min(1.0)
        }
    }

    /// Port of `Pawn::getColour()`.
    pub fn colour(&self) -> Vec3 {
        Vec3::ONE
    }

    /// Port of `Pawn::getNameColour()`.
    pub fn name_colour(&self) -> Vec3 {
        self.namecol
    }

    /// Port of `Pawn::nameVisible()`.
    pub fn name_visible(&self) -> bool {
        !((!self.selected && self.name_interval <= 0.0) || self.hidden)
    }

    /// Computes the name alpha according to C++ `Pawn::drawName`.
    pub fn name_alpha(&self) -> f32 {
        if !self.name_visible() {
            return 0.0;
        }
        let done = self.nametime - self.name_interval;
        if done < 1.0 {
            done.max(0.0)
        } else if done < self.nametime - 1.0 {
            1.0
        } else {
            (self.nametime - done).max(0.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pawn_basic_lifecycle() {
        let mut p = Pawn::new("alice".to_string(), Vec2::new(10.0, 20.0), 42);
        assert_eq!(p.name(), "alice");
        assert_eq!(p.pos(), Vec2::new(10.0, 20.0));
        assert_eq!(p.tag_id(), 42);
        assert!(!p.is_selected());
        assert!(!p.is_hidden());
        assert!(!p.is_mouseover());
        assert_eq!(p.alpha(), 0.0);

        p.size = 10.0;
        p.set_graphic_dimensions(100, 50);
        assert_eq!(p.graphic_ratio, 0.5);
        assert_eq!(p.dims, Vec2::new(10.0, 5.0));

        let b = p.bounds();
        assert_eq!(b.min, Vec2::new(5.0, 17.5));
        assert_eq!(b.max, Vec2::new(15.0, 22.5));

        p.logic(0.5);
        assert!((p.alpha() - 0.5).abs() < 1e-5);
        p.logic(0.6);
        assert_eq!(p.alpha(), 1.0);

        assert!(!p.name_visible());
        p.show_name();
        assert!(p.name_visible());
        assert_eq!(p.name_alpha(), 0.0); // done = 5 - 5 = 0
        p.logic(0.5);
        assert!((p.name_alpha() - 0.5).abs() < 1e-4);
        p.logic(1.0); // done = 1.5
        assert_eq!(p.name_alpha(), 1.0);
        p.logic(3.0); // done = 4.5 -> nametime - done = 0.5
        assert!((p.name_alpha() - 0.5).abs() < 1e-4);

        p.set_selected(true);
        assert!(p.is_selected());
        p.set_mouseover(true);
        assert!(p.is_mouseover());
        p.set_pos(Vec2::new(30.0, 40.0));
        assert_eq!(p.pos(), Vec2::new(30.0, 40.0));
        p.set_hidden(true);
        assert!(p.is_hidden());
        assert!(!p.name_visible());
    }
}
