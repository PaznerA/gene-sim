//! Contamination & immigration — the deterministic, journaled **schedule** behind the `ContainmentLevel`
//! knob (ADR-019 S2). Contamination is the default state of reality (the clean-room frame): a sealed world
//! is the expensive exception, and the moment the containment guard drops, the consortium that flies in wins
//! by default unless the residents already hold the niche. We do NOT script establish/displace/die — that
//! EMERGES from the ADR-013 metabolism→trophic→reproduce_or_die joule economy. This module only supplies the
//! *arrivals* as a deterministic, ordered list of events.
//!
//! ## Determinism (invariant #3 — the load-bearing rule of this module)
//! [`ContainmentLevel`] does NOT consult a wall-clock and draws ZERO [`SimRng`](crate::SimRng) words. It
//! deterministically EXPANDS — off a dedicated off-stream [`derive_seed`](crate::det::derive_seed) family
//! [`IMMG_STREAM_BASE`] (the soil/resource off-stream precedent) — into a **sorted `Vec`** of
//! `(due_epoch, InoculationEvent)` pairs that are a pure function of `(master_seed, ContainmentLevel,
//! ConsortiumConfig)`. Tick-clocked (`due_epoch` is a generation count, never wall-clock), the schedule is
//! journaled like any operator action so a contaminated run replays bit-for-bit. No `HashMap` is iterated
//! (the schedule is an ordered `Vec`).
//!
//! ## Hash-neutrality (no re-pin)
//! The DEFAULT is [`ContainmentLevel::Sealed`] → an EMPTY schedule (no events) → the pinned single-species
//! plant config issues no `RegionInoculate` and the `immigration` ledger tap stays zero → the pinned literal
//! `0x47a0_3c8f_6701_f240` is UNMOVED. Activating a dirtier level (or hand-firing an event) is an inert-until-
//! invoked change, exactly the SP-3 precedent.

use crate::det::derive_seed;

/// Disjoint base for the IMMIGRATION `derive_seed` stream family (ASCII "IMMG"), kept far from the soil /
/// placement / climate / resource / chem families (DECISIONS.md stream registry). Off the `SimRng` stream
/// (inv #3): the schedule draws only `derive_seed` words off this base, ZERO `next_u64`, so introducing the
/// knob cannot reorder the spawn stream or move the determinism hash.
pub const IMMG_STREAM_BASE: u64 = 0x0049_4D4D_4700_0000;

/// The number of distinct `derive_seed` words drawn PER scheduled event off [`IMMG_STREAM_BASE`]. Four
/// independent words pick the contaminant (species index into the consortium), the disc centre `(cx, cy)`,
/// and the propagule `count` — all from disjoint sub-streams so a config change to one axis does not reorder
/// the others. Pinning the per-event stride keeps the families disjoint across events.
const PER_EVENT_WORDS: u64 = 5;

/// A spatial brush region for a scheduled inoculation — a disc of world cells (centre + radius). Mirrors the
/// harness `RegionSpec` / core `Region` shape; carries NO organism handle (inv #6 — the event targets cells,
/// never an individual). Plain integer fields so the schedule is `Eq`-comparable in tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InoculationRegion {
    /// Disc centre cell x on the world grid.
    pub cx: u32,
    /// Disc centre cell y on the world grid.
    pub cy: u32,
    /// Disc radius in cells.
    pub radius: u32,
}

/// A single deterministic inoculation event the [`ContainmentLevel`] schedule expands into: WHICH contaminant
/// (`species_key`, an owned kebab key resolved to a baked `SpeciesSpec` at apply time), WHERE (`region`), HOW
/// MANY (`count`), and the per-organism starting endowment (`endow_j`, minted from the `immigration` tap).
/// This is the SAME shape the player can fire by hand via the harness `Action::RegionInoculate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InoculationEvent {
    /// The contaminant species key (== the `data/species/<key>.json` file stem) to inoculate.
    pub species_key: String,
    /// The disc region the propagule lands in.
    pub region: InoculationRegion,
    /// Number of organisms to spawn.
    pub count: u32,
    /// Per-organism starting joule reserve, MINTED from the `immigration` ledger tap (conserved).
    pub endow_j: i64,
}

/// A scheduled `(due_epoch, event)` pair — the schedule is a `Vec` of these, sorted by `due_epoch` then by
/// the event's fields (a total order so replay is exact). `due_epoch` is a GENERATION count (Tick-clocked).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledInoculation {
    /// The generation at which this event fires (a function of the Tick stream, NEVER wall-clock).
    pub due_epoch: u32,
    /// The inoculation to apply at `due_epoch`.
    pub event: InoculationEvent,
}

