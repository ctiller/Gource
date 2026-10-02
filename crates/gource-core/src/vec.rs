//! Vector types for 2D, 3D, and 4D maths, replacing the external `glam` dependency.

use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

// ============================================================================
// Vec2
// ============================================================================

/// 2-element 32-bit floating point vector.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };
    pub const ONE: Self = Self { x: 1.0, y: 1.0 };
    pub const NEG_ONE: Self = Self { x: -1.0, y: -1.0 };
    pub const MIN: Self = Self {
        x: f32::MIN,
        y: f32::MIN,
    };
    pub const MAX: Self = Self {
        x: f32::MAX,
        y: f32::MAX,
    };
    pub const NAN: Self = Self {
        x: f32::NAN,
        y: f32::NAN,
    };
    pub const INFINITY: Self = Self {
        x: f32::INFINITY,
        y: f32::INFINITY,
    };
    pub const NEG_INFINITY: Self = Self {
        x: f32::NEG_INFINITY,
        y: f32::NEG_INFINITY,
    };

    pub const X: Self = Self { x: 1.0, y: 0.0 };
    pub const Y: Self = Self { x: 0.0, y: 1.0 };
    pub const NEG_X: Self = Self { x: -1.0, y: 0.0 };
    pub const NEG_Y: Self = Self { x: 0.0, y: -1.0 };

    pub const AXES: [Self; 2] = [Self::X, Self::Y];

    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    #[inline]
    pub const fn splat(v: f32) -> Self {
        Self { x: v, y: v }
    }

    #[inline]
    pub const fn to_array(self) -> [f32; 2] {
        [self.x, self.y]
    }

    #[inline]
    pub const fn from_array(a: [f32; 2]) -> Self {
        Self { x: a[0], y: a[1] }
    }

    #[inline]
    pub const fn extend(self, z: f32) -> Vec3 {
        Vec3 {
            x: self.x,
            y: self.y,
            z,
        }
    }

    #[inline]
    pub const fn as_uvec2(&self) -> UVec2 {
        UVec2 {
            x: self.x as u32,
            y: self.y as u32,
        }
    }

    #[inline]
    pub const fn as_ivec2(&self) -> IVec2 {
        IVec2 {
            x: self.x as i32,
            y: self.y as i32,
        }
    }

    #[inline]
    pub const fn as_dvec2(self) -> (f64, f64) {
        (self.x as f64, self.y as f64)
    }

    #[inline]
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y
    }

    #[inline]
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    #[inline]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    #[inline]
    pub fn distance(self, rhs: Self) -> f32 {
        (self - rhs).length()
    }

    #[inline]
    pub fn distance_squared(self, rhs: Self) -> f32 {
        (self - rhs).length_squared()
    }

    #[inline]
    pub fn normalize(self) -> Self {
        let len = self.length();
        self / len
    }

    #[inline]
    pub fn normalize_or_zero(self) -> Self {
        let len = self.length();
        if len > 0.0 { self / len } else { Self::ZERO }
    }

    #[inline]
    pub fn lerp(self, rhs: Self, s: f32) -> Self {
        self + (rhs - self) * s
    }

    #[inline]
    pub fn min(self, rhs: Self) -> Self {
        Self {
            x: self.x.min(rhs.x),
            y: self.y.min(rhs.y),
        }
    }

    #[inline]
    pub fn max(self, rhs: Self) -> Self {
        Self {
            x: self.x.max(rhs.x),
            y: self.y.max(rhs.y),
        }
    }

    #[inline]
    pub fn clamp(self, min: Self, max: Self) -> Self {
        Self {
            x: self.x.clamp(min.x, max.x),
            y: self.y.clamp(min.y, max.y),
        }
    }

    #[inline]
    pub fn abs(self) -> Self {
        Self {
            x: self.x.abs(),
            y: self.y.abs(),
        }
    }

    #[inline]
    pub fn floor(self) -> Self {
        Self {
            x: self.x.floor(),
            y: self.y.floor(),
        }
    }

    #[inline]
    pub fn ceil(self) -> Self {
        Self {
            x: self.x.ceil(),
            y: self.y.ceil(),
        }
    }

    #[inline]
    pub fn round(self) -> Self {
        Self {
            x: self.x.round(),
            y: self.y.round(),
        }
    }

    #[inline]
    pub const fn perp(self) -> Self {
        Self {
            x: -self.y,
            y: self.x,
        }
    }

    #[inline]
    pub fn angle_between(self, rhs: Self) -> f32 {
        let cos_theta = self.dot(rhs) / (self.length() * rhs.length());
        cos_theta.clamp(-1.0, 1.0).acos()
    }
}

impl From<[f32; 2]> for Vec2 {
    #[inline]
    fn from(a: [f32; 2]) -> Self {
        Self::from_array(a)
    }
}

impl From<Vec2> for [f32; 2] {
    #[inline]
    fn from(v: Vec2) -> Self {
        v.to_array()
    }
}

impl From<(f32, f32)> for Vec2 {
    #[inline]
    fn from((x, y): (f32, f32)) -> Self {
        Self { x, y }
    }
}

impl From<Vec2> for (f32, f32) {
    #[inline]
    fn from(v: Vec2) -> Self {
        (v.x, v.y)
    }
}

impl Add for Vec2 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl AddAssign for Vec2 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl Sub for Vec2 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl SubAssign for Vec2 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}

impl Mul<Self> for Vec2 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        Self {
            x: self.x * rhs.x,
            y: self.y * rhs.y,
        }
    }
}

impl Mul<f32> for Vec2 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: f32) -> Self {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl Mul<Vec2> for f32 {
    type Output = Vec2;
    #[inline]
    fn mul(self, rhs: Vec2) -> Vec2 {
        Vec2 {
            x: self * rhs.x,
            y: self * rhs.y,
        }
    }
}

impl MulAssign<Self> for Vec2 {
    #[inline]
    fn mul_assign(&mut self, rhs: Self) {
        self.x *= rhs.x;
        self.y *= rhs.y;
    }
}

impl MulAssign<f32> for Vec2 {
    #[inline]
    fn mul_assign(&mut self, rhs: f32) {
        self.x *= rhs;
        self.y *= rhs;
    }
}

impl Div<Self> for Vec2 {
    type Output = Self;
    #[inline]
    fn div(self, rhs: Self) -> Self {
        Self {
            x: self.x / rhs.x,
            y: self.y / rhs.y,
        }
    }
}

impl Div<f32> for Vec2 {
    type Output = Self;
    #[inline]
    fn div(self, rhs: f32) -> Self {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

impl DivAssign<Self> for Vec2 {
    #[inline]
    fn div_assign(&mut self, rhs: Self) {
        self.x /= rhs.x;
        self.y /= rhs.y;
    }
}

impl DivAssign<f32> for Vec2 {
    #[inline]
    fn div_assign(&mut self, rhs: f32) {
        self.x /= rhs;
        self.y /= rhs;
    }
}

impl Neg for Vec2 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
        }
    }
}

// ============================================================================
// Vec3
// ============================================================================

