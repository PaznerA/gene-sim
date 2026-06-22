//! The chemical / signal diffusion field — allelopathy, kin-selection, and chemotaxis (ADR-013 **F5**).
//!
//! F5 layers an ENDOGENOUS, organism-emitted chemical substrate over the F3/F4 trophic web. Three planes —
//! **toxin** (channel 0, allelopathy), **kin** (1, a per-species presence marker), **alarm** (2, distress) —
//! each a flat row-major `Vec<i32>` of MILLI-UNITS where **`1 milli == 1 J` exactly** (the synthesis pin,
//! [`CHEM_J_PER_MILLI`]). Because milli == J 1:1, the chem field is part of the SAME conserved Σ the
//! [`crate::ledger`] closes — an `i32` plane value IS a joule, reconciled into the `i64` ledger by pure
//! widening (no multiply, no divide, no remainder, no API churn). The four-bucket identity becomes
//! `Σ(pools + chem + Energy + Biomass) == initial + influx − respired − chem_decay − overflow`.
//!
//! The whole pipeline is INTEGER, ORDERED, and draws **ZERO** `SimRng` (inv #3):
//! - [`diffuse_and_decay`] — an organism-free, row-major, all-`>>`-shift reflecting stencil. Diffusion is
//!   mass-EXACT (Σ-before == Σ-after, asserted by [`assert_chem_conserved`]); decay is the only chem sink, a
//!   named [`ledger::Ledger::chem_decay`] tap (NOT folded into `respired`).
//! - [`emit_chem`] — organisms spend J deterministically into the field (a paired Energy→chem move, Σ
//!   unchanged). Toxin is minted inline in [`crate::metabolism`] (re-routing the Defense budget slice the
//!   convert step already respires); kin + live-distress alarm emit here.
//! - [`ChemModifier`] — the three SENSE couplings, all reading the org's OWN cell chem frozen at start-of-tick
//!   as INTEGER PERMILLE factors folded into the pre-apportion demand product (the EditModifier precedent —
//!   never an f64 multiply on the granted-J path, the F3 invariant). Toxin suppresses uptake (+ a separate
//!   lethal Energy→respired drain); kin boosts uptake/survival; alarm biases dispersal **draw-count-neutrally**
//!   (it re-interprets the already-drawn dispersal word via a baked LUT — ZERO new RNG draws).
//!
//! At INTRODUCTION the field is seeded ALL-ZERO ([`ChemField::zeroed`]) — chem is emitted by organisms, never
//! seed-generated, so it draws no `derive_seed` and `Σchem_initial == 0` (the ledger's `initial_total` is
//! unchanged → no reset surprise). A roster where no species emits (every `budget[Defense] == 0`, never
//! distressed) leaves `ChemField == 0`, `chem_decay == 0` → the J path is byte-identical to F4.

use bevy_ecs::prelude::*;

use crate::gp::BudgetChannel;
use crate::{cell_index, ledger, Energy, OrgId, Position, Species};

/// The number of chemical/signal channels (toxin, kin, alarm) — the index-is-contract pin, parallel to
/// [`BudgetChannel`]. NEVER a `HashMap`; planes are indexed by [`ChemChannel`] ordinal (inv #3).
pub const CHEM_CHANNELS: usize = 3;

/// `1 milli-unit == 1 J` exactly (the synthesis pin, ADR-013 F5 §units_ledger). This eliminates the scale, the
/// divide, the remainder, the residual accumulator, AND the `ledger.rs` API rewrite simultaneously: an `i32`
/// chem plane value IS a joule, widened to `i64` for the ledger with no arithmetic. Chem is therefore a full
/// part of the conserved Σ, not a separate signal with its own units.
pub const CHEM_J_PER_MILLI: i32 = 1;

/// The three chem planes, in fixed declaration order. The INDEX is the channel id (the load-bearing contract,
/// read by ordinal, never by name — inv #3), exactly parallel to [`BudgetChannel`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ChemChannel {
    /// Allelopathic toxin — suppresses neighbours' uptake + a lethal maintenance drain (kin-sparing).
    Toxin = 0,
    /// Per-species presence marker — boosts own-species uptake/survival (the kin-selection mechanic).
    Kin = 1,
    /// Distress signal — emitted on low-energy / death; biases dispersal AWAY (flee chemotaxis).
    Alarm = 2,
}

impl ChemChannel {
    /// The channels in fixed declaration order — the planes are index-aligned to this.
    pub const ALL: [ChemChannel; CHEM_CHANNELS] =
        [ChemChannel::Toxin, ChemChannel::Kin, ChemChannel::Alarm];
}

// ── F5 chemostat constants — ALL integer, every "divide" a pinned power-of-two right-shift (inv #3, #7) ──────
//
// These are the F5 LANDING values; the F5.x chemostat-tuning sweep (the F3.4 precedent) re-tunes them OFF the
// hash to LEGIBLE dynamics before the literal is pinned in the Repin phase. The numeric budget stays far under
// i64::MAX: MAX_POPULATION orgs × CHEM_CAP per cell × 1024 cells, widened i32→i64, has many orders of margin.

/// Per-channel DIFFUSE right-shift (the `share = cc >> DIFFUSE_SHIFT` amount sent to EACH von-Neumann
/// neighbour). A LARGER shift = a SMALLER share = a SLOWER-spreading, more-local plane. Per-channel (inv #7)
/// so toxin spreads farther (small shift), the kin marker stays local (large shift), alarm is most volatile
/// (smallest shift) — richer emergence. Indexed by [`ChemChannel`] ordinal.
pub const DIFFUSE_SHIFT: [u32; CHEM_CHANNELS] = [4, 6, 3]; // toxin, kin, alarm

/// DECAY right-shift: `lost = plane[c] >> DECAY_SHIFT` per cell per tick (`6` → ~1/64 ≈ 1.5%/tick). The only
/// chem sink; `lost` is booked to the named [`ledger::Ledger::chem_decay`] tap. Pure shift, never negative.
pub const DECAY_SHIFT: u32 = 6;

