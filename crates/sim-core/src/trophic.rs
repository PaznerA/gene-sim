//! The obligate trophic loop + decomposer mineralization + the MEASURED `FlowMatrix` (ADR-013 F4).
//!
//! F4 closes the one tap F3 left open: the `free_nutrient` INFLUX arm is DELETED ([`crate::solar_influx`] no
//! longer mints it) and `free_nutrient` becomes ENDOGENOUS — supplied ONLY by decomposer mineralization of
//! shed detritus. The obligate cycle:
//!
//! 1. **PLANTS deplete + shed.** Autotrophs draw `free_nutrient` (via `affinity[1]`) in [`crate::metabolism`]
//!    and shed detritus on TWO arms: the F3 carcass→detritus on death PLUS a continuous LITTERFALL fraction of
//!    the excrete step every tick.
//! 2. **DECOMPOSERS mineralize.** [`mineralize`] (this module) runs AFTER metabolism: a Decomposer taps
//!    `PoolStock[cell].detritus` via `affinity[2]` against the FROZEN start-of-tick snapshot, apportions
//!    co-located decomposers' shares via [`fixed::apportion`](crate::fixed::apportion), and SPLITS the granted
//!    J — the Maintenance/Defense budget slice is RESPIRED (its own metabolism), the rest is split by the
//!    gene-driven `Strategy.mineralize_rate` (pta/AcetateOverflow-anchored): that permille → `free_nutrient`,
//!    the residual → RESPIRED. A paired detritus-debit / (free_nutrient-credit + RESPIRED-tap) move — conserves
//!    J exactly, so [`crate::ledger`] still closes every tick.
//! 3. **PLANTS re-uptake next tick** → the loop. Kill the decomposer ⇒ detritus piles up as a dead sink,
//!    `free_nutrient` drains to 0, plants fall below the maintenance floor and crash.
//!
//! The [`FlowMatrix`] is the MEASURED inversion of the retired ADR-014 fabricated cosine: every conserved J
//! transfer that crosses a species boundary is RECORDED at the moment it happens, keyed by (source, dest)
//! `SpeciesId`. Provenance is integer ([`PoolProvenance`] tracks the per-cell species composition of the two
//! biotic pools), apportioned by [`fixed::apportion`](crate::fixed::apportion). Each credit `A[i][j] += x`
//! carries a paired self-debit `A[i][i] -= x`, so **every row sums to 0 by construction** (asserted via
//! [`flow_matrix_rows_sum_to_zero`]). All `i64`, no float, no `HashMap` (inv #3); walked in canonical
//! `(cell, SpeciesId, OrgId)` order. The matrix is HASH-FOLDED at F4 (a fixed-order fold in `hash_world`) but
//! it is a measurement derived from already-hashed pools/orgs.

use bevy_ecs::prelude::*;

use crate::fixed;
use crate::gp::{self, BudgetChannel, TrophicRole};
use crate::{
    cell_index, credit_capped, ledger, Biomass, Energy, OrgId, Position, Species, SpeciesRegistry,
    BODY_FACTOR_FLOOR, ENERGY_CAP, MAINTENANCE_FLOOR, POOL_CAP,
};

/// The MEASURED S×S net-integer J flow matrix for the current generation (ADR-013 F4). Row-major:
/// `j[i*s + j_]` = NET joules that flowed FROM species `j_` INTO species `i` this tick. `s` is the
/// [`SpeciesRegistry`] length (indices ARE registry ordinals = `SpeciesId`). RESET to zero at the start of
/// every tick (per-generation flow, not cumulative). Every off-diagonal credit `A[i][j] += x` carries a
/// paired diagonal self-debit `A[i][i] -= x`, so `Σ_j A[i*s + j] == 0` for every row `i`. All `i64`.
#[derive(Resource, Debug, Clone, PartialEq, Eq, Default)]
pub struct FlowMatrix {
    s: usize,
    j: Vec<i64>,
}

impl FlowMatrix {
    /// A zeroed `s × s` matrix.
    #[must_use]
    pub(crate) fn zeroed(s: usize) -> Self {
        Self {
            s,
            j: vec![0i64; s * s],
        }
    }

    /// Reset every entry to zero (start-of-tick). Keeps `s`.
    pub(crate) fn reset(&mut self) {
        for v in &mut self.j {
            *v = 0;
        }
    }

    /// Species count (matrix dimension).
    #[must_use]
    pub fn s(&self) -> usize {
        self.s
    }

    /// The flat row-major `s*s` net-J slice (read-only; the renderer's `flow_matrix()` contract).
    #[must_use]
    pub fn flat(&self) -> &[i64] {
        &self.j
    }

    /// Record a conserved cross-species transfer: species `dest` GAINED `amount` J attributable to species
    /// `src`. The diagonal-pairing identity — `A[dest][src] += amount; A[dest][dest] -= amount` — keeps the
    /// `dest` row summing to zero. A `src == dest` (self-sourced) transfer or a non-positive amount records
    /// nothing (no spurious edge). Indices are `SpeciesId` ordinals; out-of-range is ignored defensively.
    pub(crate) fn record(&mut self, dest: usize, src: usize, amount: i64) {
        if amount <= 0 || dest == src || dest >= self.s || src >= self.s {
            return;
        }
        self.j[dest * self.s + src] += amount;
        self.j[dest * self.s + dest] -= amount;
    }
}

