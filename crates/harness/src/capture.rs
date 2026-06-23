//! D1 trace capture — the off-hash seam that turns a headless [`GeneSimEnv`] run into a
//! [`discovery::trace::PerGenTrace`] the D0 scorer consumes (inv #1/#5).
//!
//! ## Where the harness emits it (the capture seam)
//! [`capture_trace`] drives the SAME `reset` → per-generation `step(Advance(1))` loop the live env runs,
//! but AFTER each generation it READS [`GeneSimEnv::observe_all`] (per-species `pop` + `allele_freq`) and
//! [`GeneSimEnv::flow_matrix`] (the per-tick trophic FlowMatrix) into a [`discovery::trace::GenRow`]. Both
//! reads are PROVEN hash-neutral in sim-core (they draw ZERO `SimRng`, mutate nothing, and are never folded
//! into `hash_world`) — exactly as `tests/per_gen_stats.rs` proves stepping-with-reads is hash-neutral. So
//! capturing a trace CANNOT move the pinned literal `0x47a0_3c8f_6701_f240` (inv #3): a captured run and an
//! un-captured run of the same `(seed, actions)` produce the byte-identical `run_stats().hash`.
//!
//! ## What it captures (D1 schema, [`discovery::trace`])
//! - `species[]` (id / key / role ordinal) ONCE from `observe_all()` after reset — constant for the run.
//! - one `GenRow` per generation: `pop` (per-species `population_size`), `allele_q` (q16 permille of
//!   `allele_freq`), and the SPARSE off-diagonal `flow` edges (`amount > 0`) from `flow_matrix()`.
//! - `inoculations[]` derived from the journaled [`Action::RegionInoculate`] point actions.
//! - early-stop when Σpop == 0 (the run is dead) — `g` records the captured generation count.
//!
//! ## Action timing (the loop owns time)
//! `actions` is a list of `(gen, Action)` POINT actions keyed to the generation at which they fire. The loop
//! drives time itself via `Advance(1)` per generation, so an [`Action::Advance`] in the list is NOT stepped
//! (the loop already advances one generation per row); every NON-`Advance` action scheduled for generation
//! `g` is applied at the TOP of generation `g`'s step, before the single advance that produces row `g`. This
//! is the deterministic, replay-stable contract: the captured trace is a pure function of `(seed, actions)`.

use crate::{Action, Env, GeneSimEnv};
use discovery::fixed::q16;
use discovery::trace::{GenRow, InocRec, PerGenTrace, SpeciesMeta};
use sim_core::gp::TrophicRole;

/// The categorical `TrophicRole` ordinal `{Autotroph 0, Heterotroph 1, Mixotroph 2, Decomposer 3,
/// Predator 4, ObligateSymbiont 5}` — the SAME mapping as `sim_core::signature::role_ordinal` (which is
/// crate-private to sim-core), reproduced here so the trace's `SpeciesMeta::role` matches that ordinal
/// contract. Pure, ordered `match` (inv #3); appended-variant-safe (a new role would fail to compile and
/// force a review). Kept in lock-step with the `TrophicRole` declaration order.
#[must_use]
fn role_ordinal(role: TrophicRole) -> u8 {
    match role {
        TrophicRole::Autotroph => 0,
        TrophicRole::Heterotroph => 1,
        TrophicRole::Mixotroph => 2,
        TrophicRole::Decomposer => 3,
        TrophicRole::Predator => 4,
        TrophicRole::ObligateSymbiont => 5,
    }
}

