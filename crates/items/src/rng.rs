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

    /// An index drawn with chance in proportion to its weight; `None` when
    /// every weight is zero.
    pub fn weighted(&mut self, weights: &[u64]) -> Option<usize> {
        let total: u64 = weights.iter().sum();
        if total == 0 {
            return None;
        }
        let mut at = self.below_u64(total);
        for (i, &w) in weights.iter().enumerate() {
            if at < w {
                return Some(i);
            }
            at -= w;
        }
        unreachable!("the draw is below the total")
    }

    fn below_u64(&mut self, n: u64) -> u64 {
        let zone = u64::MAX - u64::MAX % n;
        loop {
            let v = self.next_u64();
            if v < zone {
                return v % n;
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

/// e^x from plain arithmetic, so a weight is the same number on every
/// platform: halve x until it is small, sum the series, square back up.
/// Basic IEEE operations are exact everywhere; library `exp` need not be.
pub fn exp(x: f64) -> f64 {
    let mut halvings = 0;
    let mut y = x;
    while y.abs() > 0.5 {
        y /= 2.0;
        halvings += 1;
    }
    let (mut sum, mut term) = (1.0, 1.0);
    for n in 1..=16 {
        term *= y / f64::from(n);
        sum += term;
    }
    for _ in 0..halvings {
        sum *= sum;
    }
    sum
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
    fn exp_matches_the_library_closely() {
        for x in [-10.0, -3.3, -0.4, 0.0, 0.25, 1.0, 2.5, 8.0] {
            let (ours, lib) = (exp(x), f64::exp(x));
            assert!((ours - lib).abs() <= lib * 1e-12, "{x}: {ours} vs {lib}");
        }
    }

    #[test]
    fn weighted_draws_follow_the_weights() {
        let mut r = Rng::new(3);
        let mut seen = [0; 3];
        for _ in 0..3000 {
            seen[r.weighted(&[1, 0, 2]).unwrap()] += 1;
        }
        assert_eq!(seen[1], 0);
        assert!(seen[2] > seen[0] * 3 / 2, "{seen:?}");
        assert_eq!(r.weighted(&[0, 0]), None);
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
