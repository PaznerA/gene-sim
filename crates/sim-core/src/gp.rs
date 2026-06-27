//! Genotype→phenotype mapping (TAXONOMY §2, SPEC §4) — Parameters → [`Trait`]s.
//!
//! This is the **only** place genotype→phenotype logic lives (invariant #2; it stays in `genome`/`sim-core`,
//! never in `godot/`). The mapping is **pure and deterministic** for a fixed genome (invariant #3) and sits
//! behind the [`GenotypePhenotypeMap`] trait so it is pluggable (invariant #5) — [`WeightedSumMap`] is the
//! Stage-1 default. No `HashMap` is iterated: we walk the genome's ordered `loci`/`parameters` only.

use crate::fixed;
use genome::{Genome, GoTermId, LocusId, ParamId};

/// A heritable trait expressed from the genome. Extensible (TAXONOMY §2); new *biological* kinds arrive as
/// ontology nodes (Stage 5), but the small fixed set the engine reasons about is enumerated here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Trait {
    /// Growth rate — feeds [`crate::Simulation`] selection (the only trait that drives the sim).
    GrowthRate,
    /// Overall height / reach of the plant.
    Stature,
    /// How much the plant branches (architecture density).
    Branchiness,
    /// Leaf size.
    LeafSize,
    /// Leaf colour hue.
    LeafHue,
    /// Surface reflectance (colour + spread).
    Reflectance,
    /// Reproductive output (flowering).
    Fecundity,
    /// Drought tolerance (sturdier taper / narrower leaves).
    DroughtTolerance,
    /// CRISPR kill-switch linkage (a discrete bool trait).
    KillSwitchLinkage,

    // ── Microbe traits (ADR-017 F2-2) — the E. coli observable phenotypes, expressed via the E. coli
    // [`ecoli_trait_map`]. Deliberately NOT in [`Trait::ALL`] (that stays the 9 plant render/CSV order); a
    // microbe species expresses these through its own `TraitMap` instead.
    /// Glucose uptake capacity (PTS system) — microbe.
    GlucoseUptake,
    /// Respiration-mode lean (aerobic ↔ fermentative) — microbe.
    RespirationMode,
    /// Acetate overflow — the Layer-3 detritus/mineralization tap — microbe.
    AcetateOverflow,
    /// Fermentation capacity (lactate / ethanol) — microbe.
    FermentationCapacity,

    // ── Predator trait (ADR-013 F6 — Bdellovibrio) — the host-attack/lytic machinery phenotype, expressed via
    // the Bdellovibrio [`bdellovibrio_trait_map`]. Deliberately NOT in [`Trait::ALL`] (the 9-trait plant order),
    // exactly like the E. coli microbe traits; a predator species expresses it through its own `TraitMap`.
    /// Predation capacity (host-cell invasion / peptidoglycan-remodeling attack rate) — predator.
    PredationCapacity,

    // ── Spore-former trait (ADR-019 S4 — the dormancy/germination mechanic) — the sporulation-cascade phenotype
    // (spo0A master regulator / sigF-sigE-sigG / the mold brlA→abaA→wetA conidiation cascade), expressed via a
    // spore-former species' own `TraitMap`. Deliberately NOT in [`Trait::ALL`] (the 9-trait plant order),
    // exactly like the E. coli microbe + predator traits — so the plant phenotype vector, CSV header, and hash
    // stay byte-unperturbed. A non-zero value marks the species a SPORE-FORMER ([`Strategy::spore_former`]); a
    // genome lacking the anchor expresses `0.0` → NOT a spore-former (the inert-off-role guarantee).
    /// Sporulation capacity (the spo0A/sigF endospore or brlA conidiation cascade) — spore-former marker.
    SporulationCapacity,

    // ── Obligate-symbiont trait (ADR-019 S5 — the host-coupling mechanic) — the amino-acid-provisioning /
    // host-exchange capacity (e.g. Carsonella's retained aromatic-amino-acid biosynthesis that it trades the
    // psyllid host for shelter), expressed via an obligate symbiont's own `TraitMap`. Deliberately NOT in
    // [`Trait::ALL`] (the 9-trait plant order), exactly like the E. coli microbe + predator + spore-former
    // traits — so the plant phenotype vector, CSV header, and hash stay byte-unperturbed. A non-zero value
    // drives [`Strategy::host_draw_rate`] (the host-coupling exchange rate); a genome lacking the anchor
    // expresses `0.0` → `host_draw_rate == 0` (the inert-off-role guarantee).
    /// Symbiosis capacity (the host-coupling amino-acid-exchange machinery) — obligate-symbiont lever.
    SymbiosisCapacity,
}

impl Trait {
    /// The traits in canonical (declaration) order — the order a [`Phenotype`] stores them in.
    /// A fixed array (not a `HashMap`) so iteration is deterministic (invariant #3). Each trait is anchored
    /// on its OWN flat genome parameter (see [`WeightedSumMap::weight`]) so they vary INDEPENDENTLY — an edit
    /// to one parameter moves exactly one trait, giving the specimen view many distinct, continuous variants.
    pub const ALL: [Trait; 9] = [
        Trait::GrowthRate,
        Trait::Stature,
        Trait::Branchiness,
        Trait::LeafSize,
        Trait::LeafHue,
        Trait::Reflectance,
        Trait::Fecundity,
        Trait::DroughtTolerance,
        Trait::KillSwitchLinkage,
    ];

    /// The trait's stable `snake_case` column name (CSV headers, JSON keys). Exhaustive — a new variant must add
    /// its name here. The 9 plant names match the historical per-gen CSV header (so the plant CSV is unchanged).
    #[must_use]
    pub fn snake_name(self) -> &'static str {
        match self {
            Trait::GrowthRate => "growth_rate",
            Trait::Stature => "stature",
            Trait::Branchiness => "branchiness",
            Trait::LeafSize => "leaf_size",
            Trait::LeafHue => "leaf_hue",
            Trait::Reflectance => "reflectance",
            Trait::Fecundity => "fecundity",
            Trait::DroughtTolerance => "drought_tolerance",
            Trait::KillSwitchLinkage => "kill_switch_linkage",
            Trait::GlucoseUptake => "glucose_uptake",
            Trait::RespirationMode => "respiration_mode",
            Trait::AcetateOverflow => "acetate_overflow",
            Trait::FermentationCapacity => "fermentation_capacity",
            Trait::PredationCapacity => "predation_capacity",
            Trait::SporulationCapacity => "sporulation_capacity",
            Trait::SymbiosisCapacity => "symbiosis_capacity",
        }
    }
}

/// An expressed phenotype: an **ordered** list of `(Trait, value)` pairs, each value clamped to `[0, 1]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Phenotype {
    /// Ordered (canonical `Trait::ALL` order). Iterate this; never a `HashMap` (invariant #3).
    pub values: Vec<(Trait, f64)>,
}

impl Phenotype {
    /// The value of a given trait, if present. Linear scan over the (tiny, ordered) list.
    #[must_use]
    pub fn get(&self, t: Trait) -> Option<f64> {
        self.values.iter().find(|(k, _)| *k == t).map(|(_, v)| *v)
    }
}

/// A pure, deterministic genotype→phenotype map (invariant #2, #3, #5).
pub trait GenotypePhenotypeMap {
    /// Express `genome` into a [`Phenotype`]. Same genome ⇒ identical phenotype.
    fn express(&self, genome: &Genome) -> Phenotype;
}

/// How a [`TraitBinding`] selects the locus carrying its parameter (ADR-017 F2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocusSelector {
    /// The locus with this id — a stable positional layout (the plant's loci).
    ByIndex(LocusId),
    /// The FIRST locus (in genome `loci` Vec order) whose `go_refs` contains this GO term — an ONTOLOGY-driven
    /// binding for species whose layout isn't positional (e.g. E. coli genes keyed by molecular function).
    ByGoAnchor(GoTermId),
}

/// One trait's binding: which locus + which parameter within it expresses the trait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraitBinding {
    /// The expressed trait.
    pub trait_: Trait,
    /// Which locus carries the parameter.
    pub locus: LocusSelector,
    /// The parameter id within that locus.
    pub param: ParamId,
}

/// An ordered, per-species set of trait bindings — the genotype→phenotype "wiring" for one species. An ordered
/// `Vec` (never a `HashMap`, inv #3); the binding order IS the [`Phenotype`] order.
#[derive(Debug, Clone, PartialEq)]
pub struct TraitMap(pub Vec<TraitBinding>);

/// Resolve a [`LocusSelector`] against a genome (ordered, pure, no RNG). `ByGoAnchor` takes the FIRST matching
/// locus in `loci` Vec order, so the result is deterministic.
fn resolve_locus(genome: &Genome, sel: LocusSelector) -> Option<&genome::Locus> {
    match sel {
        LocusSelector::ByIndex(id) => genome.locus(id),
        LocusSelector::ByGoAnchor(go) => genome.loci.iter().find(|l| l.tags.go_refs.contains(&go)),
    }
}

