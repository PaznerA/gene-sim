//! Relations index — the VIEW-ONLY nearest-species + guild-clustering boundary crate (ADR-014 re-grounded).
//!
//! **Invariant #1 (STOP THE LINE):** this is a PROCESS-BOUNDARY crate. It depends on **nothing at all**
//! (std-only) and links no GPL code. The license gate (`scripts/check_license.sh`, `relations-index` in
//! `BOUNDARY_CRATES`) enforces the dependency-free tree. The default in-Rust path (exact integer-L1 k-NN +
//! single-link union-find) needs only std; the sqlite-vec scale path stays BEHIND the boundary as a
//! subprocess / loadable-extension sidecar (a run-namespaced `.db` the sim core never opens), resolved via
//! `$RELDB_BIN → ~/.local/bin/relations-index → PATH` — exactly like `oracle-slim`/`oracle-fba` resolve their
//! CLIs — so it is never a linked crate.
//!
//! **Determinism (invariant #3):** every query is EXACT integer math — no float, no transcendental, no RNG,
//! no `HashMap` iteration in answer logic. At the shipped cardinality (S in the low tens) brute-force
//! `O(S^2)` integer-L1 over `u16[D]` is exact, instant, and bit-reproducible — it has ZERO of the ANN
//! insertion-order/float-ordering nondeterminism inv #3 forbids. sqlite-vec's approximate-recall value only
//! materializes at thousands of vectors (the future E. coli edit-variant fan-out), not today.
//!
//! **One-way view sink:** the index consumes the off-hash `species_signatures()` export and emits ordered
//! integer results that flow ONLY into the renderer (godot-sim → the Relations overlay). Nothing here ever
//! re-enters `selection()`/`metabolism()`/`hash_world` — by structure (this crate is downstream of the
//! deterministic core, which never calls it).

#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

/// The bare CLI name used when neither `RELDB_BIN` nor the pinned location resolves (PATH lookup). Reserved for
/// the future sqlite-vec scale path; the in-Rust default path spawns nothing.
const RELDB_CLI_NAME: &str = "relations-index";

/// The PINNED single-link guild threshold `T` (ADR-014): an integer-L1 distance `<= T` is an edge in the
/// single-link agglomerative clustering. A display-scaling choice (like the heatmap's max-abs scaling), NOT
/// biology — load-bearing for cross-run guild stability, so it is a const here + recorded in DECISIONS.md. At
/// the shipped `u16[12]` shared-grid layout, plant-likes cluster apart from decomposer-likes (and future
/// predator-likes) at this value.
pub const GUILD_THRESHOLD: u64 = 240_000;

/// Error returned by the relations index. **Mirrors `oracle_fba::FbaError` / `oracle_slim::SlimError`**
/// variant-for-variant so all three boundary crates share one failure surface (inv #5). The in-Rust default
/// path never errors (it is pure integer math over an in-memory slice); `Spawn`/`NonZeroExit` are RESERVED for
/// the sqlite-vec sidecar.
#[derive(Debug)]
pub enum RelError {
    /// An I/O failure preparing the (future) `.db` sidecar / work directory.
    Io(io::Error),
    /// The sidecar process could not be spawned (reserved for the sqlite-vec scale path).
    Spawn { bin: PathBuf, source: io::Error },
    /// The sidecar ran but exited non-zero (reserved for the sqlite-vec scale path). `stderr` is its captured err.
    NonZeroExit { status: String, stderr: String },
    /// The lookup produced no usable result (mirrors the sibling crates' `MissingOutput` — "ran but produced
    /// nothing usable"). For the scaffolded sidecar stub: the path is not yet wired. `detail` says which.
    MissingOutput { detail: String },
}

impl fmt::Display for RelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RelError::Io(e) => write!(f, "io error preparing relations index: {e}"),
            RelError::Spawn { bin, source } => {
                write!(
                    f,
                    "failed to spawn relations sidecar {}: {source}",
                    bin.display()
                )
            }
            RelError::NonZeroExit { status, stderr } => {
                write!(f, "relations sidecar exited {status}; stderr:\n{stderr}")
            }
            RelError::MissingOutput { detail } => {
                write!(f, "relations index produced no usable result: {detail}")
            }
        }
    }
}

