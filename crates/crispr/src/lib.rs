//! CRISPR mechanic: Cas-variant table, PAM finding (rust-bio), `Score` traits + in-core default impls,
//! and gated edit application (SPEC §4, §8 Stage 1; TAXONOMY.md §3).
//!
//! Stage 0 placeholder — this crate intentionally has no behaviour yet. It exists so the workspace is
//! whole and `crates/genome` is a confirmed dependency. The Cas-variant table will be **data**
//! (`data/cas_variants.ron`), not code (invariant: loci/kinds are data — SPEC §4).

#![forbid(unsafe_code)]

#[cfg(test)]
mod tests {
    #[test]
    fn crate_links_against_genome() {
        // Confirms the dependency edge compiles; real behaviour arrives in Stage 1.
        let g = genome::sample_genome();
        assert!(g.is_valid());
    }
}