/// The genotype→phenotype map driven by a per-species [`TraitMap`] (ADR-017 F2): each trait reads exactly the
/// locus + parameter its species names, so plant and microbe genomes express their OWN traits from one engine
/// (invariant #5). Pure + ordered; a binding whose locus/param is absent expresses a documented `0.0` (never a
/// panic), so an arbitrary loaded genome can never crash expression.
#[derive(Debug, Clone)]
pub struct OntologyMap {
    map: TraitMap,
}

impl OntologyMap {
    /// Build an `OntologyMap` from a species' [`TraitMap`].
    #[must_use]
    pub fn new(map: TraitMap) -> Self {
        Self { map }
    }
}

impl GenotypePhenotypeMap for OntologyMap {
    fn express(&self, genome: &Genome) -> Phenotype {
        let values = self
            .map
            .0
            .iter()
            .map(|b| {
                let scalar = resolve_locus(genome, b.locus)
                    .and_then(|l| l.parameters.iter().find(|p| p.id == b.param))
                    .map_or(0.0, |p| p.value.as_unit_scalar());
                (b.trait_, scalar.clamp(0.0, 1.0))
            })
            .collect();
        Phenotype { values }
    }
}

/// The default PLANT trait map — the 9 bindings that reproduce the historical flat-index anchoring EXACTLY
/// (`GrowthRate`=L0/P0, `Stature`=L0/P1, `Branchiness`=L0/P2, `LeafSize`=L1/P0, `LeafHue`=L1/P1,
/// `Reflectance`=L1/P2, `Fecundity`=L2/P0, `DroughtTolerance`=L3/P0, `KillSwitchLinkage`=L3/P1). Because each
/// binding reads exactly the parameter its old flat anchor did, [`WeightedSumMap`] expresses byte-identically
/// to before F2 (hash-neutral — proven by the unchanged pinned determinism literal).
#[must_use]
pub fn default_plant_trait_map() -> TraitMap {
    let b = |t, l, p| TraitBinding {
        trait_: t,
        locus: LocusSelector::ByIndex(LocusId(l)),
        param: ParamId(p),
    };
    TraitMap(vec![
        b(Trait::GrowthRate, 0, 0),
        b(Trait::Stature, 0, 1),
        b(Trait::Branchiness, 0, 2),
        b(Trait::LeafSize, 1, 0),
        b(Trait::LeafHue, 1, 1),
        b(Trait::Reflectance, 1, 2),
        b(Trait::Fecundity, 2, 0),
        b(Trait::DroughtTolerance, 3, 0),
        b(Trait::KillSwitchLinkage, 3, 1),
    ])
}

/// The E. coli per-species [`TraitMap`] (ADR-017 B-2): the 5 microbe traits bound by ONTOLOGY (`ByGoAnchor`) to
/// the metabolic anchor genes in `data/species/ecoli.json`, each reading that gene's activity parameter (P0,
/// `1.0`=wild-type). A knockout edit (activity→0) drives the bound trait to 0. `GrowthRate` — the only
/// selection-driving trait — anchors on the TCA backbone gene `gltA`. Ordered (inv #3); the GO ids match the
/// curated `go_refs` baked into ecoli.json by `scripts/bake_ecoli_species.py`.
#[must_use]
pub fn ecoli_trait_map() -> TraitMap {
    let b = |t, go| TraitBinding {
        trait_: t,
        locus: LocusSelector::ByGoAnchor(GoTermId(go)),
        param: ParamId(0),
    };
    TraitMap(vec![
        b(Trait::GrowthRate, 4108), // gltA — citrate synthase (TCA/growth backbone)
        b(Trait::GlucoseUptake, 8982), // ptsG — PTS glucose transporter
        b(Trait::RespirationMode, 8861), // pflB — pyruvate formate-lyase (fermentation marker)
        b(Trait::AcetateOverflow, 8959), // pta  — phosphate acetyltransferase (acetate overflow)
        b(Trait::FermentationCapacity, 8720), // ldhA — D-lactate dehydrogenase
    ])
}

/// The Bdellovibrio per-species [`TraitMap`] (ADR-013 F6 — the predator). `GrowthRate` (the selection-driving
/// trait) anchors on the TCA backbone gene `gltA` (GO-4108), exactly as E. coli does — a predator still funds its
/// own growth from kept predation-J via the budget. `PredationCapacity` — the attack-rate lever — anchors on the
/// host-cell-invasion / peptidoglycan-hydrolase molecular function (GO-8745, "lysozyme/peptidoglycan muralytic
/// activity"), the `hit`/lytic-machinery attack genes baked into `data/species/bdellovibrio.json`. A `hit`-locus
/// CRISPRi Knockdown (activity→0) drives `PredationCapacity` → 0 → `predation_rate` → 0 (the gene-driven OVERSIGHT
/// lever, exactly like `pta`→`mineralize_rate`). Ordered (inv #3). A PURE ADDITION — it touches neither
/// [`default_plant_trait_map`] nor [`ecoli_trait_map`], so the plant/E. coli express paths are byte-unperturbed.
#[must_use]
pub fn bdellovibrio_trait_map() -> TraitMap {
    let b = |t, go| TraitBinding {
        trait_: t,
        locus: LocusSelector::ByGoAnchor(GoTermId(go)),
        param: ParamId(0),
    };
    TraitMap(vec![
        b(Trait::GrowthRate, 4108), // gltA — citrate synthase (TCA/growth backbone)
        b(Trait::PredationCapacity, 8745), // peptidoglycan/host-invasion lytic machinery (the attack lever)
    ])
}

/// The obligate-symbiont (Carsonella / Syn3.0) per-species [`TraitMap`] (ADR-019 S5). `GrowthRate` (the
/// selection-driving trait) anchors on a retained translation-core gene (`tuf`/EF-Tu — the symbiont still funds
/// its own growth from kept host-J). `SymbiosisCapacity` — the host-coupling exchange lever — anchors on the
/// AMINO-ACID-PROVISIONING molecular function (GO-8652, "amino acid biosynthetic process"): Carsonella's
/// retained aromatic-amino-acid / leucine biosynthesis is exactly the metabolite trade that JUSTIFIES the
/// coupling and is the codex story of genome reduction. A provisioning-locus CRISPRi Knockdown (activity→0)
/// drives `SymbiosisCapacity` → 0 → `host_draw_rate` → 0 (the OVERSIGHT lever, exactly like `hit`→`predation_rate`).
/// Ordered (inv #3). A PURE ADDITION — it touches neither plant nor ecoli/predator express paths.
#[must_use]
pub fn symbiont_trait_map() -> TraitMap {
    let b = |t, go| TraitBinding {
        trait_: t,
        locus: LocusSelector::ByGoAnchor(GoTermId(go)),
        param: ParamId(0),
    };
    TraitMap(vec![
        b(Trait::GrowthRate, 6414), // translation elongation (EF-Tu / tuf — the retained growth backbone)
        b(Trait::SymbiosisCapacity, 8652), // amino-acid biosynthesis (the host-provisioning exchange lever)
    ])
}

/// Select the per-species [`TraitMap`] by the species `key` (ADR-017 "RUN E. coli"). A pure, ordered `match`
/// (never a `HashMap` — inv #3): `"ecoli-core"` → [`ecoli_trait_map`]; `"bdellovibrio"` → [`bdellovibrio_trait_map`];
/// `"carsonella"`/`"syn3"` → [`symbiont_trait_map`]; EVERY other key → the default plant map, so an unknown/missing
/// key degrades safely to the historical behaviour.
#[must_use]
pub fn trait_map_for(key: &str) -> TraitMap {
    match key {
        "ecoli-core" => ecoli_trait_map(),
        "bdellovibrio" => bdellovibrio_trait_map(),
        "carsonella" | "syn3" => symbiont_trait_map(),
        _ => default_plant_trait_map(),
    }
}

/// The transparent Stage-1 default for the PLANT species: each of the 9 traits reads exactly its own anchored
/// genome parameter ([`genome::ParamValue::as_unit_scalar`], clamped to `[0, 1]`), fully DECOUPLED so an edit to
/// one parameter moves exactly one trait (many independent, continuous specimen variants).
///
/// Since ADR-017 F2 this is a thin wrapper over [`OntologyMap`] carrying [`default_plant_trait_map`] — the same
/// anchoring (`GrowthRate`=L0/P0 … `KillSwitchLinkage`=L3/P1) expressed through the per-species binding engine,
/// so it stays byte-identical (hash-neutral) while E. coli / other species supply their OWN [`TraitMap`].
#[derive(Debug, Clone, Copy, Default)]
pub struct WeightedSumMap;

impl GenotypePhenotypeMap for WeightedSumMap {
    fn express(&self, genome: &Genome) -> Phenotype {
        OntologyMap::new(default_plant_trait_map()).express(genome)
    }
}