impl Error for RelError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            RelError::Io(e) => Some(e),
            RelError::Spawn { source, .. } => Some(source),
            RelError::NonZeroExit { .. } | RelError::MissingOutput { .. } => None,
        }
    }
}

impl From<io::Error> for RelError {
    fn from(e: io::Error) -> Self {
        RelError::Io(e)
    }
}

/// Resolve the relations sidecar binary path **robustly** (reserved for the sqlite-vec scale path), mirroring
/// `oracle_fba::resolve_fba_bin` / `oracle_slim::resolve_slim_bin`:
///
/// 1. `$RELDB_BIN`, if set (explicit override);
/// 2. else `$HOME/.local/bin/relations-index`, if it exists (the pinned install location);
/// 3. else the bare name `"relations-index"`, resolved via `PATH` by the OS.
///
/// The in-Rust default path does not spawn a process; this exists so the structure mirrors the sibling oracle
/// crates and the sqlite-vec sidecar is a drop-in (inv #5).
#[must_use]
pub fn resolve_reldb_bin() -> PathBuf {
    if let Some(bin) = std::env::var_os("RELDB_BIN") {
        return PathBuf::from(bin);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let candidate = Path::new(&home).join(".local/bin/relations-index");
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from(RELDB_CLI_NAME)
}

/// One nearest-neighbour result: the species ordinal (`SpeciesId` = the registry index) and its EXACT
/// integer-L1 distance from the focal species.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Neighbor {
    /// The neighbour's `SpeciesId` ordinal (the registry index the signature row came from).
    pub sid: usize,
    /// The exact integer-L1 (Manhattan) distance from the focal species, in `u32` (max possible
    /// `D * UNIT_SCALE` ≈ 786k fits comfortably).
    pub distance: u32,
}

/// The k-nearest-species query (inv #5 seam). The default impl is [`InRustIndex`]; a sqlite-vec-backed impl can
/// swap in behind this trait without touching the core or the renderer.
pub trait NearestIndex {
    /// The `k` species closest to `focal` by EXACT integer-L1 distance over the `u16[D]` signatures, excluding
    /// `focal` itself, sorted by `(distance asc, sid asc)` — a total order, fully deterministic. Returns fewer
    /// than `k` if the roster is smaller. An out-of-range `focal` yields an empty list.
    fn nearest(&self, focal: usize, k: usize) -> Vec<Neighbor>;
}

/// The guild-clustering query (inv #5 seam). The default impl is [`InRustIndex`].
pub trait GuildIndex {
    /// Single-link agglomerative clustering at the integer distance `threshold`: a guild id per species
    /// (`SpeciesId`-indexed). Guild ids are canonicalized to the LOWEST member `SpeciesId` so labels are stable
    /// run-to-run. Edges `(i, j)` with `i < j` and `d(i,j) <= threshold` are unioned in ascending `(i, j)`
    /// order — fully deterministic (no centroid float drift, no k-means seeding).
    fn guilds(&self, threshold: u64) -> Vec<u16>;
}

/// The default, deterministic, EXACT in-Rust index: pairwise integer-L1 over the flat `u16[D]` signatures plus
/// the categorical `role:u8` carried alongside (a label/filter, NEVER a distance dim). Built from the
/// `species_signatures()` export. Pure integer; no RNG, no `HashMap`, no float, no transcendental.
#[derive(Debug, Clone)]
pub struct InRustIndex {
    s: usize,
    d: usize,
    /// Flat `s * d` row-major signatures (row `i` = species `i`).
    sigs: Vec<u16>,
    /// One [`gp::TrophicRole`]-ordinal per species, carried alongside as a FILTER (`{Autotroph 0, Heterotroph
    /// 1, Mixotroph 2, Decomposer 3}`). Never enters the distance.
    roles: Vec<u8>,
}

