//! The float view of the integer scene.
//!
//! The simulation (`gource-scene`, driven by [`World::step`](crate::world::World::step))
//! keeps every position in fixed point at a fixed tick. Each frame,
//! [`World::sync_view`](crate::world::World::sync_view) derives the float
//! fields the renderer, camera and picking read (`DirNode::pos`,
//! `File::pawn.pos`, `User::pawn.pos`, radii, bounds, quadtrees) by
//! interpolating between the previous and current tick and applying the
//! [`ViewTransform`] (user rotation). Nothing derived here feeds back into
//! the simulation.

use crate::kernel::fixed::{FRAC, UNIT};
use crate::{Fx, IVec2, ONE};
use glam::Vec2;

/// Q8 scale as a float.
const SCALE: f32 = ONE as f32;

/// A float setting (world units) in Q8.
#[inline]
pub fn to_fx(v: f32) -> Fx {
    let s = (v as f64 * SCALE as f64).round();
    s.clamp(i32::MIN as f64, i32::MAX as f64) as Fx
}

/// A Q8 value in world units.
#[inline]
pub fn from_fx(v: Fx) -> f32 {
    v as f32 / SCALE
}

/// A float position (world units) in Q8.
#[inline]
pub fn to_ivec(v: Vec2) -> IVec2 {
    IVec2::new(to_fx(v.x), to_fx(v.y))
}

/// A Q8 position in world units.
#[inline]
pub fn from_ivec(v: IVec2) -> Vec2 {
    Vec2::new(from_fx(v.x), from_fx(v.y))
}

/// A [`UNIT`]-scaled direction as a float vector.
#[inline]
pub fn from_unit(v: IVec2) -> Vec2 {
    Vec2::new(v.x as f32, v.y as f32) / UNIT as f32
}

/// A Q16 area in world units squared.
#[inline]
pub fn from_area(a: i64) -> f32 {
    (a as f64 / (1u64 << (2 * FRAC)) as f64) as f32
}

/// Linear interpolation between two Q8 positions, in world units.
#[inline]
pub fn lerp_ivec(a: IVec2, b: IVec2, alpha: f32) -> Vec2 {
    let fa = from_ivec(a);
    fa + (from_ivec(b) - fa) * alpha
}

/// The user's view rotation: `p' = M p + t`, accumulated from
/// rotate requests. The simulation never rotates; forces are rotation
/// invariant, so rotating the view is equivalent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewTransform {
    /// Row-major 2x2 matrix `[a, b; c, d]`.
    pub m: [f32; 4],
    pub t: Vec2,
}

impl Default for ViewTransform {
    fn default() -> Self {
        Self {
            m: [1.0, 0.0, 0.0, 1.0],
            t: Vec2::ZERO,
        }
    }
}

impl ViewTransform {
    /// Apply to a point.
    #[inline]
    pub fn point(&self, p: Vec2) -> Vec2 {
        self.vector(p) + self.t
    }

    /// Apply the linear part only (relative offsets such as file positions).
    #[inline]
    pub fn vector(&self, v: Vec2) -> Vec2 {
        Vec2::new(
            self.m[0] * v.x + self.m[1] * v.y,
            self.m[2] * v.x + self.m[3] * v.y,
        )
    }

    /// The inverse transform of a view-space point (simulation space, world
    /// units).
    pub fn inverse_point(&self, p: Vec2) -> Vec2 {
        let det = self.m[0] * self.m[3] - self.m[1] * self.m[2];
        if det == 0.0 {
            return p;
        }
        let q = p - self.t;
        Vec2::new(
            (self.m[3] * q.x - self.m[1] * q.y) / det,
            (-self.m[2] * q.x + self.m[0] * q.y) / det,
        )
    }

    /// Compose a rotation by `(sin, cos)` about `centre` (view space; the
    /// origin if `None`) after the current transform. Matches the old
    /// `World::rotate` (`rotate_vec2`: `x' = x c - y s`, `y' = x s + y c`).
    pub fn rotate(&mut self, s: f32, c: f32, centre: Option<Vec2>) {
        let ctr = centre.unwrap_or(Vec2::ZERO);
        let r = [c, -s, s, c];
        let m = self.m;
        self.m = [
            r[0] * m[0] + r[1] * m[2],
            r[0] * m[1] + r[1] * m[3],
            r[2] * m[0] + r[3] * m[2],
            r[2] * m[1] + r[3] * m[3],
        ];
        let q = self.t - ctr;
        self.t = Vec2::new(r[0] * q.x + r[1] * q.y, r[2] * q.x + r[3] * q.y) + ctr;
    }

    /// True if this is the identity.
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions_round_trip() {
        assert_eq!(to_fx(1.0), ONE);
        assert_eq!(to_fx(-1.5), -384);
        assert_eq!(from_fx(384), 1.5);
        assert_eq!(to_fx(f32::MAX), i32::MAX);
        let v = Vec2::new(3.25, -7.5);
        assert_eq!(from_ivec(to_ivec(v)), v);
        assert_eq!(from_unit(IVec2::new(UNIT, -UNIT / 2)), Vec2::new(1.0, -0.5));
        assert_eq!(from_area(65536 * 9), 9.0);
        assert_eq!(
            lerp_ivec(IVec2::ZERO, IVec2::new(ONE * 2, 0), 0.25),
            Vec2::new(0.5, 0.0)
        );
    }

    #[test]
    fn rotation_composes_and_inverts() {
        let mut t = ViewTransform::default();
        assert!(t.is_identity());
        // 90 degrees about the origin.
        t.rotate(1.0, 0.0, None);
        let p = t.point(Vec2::new(10.0, 0.0));
        assert!((p - Vec2::new(0.0, 10.0)).length() < 1e-5);
        // Another 90 about (0, 10): (0, 10) stays put.
        t.rotate(1.0, 0.0, Some(Vec2::new(0.0, 10.0)));
        let p2 = t.point(Vec2::new(10.0, 0.0));
        assert!((p2 - Vec2::new(0.0, 10.0)).length() < 1e-5);
        let back = t.inverse_point(p2);
        assert!((back - Vec2::new(10.0, 0.0)).length() < 1e-4);
        assert!(!t.is_identity());
        let v = t.vector(Vec2::new(1.0, 0.0));
        assert!((v - Vec2::new(-1.0, 0.0)).length() < 1e-5);
        let degenerate = ViewTransform {
            m: [0.0; 4],
            t: Vec2::ZERO,
        };
        assert_eq!(degenerate.inverse_point(Vec2::ONE), Vec2::ONE);
    }
}
