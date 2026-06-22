//! The conserved-energy LEDGER of the joule economy (ADR-013 CHEMOSTAT-J, phase **F0a** — scaffolding).
//!
//! Every joule (`J`, an `i64` quantum) in the substrate is conserved EXACTLY modulo **five named taps**:
//! INFLUX (solar minted per tick), IMMIGRATION (ADR-019 — J minted with an inoculated/contaminant organism's
//! starting reserve; a SECOND source distinct from solar so an arrival is independently attributable and never
//! conjured), RESPIRED (maintenance + trophic-efficiency loss), OVERFLOW (the explicit sink for any
//! cap-saturation event, so saturating arithmetic can never *silently* destroy a quantum), and CHEM_DECAY
//! (ADR-013 F5 — the chemical/signal field's dissipation, the only chem sink; kept independently named so chem
//! dissipation stays attributable and `respired`'s meaning stays clean). The invariant **`ledger_closes`** —
//! `Σ(all live J) == initial_total + influx + immigration − respired − overflow − chem_decay` — is a SEMANTIC
//! gate stronger than the bit-hash; the metabolism/pool/chem phases (F0b/F1/F3/F5) assert it every tick.
//!
//! At **F0a** the ledger is present and inserted as a resource, but no system moves `J` yet (energy is still
//! `f64`, no pools exist), so it closes trivially against an initial total of 0. Adding it is **hash-neutral**
//! — the `Ledger` is never folded into `hash_world` and draws nothing from the `SimRng` stream.

use bevy_ecs::prelude::Resource;

/// The run's conserved-energy account. All amounts are joule quanta (`i64`). `initial_total` is the `J` present
/// in the world at reset; the three taps record cumulative flow across the world boundary since reset.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Ledger {
    /// Total `J` present in the world at reset (pools + organism stores + chem). Zero until F1 seeds pools.
    pub initial_total: i64,
    /// Cumulative `J` minted into the world (solar influx).
    pub influx: i64,
    /// Cumulative `J` minted into the world as an IMMIGRATION endowment (ADR-019): an inoculated/contaminant
    /// organism's starting reserve, MINTED here at spawn (not transferred from a parent like a birth). A SECOND
    /// source distinct from `influx` so an arrival is independently attributable. Zero on a run that issues no
    /// `RegionInoculate` (the pinned plant config) → byte-identical to the pre-ADR-019 ledger.
    pub immigration: i64,
    /// Cumulative `J` dissipated as maintenance + trophic inefficiency.
    pub respired: i64,
    /// Cumulative `J` routed to the overflow sink on cap-saturation — never silently lost.
    pub overflow: i64,
    /// Cumulative `J` dissipated by chemical/signal-field DECAY (ADR-013 F5) — the only chem sink, a FOURTH
    /// named tap (kept separate from `respired` so chem dissipation is independently attributable). Zero on a
    /// chem-free run (no species emits → `ChemField == 0` → nothing decays) → byte-identical to F4's J path.
    pub chem_decay: i64,
}

impl Ledger {
    /// The `J` the books say should currently be live: `initial + influx + immigration − respired − overflow −
    /// chem_decay`.
    #[must_use]
    pub fn expected_total(&self) -> i64 {
        self.initial_total + self.influx + self.immigration
            - self.respired
            - self.overflow
            - self.chem_decay
    }

    /// The `ledger_closes` conservation invariant: does the actually-measured live `J` equal what the taps
    /// say it should be? Later phases call this each tick with the summed live `J` of pools + organisms + chem.
    #[must_use]
    pub fn closes(&self, measured_live_total: i64) -> bool {
        measured_live_total == self.expected_total()
    }
}