impl InRustIndex {
    /// Build an index from the flat signature export `(s, d, flat s*d u16, roles s u8)`. Validates the flat
    /// length; a malformed input degrades to an empty index (queries return empties) rather than panicking.
    #[must_use]
    pub fn index(s: usize, d: usize, sigs: &[u16], roles: &[u8]) -> Self {
        if d == 0 || sigs.len() != s * d || roles.len() != s {
            return Self {
                s: 0,
                d: 0,
                sigs: Vec::new(),
                roles: Vec::new(),
            };
        }
        Self {
            s,
            d,
            sigs: sigs.to_vec(),
            roles: roles.to_vec(),
        }
    }

    /// Species count.
    #[must_use]
    pub fn species_count(&self) -> usize {
        self.s
    }

    /// The role ordinal carried alongside species `i` (the FILTER label). `None` if out of range.
    #[must_use]
    pub fn role(&self, i: usize) -> Option<u8> {
        self.roles.get(i).copied()
    }

    /// The signature row for species `i` (`None` if out of range).
    #[must_use]
    fn row(&self, i: usize) -> Option<&[u16]> {
        if i < self.s {
            Some(&self.sigs[i * self.d..(i + 1) * self.d])
        } else {
            None
        }
    }

    /// EXACT integer-L1 (Manhattan) distance between species `a` and `b`: `Σ_k |a_k − b_k|`. Accumulated in
    /// `u64` (no overflow: `D * UNIT_SCALE` is tiny), no float, no transcendental. `0` for an out-of-range pair.
    #[must_use]
    pub fn distance(&self, a: usize, b: usize) -> u64 {
        match (self.row(a), self.row(b)) {
            (Some(ra), Some(rb)) => ra
                .iter()
                .zip(rb.iter())
                .map(|(&x, &y)| u64::from(x.abs_diff(y)))
                .sum(),
            _ => 0,
        }
    }

    /// [`NearestIndex::nearest`] restricted to neighbours whose `role` ordinal equals `role` — the "nearest
    /// decomposer" FILTER (role is a label, never a distance dim). Same total order `(distance asc, sid asc)`.
    #[must_use]
    pub fn nearest_with_role(&self, focal: usize, k: usize, role: u8) -> Vec<Neighbor> {
        self.nearest_filtered(focal, k, |sid| self.roles.get(sid).copied() == Some(role))
    }

    /// The shared nearest-with-predicate core. Walks candidate sids in ascending order, computes the exact
    /// integer-L1 distance, sorts by `(distance asc, sid asc)`, truncates to `k`.
    fn nearest_filtered(
        &self,
        focal: usize,
        k: usize,
        keep: impl Fn(usize) -> bool,
    ) -> Vec<Neighbor> {
        if focal >= self.s {
            return Vec::new();
        }
        let mut cands: Vec<Neighbor> = (0..self.s)
            .filter(|&sid| sid != focal && keep(sid))
            .map(|sid| Neighbor {
                sid,
                distance: self.distance(focal, sid).min(u64::from(u32::MAX)) as u32,
            })
            .collect();
        // Total order: distance asc, then sid asc (the deterministic tie-break inv #3 requires).
        cands.sort_by(|a, b| a.distance.cmp(&b.distance).then(a.sid.cmp(&b.sid)));
        cands.truncate(k);
        cands
    }
}

impl NearestIndex for InRustIndex {
    fn nearest(&self, focal: usize, k: usize) -> Vec<Neighbor> {
        self.nearest_filtered(focal, k, |_| true)
    }
}

