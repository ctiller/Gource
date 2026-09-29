//! Small vector helpers (port of `core/vectors.{h,cpp}`).

use glam::{Vec2, Vec3, Vec4};

/// `PI` from `core/pi.h`: a *double* literal, so C++ expressions such as
/// `radius * PI` are evaluated in double precision and then rounded to
/// float. Use this (in f64) wherever the C++ code uses `PI`. It is the
/// truncated C++ literal on purpose: results must match bit for bit.
#[allow(clippy::approx_constant)]
pub const CPP_PI: f64 = 3.14159265;

/// `DEGREES_TO_RADIANS` from `core/pi.h` (also a double literal).
pub const CPP_DEGREES_TO_RADIANS: f64 = 0.017453292;

/// Rotate `v` by the angle whose sine is `s` and cosine is `c`.
pub fn rotate_vec2(v: Vec2, s: f32, c: f32) -> Vec2 {
    Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// Normalise, returning the input unchanged if it has zero length.
pub fn normalise2(v: Vec2) -> Vec2 {
    let l = v.length();
    if l > 0.0 { v / l } else { v }
}

/// Normalise, returning the input unchanged if it has zero length.
pub fn normalise3(v: Vec3) -> Vec3 {
    let l = v.length();
    if l > 0.0 { v / l } else { v }
}

/// Normalise, returning the input unchanged if it has zero length.
pub fn normalise4(v: Vec4) -> Vec4 {
    let l = v.length();
    if l > 0.0 { v / l } else { v }
}

/// A 2D value that remembers a previous snapshot so it can be interpolated
/// (port of `lerp2`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Lerp2 {
    /// Current value.
    pub v: Vec2,
    /// Snapshot taken by [`Lerp2::snap`].
    pub p: Vec2,
    /// Last interpolated value.
    pub l: Vec2,
}

impl Lerp2 {
    pub fn snap(&mut self) {
        self.p = self.v;
    }

    pub fn lerp(&mut self, n: f32) -> Vec2 {
        self.l = self.p + (self.v - self.p) * n;
        self.l
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_quarter_turn() {
        let r = rotate_vec2(Vec2::X, 1.0, 0.0);
        assert!((r - Vec2::Y).length() < 1e-6);
    }

    #[test]
    fn normalise_zero_is_zero() {
        assert_eq!(normalise2(Vec2::ZERO), Vec2::ZERO);
        assert_eq!(normalise3(Vec3::ZERO), Vec3::ZERO);
        assert_eq!(normalise4(Vec4::ZERO), Vec4::ZERO);
        assert_eq!(
            normalise4(Vec4::new(0.0, 3.0, 4.0, 0.0)),
            Vec4::new(0.0, 0.6, 0.8, 0.0)
        );
    }

    #[test]
    fn lerp2_tests() {
        let mut l = Lerp2::default();
        assert_eq!(l.v, Vec2::ZERO);
        l.v = Vec2::new(10.0, 20.0);
        l.snap();
        assert_eq!(l.p, Vec2::new(10.0, 20.0));
        l.v = Vec2::new(20.0, 40.0);
        let mid = l.lerp(0.5);
        assert_eq!(mid, Vec2::new(15.0, 30.0));
        assert_eq!(l.l, mid);

        // lerp with n = 0.0 and n = 1.0
        assert_eq!(l.lerp(0.0), Vec2::new(10.0, 20.0));
        assert_eq!(l.lerp(1.0), Vec2::new(20.0, 40.0));
        // Extrapolation
        assert_eq!(l.lerp(2.0), Vec2::new(30.0, 60.0));
    }

    #[test]
    fn rotate_vec2_arbitrary_angles() {
        // 180 degrees: sin=0, cos=-1
        let v = Vec2::new(3.0, 4.0);
        let r = rotate_vec2(v, 0.0, -1.0);
        assert!((r - Vec2::new(-3.0, -4.0)).length() < 1e-6);

        // Arbitrary angle: 45 deg
        let angle = std::f32::consts::FRAC_PI_4;
        let s = angle.sin();
        let c = angle.cos();
        let r45 = rotate_vec2(Vec2::X, s, c);
        assert!((r45 - Vec2::new(c, s)).length() < 1e-6);
    }

    #[test]
    fn normalise_non_zero_vectors() {
        let n2 = normalise2(Vec2::new(3.0, 4.0));
        assert!((n2 - Vec2::new(0.6, 0.8)).length() < 1e-6);
        assert!((n2.length() - 1.0).abs() < 1e-6);

        let n3 = normalise3(Vec3::new(2.0, 3.0, 6.0));
        assert!((n3 - Vec3::new(2.0 / 7.0, 3.0 / 7.0, 6.0 / 7.0)).length() < 1e-6);
        assert!((n3.length() - 1.0).abs() < 1e-6);

        let n4 = normalise4(Vec4::new(1.0, 1.0, 1.0, 1.0));
        assert!((n4.length() - 1.0).abs() < 1e-6);
    }
}