/// A snapshot of the live `J` currently held in the world, partitioned into its conserved buckets. The
/// `ledger_closes` contract is `Σ buckets == ledger.expected_total()`. As of ADR-013 F5 the schema's `pools +
/// chem + energy + biomass` is FULLY LIVE: `chem` is `ChemField::total()` (the toxin/kin/alarm planes, i32
/// milli == J 1:1, widened to i64 — no conversion), not the documented zero it was through F4. This struct
/// draws nothing from the `SimRng` stream and is never folded into `hash_world` — measuring conservation is
/// **hash-neutral**.
///
/// The live pipeline (F3) builds this each tick by summing, in canonical `(cell, SpeciesId, OrgId)` order
/// (never `HashMap`/Query order — inv #3): every `PoolStock` channel (light + free_nutrient + detritus over
/// all cells) into `pools`, every organism `Energy` into `energy`, every organism `Biomass` into `biomass`.
/// Integer addition is commutative so the SUM is order-independent; the canonical order matters for the
/// *overflow-routing* of capped deposits, not for this total. At F3 the synthetic fixture below is replaced by
/// the real sums and `assert_closes` runs LAST in the tick chain (after all deposits + despawns) under the
/// `determinism` feature as a HARD assert.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LiveTotal {
    /// `Σ` over all cells of `light + free_nutrient + detritus` resource-pool joules.
    pub pools: i64,
    /// `Σ` per-organism free-reserve `Energy` joules.
    pub energy: i64,
    /// `Σ` per-organism structural `Biomass` joules.
    pub biomass: i64,
    /// `Σ` chemical/signal-field joules (ADR-013 F5) — `ChemField::total()`, the toxin/kin/alarm planes
    /// (i32 milli == J, widened to i64). Zero on a chem-free run (no species emits).
    pub chem: i64,
}

impl LiveTotal {
    /// The total live `J` in the world: `pools + energy + biomass + chem`.
    #[must_use]
    pub fn sum(&self) -> i64 {
        self.pools + self.energy + self.biomass + self.chem
    }
}

/// Does the conservation contract hold? `Σ(pools + energy + biomass + chem) == initial + influx − respired −
/// overflow − chem_decay`. The SEMANTIC gate stronger than the bit-hash: it catches a lost (or minted) quantum
/// that a re-pinned hash would otherwise silently bless. Pure read — hash-neutral, no RNG.
#[must_use]
pub fn ledger_closes(ledger: &Ledger, live: &LiveTotal) -> bool {
    ledger.closes(live.sum())
}