/// Per-cell, per-species composition of the two BIOTIC pools (ADR-013 F4 provenance) — the integer mechanism
/// the [`FlowMatrix`] attributes flow over. Flat `[cell * s + species]`. PERSISTS across ticks (a carcass shed
/// this tick feeds the decomposer next tick — the obligate loop's cross-tick lag). The seed-abiotic portion of
/// a pool is NOT tracked here (so withdrawing it records no flow): `Σ_species detritus_by_species[cell] <=
/// PoolStock.detritus[cell]`. All `i64`, indexed never iterated as a `HashMap` (inv #3).
#[derive(Resource, Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PoolProvenance {
    s: usize,
    /// Species-attributed detritus per cell (deposited by carcasses + litterfall).
    detritus_by_species: Vec<i64>,
    /// Species-attributed free_nutrient per cell (minted by decomposer mineralization).
    nutrient_by_species: Vec<i64>,
    /// Reusable per-withdraw apportion scratch (perf optimization, hash-neutral): the `weights`, `shares`, and
    /// largest-remainder `rem` buffers are owned here and reused across every [`withdraw`](Self::withdraw) call
    /// so the per-share provenance apportion pays no per-call heap allocation. Internal scratch — zeroed/cleared
    /// before each use, never read as state, never hashed.
    #[allow(clippy::type_complexity)]
    scratch_w: Vec<u64>,
    scratch_s: Vec<i64>,
    scratch_rem: Vec<(u128, usize)>,
}

impl PoolProvenance {
    /// A zeroed provenance ledger for `cells` cells and `s` species.
    pub(crate) fn new(cells: usize, s: usize) -> Self {
        Self {
            s,
            detritus_by_species: vec![0i64; cells * s],
            nutrient_by_species: vec![0i64; cells * s],
            scratch_w: Vec::new(),
            scratch_s: Vec::new(),
            scratch_rem: Vec::new(),
        }
    }

    /// Attribute `amount` (>0) of a deposit to (`cell`, `species`) in `plane` (0 = detritus, 1 = nutrient).
    fn deposit(plane: &mut [i64], s: usize, cell: usize, species: usize, amount: i64) {
        if amount > 0 && species < s {
            plane[cell * s + species] += amount;
        }
    }

    /// Record a detritus deposit by `species` into `cell` (carcass or litterfall).
    pub(crate) fn deposit_detritus(&mut self, cell: usize, species: usize, amount: i64) {
        Self::deposit(&mut self.detritus_by_species, self.s, cell, species, amount);
    }

    /// Record a free_nutrient mint by `species` into `cell` (decomposer mineralization).
    pub(crate) fn deposit_nutrient(&mut self, cell: usize, species: usize, amount: i64) {
        Self::deposit(&mut self.nutrient_by_species, self.s, cell, species, amount);
    }

    /// Withdraw `withdrawn` (>0) J of a pool from `cell`, apportioning it over the species that composed that
    /// cell's biotic stock (largest-remainder, ties→lowest index — the canonical apportion), decrementing each
    /// source's slot and recording `flow[dest][src] += share` for each biotic source. The UNATTRIBUTED
    /// remainder (abiotic seed) records no flow. `plane` selects detritus (0) / nutrient (1). Pure integer,
    /// ordered by species index (inv #3).
    #[allow(clippy::too_many_arguments)]
    fn withdraw(
        plane: &mut [i64],
        s: usize,
        cell: usize,
        dest_species: usize,
        withdrawn: i64,
        flow: &mut FlowMatrix,
        weights: &mut Vec<u64>,
        shares: &mut Vec<i64>,
        rem: &mut Vec<(u128, usize)>,
    ) {
        if withdrawn <= 0 || s == 0 {
            return;
        }
        let base = cell * s;
        // Biotic stock available in this cell, in species order (the canonical apportion index, inv #3).
        weights.clear();
        weights.extend((0..s).map(|sp| plane[base + sp].max(0) as u64));
        let biotic_total: i64 = weights.iter().map(|&w| w as i64).sum();
        if biotic_total <= 0 {
            return; // all abiotic seed → no species provenance → no flow recorded
        }
        // Apportion only the BIOTIC fraction of the withdrawal (capped at the biotic stock); the rest is
        // abiotic and carries no edge. Buffer-reusing `apportion_into` is bit-identical to `apportion`.
        let attributable = withdrawn.min(biotic_total);
        shares.resize(s, 0);
        fixed::apportion_into(attributable, weights, shares, rem);
        for (sp, share) in shares.iter().enumerate() {
            if *share <= 0 {
                continue;
            }
            plane[base + sp] -= *share; // drain the source's attributed stock
            flow.record(dest_species, sp, *share); // dest GAINED *share attributable to sp
        }
    }

    /// Withdraw detritus from `cell` for a decomposer of `dest_species`, recording provenance flow.
    pub(crate) fn withdraw_detritus(
        &mut self,
        cell: usize,
        dest_species: usize,
        withdrawn: i64,
        flow: &mut FlowMatrix,
    ) {
        Self::withdraw(
            &mut self.detritus_by_species,
            self.s,
            cell,
            dest_species,
            withdrawn,
            flow,
            &mut self.scratch_w,
            &mut self.scratch_s,
            &mut self.scratch_rem,
        );
    }