impl GuildIndex for InRustIndex {
    fn guilds(&self, threshold: u64) -> Vec<u16> {
        if self.s == 0 {
            return Vec::new();
        }
        // Union-find over species indices. Each starts in its own set.
        let mut parent: Vec<usize> = (0..self.s).collect();
        fn find(parent: &mut [usize], mut x: usize) -> usize {
            // Iterative path-halving (no recursion, deterministic).
            while parent[x] != x {
                parent[x] = parent[parent[x]];
                x = parent[x];
            }
            x
        }
        // Edges walked in ascending (i, j) order (i < j); union toward the LOWER root so labels canonicalize to
        // the lowest-member SpeciesId. Deterministic single-link agglomeration.
        for i in 0..self.s {
            for j in (i + 1)..self.s {
                if self.distance(i, j) <= threshold {
                    let ri = find(&mut parent, i);
                    let rj = find(&mut parent, j);
                    if ri != rj {
                        // Lower root wins → guild id = lowest member sid.
                        let (lo, hi) = if ri < rj { (ri, rj) } else { (rj, ri) };
                        parent[hi] = lo;
                    }
                }
            }
        }
        // Canonicalize: each species' guild id = the root's index (the lowest member by construction). Fits u16
        // (S is small); guild ids are a stable subset of {SpeciesId}.
        (0..self.s).map(|i| find(&mut parent, i) as u16).collect()
    }
}

