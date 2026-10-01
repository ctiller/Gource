//! Fixed-point scalars and vectors.
//!
//! - Positions, lengths and radii: [`Fx`], Q24.8 (`ONE` = 1 world unit).
//! - Unit directions: [`IVec2`] scaled to [`UNIT`] (Q2.14).
//! - Areas: `i64` in Q16 (world units squared times 65536).
//! - Rates (`accel` in the C++ sense, units per second): Q24.8 per second;
//!   one tick moves `rate / TICK_HZ`.

use std::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

/// Fractional bits of [`Fx`].
pub const FRAC: u32 = 8;
/// One world unit.
pub const ONE: Fx = 1 << FRAC;
/// Fractional bits of unit directions.
pub const UNIT_FRAC: u32 = 14;
/// Length of a unit direction.
pub const UNIT: i32 = 1 << UNIT_FRAC;
/// Simulation ticks per second.
pub const TICK_HZ: i32 = 60;
/// Pi in Q16.
pub const PI_Q16: i64 = 205_887;

/// A Q24.8 scalar.
pub type Fx = i32;

/// `n / d` rounded to nearest, ties away from zero. `d` must be non-zero.
#[inline]
pub const fn div_round(n: i64, d: i64) -> i64 {
    let (n, d) = if d < 0 { (-n, -d) } else { (n, d) };
    if n >= 0 {
        (n + d / 2) / d
    } else {
        -((-n + d / 2) / d)
    }
}

/// Floor of the square root.
#[inline]
pub const fn isqrt(v: u64) -> u64 {
    if v < 2 {
        return v;
    }
    // Newton from an over-estimate; converges monotonically.
    let shift = (64 - v.leading_zeros()).div_ceil(2);
    let mut x = 1u64 << shift;
    loop {
        let y = (x + v / x) / 2;
        if y >= x {
            return x;
        }
        x = y;
    }
}

/// Square root of a non-negative Q16 area as a Q8 length.
#[inline]
pub fn sqrt_area(area_q16: i64) -> Fx {
    isqrt(area_q16.max(0) as u64).min(i32::MAX as u64) as Fx
}

/// Multiply two Q8 values (rounded).
#[inline]
pub fn mul_fx(a: Fx, b: Fx) -> Fx {
    div_round(a as i64 * b as i64, ONE as i64) as Fx
}

/// Saturate an `i64` into an `i32`.
#[inline]
pub fn sat(v: i64) -> i32 {
    v.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

/// Advance a value by `rate` (units per second) over one tick.
#[inline]
pub fn per_tick(rate: i64) -> i64 {
    div_round(rate, TICK_HZ as i64)
}

/// A 2D integer vector. Its scale depends on use: Q8 positions/rates or
/// [`UNIT`]-scaled directions.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IVec2 {
    pub x: i32,
    pub y: i32,
}

impl IVec2 {
    pub const ZERO: IVec2 = IVec2 { x: 0, y: 0 };

    #[inline]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Both components times `k`.
    #[inline]
    pub const fn splat(k: i32) -> Self {
        Self { x: k, y: k }
    }

    #[inline]
    pub fn len_sq(self) -> i64 {
        let (x, y) = (self.x as i64, self.y as i64);
        x * x + y * y
    }

    /// Length (floor), same scale as the components.
    #[inline]
    pub fn length(self) -> i32 {
        isqrt(self.len_sq() as u64).min(i32::MAX as u64) as i32
    }

    #[inline]
    pub fn dot(self, o: IVec2) -> i64 {
        self.x as i64 * o.x as i64 + self.y as i64 * o.y as i64
    }

    /// `self * num / den`, rounded per component (saturating).
    #[inline]
    pub fn scale(self, num: i64, den: i64) -> IVec2 {
        IVec2::new(
            sat(div_round(self.x as i64 * num, den)),
            sat(div_round(self.y as i64 * num, den)),
        )
    }

    /// The direction of `self` scaled to `len` (any scale), or `None` for
    /// the zero vector. Uses the exact length (`i64` division), so it is
    /// accurate for short vectors too.
    #[inline]
    pub fn with_len(self, len: i64) -> Option<IVec2> {
        let l = self.len_sq();
        if l == 0 {
            return None;
        }
        // Scale up first so the integer square root keeps precision.
        let (x, y) = (self.x as i64, self.y as i64);
        let big = x.unsigned_abs().max(y.unsigned_abs());
        // Pre-shift so |v| is about 2^24 before the sqrt.
        let shift = 24i32 - (64 - big.leading_zeros()) as i32;
        let (sx, sy) = if shift > 0 {
            (x << shift, y << shift)
        } else {
            (x >> -shift, y >> -shift)
        };
        let sl = isqrt((sx * sx + sy * sy) as u64) as i64;
        if sl == 0 {
            return None;
        }
        Some(IVec2::new(
            sat(div_round(sx * len, sl)),
            sat(div_round(sy * len, sl)),
        ))
    }

    /// The [`UNIT`]-scaled direction of `self`, or `None` for zero.
    #[inline]
    pub fn unit(self) -> Option<IVec2> {
        self.with_len(UNIT as i64)
    }

    /// A [`UNIT`]-scaled direction times a Q8 magnitude, giving Q8.
    #[inline]
    pub fn unit_times(self, mag: i64) -> IVec2 {
        self.scale(mag, UNIT as i64)
    }

    /// Clamp the length to at most `max` (same scale).
    #[inline]
    pub fn clamp_len(self, max: i32) -> IVec2 {
        if self.len_sq() > (max as i64) * (max as i64) {
            self.with_len(max as i64).unwrap_or(IVec2::ZERO)
        } else {
            self
        }
    }