    /// Withdraw free_nutrient from `cell` for a plant of `dest_species`, recording provenance flow.
    pub(crate) fn withdraw_nutrient(
        &mut self,
        cell: usize,
        dest_species: usize,
        withdrawn: i64,
        flow: &mut FlowMatrix,
    ) {
        Self::withdraw(
            &mut self.nutrient_by_species,
            self.s,
            cell,
            dest_species,
            withdrawn,
            flow,
            &mut self.scratch_w,
            &mut self.scratch_s,
            &mut self.scratch_rem,
        );
    }
}

/// A decomposer's `mineralize` row, snapshotted in canonical `(cell, SpeciesId, OrgId)` order so the per-cell
/// detritus contention + the flow recording are order-independent of ECS query order (inv #3).
struct MineralizeRow {
    cell: u32,
    species: u16,
    org: u64,
    body: i64,
}

/// **MINERALIZE** (ADR-013 F4 KEYSTONE) — the decomposer detritus→free_nutrient loop, run AFTER
/// [`crate::metabolism`] so decomposers tap the SAME frozen-snapshot detritus plants/carcasses fed, and BEFORE
/// `reproduce_or_die`. RNG-free, all `i64`.
///
/// Per cell (canonical `(cell, SpeciesId, OrgId)` order):
/// 1. each co-located Decomposer DEMANDS detritus via a Monod tap on its `affinity[2]` against the FROZEN
///    start-of-tick detritus stock, body-scaled;
/// 2. the cell's available detritus is APPORTIONED across demanders ([`fixed::apportion`], ties→lowest index);
/// 3. each grant is SPLIT: the Maintenance+Defense budget slice is RESPIRED (the decomposer's own metabolism);
///    of the remainder, `mineralize_rate` permille → the SAME cell's `free_nutrient` (a MINT, provenance
///    tagged), the residual → RESPIRED. Paired detritus-debit / (free_nutrient-credit + RESPIRED) — conserves
///    J, so `assert_ledger_closes` holds.
/// 4. the harvested detritus J is attributed via [`PoolProvenance`] → [`FlowMatrix`] `flow[decomposer][plant]`.
#[allow(clippy::type_complexity)]
pub(crate) fn mineralize(
    registry: Res<SpeciesRegistry>,
    edit_mod: Res<crate::EditModifierRes>,
    mut pools: ResMut<crate::PoolStock>,
    mut prov: ResMut<PoolProvenance>,
    mut flow: ResMut<FlowMatrix>,
    mut ledger: ResMut<ledger::Ledger>,
    q: Query<(&OrgId, &Species, &Biomass, &Position)>,
) {
    let width = pools.width;
    // ── Canonical order over the LIVING Decomposer set (inv #3). Non-decomposers contribute nothing here. ──
    let mut rows: Vec<MineralizeRow> = q
        .iter()
        .filter_map(|(id, sp, biomass, p)| {
            let strat = &registry.entries[sp.0 .0 as usize].strategy;
            if strat.role != TrophicRole::Decomposer {
                return None;
            }
            Some(MineralizeRow {
                cell: cell_index(p, width),
                species: sp.0 .0,
                org: id.0,
                body: biomass.0.max(crate::OFFSPRING_SEED_BIOMASS),
            })
        })
        .collect();
    if rows.is_empty() {
        return;
    }
    rows.sort_unstable_by_key(|r| (r.cell, r.species, r.org));

    // ── Pass 1: per-decomposer DEMAND against the FROZEN start-of-tick detritus stock. ──
    let frozen_detritus = pools.detritus.clone();
    let n = rows.len();
    let mut demand = vec![0i64; n];
    for (i, r) in rows.iter().enumerate() {
        let strat = &registry.entries[r.species as usize].strategy;
        let cell = r.cell as usize;
        // demand_permille = affinity[detritus] · body, both on permille grids (the metabolism demand shape).
        let aff_permille = (u64::from(strat.affinity[2]) * u64::from(fixed::PERMILLE))
            / u64::from(fixed::UNIT_SCALE);
        let body_factor = ((r.body as u128 * u128::from(fixed::PERMILLE))
            / (crate::BIOMASS_CAP as u128))
            .min(1000) as u64;
        let p = u64::from(fixed::PERMILLE);
        let dp = aff_permille * body_factor / p;
        demand[i] = crate::monod_demand(frozen_detritus[cell], dp.min(p));
    }

    // ── Pass 2: per-cell APPORTION available detritus across co-located decomposers (canonical order). ──
    // Reuse the `weights`/`shares`/`rem_scratch` buffers across every cell group (hash-neutral: `apportion_into`
    // is bit-identical to `apportion`; only the buffer ownership moved out of the loop).
    let mut granted = vec![0i64; n];
    let mut weights: Vec<u64> = Vec::new();
    let mut shares: Vec<i64> = Vec::new();
    let mut rem_scratch: Vec<(u128, usize)> = Vec::new();
    let mut i = 0usize;
    while i < n {
        let cell = rows[i].cell;
        let mut jj = i;
        while jj < n && rows[jj].cell == cell {
            jj += 1;
        }
        let group = jj - i;
        weights.clear();
        weights.extend((i..jj).map(|k| demand[k].max(0) as u64));
        let total_demand: i64 = weights.iter().map(|&w| w as i64).sum();
        if total_demand > 0 {
            let cellu = cell as usize;
            let available = pools.detritus[cellu].min(total_demand);
            shares.resize(group, 0);
            fixed::apportion_into(available, &weights, &mut shares, &mut rem_scratch);
            let mut taken = 0i64;
            for (k, share) in shares.iter().enumerate() {
                granted[i + k] = *share;
                taken += *share;
            }
            pools.detritus[cellu] -= taken; // decrement the live detritus pool ONCE
        }
        i = jj;
    }

    // ── Pass 3: SPLIT each grant (respire maint/defense + (1−mineralize_rate) residual; mint the rest as
    //    free_nutrient), record provenance flow. Canonical order; integer; conserves J. ──
    // Reuse the convert-split buffers across decomposers (perf, hash-neutral: `split_budget_into` is bit-identical).
    let mut split: Vec<i64> = Vec::new();
    let mut split_w: Vec<u64> = Vec::new();
    let mut split_rem: Vec<(u128, usize)> = Vec::new();
    for (idx, r) in rows.iter().enumerate() {
        let g = granted[idx];
        if g <= 0 {
            continue;
        }
        let cellu = r.cell as usize;
        let strat = &registry.entries[r.species as usize].strategy;
        // Maintenance + Defense slices are the decomposer's OWN metabolism → respired (split_budget conserves).
        fixed::split_budget_into(g, &strat.budget, &mut split, &mut split_w, &mut split_rem);
        let respired_meta =
            split[BudgetChannel::Maintenance as usize] + split[BudgetChannel::Defense as usize];
        let remainder = g - respired_meta; // >= 0 (split_budget conserves; both slices <= g)
                                           // Of the remainder, mineralize_rate permille → free_nutrient; the residual is respired inefficiency.
        let mut mineralized = ((remainder as u128 * u128::from(strat.mineralize_rate))
            / u128::from(fixed::PERMILLE)) as i64;
        // ADR-017 S6: a committed deep edit also throttles MINERALIZATION (a gltA/TCA KO impairs the carbon
        // processing that mints free_nutrient — the second F4 seam in the proposal). The SAME per-species
        // `[0.5,1.5]` factor scales the mint; the throttled J falls back into `respired_residual` (computed below
        // as the remainder minus the reduced mint), so the books still close. GATED on non-neutral so a no-edit
        // run is byte-identical (the mineralize mint is unchanged → the pinned hash is unmoved).
        let edit_factor_q = edit_mod.factor_q(r.species);
        if edit_factor_q != crate::EDIT_FACTOR_NEUTRAL_Q {
            mineralized = ((mineralized as u128 * u128::from(edit_factor_q))
                / u128::from(fixed::PERMILLE)) as i64;
        }
        let respired_residual = remainder - mineralized;

        // Mint mineralized J into the SAME cell's free_nutrient, capped → overflow (never silent clamp).
        let headroom = (POOL_CAP - pools.free_nutrient[cellu]).max(0);
        let accepted = mineralized.min(headroom);
        pools.free_nutrient[cellu] += accepted;
        // Provenance: this decomposer minted `accepted` J of free_nutrient in this cell (plants will draw it,
        // attributing flow[plant][decomposer] next tick).
        prov.deposit_nutrient(cellu, r.species as usize, accepted);
        let mint_overflow = mineralized - accepted;

        // RESPIRED tap: maint/defense + the mineralization-inefficiency residual + any cap overflow on the mint
        // is routed to OVERFLOW (so the books net out). detritus_debited == accepted + respired + overflow.
        ledger.respired += respired_meta + respired_residual;
        ledger.overflow += mint_overflow;

        // FlowMatrix: this decomposer HARVESTED `g` of detritus — attribute it over the species that deposited
        // this cell's detritus (carcasses + litterfall). flow[decomposer][plant] += attributed share.
        prov.withdraw_detritus(cellu, r.species as usize, g, &mut flow);
    }
}