// ── ADR-013 F2: ecological Strategy substrate (expressed + cached, UNREAD by selection → hash-neutral) ──
//
// A species' genome expresses, through the SAME `OntologyMap::express` engine (invariant #2 — the only
// genotype→phenotype path), a conserved metabolic-budget `Strategy`. It is cached once per species at reset
// and read by NOTHING in the sim path at F2 (F3's metabolism pipeline is its first reader), so it folds
// into no hash and draws nothing from `SimRng`. All-integer fields (`u16`) so `Strategy: Eq` is derivable
// (no f64-equality hazard) and cached strategies are bit-comparable in tests.

/// How a species earns its joules — sets which F3 metabolic tap it will draw from (ADR-013 §Decision,
/// DECISIONS.md:538). CATEGORICAL: declared per-species as DATA (see [`role_for`]), NEVER derived from
/// allele-frequency scalars (a role must not drift with edits). Fieldless `Copy`/`Eq` (no float) so it folds
/// into [`Strategy`]'s derived `Eq`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrophicRole {
    /// Earns from a primary resource channel (light) — the abstract-plant default.
    Autotroph,
    /// Earns by consuming other organisms / organic carbon (e.g. E. coli on glucose).
    Heterotroph,
    /// Mixed strategy (both autotrophic and heterotrophic taps).
    Mixotroph,
    /// Earns by mineralizing detritus — the plant→detritus→microbe loop (F4 obligate decomposer).
    Decomposer,
    /// Earns ONLY by consuming (predating) other organisms' joules — the F6 Bdellovibrio predator. Taps NO
    /// abiotic resource channel (unlike `Heterotroph`, which would also draw free_nutrient/detritus via the
    /// metabolism apportion), so the [`crate::trophic::predation`] kernel is its SOLE income — that is the
    /// STRUCTURAL guarantee a dedicated role buys (a `Heterotroph + affinity` would double-dip the abiotic taps).
    Predator,
    /// Earns ONLY by drawing kept-J from a co-located HOST organism — the ADR-019 S5 obligate endosymbiont
    /// (e.g. *Candidatus* Carsonella ruddii inside its psyllid bacteriocyte). APPENDED after `Predator` so every
    /// existing discriminant is unperturbed. Like `Predator`, it taps NO abiotic resource channel — the
    /// [`crate::trophic::host_coupling`] pass is its SOLE income — which is the STRUCTURAL "cannot free-live"
    /// guarantee: a NEW variant falls THROUGH all three [`crate::metabolism`] abiotic taps (light / free_nutrient
    /// / detritus, each gated on `Autotroph|Heterotroph|Mixotroph|Decomposer`), so it draws zero abiotic J with
    /// NO metabolism edit. Hostless ⇒ no income ⇒ it starves below the maintenance floor (emergent death).
    ObligateSymbiont,
}

impl Default for TrophicRole {
    /// The plant default, preserving current single-species behaviour.
    fn default() -> Self {
        TrophicRole::Autotroph
    }
}

/// Is a [`TrophicRole`] eligible PREY for the F6 predator ([`crate::trophic::predation`])? An ordered, pure
/// predicate (never a `HashMap`, inv #3): `true` for {`Heterotroph`, `Decomposer`}, `false` for {`Autotroph`,
/// `Mixotroph`, `Predator`}. This encodes Bdellovibrio's gram-negative-bacteria host range — it invades the
/// periplasm of OTHER BACTERIA, not plant cells (`Autotroph`), and there is NO intraguild/hyper-predation in
/// this slice (`Predator` is not prey). `Mixotroph` (algal/plant-ish) is excluded pending the taxonomy owner.
/// E. coli is a `Decomposer` → eligible prey, so the F6 predator closes plant→microbe→predator. ADR-019 S5:
/// an `ObligateSymbiont` is NOT prey either (it falls through the `matches!` automatically — a Bdellovibrio
/// co-located with an endosymbiont sheltered inside its host cannot eat it; the host is the cullable target).
#[must_use]
pub fn is_prey(role: TrophicRole) -> bool {
    matches!(role, TrophicRole::Heterotroph | TrophicRole::Decomposer)
}

/// Map a species `key` → its trophic role. An ordered `match` exactly parallel to [`trait_map_for`] — the
/// SAME key-dispatch seam already proven for [`TraitMap`] selection. Pure, never a `HashMap` (inv #3); an
/// unknown/missing key degrades safely to the plant default. ADR-013 F4 flips `"ecoli-core" => Decomposer`
/// for the obligate plant→detritus→E. coli loop; a `Niche.trophic_role` JSON field can OVERRIDE this per spec
/// via [`role_from_override`] (the data-driven path the boundary uses).
#[must_use]
pub fn role_for(key: &str) -> TrophicRole {
    match key {
        "ecoli-core" => TrophicRole::Decomposer,
        "bdellovibrio" => TrophicRole::Predator,
        // ADR-019 S5: the obligate endosymbionts (the JSON override is the load-bearing path; these are the
        // key defaults, mirroring `"bdellovibrio" => Predator`).
        "carsonella" | "syn3" => TrophicRole::ObligateSymbiont,
        _ => TrophicRole::Autotroph,
    }
}

/// Resolve a `niche.trophic_role` string into a [`TrophicRole`] (ADR-013 F4 — the DATA-driven role override).
/// Case-insensitive, ordered `match` (never a `HashMap`, inv #3). An unknown/empty string degrades to
/// [`role_for`] — so a typo can never silently zero a species' niche; it falls back to the key default.
#[must_use]
pub fn role_from_str(s: &str) -> Option<TrophicRole> {
    match s.trim().to_ascii_lowercase().as_str() {
        "autotroph" => Some(TrophicRole::Autotroph),
        "heterotroph" => Some(TrophicRole::Heterotroph),
        "mixotroph" => Some(TrophicRole::Mixotroph),
        "decomposer" => Some(TrophicRole::Decomposer),
        "predator" => Some(TrophicRole::Predator),
        // ADR-019 S5: the obligate-symbiont role (accept the common spellings the data may use).
        "symbiont" | "obligate_symbiont" | "obligatesymbiont" => {
            Some(TrophicRole::ObligateSymbiont)
        }
        _ => None,
    }
}

/// The role the boundary assigns a species: the `niche.trophic_role` OVERRIDE when present + recognized,
/// else [`role_for`] of the key (ADR-013 F4). The single seam the JSON→roster boundary uses, keeping the role
/// CATEGORICAL data (inv: never derived from genome scalars), so a CRISPR edit can't flip it.
#[must_use]
pub fn role_from_override(override_role: Option<&str>, key: &str) -> TrophicRole {
    override_role
        .and_then(role_from_str)
        .unwrap_or_else(|| role_for(key))
}

/// The canonical lowercase `niche.trophic_role` string for a [`TrophicRole`] — the INVERSE of
/// [`role_from_str`] (`role_from_str(role_to_str(r)) == Some(r)` for every role). Ordered `match` (inv #3).
/// The Variant-Lab species export (`Simulation::export_species_spec`) writes this so a reseeded variant
/// resolves back to the SAME role through [`role_from_override`] (the save→reseed contract). `ObligateSymbiont`
/// emits `"symbiont"` (the primary spelling `role_from_str` accepts).
#[must_use]
pub fn role_to_str(role: TrophicRole) -> &'static str {
    match role {
        TrophicRole::Autotroph => "autotroph",
        TrophicRole::Heterotroph => "heterotroph",
        TrophicRole::Mixotroph => "mixotroph",
        TrophicRole::Decomposer => "decomposer",
        TrophicRole::Predator => "predator",
        TrophicRole::ObligateSymbiont => "symbiont",
    }
}

/// The five conserved metabolic-budget channels (ADR-013 §Decision pillar 2, DECISIONS.md:537), in fixed
/// declaration order. The INDEX is the channel id — the load-bearing contract F3/F4 read by index, never by
/// name (never a `HashMap`, inv #3). DECISIONS.md:537 pins only the SHAPE `[u16; 5]` summing to 1000 permille;
/// these names take `Acquisition` as the F3 `uptake` tap (channel 0) and `Maintenance` as the always-positive
/// floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BudgetChannel {
    /// Resource-uptake apparatus — the F3 `uptake` tap (DECISIONS.md:543).
    Acquisition = 0,
    /// Somatic growth / size.
    Growth = 1,
    /// Offspring output (fecundity).
    Reproduction = 2,
    /// Baseline upkeep — the always-positive floor slice.
    Maintenance = 3,
    /// Stress / drought / hardiness investment.
    Defense = 4,
}

impl BudgetChannel {
    /// The channels in fixed declaration order — `[u16; N]` budgets are index-aligned to this.
    pub const ALL: [BudgetChannel; 5] = [
        BudgetChannel::Acquisition,
        BudgetChannel::Growth,
        BudgetChannel::Reproduction,
        BudgetChannel::Maintenance,
        BudgetChannel::Defense,
    ];
    /// Channel count.
    pub const N: usize = 5;
}