/// 3-element 32-bit floating point vector.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    pub const ONE: Self = Self {
        x: 1.0,
        y: 1.0,
        z: 1.0,
    };
    pub const NEG_ONE: Self = Self {
        x: -1.0,
        y: -1.0,
        z: -1.0,
    };
    pub const MIN: Self = Self {
        x: f32::MIN,
        y: f32::MIN,
        z: f32::MIN,
    };
    pub const MAX: Self = Self {
        x: f32::MAX,
        y: f32::MAX,
        z: f32::MAX,
    };
    pub const NAN: Self = Self {
        x: f32::NAN,
        y: f32::NAN,
        z: f32::NAN,
    };
    pub const INFINITY: Self = Self {
        x: f32::INFINITY,
        y: f32::INFINITY,
        z: f32::INFINITY,
    };
    pub const NEG_INFINITY: Self = Self {
        x: f32::NEG_INFINITY,
        y: f32::NEG_INFINITY,
        z: f32::NEG_INFINITY,
    };

    pub const X: Self = Self {
        x: 1.0,
        y: 0.0,
        z: 0.0,
    };
    pub const Y: Self = Self {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };
    pub const Z: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    };
    pub const NEG_X: Self = Self {
        x: -1.0,
        y: 0.0,
        z: 0.0,
    };
    pub const NEG_Y: Self = Self {
        x: 0.0,
        y: -1.0,
        z: 0.0,
    };
    pub const NEG_Z: Self = Self {
        x: 0.0,
        y: 0.0,
        z: -1.0,
    };

    pub const AXES: [Self; 3] = [Self::X, Self::Y, Self::Z];

    #[inline]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    #[inline]
    pub const fn splat(v: f32) -> Self {
        Self { x: v, y: v, z: v }
    }

    #[inline]
    pub const fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }

    #[inline]
    pub const fn from_array(a: [f32; 3]) -> Self {
        Self {
            x: a[0],
            y: a[1],
            z: a[2],
        }
    }

    #[inline]
    pub const fn extend(self, w: f32) -> Vec4 {
        Vec4 {
            x: self.x,
            y: self.y,
            z: self.z,
            w,
        }
    }

    #[inline]
    pub const fn truncate(self) -> Vec2 {
        Vec2 {
            x: self.x,
            y: self.y,
        }
    }

    #[inline]
    pub const fn xy(self) -> Vec2 {
        Vec2 {
            x: self.x,
            y: self.y,
        }
    }

    #[inline]
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    #[inline]
    pub fn cross(self, rhs: Self) -> Self {
        Self {
            x: self.y * rhs.z - self.z * rhs.y,
            y: self.z * rhs.x - self.x * rhs.z,
            z: self.x * rhs.y - self.y * rhs.x,
        }
    }

    #[inline]
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    #[inline]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    #[inline]
    pub fn distance(self, rhs: Self) -> f32 {
        (self - rhs).length()
    }

    #[inline]
    pub fn distance_squared(self, rhs: Self) -> f32 {
        (self - rhs).length_squared()
    }

    #[inline]
    pub fn normalize(self) -> Self {
        let len = self.length();
        self / len
    }

    #[inline]
    pub fn normalize_or_zero(self) -> Self {
        let len = self.length();
        if len > 0.0 { self / len } else { Self::ZERO }
    }

    #[inline]
    pub fn lerp(self, rhs: Self, s: f32) -> Self {
        self + (rhs - self) * s
    }

    #[inline]
    pub fn min(self, rhs: Self) -> Self {
        Self {
            x: self.x.min(rhs.x),
            y: self.y.min(rhs.y),
            z: self.z.min(rhs.z),
        }
    }

    #[inline]
    pub fn max(self, rhs: Self) -> Self {
        Self {
            x: self.x.max(rhs.x),
            y: self.y.max(rhs.y),
            z: self.z.max(rhs.z),
        }
    }

    #[inline]
    pub fn clamp(self, min: Self, max: Self) -> Self {
        Self {
            x: self.x.clamp(min.x, max.x),
            y: self.y.clamp(min.y, max.y),
            z: self.z.clamp(min.z, max.z),
        }
    }

    #[inline]
    pub fn abs(self) -> Self {
        Self {
            x: self.x.abs(),
            y: self.y.abs(),
            z: self.z.abs(),
        }
    }

    #[inline]
    pub fn floor(self) -> Self {
        Self {
            x: self.x.floor(),
            y: self.y.floor(),
            z: self.z.floor(),
        }
    }

    #[inline]
    pub fn ceil(self) -> Self {
        Self {
            x: self.x.ceil(),
            y: self.y.ceil(),
            z: self.z.ceil(),
        }
    }

    #[inline]
    pub fn round(self) -> Self {
        Self {
            x: self.x.round(),
            y: self.y.round(),
            z: self.z.round(),
        }
    }
}

impl From<[f32; 3]> for Vec3 {
    #[inline]
    fn from(a: [f32; 3]) -> Self {
        Self::from_array(a)
    }
}

impl From<Vec3> for [f32; 3] {
    #[inline]
    fn from(v: Vec3) -> Self {
        v.to_array()
    }
}

impl From<(f32, f32, f32)> for Vec3 {
    #[inline]
    fn from((x, y, z): (f32, f32, f32)) -> Self {
        Self { x, y, z }
    }
}

impl From<Vec3> for (f32, f32, f32) {
    #[inline]
    fn from(v: Vec3) -> Self {
        (v.x, v.y, v.z)
    }
}

impl Add for Vec3 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
        }
    }
}

impl AddAssign for Vec3 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
        self.z += rhs.z;
    }
}

impl Sub for Vec3 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
        }
    }
}

impl SubAssign for Vec3 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
        self.z -= rhs.z;
    }
}

impl Mul<Self> for Vec3 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        Self {
            x: self.x * rhs.x,
            y: self.y * rhs.y,
            z: self.z * rhs.z,
        }
    }
}

impl Mul<f32> for Vec3 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: f32) -> Self {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
            z: self.z * rhs,
        }
    }
}

impl Mul<Vec3> for f32 {
    type Output = Vec3;
    #[inline]
    fn mul(self, rhs: Vec3) -> Vec3 {
        Vec3 {
            x: self * rhs.x,
            y: self * rhs.y,
            z: self * rhs.z,
        }
    }
}

impl MulAssign<Self> for Vec3 {
    #[inline]
    fn mul_assign(&mut self, rhs: Self) {
        self.x *= rhs.x;
        self.y *= rhs.y;
        self.z *= rhs.z;
    }
}

impl MulAssign<f32> for Vec3 {
    #[inline]
    fn mul_assign(&mut self, rhs: f32) {
        self.x *= rhs;
        self.y *= rhs;
        self.z *= rhs;
    }
}

impl Div<Self> for Vec3 {
    type Output = Self;
    #[inline]
    fn div(self, rhs: Self) -> Self {
        Self {
            x: self.x / rhs.x,
            y: self.y / rhs.y,
            z: self.z / rhs.z,
        }
    }
}

impl Div<f32> for Vec3 {
    type Output = Self;
    #[inline]
    fn div(self, rhs: f32) -> Self {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
            z: self.z / rhs,
        }
    }
}

impl DivAssign<Self> for Vec3 {
    #[inline]
    fn div_assign(&mut self, rhs: Self) {
        self.x /= rhs.x;
        self.y /= rhs.y;
        self.z /= rhs.z;
    }
}

impl DivAssign<f32> for Vec3 {
    #[inline]
    fn div_assign(&mut self, rhs: f32) {
        self.x /= rhs;
        self.y /= rhs;
        self.z /= rhs;
    }
}

impl Neg for Vec3 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }
}

// ============================================================================
// Vec4
// ============================================================================

