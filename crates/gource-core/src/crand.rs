//! A replica of glibc's `rand()`.
//!
//! The C++ physics nudge overlapping directories and users apart with
//! `rand()`, which it never seeds, so every C++ run uses glibc's default
//! sequence. Producing the same numbers in the same order keeps Rust runs
//! deterministic and frame-for-frame comparable with the C++ binary.

/// glibc's default `random()` generator (`TYPE_3`: additive feedback with the
/// trinomial x^31 + x^3 + 1), as used by `rand()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CRand {
    state: [i32; DEGREE],
    front: usize,
    rear: usize,
}

const DEGREE: usize = 31;
const SEPARATION: usize = 3;

impl CRand {
    /// The largest value [`CRand::rand`] returns (C `RAND_MAX`).
    pub const RAND_MAX: i32 = i32::MAX;

    /// C `srand(seed)`. C programs that never call `srand` use seed 1.
    pub fn new(seed: u32) -> Self {
        // glibc replaces a zero seed with 1.
        let seed = seed.max(1);
        let mut state = [0i32; DEGREE];
        state[0] = seed as i32;
        let mut word = i64::from(seed);
        for slot in state.iter_mut().skip(1) {
            // 16807 * word % 2147483647 without overflowing 31 bits.
            let hi = word / 127_773;
            let lo = word % 127_773;
            word = 16_807 * lo - 2_836 * hi;
            if word < 0 {
                word += 2_147_483_647;
            }
            *slot = word as i32;
        }
        let mut rng = Self {
            state,
            front: SEPARATION,
            rear: 0,
        };
        for _ in 0..DEGREE * 10 {
            rng.rand();
        }
        rng
    }

    /// C `rand()`: the next value in `0..=RAND_MAX`.
    pub fn rand(&mut self) -> i32 {
        let value = (self.state[self.front] as u32).wrapping_add(self.state[self.rear] as u32);
        self.state[self.front] = value as i32;
        self.front += 1;
        self.rear += 1;
        if self.front >= DEGREE {
            self.front = 0;
        } else if self.rear >= DEGREE {
            self.rear = 0;
        }
        // The least random bit is dropped.
        (value >> 1) as i32
    }
}

impl Default for CRand {
    /// The sequence of a C program that never calls `srand`.
    fn default() -> Self {
        Self::new(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_sequence_matches_glibc() {
        let mut rng = CRand::default();
        let first: Vec<i32> = (0..10).map(|_| rng.rand()).collect();
        assert_eq!(
            first,
            [
                1_804_289_383,
                846_930_886,
                1_681_692_777,
                1_714_636_915,
                1_957_747_793,
                424_238_335,
                719_885_386,
                1_649_760_492,
                596_516_649,
                1_189_641_421,
            ]
        );
    }

    #[test]
    fn zero_seed_is_seed_one() {
        let mut zero = CRand::new(0);
        let mut one = CRand::new(1);
        for _ in 0..100 {
            assert_eq!(zero.rand(), one.rand());
        }
    }

    #[test]
    fn values_stay_in_range_over_many_wraps() {
        let mut rng = CRand::new(12345);
        for _ in 0..10_000 {
            let v = rng.rand();
            assert!((0..=CRand::RAND_MAX).contains(&v));
        }
    }
}