/// Per-cell hard ceiling on any single chem plane (milli == J). An emit past this routes the rejected part to
/// [`ledger::Ledger::overflow`] (never a silent clamp — the `credit_capped` precedent).
pub const CHEM_CAP: i32 = 50_000_000;

/// Of an org's Defense budget slice J (already respired in convert), this fraction is RE-ROUTED into the toxin
/// plane instead of respired: `toxin_minted = defense_J · TOXIN_YIELD_NUM / TOXIN_YIELD_DEN` (floored once).
/// A species with `budget[Defense] == 0` mints zero → an allelopathy-off roster is byte-identical (hash-neutral).
pub const TOXIN_YIELD_NUM: i64 = 6;
/// Toxin-yield denominator (see [`TOXIN_YIELD_NUM`]) — 60% of the Defense slice becomes field toxin.
pub const TOXIN_YIELD_DEN: i64 = 10;

/// The flat J each living org spends per tick on its KIN presence marker (`min(KIN_BASE, energy)` debited from
/// Energy, deposited 1:1 to the kin plane + tagged in [`KinProvenance`]). Small (presence-signalling, not a
/// costly secretion) but non-zero so it stays a CONSERVED joule, not a mint-from-nothing.
pub const KIN_BASE: i64 = 50;

/// LIVE-DISTRESS alarm trigger: an org whose start-of-tick `Energy` is below this (a multiple of the
/// maintenance floor) spends `min(ALARM_BASE, energy)` J → the alarm plane in [`emit_chem`].
pub const ALARM_TRIGGER: i64 = 20_000;
/// The J a distressed org spends on its alarm signal (`min(ALARM_BASE, energy)` 1:1 → alarm plane).
pub const ALARM_BASE: i64 = 100;

/// Of a dying org's carcass residual, this fraction is diverted to the alarm plane INSTEAD of detritus (a
/// residual split like LITTERFALL — stays conserved, the residual was already J about to deposit). NUM/DEN.
pub const ALARM_FRACTION_NUM: i64 = 1;
/// Death-alarm fraction denominator (see [`ALARM_FRACTION_NUM`]) — 10% of the carcass residual becomes alarm.
pub const ALARM_FRACTION_DEN: i64 = 10;

// ── SENSE coupling constants (integer permille factors folded into the demand product / maintenance debit) ───

/// TOXIN → SUPPRESS UPTAKE: `tox_suppress = PERMILLE − min(PERMILLE, frozen_toxin · NUM / DEN)`, floored at
/// [`TOXIN_SUPPRESS_FLOOR`] so it never fully zeroes (the `match_permille.max(PERMILLE/4)` floor, ADR-005 spirit).
/// Per this much local toxin, demand drops 1 permille — GENTLE at default levels (~2e5/cell → ~6% drop), so
/// the F4 ecology relationship survives; the F5.x sweep re-tunes for competitive exclusion.
pub const TOXIN_SUPPRESS_NUM: i64 = 1;
/// Toxin-suppress denominator (see [`TOXIN_SUPPRESS_NUM`]): per this much local toxin, demand drops 1 permille.
pub const TOXIN_SUPPRESS_DEN: i64 = 32_000;
/// Minimum permille the toxin-suppress factor returns even at saturating local toxin (soft, never a hard zero).
pub const TOXIN_SUPPRESS_FLOOR: u64 = 250;

/// TOXIN → LETHAL DRAIN: `tox_drain = frozen_toxin · NUM / DEN` J added to the maintenance debit (the victim
/// burns ITS reserves resisting → respired). `min(drain, energy)` so it never drives Energy below 0. GENTLE
/// (the F5 landing value): at the default-roster toxin levels (~2e5/cell) the drain is a few percent of
/// `MAINTENANCE_BASE`, so it scales both with/without-decomposer worlds down proportionally instead of
/// inverting the F4 carrying-capacity relationship. The F5.x sweep re-tunes it for legible allelopathy.
pub const TOXIN_DRAIN_NUM: i64 = 1;
/// Toxin-drain denominator (see [`TOXIN_DRAIN_NUM`]). Large → gentle per-tick drain.
pub const TOXIN_DRAIN_DEN: i64 = 4_000;

/// KIN-SPARING discount on the toxin lethal-drain (permille) when the org has its OWN-species kin marker
/// present at its cell: `tox_drain · KIN_SPARE_PERMILLE / 1000`. Makes allelopathy ASYMMETRIC (a lineage
/// tolerates its own toxin) so Defense is not strictly self-defeating (resolves the self-poisoning concern).
pub const KIN_SPARE_PERMILLE: i64 = 400; // own-species toxin hits at 40% strength

/// KIN → BOOST: `kin_boost = PERMILLE + min(KIN_BOOST_CAP, kin_own · NUM / DEN)` (a >1000 demand factor, the
/// Activate `[1000,1500]` lift), AND lowers the maintenance debit. `kin_own` is read per-species from
/// [`KinProvenance`]. Integer permille, gated on `kin_own != 0`.
pub const KIN_BOOST_NUM: i64 = 1;
/// Kin-boost denominator (see [`KIN_BOOST_NUM`]).
pub const KIN_BOOST_DEN: i64 = 4_000;
/// The maximum permille the kin-boost adds above neutral `1000` (caps the lift at `1000 + KIN_BOOST_CAP`).
pub const KIN_BOOST_CAP: u64 = 500;
/// KIN → SURVIVAL: the maintenance debit is scaled by `(PERMILLE − min(KIN_SURVIVAL_CAP, kin_own · NUM/DEN))`
/// permille (kin cooperation lowers upkeep). Same `kin_own`; gated on non-zero.
pub const KIN_SURVIVAL_CAP: u64 = 400; // upkeep can drop to 60% in a dense kin cluster

/// One permille (the fixed-point factor grid), re-exported from [`crate::fixed::PERMILLE`] as an `i64` for the
/// integer sense math below.
const PERMILLE: i64 = crate::fixed::PERMILLE as i64;

