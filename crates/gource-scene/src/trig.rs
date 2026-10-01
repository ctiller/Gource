//! Const-generated sine table and angle helpers.
//!
//! Angles are `u16` fractions of a full turn (65536 = 360°). The table is
//! computed at compile time with integer-only Taylor series, so it is the
//! same on every target.

use crate::fixed::{IVec2, UNIT};

/// Table entries per full turn.
pub const TABLE_LEN: usize = 4096;

/// Pi in Q60.
const PI_Q60: i128 = 3_622_009_729_038_561_421;

/// `sin(x)` for `x` in Q60 radians, `|x| <= pi/2`, as Q60.
const fn sin_q60(x: i128) -> i128 {
    let x2 = (x * x) >> 60;
    let mut term = x;
    let mut sum = x;
    let mut k = 1;
    while k <= 10 {
        term = -((term * x2) >> 60) / ((2 * k) * (2 * k + 1));
        sum += term;
        k += 1;
    }
    sum
}

const fn build_table() -> [i32; TABLE_LEN] {
    let mut t = [0i32; TABLE_LEN];
    let quarter = TABLE_LEN / 4;
    let mut i = 0;
    while i <= quarter {
        // x = (i / quarter) * pi/2
        let x = PI_Q60 * (i as i128) / (2 * quarter as i128);
        let s = sin_q60(x);
        // Round Q60 -> Q14.
        let v = ((s + (1i128 << 45)) >> 46) as i32;
        t[i] = v;
        if i > 0 && i < quarter {
            t[2 * quarter - i] = v;
        }
        if i == quarter {
            t[quarter] = v;
        }
        i += 1;
    }
    // Second half is the negated first half.
    let mut j = 1;
    while j < 2 * quarter {
        t[2 * quarter + j] = -t[j];
        j += 1;
    }
    t[2 * quarter] = 0;
    t
}

/// `sin` over a full turn, [`UNIT`]-scaled.
pub static SIN: [i32; TABLE_LEN] = build_table();

/// `(sin, cos)` of `angle` (65536 = full turn), [`UNIT`]-scaled.
#[inline]
pub fn sin_cos(angle: u16) -> (i32, i32) {
    let i = (angle as usize) >> 4;
    let c = (i + TABLE_LEN / 4) & (TABLE_LEN - 1);
    (SIN[i], SIN[c])
}

/// The unit direction at `angle`: `(cos, sin)`, [`UNIT`]-scaled.
#[inline]
pub fn direction(angle: u16) -> IVec2 {
    let (s, c) = sin_cos(angle);
    IVec2::new(c, s)
}

/// The angle (65536 = full turn) of `num / den` of a turn, rounded.
#[inline]
pub fn turn_fraction(num: u64, den: u64) -> u16 {
    if den == 0 {
        return 0;
    }
    ((num * 65536 + den / 2) / den) as u16
}

/// Golden angle, in 65536ths of a turn.
pub const GOLDEN_ANGLE: u16 = 25_042;

#[allow(dead_code)]
const _: () = assert!(UNIT == 16384);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_landmarks() {
        assert_eq!(sin_cos(0), (0, UNIT));
        assert_eq!(sin_cos(16384), (UNIT, 0));
        assert_eq!(sin_cos(32768), (0, -UNIT));
        assert_eq!(sin_cos(49152), (-UNIT, 0));
        // sin 45° = cos 45° = 0.70710678
        let (s, c) = sin_cos(8192);
        assert_eq!((s, c), (11585, 11585));
        // sin 30° (table index 341.33 -> 341, 29.97°)
        let (s, _) = sin_cos(5461);
        assert!((s - 8184).abs() <= 1, "{s}");
    }

    #[test]
    fn table_is_unit_length() {
        for a in (0..=u16::MAX).step_by(97) {
            let d = direction(a);
            let l = d.len_sq();
            let u = (UNIT as i64) * (UNIT as i64);
            assert!((l - u).abs() < 2 * UNIT as i64, "angle {a}: {l} vs {u}");
        }
    }

    #[test]
    fn fractions() {
        assert_eq!(turn_fraction(1, 4), 16384);
        assert_eq!(turn_fraction(1, 2), 32768);
        assert_eq!(turn_fraction(3, 0), 0);
        assert_eq!(direction(GOLDEN_ANGLE).length() / 100, UNIT / 100);
    }

    #[test]
    fn runtime_build_matches_const_table() {
        // The table is built in const context; rebuilding it at runtime
        // checks the builder is pure (and covers it).
        let built = std::hint::black_box(build_table as fn() -> [i32; TABLE_LEN])();
        assert_eq!(built, SIN);
    }
}
