# Multi-species prep — JSON species + speciation + relations/vector-DB (DRAFT)

> **DRAFT — `multispecies-relations-prep` workflow (7 agents). DESIGN/PREP ONLY, no code. Builds on the R3/Rel proposals + the richer 9-trait genome.**

## Summary

PREP PLAN — multi-species ecosystems for gene-sim (JSON species starters + speciation-via-edit + relations/vector-DB), sequenced as gated slices on top of the ADR-013 F4 spine and the R3/Rel drafts. DESIGN ONLY.

THREE USER ASKS layer as PREREQUISITE CHAINS, not parallel work:
(1) JSON-defined unique starter specimen per species → needs genome serde DTO + loader (no genome serde today; only LocusId derives it, genome/lib.rs:19) AND, for STRUCTURALLY distinct genomes, the WeightedSumMap ontology re-key (gp.rs:97-116 hardwires 9 traits to flat param indices 0..8). VALUE-only species are safe now; STRUCTURAL species are blocked behind the re-key.
(2) CRISPR edit FORKS a new species (speciation) → needs R3 (SpeciesRegistry + heritable Species tag + S-pool selection + species-routed edits) to land FIRST, then a NEW core op fork_species_by_edit that clones the parent SpeciesEntry, edits the COPY, appends a new SpeciesId, and region-migrates organisms. Its own ADR + a 3rd determinism re-pin.
(3) Relations + vector DB → split into IN-CORE on-hash GLV community matrix (exact quantized cosine; synergy/parasitism/predation DYNAMICS) and a BOUNDARY off-hash sqlite-vec sidecar (crates/relations-index, oracle-slim template) powering the advisory "nearest cousins"/lineage view. The shared artifact is the pure embed() RepVector ([u16;9], D=9 NOT the draft's stale 5). View-only Rel-2..4 can ship BEFORE R3 (1x1 neutral matrix); only Rel-5 coupling needs K>=2 and is a re-pin.

CRITICAL SEQUENCING: genome-serde-DTO (hash-neutral) and Rel-view (hash-neutral) are the safe entry slices that need NO sign-off. R3-A/B is the stop-the-line gate everything else waits on (inv #6). The WeightedSumMap ontology re-key gates structural-distinct species. Speciation and Rel-5 coupling are deliberate re-pins, each its own ADR + 🛑 human sign-off.

CURRENT-CODE ANCHORS verified: single GenomeRes(Genome) lib.rs:85 from sample_genome() at reset lib.rs:395; with_genome_and_rng lib.rs:642 + apply_edit_region lib.rs:675 use a replace/restore SimRng dance (the determinism-safe pattern speciation reuses); species_genome() lib.rs:622 reaches the one GenomeRes; campaign.rs:56-103 is the flat-field-#[serde(default)]+reconstruct precedent the SpeciesSpec DTO mirrors (EnvParams is not serde, exactly like Genome).


## JSON species-starter schema + loader

CONCRETE JSON SPECIES-STARTER SCHEMA + LOADER (data/species/*.json — the future in-game editor's save format; inv #2: JSON is inert DATA, the Rust builder is the only JSON→Genome path).

ROUTE: a serde DTO + validating builder, NOT serde-derives on the genome types. Rationale (mirrors campaign.rs:56 where EnvParams is reconstructed from flat fields because it is not serde): (a) DnaSequence wraps a PRIVATE Vec<u8> with a validating ACGT constructor (genome/lib.rs:43, returns bad-byte index) — a naive derive would bypass validation; (b) ParamValue is an enum needing a STABLE on-disk tagged repr decoupled from the in-memory enum; (c) keeps the data-model crate dependency-light. Place the DTO in a serde-gated genome::spec module (co-located, golden-testable against sample_genome) OR a new crates/species-spec (keeps genome serde-free) — OPEN QUESTION, recommend gated module.

DTO (genome/src/spec.rs behind a `serde` feature):
  SpeciesSpec { format_version:u16, key:String (kebab, == file stem, roster ordering + lineage id), name:String, niche:Niche, genome:GenomeSpec }
  Niche { #[serde(default)] entity_count:u32, description:String, temp_optimum:Option<f64>, parent_key:Option<String> (RESERVED for fork provenance) }
  GenomeSpec { version:u16 (==Genome.version, currently 2), loci:Vec<LocusSpec> }
  LocusSpec { id:u32 (== index, builder ASSERTS), name:String, sequence:String (validated by DnaSequence::new on build), parameters:Vec<ParameterSpec>, tags:OntologyTagsSpec }
  ParameterSpec { id:u32, value:ParamValueSpec }
  ParamValueSpec #[serde(tag="kind", rename_all="snake_case")] = Numeric{value,min,max:f64} | Enum{value,cardinality:u16} | Bool{value:bool}  (internally-tagged {"kind":"numeric",...} for editor readability — PIN this now, it is the on-disk contract)
  OntologyTagsSpec { so_term:u32, #[serde(default)] go_refs:Vec<u32> }

BUILDER (the single validating path; structured path-carrying errors, NO RNG, no HashMap, Vec-ordered → pure fn of file bytes):
  SpeciesSpec::build() -> Result<BuiltSpecies, SpecError>: per locus assert id==index (LocusIdMismatch), DnaSequence::new (BadBase{locus,byte}), per param ParamValue::is_valid() (ParamOutOfDomain), then Genome::is_valid() (GenomeInvalid). Out-of-domain values are STRICT-REJECTED at load (NOT clamped like edits — the file is authoritative input).
  BuiltSpecies { key, name, entity_count, genome:Genome }
  SpeciesSpec::from_genome(&Genome) — inverse for editor SAVE + golden round-trip.

LOADER PLAN (BOUNDARY does file I/O; CORE stays filesystem-free):
  - crates/harness + crates/godot-sim: load_species_roster / load_species_file = read_to_string + serde_json::from_str (mirrors campaign::load_campaign campaign.rs:143; serde error → io::ErrorKind::InvalidData), then build() each spec surfacing SpecError to the player. The boundary loads→builds→hands the core a validated Vec<BuiltSpecies> or a default.
  - reset_with_env (lib.rs:388) replaces `genome::sample_genome()` (lib.rs:395) with species_roster_or_default(): when no manifest is supplied it returns vec![BuiltSpecies::from(sample_genome(),"default")] → byte-identical world → pinned hash UNCHANGED (HASH-NEUTRAL by construction; this is the R3-A guard). Per species express WeightedSumMap → base_growth, build the registry entry.
  - Roster: per-species files data/species/<key>.json + a thin manifest listing member keys in SpeciesId order (better for editor save + forks) vs one inline roster file (OPEN QUESTION).

GOLDEN/GATE TESTS: SpeciesSpec::from_genome(sample_genome()).build()?.genome == sample_genome() (lossless); data/species/default.json loads to sample_genome() exactly; shipped_species_files_load loads+builds every data/species/*.json (mirrors campaign.rs:481 shipped_intro_manifest_loads) so committed species are caught by tools/gate.sh as data-not-code.

WeightedSumMap CAVEAT (expression-correctness, not a determinism break): the format ALLOWS arbitrary layouts but gp.rs anchors traits to flat indices 0..8, so a non-canonical layout mis-EXPRESSES (still deterministically). Until expression is re-keyed off OntologyTags, the builder should WARN (or a --strict validate step ERROR) unless the genome matches the canonical 4-locus/9-param shape. Ship VALUE-only homogeneous-layout species now; structural later.


## Speciation-via-edit

SPECIATION-VIA-EDIT (the user's "an edit forks a lineage into a NEW species") — a NEW deterministic core operator layered on R3, NOT a tweak to R3's replace-in-place edit. Its own ADR + a 3rd determinism re-pin + 🛑 human sign-off.

NEW CORE OP (sim-core/lib.rs, alongside with_genome_and_rng @642 / apply_edit_region @675):
  fork_species_by_edit(parent:SpeciesId, region:Region, edit:EditAction, min_pop:u32) -> Option<ForkOutcome>
  ForkOutcome { child:SpeciesId (==registry.len()-1 after push), parent, migrated:u32 (>0 by construction), edit:EditOutcome, child_base_growth:f64 }

6-STEP BODY (every step reuses existing machinery; species/region-granular per inv #6, NO per-organism handle):
  1. PRE-CHECK (zero RNG, BEFORE any mutation): count parent-species orgs whose Position ∈ region (reuse apply_edit_region's region.contains loop lib.rs:693, filtered Species==parent). If < min_pop → return None → a refused fork is HASH-NEUTRAL (no empty lineage ever created).
  2. CLONE: child_genome = registry[parent].genome.clone(); push target = SpeciesId(registry.len()). Vec-indexed, never HashMap (inv #3).
  3. GATE+EDIT THE COPY: crispr::apply_edit(&mut child_genome, edit, ..., rng) via the SAME replace/restore SimRng dance as with_genome_and_rng (lib.rs:649-657). Gate is UNCHANGED (already takes &mut Genome — clean inv #5 seam). On EditOutcome::Failed → restore RNG, return None (the failed gate's draws stay folded). Parent genome untouched → both lineages now diverge.
  4. EXPRESS+APPEND: bg = WeightedSumMap.express(&child_genome).get(GrowthRate); registry.push(SpeciesEntry{genome:child_genome, base_growth:bg, name:derive_child_name(parent), parent:Some(parent), birth_tick:tick, lineage:registry[parent].lineage}).
  5. MIGRATE (zero RNG): iterate (OrgId, Position, &mut Species) in OrgId order; every org with Species==parent && region.contains → set Species=child. Order-independent, no draw — mirrors apply_edit_region's allele shift. Next step()'s S-pool selection gives the child its own Wright-Fisher pool; offspring inherit Species so lineages NEVER re-merge.
  6. RE-PIN: fold the now-TIME-VARYING species_count (current registry len at hash time, not a reset constant) + the per-species parameter_count SUM into hash_world.

WHY REGION-SCOPED MIGRATION (not a sampled fraction): the region disc draws ZERO selection RNG (operator-chosen, contains is pure), so the ONLY RNG cost is the genome edit (≤2 draws, fixed, independent of org count). A fitness/fraction split would need ordered SimRng draws + fold into the re-pin — strictly more surface for no PoC benefit. "Fork ENTIRE species A" is just region=whole-world. Total population conserved (re-tag, no spawn/free-list).

EXTENDED SpeciesEntry (adds lineage on top of R3's flat {genome,base_growth,name}):
  + parent:Option<SpeciesId> (None for roster/JSON species, Some for forks)
  + birth_tick:u64
  + lineage:LineageId (STABLE root-of-tree id assigned at reset, INHERITED by every fork descendant — this is the fork-stable "GroupId that survives forks" the vector DB needs for nearest-cousins). SpeciesId==registry index (time-varying len); LineageId is separate, stable, ordered.

NEW Action variant (harness, serde-default for journal back-compat per campaign.rs:58):
  ForkSpecies { #[serde(default)] parent:SpeciesId, region:Region, edit:EditAction, #[serde(default=...)] min_pop:u32 }  → old actions.ndjson replays as species 0.

godot-sim binding: LiveSim::fork_species(parent,region,edit) -> i64 (new species id or -1). main.gd's specimen log becomes per-species; a fork edit is the BASELINE of a NEW species PAGE pointing at parent_species_id (a lineage TREE/fork edge, not a row under the parent) — drawn renderer-side from core-exported LineageMeta, no biology in GDScript (inv #2).

HARD DEPENDENCY: speciation re-tags EXISTING orgs (no spawn), so per-species N becomes variable the MOMENT a fork lands → this forces R3-F (variable abundance) to land BEFORE or WITH speciation, not after (OPEN QUESTION whether conserved-total sidesteps R3-F). Value-only forks work with gp.rs unchanged; STRUCTURAL forks (add/remove loci) are blocked behind the WeightedSumMap ontology re-key.


## Relations + vector DB

RELATIONS + VECTOR-DB-AT-THE-BOUNDARY — split into TWO PHYSICALLY SEPARATE computations (the firewall is structural, not a hot-path check):

A. IN-CORE, ON-HASH (sim-core, deterministic, integer/fixed-point only — drives synergy/parasitism/predation DYNAMICS):
  - embed.rs: REL_EMBED_VERSION=1; D_PHENO = gp::Trait::ALL.len() = 9 (NOT the draft's stale 5 — there are now 9 traits, gp.rs:39). q(x)=round(x.clamp(0,1)*65535). embed(map,genome)->RepVector{dims:[u16;9]} walks express() output in Trait::ALL order; pure, ordered, HashMap-free.
  - relations.rs: cos_q(a,b)->i32 EXACT integer cosine (u128 dot/norm, integer isqrt, COEFF_SCALE=10_000) — no float, no transcendentals. RelationMatrix{k, a:Vec<i32> row-major k*k} built each consulted generation in ascending GroupId order (row/col == SpeciesId). Sign pattern names RelKind: +/+ mutualism, -/- competition, +/- parasitism/predation.
  - RelationModifier trait + DefaultRelationModifier: ONE strictly-positive [0.5,1.5] factor, mirroring soil::EnvironmentModifier (soil.rs:174) / climate::ClimateModifier (climate.rs:99). selection() (lib.rs:241) multiplies it into the weight product alongside fitness*soil*climate, looked up by the org's GroupId in stable order. Neutral world stays selection-neutral until a coeff is non-zero (climate-extremity precedent) → re-pin is the WIRING, hash only diverges once a relation is set.

B. AT THE BOUNDARY, OFF-HASH (crates/relations-index sidecar, oracle-slim template — never linked into sim-core/game binary, inv #1):
  - Structural clone of crates/oracle-slim: zero normal/build deps, $RELDB_BIN→pinned path→PATH resolver, plain-data RelRow in / Result<Vec<Neighbour>, RelError> out, NO byte-compare of DB output (determinism judged UPSTREAM by embed(), not by ANN). To keep inv #1 honest with sqlite-vec's loadable-extension model, wrap the extension in a TINY CLI shim binary (like oracle-slim shells out to `slim`) so relations-index LINKS NOTHING — preserving the zero-dep gate.
  - The harness writes per-species RepVectors to a run-namespaced sqlite-vec .db at {reset, epoch, FORK} from its SEPARATE off-RNG GeneSimEnv (exactly how write_snapshots/write_specimens already run on a fresh env, never touching the live SimRng). Row: rel_rows(run_id, generation, species_id, parent_id (-1 for founders), embed_ver, abundance) + vec0(embedding float[9]).
  - Answers KNN ("nearest cousins") + lineage-neighbourhood queries read-only for the VIEW. sqlite-vec = single-file, dual Apache/MIT (no GPL), brute-force KNN fine at inv-#6 cardinality; PIN the version in DECISIONS.md (inv #7).

THE FIREWALL — one legal direction core→DB, never DB→hashed logic: the embed() output [u16;9] is the ONLY artifact shared. The matrix (A) reads it directly; the DB (B) reads the EXPORTED COPY. DB output (neighbours/distances, re-quantized to u32) is NEVER read by selection() or any hashed system. Safety is structural (two separate computations). The sim hash must be byte-identical whether the sidecar is present, absent, or returns different neighbours run-to-run (the acceptance test).

WHAT STAYS OUT OF THE SIM HASH: the entire sidecar DB, all ANN/KNN output, raw similarity floats, and (in the PoC) LineageId/parent provenance unless a relation modifier keys off it. embed() is hash-NEUTRAL while view-only (Rel-2..4, rides the snapshot/specimens export path) and becomes hash-LOAD-BEARING only at Rel-5 coupling — scope this claim explicitly in docs or a reviewer is misled.

SPECIATION × REL: a fork APPENDS a SpeciesId → K grows mid-run → matrix is O(K²) recompute (already required for the child row) and the sidecar re-emits the child RepVector on the FORK event. Lineage neighbourhoods become the killer feature precisely because a fork's child RepVector is near its parent by construction (small edit delta) → KNN surfaces recent forks as nearest cousins. GroupId must be the fork-STABLE LineageId so cousins survive forks.

DECOUPLING: RepVector export serializes embed()'s OUTPUT ([u16;9] + integer ids), NOT the Genome — so Phase Rel does NOT need genome serde and can ship BEFORE the JSON-species loader. View-only Rel-2..4 can land BEFORE R3 (matrix 1x1, selection-neutral); only Rel-5 needs K>=2.

LICENSE GATE: tools/check_license.sh part-B is hard-keyed to the literal "oracle-slim" and does NOT cover relations-index — generalize it to a boundary-crate list (or sibling zero-dep assertion) FIRST, else the inv-#1 boundary for the vector DB is mechanically UNENFORCED.


## Slice sequence

### P0-license-gate — Generalize tools/check_license.sh part-B off the literal 'oracle-slim' to a boundary-crate LIST (oracle-slim, relations-index) so any future vector-DB/GPL boundary crate is enforced. Add a test asserting the new crate is zero-normal-dep. Hash-neutral, no sign-off.

- **Touches:** `tools/check_license.sh`, `docs/llm/DECISIONS.md`
- **Determinism:** hash-neutral
- **Depends on:** none

### P1-species-spec-dto — Add the SpeciesSpec serde DTO + validating builder (build() re-runs DnaSequence::new + ParamValue::is_valid + Genome::is_valid, structured path-carrying errors) in a serde-gated genome::spec module. Golden round-trip: from_genome(sample_genome()).build()==sample_genome(). NO loader wiring yet.

- **Touches:** `crates/genome/src/spec.rs`, `crates/genome/src/lib.rs`, `crates/genome/Cargo.toml`
- **Determinism:** hash-neutral
- **Depends on:** none

### P2-species-loader — Boundary loader load_species_roster/load_species_file (read_to_string+from_str, mirror campaign::load_campaign) in harness+godot-sim, surfacing SpecError/io::Error to the player. Ship data/species/default.json (== sample_genome) + a shipped_species_files_load gate test. CORE stays filesystem-free; reset still uses sample_genome default.

- **Touches:** `crates/harness/src/species.rs`, `crates/godot-sim/src/lib.rs`, `data/species/default.json`
- **Determinism:** hash-neutral
- **Depends on:** P1-species-spec-dto

### Rel-2-embed — Add sim-core embed.rs: REL_EMBED_VERSION, D_PHENO=9, q(), RepVector, pure embed() over express() output. Wire it onto the snapshot/specimens export path (off-RNG). Prove pinned determinism hash UNCHANGED (embed rides the path that never touches SimRng).

- **Touches:** `crates/sim-core/src/embed.rs`, `crates/sim-core/src/lib.rs`
- **Determinism:** hash-neutral
- **Depends on:** none

### Rel-3-sidecar — crates/relations-index (oracle-slim structural clone, zero-dep, $RELDB_BIN resolver, tiny CLI shim wrapping sqlite-vec so nothing is linked). Harness writes per-species RepVector rows at {reset,epoch} from the separate off-RNG env. Pin sqlite-vec version in DECISIONS.md.

- **Touches:** `crates/relations-index/src/lib.rs`, `crates/relations-index/Cargo.toml`, `crates/harness/src/main.rs`, `docs/llm/DECISIONS.md`
- **Determinism:** hash-neutral
- **Depends on:** Rel-2-embed

### Rel-4-view — Relations VIEW: read-only KNN 'nearest cousins'/lineage-neighbourhood query API on relations-index; godot dual-provenance panel (authoritative matrix heatmap + advisory ANN overlay) that degrades gracefully when the sidecar is absent. No biology in GDScript.

- **Touches:** `crates/relations-index/src/lib.rs`, `crates/godot-sim/src/lib.rs`, `godot/relations_view.gd`
- **Determinism:** hash-neutral
- **Depends on:** Rel-3-sidecar

### R3-A-registry — STOP-THE-LINE (inv #6, human sign-off). Introduce SpeciesId newtype + ordered Vec SpeciesRegistry(Vec<SpeciesEntry{genome,base_growth,name,parent,birth_tick,lineage}>) + heritable off-stream Species tag. Default 1-species roster keeps hash byte-identical. reset_with_env builds registry from species_roster_or_default() (consuming P2's BuiltSpecies).

- **Touches:** `crates/sim-core/src/lib.rs`
- **Determinism:** hash-neutral
- **Depends on:** P2-species-loader

### R3-B-selection-snapshot — STOP-THE-LINE + RE-PIN #1 (structural). S independent fixed-size Wright-Fisher pools in ascending SpeciesId order (offspring inherit Species); GSS2->GSS3 per-species snapshot planes + LineageMeta. Fold Species field into org rows + species_count/per-species param_count into hash_world. New pinned literal, ledgered same commit.

- **Touches:** `crates/sim-core/src/lib.rs`, `crates/sim-core/src/snapshot.rs`
- **Determinism:** 🔁 RE-PIN
- **Depends on:** R3-A-registry

### R3-D-edit-routing — Mandatory SpeciesId selector on Action::ApplyEdit/ApplyEditRegion + region species-filter; serde-default keeps actions.ndjson back-compat (old journals → species 0).

- **Touches:** `crates/harness/src/lib.rs`, `crates/sim-core/src/lib.rs`, `crates/godot-sim/src/lib.rs`
- **Determinism:** hash-neutral
- **Depends on:** R3-B-selection-snapshot

### Rel-5-coupling — STOP-THE-LINE + RE-PIN #2. Wire RelationMatrix (cos_q over registry RepVectors) + RelationModifier [0.5,1.5] factor into selection() weight product. Neutral world stays selection-neutral (zero coeffs) so re-pin is the wiring; new literal ledgered. embed() becomes hash-load-bearing here.

- **Touches:** `crates/sim-core/src/relations.rs`, `crates/sim-core/src/lib.rs`
- **Determinism:** 🔁 RE-PIN
- **Depends on:** R3-D-edit-routing

### F2-ontology-rekey — STOP-THE-LINE + RE-PIN. Re-key WeightedSumMap (gp.rs:97-116) off locus id / OntologyTags instead of global flat param index, so STRUCTURALLY distinct JSON genomes express correctly (widen GenotypePhenotypeMap per ecology-substrate F2). Unblocks heterogeneous-layout species + ontology embedding block. Append the ontology block to embed() at the SAME time.

- **Touches:** `crates/sim-core/src/gp.rs`, `crates/sim-core/src/embed.rs`
- **Determinism:** 🔁 RE-PIN
- **Depends on:** R3-B-selection-snapshot

### R3-F-variable-abundance — STOP-THE-LINE + RE-PIN. Per-species variable population (prerequisite for speciation re-tag making N variable). Decouple per-species N from a fixed reset constant; fold variable abundance into hash_world.

- **Touches:** `crates/sim-core/src/lib.rs`, `crates/sim-core/src/snapshot.rs`
- **Determinism:** 🔁 RE-PIN
- **Depends on:** R3-B-selection-snapshot

### SPECIATION-fork — STOP-THE-LINE + RE-PIN #3 (own ADR). fork_species_by_edit(parent,region,edit,min_pop): pre-check (zero-RNG count), clone parent genome, gate+edit the COPY via replace/restore SimRng dance, express+append SpeciesEntry, zero-RNG region migration of Species tag, fold time-varying species_count into hash. Action::ForkSpecies (serde-default). Sidecar re-emits child row on fork. New pinned literal.

- **Touches:** `crates/sim-core/src/lib.rs`, `crates/harness/src/lib.rs`, `crates/godot-sim/src/lib.rs`, `crates/relations-index/src/lib.rs`
- **Determinism:** 🔁 RE-PIN
- **Depends on:** R3-F-variable-abundance

### UI-lineage-tree — Renderer-only (inv #2): per-species specimen log keyed by SpeciesId; a fork edit becomes the BASELINE of a new species PAGE with a parent_species_id edge; draw the lineage TREE from core-exported LineageMeta. GDScript computes no biology.

- **Touches:** `godot/main.gd`, `godot/relations_view.gd`
- **Determinism:** hash-neutral
- **Depends on:** SPECIATION-fork

## Invariant risks
- INV #3 (determinism) — THREE deliberate re-pins are in this plan (R3-B structural species_count/Species-field fold; Rel-5 coupling; SPECIATION time-varying species_count + organism re-tag), plus the F2/R3-F re-pins. Each must be a single ledgered commit updating the pinned hash literal (lib.rs ~795) with human sign-off. Risk: a re-pin landing without the ledger line, or species_count folded as a reset constant instead of the live registry len at hash time.
- INV #3 — fork migration & genome-clone draws MUST come from the single SimRng in stable OrgId order via the existing replace/restore dance (lib.rs:649-657). Region-scoped migration draws ZERO selection RNG (the safe policy); any future fitness/fraction split adds ordered draws that MUST fold into the re-pin. Risk: a HashMap iteration creeping into the registry, matrix, or migration loop (everything must stay Vec-ordered).
- INV #1 (GPL/external at the process boundary) — the vector DB must be subprocess/boundary-only. sqlite-vec's loadable-extension model would LINK an extension into relations-index; the mitigation (tiny CLI shim binary so relations-index links nothing, like oracle-slim shells out to `slim`) MUST be honored. check_license.sh part-B is currently hard-keyed to 'oracle-slim' and does NOT cover relations-index → the boundary is mechanically UNENFORCED until P0 generalizes the gate. Vector-DB output must NEVER re-enter the sim hash.
- INV #2 (biology in core, render read-only) — the JSON species file is DATA; the ONLY JSON→Genome path is the Rust builder which enforces every domain invariant (no clamping at load; strict-reject). The in-game editor WRITES a SpeciesSpec via from_genome at the boundary (godot-sim/harness), NEVER in GDScript. The lineage TREE and relations view are drawn renderer-side from core-exported LineageMeta/matrix — GDScript computes no biology.
- INV #5 (science pluggable behind a trait) — embed()/cos_q/RelationModifier and the WeightedSumMap re-key (F2) are clean trait swaps (GenotypePhenotypeMap, RelationModifier mirror soil/climate). Risk: the F2 re-key touches biology and silently mis-expresses any non-canonical genome until it lands — VALUE-only species are safe, STRUCTURAL species are BLOCKED behind F2; ship homogeneous-layout files first.
- INV #6 (species/region granularity) — speciation is a species/region operator action (fork_species_by_edit takes parent SpeciesId + Region, never an organism handle). Risk: any 'which individual organisms fork' RL-style per-organism selection violates #6; the region disc is the granularity-correct policy.
- INV #7 (pinned versions) — sqlite-vec engine/extension version + the relations-index shim must be pinned in DECISIONS.md alongside SLiM/Godot/Bevy/Rust; ANN is NOT bit-stable across versions, which is acceptable ONLY because the DB is view-only/off-hash.
- STALE-CLAIM risk — the Rel draft pins embedding D=5 (phenotype-only); there are now 9 traits (gp.rs:39) so D=9. And the draft claims a blanket genome-serde gap, but LocusId already derives serde (genome/lib.rs:19) and RepVector export needs NO genome serde at all — Phase Rel is decoupled from the JSON-species serde chain. Building on the stale draft numbers would corrupt the index layout.

## ⚖️ Open questions for human
1. R3 sign-off: R3-A/B is a stop-the-line invariant gate (#6) still unsigned. Approve the ordered Vec SpeciesRegistry + off-stream heritable Species tag + S-pool selection + GSS3 planes as the multi-species spine before any speciation/relations-coupling work? (R3-B is re-pin #1.)
2. Speciation is its OWN ADR + the 3rd determinism re-pin. Approve fork_species_by_edit (clone parent genome, edit the COPY, append SpeciesId, region-migrate organisms, conserved total) as the speciation primitive — and confirm region-scoped migration (zero-RNG) over a fitness/fraction split (which adds ordered draws + more re-pin surface)?
3. Sequencing: does speciation REQUIRE R3-F (variable abundance) to land first/with it (a fork re-tags existing orgs so per-species N becomes variable immediately), or does the conserved-total region migration sidestep R3-F? This decides whether R3-F is a hard dependency of SPECIATION-fork.
4. Relations coupling vs display-only for the PoC: ship Rel-0..4 (view-only, zero determinism risk, full relations view + lineage neighbourhoods) and DEFER Rel-5 coupling (the re-pin where relations affect evolution)? Or commit to coupling now?
5. Confirm Rel view-only (Rel-2..4) may land BEFORE R3 sign-off (1x1 neutral matrix, embed needs no genome serde) — contradicting the Rel draft's 'hard dependency on R3' framing for the view half.
6. WeightedSumMap ontology re-key (F2) is a hard prerequisite for STRUCTURALLY distinct JSON species (and for structural forks). Sequence it before shipping heterogeneous-layout species, or ship VALUE-only species now and gate the loader with a --strict canonical-layout check until F2 lands?
7. JSON format pins to lock NOW (on-disk editor contract, costly to change later): (a) ParamValueSpec internally-tagged {"kind":"numeric",...} vs externally-tagged; (b) DTO+builder in a serde-gated genome::spec module vs a new crates/species-spec; (c) per-species files + thin roster manifest vs one inline roster file; (d) keep explicit min/max per Numeric vs default-to-[0,1]; (e) include the reserved niche.parent_key lineage field in v1?
8. LineageId semantics on fork: child SHARES parent's LineageId (one clade, so 'nearest cousins' spans the whole family) vs BRANCHES a new LineageId per fork? Recommend shared-clade + a per-species fork depth/path for tree drawing — confirm.
9. Speciation policy edges: min_pop default to permit a fork; is a fork that orphans the PARENT (migrates 100%, leaving species A extinct) allowed or must ≥1 remain (interacts with ADR-005 no-extinction)? And on extinction, is a SpeciesId/registry slot tombstoned (recommended, monotonic append) or reused (breaks positional indexing)? Also: cap max_species to bound GSS4 snapshot size + S-pool cost?
10. sqlite-vec ruling: confirm sqlite-vec (single-file, dual Apache/MIT, brute-force KNN) over LanceDB, AND confirm the CLI-shim mitigation satisfies inv #1 (the loadable-extension is wrapped in a subprocess so relations-index links nothing) — this is the sharpest inv-#1 nuance and needs an explicit ruling + a pinned version in DECISIONS.md.