/// The endogenous, organism-emitted chemical/signal field (ADR-013 F5). Three milli-unit (== J) planes plus
/// ONE reused double-buffer scratch plane. Row-major `cell = y*width + x`, dims == `WORLD_DIMS` == the
/// `PoolStock` dims (asserted at reset). A `Resource`, inserted right after `PoolStock`. Folded into
/// `hash_world` (the three live planes, raw `i32` row-major) at the F5 re-pin; summed into
/// [`ledger::LiveTotal::chem`] by widening i32→i64. The `scratch` plane is internal double-buffer state, never
/// hashed (it is zero outside [`diffuse_and_decay`]). NEVER a `HashMap` (inv #3).
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChemField {
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Allelopathic toxin (channel 0), milli-units (== J), row-major.
    toxin: Vec<i32>,
    /// Per-species presence marker (channel 1), milli-units (== J), row-major.
    kin: Vec<i32>,
    /// Distress signal (channel 2), milli-units (== J), row-major.
    alarm: Vec<i32>,
    /// Pre-allocated double-buffer scratch, ONE plane reused (zeroed) per channel in [`diffuse_and_decay`].
    /// Internal state only — zero between ticks, never folded into the hash.
    scratch: Vec<i32>,
}

impl ChemField {
    /// A fresh ALL-ZERO field sized to `width × height` (chem is endogenous — emitted, never seed-generated).
    /// Σ == 0, so adding it leaves the ledger's `initial_total` unchanged (no reset surprise).
    pub(crate) fn zeroed(width: u32, height: u32) -> Self {
        let cells = (width as usize) * (height as usize);
        Self {
            width,
            height,
            toxin: vec![0; cells],
            kin: vec![0; cells],
            alarm: vec![0; cells],
            scratch: vec![0; cells],
        }
    }

    /// Immutable per-channel plane (`0` toxin, `1` kin, `2` alarm). Mirrors [`crate::pool_channel`].
    pub(crate) fn plane(&self, ch: usize) -> &[i32] {
        match ch {
            0 => &self.toxin,
            1 => &self.kin,
            _ => &self.alarm,
        }
    }

    /// Mutable per-channel plane (see [`plane`](Self::plane)).
    pub(crate) fn plane_mut(&mut self, ch: usize) -> &mut [i32] {
        match ch {
            0 => &mut self.toxin,
            1 => &mut self.kin,
            _ => &mut self.alarm,
        }
    }

    /// `Σ` over all cells of `toxin + kin + alarm`, each i32 cell WIDENED to i64 before adding (exact,
    /// commutative, no overflow) — the [`ledger::LiveTotal::chem`] term. Because milli == J 1:1, this sum IS
    /// joules with zero conversion.
    pub(crate) fn total(&self) -> i64 {
        let s = |v: &[i32]| -> i64 { v.iter().map(|&x| i64::from(x)).sum() };
        s(&self.toxin) + s(&self.kin) + s(&self.alarm)
    }

    /// `Σ` over one channel's cells, widened to i64. Used by the conservation assert + tests.
    fn channel_total(&self, ch: usize) -> i64 {
        self.plane(ch).iter().map(|&x| i64::from(x)).sum()
    }

    /// Deposit `amount` (>0) milli-J into `plane[cell]` capped at [`CHEM_CAP`]; returns the REJECTED overflow
    /// part (routed to [`ledger::Ledger::overflow`] by the caller — never a silent clamp). The `credit_capped`
    /// precedent, on the i32 chem grid.
    fn deposit_capped(plane: &mut [i32], cell: usize, amount: i32) -> i32 {
        deposit_capped_plane(plane, cell, amount)
    }
}

/// Deposit `amount` (>0) milli-J into `plane[cell]` capped at [`CHEM_CAP`]; returns the REJECTED overflow part
/// (the caller books it to [`ledger::Ledger::overflow`] — never a silent clamp). Free function so
/// [`crate::metabolism`] can mint toxin into a borrowed [`ChemField`] plane without holding the whole resource.
pub(crate) fn deposit_capped_plane(plane: &mut [i32], cell: usize, amount: i32) -> i32 {
    if amount <= 0 {
        return 0;
    }
    let headroom = (CHEM_CAP - plane[cell]).max(0);
    let accepted = amount.min(headroom);
    plane[cell] += accepted;
    amount - accepted
}

/// Per-cell, per-species KIN attribution (ADR-013 F5) — REUSING the [`crate::trophic::PoolProvenance`]
/// mechanism: flat `[cell*S + species]`, so a sensing org can read "how much of MY species' marker is here".
/// Held as `i64` (it accumulates the same joules the kin plane does, but per-species). PERSISTS cross-tick
/// like the chem planes (gradients linger). The TOTAL over species at a cell does NOT need to equal the kin
/// plane (diffusion moves the aggregate plane but provenance is a deposit ledger — sensing reads provenance,
/// the demand/decay math reads the plane). Reset all-zero at run start. NEVER a `HashMap` (inv #3).
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub(crate) struct KinProvenance {
    s: usize,
    /// Per-species kin marker per cell, flat `[cell*S + species]`, milli-J (== J).
    kin: Vec<i64>,
}

impl KinProvenance {
    /// A zeroed provenance ledger for `cells` cells and `s` species.
    pub(crate) fn new(cells: usize, s: usize) -> Self {
        Self {
            s,
            kin: vec![0i64; cells * s],
        }
    }

    /// Attribute `amount` (>0) of kin marker to (`cell`, `species`).
    fn deposit(&mut self, cell: usize, species: usize, amount: i64) {
        if amount > 0 && species < self.s {
            self.kin[cell * self.s + species] += amount;
        }
    }

    /// Read the own-species kin marker accumulated at `cell` for `species` (the sense input). `0` for an
    /// out-of-range species. Pure read.
    pub(crate) fn own(&self, cell: usize, species: usize) -> i64 {
        if species < self.s {
            self.kin[cell * self.s + species]
        } else {
            0
        }
    }
}