/// 4-element 32-bit floating point vector.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Vec4 {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 0.0,
    };
    pub const ONE: Self = Self {
        x: 1.0,
        y: 1.0,
        z: 1.0,
        w: 1.0,
    };
    pub const NEG_ONE: Self = Self {
        x: -1.0,
        y: -1.0,
        z: -1.0,
        w: -1.0,
    };
    pub const MIN: Self = Self {
        x: f32::MIN,
        y: f32::MIN,
        z: f32::MIN,
        w: f32::MIN,
    };
    pub const MAX: Self = Self {
        x: f32::MAX,
        y: f32::MAX,
        z: f32::MAX,
        w: f32::MAX,
    };
    pub const NAN: Self = Self {
        x: f32::NAN,
        y: f32::NAN,
        z: f32::NAN,
        w: f32::NAN,
    };
    pub const INFINITY: Self = Self {
        x: f32::INFINITY,
        y: f32::INFINITY,
        z: f32::INFINITY,
        w: f32::INFINITY,
    };
    pub const NEG_INFINITY: Self = Self {
        x: f32::NEG_INFINITY,
        y: f32::NEG_INFINITY,
        z: f32::NEG_INFINITY,
        w: f32::NEG_INFINITY,
    };

    pub const X: Self = Self {
        x: 1.0,
        y: 0.0,
        z: 0.0,
        w: 0.0,
    };
    pub const Y: Self = Self {
        x: 0.0,
        y: 1.0,
        z: 0.0,
        w: 0.0,
    };
    pub const Z: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 1.0,
        w: 0.0,
    };
    pub const W: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };
    pub const NEG_X: Self = Self {
        x: -1.0,
        y: 0.0,
        z: 0.0,
        w: 0.0,
    };
    pub const NEG_Y: Self = Self {
        x: 0.0,
        y: -1.0,
        z: 0.0,
        w: 0.0,
    };
    pub const NEG_Z: Self = Self {
        x: 0.0,
        y: 0.0,
        z: -1.0,
        w: 0.0,
    };
    pub const NEG_W: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: -1.0,
    };

    pub const AXES: [Self; 4] = [Self::X, Self::Y, Self::Z, Self::W];

    #[inline]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    #[inline]
    pub const fn splat(v: f32) -> Self {
        Self {
            x: v,
            y: v,
            z: v,
            w: v,
        }
    }

    #[inline]
    pub const fn to_array(self) -> [f32; 4] {
        [self.x, self.y, self.z, self.w]
    }

    #[inline]
    pub const fn from_array(a: [f32; 4]) -> Self {
        Self {
            x: a[0],
            y: a[1],
            z: a[2],
            w: a[3],
        }
    }

    #[inline]
    pub const fn truncate(self) -> Vec3 {
        Vec3 {
            x: self.x,
            y: self.y,
            z: self.z,
        }
    }

    #[inline]
    pub const fn xyz(self) -> Vec3 {
        Vec3 {
            x: self.x,
            y: self.y,
            z: self.z,
        }
    }

    #[inline]
    pub const fn xy(self) -> Vec2 {
        Vec2 {
            x: self.x,
            y: self.y,
        }
    }

    #[inline]
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z + self.w * rhs.w
    }

    #[inline]
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    #[inline]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    #[inline]
    pub fn distance(self, rhs: Self) -> f32 {
        (self - rhs).length()
    }

    #[inline]
    pub fn distance_squared(self, rhs: Self) -> f32 {
        (self - rhs).length_squared()
    }

    #[inline]
    pub fn normalize(self) -> Self {
        let len = self.length();
        self / len
    }

    #[inline]
    pub fn normalize_or_zero(self) -> Self {
        let len = self.length();
        if len > 0.0 { self / len } else { Self::ZERO }
    }

    #[inline]
    pub fn lerp(self, rhs: Self, s: f32) -> Self {
        self + (rhs - self) * s
    }

    #[inline]
    pub fn min(self, rhs: Self) -> Self {
        Self {
            x: self.x.min(rhs.x),
            y: self.y.min(rhs.y),
            z: self.z.min(rhs.z),
            w: self.w.min(rhs.w),
        }
    }

    #[inline]
    pub fn max(self, rhs: Self) -> Self {
        Self {
            x: self.x.max(rhs.x),
            y: self.y.max(rhs.y),
            z: self.z.max(rhs.z),
            w: self.w.max(rhs.w),
        }
    }

    #[inline]
    pub fn clamp(self, min: Self, max: Self) -> Self {
        Self {
            x: self.x.clamp(min.x, max.x),
            y: self.y.clamp(min.y, max.y),
            z: self.z.clamp(min.z, max.z),
            w: self.w.clamp(min.w, max.w),
        }
    }

    #[inline]
    pub fn abs(self) -> Self {
        Self {
            x: self.x.abs(),
            y: self.y.abs(),
            z: self.z.abs(),
            w: self.w.abs(),
        }
    }

    #[inline]
    pub fn floor(self) -> Self {
        Self {
            x: self.x.floor(),
            y: self.y.floor(),
            z: self.z.floor(),
            w: self.w.floor(),
        }
    }

    #[inline]
    pub fn ceil(self) -> Self {
        Self {
            x: self.x.ceil(),
            y: self.y.ceil(),
            z: self.z.ceil(),
            w: self.w.ceil(),
        }
    }

    #[inline]
    pub fn round(self) -> Self {
        Self {
            x: self.x.round(),
            y: self.y.round(),
            z: self.z.round(),
            w: self.w.round(),
        }
    }
}

impl From<[f32; 4]> for Vec4 {
    #[inline]
    fn from(a: [f32; 4]) -> Self {
        Self::from_array(a)
    }
}

impl From<Vec4> for [f32; 4] {
    #[inline]
    fn from(v: Vec4) -> Self {
        v.to_array()
    }
}

impl From<(f32, f32, f32, f32)> for Vec4 {
    #[inline]
    fn from((x, y, z, w): (f32, f32, f32, f32)) -> Self {
        Self { x, y, z, w }
    }
}

impl From<Vec4> for (f32, f32, f32, f32) {
    #[inline]
    fn from(v: Vec4) -> Self {
        (v.x, v.y, v.z, v.w)
    }
}

impl Add for Vec4 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
            w: self.w + rhs.w,
        }
    }
}

impl AddAssign for Vec4 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
        self.z += rhs.z;
        self.w += rhs.w;
    }
}

impl Sub for Vec4 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
            w: self.w - rhs.w,
        }
    }
}

impl SubAssign for Vec4 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
        self.z -= rhs.z;
        self.w -= rhs.w;
    }
}

impl Mul<Self> for Vec4 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        Self {
            x: self.x * rhs.x,
            y: self.y * rhs.y,
            z: self.z * rhs.z,
            w: self.w * rhs.w,
        }
    }
}

impl Mul<f32> for Vec4 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: f32) -> Self {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
            z: self.z * rhs,
            w: self.w * rhs,
        }
    }
}

impl Mul<Vec4> for f32 {
    type Output = Vec4;
    #[inline]
    fn mul(self, rhs: Vec4) -> Vec4 {
        Vec4 {
            x: self * rhs.x,
            y: self * rhs.y,
            z: self * rhs.z,
            w: self * rhs.w,
        }
    }
}

impl MulAssign<Self> for Vec4 {
    #[inline]
    fn mul_assign(&mut self, rhs: Self) {
        self.x *= rhs.x;
        self.y *= rhs.y;
        self.z *= rhs.z;
        self.w *= rhs.w;
    }
}

impl MulAssign<f32> for Vec4 {
    #[inline]
    fn mul_assign(&mut self, rhs: f32) {
        self.x *= rhs;
        self.y *= rhs;
        self.z *= rhs;
        self.w *= rhs;
    }
}

impl Div<Self> for Vec4 {
    type Output = Self;
    #[inline]
    fn div(self, rhs: Self) -> Self {
        Self {
            x: self.x / rhs.x,
            y: self.y / rhs.y,
            z: self.z / rhs.z,
            w: self.w / rhs.w,
        }
    }
}

