//! A small, documented, deterministic generator: `SplitMix64` expands a seed into the
//! state of `xoshiro256**`. Both are public-domain algorithms by Vigna and Blackman; the
//! test vectors below are the published ones, so the stream is identical on every
//! platform and every build.

/// A deterministic 64-bit generator (`xoshiro256**`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng {
    state: [u64; 4],
}

/// One step of `SplitMix64`.
#[must_use]
pub fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Rng {
    /// Seeds the generator; equal seeds give equal streams.
    #[must_use]
    pub fn from_seed(seed: u64) -> Self {
        let mut mix = seed;
        let state = [
            splitmix64(&mut mix),
            splitmix64(&mut mix),
            splitmix64(&mut mix),
            splitmix64(&mut mix),
        ];
        Self { state }
    }

    /// Starts from an explicit state (for the published test vectors).
    #[must_use]
    pub const fn from_state(state: [u64; 4]) -> Self {
        Self { state }
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        let result = self.state[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.state[1] << 17;
        self.state[2] ^= self.state[0];
        self.state[3] ^= self.state[1];
        self.state[1] ^= self.state[2];
        self.state[0] ^= self.state[3];
        self.state[2] ^= t;
        self.state[3] = self.state[3].rotate_left(45);
        result
    }

    /// A value in `0..bound` (`0` when `bound` is `0`), by multiply-and-shift.
    pub fn below(&mut self, bound: u64) -> u64 {
        let wide = u128::from(self.next_u64()) * u128::from(bound);
        u64::try_from(wide >> 64).unwrap_or(u64::MAX)
    }

    /// A value in `lo..=hi` (`lo` when `hi < lo`).
    pub fn range(&mut self, lo: u64, hi: u64) -> u64 {
        if hi <= lo {
            return lo;
        }
        let span = hi - lo;
        if span == u64::MAX {
            return self.next_u64();
        }
        lo + self.below(span + 1)
    }

    /// An index in `0..len` (`0` when `len` is `0`).
    pub fn index(&mut self, len: usize) -> usize {
        usize::try_from(self.below(u64::try_from(len).unwrap_or(u64::MAX))).unwrap_or(usize::MAX)
    }

    /// True with probability `numerator / denominator`.
    pub fn chance(&mut self, numerator: u64, denominator: u64) -> bool {
        self.below(denominator) < numerator
    }

    /// One of `items`, or `None` when there are none.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            return None;
        }
        let index = self.index(items.len());
        items.get(index)
    }

    /// `len` random bytes.
    pub fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len)
            .map(|_| u8::try_from(self.below(256)).unwrap_or(u8::MAX))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{splitmix64, Rng};

    #[test]
    fn splitmix64_matches_the_published_vector_for_seed_zero() {
        let mut state = 0;
        let got: Vec<u64> = (0..4).map(|_| splitmix64(&mut state)).collect();
        assert_eq!(
            got,
            [
                0xE220_A839_7B1D_CDAF,
                0x6E78_9E6A_A1B9_65F4,
                0x06C4_5D18_8009_454F,
                0xF88B_B8A8_724C_81EC
            ]
        );
    }

    #[test]
    fn xoshiro_matches_the_published_vector_for_state_1_2_3_4() {
        let mut rng = Rng::from_state([1, 2, 3, 4]);
        let got: Vec<u64> = (0..10).map(|_| rng.next_u64()).collect();
        assert_eq!(
            got,
            [
                11_520,
                0,
                1_509_978_240,
                1_215_971_899_390_074_240,
                1_216_172_134_540_287_360,
                607_988_272_756_665_600,
                16_172_922_978_634_559_625,
                8_476_171_486_693_032_832,
                10_595_114_339_597_558_777,
                2_904_607_092_377_533_576,
            ]
        );
    }

    #[test]
    fn seeding_is_deterministic_and_seeds_differ() {
        let a: Vec<u64> = (0..8)
            .scan(Rng::from_seed(42), |r, _| Some(r.next_u64()))
            .collect();
        let b: Vec<u64> = (0..8)
            .scan(Rng::from_seed(42), |r, _| Some(r.next_u64()))
            .collect();
        let c: Vec<u64> = (0..8)
            .scan(Rng::from_seed(43), |r, _| Some(r.next_u64()))
            .collect();
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(Rng::from_seed(0), Rng::from_state(Rng::from_seed(0).state));
    }

    #[test]
    fn below_range_index_and_chance_stay_in_bounds_and_cover_them() {
        let mut rng = Rng::from_seed(7);
        assert_eq!(rng.below(0), 0);
        assert_eq!(rng.below(1), 0);
        let mut seen = [false; 5];
        for _ in 0..500 {
            let v = rng.below(5);
            assert!(v < 5);
            seen[usize::try_from(v).expect("small")] = true;
        }
        assert!(seen.iter().all(|&s| s), "every value below 5 appears");
        for _ in 0..200 {
            let v = rng.range(10, 12);
            assert!((10..=12).contains(&v));
        }
        assert_eq!(rng.range(5, 5), 5);
        assert_eq!(rng.range(9, 3), 9, "an empty range yields its low end");
        assert!(rng.range(u64::MAX - 1, u64::MAX) >= u64::MAX - 1);
        let full: Vec<u64> = (0..50).map(|_| rng.range(0, u64::MAX)).collect();
        assert!(
            full.iter().any(|&v| v > u64::MAX / 2) && full.iter().any(|&v| v < u64::MAX / 2),
            "the full range"
        );
        assert_eq!(rng.index(0), 0);
        for _ in 0..100 {
            assert!(rng.index(3) < 3);
        }
        assert!(!rng.chance(0, 10), "never");
        assert!(rng.chance(10, 10), "always");
        let heads = (0..1000).filter(|_| rng.chance(1, 2)).count();
        assert!((400..=600).contains(&heads), "{heads}");
    }

    #[test]
    fn pick_and_bytes() {
        let mut rng = Rng::from_seed(1);
        let empty: [u8; 0] = [];
        assert_eq!(rng.pick(&empty), None);
        assert_eq!(rng.pick(&["only"]), Some(&"only"));
        let items = ["a", "b", "c"];
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..100 {
            seen.insert(*rng.pick(&items).expect("non-empty"));
        }
        assert_eq!(seen.len(), 3);
        assert_eq!(rng.bytes(0), Vec::<u8>::new());
        let bytes = rng.bytes(1000);
        assert_eq!(bytes.len(), 1000);
        assert!(bytes.iter().any(|&b| b > 200) && bytes.iter().any(|&b| b < 50));
    }
}
