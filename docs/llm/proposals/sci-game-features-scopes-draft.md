# Sci-based sim-game — feature roadmap + zoom-scopes architecture (DRAFT)

> **DRAFT — `sci-game-features-scopes-design`. DESIGN/RESEARCH ONLY, no code. Grounds on the ADR-013 joule engine.**

## Vision

A continuous ZOOM from one biological cell of a specimen → tissue → organ → whole body on the map → region → ecosystem. Every scope is a different read of the SAME genome-derived, joule-denominated state (ADR-013 CHEMOSTAT-J), not a separate art asset. Core idea: an evidence-based genome→phenotype mapping at each scale, conserved by construction, with every load-bearing number traceable to a cited source. A future LLM-genome layer enters only as frozen, schema-validated, integer-quantized data the deterministic core consumes (process-boundary, never linked, never on the sim path).

## Zoom scopes (max-zoom cell → ecosystem)

**cell** (max zoom) — an individual cell of ONE specimen (NOT today's 32×32 world grid-cell; the current 'cells' zoom is a misnomer).
- Shows: per-cell-type i64 J/biomass sub-account; u16 expression vector (loci ON in this cell type); local toxin/signal; at depth, molecular-product abundances.
- Mapping (Scale 0 MOLECULAR + 1 CELLULAR): Locus(DnaSequence+Params+GO-MF) → product abundance via baked codon/k-mer LUT (bases finally do work beyond PAM-finding); products+GO-CC/part_of → per-cell-type state. Cell types = GO cellular_component nodes.
- Core: CellTypeProfile per specimen, J apportioned from Biomass via fixed::apportion. Render: representative cell glyphs colored by expression. On-demand for inspected specimen only, off the per-tick hash.

**cell-cluster / tissue** — an ordered group of like cell types (epidermis, mesophyll, vascular bundle).
- Shows: aggregate cluster J/biomass, dominant expression program, a tissue-level trait (e.g. water-retention).
- Mapping: deterministic single-link grouping of CellTypeProfile by ontology part_of parent (reuses F6 guild-clustering at a pinned threshold). Cluster J = Σ member-cell J → conservation closes.
- Core: integer sums over ordered Vec keyed by ontology id, no RNG. Render: clustered glyphs as a labeled tissue patch.

**organ** — root/stem/leaf/flower (the level the L-system gestures at but doesn't back with real state).
- Shows: per-organ J/biomass budget; organ trait coefficients (leaf light-capture, root drought-uptake, flower fecundity); the allocation trade-off made spatial.
- Mapping (Scale 2 ORGAN): cell-type mix → organ coefficients (GO-BP / Plant-Ontology terms). Biomass apportioned across organs by the EXISTING Strategy.budget ([u16;5] permille, ADR-013 F2) — the budget simplex becomes the organ split, zero new allocation math (textbook root:shoot:reproductive partitioning).
- Core: split_budget(biomass, strategy.budget), already conserved/tested. Render: REPARENT _plant_params_from_traits to core per-organ trait vectors (morphology evidence-keyed to the budget, not a GDScript heuristic).

**specimen-in-map** — ONE organism at a real world Position, selectable/zoom-into-able (the missing identity link between ecosystem and anatomy).
- Shows: whole-organism trait vector, Energy/Biomass, species, age/lineage net-J, NichePoint, handle to drill into organs/cells.
- Mapping (Scale 3 ORGANISM): organ coefficients roll up into the existing Strategy budget + TrophicRole + affinity CHEMOSTAT-J already consumes — pipeline TOP stage IS the F2 budget, so nothing downstream changes.
- Core: add a per-organism RECORD STREAM to the snapshot (new GSS kind, OrgId-ordered, off RNG/hash path) + a stable handle to request OrganProfile/CellProfile. Render: single L-system plant + inspect panel.

**region** — a sub-area / guild territory / agent operational area (the natural agent-action scope, inv #6: agency is species/region level).
- Shows: per-species density planes; FlowMatrix relations (who-eats-whom, synergy/parasitism/predation — signed-A heatmap); guild membership; aggregate pool depletion.
- Mapping: no new mapping — aggregation of organism strategies over a window + emergent trophic FlowMatrix from TrophicRole/affinity.
- Core: ADR-013 F4/F6 output folded in (i,j) order. Render: windowed data-layer overlay + relations sub-view.

**ecosystem** — the whole 32×32 world (well-developed today).
- Shows: today's 6-7 channels (density/allele_freq/fitness/soil_moisture/nutrients/ph) + conserved-J pool + chem-diffusion channels (F1/F5).
- Mapping (Scale 4 POPULATION): strategies aggregated over the run; allele_freq is the pop-gen read. The scope the SLiM oracle validates against.
- Core: existing GridSnapshot, extended for pools/chem, off RNG/hash path. Render: today's grid view, unchanged.

## LLM-genome layer (process boundary, inv #1/#2)

`crates/oracle-genome`, modeled on `oracle-slim`: std-only, shells to an LLM at the boundary, no GPL, never linked. Two-phase contract. **OFFLINE COMPILE** (non-deterministic, per genome/ontology change): proposes a multi-scale ExpressionTable (locus → products / cell types / organs / Strategy budget). Each proposal clears 3 gates or is REJECTED: (a) TAXONOMY §4 JSON schema; (b) is_a/part_of-subclassed under existing SO/GO/ENVO; (c) resolvable cited SourceId. Admitted rows quantized at the single chokepoint `fixed::to_unit_u16` → versioned, content-hashed, integer-only FROZEN ARTIFACT. **ONLINE CONSUME** (deterministic): core reads only the frozen integer table; content-hash folded into the run hash. Missing/stale → in-core DEFAULT map (inv #5), so the sim never blocks on the LLM. LLM non-determinism is quarantined to compile time — same freeze-then-consume contract the SLiM oracle uses.

## Feature roadmap

- **Tier 0 — Game spine:** Scenario {seed, pinned versions, dims, roster+genomes, env params, edit budget, predicates} as the one hashable artifact (= R4); RNG-free objective/predicate engine over per-gen stats (hash-neutral); `crates/provenance` Sourced<T> (value+SourceId+DerivationKind, zero hash bytes) + sources.ron registry.
- **Tier 1 — CRISPR game loop (mostly exposing core):** guide-design workbench (surface core PAM sites + on/off-target scores pre-commit); edit budget / cost-risk economy; time-series + cross-run telemetry over existing Parquet; sharing/replay/challenge/leaderboard keyed by run-hash.
- **Tier 2 — Evidence mandate (user #1 priority):** check_provenance gate in gate.sh (RED if a hot-path value is Heuristic/Assumption unallowlisted); wrap sim constants in Sourced<T>; pin real SO/GO/ENVO/NCBI-tax snapshots loaded as data; `crates/calibration` gates (Wright-Fisher, Tilman R*, Monod, Lotka-Volterra, vs SLiM); `crates/oracle-crispr` (Bioconductor crisprScore/CFD) with Doench rank-corr golden test; provenance shown in INSPECT.
- **Tier 3 — Multi-scale scopes core data model (🛑 STOP-THE-LINE, ADR + sign-off):** refactor gp.rs flat WeightedSumMap → OntologyTags-keyed Parameter→product→cell→organ→organism→population pipeline behind GenotypePhenotypeMap (open TraitId(GoTermId,Scale) replacing the closed 5-variant enum); conserved-J sub-ledger (organ/tissue/cell via fixed::apportion); on-demand per-specimen OrganProfile/CellProfile (off hash); per-organism stream + GSS3 snapshot + specimen handle; reparent L-system to core organ vectors + cell/cluster glyphs; renderer Scope ENUM {Cell,Cluster,Organ,Specimen,Region,Ecosystem} (built LAST).
- **Tier 4 — LLM-genome layer (Stage 5; 🛑 ADR + sign-off):** frozen multi-scale ExpressionTable artifact (content-hash in seed.json); `crates/oracle-genome` compile-only boundary subprocess (validate/admit/reject, quantize, write); scale-indexed pipeline consumes it with in-core DEFAULT fallback.
- **Cross-cutting — Accessibility:** colorblind-safe overlay palettes (palette = a shader uniform); keyboard-navigable scopes/inspect; tutorial Scenario via the predicate engine; glossary/ontology tooltips.

## Evidence-based strategy (three build gates)

1. **Provenance as data:** Sourced<T>{value, SourceId, DerivationKind, schema version} derefs to value → zero hash bytes; checked-in sources.ron registry.
2. **Provenance-reachability gate:** build FAILS if any hot-path value is Heuristic/Assumption without a DECISIONS.md allowlist entry; every used SourceId must resolve.
3. **Real pinned ontologies:** SO (locus kinds), GO (MF/BP/CC = the three inner scales), ENVO (biome/climate/soil), NCBI-tax (SpeciesId), admitted via TAXONOMY §4 is_a gate, pinned by IRI+date; loci seeded from real Ensembl GFF3 + UniProt.
4. **Calibration-as-a-gate** (`crates/calibration`, off the hashed run): RED until the core reproduces textbook results within pinned tolerance — pop-gen vs Wright-Fisher + SLiM cross-check; ecology vs Tilman R*/competitive-exclusion/Monod/Lotka-Volterra; CRISPR vs Doench Spearman.
5. **CRISPR scoring** grounded in published predictors (Rule Set 2/Azimuth, CFD) behind inv-#5 traits; `oracle-crispr` realistic tier; PAM rows Sourced.
6. **Transcendental math** (diffusion/ODEs/predictor floats/LLM logits) ONLY at the boundary or calibration harness, never on the deterministic path; any boundary number quantized once at `fixed::to_unit_u16`, recorded OracleFrozen. Net: every load-bearing number is cited, calibrated, and bit-reproducible.

## ⚖️ Open questions for human

1. **Sequencing:** push ADR-013 (F0-F7) to completion before scope work, or interleave the cheap evidence/game pillars (Tiers 0-2) in parallel?
2. **Inner-scope cost model:** accept inner anatomy as an on-demand render-driven decomposition of the SELECTED specimen (off the live hash), or fold compartments into the per-tick hash for every organism (far costlier, more re-pins)?
3. **Trait-space breaking change:** retiring the closed 5-variant Trait enum for an open TraitId(GoTermId,Scale) is a 🛑 inv #2/#3/#5 refactor — approve as a dedicated ADR + workflow (not an in-loop slice)?
4. **Ontology acquisition:** is vendoring sizable release-versioned SO/GO/ENVO/NCBI-tax + Ensembl/UniProt data (footprint + licensing) acceptable, and which species/biome grounds the PoC?
5. **LLM provider + determinism contract:** which LLM/CLI at the boundary, and do you accept that a different model/version → a DIFFERENT ledgered run (content-hash folded in)?
6. **Calibration tolerances:** who owns the tolerance bands, and is in-core-vs-SLiM disagreement beyond tolerance a true stop-the-line halt?
7. **Game identity:** Cities-Skylines legible info-sandbox vs goal-driven challenge/campaign? Tiers 0-1 build the spine for both but shape which Tier-1 features get depth.
8. **Agency granularity (inv #6):** confirm inner scopes are inspect-only — no per-organism/per-cell surgery; edits stay species/region-wide.