/// Hard-assert the conservation contract, panicking with a diagnostic that names the exact joule discrepancy
/// and every term — so a leak fails LEGIBLY at its source rather than as an opaque downstream hash drift. F3
/// runs this LAST in the tick chain (under the `determinism` feature, as a hard assert on both CI arches) so a
/// lost quantum fails the gate on aarch64 even if a re-pinned hash would otherwise close on each arch
/// independently. Exercised on a synthetic fixture now (see tests); wired onto the live pipeline at F3 merge.
///
/// # Panics
/// Panics unless `Σ(pools + energy + biomass + chem) == ledger.expected_total()`.
pub fn assert_ledger_closes(ledger: &Ledger, live: &LiveTotal) {
    let measured = live.sum();
    let expected = ledger.expected_total();
    assert!(
        measured == expected,
        "ledger_closes VIOLATED: measured live J = {measured} (pools={} + energy={} + biomass={} + chem={}) \
         != expected {expected} (initial={} + influx={} + immigration={} − respired={} − overflow={} − chem_decay={}); leak of {} J",
        live.pools,
        live.energy,
        live.biomass,
        live.chem,
        ledger.initial_total,
        ledger.influx,
        ledger.immigration,
        ledger.respired,
        ledger.overflow,
        ledger.chem_decay,
        measured - expected,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_ledger_closes_at_zero() {
        let l = Ledger::default();
        assert_eq!(l.expected_total(), 0);
        assert!(l.closes(0));
        assert!(!l.closes(1));
    }

    #[test]
    fn taps_conserve() {
        // 1000 seeded, +300 minted, −120 respired, −30 to overflow, −10 to chem_decay → 1140 should be live.
        let l = Ledger {
            initial_total: 1000,
            influx: 300,
            immigration: 0,
            respired: 120,
            overflow: 30,
            chem_decay: 10,
        };
        assert_eq!(l.expected_total(), 1140);
        assert!(l.closes(1140));
        assert!(
            !l.closes(1139),
            "a single lost quantum must break the books"
        );
    }

    #[test]
    fn immigration_tap_conserves() {
        // ADR-019: 1000 seeded, +300 solar influx, +500 immigration endowment, −120 respired → 1680 live. The
        // immigration tap is a SECOND source distinct from influx so an arrival is independently attributable.
        let l = Ledger {
            initial_total: 1000,
            influx: 300,
            immigration: 500,
            respired: 120,
            overflow: 0,
            chem_decay: 0,
        };
        assert_eq!(l.expected_total(), 1680);
        assert!(l.closes(1680));
        assert!(
            !l.closes(1681),
            "a conjured immigrant quantum must break the books"
        );
        // A run that never inoculates (immigration == 0) is byte-identical to the pre-ADR-019 ledger math.
        let no_immig = Ledger {
            immigration: 0,
            ..l
        };
        assert_eq!(no_immig.expected_total(), 1180);
    }

    #[test]
    fn live_total_sums_all_four_buckets_incl_chem() {
        // ADR-013 F5: chem is now a LIVE bucket (the toxin/kin/alarm field). Σ folds it in.
        let live = LiveTotal {
            pools: 400,
            energy: 250,
            biomass: 100,
            chem: 75,
        };
        assert_eq!(live.sum(), 825);
        // A chem-free run still closes with chem == 0 (byte-identical to F4's J path).
        let chem_free = LiveTotal { chem: 0, ..live };
        assert_eq!(chem_free.sum(), 750);
    }

    #[test]
    fn ledger_closes_helper_matches_taps() {
        let l = Ledger {
            initial_total: 1000,
            influx: 300,
            immigration: 0,
            respired: 120,
            overflow: 30,
            chem_decay: 0,
        }; // expected 1150
        let ok = LiveTotal {
            pools: 800,
            energy: 250,
            biomass: 100,
            chem: 0,
        }; // 1150
        let leaky = LiveTotal {
            pools: 799,
            energy: 250,
            biomass: 100,
            chem: 0,
        }; // 1149
        assert!(ledger_closes(&l, &ok));
        assert!(!ledger_closes(&l, &leaky), "a lost quantum must not close");
        assert_ledger_closes(&l, &ok); // must NOT panic
    }

    #[test]
    #[should_panic(expected = "ledger_closes VIOLATED")]
    fn assert_ledger_closes_panics_on_a_lost_quantum() {
        let l = Ledger {
            initial_total: 1000,
            ..Default::default()
        };
        let leaky = LiveTotal {
            pools: 999, // one quantum has vanished — no tap accounts for it
            ..Default::default()
        };
        assert_ledger_closes(&l, &leaky);
    }

    /// A SYNTHETIC fixture that walks the F3 tick pipeline's J transfers tick-by-tick and asserts closure after
    /// EACH tick — including spawn (seed), uptake (pool→org), maintenance/efficiency (respired tap), excrete
    /// (org→detritus pool), a cap-saturation OVERFLOW event, a carcass→detritus death deposit, and a run driven
    /// to full extinction (empty population, ledger still closes). This is the structure the live pipeline will
    /// reuse at F3 merge; here every transfer is a paired debit/credit or a named-tap write, so closure holds
    /// BY CONSTRUCTION and the assert is the guard that catches an unpaired arithmetic. NO RNG, NO float, NO
    /// HashMap — pure i64. Models the world as one cell's pools + a small org list, exactly the conserved
    /// buckets `live_total` will sum over the real ECS world.
    #[test]
    fn synthetic_fixture_ledger_closes_every_tick() {
        // World state (the conserved buckets). chem = 0 (F5).
        // PoolStock light + free_nutrient + detritus, lumped for the fixture; seeded at reset below.
        let mut orgs: Vec<(i64, i64)> = Vec::new(); // (Energy, Biomass) per live organism
        let mut ledger = Ledger::default();

        // Helper: current measured live total, summed in a stable order (Vec iteration, never HashMap).
        let live = |pools: i64, orgs: &[(i64, i64)]| -> LiveTotal {
            let energy: i64 = orgs.iter().map(|o| o.0).sum();
            let biomass: i64 = orgs.iter().map(|o| o.1).sum();
            LiveTotal {
                pools,
                energy,
                biomass,
                chem: 0,
            }
        };

        // ── reset: seed the world. initial_total = pools + Σenergy + Σbiomass, computed once, off-RNG. ──
        let mut pools: i64 = 1_000;
        orgs.push((100, 20)); // org A
        orgs.push((80, 20)); // org B
        ledger.initial_total = live(pools, &orgs).sum();
        assert_ledger_closes(&ledger, &live(pools, &orgs));

        // ── tick 1: solar INFLUX mints J into the pool (the only source) → influx tap. ──
        let minted = 300;
        pools += minted;
        ledger.influx += minted;
        assert_ledger_closes(&ledger, &live(pools, &orgs));

        // ── tick 2: UPTAKE — J leaves the pool, enters orgs (paired debit/credit, no tap touched). ──
        let granted_a = 120;
        let granted_b = 90;
        pools -= granted_a + granted_b;
        orgs[0].0 += granted_a; // into Energy reserve
        orgs[1].0 += granted_b;
        assert_ledger_closes(&ledger, &live(pools, &orgs));

        // ── tick 3: CONVERT efficiency loss + MAINTENANCE debit → respired tap (the only sink). ──
        let respired_a = 40; // maintenance + trophic-inefficiency dissipation
        let respired_b = 30;
        orgs[0].0 -= respired_a;
        orgs[1].0 -= respired_b;
        ledger.respired += respired_a + respired_b;
        assert_ledger_closes(&ledger, &live(pools, &orgs));

        // ── tick 4: EXCRETE — org sheds carbon back into the detritus pool (paired debit/credit). ──
        let excreted = 25;
        orgs[0].0 -= excreted;
        pools += excreted;
        assert_ledger_closes(&ledger, &live(pools, &orgs));

        // ── tick 5: cap-saturation OVERFLOW — influx into an already-full cell spills to the overflow tap,
        //            never silently clamped. The minted-but-rejected J is routed, not dropped. ──
        let minted2 = 200;
        let accepted = 150; // cell had only this much headroom
        let spilled = minted2 - accepted; // 50 → overflow
        pools += accepted;
        ledger.influx += minted2; // ALL minted J is booked to influx…
        ledger.overflow += spilled; // …and the rejected part is booked to overflow (so it nets out)
        assert_ledger_closes(&ledger, &live(pools, &orgs));

        // ── tick 6: a DEATH — org B starves; its residual Energy+Biomass deposits to detritus (carcass→
        //            detritus, conserving J) and the org is removed. live_total then counts the J in detritus
        //            (pools), never twice. The despawn happens AFTER the deposit, both BEFORE the assert. ──
        let (res_e, res_b) = orgs[1];
        pools += res_e + res_b; // carcass → detritus
        orgs.remove(1); // despawn (collected, not mutate-during-iterate)
        assert_ledger_closes(&ledger, &live(pools, &orgs));

        // ── tick 7: drive to full EXTINCTION — the last org dies; population empties; ledger STILL closes. ──
        let (res_e, res_b) = orgs[0];
        pools += res_e + res_b;
        orgs.clear();
        let empty = live(pools, &orgs);
        assert_eq!(empty.energy, 0);
        assert_eq!(empty.biomass, 0);
        assert_ledger_closes(&ledger, &empty);

        // Final cross-check: every J that ever entered is accounted for as live-or-tapped. With orgs empty,
        // all live J is in the pool, and it equals initial + influx − respired − overflow − chem_decay.
        assert_eq!(empty.sum(), pools);
        assert_eq!(empty.sum(), ledger.expected_total());
    }

    /// ADR-013 F5: the FOUR-bucket close with a non-zero chem field. EMIT is a paired Energy→chem move (Σ
    /// unchanged); DECAY is the only chem sink (→ the chem_decay tap). The books close every step by
    /// construction; the assert is the guard that catches an unpaired chem move.
    #[test]
    fn synthetic_fixture_closes_with_a_live_chem_field() {
        let mut ledger = Ledger::default();
        let mut pools: i64 = 500;
        let mut energy: i64 = 300; // one org's Energy reserve
        let mut chem: i64 = 0;
        let mk = |pools: i64, energy: i64, chem: i64| LiveTotal {
            pools,
            energy,
            biomass: 0,
            chem,
        };
        ledger.initial_total = mk(pools, energy, chem).sum();
        assert_ledger_closes(&ledger, &mk(pools, energy, chem));

        // EMIT: the org spends 50 J on a kin marker — Energy→chem, a paired move, Σ unchanged, no tap.
        let emit = 50;
        energy -= emit;
        chem += emit; // milli == J 1:1
        assert_ledger_closes(&ledger, &mk(pools, energy, chem));

        // DECAY: the chem field dissipates 8 J → the chem_decay tap (the only chem sink).
        let decay = 8;
        chem -= decay;
        ledger.chem_decay += decay;
        assert_ledger_closes(&ledger, &mk(pools, energy, chem));

        // INFLUX + a normal respired tick still compose with the chem taps.
        pools += 100;
        ledger.influx += 100;
        energy -= 20;
        ledger.respired += 20;
        assert_ledger_closes(&ledger, &mk(pools, energy, chem));

        // The decayed J is GONE from the live total but ACCOUNTED in the tap (the books still close).
        assert_eq!(mk(pools, energy, chem).sum(), ledger.expected_total());
        assert_eq!(ledger.chem_decay, decay);
    }
}