/// **RESET CHEM SCRATCH** (ADR-013 F5) — zero the reused double-buffer scratch plane at the START of every
/// tick. The [`ChemField`] planes themselves PERSIST (concentrations are cross-tick gradient state, like the
/// `PoolProvenance` obligate-loop lag). RNG-free, organism-free. Runs right after [`crate::trophic::reset_flow`].
pub(crate) fn reset_chem_scratch(mut chem: ResMut<ChemField>) {
    for v in &mut chem.scratch {
        *v = 0;
    }
}

/// **DIFFUSE AND DECAY** (ADR-013 F5 KEYSTONE) — the organism-free, row-major, all-`>>`-shift field math. One
/// named pass: a mass-EXACT reflecting-boundary diffusion (Σ-before == Σ-after, asserted per channel) followed
/// by the only chem sink, decay (the named [`ledger::Ledger::chem_decay`] tap). RNG-free, no transcendental,
/// no `HashMap`, iterates CELLS only (touches no organisms → no `(cell, SpeciesId, OrgId)` sort needed).
///
/// Runs on the PREVIOUS tick's emitted chem BEFORE this tick's organisms sense it (a one-tick propagation lag,
/// mirroring the F4 detritus→mineralize lag), and the conservation assert brackets a clean diffusion step.
pub(crate) fn diffuse_and_decay(mut chem: ResMut<ChemField>, mut ledger: ResMut<ledger::Ledger>) {
    let w = chem.width as i64;
    let h = chem.height as i64;
    let cells = (chem.width as usize) * (chem.height as usize);

    // ── DIFFUSE: per channel, in fixed order, into the zeroed scratch, then swap into the live plane. ──
    // The index IS the channel ordinal (the index-is-contract pin, inv #3) — an enumerate() would obscure it.
    #[allow(clippy::needless_range_loop)]
    for ch in 0..CHEM_CHANNELS {
        let shift = DIFFUSE_SHIFT[ch];
        let before = chem.channel_total(ch);

        // Zero the scratch (reused buffer). reset_chem_scratch zeroed it at tick start, but a second channel in
        // the same pass needs it zeroed again after the previous channel swapped it out — do it here so the pass
        // is self-contained.
        for v in &mut chem.scratch {
            *v = 0;
        }
        // Snapshot the live plane so the read is frozen while we scatter into scratch.
        let src: Vec<i32> = chem.plane(ch).to_vec();
        for cy in 0..h {
            for cx in 0..w {
                let c = (cy * w + cx) as usize;
                let cc = src[c];
                if cc == 0 {
                    continue;
                }
                // The AMOUNT sent to EACH of the up-to-4 von-Neumann neighbours (a pinned right-shift).
                let share = cc >> shift;
                // Walk the 4 neighbours in PINNED order [N, E, S, W]. In-grid → neighbour gets `share`;
                // off-grid (reflecting boundary) → the share is RETURNED TO SELF (no quantum crosses the edge).
                let neighbours: [(i64, i64); 4] =
                    [(cx, cy - 1), (cx + 1, cy), (cx, cy + 1), (cx - 1, cy)];
                for (nx, ny) in neighbours {
                    if nx >= 0 && nx < w && ny >= 0 && ny < h {
                        let n = (ny * w + nx) as usize;
                        chem.scratch[n] += share;
                    } else {
                        chem.scratch[c] += share; // reflect
                    }
                }
                // The kept remainder (floor-routing, no quantum lost): cc − 4*share stays in-cell.
                chem.scratch[c] += cc - 4 * share;
            }
        }
        // Swap scratch into the live plane (every share sent was received → exact conservation by construction).
        // Take the scratch buffer OUT (leaving an empty placeholder) so we can write it into the plane without a
        // simultaneous borrow; restore it after so the buffer is reused (no per-tick alloc).
        let buf = std::mem::take(&mut chem.scratch);
        chem.plane_mut(ch).copy_from_slice(&buf[..cells]);
        chem.scratch = buf;
        // HARD assert under determinism: diffusion moved no J across the world boundary, so Σ is unchanged.
        let after = chem.channel_total(ch);
        assert_chem_conserved(ch, before, after);
    }

    // ── DECAY (the named tap) — the ONLY chem sink, after the conserve-asserted diffusion fold. ──
    let mut decayed: i64 = 0;
    for ch in 0..CHEM_CHANNELS {
        for cell in chem.plane_mut(ch).iter_mut() {
            debug_assert!(*cell >= 0, "chem plane must be non-negative");
            let lost = *cell >> DECAY_SHIFT; // pure shift, integer, lost <= cell → never negative
            *cell -= lost;
            decayed += i64::from(lost); // milli == J 1:1 → `lost` IS joules, no conversion
        }
    }
    // The ONLY ledger movement in this system: chem dissipation → the FOURTH named tap (NOT folded into
    // `respired` — keeps respired's meaning clean + makes chem decay independently attributable).
    ledger.chem_decay += decayed;
}

/// Assert one channel's chem total is unchanged across the diffusion fold (Σ-before == Σ-after) — the binding
/// conservation contract for the reflecting stencil. HARD under `--features determinism` (the CI multi-ISA
/// legs build it), `debug_assert` otherwise — the exact [`crate::trophic::assert_flow_closes`] cfg pattern.
///
/// # Panics
/// Panics (under determinism) unless `before == after` for the channel.
fn assert_chem_conserved(ch: usize, before: i64, after: i64) {
    #[cfg(feature = "determinism")]
    assert!(
        before == after,
        "chem diffusion VIOLATED conservation on channel {ch}: Σ-before {before} != Σ-after {after} \
         (leak of {} milli-J; the reflecting stencil must move no quantum across the world boundary)",
        after - before,
    );
    #[cfg(not(feature = "determinism"))]
    debug_assert!(
        before == after,
        "chem diffusion VIOLATED conservation on channel {ch}: Σ-before {before} != Σ-after {after}",
    );
    let _ = (ch, before, after);
}

