//! D1 trace schema — the off-hash export the harness POPULATES and the D0 scorer READS (inv #2/#3).
//!
//! Every field is integer (or already-quantized: `allele_q` is q16 permille). The trace is a PURE projection
//! of `observe_all()` + `flow_matrix()` per generation, both proven hash-neutral in sim-core — capturing it
//! cannot move the pinned `hash_world` literal (inv #3). Defined HERE (std+serde) so the scorer carries no
//! sim-core dependency: the harness builds a `PerGenTrace` from sim-core types and hands the plain struct over
//! the capture seam (inv #1/#5).

use serde::{Deserialize, Serialize};

/// Per-species constant metadata (id, key, trophic role ordinal) — fixed for the whole run.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpeciesMeta {
    /// Stable species id (matches the `pop`/`allele_q` index position across all rows).
    pub id: u16,
    /// Human-readable species key (config name); carried for explainability, unused by the score path.
    pub key: String,
    /// `TrophicRole` ordinal, const per run (matches `sim-core::signature::role_ordinal`).
    pub role: u8,
}

/// One generation's observed state. `pop`/`allele_q` are indexed by species position (same order as
/// [`PerGenTrace::species`]); `flow` is the SPARSE FlowMatrix for this tick.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenRow {
    /// Generation index this row was captured at.
    pub gen: u32,
    /// Per-species population count (indexed by species position).
    pub pop: Vec<u32>,
    /// Per-species mean-allele fraction as q16 permille (`fixed::q16`); reserved for future genetic metrics.
    pub allele_q: Vec<u16>,
    /// Sparse FlowMatrix entries `(dest, src, amount)` with `amount > 0` (J transferred src → dest this tick).
    pub flow: Vec<(u16, u16, i64)>,
}

/// A journaled inoculation event (from `actions.ndjson` `RegionInoculate`) — drives M5's
/// IMMIGRATE_ESTABLISHED event.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InocRec {
    /// Generation the inoculation was applied at.
    pub gen: u32,
    /// Species id inoculated.
    pub species_id: u16,
    /// Number of organisms introduced.
    pub count: u32,
}

/// The full per-generation trace of one run — the unit the scorer consumes. All integer / quantized.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PerGenTrace {
    /// Number of species (== `species.len()`).
    pub s: u16,
    /// Number of generations actually captured (== `rows.len()`; may be < `gens_requested` on early-stop).
    pub g: u32,
    /// Generations the run was asked to execute (drives M6's `ran_long_bp`).
    pub gens_requested: u32,
    /// Per-species constant metadata.
    pub species: Vec<SpeciesMeta>,
    /// One [`GenRow`] per captured generation, in ascending `gen` order.
    pub rows: Vec<GenRow>,
    /// Journaled inoculations during the run.
    pub inoculations: Vec<InocRec>,
    /// The master seed of the run (carried for gem reproducibility; not scored).
    pub seed: u64,
    /// The recorded `hash_world` at run end (the reproducibility contract anchor; not scored).
    pub recorded_hash: u64,
}