/// A species' expressed ecological strategy (ADR-013 F2). Cached in the species registry; NOT folded into
/// `hash_world` and NOT read by `selection` at F2 (F3 is its first reader), so it is hash-neutral. Derived
/// `Eq` over all-integer fields → bit-comparable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Strategy {
    /// Permille shares over [`BudgetChannel::ALL`], summing to EXACTLY [`fixed::PERMILLE`] (1000). Built ONLY
    /// via [`fixed::normalize_permille`] so the simplex invariant holds by construction.
    /// `[acq, grow, repro, maint, def]`.
    pub budget: [u16; 5],
    /// How this species earns its joules — sets which F3 metabolic tap it draws from.
    pub role: TrophicRole,
    /// Per-resource-channel uptake affinity on the fixed `u16` grid `[0, UNIT_SCALE]`, one slot per
    /// [`crate::resource::RESOURCE_CHANNELS`] (light, free_nutrient, detritus). NOT a simplex — a preference
    /// profile.
    pub affinity: [u16; crate::resource::RESOURCE_CHANNELS],
    /// Per-org MINERALIZATION fraction in permille `[0, 1000]` (ADR-013 F4): of a Decomposer's granted
    /// detritus-J, the share re-deposited into the SAME cell's `free_nutrient` (the rest is RESPIRED as the
    /// decomposer's own metabolism). Anchored on [`Trait::AcetateOverflow`] (pta, GO-8959 — the Layer-3
    /// detritus/mineralization tap), so a `pta` CRISPRi Knockdown throttles per-org mineralization. Read ONLY
    /// by the F4 `mineralize` system for a Decomposer; inert for every other role.
    pub mineralize_rate: u16,
    /// Per-org PREDATION attack rate on the fixed `u16` grid `[0, UNIT_SCALE]` (ADR-013 F6, parallel to
    /// [`Strategy::mineralize_rate`]): gene-anchored on [`Trait::PredationCapacity`] (the Bdellovibrio
    /// `hit`/lytic-machinery attack genes, GO-8745), so a `hit`-locus CRISPRi Knockdown throttles the attack rate
    /// (the OVERSIGHT lever). Read ONLY by the F6 [`crate::trophic::predation`] system for a
    /// [`TrophicRole::Predator`]; INERT (`== 0`) for every other role — absent anchor → `0`, exactly like
    /// `mineralize_rate` is inert off a Decomposer. Keeps predator demand OUT of metabolism's resource-channel
    /// apportion (a predator taps no abiotic pool — the kernel is its sole income).
    pub predation_rate: u16,
    /// Is this species a SPORE-FORMER (ADR-019 S4)? Gene-anchored on [`Trait::SporulationCapacity`] (the
    /// spo0A/sigF endospore or brlA conidiation cascade), set `true` iff the expressed sporulation trait is
    /// non-zero — exactly the inert-off-role precedent of `predation_rate`/`mineralize_rate`. Read ONLY by the
    /// S4 sporulation arms ([`crate::trophic::sporulation_split`] in `region_cull` + `reproduce_or_die`) and the
    /// [`crate::trophic::germinate`] pass. `false` for every plant/ecoli/predator genome (no anchor → `0` →
    /// `false`), so both sporulation branches and germination are byte-identical no-ops on a non-spore-former.
    pub spore_former: bool,
    /// Per-org HOST-COUPLING draw rate on the fixed `u16` grid `[0, UNIT_SCALE]` (ADR-019 S5, parallel to
    /// [`Strategy::predation_rate`]): gene-anchored on [`Trait::SymbiosisCapacity`] (the host-exchange /
    /// amino-acid-provisioning machinery), so a CRISPRi Knockdown of that locus throttles the coupling rate
    /// (the OVERSIGHT lever). Read ONLY by the S5 [`crate::trophic::host_coupling`] system for a
    /// [`TrophicRole::ObligateSymbiont`]; INERT (`== 0`) for every other role — absent anchor → `0`, exactly
    /// like `predation_rate` is inert off a Predator. A symbiont taps no abiotic pool, so this coupling draw is
    /// its SOLE income (the host→symbiont arm); a benign-low value lets coexistence be reachable, not a pure drain.
    pub host_draw_rate: u16,
}

/// The channel→anchor-trait pairing (declaration-ordered, parallel to [`BudgetChannel::ALL`]). Each channel's
/// raw weight is the value of its anchor trait in the expressed [`Phenotype`]; a species' own [`TraitMap`]
/// only binds the traits it has, so an absent anchor yields the documented `0.0` (never a panic). Both the
/// plant (9-trait) and E. coli (5-trait) maps express through EXISTING traits:
///   Acquisition  ← LeafSize             (plant light-capture proxy) / GlucoseUptake (ecoli)
///   Growth       ← GrowthRate
///   Reproduction ← Fecundity            / FermentationCapacity (ecoli)
///   Maintenance  ← DroughtTolerance     / RespirationMode      (ecoli)
///   Defense      ← Reflectance          / AcetateOverflow      (ecoli)
const CHANNEL_TRAITS: [(BudgetChannel, Trait); 5] = [
    (BudgetChannel::Acquisition, Trait::LeafSize),
    (BudgetChannel::Growth, Trait::GrowthRate),
    (BudgetChannel::Reproduction, Trait::Fecundity),
    (BudgetChannel::Maintenance, Trait::DroughtTolerance),
    (BudgetChannel::Defense, Trait::Reflectance),
];
// The ecoli anchors share the channel order; they bind via the species' own TraitMap, so the named plant
// anchors above are simply absent for E. coli (→ 0.0) while its own anchors express in their slots. This
// const keeps the per-channel anchor lookup ordered and HashMap-free (inv #3).
const CHANNEL_TRAITS_ECOLI: [(BudgetChannel, Trait); 5] = [
    (BudgetChannel::Acquisition, Trait::GlucoseUptake),
    (BudgetChannel::Growth, Trait::GrowthRate),
    (BudgetChannel::Reproduction, Trait::FermentationCapacity),
    (BudgetChannel::Maintenance, Trait::RespirationMode),
    (BudgetChannel::Defense, Trait::AcetateOverflow),
];
/// The PREDATOR (Bdellovibrio) channel anchors (ADR-013 F6) — a PURE ADDITION that leaves the plant/E. coli
/// anchor tables untouched (so their express paths are byte-unperturbed). `Growth` is the only anchor the small
/// Bdellovibrio map binds (`gltA`/GrowthRate); the other four channels are absent in its TraitMap → `0.0`, so
/// `normalize_permille` puts all the budget on Growth (a lean predator that funds growth from kept predation-J).
/// `Acquisition` anchors on `PredationCapacity` so an attack-rate-heavy predator also invests an acquisition
/// slice — harmless, since the F6 kernel reads the dedicated `predation_rate`, not this budget channel.
const CHANNEL_TRAITS_PREDATOR: [(BudgetChannel, Trait); 5] = [
    (BudgetChannel::Acquisition, Trait::PredationCapacity),
    (BudgetChannel::Growth, Trait::GrowthRate),
    (BudgetChannel::Reproduction, Trait::PredationCapacity),
    (BudgetChannel::Maintenance, Trait::GrowthRate),
    (BudgetChannel::Defense, Trait::PredationCapacity),
];
/// The OBLIGATE-SYMBIONT (Carsonella / Syn3.0) channel anchors (ADR-019 S5) — a PURE ADDITION leaving the
/// plant/E. coli/predator anchor tables untouched. A reduced, host-dependent endosymbiont funds a LEAN budget:
/// `Growth` + `Maintenance` anchor on `GrowthRate` (the kept host-J funds growth + upkeep), the other three
/// channels anchor on `SymbiosisCapacity` so a symbiont that invests in the host-exchange machinery also keeps a
/// valid simplex (the F6-predator pattern). The kernel reads the dedicated `host_draw_rate`, not this budget —
/// this table only ensures the kept-J it gains splits over a valid 1000-permille simplex.
const CHANNEL_TRAITS_SYMBIONT: [(BudgetChannel, Trait); 5] = [
    (BudgetChannel::Acquisition, Trait::SymbiosisCapacity),
    (BudgetChannel::Growth, Trait::GrowthRate),
    (BudgetChannel::Reproduction, Trait::SymbiosisCapacity),
    (BudgetChannel::Maintenance, Trait::GrowthRate),
    (BudgetChannel::Defense, Trait::SymbiosisCapacity),
];

/// The uniform fallback budget (`[200; 5]`, Σ = 1000) substituted when every channel anchor expresses `0.0`
/// (so `normalize_permille` would return all-zero). Keeps a cached [`Strategy`] ALWAYS a valid 1000-simplex.
/// Never read by selection → hash-neutral regardless.
const UNIFORM_BUDGET: [u16; 5] = [200, 200, 200, 200, 200];

