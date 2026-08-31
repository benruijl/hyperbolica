/// Stable, small deterministic generator for Euler specializations.
///
/// HyperFLINT's contract requires caller-seeded generic points, not a
/// cryptographic stream.  SplitMix64 gives a fully specified cross-platform
/// sequence and avoids depending on a process-global or thread-local RNG.
#[derive(Clone, Debug)]
pub(crate) struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    pub(crate) const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    /// Uniform draw from the inclusive range, without modulo bias.
    pub(crate) fn inclusive(&mut self, lower: u32, upper: u32) -> u32 {
        debug_assert!(lower <= upper);
        let width = u64::from(upper) - u64::from(lower) + 1;
        let zone = u64::MAX - u64::MAX % width;
        loop {
            let value = self.next_u64();
            if value < zone {
                return lower + (value % width) as u32;
            }
        }
    }

    pub(crate) fn shuffle<T>(&mut self, values: &mut [T]) {
        for upper in (1..values.len()).rev() {
            let index = self.inclusive(0, upper as u32) as usize;
            values.swap(upper, index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_stream_and_shuffle_are_reproducible() {
        let mut left = DeterministicRng::new(87178);
        let mut right = DeterministicRng::new(87178);
        assert_eq!(
            (0..20).map(|_| left.next_u64()).collect::<Vec<_>>(),
            (0..20).map(|_| right.next_u64()).collect::<Vec<_>>()
        );
        let mut a = (0..32).collect::<Vec<_>>();
        let mut b = a.clone();
        left.shuffle(&mut a);
        right.shuffle(&mut b);
        assert_eq!(a, b);
    }
}
