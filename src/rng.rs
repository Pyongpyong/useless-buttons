//! A tiny, dependency-free, deterministic PRNG (xorshift64*).
//!
//! We deliberately avoid the `rand` crate: it pulls in a fair amount of code
//! and, more importantly, indirection that bloats the wasm binary for very
//! little benefit here. All we need is fast, seedable, reproducible noise.

/// xorshift64* generator. Not cryptographically secure, not even close —
/// it just needs to be fast and to produce well-distributed bits for
/// visual noise.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    /// Build a generator from a 32-bit seed. The seed is mixed so that
    /// `seed = 0` (or other low-entropy seeds) still produces a healthy
    /// internal state — xorshift's all-zero state is a fixed point and
    /// must never be reached.
    pub fn new(seed: u32) -> Self {
        let mut state = (seed as u64) ^ 0x9E37_79B9_7F4A_7C15;
        state = state.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        state ^= state >> 27;
        if state == 0 {
            state = 0xD1B5_4A32_D192_ED03;
        }
        Rng(state)
    }

    /// Next raw 64-bit output.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Next raw 32-bit output.
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Uniform float in `[0, 1)`.
    pub fn next_f32(&mut self) -> f32 {
        // Take the top 24 bits so the result is exactly representable and
        // uniformly distributed across f32 mantissa precision.
        let bits = self.next_u64() >> 40;
        (bits as f32) / ((1u64 << 24) as f32)
    }

    /// Uniform float in `[lo, hi)`. If `hi <= lo` this returns `lo`.
    pub fn range_f32(&mut self, lo: f32, hi: f32) -> f32 {
        if hi <= lo {
            return lo;
        }
        lo + self.next_f32() * (hi - lo)
    }

    /// Uniform integer in `[lo, hi)`. If `hi <= lo` this returns `lo`.
    pub fn range_i32(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        let span = (hi - lo) as f32;
        lo + (self.next_f32() * span) as i32
    }

    /// Uniform integer in `[lo, hi)` for usize ranges (grid indices, etc).
    pub fn range_usize(&mut self, lo: usize, hi: usize) -> usize {
        if hi <= lo {
            return lo;
        }
        let span = (hi - lo) as f32;
        lo + (self.next_f32() * span) as usize
    }

    /// `true` with probability `p` (clamped to `[0, 1]`).
    pub fn bool_p(&mut self, p: f32) -> bool {
        self.next_f32() < p.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_for_same_seed() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn zero_seed_does_not_degenerate() {
        let mut rng = Rng::new(0);
        let mut seen_nonzero = false;
        for _ in 0..64 {
            if rng.next_u64() != 0 {
                seen_nonzero = true;
            }
        }
        assert!(seen_nonzero, "seed 0 must not produce a stuck-at-zero stream");
    }

    #[test]
    fn next_f32_in_unit_range() {
        let mut rng = Rng::new(7);
        for _ in 0..10_000 {
            let v = rng.next_f32();
            assert!((0.0..1.0).contains(&v), "value {v} out of [0,1)");
        }
    }

    #[test]
    fn range_f32_respects_bounds() {
        let mut rng = Rng::new(99);
        for _ in 0..10_000 {
            let v = rng.range_f32(-5.0, 5.0);
            assert!((-5.0..5.0).contains(&v));
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(2);
        let sa: Vec<u64> = (0..8).map(|_| a.next_u64()).collect();
        let sb: Vec<u64> = (0..8).map(|_| b.next_u64()).collect();
        assert_ne!(sa, sb);
    }
}