impl Div<f32> for Vec4 {
    type Output = Self;
    #[inline]
    fn div(self, rhs: f32) -> Self {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
            z: self.z / rhs,
            w: self.w / rhs,
        }
    }
}

impl DivAssign<Self> for Vec4 {
    #[inline]
    fn div_assign(&mut self, rhs: Self) {
        self.x /= rhs.x;
        self.y /= rhs.y;
        self.z /= rhs.z;
        self.w /= rhs.w;
    }
}

impl DivAssign<f32> for Vec4 {
    #[inline]
    fn div_assign(&mut self, rhs: f32) {
        self.x /= rhs;
        self.y /= rhs;
        self.z /= rhs;
        self.w /= rhs;
    }
}

impl Neg for Vec4 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
            z: -self.z,
            w: -self.w,
        }
    }
}

// ============================================================================
// UVec2
// ============================================================================

/// 2-element 32-bit unsigned integer vector.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct UVec2 {
    pub x: u32,
    pub y: u32,
}

impl UVec2 {
    pub const ZERO: Self = Self { x: 0, y: 0 };
    pub const ONE: Self = Self { x: 1, y: 1 };
    pub const MIN: Self = Self {
        x: u32::MIN,
        y: u32::MIN,
    };
    pub const MAX: Self = Self {
        x: u32::MAX,
        y: u32::MAX,
    };

    pub const X: Self = Self { x: 1, y: 0 };
    pub const Y: Self = Self { x: 0, y: 1 };

    pub const AXES: [Self; 2] = [Self::X, Self::Y];

    #[inline]
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }

    #[inline]
    pub const fn splat(v: u32) -> Self {
        Self { x: v, y: v }
    }

    #[inline]
    pub const fn to_array(self) -> [u32; 2] {
        [self.x, self.y]
    }

    #[inline]
    pub const fn from_array(a: [u32; 2]) -> Self {
        Self { x: a[0], y: a[1] }
    }

    #[inline]
    pub const fn as_vec2(&self) -> Vec2 {
        Vec2 {
            x: self.x as f32,
            y: self.y as f32,
        }
    }

    #[inline]
    pub const fn as_ivec2(&self) -> IVec2 {
        IVec2 {
            x: self.x as i32,
            y: self.y as i32,
        }
    }

    #[inline]
    pub fn min(self, rhs: Self) -> Self {
        Self {
            x: self.x.min(rhs.x),
            y: self.y.min(rhs.y),
        }
    }

    #[inline]
    pub fn max(self, rhs: Self) -> Self {
        Self {
            x: self.x.max(rhs.x),
            y: self.y.max(rhs.y),
        }
    }

    #[inline]
    pub fn clamp(self, min: Self, max: Self) -> Self {
        Self {
            x: self.x.clamp(min.x, max.x),
            y: self.y.clamp(min.y, max.y),
        }
    }
}

impl From<[u32; 2]> for UVec2 {
    #[inline]
    fn from(a: [u32; 2]) -> Self {
        Self::from_array(a)
    }
}

impl From<UVec2> for [u32; 2] {
    #[inline]
    fn from(v: UVec2) -> Self {
        v.to_array()
    }
}

impl From<(u32, u32)> for UVec2 {
    #[inline]
    fn from((x, y): (u32, u32)) -> Self {
        Self { x, y }
    }
}

impl From<UVec2> for (u32, u32) {
    #[inline]
    fn from(v: UVec2) -> Self {
        (v.x, v.y)
    }
}

impl Add for UVec2 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl AddAssign for UVec2 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl Sub for UVec2 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl SubAssign for UVec2 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}

impl Mul<Self> for UVec2 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        Self {
            x: self.x * rhs.x,
            y: self.y * rhs.y,
        }
    }
}

impl Mul<u32> for UVec2 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: u32) -> Self {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl Mul<UVec2> for u32 {
    type Output = UVec2;
    #[inline]
    fn mul(self, rhs: UVec2) -> UVec2 {
        UVec2 {
            x: self * rhs.x,
            y: self * rhs.y,
        }
    }
}

impl MulAssign<Self> for UVec2 {
    #[inline]
    fn mul_assign(&mut self, rhs: Self) {
        self.x *= rhs.x;
        self.y *= rhs.y;
    }
}

impl MulAssign<u32> for UVec2 {
    #[inline]
    fn mul_assign(&mut self, rhs: u32) {
        self.x *= rhs;
        self.y *= rhs;
    }
}

impl Div<Self> for UVec2 {
    type Output = Self;
    #[inline]
    fn div(self, rhs: Self) -> Self {
        Self {
            x: self.x / rhs.x,
            y: self.y / rhs.y,
        }
    }
}

impl Div<u32> for UVec2 {
    type Output = Self;
    #[inline]
    fn div(self, rhs: u32) -> Self {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

impl DivAssign<Self> for UVec2 {
    #[inline]
    fn div_assign(&mut self, rhs: Self) {
        self.x /= rhs.x;
        self.y /= rhs.y;
    }
}

impl DivAssign<u32> for UVec2 {
    #[inline]
    fn div_assign(&mut self, rhs: u32) {
        self.x /= rhs;
        self.y /= rhs;
    }
}

// ============================================================================
// IVec2
// ============================================================================

/// 2-element 32-bit signed integer vector.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct IVec2 {
    pub x: i32,
    pub y: i32,
}

impl IVec2 {
    pub const ZERO: Self = Self { x: 0, y: 0 };
    pub const ONE: Self = Self { x: 1, y: 1 };
    pub const NEG_ONE: Self = Self { x: -1, y: -1 };
    pub const MIN: Self = Self {
        x: i32::MIN,
        y: i32::MIN,
    };
    pub const MAX: Self = Self {
        x: i32::MAX,
        y: i32::MAX,
    };

    pub const X: Self = Self { x: 1, y: 0 };
    pub const Y: Self = Self { x: 0, y: 1 };
    pub const NEG_X: Self = Self { x: -1, y: 0 };
    pub const NEG_Y: Self = Self { x: 0, y: -1 };

    pub const AXES: [Self; 2] = [Self::X, Self::Y];

    #[inline]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    #[inline]
    pub const fn splat(v: i32) -> Self {
        Self { x: v, y: v }
    }

    #[inline]
    pub const fn to_array(self) -> [i32; 2] {
        [self.x, self.y]
    }

    #[inline]
    pub const fn from_array(a: [i32; 2]) -> Self {
        Self { x: a[0], y: a[1] }
    }

    #[inline]
    pub const fn as_vec2(&self) -> Vec2 {
        Vec2 {
            x: self.x as f32,
            y: self.y as f32,
        }
    }

    #[inline]
    pub const fn as_uvec2(&self) -> UVec2 {
        UVec2 {
            x: self.x as u32,
            y: self.y as u32,
        }
    }

    #[inline]
    pub fn dot(self, rhs: Self) -> i32 {
        self.x * rhs.x + self.y * rhs.y
    }

    #[inline]
    pub fn min(self, rhs: Self) -> Self {
        Self {
            x: self.x.min(rhs.x),
            y: self.y.min(rhs.y),
        }
    }

    #[inline]
    pub fn max(self, rhs: Self) -> Self {
        Self {
            x: self.x.max(rhs.x),
            y: self.y.max(rhs.y),
        }
    }

    #[inline]
    pub fn clamp(self, min: Self, max: Self) -> Self {
        Self {
            x: self.x.clamp(min.x, max.x),
            y: self.y.clamp(min.y, max.y),
        }
    }

