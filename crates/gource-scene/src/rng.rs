//! Counter-based randomness: every draw is a pure function of
//! `(seed, tick, entity, salt)`, so there is no shared RNG state and the
//! order in which entities are processed never matters.

use crate::fixed::IVec2;
use crate::trig;

/// Salts that separate independent draws for the same entity and tick.
pub mod salt {
    pub const DIR_COINCIDENT: u64 = 1;
    pub const USER_COINCIDENT: u64 = 2;
    pub const USER_ACTION: u64 = 3;
    pub const FILE_PLACE: u64 = 4;
    pub const DIR_PLACE: u64 = 5;
}

#[inline]
fn mix(mut z: u64) -> u64 {
    // splitmix64 finaliser
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// A 64-bit hash of the key.
#[inline]
pub fn hash4(seed: u64, tick: u64, entity: u64, salt: u64) -> u64 {
    let mut h = mix(seed ^ 0x9e37_79b9_7f4a_7c15);
    h = mix(h ^ tick);
    h = mix(h ^ entity.wrapping_mul(0x9e37_79b9_7f4a_7c15));
    mix(h ^ salt)
}

/// A pseudo-random [`UNIT`](crate::UNIT)-scaled direction.
#[inline]
pub fn random_dir(seed: u64, tick: u64, entity: u64, salt: u64) -> IVec2 {
    trig::direction(hash4(seed, tick, entity, salt) as u16)
}

/// For a pair, a direction from `a` to `b` used when they coincide. It is
/// antisymmetric (`pair_dir(a, b) == -pair_dir(b, a)`), so equal and
/// opposite pushes cancel in sums.
#[inline]
pub fn pair_dir(seed: u64, tick: u64, a: u64, b: u64, salt: u64) -> IVec2 {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let d = random_dir(seed, tick, lo.wrapping_mul(0x1_0000_0001) ^ hi, salt);
    if a <= b { d } else { -d }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UNIT;

    #[test]
    fn deterministic_and_keyed() {
        assert_eq!(hash4(1, 2, 3, 4), hash4(1, 2, 3, 4));
        assert_ne!(hash4(1, 2, 3, 4), hash4(1, 2, 3, 5));
        assert_ne!(hash4(1, 2, 3, 4), hash4(1, 3, 3, 4));
        assert_ne!(hash4(1, 2, 3, 4), hash4(2, 2, 3, 4));
        let d = random_dir(7, 8, 9, 10);
        assert!((d.length() - UNIT).abs() < 4);
    }

    #[test]
    fn pair_dirs_are_antisymmetric() {
        let a = pair_dir(1, 2, 10, 20, 0);
        let b = pair_dir(1, 2, 20, 10, 0);
        assert_eq!(a, -b);
    }

    #[test]
    fn directions_spread() {
        let mut quadrants = [0; 4];
        for e in 0..400 {
            let d = random_dir(0, 0, e, 0);
            quadrants[((d.x >= 0) as usize) * 2 + (d.y >= 0) as usize] += 1;
        }
        assert!(quadrants.iter().all(|&q| q > 60), "{quadrants:?}");
    }
}