/// **EMIT CHEM** (ADR-013 F5) — organisms spend J deterministically into the field. RNG-FREE, integer. Runs
/// AFTER `mineralize` and BEFORE `reproduce_or_die`, reading start-of-tick Energy. Two paired Energy→chem
/// moves (Σ unchanged):
/// - **KIN** (channel 1): each living org spends `min(KIN_BASE, energy)` J → the kin plane 1:1 AND tags
///   [`KinProvenance`] for per-species attribution. Presence-signalling.
/// - **LIVE-DISTRESS ALARM** (channel 2): an org whose start-of-tick `Energy < ALARM_TRIGGER` spends
///   `min(ALARM_BASE, energy)` J → the alarm plane 1:1.
///
/// (TOXIN is minted inline in [`crate::metabolism`] so the respired↔toxin paired move is atomic where the
/// Defense slice is computed; DEATH-alarm rides the existing canonical death pass in `reproduce_or_die`.)
///
/// Builds ONE canonical `(cell, SpeciesId, OrgId)`-sorted row vector over the living set (the metabolism
/// collect-then-sort idiom) so within-tick emit order is fixed; per-cell [`CHEM_CAP`] saturation routes the
/// rejected part to [`ledger::Ledger::overflow`]. Mutates Energy via an OrgId-keyed map applied in a second
/// pass (never mutate-during-query — inv #3).
#[allow(clippy::type_complexity)]
pub(crate) fn emit_chem(
    mut chem: ResMut<ChemField>,
    mut kin_prov: ResMut<KinProvenance>,
    mut ledger: ResMut<ledger::Ledger>,
    mut q: Query<(&OrgId, &Species, &mut Energy, &Position)>,
) {
    let width = chem.width;
    // ── Canonical (cell, SpeciesId, OrgId) order over the LIVING set (inv #3). ──
    struct EmitRow {
        cell: u32,
        species: u16,
        org: u64,
        energy: i64,
    }
    let mut rows: Vec<EmitRow> = q
        .iter()
        .map(|(id, sp, e, p)| EmitRow {
            cell: cell_index(p, width),
            species: sp.0 .0,
            org: id.0,
            energy: e.0,
        })
        .collect();
    rows.sort_unstable_by_key(|r| (r.cell, r.species, r.org));

    // Per-org J spent (debited from Energy in a second pass). Each is a PAIRED move: the J leaves Energy and
    // appears 1:1 as a milli-unit in ChemField → Σ unchanged.
    let mut spent: std::collections::BTreeMap<u64, i64> = std::collections::BTreeMap::new();
    let mut overflow: i64 = 0;
    for r in &rows {
        let cell = r.cell as usize;
        let mut energy = r.energy;
        // KIN marker: spend min(KIN_BASE, energy) → kin plane + per-species provenance.
        let kin_spend = KIN_BASE.min(energy.max(0));
        if kin_spend > 0 {
            // milli == J 1:1, and KIN_BASE << i32::MAX so the cast is exact.
            let rejected = ChemField::deposit_capped(
                chem.plane_mut(ChemChannel::Kin as usize),
                cell,
                kin_spend as i32,
            );
            let accepted = kin_spend - i64::from(rejected);
            kin_prov.deposit(cell, r.species as usize, accepted);
            energy -= kin_spend; // the WHOLE spend leaves Energy; the rejected part is booked to overflow
            overflow += i64::from(rejected);
            *spent.entry(r.org).or_insert(0) += kin_spend;
        }
        // LIVE-DISTRESS alarm: a low-energy org signals (reads the post-kin Energy so the two spends compose).
        if energy < ALARM_TRIGGER {
            let alarm_spend = ALARM_BASE.min(energy.max(0));
            if alarm_spend > 0 {
                let rejected = ChemField::deposit_capped(
                    chem.plane_mut(ChemChannel::Alarm as usize),
                    cell,
                    alarm_spend as i32,
                );
                overflow += i64::from(rejected);
                *spent.entry(r.org).or_insert(0) += alarm_spend;
            }
        }
    }
    ledger.overflow += overflow;

    // ── Apply the Energy debits (paired move complete; never mutate-during-query — inv #3). ──
    if !spent.is_empty() {
        for (id, _sp, mut e, _p) in q.iter_mut() {
            if let Some(&debit) = spent.get(&id.0) {
                e.0 -= debit;
            }
        }
    }
}

/// **ASSERT CHEM CONSERVED (semantic)** (ADR-013 F5) — re-derives the chem book across the whole tick: the
/// current chem Σ must equal the prior Σ plus everything emitted minus everything decayed/overflowed. This is
/// the chem analogue of `assert_flow_closes` / `ledger_closes`. At F5 the per-channel diffusion assert (inside
/// [`diffuse_and_decay`]) already brackets the only step that could move a quantum unaccountably; this hook is
/// reserved as the semantic chem gate and currently asserts the cheap invariant that every chem plane is
/// non-negative (a negative cell would mean decay/withdraw underflowed). HARD under determinism. Pure read —
/// hash-neutral.
pub(crate) fn assert_chem_conserved_system(chem: Res<ChemField>) {
    let nonneg = || -> bool { (0..CHEM_CHANNELS).all(|ch| chem.plane(ch).iter().all(|&v| v >= 0)) };
    #[cfg(feature = "determinism")]
    assert!(
        nonneg(),
        "chem plane went negative — a decay/emit underflow (the conservation books cannot close)"
    );
    #[cfg(not(feature = "determinism"))]
    debug_assert!(nonneg(), "chem plane went negative");
    let _ = &chem;
}

/// The three SENSE couplings behind a trait (inv #5 — science pluggable). An in-core default impl
/// ([`InCoreChem`]) reads the org's OWN cell chem (FROZEN at start-of-tick by the caller) + its own-species
/// kin marker, returning INTEGER PERMILLE factors the demand/maintenance math folds in. A subprocess-backed
/// "realistic" impl could replace it without touching the metabolism/reproduce systems. All factors are
/// computed so a CHEM-FREE cell (all inputs 0) returns the NEUTRAL values → the byte-identical pre-F5 math
/// (the EditModifier `!= NEUTRAL` gate precedent).
pub(crate) trait ChemModifier {
    /// Toxin → uptake-suppress permille `[TOXIN_SUPPRESS_FLOOR, 1000]` (1000 = no suppression at zero toxin).
    fn toxin_suppress_permille(&self, frozen_toxin: i32) -> u64;
    /// Kin → uptake-boost permille `[1000, 1000+KIN_BOOST_CAP]` (1000 = no boost at zero kin).
    fn kin_boost_permille(&self, kin_own: i64) -> u64;
    /// Kin → maintenance-survival permille `[1000−KIN_SURVIVAL_CAP, 1000]` scaling the upkeep debit DOWN.
    fn kin_survival_permille(&self, kin_own: i64) -> u64;
    /// Toxin → lethal maintenance drain J (added to the upkeep debit), kin-spared if own kin is present.
    fn toxin_drain_j(&self, frozen_toxin: i32, kin_own: i64) -> i64;
}