/// The contamination-pressure knob (ADR-019 S2): an explicit ISO-14644-1:2015 air-cleanliness ladder, not an
/// arbitrary slider. Dirtier (lower containment) → more pressure: more frequent events, larger propagules,
/// more diversity. The DEFAULT is [`Sealed`](ContainmentLevel::Sealed) (OFF) → an empty schedule → the pinned
/// config is byte-identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContainmentLevel {
    /// ISO 5 / Class 100 / GMP Grade A — near-zero pressure: the schedule is EMPTY (OFF). The DEFAULT, so a
    /// run that never touches the knob issues no immigration events (hash-neutral).
    #[default]
    Sealed,
    /// ISO 7 / Class 10 000 / Grade C — sparse: low frequency, small propagules.
    Clean,
    /// ISO 8 / Class 100 000 / Grade D — frequent: the realistic open-bench default.
    Lab,
    /// ISO 9 / room air — constant flood: the "lab weeds take the plate" mode.
    Open,
}

impl ContainmentLevel {
    /// The number of immigration events this level schedules over the run horizon. `Sealed` → 0 (OFF). Pure
    /// integer ladder; dirtier levels schedule strictly more arrivals.
    #[must_use]
    pub fn event_count(self) -> u32 {
        match self {
            ContainmentLevel::Sealed => 0,
            ContainmentLevel::Clean => 2,
            ContainmentLevel::Lab => 6,
            ContainmentLevel::Open => 16,
        }
    }

    /// The per-event propagule-size CEILING (max organisms an event drops). Dirtier → bigger landings. The
    /// actual count for an event is `1 + (deterministic word % size_cap)` so every event lands ≥ 1 organism.
    #[must_use]
    pub fn propagule_cap(self) -> u32 {
        match self {
            ContainmentLevel::Sealed => 0,
            ContainmentLevel::Clean => 2,
            ContainmentLevel::Lab => 6,
            ContainmentLevel::Open => 20,
        }
    }
}

/// The menu of contaminant species the [`ContainmentLevel`] schedule may draw from (the "consortium"). An
/// ORDERED list of `species_key`s (resolved to baked `SpeciesSpec`s at apply time) — never a `HashMap`
/// (inv #3). An empty consortium yields an empty schedule regardless of the knob (nothing to inoculate).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConsortiumConfig {
    /// The contaminant species keys in play, in fixed order (the schedule's species index keys into this).
    pub species_keys: Vec<String>,
    /// The brush radius every scheduled event uses (a fixed pressure parameter, not a per-event random).
    pub radius: u32,
    /// The per-organism starting endowment every scheduled immigrant receives (minted from the `immigration`
    /// tap). A fixed pressure parameter so the schedule's J accounting is a pure function of its config.
    pub endow_j: i64,
    /// The run horizon in generations the events are spread across (`due_epoch ∈ [0, horizon)`).
    pub horizon: u32,
}

impl ConsortiumConfig {
    /// A reasonable default consortium for the contamination mode: the Mode-A airborne contaminants, a fixed
    /// brush radius, a modest endowment, spread over a 200-generation horizon. The keys reference baked
    /// `data/species/<key>.json` files (the data agent's S0); an absent file is handled by the apply boundary,
    /// not here (this module is pure schedule math).
    #[must_use]
    pub fn default_mode_a() -> Self {
        Self {
            species_keys: vec![
                "bacillus".to_string(),
                "pseudomonas".to_string(),
                "aspergillus-niger".to_string(),
            ],
            radius: 4,
            endow_j: 1_000_000,
            horizon: 200,
        }
    }
}

