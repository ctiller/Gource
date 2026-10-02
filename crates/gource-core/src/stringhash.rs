//! Deterministic string hashing used for colours and initial positions
//! (port of `core/stringhash.cpp`).
//!
//! The exact arithmetic of the C++ implementation is preserved (signed chars,
//! XOR with the remaining length, wrapping 32-bit integer arithmetic) so that
//! file colours and directory layouts match the original program.

use crate::math::normalise3;
use crate::vec::{Vec2, Vec3};

/// Default hash seed (`gStringHashSeed` in the C++ code).
pub const DEFAULT_SEED: i32 = 31;

/// String hasher with a configurable seed. The seed can be changed at runtime
/// (the `s` key picks a random one) which recolours everything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StringHasher {
    pub seed: i32,
}

impl Default for StringHasher {
    fn default() -> Self {
        Self { seed: DEFAULT_SEED }
    }
}

impl StringHasher {
    pub const fn new(seed: i32) -> Self {
        Self { seed }
    }

    /// `stringHash`: `val += (signed char)str[i] * (seed ^ (n - i))`, then abs.
    pub fn hash(&self, s: &str) -> i32 {
        let bytes = s.as_bytes();
        let n = bytes.len() as i32;
        let mut val: i32 = 0;
        for (i, &b) in bytes.iter().enumerate() {
            let c = b as i8 as i32;
            val = val.wrapping_add(c.wrapping_mul(self.seed ^ n.wrapping_sub(i as i32)));
        }
        if val < 0 {
            val = val.wrapping_neg();
        }
        val
    }

    /// `vec2Hash`: a unit direction derived from the hash.
    pub fn vec2_hash(&self, s: &str) -> Vec2 {
        let hash = self.hash(s);
        let x = (hash / 7) % 255 - 127;
        let y = (hash / 3) % 255 - 127;
        let v = Vec2::new(x as f32, y as f32);
        let l = v.length();
        if l > 0.0 { v / l } else { v }
    }

    /// `vec3Hash`.
    pub fn vec3_hash(&self, s: &str) -> Vec3 {
        let hash = self.hash(s);
        let x = (hash / 7) % 255 - 127;
        let y = (hash / 3) % 255 - 127;
        let z = hash % 255;
        normalise3(Vec3::new(x as f32, y as f32, z as f32))
    }

    /// `colourHash`: a normalised RGB colour derived from the hash.
    pub fn colour_hash(&self, s: &str) -> Vec3 {
        Vec3::from(self.colour_rgb(s))
    }

    /// `colourHash` as plain `[r, g, b]` (no glam): the same f32 arithmetic
    /// as `normalise3` (`sqrt(r*r + g*g + b*b)`, then divide).
    pub fn colour_rgb(&self, s: &str) -> [f32; 3] {
        let mut hash = self.hash(s);
        if hash == 0 {
            hash += 1;
        }
        let v = [
            ((hash / 7) % 255) as f32,
            ((hash / 3) % 255) as f32,
            (hash % 255) as f32,
        ];
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if l > 0.0 {
            [v[0] / l, v[1] / l, v[2] / l]
        } else {
            v
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_rgb_matches_colour_hash_bits() {
        let h = StringHasher::default();
        for s in ["", "rs", "cpp", "Makefile", "a", "zzzzzz"] {
            let a = h.colour_rgb(s);
            let b = h.colour_hash(s);
            let expect = normalise3(Vec3::new(
                ((h.hash(s).max(1) / 7) % 255) as f32,
                ((h.hash(s).max(1) / 3) % 255) as f32,
                (h.hash(s).max(1) % 255) as f32,
            ));
            assert_eq!(a.map(f32::to_bits), b.to_array().map(f32::to_bits));
            assert_eq!(b, expect, "{s}");
        }
    }

    #[test]
    fn empty_string_hashes_to_zero() {
        assert_eq!(StringHasher::default().hash(""), 0);
    }

    #[test]
    fn matches_reference_arithmetic() {
        // "ab": n=2 -> 'a'*(31^2) + 'b'*(31^1) = 97*29 + 98*30 = 2813 + 2940
        assert_eq!(StringHasher::default().hash("ab"), 97 * 29 + 98 * 30);
    }

    #[test]
    fn colour_is_normalised() {
        let c = StringHasher::default().colour_hash("rs");
        assert!((c.length() - 1.0).abs() < 1e-5);
        let z = StringHasher::default().colour_hash("");
        assert!((z.length() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn signed_chars_and_wrapping() {
        // Non-ASCII bytes are negative when treated as signed chars.
        let h = StringHasher::default();
        let v = h.hash("é");
        assert!(v >= 0 || v == i32::MIN);
        // Long strings must not panic on overflow.
        let long = "x".repeat(100_000);
        let _ = h.hash(&long);
    }

    #[test]
    fn matches_cpp_stringhash_goldens() {
        let golden_data = include_str!("../tests/data/stringhash_golden.txt");
        for line in golden_data.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            let seed: i32 = parts[0].parse().unwrap();
            let s = parts[1];
            let expected_h: i32 = parts[2].parse().unwrap();

            let v2_parts: Vec<f32> = parts[3]
                .split_whitespace()
                .map(|x| x.parse().unwrap())
                .collect();
            let expected_v2 = Vec2::new(v2_parts[0], v2_parts[1]);

            let v3_parts: Vec<f32> = parts[4]
                .split_whitespace()
                .map(|x| x.parse().unwrap())
                .collect();
            let expected_v3 = Vec3::new(v3_parts[0], v3_parts[1], v3_parts[2]);

            let col_parts: Vec<f32> = parts[5]
                .split_whitespace()
                .map(|x| x.parse().unwrap())
                .collect();
            let expected_col = Vec3::new(col_parts[0], col_parts[1], col_parts[2]);

            let hasher = StringHasher::new(seed);
            assert_eq!(
                hasher.hash(s),
                expected_h,
                "hash mismatch for s={:?} seed={}",
                s,
                seed
            );

            let v2 = hasher.vec2_hash(s);
            assert!(
                (v2 - expected_v2).length() < 1e-5,
                "vec2 mismatch: {:?} vs {:?}",
                v2,
                expected_v2
            );

            let v3 = hasher.vec3_hash(s);
            assert!(
                (v3 - expected_v3).length() < 1e-5,
                "vec3 mismatch: {:?} vs {:?}",
                v3,
                expected_v3
            );

            let col = hasher.colour_hash(s);
            assert!(
                (col - expected_col).length() < 1e-5,
                "col mismatch: {:?} vs {:?}",
                col,
                expected_col
            );
        }
    }
}