    #[inline]
    pub fn abs(self) -> Self {
        Self {
            x: self.x.abs(),
            y: self.y.abs(),
        }
    }

    #[inline]
    pub const fn perp(self) -> Self {
        Self {
            x: -self.y,
            y: self.x,
        }
    }
}

impl From<[i32; 2]> for IVec2 {
    #[inline]
    fn from(a: [i32; 2]) -> Self {
        Self::from_array(a)
    }
}

impl From<IVec2> for [i32; 2] {
    #[inline]
    fn from(v: IVec2) -> Self {
        v.to_array()
    }
}

impl From<(i32, i32)> for IVec2 {
    #[inline]
    fn from((x, y): (i32, i32)) -> Self {
        Self { x, y }
    }
}

impl From<IVec2> for (i32, i32) {
    #[inline]
    fn from(v: IVec2) -> Self {
        (v.x, v.y)
    }
}

impl Add for IVec2 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl AddAssign for IVec2 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl Sub for IVec2 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl SubAssign for IVec2 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}

impl Mul<Self> for IVec2 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        Self {
            x: self.x * rhs.x,
            y: self.y * rhs.y,
        }
    }
}

impl Mul<i32> for IVec2 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: i32) -> Self {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl Mul<IVec2> for i32 {
    type Output = IVec2;
    #[inline]
    fn mul(self, rhs: IVec2) -> IVec2 {
        IVec2 {
            x: self * rhs.x,
            y: self * rhs.y,
        }
    }
}

impl MulAssign<Self> for IVec2 {
    #[inline]
    fn mul_assign(&mut self, rhs: Self) {
        self.x *= rhs.x;
        self.y *= rhs.y;
    }
}

impl MulAssign<i32> for IVec2 {
    #[inline]
    fn mul_assign(&mut self, rhs: i32) {
        self.x *= rhs;
        self.y *= rhs;
    }
}

impl Div<Self> for IVec2 {
    type Output = Self;
    #[inline]
    fn div(self, rhs: Self) -> Self {
        Self {
            x: self.x / rhs.x,
            y: self.y / rhs.y,
        }
    }
}

impl Div<i32> for IVec2 {
    type Output = Self;
    #[inline]
    fn div(self, rhs: i32) -> Self {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

impl DivAssign<Self> for IVec2 {
    #[inline]
    fn div_assign(&mut self, rhs: Self) {
        self.x /= rhs.x;
        self.y /= rhs.y;
    }
}

impl DivAssign<i32> for IVec2 {
    #[inline]
    fn div_assign(&mut self, rhs: i32) {
        self.x /= rhs;
        self.y /= rhs;
    }
}

impl Neg for IVec2 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
        }
    }
}

// ============================================================================
// Constructor helper functions matching glam::vec2, glam::uvec2, etc.
// ============================================================================

#[inline]
pub const fn vec2(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}