/// The lightweight in-core [`ChemModifier`] default (inv #5). All-integer, no float, deterministic.
pub(crate) struct InCoreChem;

impl ChemModifier for InCoreChem {
    fn toxin_suppress_permille(&self, frozen_toxin: i32) -> u64 {
        if frozen_toxin <= 0 {
            return PERMILLE as u64; // chem-free cell → neutral 1000 → byte-identical pre-F5 demand math
        }
        let drop =
            (i64::from(frozen_toxin) * TOXIN_SUPPRESS_NUM / TOXIN_SUPPRESS_DEN).min(PERMILLE);
        ((PERMILLE - drop) as u64).max(TOXIN_SUPPRESS_FLOOR)
    }

    fn kin_boost_permille(&self, kin_own: i64) -> u64 {
        if kin_own <= 0 {
            return PERMILLE as u64; // neutral
        }
        let lift = (kin_own * KIN_BOOST_NUM / KIN_BOOST_DEN).clamp(0, KIN_BOOST_CAP as i64) as u64;
        PERMILLE as u64 + lift
    }

    fn kin_survival_permille(&self, kin_own: i64) -> u64 {
        if kin_own <= 0 {
            return PERMILLE as u64; // neutral (no upkeep discount)
        }
        let cut =
            (kin_own * KIN_BOOST_NUM / KIN_BOOST_DEN).clamp(0, KIN_SURVIVAL_CAP as i64) as u64;
        PERMILLE as u64 - cut
    }

    fn toxin_drain_j(&self, frozen_toxin: i32, kin_own: i64) -> i64 {
        if frozen_toxin <= 0 {
            return 0; // chem-free cell → no drain → byte-identical pre-F5 maintenance
        }
        let raw = i64::from(frozen_toxin) * TOXIN_DRAIN_NUM / TOXIN_DRAIN_DEN;
        if kin_own > 0 {
            // KIN-SPARING: an org tolerant of its own lineage's toxin pays a discounted drain.
            raw * KIN_SPARE_PERMILLE / PERMILLE
        } else {
            raw
        }
    }
}

/// The baked alarm-bias LUT (ADR-013 F5) — DRAW-COUNT-NEUTRAL dispersal chemotaxis. The birth path already
/// draws EXACTLY one dispersal word and maps `ddisp % 9` to a Moore step; F5 adds ZERO draws and instead
/// RE-INTERPRETS that already-drawn index. Given the raw Moore index `raw_k` (`0..9`) and the gradient
/// direction `dir` (the Moore index of the LOWEST-alarm neighbour, `0..9`), this returns the EFFECTIVE Moore
/// step — biased to FLEE the alarm. A BAKED const table so it is byte-identical cross-platform.
///
/// Construction: the effective step nudges the raw step one Moore cell toward the FLEE direction (the cell
/// OPPOSITE the highest-alarm gradient = `dir`, which is already the lowest-alarm neighbour). To keep it a
/// pure deterministic remap with no arithmetic ambiguity we blend: even raw indices keep the raw step (so
/// dispersal still explores), odd raw indices snap to the flee direction (so the population statistically
/// drifts away from stress). This is bit-reproducible and adds no RNG.
pub(crate) fn alarm_bias_step(raw_k: u64, flee_dir: u64) -> u64 {
    // Both inputs are already in 0..9 (raw_k = ddisp % 9; flee_dir is a Moore index). Snap odd draws to the
    // flee direction, keep even draws as the raw exploratory step. Pure integer, no branch on float.
    if raw_k % 2 == 1 {
        flee_dir % 9
    } else {
        raw_k % 9
    }
}

/// Compute the FLEE direction at a parent cell: the Moore index (`0..9`, `4` = stay) of the LOWEST-alarm
/// neighbour among the 8 Moore neighbours + the centre, reading the FROZEN start-of-tick alarm plane. Ties →
/// the LOWEST Moore index (bit-reproducible, no sqrt). Returns `None` when the total neighbour alarm is 0 (a
/// chem-free neighbourhood → the caller falls back to the plain `ddisp % 9` so the run is byte-identical).
pub(crate) fn flee_direction(
    frozen_alarm: &[i32],
    width: u32,
    height: u32,
    px: u32,
    py: u32,
) -> Option<u64> {
    let w = width as i64;
    let h = height as i64;
    let mut total: i64 = 0;
    let mut best_alarm = i64::MAX;
    let mut best_k: u64 = 4; // default: stay (centre)
    for k in 0u64..9 {
        let dx = (k % 3) as i64 - 1;
        let dy = (k / 3) as i64 - 1;
        let nx = px as i64 + dx;
        let ny = py as i64 + dy;
        if nx < 0 || nx >= w || ny < 0 || ny >= h {
            continue; // off-grid Moore cell contributes nothing (treated as no-information, not zero-alarm)
        }
        let a = i64::from(frozen_alarm[(ny * w + nx) as usize]);
        total += a;
        // Lowest alarm wins; ties keep the lowest Moore index (k ascends, so a strict `<` does this).
        if a < best_alarm {
            best_alarm = a;
            best_k = k;
        }
    }
    if total == 0 {
        None // chem-free neighbourhood → byte-identical fallback to plain ddisp % 9
    } else {
        Some(best_k)
    }
}

