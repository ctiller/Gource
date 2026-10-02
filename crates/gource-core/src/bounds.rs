//! Axis aligned 2D bounding boxes (port of `core/bounds.h`).

use crate::vec::Vec2;

/// An axis aligned bounding box that grows as points are added.
///
/// Mirrors the C++ `Bounds2D`: a freshly reset box is "empty" (`first == true`)
/// and the first point added sets both `min` and `max`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds2D {
    pub min: Vec2,
    pub max: Vec2,
    /// True until the first point has been added.
    pub first: bool,
}

impl Default for Bounds2D {
    fn default() -> Self {
        Self::new()
    }
}

impl Bounds2D {
    /// An empty box.
    pub const fn new() -> Self {
        Self {
            min: Vec2::ZERO,
            max: Vec2::ZERO,
            first: true,
        }
    }

    /// A box containing exactly the two given points.
    pub fn from_points(a: Vec2, b: Vec2) -> Self {
        let mut bounds = Self::new();
        bounds.update(a);
        bounds.update(b);
        bounds
    }

    pub fn is_empty(&self) -> bool {
        self.first
    }

    pub fn centre(&self) -> Vec2 {
        self.min + (self.max - self.min) * 0.5
    }

    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }

    pub fn area(&self) -> f32 {
        self.width() * self.height()
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Grow to contain `point`.
    pub fn update(&mut self, point: Vec2) {
        if self.first {
            self.min = point;
            self.max = point;
            self.first = false;
            return;
        }
        if self.min.x > point.x {
            self.min.x = point.x;
        }
        if self.min.y > point.y {
            self.min.y = point.y;
        }
        if self.max.x < point.x {
            self.max.x = point.x;
        }
        if self.max.y < point.y {
            self.max.y = point.y;
        }
    }

    /// Grow to contain another box (its min and max corners).
    pub fn update_bounds(&mut self, other: &Bounds2D) {
        self.update(other.min);
        self.update(other.max);
    }

    /// Reset and set to contain only `other`.
    pub fn set_bounds(&mut self, other: &Bounds2D) {
        self.reset();
        self.update_bounds(other);
    }

    /// Reset and set to contain only `point`.
    pub fn set_point(&mut self, point: Vec2) {
        self.reset();
        self.update(point);
    }

    /// Reset and set to contain the two points.
    pub fn set_points(&mut self, a: Vec2, b: Vec2) {
        self.reset();
        self.update(a);
        self.update(b);
    }

    /// Inclusive containment test. An empty box contains nothing.
    pub fn contains(&self, point: Vec2) -> bool {
        if self.first {
            return false;
        }
        self.min.x <= point.x
            && self.min.y <= point.y
            && self.max.x >= point.x
            && self.max.y >= point.y
    }

    /// Inclusive overlap test (note: like the C++ version this ignores `first`).
    pub fn overlaps(&self, b: &Bounds2D) -> bool {
        !(self.max.y < b.min.y
            || self.min.y > b.max.y
            || self.max.x < b.min.x
            || self.min.x > b.max.x)
    }

    /// The four corners in drawing order: min, (max.x,min.y), max, (min.x,max.y).
    pub fn corners(&self) -> [Vec2; 4] {
        [
            self.min,
            Vec2::new(self.max.x, self.min.y),
            self.max,
            Vec2::new(self.min.x, self.max.y),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grows_from_first_point() {
        let mut b = Bounds2D::new();
        assert!(b.is_empty());
        assert!(!b.contains(Vec2::ZERO));
        b.update(Vec2::new(1.0, 2.0));
        assert_eq!(b.min, Vec2::new(1.0, 2.0));
        assert_eq!(b.max, Vec2::new(1.0, 2.0));
        b.update(Vec2::new(-1.0, 5.0));
        assert_eq!(b.min, Vec2::new(-1.0, 2.0));
        assert_eq!(b.max, Vec2::new(1.0, 5.0));
        assert_eq!(b.width(), 2.0);
        assert_eq!(b.height(), 3.0);
        assert_eq!(b.area(), 6.0);
        assert_eq!(b.centre(), Vec2::new(0.0, 3.5));
        assert!(b.contains(Vec2::new(0.0, 3.0)));
        assert!(b.contains(Vec2::new(1.0, 5.0)));
        assert!(!b.contains(Vec2::new(1.1, 5.0)));
    }

    #[test]
    fn overlap() {
        let a = Bounds2D::from_points(Vec2::ZERO, Vec2::splat(10.0));
        let b = Bounds2D::from_points(Vec2::splat(10.0), Vec2::splat(20.0));
        let c = Bounds2D::from_points(Vec2::splat(10.1), Vec2::splat(20.0));
        assert!(a.overlaps(&b));
        assert!(!a.overlaps(&c));
    }

    #[test]
    fn setters_and_corners() {
        let mut b = Bounds2D::default();
        assert!(b.is_empty());
        b.set_point(Vec2::new(5.0, 5.0));
        assert!(!b.is_empty());
        assert_eq!(b.min, Vec2::new(5.0, 5.0));
        assert_eq!(b.max, Vec2::new(5.0, 5.0));

        b.set_points(Vec2::new(0.0, 10.0), Vec2::new(20.0, 30.0));
        assert_eq!(b.min, Vec2::new(0.0, 10.0));
        assert_eq!(b.max, Vec2::new(20.0, 30.0));

        let corners = b.corners();
        assert_eq!(corners[0], Vec2::new(0.0, 10.0));
        assert_eq!(corners[1], Vec2::new(20.0, 10.0));
        assert_eq!(corners[2], Vec2::new(20.0, 30.0));
        assert_eq!(corners[3], Vec2::new(0.0, 30.0));

        let other = Bounds2D::from_points(Vec2::new(-10.0, -5.0), Vec2::new(15.0, 25.0));
        b.update_bounds(&other);
        assert_eq!(b.min, Vec2::new(-10.0, -5.0));
        assert_eq!(b.max, Vec2::new(20.0, 30.0));

        b.set_bounds(&other);
        assert_eq!(b.min, Vec2::new(-10.0, -5.0));
        assert_eq!(b.max, Vec2::new(15.0, 25.0));

        b.reset();
        assert!(b.is_empty());
    }

    #[test]
    fn degenerate_and_empty_bounds() {
        // Point bounds (width = 0, height = 0, area = 0)
        let mut b = Bounds2D::new();
        b.update(Vec2::new(5.0, 5.0));
        assert!(!b.is_empty());
        assert_eq!(b.width(), 0.0);
        assert_eq!(b.height(), 0.0);
        assert_eq!(b.area(), 0.0);
        assert_eq!(b.centre(), Vec2::new(5.0, 5.0));
        assert!(b.contains(Vec2::new(5.0, 5.0)));
        assert!(!b.contains(Vec2::new(5.0, 5.001)));

        // Overlap with degenerate point bounds
        let p_same = Bounds2D::from_points(Vec2::new(5.0, 5.0), Vec2::new(5.0, 5.0));
        assert!(b.overlaps(&p_same));

        let p_diff = Bounds2D::from_points(Vec2::new(5.1, 5.0), Vec2::new(5.1, 5.0));
        assert!(!b.overlaps(&p_diff));

        // Line bounds (horizontal line: height = 0)
        let line = Bounds2D::from_points(Vec2::new(0.0, 5.0), Vec2::new(10.0, 5.0));
        assert_eq!(line.width(), 10.0);
        assert_eq!(line.height(), 0.0);
        assert_eq!(line.area(), 0.0);
        assert!(line.contains(Vec2::new(5.0, 5.0)));
        assert!(line.contains(Vec2::new(0.0, 5.0)));
        assert!(line.contains(Vec2::new(10.0, 5.0)));
        assert!(!line.contains(Vec2::new(5.0, 5.1)));

        // Empty bounds behaviour
        let empty = Bounds2D::new();
        assert!(empty.is_empty());
        assert!(!empty.contains(Vec2::ZERO));
        assert_eq!(empty.centre(), Vec2::ZERO);

        // Touching boundary edges
        let b1 = Bounds2D::from_points(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0));
        let b2 = Bounds2D::from_points(Vec2::new(10.0, 0.0), Vec2::new(20.0, 10.0));
        assert!(b1.overlaps(&b2));
        assert!(b2.overlaps(&b1));

        // Negative coordinates
        let neg = Bounds2D::from_points(Vec2::new(-20.0, -30.0), Vec2::new(-10.0, -5.0));
        assert_eq!(neg.width(), 10.0);
        assert_eq!(neg.height(), 25.0);
        assert_eq!(neg.centre(), Vec2::new(-15.0, -17.5));
        assert!(neg.contains(Vec2::new(-15.0, -20.0)));
        assert!(!neg.contains(Vec2::new(0.0, 0.0)));
    }
}