#[inline]
pub const fn vec3(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

#[inline]
pub const fn vec4(x: f32, y: f32, z: f32, w: f32) -> Vec4 {
    Vec4::new(x, y, z, w)
}

#[inline]
pub const fn uvec2(x: u32, y: u32) -> UVec2 {
    UVec2::new(x, y)
}

#[inline]
pub const fn ivec2(x: i32, y: i32) -> IVec2 {
    IVec2::new(x, y)
}

// ============================================================================
// Unit Tests (targeting 100% coverage)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vec2_constants_and_constructors() {
        assert_eq!(Vec2::ZERO, Vec2::new(0.0, 0.0));
        assert_eq!(Vec2::ONE, Vec2::new(1.0, 1.0));
        assert_eq!(Vec2::NEG_ONE, Vec2::new(-1.0, -1.0));
        assert_eq!(Vec2::MIN, Vec2::new(f32::MIN, f32::MIN));
        assert_eq!(Vec2::MAX, Vec2::new(f32::MAX, f32::MAX));
        assert!(Vec2::NAN.x.is_nan() && Vec2::NAN.y.is_nan());
        assert_eq!(Vec2::INFINITY, Vec2::new(f32::INFINITY, f32::INFINITY));
        assert_eq!(
            Vec2::NEG_INFINITY,
            Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY)
        );

        assert_eq!(Vec2::X, Vec2::new(1.0, 0.0));
        assert_eq!(Vec2::Y, Vec2::new(0.0, 1.0));
        assert_eq!(Vec2::NEG_X, Vec2::new(-1.0, 0.0));
        assert_eq!(Vec2::NEG_Y, Vec2::new(0.0, -1.0));
        assert_eq!(Vec2::AXES, [Vec2::X, Vec2::Y]);

        assert_eq!(Vec2::default(), Vec2::ZERO);
        assert_eq!(Vec2::splat(5.0), Vec2::new(5.0, 5.0));
        assert_eq!(vec2(3.0, 4.0), Vec2::new(3.0, 4.0));
    }

    #[test]
    fn test_vec2_conversions() {
        let v = Vec2::new(3.0, 4.0);
        assert_eq!(v.to_array(), [3.0, 4.0]);
        assert_eq!(Vec2::from_array([3.0, 4.0]), v);
        assert_eq!(Vec2::from([3.0, 4.0]), v);
        assert_eq!(<[f32; 2]>::from(v), [3.0, 4.0]);
        assert_eq!(Vec2::from((3.0, 4.0)), v);
        assert_eq!(<(f32, f32)>::from(v), (3.0, 4.0));

        assert_eq!(v.extend(5.0), Vec3::new(3.0, 4.0, 5.0));
        assert_eq!(v.as_uvec2(), UVec2::new(3, 4));
        assert_eq!(v.as_ivec2(), IVec2::new(3, 4));
        assert_eq!(v.as_dvec2(), (3.0f64, 4.0f64));
    }

    #[test]
    fn test_vec2_ops() {
        let a = Vec2::new(1.0, 2.0);
        let b = Vec2::new(3.0, 4.0);

        assert_eq!(a + b, Vec2::new(4.0, 6.0));
        let mut c = a;
        c += b;
        assert_eq!(c, Vec2::new(4.0, 6.0));

        assert_eq!(a - b, Vec2::new(-2.0, -2.0));
        let mut d = a;
        d -= b;
        assert_eq!(d, Vec2::new(-2.0, -2.0));

        assert_eq!(a * b, Vec2::new(3.0, 8.0));
        let mut m = a;
        m *= b;
        assert_eq!(m, Vec2::new(3.0, 8.0));

        assert_eq!(a * 2.0, Vec2::new(2.0, 4.0));
        assert_eq!(2.0 * a, Vec2::new(2.0, 4.0));
        let mut ms = a;
        ms *= 2.0;
        assert_eq!(ms, Vec2::new(2.0, 4.0));

        assert_eq!(b / a, Vec2::new(3.0, 2.0));
        let mut dv = b;
        dv /= a;
        assert_eq!(dv, Vec2::new(3.0, 2.0));

        assert_eq!(b / 2.0, Vec2::new(1.5, 2.0));
        let mut ds = b;
        ds /= 2.0;
        assert_eq!(ds, Vec2::new(1.5, 2.0));

        assert_eq!(-a, Vec2::new(-1.0, -2.0));
    }

    #[test]
    fn test_vec2_methods() {
        let v = Vec2::new(3.0, 4.0);
        assert_eq!(v.dot(Vec2::new(2.0, 1.0)), 10.0);
        assert_eq!(v.length_squared(), 25.0);
        assert_eq!(v.length(), 5.0);
        assert_eq!(v.distance(Vec2::new(6.0, 8.0)), 5.0);
        assert_eq!(v.distance_squared(Vec2::new(6.0, 8.0)), 25.0);

        assert_eq!(v.normalize(), Vec2::new(0.6, 0.8));
        assert_eq!(Vec2::ZERO.normalize_or_zero(), Vec2::ZERO);
        assert_eq!(v.normalize_or_zero(), Vec2::new(0.6, 0.8));

        assert_eq!(
            Vec2::ZERO.lerp(Vec2::new(10.0, 20.0), 0.5),
            Vec2::new(5.0, 10.0)
        );
        assert_eq!(
            Vec2::new(1.0, 5.0).min(Vec2::new(3.0, 2.0)),
            Vec2::new(1.0, 2.0)
        );
        assert_eq!(
            Vec2::new(1.0, 5.0).max(Vec2::new(3.0, 2.0)),
            Vec2::new(3.0, 5.0)
        );
        assert_eq!(
            Vec2::new(-2.0, 15.0).clamp(Vec2::ZERO, Vec2::new(10.0, 10.0)),
            Vec2::new(0.0, 10.0)
        );

        assert_eq!(Vec2::new(-1.5, -2.5).abs(), Vec2::new(1.5, 2.5));
        assert_eq!(Vec2::new(1.7, -1.2).floor(), Vec2::new(1.0, -2.0));
        assert_eq!(Vec2::new(1.2, -1.7).ceil(), Vec2::new(2.0, -1.0));
        assert_eq!(Vec2::new(1.2, 1.8).round(), Vec2::new(1.0, 2.0));

        assert_eq!(Vec2::new(1.0, 0.0).perp(), Vec2::new(0.0, 1.0));
        assert_eq!(Vec2::new(0.0, 1.0).perp(), Vec2::new(-1.0, 0.0));

        let angle = Vec2::X.angle_between(Vec2::Y);
        assert!((angle - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
    }

    #[test]
    fn test_vec3_constants_and_constructors() {
        assert_eq!(Vec3::ZERO, Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(Vec3::ONE, Vec3::new(1.0, 1.0, 1.0));
        assert_eq!(Vec3::NEG_ONE, Vec3::new(-1.0, -1.0, -1.0));
        assert_eq!(Vec3::MIN, Vec3::new(f32::MIN, f32::MIN, f32::MIN));
        assert_eq!(Vec3::MAX, Vec3::new(f32::MAX, f32::MAX, f32::MAX));
        assert!(Vec3::NAN.x.is_nan() && Vec3::NAN.y.is_nan() && Vec3::NAN.z.is_nan());
        assert_eq!(
            Vec3::INFINITY,
            Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY)
        );
        assert_eq!(
            Vec3::NEG_INFINITY,
            Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY)
        );

        assert_eq!(Vec3::X, Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(Vec3::Y, Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(Vec3::Z, Vec3::new(0.0, 0.0, 1.0));
        assert_eq!(Vec3::NEG_X, Vec3::new(-1.0, 0.0, 0.0));
        assert_eq!(Vec3::NEG_Y, Vec3::new(0.0, -1.0, 0.0));
        assert_eq!(Vec3::NEG_Z, Vec3::new(0.0, 0.0, -1.0));
        assert_eq!(Vec3::AXES, [Vec3::X, Vec3::Y, Vec3::Z]);

        assert_eq!(Vec3::default(), Vec3::ZERO);
        assert_eq!(Vec3::splat(5.0), Vec3::new(5.0, 5.0, 5.0));
        assert_eq!(vec3(3.0, 4.0, 5.0), Vec3::new(3.0, 4.0, 5.0));
    }

    #[test]
    fn test_vec3_conversions() {
        let v = Vec3::new(3.0, 4.0, 5.0);
        assert_eq!(v.to_array(), [3.0, 4.0, 5.0]);
        assert_eq!(Vec3::from_array([3.0, 4.0, 5.0]), v);
        assert_eq!(Vec3::from([3.0, 4.0, 5.0]), v);
        assert_eq!(<[f32; 3]>::from(v), [3.0, 4.0, 5.0]);
        assert_eq!(Vec3::from((3.0, 4.0, 5.0)), v);
        assert_eq!(<(f32, f32, f32)>::from(v), (3.0, 4.0, 5.0));

        assert_eq!(v.extend(6.0), Vec4::new(3.0, 4.0, 5.0, 6.0));
        assert_eq!(v.truncate(), Vec2::new(3.0, 4.0));
        assert_eq!(v.xy(), Vec2::new(3.0, 4.0));
    }

    #[test]
    fn test_vec3_ops() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);

        assert_eq!(a + b, Vec3::new(5.0, 7.0, 9.0));
        let mut c = a;
        c += b;
        assert_eq!(c, Vec3::new(5.0, 7.0, 9.0));

        assert_eq!(a - b, Vec3::new(-3.0, -3.0, -3.0));
        let mut d = a;
        d -= b;
        assert_eq!(d, Vec3::new(-3.0, -3.0, -3.0));

        assert_eq!(a * b, Vec3::new(4.0, 10.0, 18.0));
        let mut m = a;
        m *= b;
        assert_eq!(m, Vec3::new(4.0, 10.0, 18.0));

        assert_eq!(a * 2.0, Vec3::new(2.0, 4.0, 6.0));
        assert_eq!(2.0 * a, Vec3::new(2.0, 4.0, 6.0));
        let mut ms = a;
        ms *= 2.0;
        assert_eq!(ms, Vec3::new(2.0, 4.0, 6.0));

        assert_eq!(b / Vec3::new(2.0, 2.5, 3.0), Vec3::new(2.0, 2.0, 2.0));
        let mut dv = b;
        dv /= Vec3::new(2.0, 2.5, 3.0);
        assert_eq!(dv, Vec3::new(2.0, 2.0, 2.0));

        assert_eq!(b / 2.0, Vec3::new(2.0, 2.5, 3.0));
        let mut ds = b;
        ds /= 2.0;
        assert_eq!(ds, Vec3::new(2.0, 2.5, 3.0));

        assert_eq!(-a, Vec3::new(-1.0, -2.0, -3.0));
    }

    #[test]
    fn test_vec3_methods() {
        let v = Vec3::new(2.0, 3.0, 6.0);
        assert_eq!(v.dot(Vec3::new(1.0, 2.0, 3.0)), 2.0 + 6.0 + 18.0);
        assert_eq!(v.cross(Vec3::Z), Vec3::new(3.0, -2.0, 0.0));
        assert_eq!(v.length_squared(), 4.0 + 9.0 + 36.0);
        assert_eq!(v.length(), 7.0);
        assert_eq!(v.distance(Vec3::new(2.0, 3.0, 13.0)), 7.0);
        assert_eq!(v.distance_squared(Vec3::new(2.0, 3.0, 13.0)), 49.0);

        assert_eq!(v.normalize(), Vec3::new(2.0 / 7.0, 3.0 / 7.0, 6.0 / 7.0));
        assert_eq!(Vec3::ZERO.normalize_or_zero(), Vec3::ZERO);
        assert_eq!(
            v.normalize_or_zero(),
            Vec3::new(2.0 / 7.0, 3.0 / 7.0, 6.0 / 7.0)
        );

        assert_eq!(
            Vec3::ZERO.lerp(Vec3::new(10.0, 20.0, 30.0), 0.5),
            Vec3::new(5.0, 10.0, 15.0)
        );
        assert_eq!(
            Vec3::new(1.0, 5.0, 2.0).min(Vec3::new(3.0, 2.0, 4.0)),
            Vec3::new(1.0, 2.0, 2.0)
        );
        assert_eq!(
            Vec3::new(1.0, 5.0, 2.0).max(Vec3::new(3.0, 2.0, 4.0)),
            Vec3::new(3.0, 5.0, 4.0)
        );
        assert_eq!(
            Vec3::new(-2.0, 15.0, 5.0).clamp(Vec3::ZERO, Vec3::new(10.0, 10.0, 10.0)),
            Vec3::new(0.0, 10.0, 5.0)
        );

        assert_eq!(Vec3::new(-1.5, -2.5, -3.5).abs(), Vec3::new(1.5, 2.5, 3.5));
        assert_eq!(Vec3::new(1.7, -1.2, 0.5).floor(), Vec3::new(1.0, -2.0, 0.0));
        assert_eq!(Vec3::new(1.2, -1.7, 0.5).ceil(), Vec3::new(2.0, -1.0, 1.0));
        assert_eq!(Vec3::new(1.2, 1.8, -1.6).round(), Vec3::new(1.0, 2.0, -2.0));
    }

    #[test]
    fn test_vec4_constants_and_constructors() {
        assert_eq!(Vec4::ZERO, Vec4::new(0.0, 0.0, 0.0, 0.0));
        assert_eq!(Vec4::ONE, Vec4::new(1.0, 1.0, 1.0, 1.0));
        assert_eq!(Vec4::NEG_ONE, Vec4::new(-1.0, -1.0, -1.0, -1.0));
        assert_eq!(Vec4::MIN, Vec4::new(f32::MIN, f32::MIN, f32::MIN, f32::MIN));
        assert_eq!(Vec4::MAX, Vec4::new(f32::MAX, f32::MAX, f32::MAX, f32::MAX));
        assert!(
            Vec4::NAN.x.is_nan()
                && Vec4::NAN.y.is_nan()
                && Vec4::NAN.z.is_nan()
                && Vec4::NAN.w.is_nan()
        );
        assert_eq!(
            Vec4::INFINITY,
            Vec4::new(f32::INFINITY, f32::INFINITY, f32::INFINITY, f32::INFINITY)
        );
        assert_eq!(
            Vec4::NEG_INFINITY,
            Vec4::new(
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY
            )
        );

        assert_eq!(Vec4::X, Vec4::new(1.0, 0.0, 0.0, 0.0));
        assert_eq!(Vec4::Y, Vec4::new(0.0, 1.0, 0.0, 0.0));
        assert_eq!(Vec4::Z, Vec4::new(0.0, 0.0, 1.0, 0.0));
        assert_eq!(Vec4::W, Vec4::new(0.0, 0.0, 0.0, 1.0));
        assert_eq!(Vec4::NEG_X, Vec4::new(-1.0, 0.0, 0.0, 0.0));
        assert_eq!(Vec4::NEG_Y, Vec4::new(0.0, -1.0, 0.0, 0.0));
        assert_eq!(Vec4::NEG_Z, Vec4::new(0.0, 0.0, -1.0, 0.0));
        assert_eq!(Vec4::NEG_W, Vec4::new(0.0, 0.0, 0.0, -1.0));
        assert_eq!(Vec4::AXES, [Vec4::X, Vec4::Y, Vec4::Z, Vec4::W]);

        assert_eq!(Vec4::default(), Vec4::ZERO);
        assert_eq!(Vec4::splat(5.0), Vec4::new(5.0, 5.0, 5.0, 5.0));
        assert_eq!(vec4(3.0, 4.0, 5.0, 6.0), Vec4::new(3.0, 4.0, 5.0, 6.0));
    }

    #[test]
    fn test_vec4_conversions() {
        let v = Vec4::new(3.0, 4.0, 5.0, 6.0);
        assert_eq!(v.to_array(), [3.0, 4.0, 5.0, 6.0]);
        assert_eq!(Vec4::from_array([3.0, 4.0, 5.0, 6.0]), v);
        assert_eq!(Vec4::from([3.0, 4.0, 5.0, 6.0]), v);
        assert_eq!(<[f32; 4]>::from(v), [3.0, 4.0, 5.0, 6.0]);
        assert_eq!(Vec4::from((3.0, 4.0, 5.0, 6.0)), v);
        assert_eq!(<(f32, f32, f32, f32)>::from(v), (3.0, 4.0, 5.0, 6.0));

        assert_eq!(v.truncate(), Vec3::new(3.0, 4.0, 5.0));
        assert_eq!(v.xyz(), Vec3::new(3.0, 4.0, 5.0));
        assert_eq!(v.xy(), Vec2::new(3.0, 4.0));
    }

    #[test]
    fn test_vec4_ops() {
        let a = Vec4::new(1.0, 2.0, 3.0, 4.0);
        let b = Vec4::new(5.0, 6.0, 7.0, 8.0);

        assert_eq!(a + b, Vec4::new(6.0, 8.0, 10.0, 12.0));
        let mut c = a;
        c += b;
        assert_eq!(c, Vec4::new(6.0, 8.0, 10.0, 12.0));

        assert_eq!(a - b, Vec4::new(-4.0, -4.0, -4.0, -4.0));
        let mut d = a;
        d -= b;
        assert_eq!(d, Vec4::new(-4.0, -4.0, -4.0, -4.0));

        assert_eq!(a * b, Vec4::new(5.0, 12.0, 21.0, 32.0));
        let mut m = a;
        m *= b;
        assert_eq!(m, Vec4::new(5.0, 12.0, 21.0, 32.0));

        assert_eq!(a * 2.0, Vec4::new(2.0, 4.0, 6.0, 8.0));
        assert_eq!(2.0 * a, Vec4::new(2.0, 4.0, 6.0, 8.0));
        let mut ms = a;
        ms *= 2.0;
        assert_eq!(ms, Vec4::new(2.0, 4.0, 6.0, 8.0));

        assert_eq!(b / a, Vec4::new(5.0, 3.0, 7.0 / 3.0, 2.0));
        let mut dv = b;
        dv /= a;
        assert_eq!(dv, Vec4::new(5.0, 3.0, 7.0 / 3.0, 2.0));

        assert_eq!(b / 2.0, Vec4::new(2.5, 3.0, 3.5, 4.0));
        let mut ds = b;
        ds /= 2.0;
        assert_eq!(ds, Vec4::new(2.5, 3.0, 3.5, 4.0));

        assert_eq!(-a, Vec4::new(-1.0, -2.0, -3.0, -4.0));
    }

    #[test]
    fn test_vec4_methods() {
        let v = Vec4::new(1.0, 2.0, 3.0, 4.0);
        assert_eq!(v.dot(Vec4::ONE), 10.0);
        assert_eq!(v.length_squared(), 30.0);
        assert_eq!(v.length(), 30.0f32.sqrt());
        assert_eq!(v.distance(Vec4::ZERO), 30.0f32.sqrt());
        assert_eq!(v.distance_squared(Vec4::ZERO), 30.0);

        let n = v.normalize();
        assert!((n.length() - 1.0).abs() < 1e-6);
        assert_eq!(Vec4::ZERO.normalize_or_zero(), Vec4::ZERO);
        assert!((v.normalize_or_zero().length() - 1.0).abs() < 1e-6);

        assert_eq!(Vec4::ZERO.lerp(Vec4::splat(10.0), 0.5), Vec4::splat(5.0));
        assert_eq!(
            Vec4::new(1.0, 5.0, 2.0, 8.0).min(Vec4::new(3.0, 2.0, 4.0, 6.0)),
            Vec4::new(1.0, 2.0, 2.0, 6.0)
        );
        assert_eq!(
            Vec4::new(1.0, 5.0, 2.0, 8.0).max(Vec4::new(3.0, 2.0, 4.0, 6.0)),
            Vec4::new(3.0, 5.0, 4.0, 8.0)
        );
        assert_eq!(
            Vec4::new(-2.0, 15.0, 5.0, 12.0).clamp(Vec4::ZERO, Vec4::splat(10.0)),
            Vec4::new(0.0, 10.0, 5.0, 10.0)
        );

        assert_eq!(
            Vec4::new(-1.5, -2.5, -3.5, -4.5).abs(),
            Vec4::new(1.5, 2.5, 3.5, 4.5)
        );
        assert_eq!(
            Vec4::new(1.7, -1.2, 0.5, 2.1).floor(),
            Vec4::new(1.0, -2.0, 0.0, 2.0)
        );
        assert_eq!(
            Vec4::new(1.2, -1.7, 0.5, 2.1).ceil(),
            Vec4::new(2.0, -1.0, 1.0, 3.0)
        );
        assert_eq!(
            Vec4::new(1.2, 1.8, -1.6, 2.5).round(),
            Vec4::new(1.0, 2.0, -2.0, 3.0)
        );
    }

    #[test]
    fn test_uvec2() {
        assert_eq!(UVec2::ZERO, UVec2::new(0, 0));
        assert_eq!(UVec2::ONE, UVec2::new(1, 1));
        assert_eq!(UVec2::MIN, UVec2::new(u32::MIN, u32::MIN));
        assert_eq!(UVec2::MAX, UVec2::new(u32::MAX, u32::MAX));
        assert_eq!(UVec2::X, UVec2::new(1, 0));
        assert_eq!(UVec2::Y, UVec2::new(0, 1));
        assert_eq!(UVec2::AXES, [UVec2::X, UVec2::Y]);
        assert_eq!(UVec2::default(), UVec2::ZERO);
        assert_eq!(UVec2::splat(5), UVec2::new(5, 5));
        assert_eq!(uvec2(3, 4), UVec2::new(3, 4));

        let v = UVec2::new(3, 4);
        assert_eq!(v.to_array(), [3, 4]);
        assert_eq!(UVec2::from_array([3, 4]), v);
        assert_eq!(UVec2::from([3, 4]), v);
        assert_eq!(<[u32; 2]>::from(v), [3, 4]);
        assert_eq!(UVec2::from((3, 4)), v);
        assert_eq!(<(u32, u32)>::from(v), (3, 4));

        assert_eq!(v.as_vec2(), Vec2::new(3.0, 4.0));
        assert_eq!(v.as_ivec2(), IVec2::new(3, 4));

        let a = UVec2::new(2, 4);
        let b = UVec2::new(6, 8);
        assert_eq!(a + b, UVec2::new(8, 12));
        let mut c = a;
        c += b;
        assert_eq!(c, UVec2::new(8, 12));
        assert_eq!(b - a, UVec2::new(4, 4));
        let mut d = b;
        d -= a;
        assert_eq!(d, UVec2::new(4, 4));
        assert_eq!(a * b, UVec2::new(12, 32));
        let mut m = a;
        m *= b;
        assert_eq!(m, UVec2::new(12, 32));
        assert_eq!(a * 3, UVec2::new(6, 12));
        assert_eq!(3 * a, UVec2::new(6, 12));
        let mut ms = a;
        ms *= 3;
        assert_eq!(ms, UVec2::new(6, 12));
        assert_eq!(b / a, UVec2::new(3, 2));
        let mut dv = b;
        dv /= a;
        assert_eq!(dv, UVec2::new(3, 2));
        assert_eq!(b / 2, UVec2::new(3, 4));
        let mut ds = b;
        ds /= 2;
        assert_eq!(ds, UVec2::new(3, 4));

        assert_eq!(UVec2::new(1, 5).min(UVec2::new(3, 2)), UVec2::new(1, 2));
        assert_eq!(UVec2::new(1, 5).max(UVec2::new(3, 2)), UVec2::new(3, 5));
        assert_eq!(
            UVec2::new(1, 15).clamp(UVec2::splat(2), UVec2::splat(10)),
            UVec2::new(2, 10)
        );
    }

    #[test]
    fn test_ivec2() {
        assert_eq!(IVec2::ZERO, IVec2::new(0, 0));
        assert_eq!(IVec2::ONE, IVec2::new(1, 1));
        assert_eq!(IVec2::NEG_ONE, IVec2::new(-1, -1));
        assert_eq!(IVec2::MIN, IVec2::new(i32::MIN, i32::MIN));
        assert_eq!(IVec2::MAX, IVec2::new(i32::MAX, i32::MAX));
        assert_eq!(IVec2::X, IVec2::new(1, 0));
        assert_eq!(IVec2::Y, IVec2::new(0, 1));
        assert_eq!(IVec2::NEG_X, IVec2::new(-1, 0));
        assert_eq!(IVec2::NEG_Y, IVec2::new(0, -1));
        assert_eq!(IVec2::AXES, [IVec2::X, IVec2::Y]);
        assert_eq!(IVec2::default(), IVec2::ZERO);
        assert_eq!(IVec2::splat(5), IVec2::new(5, 5));
        assert_eq!(ivec2(3, 4), IVec2::new(3, 4));

        let v = IVec2::new(3, 4);
        assert_eq!(v.to_array(), [3, 4]);
        assert_eq!(IVec2::from_array([3, 4]), v);
        assert_eq!(IVec2::from([3, 4]), v);
        assert_eq!(<[i32; 2]>::from(v), [3, 4]);
        assert_eq!(IVec2::from((3, 4)), v);
        assert_eq!(<(i32, i32)>::from(v), (3, 4));

        assert_eq!(v.as_vec2(), Vec2::new(3.0, 4.0));
        assert_eq!(v.as_uvec2(), UVec2::new(3, 4));
        assert_eq!(v.dot(IVec2::new(2, 1)), 10);
        assert_eq!(v.perp(), IVec2::new(-4, 3));

        let a = IVec2::new(2, -4);
        let b = IVec2::new(6, 8);
        assert_eq!(a + b, IVec2::new(8, 4));
        let mut c = a;
        c += b;
        assert_eq!(c, IVec2::new(8, 4));
        assert_eq!(b - a, IVec2::new(4, 12));
        let mut d = b;
        d -= a;
        assert_eq!(d, IVec2::new(4, 12));
        assert_eq!(a * b, IVec2::new(12, -32));
        let mut m = a;
        m *= b;
        assert_eq!(m, IVec2::new(12, -32));
        assert_eq!(a * 3, IVec2::new(6, -12));
        assert_eq!(3 * a, IVec2::new(6, -12));
        let mut ms = a;
        ms *= 3;
        assert_eq!(ms, IVec2::new(6, -12));
        assert_eq!(b / IVec2::new(2, 4), IVec2::new(3, 2));
        let mut dv = b;
        dv /= IVec2::new(2, 4);
        assert_eq!(dv, IVec2::new(3, 2));
        assert_eq!(b / 2, IVec2::new(3, 4));
        let mut ds = b;
        ds /= 2;
        assert_eq!(ds, IVec2::new(3, 4));
        assert_eq!(-a, IVec2::new(-2, 4));

        assert_eq!(a.abs(), IVec2::new(2, 4));
        assert_eq!(IVec2::new(1, 5).min(IVec2::new(3, 2)), IVec2::new(1, 2));
        assert_eq!(IVec2::new(1, 5).max(IVec2::new(3, 2)), IVec2::new(3, 5));
        assert_eq!(
            IVec2::new(-1, 15).clamp(IVec2::ZERO, IVec2::splat(10)),
            IVec2::new(0, 10)
        );
    }
}