/// Read-only accessors for sensing (used by `metabolism` / `reproduce_or_die`). The frozen planes are cloned
/// by the caller at start-of-tick (the `frozen_light` discipline) so within-tick emit never affects sense.
impl ChemField {
    /// Clone the toxin plane (start-of-tick frozen snapshot for the demand/drain sense).
    pub(crate) fn frozen_toxin(&self) -> Vec<i32> {
        self.toxin.clone()
    }
    /// Clone the alarm plane (start-of-tick frozen snapshot for the flee-dispersal sense).
    pub(crate) fn frozen_alarm(&self) -> Vec<i32> {
        self.alarm.clone()
    }
    /// Off-hash render projection: nearest-cell resample of an i32 chem plane → f32 in `[0,1]` by [`CHEM_CAP`].
    /// Mirrors [`crate::pool_sample_to`] exactly (the single audited `/CHEM_CAP` display divide). Pure read.
    pub(crate) fn sample_to(
        plane: &[i32],
        pw: u32,
        ph: u32,
        tx: u32,
        ty: u32,
        target_w: u32,
        target_h: u32,
    ) -> f32 {
        let sx = ((u64::from(tx) * u64::from(pw)) / u64::from(target_w)).min(u64::from(pw) - 1);
        let sy = ((u64::from(ty) * u64::from(ph)) / u64::from(target_h)).min(u64::from(ph) - 1);
        let idx = (sy * u64::from(pw) + sx) as usize;
        (f64::from(plane[idx]) / f64::from(CHEM_CAP)) as f32
    }
    /// Read the toxin/kin/alarm planes for the render snapshot (read-only borrow; the renderer resamples them).
    pub(crate) fn render_planes(&self) -> (&[i32], &[i32], &[i32]) {
        (&self.toxin, &self.kin, &self.alarm)
    }
}