    #[inline]
    pub fn min(self, o: IVec2) -> IVec2 {
        IVec2::new(self.x.min(o.x), self.y.min(o.y))
    }

    #[inline]
    pub fn max(self, o: IVec2) -> IVec2 {
        IVec2::new(self.x.max(o.x), self.y.max(o.y))
    }

    /// Advance `self` (a position) by `rate` (per second) over one tick.
    #[inline]
    pub fn step(self, rate: IVec2) -> IVec2 {
        IVec2::new(
            sat(self.x as i64 + per_tick(rate.x as i64)),
            sat(self.y as i64 + per_tick(rate.y as i64)),
        )
    }
}

impl Add for IVec2 {
    type Output = IVec2;
    #[inline]
    fn add(self, o: IVec2) -> IVec2 {
        IVec2::new(self.x.saturating_add(o.x), self.y.saturating_add(o.y))
    }
}

impl Sub for IVec2 {
    type Output = IVec2;
    #[inline]
    fn sub(self, o: IVec2) -> IVec2 {
        IVec2::new(self.x.saturating_sub(o.x), self.y.saturating_sub(o.y))
    }
}

impl Neg for IVec2 {
    type Output = IVec2;
    #[inline]
    fn neg(self) -> IVec2 {
        IVec2::new(self.x.saturating_neg(), self.y.saturating_neg())
    }
}

impl AddAssign for IVec2 {
    #[inline]
    fn add_assign(&mut self, o: IVec2) {
        *self = *self + o;
    }
}

impl SubAssign for IVec2 {
    #[inline]
    fn sub_assign(&mut self, o: IVec2) {
        *self = *self - o;
    }
}

impl Mul<i32> for IVec2 {
    type Output = IVec2;
    #[inline]
    fn mul(self, k: i32) -> IVec2 {
        IVec2::new(self.x.saturating_mul(k), self.y.saturating_mul(k))
    }
}

/// An `i64` accumulator for summing many [`IVec2`] contributions without
/// overflow; the sum is order-independent.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Acc {
    pub x: i64,
    pub y: i64,
}

impl Acc {
    #[inline]
    pub fn add(&mut self, v: IVec2) {
        self.x += v.x as i64;
        self.y += v.y as i64;
    }

    #[inline]
    pub fn sub(&mut self, v: IVec2) {
        self.x -= v.x as i64;
        self.y -= v.y as i64;
    }

    #[inline]
    pub fn get(self) -> IVec2 {
        IVec2::new(sat(self.x), sat(self.y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_division() {
        assert_eq!(div_round(5, 2), 3);
        assert_eq!(div_round(-5, 2), -3);
        assert_eq!(div_round(4, 3), 1);
        assert_eq!(div_round(-4, 3), -1);
        assert_eq!(div_round(5, -2), -3);
        assert_eq!(div_round(0, 7), 0);
    }

    #[test]
    fn integer_sqrt() {
        for v in [0u64, 1, 2, 3, 4, 15, 16, 17, 99, 100, 1 << 40, u64::MAX] {
            let r = isqrt(v);
            assert!(r.checked_mul(r).is_some_and(|sq| sq <= v));
            assert!((r + 1).checked_mul(r + 1).is_none_or(|sq| sq > v));
        }
        assert_eq!(sqrt_area(ONE as i64 * ONE as i64 * 9), 3 * ONE);
        assert_eq!(sqrt_area(-5), 0);
    }

    #[test]
    fn vector_ops() {
        let a = IVec2::new(3 * ONE, 4 * ONE);
        assert_eq!(a.length(), 5 * ONE);
        assert_eq!(a.len_sq(), 25 * (ONE as i64).pow(2));
        assert_eq!(a.dot(IVec2::new(1, 1)), 7 * ONE as i64);
        let u = a.unit().unwrap();
        assert_eq!(u, IVec2::new(UNIT * 3 / 5, UNIT * 4 / 5));
        assert!(IVec2::ZERO.unit().is_none());
        // Tiny vectors still get an accurate direction.
        let t = IVec2::new(1, 0).unit().unwrap();
        assert_eq!(t, IVec2::new(UNIT, 0));
        assert_eq!(a.clamp_len(ONE).length(), ONE);
        assert_eq!(a.clamp_len(10 * ONE), a);
        assert_eq!(u.unit_times(5 * ONE as i64), IVec2::new(3 * ONE, 4 * ONE));
        assert_eq!(a + a - a, a);
        assert_eq!(-a, IVec2::new(-3 * ONE, -4 * ONE));
        assert_eq!(a * 2, IVec2::new(6 * ONE, 8 * ONE));
        assert_eq!(a.scale(1, 2), IVec2::new(384, 512));
        assert_eq!(a.min(IVec2::ZERO), IVec2::ZERO);
        assert_eq!(a.max(IVec2::ZERO), a);
        assert_eq!(IVec2::splat(2), IVec2::new(2, 2));
        let mut b = a;
        b += a;
        b -= a;
        assert_eq!(b, a);
        // One tick at 60 units/s moves one unit.
        assert_eq!(
            IVec2::ZERO.step(IVec2::new(60 * ONE, -60 * ONE)),
            IVec2::new(ONE, -ONE)
        );
        assert_eq!(mul_fx(2 * ONE, 3 * ONE), 6 * ONE);
        assert_eq!(sat(i64::MAX), i32::MAX);
        let mut acc = Acc::default();
        acc.add(a);
        acc.add(a);
        acc.sub(a);
        assert_eq!(acc.get(), a);
    }
}