/// Expand `(master_seed, level, config)` into a sorted, deterministic schedule of journaled inoculation
/// events — the pure function ADR-019 S2 pins. ZERO [`SimRng`](crate::SimRng) draws: every word comes off the
/// off-stream [`IMMG_STREAM_BASE`] [`derive_seed`](crate::det::derive_seed) family. No wall-clock, no
/// `HashMap`. The returned `Vec` is sorted by `(due_epoch, species_key, cx, cy, radius, count)` so replay is
/// exact (a total order). `Sealed` (the default) or an empty consortium → an EMPTY schedule (hash-neutral).
///
/// `world_w`/`world_h` bound the disc centre so a scheduled region always lands on the grid.
#[must_use]
pub fn expand_schedule(
    master_seed: u64,
    level: ContainmentLevel,
    config: &ConsortiumConfig,
    world_w: u32,
    world_h: u32,
) -> Vec<ScheduledInoculation> {
    let n_events = level.event_count();
    let n_species = config.species_keys.len() as u64;
    // OFF: no events, no consortium, or a zero horizon → empty schedule (the Sealed default is byte-neutral).
    if n_events == 0 || n_species == 0 || config.horizon == 0 || world_w == 0 || world_h == 0 {
        return Vec::new();
    }
    let cap = level.propagule_cap().max(1) as u64;
    let mut out: Vec<ScheduledInoculation> = Vec::with_capacity(n_events as usize);
    for e in 0..u64::from(n_events) {
        // Five disjoint off-stream words for this event (species, epoch, cx, cy, count). The per-event stride
        // keeps each event's family disjoint from the next (no collisions — the derive_seed registry contract).
        let base = IMMG_STREAM_BASE + e * PER_EVENT_WORDS;
        let w_species = derive_seed(master_seed, base);
        let w_epoch = derive_seed(master_seed, base + 1);
        let w_cx = derive_seed(master_seed, base + 2);
        let w_cy = derive_seed(master_seed, base + 3);
        let w_count = derive_seed(master_seed, base + 4);

        let species_key = config.species_keys[(w_species % n_species) as usize].clone();
        let due_epoch = (w_epoch % u64::from(config.horizon)) as u32;
        let cx = (w_cx % u64::from(world_w)) as u32;
        let cy = (w_cy % u64::from(world_h)) as u32;
        // Every event lands ≥ 1 organism; the size is bounded by the level's propagule cap.
        let count = 1 + (w_count % cap) as u32;

        out.push(ScheduledInoculation {
            due_epoch,
            event: InoculationEvent {
                species_key,
                region: InoculationRegion {
                    cx,
                    cy,
                    radius: config.radius,
                },
                count,
                endow_j: config.endow_j,
            },
        });
    }
    // Total order so the schedule is replay-exact (no HashMap, no unstable tie-break). Sort by due_epoch then
    // every event field.
    out.sort_by(|a, b| {
        (
            a.due_epoch,
            &a.event.species_key,
            a.event.region.cx,
            a.event.region.cy,
            a.event.region.radius,
            a.event.count,
        )
            .cmp(&(
                b.due_epoch,
                &b.event.species_key,
                b.event.region.cx,
                b.event.region.cy,
                b.event.region.radius,
                b.event.count,
            ))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ConsortiumConfig {
        ConsortiumConfig {
            species_keys: vec!["bacillus".to_string(), "pseudomonas".to_string()],
            radius: 3,
            endow_j: 500_000,
            horizon: 100,
        }
    }

    #[test]
    fn sealed_default_is_empty_off() {
        // The DEFAULT (Sealed) → an EMPTY schedule (OFF) → the pinned config issues no events (hash-neutral).
        assert_eq!(ContainmentLevel::default(), ContainmentLevel::Sealed);
        let sched = expand_schedule(42, ContainmentLevel::Sealed, &cfg(), 32, 32);
        assert!(
            sched.is_empty(),
            "Sealed must produce no immigration events"
        );
    }

    #[test]
    fn empty_consortium_is_empty_even_when_dirty() {
        // No contaminant keys → nothing to inoculate, regardless of the knob.
        let empty = ConsortiumConfig {
            species_keys: vec![],
            ..cfg()
        };
        assert!(expand_schedule(42, ContainmentLevel::Open, &empty, 32, 32).is_empty());
    }

    #[test]
    fn same_seed_knob_config_is_identical_schedule() {
        // Determinism (inv #3): the schedule is a PURE function of (seed, level, config) — two expansions are
        // byte-identical, and a different seed diverges.
        let a = expand_schedule(7, ContainmentLevel::Lab, &cfg(), 32, 32);
        let b = expand_schedule(7, ContainmentLevel::Lab, &cfg(), 32, 32);
        assert_eq!(a, b, "same seed+knob+config → identical schedule");
        let c = expand_schedule(8, ContainmentLevel::Lab, &cfg(), 32, 32);
        assert_ne!(a, c, "a different master seed must diverge the schedule");
    }

    #[test]
    fn schedule_is_sorted_and_well_formed() {
        let sched = expand_schedule(123, ContainmentLevel::Open, &cfg(), 32, 32);
        assert_eq!(sched.len(), ContainmentLevel::Open.event_count() as usize);
        // Sorted by due_epoch (the primary key).
        assert!(sched.windows(2).all(|w| w[0].due_epoch <= w[1].due_epoch));
        for s in &sched {
            assert!(s.due_epoch < cfg().horizon, "epoch within the horizon");
            assert!(s.event.region.cx < 32 && s.event.region.cy < 32, "on grid");
            assert!(s.event.count >= 1, "every event lands ≥ 1 organism");
            assert!(
                s.event.count <= ContainmentLevel::Open.propagule_cap(),
                "count within the propagule cap"
            );
            assert!(
                cfg().species_keys.contains(&s.event.species_key),
                "species from the consortium"
            );
            assert_eq!(s.event.endow_j, cfg().endow_j);
            assert_eq!(s.event.region.radius, cfg().radius);
        }
    }

    #[test]
    fn dirtier_levels_schedule_more_pressure() {
        // The ladder is monotone: dirtier → strictly more events (and a larger propagule ceiling).
        let n = |lvl| expand_schedule(5, lvl, &cfg(), 32, 32).len();
        assert_eq!(n(ContainmentLevel::Sealed), 0);
        assert!(n(ContainmentLevel::Clean) < n(ContainmentLevel::Lab));
        assert!(n(ContainmentLevel::Lab) < n(ContainmentLevel::Open));
    }
}
