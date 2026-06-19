//! Property invariant: across arbitrary seeds and run lengths, two runs of the same config produce the
//! identical hash (the determinism contract, SPEC §6/§10.3). Runs under `cargo test --features proptest`.
#![cfg(feature = "proptest")]

use proptest::prelude::*;
use sim_core::{derive_seed, run_headless, SimConfig};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn same_config_same_hash(master in any::<u64>(), stream in 0u64..64, gens in 0u64..40, n in 0u32..400) {
        let cfg = SimConfig { seed: derive_seed(master, stream), generations: gens, entity_count: n };
        prop_assert_eq!(run_headless(&cfg).hash, run_headless(&cfg).hash);
    }
}