/// The Defense budget slice J for a granted total — used by `metabolism` to mint toxin. A thin re-derivation
/// of [`crate::fixed::split_budget`]'s Defense slot so the toxin mint can be computed where the convert split
/// already runs, without re-splitting.
pub(crate) fn defense_slice(split: &[i64]) -> i64 {
    split[BudgetChannel::Defense as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field_with(width: u32, height: u32, ch: usize, vals: &[(usize, i32)]) -> ChemField {
        let mut f = ChemField::zeroed(width, height);
        for &(c, v) in vals {
            f.plane_mut(ch)[c] = v;
        }
        f
    }

    /// Diffusion is mass-EXACT: Σ-before == Σ-after on every channel, for a point source AND a random-ish field.
    #[test]
    fn diffusion_conserves_mass_exactly() {
        // A single hot cell in the middle of a 5×5 grid, on every channel.
        for ch in 0..CHEM_CHANNELS {
            let mut f = field_with(5, 5, ch, &[(12, 1_000_000)]);
            let before = f.channel_total(ch);
            // Drive several diffusion steps (no decay) and assert conservation each step.
            for _ in 0..20 {
                diffuse_channel_only(&mut f, ch);
                assert_eq!(
                    f.channel_total(ch),
                    before,
                    "channel {ch} must conserve Σ across diffusion"
                );
            }
        }
    }

    /// Reflecting boundary: a hot EDGE/CORNER cell loses no quantum to the world edge.
    #[test]
    fn diffusion_reflecting_boundary_conserves() {
        // corner (0,0), edge-mid (top row), on the toxin channel (small shift → big shares → stresses the edge).
        for &cell in &[0usize, 2, 4, 20, 24] {
            let mut f = field_with(5, 5, 0, &[(cell, 777_777)]);
            let before = f.channel_total(0);
            for _ in 0..15 {
                diffuse_channel_only(&mut f, 0);
                assert_eq!(f.channel_total(0), before, "edge/corner cell {cell} leaked");
            }
        }
    }

    /// Diffusion SPREADS a point source to its neighbours (it is not a no-op when the shift permits a share).
    #[test]
    fn diffusion_spreads_to_neighbours() {
        let mut f = field_with(5, 5, 0, &[(12, 1_000_000)]);
        diffuse_channel_only(&mut f, 0);
        // The four von-Neumann neighbours of cell 12 (centre) are 7, 11, 13, 17.
        for &n in &[7usize, 11, 13, 17] {
            assert!(
                f.plane(0)[n] > 0,
                "neighbour {n} must have received a share"
            );
        }
        // The centre kept the remainder (still the largest).
        assert!(f.plane(0)[12] > 0);
    }

    /// Decay removes exactly `Σ(plane >> DECAY_SHIFT)` and books it to the chem_decay tap; never negative.
    #[test]
    fn decay_is_a_named_tap_and_never_negative() {
        let mut f = field_with(4, 4, 0, &[(0, 64_000), (5, 6_300), (10, 1)]);
        let before = f.channel_total(0);
        let mut decayed = 0i64;
        for c in 0..16 {
            let plane = f.plane_mut(0);
            let lost = plane[c] >> DECAY_SHIFT;
            plane[c] -= lost;
            decayed += i64::from(lost);
            assert!(f.plane(0)[c] >= 0);
        }
        assert_eq!(f.channel_total(0), before - decayed);
        // 64_000 >> 6 == 1000; 6_300 >> 6 == 98; 1 >> 6 == 0 (so it contributes nothing).
        assert_eq!(decayed, 1000 + 98);
    }

    /// `total()` widens i32→i64 and sums all three planes exactly.
    #[test]
    fn total_widens_and_sums_all_planes() {
        let mut f = ChemField::zeroed(2, 2);
        f.plane_mut(0)[0] = 100;
        f.plane_mut(1)[1] = 200;
        f.plane_mut(2)[2] = 300;
        assert_eq!(f.total(), 600);
    }

    /// deposit_capped routes the over-cap part out as overflow (never silently clamps).
    #[test]
    fn deposit_capped_routes_overflow() {
        let mut f = ChemField::zeroed(1, 1);
        f.plane_mut(0)[0] = CHEM_CAP - 10;
        let rejected = ChemField::deposit_capped(f.plane_mut(0), 0, 100);
        assert_eq!(f.plane(0)[0], CHEM_CAP, "filled to the cap");
        assert_eq!(rejected, 90, "the over-cap part is returned, not dropped");
    }

    /// KinProvenance attributes per-species and reads own-species back.
    #[test]
    fn kin_provenance_per_species() {
        let mut p = KinProvenance::new(4, 3);
        p.deposit(2, 1, 500);
        p.deposit(2, 1, 50);
        p.deposit(2, 0, 7);
        assert_eq!(p.own(2, 1), 550);
        assert_eq!(p.own(2, 0), 7);
        assert_eq!(p.own(2, 2), 0);
        assert_eq!(p.own(2, 9), 0, "out-of-range species reads 0");
    }

    /// SENSE: a chem-free cell returns NEUTRAL factors (the byte-identical pre-F5 gate).
    #[test]
    fn chem_free_cell_is_neutral() {
        let m = InCoreChem;
        assert_eq!(m.toxin_suppress_permille(0), PERMILLE as u64);
        assert_eq!(m.kin_boost_permille(0), PERMILLE as u64);
        assert_eq!(m.kin_survival_permille(0), PERMILLE as u64);
        assert_eq!(m.toxin_drain_j(0, 0), 0);
    }

    /// SENSE: toxin suppresses (factor < 1000) but never below the floor; drain rises with toxin.
    #[test]
    fn toxin_suppresses_and_drains() {
        let m = InCoreChem;
        let s = m.toxin_suppress_permille(320_000); // 320_000 / TOXIN_SUPPRESS_DEN permille drop
        assert!(s < PERMILLE as u64 && s >= TOXIN_SUPPRESS_FLOOR);
        // Saturating toxin floors the suppress factor.
        assert_eq!(m.toxin_suppress_permille(i32::MAX), TOXIN_SUPPRESS_FLOOR);
        // Drain scales with toxin and is kin-spared when own kin is present.
        let full = m.toxin_drain_j(40_000_000, 0);
        let spared = m.toxin_drain_j(40_000_000, 1);
        assert!(full > 0);
        assert!(
            spared < full,
            "own-species kin spares the lineage from its own toxin"
        );
        assert_eq!(spared, full * KIN_SPARE_PERMILLE / PERMILLE);
    }

    /// SENSE: kin boosts uptake (>1000, capped) and lowers upkeep (<1000), both gated on non-zero kin.
    #[test]
    fn kin_boosts_and_lowers_upkeep() {
        let m = InCoreChem;
        let boost = m.kin_boost_permille(400_000); // 400_000/4000 = 100 permille lift
        assert!(boost > PERMILLE as u64);
        assert_eq!(
            m.kin_boost_permille(i64::MAX),
            PERMILLE as u64 + KIN_BOOST_CAP
        );
        let surv = m.kin_survival_permille(400_000);
        assert!(surv < PERMILLE as u64);
        assert_eq!(
            m.kin_survival_permille(i64::MAX),
            PERMILLE as u64 - KIN_SURVIVAL_CAP
        );
    }

    /// ALARM: flee_direction returns None on a chem-free neighbourhood (the byte-identical fallback), and the
    /// LOWEST-alarm Moore index otherwise (ties → lowest index).
    #[test]
    fn flee_direction_picks_lowest_alarm() {
        // 3×3 grid, all zero → None.
        let zero = vec![0i32; 9];
        assert_eq!(flee_direction(&zero, 3, 3, 1, 1), None);
        // High alarm to the EAST of centre (cell idx 5), low everywhere else → flee NOT east.
        let mut a = vec![10i32; 9];
        a[5] = 9000; // east neighbour of centre (1,1)
        a[3] = 0; // west neighbour is the lowest
        let dir = flee_direction(&a, 3, 3, 1, 1).unwrap();
        // Moore index of the WEST neighbour relative to centre: dx=-1,dy=0 → k = (dy+1)*3 + (dx+1) = 1*3+0 = 3.
        assert_eq!(dir, 3, "flee toward the lowest-alarm (west) cell");
    }

    /// ALARM: the bias step is draw-count-neutral — it only remaps an already-drawn word, deterministically.
    #[test]
    fn alarm_bias_is_a_pure_remap() {
        // Even raw indices keep the exploratory step; odd snap to the flee direction.
        assert_eq!(alarm_bias_step(4, 3), 4); // even → raw
        assert_eq!(alarm_bias_step(2, 7), 2); // even → raw
        assert_eq!(alarm_bias_step(5, 3), 3); // odd → flee
        assert_eq!(alarm_bias_step(7, 0), 0); // odd → flee
                                              // Deterministic: same inputs, same output, always.
        assert_eq!(alarm_bias_step(5, 3), alarm_bias_step(5, 3));
    }

    // ── test helper: run ONE diffusion step on a single channel, no decay (mirrors diffuse_and_decay's fold). ──
    fn diffuse_channel_only(f: &mut ChemField, ch: usize) {
        let w = f.width as i64;
        let h = f.height as i64;
        let cells = (f.width as usize) * (f.height as usize);
        for v in &mut f.scratch {
            *v = 0;
        }
        let src: Vec<i32> = f.plane(ch).to_vec();
        let shift = DIFFUSE_SHIFT[ch];
        for cy in 0..h {
            for cx in 0..w {
                let c = (cy * w + cx) as usize;
                let cc = src[c];
                if cc == 0 {
                    continue;
                }
                let share = cc >> shift;
                let neighbours: [(i64, i64); 4] =
                    [(cx, cy - 1), (cx + 1, cy), (cx, cy + 1), (cx - 1, cy)];
                for (nx, ny) in neighbours {
                    if nx >= 0 && nx < w && ny >= 0 && ny < h {
                        f.scratch[(ny * w + nx) as usize] += share;
                    } else {
                        f.scratch[c] += share;
                    }
                }
                f.scratch[c] += cc - 4 * share;
            }
        }
        let buf = std::mem::take(&mut f.scratch);
        f.plane_mut(ch).copy_from_slice(&buf[..cells]);
        f.scratch = buf;
    }
}
