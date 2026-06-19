//! Deterministic seed derivation (invariant #3, SNIPPETS.md).
//!
//! One master seed derives every sub-seed (sim-core RNG now; SLiM `-seed` in Stage 2). The same scheme
//! is used for the harness's per-run seeds, so a batch of runs is fully reproducible from one master seed.

/// Derive a sub-seed from a master seed and a stream index via a splitmix64 step.
///
/// Deterministic, well-distributed, and stateless — `derive_seed(m, i)` is stable across runs/builds.
#[must_use]
pub fn derive_seed(master: u64, stream: u64) -> u64 {
    // Mix the stream into the master via an odd-constant multiply (a bijection mod 2^64, so distinct
    // streams give distinct offsets — including 0 vs 1), then a full splitmix64 finalizer.
    let mut z = master.wrapping_add(stream.wrapping_add(1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_distinct() {
        assert_eq!(derive_seed(42, 0), derive_seed(42, 0));
        assert_ne!(derive_seed(42, 0), derive_seed(42, 1));
        assert_ne!(derive_seed(1, 0), derive_seed(2, 0));
    }

    #[test]
    fn no_collisions_across_consecutive_streams() {
        // Guards the `stream | 1` bug class: 0..256 streams off one master must all differ.
        let mut seen = std::collections::HashSet::new();
        for i in 0..256u64 {
            assert!(
                seen.insert(derive_seed(42, i)),
                "seed collision at stream {i}"
            );
        }
    }
}