/// Build a [`GenRow`] for the just-stepped generation `gen` from the env's read-only projections.
///
/// `pop[i]` is species `i`'s `population_size`; `allele_q[i]` is q16-permille of its `allele_freq` (the ONE
/// fenced float touch, done here at capture — never on the score path). `flow` is the SPARSE off-diagonal
/// FlowMatrix: `flat[i*s + j]` is net J from species `j` INTO species `i` this tick, so a positive
/// off-diagonal entry becomes `(dest = i, src = j, amount)` (the `(dest, src, amount>0)` schema). Reading
/// `observe_all`/`flow_matrix` draws ZERO `SimRng` and is never folded into `hash_world` (inv #3).
fn gen_row(env: &GeneSimEnv, gen: u32) -> GenRow {
    let obs = env.observe_all();
    let pop: Vec<u32> = obs.iter().map(|o| o.population_size).collect();
    let allele_q: Vec<u16> = obs.iter().map(|o| q16(o.allele_freq)).collect();

    let (s, flat) = env.flow_matrix();
    let mut flow: Vec<(u16, u16, i64)> = Vec::new();
    for i in 0..s {
        for j in 0..s {
            if i == j {
                continue; // off-diagonal only (a species does not "flow to itself")
            }
            let amount = flat[i * s + j];
            if amount > 0 {
                // (dest = i, src = j): flat[i*s+j] = J from species j INTO species i (ADR-013 F4).
                flow.push((i as u16, j as u16, amount));
            }
        }
    }
    GenRow {
        gen,
        pop,
        allele_q,
        flow,
    }
}

/// Capture a per-generation D1 trace of a headless run (the D1 capture seam, inv #1/#5).
///
/// `reset(seed)`, then for `gen` in `1..=gens` apply every NON-[`Action::Advance`] POINT action in `actions`
/// scheduled for this `gen` (in the order they appear), `step(Advance(1))` to advance ONE generation, and
/// push a [`GenRow`] read from `observe_all()` + `flow_matrix()`. `species[]` (id / key / role ordinal) is
/// read ONCE from `observe_all()` after reset; `inoculations[]` is derived from the journaled
/// [`Action::RegionInoculate`]s (each resolved to its species ordinal by `species_key`). The loop EARLY-STOPS
/// when the total living population hits 0 (the run is dead): `g` then records how many generations were
/// captured (`g < gens`), with `gens_requested = gens` kept for M6's `ran_long_bp`.
///
/// CAPTURE IS OFF-HASH (inv #3): `observe_all`/`flow_matrix` draw ZERO `SimRng` and are never folded into
/// `hash_world`, so a captured run and an un-captured run of the same `(seed, actions)` produce the
/// byte-identical `run_stats().hash` — capturing a trace cannot move `0x47a0_3c8f_6701_f240`. The returned
/// `recorded_hash` is the run's `run_stats().hash` (folded in once at the end, exactly as a normal episode).
#[must_use]
pub fn capture_trace(
    env: &mut GeneSimEnv,
    seed: u64,
    gens: u32,
    actions: &[(u32, Action)],
) -> PerGenTrace {
    env.reset(seed);

    // species[] ONCE from observe_all (constant per run): id / key / role ordinal, in SpeciesId order.
    let species: Vec<SpeciesMeta> = env
        .observe_all()
        .into_iter()
        .map(|o| SpeciesMeta {
            id: o.species_id,
            key: o.key,
            role: role_ordinal(o.role),
        })
        .collect();

    // inoculations[] from the journaled RegionInoculate point actions, resolved to a species ordinal by key
    // against the species[] just read. An unresolved key (never registered) is skipped (a logged no-op live).
    let inoculations: Vec<InocRec> = actions
        .iter()
        .filter_map(|(gen, a)| match a {
            Action::RegionInoculate {
                species_key, count, ..
            } => species
                .iter()
                .find(|m| &m.key == species_key)
                .map(|m| InocRec {
                    gen: *gen,
                    species_id: m.id,
                    count: *count,
                }),
            _ => None,
        })
        .collect();

    let mut rows: Vec<GenRow> = Vec::with_capacity(gens as usize);
    let mut captured: u32 = 0;
    for gen in 1..=gens {
        // Apply every POINT action scheduled for THIS gen (skip Advance — the loop owns time), in list order.
        for (g, a) in actions {
            if *g == gen && !matches!(a, Action::Advance(_)) {
                env.step(a.clone());
            }
        }
        // Advance exactly one generation, then read the row from the off-hash projections.
        env.step(Action::Advance(1));
        let row = gen_row(env, gen);
        let total: u64 = row.pop.iter().map(|&p| u64::from(p)).sum();
        rows.push(row);
        captured = gen;
        if total == 0 {
            break; // the run is dead — early-stop (g records the captured generation count)
        }
    }

    let recorded_hash = env.run_stats().hash;
    PerGenTrace {
        s: species.len() as u16,
        g: captured,
        gens_requested: gens,
        species,
        rows,
        inoculations,
        seed,
        recorded_hash,
    }
}