/// The sqlite-vec SCALE-PATH stub (scaffolded, probe-and-skip, NOT wired). When a roster-size trigger trips,
/// a separate `relations-index` CLI binary linking sqlite-vec (Apache-2.0 OR MIT, GPL-clean) is shelled out to
/// — writing run-namespaced `.db` sidecar rows, returning already-ordered integer results across the boundary.
/// Until that lands this returns [`RelError::MissingOutput`] (the in-Rust default is the only CI/gate path).
///
/// # Errors
/// Always [`RelError::MissingOutput`] for now — the sidecar is not yet built/wired (probe-and-skip).
pub fn index_via_sidecar(
    _s: usize,
    _d: usize,
    _sigs: &[u16],
    _roles: &[u8],
) -> Result<InRustIndex, RelError> {
    let _bin = resolve_reldb_bin(); // resolved for structure parity; never spawned at this scale.
    Err(RelError::MissingOutput {
        detail: "sqlite-vec scale path not yet wired (in-Rust InRustIndex is the active path)"
            .into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 3-species fixture: an autotroph (plant-like), a decomposer (E. coli-like), and a heterotroph
    /// (predator-like). The autotroph and decomposer are far apart in budget/affinity; the heterotroph sits
    /// closer to the decomposer. D = 12 (the pinned layout).
    fn fixture() -> InRustIndex {
        let d = 12;
        // Three distinct signature rows on the u16 grid. The exact values matter only relatively.
        let autotroph = [
            60000u16, 5000, 8000, 4000, 2000, 60000, 1000, 0, 0, 10000, 2000, 30000,
        ];
        let decomposer = [
            3000u16, 4000, 5000, 50000, 8000, 0, 2000, 60000, 50000, 50000, 40000, 30000,
        ];
        let heterotroph = [
            5000u16, 6000, 6000, 45000, 9000, 1000, 3000, 55000, 48000, 48000, 42000, 30000,
        ];
        let mut sigs = Vec::new();
        sigs.extend_from_slice(&autotroph);
        sigs.extend_from_slice(&decomposer);
        sigs.extend_from_slice(&heterotroph);
        // roles: Autotroph 0, Decomposer 3, Heterotroph 1.
        let roles = vec![0u8, 3, 1];
        InRustIndex::index(3, d, &sigs, &roles)
    }

    #[test]
    fn distance_is_symmetric_and_zero_on_self() {
        let idx = fixture();
        assert_eq!(idx.distance(0, 0), 0);
        assert_eq!(idx.distance(0, 1), idx.distance(1, 0));
        assert!(idx.distance(0, 1) > 0);
    }

    #[test]
    fn nearest_is_deterministic_and_ordered() {
        let idx = fixture();
        // The autotroph's nearest are the two others, ordered by distance asc then sid asc.
        let n = idx.nearest(0, 2);
        assert_eq!(n.len(), 2);
        assert!(n[0].distance <= n[1].distance, "sorted by distance asc");
        // Reproducible bit-for-bit.
        assert_eq!(idx.nearest(0, 2), idx.nearest(0, 2));
        // Out-of-range focal → empty.
        assert!(idx.nearest(99, 2).is_empty());
        // k larger than the roster → at most s-1.
        assert_eq!(idx.nearest(0, 100).len(), 2);
    }

    #[test]
    fn nearest_decomposer_filter_works() {
        let idx = fixture();
        // From the heterotroph (sid 2), the nearest DECOMPOSER (role 3) is sid 1 only.
        let n = idx.nearest_with_role(2, 5, 3);
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].sid, 1);
    }

    #[test]
    fn ties_break_to_lowest_sid() {
        // Two equidistant candidates from focal 0 → the lower sid comes first.
        let d = 2;
        let sigs = [
            0u16, 0, /*sid0*/ 0, 10, /*sid1*/ 0, 10, /*sid2*/
        ];
        let roles = vec![0u8, 0, 0];
        let idx = InRustIndex::index(3, d, &sigs, &roles);
        let n = idx.nearest(0, 2);
        assert_eq!(n[0].distance, n[1].distance, "both at distance 10");
        assert_eq!(n[0].sid, 1, "tie → lowest sid first");
        assert_eq!(n[1].sid, 2);
    }

    #[test]
    fn guilds_separate_distinct_species_at_a_tight_threshold() {
        let idx = fixture();
        // At a tight threshold, the far-apart autotroph is its own guild; the close decomposer+heterotroph
        // merge. Tune the threshold to the fixture's distances.
        let d01 = idx.distance(0, 1);
        let d12 = idx.distance(1, 2);
        assert!(
            d12 < d01,
            "heterotroph is closer to the decomposer than the autotroph is"
        );
        let t = (d12 + d01) / 2; // between the two → 1↔2 edge in, 0↔1 edge out
        let g = idx.guilds(t);
        assert_eq!(g.len(), 3);
        assert_ne!(g[0], g[1], "autotroph clusters apart from the decomposer");
        assert_eq!(g[1], g[2], "decomposer + heterotroph share a guild");
        // Guild id canonicalized to the lowest member sid.
        assert_eq!(g[1], 1, "merged guild id = lowest member (sid 1)");
    }

    #[test]
    fn guilds_canonical_labels_are_stable() {
        let idx = fixture();
        // Same threshold → byte-identical guild labels every call (no float drift, no seeding).
        assert_eq!(idx.guilds(GUILD_THRESHOLD), idx.guilds(GUILD_THRESHOLD));
    }

    #[test]
    fn guilds_all_merge_at_a_huge_threshold_and_split_at_zero() {
        let idx = fixture();
        // Huge threshold → one guild (all canonicalized to sid 0).
        let all = idx.guilds(u64::MAX);
        assert!(all.iter().all(|&g| g == 0), "everything in guild 0");
        // Threshold 0 → every distinct species its own guild.
        let none = idx.guilds(0);
        assert_eq!(none, vec![0, 1, 2]);
    }

    #[test]
    fn malformed_input_degrades_to_empty() {
        // Wrong flat length → empty index, queries return empties (never a panic).
        let idx = InRustIndex::index(3, 12, &[1, 2, 3], &[0, 0, 0]);
        assert_eq!(idx.species_count(), 0);
        assert!(idx.nearest(0, 5).is_empty());
        assert!(idx.guilds(100).is_empty());
    }

    #[test]
    fn sidecar_path_is_probe_and_skip() {
        // The scale path is scaffolded-not-built: it returns MissingOutput (the in-Rust path is active).
        let err = index_via_sidecar(2, 12, &[], &[]).unwrap_err();
        assert!(matches!(err, RelError::MissingOutput { .. }), "got {err:?}");
    }

    #[test]
    fn resolve_reldb_bin_uses_explicit_override() {
        // An explicit $RELDB_BIN override resolves verbatim — exercises step 1 without depending on the host's
        // PATH / install state (which varies in CI).
        std::env::set_var("RELDB_BIN", "/tmp/relations-index-test-bin");
        let bin = resolve_reldb_bin();
        std::env::remove_var("RELDB_BIN");
        assert_eq!(bin, PathBuf::from("/tmp/relations-index-test-bin"));
    }
}