/// Assert every row of the [`FlowMatrix`] sums to zero — the relation-conservation analogue of
/// `ledger_closes` (ADR-013 F4). A structural integer identity (the diagonal self-sink absorbs the row's net),
/// so a flow that doesn't balance is a bug, not a re-pin. HARD under `--features determinism`, `debug_assert`
/// otherwise. Pure read — hash-neutral.
fn assert_flow_rows_sum_zero(flow: &FlowMatrix) {
    let s = flow.s;
    for i in 0..s {
        let row_sum: i64 = (0..s).map(|j| flow.j[i * s + j]).sum();
        assert!(
            row_sum == 0,
            "flow_matrix_rows_sum_to_zero VIOLATED: row {i} sums to {row_sum} (must be 0 by the \
             diagonal-pairing construction)"
        );
    }
}

/// **RESET FLOW** (ADR-013 F4) — zero the [`FlowMatrix`] at the START of every tick (per-generation flow). The
/// FIRST system after `advance_tick`, before any transfer accumulates into it.
pub(crate) fn reset_flow(mut flow: ResMut<FlowMatrix>) {
    flow.reset();
}

// ── ADR-013 F6: the Bdellovibrio PREDATION interaction kernel — the first org-eats-org J flow ────────────────
//
// A `Predator` (Bdellovibrio) earns its joules ONLY here (its role taps no abiotic pool in `metabolism`). The
// kernel is modeled cell-for-cell on the proven `mineralize` skeleton: a FROZEN start-of-tick prey census,
// per-cell `apportion` of predator demand, and a paired conserved transfer (prey J → predator − efficiency-tax
// → respired). It DEBITS prey Energy/Biomass, marks-and-deposits its own kills' carcass residual + despawns
// them (predation runs AFTER reproduce_or_die, so a prey drained to 0 by predation would otherwise survive as a
// 0-J zombie). RNG-free (DrawCount untouched → births stay the sole RNG consumer), all `i64`, no `HashMap`
// iterated (BTreeMap for the org-keyed prey-debit set), walked in canonical `(cell, SpeciesId, OrgId)` order
// (inv #3). On a predator-free roster the predator row vector is empty → early `return` (the mineralize no-op
// precedent) → it moves no value and draws nothing → HASH-NEUTRAL on the pinned plant run.