/// Express a genome into its ecological [`Strategy`] (ADR-013 F2). PURE + deterministic, drawing ZERO from any
/// `SimRng` (it calls only [`OntologyMap::express`], [`fixed::to_unit_u16`], and [`fixed::normalize_permille`],
/// all pure integer/IEEE math). The genome is read ONLY through `map.express()` — the invariant-#2-blessed
/// path — so plant (9-trait) and E. coli (5-trait) genomes both feed in UNCHANGED; no new genome traversal.
///
/// Steps (all ordered, no `HashMap`, no RNG):
/// 1. express the phenotype once via the EXISTING engine;
/// 2. pull the five channel anchor weights by NAMED [`Trait`] lookup (absent trait → `0.0`, never a panic);
/// 3. quantize each `[0, 1]` weight to the `u16` grid via [`fixed::to_unit_u16`] (the single audited f64→int
///    chokepoint), widening to `[u64; 5]`;
/// 4. normalize to a permille simplex via [`fixed::normalize_permille`] (largest-remainder; ties → lowest
///    index; conserves the total EXACTLY) → `budget` sums to EXACTLY 1000 by construction;
/// 5. all-zero weights → substitute the uniform [`UNIFORM_BUDGET`] fallback (always a valid simplex);
/// 6. `affinity` is a preference profile (NOT normalized), index-aligned to `RESOURCE_CHANNELS`;
/// 7. `role` is the caller-supplied argument (from [`role_for`]), NOT re-derived from scalars.
#[must_use]
pub fn express_strategy(map: &OntologyMap, genome: &Genome, role: TrophicRole) -> Strategy {
    let p = map.express(genome);
    // The anchor table is chosen by role so each species reads ITS OWN traits; absent anchors → 0.0 anyway,
    // but selecting the table keeps the lookup tight and the intent explicit (ordered, HashMap-free, inv #3).
    let anchors = match role {
        TrophicRole::Heterotroph | TrophicRole::Decomposer | TrophicRole::Mixotroph => {
            &CHANNEL_TRAITS_ECOLI
        }
        TrophicRole::Predator => &CHANNEL_TRAITS_PREDATOR,
        TrophicRole::ObligateSymbiont => &CHANNEL_TRAITS_SYMBIONT,
        TrophicRole::Autotroph => &CHANNEL_TRAITS,
    };
    // Raw channel weights on the u16 grid, ordered by BudgetChannel::ALL.
    let mut raw5 = [0u64; 5];
    for (i, (_ch, t)) in anchors.iter().enumerate() {
        let w = p.get(*t).unwrap_or(0.0).clamp(0.0, 1.0);
        raw5[i] = u64::from(fixed::to_unit_u16(w));
    }
    let v = fixed::normalize_permille(&raw5); // statically len 5; ties → lowest index; Σ == PERMILLE or 0.
    let budget: [u16; 5] = if v.iter().map(|&x| u32::from(x)).sum::<u32>() == 0 {
        UNIFORM_BUDGET // degenerate all-zero anchors → documented uniform simplex (never an invalid budget).
    } else {
        [v[0], v[1], v[2], v[3], v[4]]
    };
    // Affinity: a PREFERENCE profile (NOT a simplex), 1:1 with RESOURCE_CHANNELS (light, free_nutrient,
    // detritus). LeafSize → light (plant light-capture), GrowthRate → free_nutrient (autotroph uptake),
    // GlucoseUptake → detritus (ADR-013 F4: the decomposer detritus-pull, anchored on ptsG/GO-8982 — absent
    // for a plant genome → 0.0, present for E. coli). All three quantized through the audited f64→int chokepoint.
    let affinity: [u16; crate::resource::RESOURCE_CHANNELS] = [
        fixed::to_unit_u16(p.get(Trait::LeafSize).unwrap_or(0.0).clamp(0.0, 1.0)),
        fixed::to_unit_u16(p.get(Trait::GrowthRate).unwrap_or(0.0).clamp(0.0, 1.0)),
        fixed::to_unit_u16(p.get(Trait::GlucoseUptake).unwrap_or(0.0).clamp(0.0, 1.0)),
    ];
    // Mineralization fraction (ADR-013 F4): AcetateOverflow → pta (GO-8959), the gene-driven share of granted
    // detritus-J a Decomposer re-deposits as free_nutrient. Quantized [0,1]→u16, then expressed as PERMILLE so
    // it index-aligns with the budget grid. Absent for a plant genome (→ 0), so it is inert off a Decomposer.
    let mineralize_rate = ((u64::from(fixed::to_unit_u16(
        p.get(Trait::AcetateOverflow).unwrap_or(0.0).clamp(0.0, 1.0),
    )) * u64::from(fixed::PERMILLE))
        / u64::from(fixed::UNIT_SCALE)) as u16;
    // Predation attack rate (ADR-013 F6): PredationCapacity → the F6 predation kernel's per-org attack-rate
    // lever, quantized [0,1]→u16 directly on the grid (parallel to the affinity slots, NOT a permille). Absent
    // for any non-predator genome (→ 0), so it is inert off a Predator — exactly like mineralize_rate off a
    // Decomposer. A `hit`-locus CRISPRi Knockdown (PredationCapacity→0) drives it to 0 (the OVERSIGHT lever).
    let predation_rate = fixed::to_unit_u16(
        p.get(Trait::PredationCapacity)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0),
    );
    // Spore-former marker (ADR-019 S4): SporulationCapacity → the spore/germination mechanic's enabling flag,
    // `true` iff the expressed sporulation trait quantizes above zero (parallel to predation_rate's anchor read).
    // Absent for any non-spore-former genome (→ 0 → false), so it is inert exactly like predation_rate off a
    // Predator. A spo0A/brlA-anchored cascade off-state (SporulationCapacity→0) drives it to false.
    let spore_former = fixed::to_unit_u16(
        p.get(Trait::SporulationCapacity)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0),
    ) > 0;
    // Host-coupling draw rate (ADR-019 S5): SymbiosisCapacity → the S5 host_coupling kernel's per-org draw-rate
    // lever, quantized [0,1]→u16 directly on the grid (parallel to predation_rate). Absent for any non-symbiont
    // genome (→ 0), so it is inert off an ObligateSymbiont — exactly like predation_rate off a Predator. A
    // SymbiosisCapacity-anchored CRISPRi Knockdown (→0) drives it to 0 (the OVERSIGHT lever).
    let host_draw_rate = fixed::to_unit_u16(
        p.get(Trait::SymbiosisCapacity)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0),
    );
    Strategy {
        budget,
        role,
        affinity,
        mineralize_rate,
        predation_rate,
        spore_former,
        host_draw_rate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn express_is_deterministic_for_fixed_genome() {
        let g = genome::sample_genome();
        // AC1: same genome expressed twice ⇒ identical phenotype.
        assert_eq!(WeightedSumMap.express(&g), WeightedSumMap.express(&g));
    }

    #[test]
    fn all_trait_values_in_unit_range() {
        let g = genome::sample_genome();
        let p = WeightedSumMap.express(&g);
        assert_eq!(p.values.len(), Trait::ALL.len());
        for (t, v) in &p.values {
            assert!((0.0..=1.0).contains(v), "trait {t:?} = {v} out of [0,1]");
        }
    }

    #[test]
    fn get_returns_each_trait() {
        let g = genome::sample_genome();
        let p = WeightedSumMap.express(&g);
        for t in Trait::ALL {
            assert!(p.get(t).is_some(), "missing trait {t:?}");
        }
    }

    #[test]
    fn growth_rate_tracks_first_parameter() {
        // p0 of sample_genome is Numeric{value:0.6,0..1} → unit scalar 0.6; GrowthRate = 1.0 * p0.
        let g = genome::sample_genome();
        let p = WeightedSumMap.express(&g);
        assert!((p.get(Trait::GrowthRate).unwrap() - 0.6).abs() < 1e-9);
    }

    #[test]
    fn f2_default_plant_map_pins_expression() {
        // F2 (ADR-017): the ontology re-key must express sample_genome BYTE-IDENTICALLY to the pre-F2 flat
        // anchoring — pinning every trait value proves the re-key is hash-neutral (allele_freq unchanged).
        let g = genome::sample_genome();
        let p = WeightedSumMap.express(&g);
        let expect = [
            (Trait::GrowthRate, 0.6),
            (Trait::Stature, 0.5),
            (Trait::Branchiness, 0.5),
            (Trait::LeafSize, 0.5),
            (Trait::LeafHue, 0.45),
            (Trait::Reflectance, 0.5),
            (Trait::Fecundity, 0.4),
            (Trait::DroughtTolerance, 0.5),
            (Trait::KillSwitchLinkage, 0.0), // Bool(false) → 0.0
        ];
        assert_eq!(p.values.len(), expect.len());
        for ((t, v), (et, ev)) in p.values.iter().zip(expect.iter()) {
            assert_eq!(t, et, "phenotype must stay in Trait::ALL order");
            assert!((v - ev).abs() < 1e-9, "{t:?} = {v}, expected {ev}");
        }
        // The wrapper is exactly OntologyMap(default_plant_trait_map).
        assert_eq!(p, OntologyMap::new(default_plant_trait_map()).express(&g));
    }

    #[test]
    fn by_go_anchor_resolves_first_matching_locus() {
        // An ontology-driven binding reads the FIRST locus whose go_refs contains the anchor (Vec order):
        // sample_genome's L0 carries GO 40007, so ByGoAnchor(40007)/P0 reads L0/P0 = 0.6.
        let g = genome::sample_genome();
        let map = TraitMap(vec![TraitBinding {
            trait_: Trait::GrowthRate,
            locus: LocusSelector::ByGoAnchor(GoTermId(40007)),
            param: ParamId(0),
        }]);
        let p = OntologyMap::new(map).express(&g);
        assert!((p.get(Trait::GrowthRate).unwrap() - 0.6).abs() < 1e-9);
    }

    #[test]
    fn trait_map_for_selects_by_key() {
        // Ordered match (inv #3): the E. coli key → microbe map; every other key → the default plant map.
        assert_eq!(trait_map_for("ecoli-core"), ecoli_trait_map());
        assert_eq!(trait_map_for("default"), default_plant_trait_map());
        assert_eq!(trait_map_for("unknown-species"), default_plant_trait_map());
    }

    #[test]
    fn missing_binding_expresses_zero_not_panic() {
        // A binding whose locus/param is absent yields a documented 0.0 (so an arbitrary loaded genome is safe).
        let g = genome::sample_genome();
        let map = TraitMap(vec![TraitBinding {
            trait_: Trait::GrowthRate,
            locus: LocusSelector::ByIndex(LocusId(99)),
            param: ParamId(0),
        }]);
        assert_eq!(
            OntologyMap::new(map).express(&g).get(Trait::GrowthRate),
            Some(0.0)
        );
    }

    // ── ADR-013 F2: Strategy / TrophicRole / express_strategy ──────────────────────────────────────

    /// A genome whose 5 channel anchors all express EXACTLY a given uniform value, via a tiny synthetic
    /// `OntologyMap` whose bindings all read the same parameter. Used to drive the express path at known
    /// channel weights through `express_strategy`.
    fn uniform_anchor_map(value: f64) -> (OntologyMap, Genome) {
        // One locus, one parameter = `value`; every channel anchor (and the affinity anchors) binds to it.
        let g = Genome {
            version: 2,
            loci: vec![genome::Locus {
                id: LocusId(0),
                name: "anchor".to_string(),
                sequence: genome::DnaSequence::new(*b"ACGTGGACGTTTTAGGCCGG").unwrap(),
                parameters: vec![genome::Parameter {
                    id: ParamId(0),
                    value: genome::ParamValue::Numeric {
                        value,
                        min: 0.0,
                        max: 1.0,
                    },
                }],
                tags: genome::OntologyTags {
                    so_term: genome::SoTermId(704),
                    go_refs: vec![],
                },
            }],
        };
        let b = |t| TraitBinding {
            trait_: t,
            locus: LocusSelector::ByIndex(LocusId(0)),
            param: ParamId(0),
        };
        // Bind all five plant channel anchors + the two affinity anchors to the single parameter.
        let map = OntologyMap::new(TraitMap(vec![
            b(Trait::LeafSize),
            b(Trait::GrowthRate),
            b(Trait::Fecundity),
            b(Trait::DroughtTolerance),
            b(Trait::Reflectance),
        ]));
        (map, g)
    }

    #[test]
    fn strategy_budget_is_1000_simplex() {
        // Plant: sample_genome under the default plant map.
        let g = genome::sample_genome();
        let plant_map = OntologyMap::new(default_plant_trait_map());
        let s = express_strategy(&plant_map, &g, TrophicRole::Autotroph);
        assert_eq!(s.budget.len(), 5);
        assert_eq!(
            s.budget.iter().map(|&x| u32::from(x)).sum::<u32>(),
            fixed::PERMILLE,
            "plant budget must be a 1000-permille simplex"
        );
        // E. coli: the ecoli map over sample_genome — none of its GO anchors match, so every channel weight is
        // 0.0 → the documented uniform fallback, still a valid 1000-simplex (never all-zero, never a panic).
        let ecoli_map = OntologyMap::new(ecoli_trait_map());
        let se = express_strategy(&ecoli_map, &g, TrophicRole::Heterotroph);
        assert_eq!(
            se.budget.iter().map(|&x| u32::from(x)).sum::<u32>(),
            fixed::PERMILLE,
            "ecoli budget must be a 1000-permille simplex"
        );
    }

    #[test]
    fn strategy_ties_break_to_lowest_index() {
        // All five anchors EQUAL (0.5) → normalize_permille floors 200 each (=1000) with zero leftover, so the
        // budget is exactly uniform; the tie-break path is the same apportion ties-to-lowest-index contract.
        let (map, g) = uniform_anchor_map(0.5);
        let s = express_strategy(&map, &g, TrophicRole::Autotroph);
        assert_eq!(s.budget, [200, 200, 200, 200, 200]);
        // An uneven case where leftover quanta exist: weights [1,1,1,1,1] on the u16 grid → 1000 splits 200
        // each (no leftover). To exercise a real leftover, drive raw weights that DON'T divide evenly: feed
        // raw5 = [1,1,1] directly through normalize_permille and assert the extra quantum lands on index 0.
        assert_eq!(fixed::normalize_permille(&[1, 1, 1])[0], 334);
        assert_eq!(&fixed::normalize_permille(&[1, 1, 1])[1..], &[333, 333]);
    }

    #[test]
    fn strategy_all_zero_weights_fall_back_to_uniform() {
        // Every channel anchor expresses 0.0 → normalize_permille returns all-zero; express_strategy
        // substitutes the documented uniform [200;5] fallback (sums to 1000, never invalid, never a panic).
        let (map, g) = uniform_anchor_map(0.0);
        let s = express_strategy(&map, &g, TrophicRole::Autotroph);
        assert_eq!(s.budget, [200, 200, 200, 200, 200]);
        assert_eq!(s.budget.iter().map(|&x| u32::from(x)).sum::<u32>(), 1000);
    }

    #[test]
    fn express_strategy_is_deterministic() {
        // Derived Eq over integer fields (no float-eq): same inputs → byte-identical Strategy (inv #3).
        let g = genome::sample_genome();
        let m = OntologyMap::new(default_plant_trait_map());
        assert_eq!(
            express_strategy(&m, &g, TrophicRole::Autotroph),
            express_strategy(&m, &g, TrophicRole::Autotroph)
        );
    }

    #[test]
    fn per_species_expression_pinned() {
        // F2 analogue of `f2_default_plant_map_pins_expression`: lock the genome→Strategy mapping so a
        // channel-anchor regression is caught. sample_genome under the default plant map expresses
        //   Acquisition<-LeafSize=0.5, Growth<-GrowthRate=0.6, Reproduction<-Fecundity=0.4,
        //   Maintenance<-DroughtTolerance=0.5, Defense<-Reflectance=0.5.
        let g = genome::sample_genome();
        let plant_map = OntologyMap::new(default_plant_trait_map());
        let s = express_strategy(&plant_map, &g, TrophicRole::Autotroph);
        assert_eq!(s.budget, [200, 240, 160, 200, 200], "pinned plant budget");
        assert_eq!(s.role, TrophicRole::Autotroph);
        // ADR-013 F4: detritus affinity (slot 2) anchors on GlucoseUptake — absent for the plant genome → 0;
        // mineralize_rate anchors on AcetateOverflow — also absent for a plant → 0 (inert off a Decomposer).
        assert_eq!(s.affinity, [32767, 39321, 0], "pinned plant affinity");
        assert_eq!(
            s.mineralize_rate, 0,
            "plant has no AcetateOverflow → mineralize_rate 0"
        );
        // ADR-013 F6: the plant has no PredationCapacity anchor → predation_rate 0 (inert off a Predator).
        assert_eq!(
            s.predation_rate, 0,
            "plant has no PredationCapacity → predation_rate 0"
        );
        // E. coli role pin (its budget is the uniform fallback over sample_genome — anchors don't match).
        let ecoli_map = OntologyMap::new(ecoli_trait_map());
        let se = express_strategy(&ecoli_map, &g, TrophicRole::Decomposer);
        assert_eq!(se.role, TrophicRole::Decomposer);
        assert_eq!(se.budget, [200, 200, 200, 200, 200]);
    }

    #[test]
    fn f4_decomposer_mineralize_rate_and_detritus_affinity_are_gene_driven() {
        // ADR-013 F4: a Decomposer's detritus affinity comes from GlucoseUptake (ptsG) and its mineralize_rate
        // from AcetateOverflow (pta). A synthetic map binding BOTH to a known value drives them off the genome —
        // proving the F4 levers are gene-driven (a pta/ptsG CRISPRi edit moves them), not constants.
        let g = Genome {
            version: 2,
            loci: vec![genome::Locus {
                id: LocusId(0),
                name: "anchor".to_string(),
                sequence: genome::DnaSequence::new(*b"ACGTGGACGTTTTAGGCCGG").unwrap(),
                parameters: vec![genome::Parameter {
                    id: ParamId(0),
                    value: genome::ParamValue::Numeric {
                        value: 0.5,
                        min: 0.0,
                        max: 1.0,
                    },
                }],
                tags: genome::OntologyTags {
                    so_term: genome::SoTermId(704),
                    go_refs: vec![],
                },
            }],
        };
        let b = |t| TraitBinding {
            trait_: t,
            locus: LocusSelector::ByIndex(LocusId(0)),
            param: ParamId(0),
        };
        let map = OntologyMap::new(TraitMap(vec![
            b(Trait::GlucoseUptake),
            b(Trait::AcetateOverflow),
        ]));
        let s = express_strategy(&map, &g, TrophicRole::Decomposer);
        // GlucoseUptake=0.5 → detritus affinity = to_unit_u16(0.5) = 32767.
        assert_eq!(
            s.affinity[2], 32767,
            "detritus affinity tracks GlucoseUptake (ptsG)"
        );
        // AcetateOverflow=0.5 → mineralize_rate = to_unit_u16(0.5)*1000/65535 = 32767*1000/65535 = 499 permille.
        assert_eq!(
            s.mineralize_rate, 499,
            "mineralize_rate tracks AcetateOverflow (pta)"
        );
        // Knock the pta anchor down → mineralize_rate falls (the CRISPRi ripple lever).
        let mut g_ko = g.clone();
        if let genome::ParamValue::Numeric { value, .. } = &mut g_ko.loci[0].parameters[0].value {
            *value = 0.1;
        }
        let s_ko = express_strategy(&map, &g_ko, TrophicRole::Decomposer);
        assert!(
            s_ko.mineralize_rate < s.mineralize_rate,
            "knocking down pta/AcetateOverflow lowers mineralize_rate ({} -> {})",
            s.mineralize_rate,
            s_ko.mineralize_rate
        );
    }

    #[test]
    fn f6_predator_predation_rate_is_gene_driven_and_inert_off_predator() {
        // ADR-013 F6: a Predator's predation_rate comes from PredationCapacity (the hit/lytic attack genes). A
        // synthetic map binding it to a known value drives the attack-rate lever off the genome — proving a
        // hit-locus CRISPRi edit moves it. INERT (0) for a plant genome (no PredationCapacity anchor).
        let g = Genome {
            version: 2,
            loci: vec![genome::Locus {
                id: LocusId(0),
                name: "anchor".to_string(),
                sequence: genome::DnaSequence::new(*b"ACGTGGACGTTTTAGGCCGG").unwrap(),
                parameters: vec![genome::Parameter {
                    id: ParamId(0),
                    value: genome::ParamValue::Numeric {
                        value: 0.5,
                        min: 0.0,
                        max: 1.0,
                    },
                }],
                tags: genome::OntologyTags {
                    so_term: genome::SoTermId(704),
                    go_refs: vec![],
                },
            }],
        };
        let b = |t| TraitBinding {
            trait_: t,
            locus: LocusSelector::ByIndex(LocusId(0)),
            param: ParamId(0),
        };
        let map = OntologyMap::new(TraitMap(vec![b(Trait::PredationCapacity)]));
        let s = express_strategy(&map, &g, TrophicRole::Predator);
        // PredationCapacity=0.5 → predation_rate = to_unit_u16(0.5) = 32767.
        assert_eq!(
            s.predation_rate, 32767,
            "predation_rate tracks PredationCapacity (the hit/lytic attack lever)"
        );
        assert_eq!(s.role, TrophicRole::Predator);
        // Knock the hit anchor down → predation_rate falls (the CRISPRi throttle lever).
        let mut g_ko = g.clone();
        if let genome::ParamValue::Numeric { value, .. } = &mut g_ko.loci[0].parameters[0].value {
            *value = 0.1;
        }
        let s_ko = express_strategy(&map, &g_ko, TrophicRole::Predator);
        assert!(
            s_ko.predation_rate < s.predation_rate,
            "knocking down the hit/PredationCapacity anchor lowers predation_rate ({} -> {})",
            s.predation_rate,
            s_ko.predation_rate
        );
        // INERT off a Predator: the same map under an Autotroph role still expresses predation_rate from the
        // bound anchor (the field tracks the gene, not the role) — but the kernel only READS it for a Predator,
        // and a plant genome (no PredationCapacity binding) yields 0:
        let s_plant = express_strategy(
            &OntologyMap::new(default_plant_trait_map()),
            &genome::sample_genome(),
            TrophicRole::Autotroph,
        );
        assert_eq!(s_plant.predation_rate, 0, "no anchor → predation_rate 0");
    }

    #[test]
    fn s4_spore_former_flag_is_gene_anchored_and_inert_without_the_anchor() {
        // ADR-019 S4: spore_former is `true` iff SporulationCapacity (the spo0A/sigF or brlA cascade) expresses
        // non-zero — exactly the inert-off-role precedent of predation_rate/mineralize_rate. A synthetic map
        // binding the anchor marks the species a spore-former; knocking the anchor to 0 clears the flag; a plant
        // genome (no anchor) is NEVER a spore-former (byte-neutral on the pinned single-plant run).
        let g = Genome {
            version: 2,
            loci: vec![genome::Locus {
                id: LocusId(0),
                name: "anchor".to_string(),
                sequence: genome::DnaSequence::new(*b"ACGTGGACGTTTTAGGCCGG").unwrap(),
                parameters: vec![genome::Parameter {
                    id: ParamId(0),
                    value: genome::ParamValue::Numeric {
                        value: 0.7,
                        min: 0.0,
                        max: 1.0,
                    },
                }],
                tags: genome::OntologyTags {
                    so_term: genome::SoTermId(704),
                    go_refs: vec![],
                },
            }],
        };
        let b = |t| TraitBinding {
            trait_: t,
            locus: LocusSelector::ByIndex(LocusId(0)),
            param: ParamId(0),
        };
        let map = OntologyMap::new(TraitMap(vec![b(Trait::SporulationCapacity)]));
        let s = express_strategy(&map, &g, TrophicRole::Decomposer);
        assert!(
            s.spore_former,
            "a non-zero SporulationCapacity marks the species a spore-former"
        );
        // Knock the cascade anchor fully off (the off-state) → not a spore-former.
        let mut g_off = g.clone();
        if let genome::ParamValue::Numeric { value, .. } = &mut g_off.loci[0].parameters[0].value {
            *value = 0.0;
        }
        let s_off = express_strategy(&map, &g_off, TrophicRole::Decomposer);
        assert!(
            !s_off.spore_former,
            "SporulationCapacity → 0 (cascade off) clears the spore-former flag"
        );
        // A plant genome (no SporulationCapacity binding) is NEVER a spore-former — the inert-off-role guarantee
        // that keeps the pinned single-plant run byte-neutral.
        let s_plant = express_strategy(
            &OntologyMap::new(default_plant_trait_map()),
            &genome::sample_genome(),
            TrophicRole::Autotroph,
        );
        assert!(
            !s_plant.spore_former,
            "no SporulationCapacity anchor → not a spore-former (pinned-run byte-neutrality)"
        );
    }

    #[test]
    fn s5_host_draw_rate_is_gene_anchored_and_inert_without_the_anchor() {
        // ADR-019 S5: host_draw_rate is the host-coupling exchange lever, anchored on SymbiosisCapacity (the
        // amino-acid-provisioning machinery) — exactly the inert-off-role precedent of predation_rate. A
        // synthetic map binding the anchor drives the rate off the genome; knocking it to 0 zeroes the rate; a
        // plant genome (no anchor) is NEVER a coupler (byte-neutral on the pinned single-plant run).
        let g = Genome {
            version: 2,
            loci: vec![genome::Locus {
                id: LocusId(0),
                name: "anchor".to_string(),
                sequence: genome::DnaSequence::new(*b"ACGTGGACGTTTTAGGCCGG").unwrap(),
                parameters: vec![genome::Parameter {
                    id: ParamId(0),
                    value: genome::ParamValue::Numeric {
                        value: 0.5,
                        min: 0.0,
                        max: 1.0,
                    },
                }],
                tags: genome::OntologyTags {
                    so_term: genome::SoTermId(704),
                    go_refs: vec![],
                },
            }],
        };
        let b = |t| TraitBinding {
            trait_: t,
            locus: LocusSelector::ByIndex(LocusId(0)),
            param: ParamId(0),
        };
        let map = OntologyMap::new(TraitMap(vec![b(Trait::SymbiosisCapacity)]));
        let s = express_strategy(&map, &g, TrophicRole::ObligateSymbiont);
        // SymbiosisCapacity=0.5 → host_draw_rate = to_unit_u16(0.5) = 32767.
        assert_eq!(
            s.host_draw_rate, 32767,
            "host_draw_rate tracks SymbiosisCapacity (the host-exchange lever)"
        );
        assert_eq!(s.role, TrophicRole::ObligateSymbiont);
        // The symbiont budget is a valid 1000-simplex (the lean CHANNEL_TRAITS_SYMBIONT table).
        assert_eq!(
            s.budget.iter().map(|&x| u32::from(x)).sum::<u32>(),
            fixed::PERMILLE,
            "symbiont budget must be a 1000-permille simplex"
        );
        // Knock the provisioning anchor down → host_draw_rate falls (the CRISPRi throttle lever).
        let mut g_ko = g.clone();
        if let genome::ParamValue::Numeric { value, .. } = &mut g_ko.loci[0].parameters[0].value {
            *value = 0.0;
        }
        let s_ko = express_strategy(&map, &g_ko, TrophicRole::ObligateSymbiont);
        assert_eq!(
            s_ko.host_draw_rate, 0,
            "knocking SymbiosisCapacity to 0 zeroes the host-coupling draw rate"
        );
        // A plant genome (no SymbiosisCapacity binding) is NEVER a coupler — the inert-off-role guarantee.
        let s_plant = express_strategy(
            &OntologyMap::new(default_plant_trait_map()),
            &genome::sample_genome(),
            TrophicRole::Autotroph,
        );
        assert_eq!(
            s_plant.host_draw_rate, 0,
            "no SymbiosisCapacity anchor → host_draw_rate 0 (pinned-run byte-neutrality)"
        );
    }

    #[test]
    fn s5_symbiont_role_and_map_dispatch_by_key() {
        // ADR-019 S5: the data-driven seams light up for the obligate-symbiont keys without new roster plumbing.
        assert_eq!(role_for("carsonella"), TrophicRole::ObligateSymbiont);
        assert_eq!(role_for("syn3"), TrophicRole::ObligateSymbiont);
        assert_eq!(
            role_from_str("symbiont"),
            Some(TrophicRole::ObligateSymbiont)
        );
        assert_eq!(
            role_from_str("Obligate_Symbiont"),
            Some(TrophicRole::ObligateSymbiont),
            "case-insensitive"
        );
        assert_eq!(trait_map_for("carsonella"), symbiont_trait_map());
        assert_eq!(trait_map_for("syn3"), symbiont_trait_map());
        // The niche.trophic_role override path the JSON boundary uses.
        assert_eq!(
            role_from_override(Some("symbiont"), "carsonella"),
            TrophicRole::ObligateSymbiont
        );
        assert_eq!(
            role_from_override(None, "carsonella"),
            TrophicRole::ObligateSymbiont
        );
        // An ObligateSymbiont is NOT eligible prey (a Bdellovibrio co-located with an endosymbiont cannot eat
        // it — the host is the cullable target). The is_prey fall-through is the mechanism.
        assert!(
            !is_prey(TrophicRole::ObligateSymbiont),
            "an endosymbiont sheltered in its host is provably not eaten by a co-located predator"
        );
    }

    #[test]
    fn is_prey_encodes_bdellovibrio_host_range() {
        // ADR-013 F6: Bdellovibrio invades gram-negative BACTERIA — Heterotroph/Decomposer are prey; Autotroph
        // (plant cells), Mixotroph (algal/plant-ish), and Predator (no hyper-predation this slice) are NOT.
        assert!(is_prey(TrophicRole::Heterotroph));
        assert!(
            is_prey(TrophicRole::Decomposer),
            "E. coli (Decomposer) is prey"
        );
        assert!(!is_prey(TrophicRole::Autotroph), "plants are not prey");
        assert!(!is_prey(TrophicRole::Mixotroph));
        assert!(
            !is_prey(TrophicRole::Predator),
            "no hyper-predation this slice"
        );
    }

    #[test]
    fn bdellovibrio_role_and_map_dispatch_by_key() {
        // ADR-013 F6: the data-driven seams light up for "bdellovibrio" without new roster plumbing.
        assert_eq!(role_for("bdellovibrio"), TrophicRole::Predator);
        assert_eq!(role_from_str("predator"), Some(TrophicRole::Predator));
        assert_eq!(
            role_from_str("Predator"),
            Some(TrophicRole::Predator),
            "case-insensitive"
        );
        assert_eq!(trait_map_for("bdellovibrio"), bdellovibrio_trait_map());
        // The niche.trophic_role override path the JSON boundary uses.
        assert_eq!(
            role_from_override(Some("predator"), "bdellovibrio"),
            TrophicRole::Predator
        );
        assert_eq!(
            role_from_override(None, "bdellovibrio"),
            TrophicRole::Predator
        );
    }

    #[test]
    fn role_from_override_and_str_resolve() {
        // ADR-013 F4: the DATA-driven role override. A recognized string wins; an unknown/empty/None falls back
        // to role_for(key) — so a typo can never silently zero a niche.
        assert_eq!(role_from_str("decomposer"), Some(TrophicRole::Decomposer));
        assert_eq!(
            role_from_str("AutoTroph"),
            Some(TrophicRole::Autotroph),
            "case-insensitive"
        );
        assert_eq!(
            role_from_str("  mixotroph "),
            Some(TrophicRole::Mixotroph),
            "trimmed"
        );
        assert_eq!(role_from_str("nonsense"), None);
        // Override present + recognized → wins over the key default.
        assert_eq!(
            role_from_override(Some("autotroph"), "ecoli-core"),
            TrophicRole::Autotroph
        );
        // Override absent → role_for(key): ecoli-core defaults to Decomposer at F4.
        assert_eq!(
            role_from_override(None, "ecoli-core"),
            TrophicRole::Decomposer
        );
        // Override unrecognized → falls back to role_for(key).
        assert_eq!(
            role_from_override(Some("bogus"), "default"),
            TrophicRole::Autotroph
        );
    }

    #[test]
    fn role_to_str_is_the_inverse_of_role_from_str() {
        // The Variant-Lab export round-trip contract (Slice B): every TrophicRole serializes to a string that
        // role_from_str (and thus role_from_override) reads back to the SAME role, so a reseeded variant keeps
        // its trophic_role. Exhaustive over the role set — a new role MUST extend role_to_str (no `_` arm).
        for role in [
            TrophicRole::Autotroph,
            TrophicRole::Heterotroph,
            TrophicRole::Mixotroph,
            TrophicRole::Decomposer,
            TrophicRole::Predator,
            TrophicRole::ObligateSymbiont,
        ] {
            assert_eq!(
                role_from_str(role_to_str(role)),
                Some(role),
                "role_to_str({role:?}) must round-trip through role_from_str"
            );
        }
    }

    #[test]
    fn affinity_in_unit_grid() {
        // Affinity is a PREFERENCE profile on the u16 grid (NOT a simplex): light←LeafSize, free_nutrient←
        // GrowthRate, detritus←0.0. With both anchors bound to the same in-range value, each entry is exactly
        // its `to_unit_u16` quantization (and the detritus slot is always 0) — the grid contract.
        for &v in &[0.0_f64, 0.25, 0.5, 0.75, 1.0] {
            let (map, g) = uniform_anchor_map(v);
            let s = express_strategy(&map, &g, TrophicRole::Autotroph);
            let q = fixed::to_unit_u16(v);
            assert_eq!(s.affinity, [q, q, 0], "affinity off the grid for v={v}");
        }
    }

    #[test]
    fn role_for_selects_by_key() {
        // Ordered match (inv #3), parallel to trait_map_for_selects_by_key; unknown key degrades to plant.
        // ADR-013 F4 flips ecoli-core to Decomposer (the obligate plant→detritus→microbe loop).
        assert_eq!(role_for("ecoli-core"), TrophicRole::Decomposer);
        assert_eq!(role_for("default"), TrophicRole::Autotroph);
        assert_eq!(role_for("unknown"), TrophicRole::Autotroph);
        assert_eq!(TrophicRole::default(), TrophicRole::Autotroph);
    }

    #[test]
    fn strategy_matches_normalize_permille_for_same_weights() {
        // The Strategy path is now a real downstream CONSUMER of fixed::normalize_permille (closes the F-1
        // "LANDED but UNUSED" gap): the pinned plant budget IS what normalize_permille returns for the same
        // quantized channel weights.
        let raw5 = [
            u64::from(fixed::to_unit_u16(0.5)), // LeafSize
            u64::from(fixed::to_unit_u16(0.6)), // GrowthRate
            u64::from(fixed::to_unit_u16(0.4)), // Fecundity
            u64::from(fixed::to_unit_u16(0.5)), // DroughtTolerance
            u64::from(fixed::to_unit_u16(0.5)), // Reflectance
        ];
        let expect = fixed::normalize_permille(&raw5);
        let g = genome::sample_genome();
        let s = express_strategy(
            &OntologyMap::new(default_plant_trait_map()),
            &g,
            TrophicRole::Autotroph,
        );
        assert_eq!(&s.budget[..], &expect[..]);
    }
}
