//! A small seeded generator, so a seed gives the same placement on every
//! platform and build: SplitMix64 to spread the seed, xoshiro256** to draw.
//! Both are public-domain algorithms (Vigna), written here from their
//! published descriptions.

pub struct Rng([u64; 4]);

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut x = seed;
        let mut next = || {
            x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        Rng([next(), next(), next(), next()])
    }

    pub fn next_u64(&mut self) -> u64 {
        let s = &mut self.0;
        let out = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        out
    }

    /// A number in `0..n`, without the bias of a plain remainder.
    pub fn below(&mut self, n: usize) -> usize {
        assert!(n > 0, "below(0)");
        let n = n as u64;
        let zone = u64::MAX - u64::MAX % n;
        loop {
            let v = self.next_u64();
            if v < zone {
                return (v % n) as usize;
            }
        }
    }

    /// Fisher-Yates.
    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            v.swap(i, self.below(i + 1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seed_gives_the_same_numbers_every_time() {
        let a: Vec<u64> = {
            let mut r = Rng::new(42);
            (0..4).map(|_| r.next_u64()).collect()
        };
        let mut r = Rng::new(42);
        assert_eq!(a, (0..4).map(|_| r.next_u64()).collect::<Vec<_>>());
        // Pinned to values worked out separately from the published
        // algorithms, so a change to the generator is noticed: it would
        // change every seed's placement.
        assert_eq!(a[..2], [0x1578_0b2e_0c2e_c716, 0x6104_d986_6d11_3a7e]);
        assert_ne!(Rng::new(42).next_u64(), Rng::new(43).next_u64());
    }

    #[test]
    fn below_stays_in_range_and_covers_it() {
        let mut r = Rng::new(7);
        let mut seen = [0; 5];
        for _ in 0..1000 {
            seen[r.below(5)] += 1;
        }
        assert!(seen.iter().all(|&n| n > 100), "{seen:?}");
    }
}