/// A predator's `predation` row, snapshotted in canonical `(cell, SpeciesId, OrgId)` order.
struct PredatorRow {
    cell: u32,
    species: u16,
    org: u64,
    body: i64,
}

/// A prey org's frozen consumable-J row, snapshotted in canonical `(cell, SpeciesId, OrgId)` order BEFORE any
/// predator acts (within-tick kills never re-feed this tick's demand — the `frozen_detritus` discipline).
struct PreyRow {
    cell: u32,
    species: u16,
    org: u64,
    entity: Entity,
    /// Frozen `Energy + Biomass` (its consumable J) at start-of-system.
    frozen_j: i64,
    energy: i64,
    biomass: i64,
}

/// A pending debit applied to a prey org after the canonical walk (never mutate-during-query — the
/// reproduce_or_die discipline). `eaten >= 0`; the prey loses `eaten` J Energy-first-then-Biomass.
struct PreyDebit {
    eaten: i64,
    dead: bool,
}

/// **PREDATION** (ADR-013 F6 KEYSTONE) — Bdellovibrio consume co-located prey J on a FROZEN start-of-tick census,
/// apportioned across co-located predators + across prey orgs (largest-remainder, ties→lowest canonical index).
/// Runs AFTER `reproduce_or_die`, BEFORE the trailing asserts. RNG-free, all `i64`, no `HashMap` iterated.
///
/// FOUR PASSES (deterministic, canonical `(cell, SpeciesId, OrgId)` order):
/// 1. FREEZE the start-of-tick prey census per cell (`frozen_prey_j[cell]` = Σ prey `Energy+Biomass`) + the
///    per-cell ordered prey-org list; build the predator row vector. Empty predators → early `return` (no-op).
/// 2. DEMAND per predator: Monod on `frozen_prey_j[cell]` (a Holling Type-II saturating functional response),
///    folding `predation_rate · body_factor · edit_factor` into ONE demand permille (EditModifier GATED on
///    non-neutral so a no-edit run is byte-identical).
/// 3. APPORTION the cell's available frozen prey J across co-located predators ([`fixed::apportion_into`]).
/// 4. CONSUME + ATTRIBUTE: split each predator's grant across the cell's prey orgs by frozen J; debit prey
///    Energy-first-then-Biomass; predator GAINS `kept = granted · EFF_NUM/EFF_DEN` (the existing 7/10 trophic
///    efficiency), the tax `granted − kept` → respired; record `flow[predator][prey] += kept` per prey species.
///    A fully-eaten prey (J → 0) is despawned; a partial bite leaving a sub-floor residual deposits the carcass
///    to the cell `detritus` pool (the reproduce_or_die carcass→detritus path) — so a kill leaves a species-
///    tagged carcass the decomposers later mineralize (second-order loop closure).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn predation(
    registry: Res<SpeciesRegistry>,
    edit_mod: Res<crate::EditModifierRes>,
    mut commands: Commands,
    mut pools: ResMut<crate::PoolStock>,
    mut prov: ResMut<PoolProvenance>,
    mut flow: ResMut<FlowMatrix>,
    mut ledger: ResMut<ledger::Ledger>,
    mut q: Query<(
        Entity,
        &OrgId,
        &Species,
        &mut Energy,
        &mut Biomass,
        &Position,
    )>,
) {
    let width = pools.width;
    // ── Pass 1a: build the canonical PREDATOR row vector. A predator-free roster → empty → early return (the
    //    mineralize `if rows.is_empty() { return }` no-op precedent; the hash-neutrality anchor). ──
    let mut predators: Vec<PredatorRow> = q
        .iter()
        .filter_map(|(_e, id, sp, _en, biomass, p)| {
            let strat = &registry.entries[sp.0 .0 as usize].strategy;
            if strat.role != TrophicRole::Predator {
                return None;
            }
            Some(PredatorRow {
                cell: cell_index(p, width),
                species: sp.0 .0,
                org: id.0,
                body: biomass.0.max(crate::OFFSPRING_SEED_BIOMASS),
            })
        })
        .collect();
    if predators.is_empty() {
        return;
    }
    predators.sort_unstable_by_key(|r| (r.cell, r.species, r.org));

    // ── Pass 1b: FREEZE the start-of-tick prey census in canonical order (BEFORE any predator acts). Within-tick
    //    kills never re-feed this tick's demand (the `frozen_detritus = pools.detritus.clone()` discipline). ──
    let mut prey: Vec<PreyRow> = q
        .iter()
        .filter_map(|(e, id, sp, energy, biomass, p)| {
            let strat = &registry.entries[sp.0 .0 as usize].strategy;
            if !gp::is_prey(strat.role) {
                return None;
            }
            let frozen_j = energy.0.max(0) + biomass.0.max(0);
            if frozen_j <= 0 {
                return None;
            }
            Some(PreyRow {
                cell: cell_index(p, width),
                species: sp.0 .0,
                org: id.0,
                entity: e,
                frozen_j,
                energy: energy.0,
                biomass: biomass.0,
            })
        })
        .collect();
    if prey.is_empty() {
        return; // predators present but nothing to eat this tick — no transfer, no-op.
    }
    prey.sort_unstable_by_key(|r| (r.cell, r.species, r.org));

    // Per-cell frozen prey J (the Monod stock S). Indexed by cell (a dense Vec, never a HashMap — inv #3).
    let cells = (pools.width as usize) * (pools.height as usize);
    let mut frozen_prey_j = vec![0i64; cells];
    for r in &prey {
        frozen_prey_j[r.cell as usize] += r.frozen_j;
    }

    // ── Pass 2: per-predator DEMAND (Monod on the frozen prey J → Holling Type-II). ──
    let n = predators.len();
    let mut demand = vec![0i64; n];
    for (i, r) in predators.iter().enumerate() {
        let strat = &registry.entries[r.species as usize].strategy;
        let cell = r.cell as usize;
        // demand_permille = predation_rate · body, both on permille grids (the metabolism/mineralize demand shape).
        let rate_permille = (u64::from(strat.predation_rate) * u64::from(fixed::PERMILLE))
            / u64::from(fixed::UNIT_SCALE);
        let body_factor = (((r.body as u128 * u128::from(fixed::PERMILLE))
            / (crate::BIOMASS_CAP as u128))
            .min(1000) as u64)
            .max(BODY_FACTOR_FLOOR);
        let p = u64::from(fixed::PERMILLE);
        let mut dp = rate_permille * body_factor / p;
        // ADR-017 S6 / F6 OVERSIGHT: a committed edit on the PREDATOR throttles its attack rate (a `hit`-locus
        // knockdown). The SAME per-species `[0.5,1.5]` factor scales the demand. GATED on non-neutral so a
        // no-edit run is byte-identical (the demand is unchanged → the predator-free pinned hash is unmoved).
        let edit_factor_q = edit_mod.factor_q(r.species);
        if edit_factor_q != crate::EDIT_FACTOR_NEUTRAL_Q {
            dp = dp * u64::from(edit_factor_q) / u64::from(fixed::PERMILLE);
        }
        demand[i] = crate::monod_demand(frozen_prey_j[cell], dp.min(p));
    }

    // ── Pass 3: per-cell APPORTION the available frozen prey J across co-located predators (canonical order). ──
    let mut granted = vec![0i64; n];
    let mut weights: Vec<u64> = Vec::new();
    let mut shares: Vec<i64> = Vec::new();
    let mut rem_scratch: Vec<(u128, usize)> = Vec::new();
    let mut i = 0usize;
    while i < n {
        let cell = predators[i].cell;
        let mut jj = i;
        while jj < n && predators[jj].cell == cell {
            jj += 1;
        }
        let group = jj - i;
        weights.clear();
        weights.extend((i..jj).map(|k| demand[k].max(0) as u64));
        let total_demand: i64 = weights.iter().map(|&w| w as i64).sum();
        if total_demand > 0 {
            // Available = the frozen prey J in this cell, capped at total demand (no predator grant exceeds it).
            let available = frozen_prey_j[cell as usize].min(total_demand);
            shares.resize(group, 0);
            fixed::apportion_into(available, &weights, &mut shares, &mut rem_scratch);
            for (k, share) in shares.iter().enumerate() {
                granted[i + k] = *share;
            }
        }
        i = jj;
    }

    // ── Pass 4: CONSUME + ATTRIBUTE — a CONSERVING two-level per-cell apportion. ──
    // A naïve "each predator independently apportions its grant across all prey" double-counts a fat prey when
    // two predators share a cell (Σ takes on one prey can exceed its frozen J → the prey can't pay it → minted J).
    // Instead, per cell: (1) apportion the cell's TOTAL grant (Σ predator grants, already ≤ frozen_prey_j[cell]
    // ≤ Σ prey frozen_j by Pass 3) across the prey orgs weighted by frozen_j — so each prey gives ≤ its frozen_j
    // EXACTLY (the apportioned total ≤ Σ weights). (2) split each prey's consumed amount back across the cell's
    // predators by their grant share, and likewise split the predator's grant across the prey it ate — so the
    // FlowMatrix edge + predator credit are attributed per (predator, prey-species) with no over-eat. Conserves
    // J exactly: Σ prey-loss == Σ predator-grant == Σ (kept + tax). All canonical-ordered, integer (inv #3).
    let mut prey_debit: std::collections::BTreeMap<u64, PreyDebit> =
        std::collections::BTreeMap::new();
    let mut pred_credit: std::collections::BTreeMap<u64, i64> = std::collections::BTreeMap::new();

    let mut prey_w: Vec<u64> = Vec::new();
    let mut prey_eaten: Vec<i64> = Vec::new(); // per cell-prey: total J this prey loses this tick
    let mut pred_w: Vec<u64> = Vec::new();
    let mut pred_share: Vec<i64> = Vec::new(); // per cell-predator: how much of THIS prey it takes
    let mut rem4: Vec<(u128, usize)> = Vec::new();
    let mut pi = 0usize;
    while pi < n {
        let cell = predators[pi].cell;
        let mut pj = pi;
        while pj < n && predators[pj].cell == cell {
            pj += 1;
        }
        // The slice of prey rows in this cell (prey is globally sorted by (cell, species, org)).
        let prey_lo = prey.partition_point(|r| r.cell < cell);
        let prey_hi = prey.partition_point(|r| r.cell <= cell);
        let cell_prey = &prey[prey_lo..prey_hi];
        // Cell total grant (Σ predator grants in this cell) — already ≤ frozen_prey_j[cell] by Pass 3.
        let cell_grant: i64 = (pi..pj).map(|k| granted[k].max(0)).sum();
        if !cell_prey.is_empty() && cell_grant > 0 {
            // (1) Apportion the cell's total grant across prey by frozen_j (each prey gives ≤ its frozen_j).
            prey_w.clear();
            prey_w.extend(cell_prey.iter().map(|r| r.frozen_j.max(0) as u64));
            prey_eaten.resize(cell_prey.len(), 0);
            fixed::apportion_into(cell_grant, &prey_w, &mut prey_eaten, &mut rem4);
            // Predator grant weights for the per-prey back-split (ordered, canonical predator order).
            pred_w.clear();
            pred_w.extend((pi..pj).map(|k| granted[k].max(0) as u64));
            for (idx, pr) in cell_prey.iter().enumerate() {
                let eaten = prey_eaten[idx];
                if eaten <= 0 {
                    continue;
                }
                // Record the prey debit (it loses exactly `eaten`, Energy-first-then-Biomass, applied below).
                prey_debit
                    .entry(pr.org)
                    .or_insert(PreyDebit {
                        eaten: 0,
                        dead: false,
                    })
                    .eaten += eaten;
                // (2) Split this prey's `eaten` back across the cell's predators by their grant share, so the
                // predator credit + FlowMatrix edge are attributed per (predator, prey-species). Σ pred_share == eaten.
                pred_share.resize(pj - pi, 0);
                fixed::apportion_into(eaten, &pred_w, &mut pred_share, &mut rem4);
                for (kk, k) in (pi..pj).enumerate() {
                    let take = pred_share[kk];
                    if take <= 0 {
                        continue;
                    }
                    let pred_species = predators[k].species as usize;
                    let pred_org = predators[k].org;
                    // Belt-and-suspenders: an org can never eat itself even if a future Predator were mis-tagged
                    // (a Predator is never prey, so pred/prey species differ by construction — assert it).
                    debug_assert!(
                        pr.org != pred_org && pr.species as usize != pred_species,
                        "predation self-eat guard: prey org/species must differ from predator"
                    );
                    // Predator GAINS kept = take · EFF_NUM/EFF_DEN; the tax (take − kept) → respired (a residual,
                    // never an independent divide that double-floors — finding #7).
                    let kept = take * crate::EFF_NUM / crate::EFF_DEN;
                    let tax = take - kept;
                    ledger.respired += tax;
                    *pred_credit.entry(pred_org).or_insert(0) += kept;
                    // FlowMatrix: net assimilated J flowed FROM prey species INTO predator species (the off-
                    // diagonal predation edge; the diagonal pairing keeps the predator row summing to 0).
                    flow.record(pred_species, pr.species as usize, kept);
                }
            }
        }
        pi = pj;
    }

    // Apply prey debits: Energy-first-then-Biomass; a prey drained to 0 J is fully eaten (despawn, no residual);
    // a partial bite leaving a sub-floor residual deposits the carcass to detritus (the reproduce_or_die path).
    let mut to_despawn: Vec<Entity> = Vec::new();
    for r in &prey {
        let Some(d) = prey_debit.get_mut(&r.org) else {
            continue;
        };
        if d.eaten <= 0 {
            continue;
        }
        let eaten = d.eaten.min(r.frozen_j); // never eat more than the frozen consumable J (apportion guarantees)
        let new_energy = r.energy - eaten.min(r.energy.max(0));
        let energy_eaten = r.energy - new_energy; // the part of `eaten` taken from Energy
        let biomass_eaten = eaten - energy_eaten; // the rest from Biomass
        let new_biomass = r.biomass - biomass_eaten;
        let residual = new_energy.max(0) + new_biomass.max(0);
        // A prey fully consumed (no residual) OR drained below the maintenance floor with no body left is dead.
        let dead = residual <= 0 || (new_energy < MAINTENANCE_FLOOR && new_biomass <= 0);
        if dead {
            d.dead = true;
            // Deposit any residual (a final partial bite may leave a small carcass) to the cell detritus pool,
            // species-tagged so a decomposer's later harvest attributes flow[decomposer][prey] (loop closure).
            if residual > 0 {
                let cellu = r.cell as usize;
                let headroom = (POOL_CAP - pools.detritus[cellu]).max(0);
                let accepted = residual.min(headroom);
                pools.detritus[cellu] += accepted;
                prov.deposit_detritus(cellu, r.species as usize, accepted);
                ledger.overflow += residual - accepted; // detritus cap spill → overflow (nets out)
            }
            to_despawn.push(r.entity);
        }
    }

    // Apply the surviving-prey debits + predator credits via the live query (AFTER the walk; never mutate-during-
    // query for the despawned set). Despawns are deferred through Commands.
    let despawn_set: std::collections::BTreeSet<Entity> = to_despawn.iter().copied().collect();
    for (entity, id, _sp, mut energy, mut biomass, _p) in q.iter_mut() {
        if despawn_set.contains(&entity) {
            continue; // about to despawn; its J already deposited as carcass / fully consumed
        }
        if let Some(d) = prey_debit.get(&id.0) {
            // Survivor: debit Energy-first-then-Biomass by the eaten amount.
            let eaten = d.eaten.min(energy.0.max(0) + biomass.0.max(0));
            let from_energy = eaten.min(energy.0.max(0));
            energy.0 -= from_energy;
            biomass.0 -= eaten - from_energy;
        }
        if let Some(&kept) = pred_credit.get(&id.0) {
            // Predator gains the kept J into Energy; past the cap → overflow (never a silent clamp).
            let (new_e, over) = credit_capped(energy.0, kept, ENERGY_CAP);
            energy.0 = new_e;
            ledger.overflow += over;
        }
    }
    for e in &to_despawn {
        commands.entity(*e).despawn();
    }
}

