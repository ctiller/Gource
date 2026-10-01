//! A 64-bit state hash for determinism checks (native vs wasm, server vs
//! client lockstep, replay verification).

use crate::kernel::fixed::IVec2;

/// Order-sensitive FNV-1a style hasher over integer words.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct StateHasher(u64);

impl Default for StateHasher {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl StateHasher {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn write_u64(&mut self, v: u64) {
        for b in v.to_le_bytes() {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    #[inline]
    pub fn write_i32(&mut self, v: i32) {
        self.write_u64(v as u32 as u64);
    }

    #[inline]
    pub fn write_vec(&mut self, v: IVec2) {
        self.write_i32(v.x);
        self.write_i32(v.y);
    }

    pub fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_sensitive() {
        let mut a = StateHasher::new();
        a.write_vec(IVec2::new(1, 2));
        let mut b = StateHasher::new();
        b.write_vec(IVec2::new(2, 1));
        assert_ne!(a.finish(), b.finish());
        let mut c = StateHasher::new();
        c.write_i32(1);
        c.write_i32(2);
        assert_eq!(a.finish(), c.finish());
        c.write_u64(9);
        assert_ne!(a.finish(), c.finish());
    }
}