/// **ASSERT FLOW CLOSES** (ADR-013 F4) — runs near the END of the tick (after every transfer recorded into the
/// matrix), asserting the per-row zero-sum identity holds. Mirrors `measure_and_assert_ledger`.
pub(crate) fn assert_flow_closes(flow: Res<FlowMatrix>) {
    #[cfg(feature = "determinism")]
    assert_flow_rows_sum_zero(&flow);
    #[cfg(not(feature = "determinism"))]
    debug_assert!(
        {
            assert_flow_rows_sum_zero(&flow);
            true
        },
        "flow matrix rows must sum to zero"
    );
    let _ = &flow;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flow_matrix_record_keeps_rows_sum_zero() {
        // ADR-013 F4: every record() applies the diagonal-pairing, so every row sums to zero BY CONSTRUCTION.
        let mut f = FlowMatrix::zeroed(3);
        f.record(0, 1, 100); // species 0 gained 100 from species 1
        f.record(0, 2, 40); // species 0 gained 40 from species 2
        f.record(2, 1, 7); // species 2 gained 7 from species 1
        assert_flow_rows_sum_zero(&f);
        // Off-diagonals carry the edges; the diagonal absorbs the row net. Index helper: row i, col j.
        let at = |f: &FlowMatrix, i: usize, j: usize| f.flat()[i * 3 + j];
        assert_eq!(at(&f, 0, 1), 100);
        assert_eq!(at(&f, 0, 2), 40);
        assert_eq!(
            at(&f, 0, 0),
            -140,
            "diagonal = negation of row off-diagonals"
        );
        assert_eq!(at(&f, 2, 1), 7);
        assert_eq!(at(&f, 2, 2), -7);
    }

    #[test]
    fn flow_matrix_ignores_self_and_nonpositive() {
        let mut f = FlowMatrix::zeroed(2);
        f.record(1, 1, 50); // self-source: no edge
        f.record(0, 1, 0); // zero: no edge
        f.record(0, 1, -3); // negative: no edge
        assert!(f.flat().iter().all(|&v| v == 0), "no spurious edges");
        assert_flow_rows_sum_zero(&f);
    }

    #[test]
    fn flow_matrix_reset_zeroes_keeping_dimension() {
        let mut f = FlowMatrix::zeroed(2);
        f.record(0, 1, 9);
        f.reset();
        assert_eq!(f.s(), 2);
        assert!(f.flat().iter().all(|&v| v == 0));
    }

    #[test]
    fn provenance_withdraw_attributes_flow_and_conserves() {
        // Two depositor species (1 and 2) feed cell 0's detritus; species 0 (the decomposer) harvests it. The
        // harvested J must be attributed over the depositors in proportion (largest-remainder), the source
        // stocks drained, and each row of the resulting flow matrix sums to zero.
        let s = 3;
        let mut prov = PoolProvenance::new(1, s);
        prov.deposit_detritus(0, 1, 60); // species 1 shed 60
        prov.deposit_detritus(0, 2, 40); // species 2 shed 40
        let mut flow = FlowMatrix::zeroed(s);
        prov.withdraw_detritus(0, 0, 100, &mut flow); // decomposer (sp 0) harvests all 100
                                                      // flow[0][1] = 60, flow[0][2] = 40 (proportional), flow[0][0] = -100.
        assert_eq!(flow.flat()[1], 60); // row 0, col 1
        assert_eq!(flow.flat()[2], 40); // row 0, col 2
        assert_eq!(flow.flat()[0], -100); // row 0, col 0 (diagonal)
        assert_flow_rows_sum_zero(&flow);
        // The source stocks were drained.
        assert_eq!(prov.detritus_by_species[1], 0);
        assert_eq!(prov.detritus_by_species[2], 0);
    }

    #[test]
    fn provenance_abiotic_remainder_records_no_flow() {
        // Withdrawing MORE than the biotic stock attributes only the biotic part (the rest is abiotic seed →
        // no flow edge). Conserves: only the biotic 30 is recorded; the extra 70 carries no provenance.
        let s = 2;
        let mut prov = PoolProvenance::new(1, s);
        prov.deposit_detritus(0, 1, 30); // only 30 J is biotic
        let mut flow = FlowMatrix::zeroed(s);
        prov.withdraw_detritus(0, 0, 100, &mut flow); // harvest 100, but only 30 attributable
        assert_eq!(flow.flat()[1], 30, "only the biotic 30 is an edge"); // row 0, col 1
        assert_eq!(flow.flat()[0], -30); // row 0, col 0 (diagonal)
        assert_flow_rows_sum_zero(&flow);
        assert_eq!(prov.detritus_by_species[1], 0);
    }

    #[test]
    fn flow_uses_resource_channels_const() {
        // Guard: the detritus slot the decomposer taps is the last RESOURCE_CHANNEL (a compile-time sanity that
        // affinity[2] is the detritus channel the F4 design names).
        assert_eq!(crate::resource::RESOURCE_CHANNELS, 3);
    }
}
