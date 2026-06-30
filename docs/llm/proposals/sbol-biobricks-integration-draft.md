# SBOL + BioBricks deep integration — PINNED spec + ADR-draft

> **DESIGN ONLY — sign-off required.** This is a pinned, buildable spec, NOT an implementation. It touches the
> genome model, inv #2, the determinism hash (inv #3), and proposes a **new invariant (inv #8)** → it CANNOT be
> implemented without the user's explicit sign-off (CLAUDE.md per-slice loop step 2: *>~1 day OR touches an
> invariant → STOP, ask the human*). No production code or Cargo changes accompany this doc.
>
> User brief (2026-06-30): *"hluboká integrace s SBOL (Synthetic Biology Open Language) — nesmí proběhnout proces,
> který není v tomto jazyce definovaný; promysli i BioBricks Foundation přístup."* → **deep** SBOL integration with
> a **closed-world** constraint: **no genetic process may execute that is not defined as an SBOL construct.** Plus
> the BioBricks Foundation approach (standard, characterized, composable parts + a real registry + an assembly grammar).

---

## 0. What is pinned (TL;DR)

| Decision | PINNED choice | Why / citation |
|---|---|---|
| Standard version | **SBOL3 (v3.1.0)** canonical; SBOL2 import-only at the subprocess boundary | SBOL3 collapses structure+function into one recursive `Component`, maps to RDF→JSON-LD via plain serde ([Frontiers fbioe.2020.01009](https://www.frontiersin.org/journals/bioengineering-and-biotechnology/articles/10.3389/fbioe.2020.01009/full); [data model v3.1.0](https://sbolstandard.org/datamodel-specification/version-3.1.0/)) |
| Serialization | **SBOL3 JSON-LD subset via `serde_json`** (ordered, no RDF engine, no network) | JSON-LD = plain JSON + `@context`; a fixed-subset writer round-trips deterministically with serde alone (research brief §2/§6) |
| In-core SBOL | **Hand-rolled focused subset in a new `crates/sbol` (`std`+`serde` only)** — NOT any third-party Rust SBOL crate as a core dep | an RDF-engine-backed SBOL crate (Oxigraph + a network client) is a determinism + footprint + bus-factor risk on the hot path. *(A Rust `sbol`/`sbol-rs` crate may exist — **UNVERIFIED**, crates.io lookup unavailable; the avoid-in-core decision holds regardless of its exact state.)* |
| Full-fidelity validation | **`sbol-cli` / pySBOL3 as a separate CLI subprocess** (the `oracle-slim` boundary pattern) | keeps Oxigraph/`ureq`/pre-1.0 churn off the deterministic hot path + out of the game binary |
| Assembly grammar | **SBOL3 `Constraint`-typed composition + a BioBrick RFC10 part-class grammar** (promoter→RBS→CDS→terminator ordering); Type-IIS/MoClo deferred | RFC10 is the canonical, simplest composable-parts standard; pinning one grammar is inv #7 |
| Registry grounding | **iGEM Registry `BBa_*` ids as inert string provenance tags + SO/SBO/GO/ChEBI IRIs as plain constants** (reference, not redistribute) | referencing term/part IRIs is not redistribution → no attribution/ShareAlike load (research §6) |
| Closed-world gate | **Deterministic in-core SBOL validator in front of genotype→phenotype**; ungrounded Locus/edit/Interaction is REJECTED before it can mutate sim state | the user's load-bearing ask (§3) |
| New invariant | **Recommend ADR-pin inv #8 now, elevate to the numbered invariant list at SB2 sign-off** (§4) | elevation is the user's call; ADR-pin first keeps it reversible until SB2 proves it holds |
| ADR number | **ADR-037** (see §8 for the reservation rationale) | ADR-034 is the committed max; 035 + 036 are reserved on pending branches |

---

## 1. Why this is a formalization, not a rewrite — the model is already SBOL-shaped

`crates/genome` already encodes the SBOL substrate (verified against the live source):

- `Locus { id: LocusId, name: String, sequence: DnaSequence (validated ACGT), parameters: Vec<Parameter>, tags: OntologyTags }`
  (`crates/genome/src/lib.rs:154`).
- `OntologyTags { so_term: SoTermId, go_refs: Vec<GoTermId> }` (`lib.rs:146`) — **a locus's "kind" IS a Sequence
  Ontology term**; the module doc pins *"Loci are data, not code — a locus's kind is an ontology tag (`SoTermId`),
  never a Rust enum."* The baked `data/species/ecoli.json` tags every CDS `so_term: 704` = **SO:0000704 "gene"**,
  with real NCBI CDS bytes (`GCF_000005845.2`).
- `crates/crispr` operates directly on `DnaSequence` (PAM finding via `bio`, gated `Edit`s) — edits are already
  sequence-level transforms (`crates/crispr/src/lib.rs:512` `Edit`, `:529` `EditKind`).
- Trophic behaviour is already a typed-interaction layer: `gp::TrophicRole`
  (`Autotroph|Heterotroph|Mixotroph|Decomposer|Predator|ObligateSymbiont`, `crates/sim-core/src/gp.rs:335`) +
  the conserved `trophic::FlowMatrix` (`crates/sim-core/src/trophic.rs:45`, `Σ_j A[i][j] == 0`).

**SBOL is built on exactly these ontologies.** An SBOL3 `Component.role` is an SO term; `Component.type` and
`Interaction`/`Participation` roles are SBO; cellular components are GO; a `Component` carries `Sequence`s; molecules
are ChEBI via `ExternallyDefined` ([data model v3.1.0](https://sbolstandard.org/datamodel-specification/version-3.1.0/)).
So `Locus(SO) + DnaSequence + go_refs + the FlowMatrix interactions` map **almost 1:1** onto SBOL3. Deep SBOL is a
**formalization + a closed-world gate**, not new biology.

---

## 2. PINNED CHOICES

### 2.1 SBOL3 (v3.1.0), not SBOL2

Pinned: **SBOL3 is canonical.** SBOL3 unifies SBOL2's split `ComponentDefinition` + `ModuleDefinition` into one
recursive `Component` carrying both structure and function, and defines a mapping to an **RDF graph** that
serializes through standard tooling to Turtle / RDF-XML / N-Triples / **JSON-LD**
([Frontiers fbioe.2020.01009](https://www.frontiersin.org/journals/bioengineering-and-biotechnology/articles/10.3389/fbioe.2020.01009/full);
[SBOL3.1.0 PDF](https://sbolstandard.org/docs/SBOL3.1.0.pdf); [PMC10063177](https://pmc.ncbi.nlm.nih.gov/articles/PMC10063177/)).
JSON-LD (plain JSON + an `@context` mapping short keys → IRIs) is the serde-friendly path. SBOL2 (bespoke
constrained RDF-XML, mandatory compliant URIs) is **import-only** through the subprocess boundary (§2.2/§5), using
the spec's documented SBOL2↔SBOL3 mapping appendix.

### 2.2 In-core Rust SBOL subset — the EXACT structs, and the in-core vs subprocess split

Pinned: a **new `crates/sbol` crate, `std`+`serde` only**, hand-rolling the §5-minimal SBOL3 subset. We do **not**
take a core dependency on any third-party Rust SBOL crate. **(UNVERIFIED: a Rust `sbol`/`sbol-rs` crate may exist —
the crates.io/repo lookup was unavailable during research, so its version/maturity/dep-tree could NOT be
independently confirmed; do not treat the earlier specific figures as fact.) The decision does not rest on that
crate's exact state:** an RDF-engine-backed SBOL crate (Oxigraph + a network client like `ureq`) is a determinism +
footprint + bus-factor risk on the deterministic hot path regardless, so it is **AVOID-in-core / subprocess-only**
either way. Full-fidelity SBOL (arbitrary RDF, Turtle/RDF-XML, the full conformance validator) lives **out-of-process**
in a `sbol-cli`/pySBOL3 subprocess, behind the `oracle-slim` boundary pattern — verify the exact tool + license at
SB4 implementation time.

The exact in-core structs (ordered fields, `Vec` not `HashMap`, integer-newtype ids → inv #3):

```rust
// crates/sbol/src/lib.rs  —  std + serde ONLY (inv #5 posture). All ids ORDERED; no HashMap iteration (inv #3).

/// An IRI of SBOL's required form `<namespace>/<displayId>`. Stored as an interned ordered index, NOT a hashed
/// string, so document order is stable (inv #3). The string table is a `Vec<String>` resolved by `IriId(u32)`.
pub struct IriId(pub u32);

/// SBOL3 Sequence — primary structure. `encoding` is an IRI constant (IUPAC-DNA).
pub struct Sequence { pub identity: IriId, pub elements: genome::DnaSequence, pub encoding: IriId }

/// SBOL3 Location/Range into a Sequence (1-based inclusive per SBOL3).
pub struct Range { pub sequence: IriId, pub start: u32, pub end: u32 }

/// SBOL3 Feature subtypes used by the subset. A locus is a SubComponent or a SequenceFeature.
pub enum Feature {
    SubComponent   { identity: IriId, instance_of: IriId, location: Option<Range> },
    SequenceFeature{ identity: IriId, role: SoRole, location: Range },
    ExternallyDefined { identity: IriId, definition: IriId /* ChEBI/UniProt IRI */ },
}

/// SBOL3 Constraint — sequential/topological composition relations (precedes / contains / meets).
pub struct Constraint { pub identity: IriId, pub restriction: ConstraintKind, pub subject: IriId, pub object: IriId }
pub enum ConstraintKind { Precedes, Contains, Meets }   // closed set we honor; others REJECTED on import

/// SBOL3 Participation — an SBO role over a Feature.
pub struct Participation { pub roles: Vec<SboRole>, pub participant: IriId }

/// SBOL3 Interaction — SBO-typed; the trophic/regulatory/metabolic edge.
pub struct Interaction { pub identity: IriId, pub types: Vec<SboRole>, pub participations: Vec<Participation> }

/// SBOL3 Component — the central recursive design entity. `type` = SBO/SO; `role` = SO.
pub struct Component {
    pub identity: IriId,
    pub types: Vec<IriId>,          // SBO/SO type IRIs (e.g. DNA region)
    pub roles: Vec<SoRole>,         // SO role IRIs
    pub has_sequence: Vec<IriId>,   // -> Sequence identities
    pub features: Vec<Feature>,
    pub constraints: Vec<Constraint>,
    pub interactions: Vec<Interaction>,
}

/// A whole SBOL3 document: the TopLevels in stable order + the IRI string table.
pub struct SbolDocument { pub namespace: IriId, pub components: Vec<Component>,
                          pub sequences: Vec<Sequence>, pub strings: Vec<String> }

/// Typed ontology-role newtypes wrapping the EXISTING genome ids — keeps "role is data" (inv #2):
pub struct SoRole(pub genome::SoTermId);   // SO term usable as Component.role / SequenceFeature.role
pub struct SboRole(pub u32);               // SBO term for Interaction.type / Participation.role
```

**In-core (linked) responsibilities:** build an `SbolDocument` from a `Genome`/`SpeciesSpec`; the deterministic
well-formedness + role + assembly-grammar validator (§3); JSON-LD write/read of THIS subset. **Subprocess (boundary)
responsibilities:** full 149-rule conformance, SBOL2 import, Turtle/RDF-XML, SynBioHub round-trip — invoked as a CLI
child, output parsed as JSON-LD, never linked. The trait seam mirrors inv #5's score/oracle pattern:

```rust
pub trait SbolValidator { fn validate(&self, doc: &SbolDocument) -> Result<(), Vec<SbolViolation>>; }
// InCoreValidator (default, deterministic, std)  ·  SubprocessValidator (optional, sbol-cli/pySBOL3, boundary)
```

### 2.3 BioBrick assembly standard / grammar

Pinned: **BioBrick RFC10** (classic prefix/suffix idempotent assembly) as the part-class grammar; **Type-IIS / MoClo /
3A deferred** to a later ADR. The grammar is the closed-world at the *composition* level: a species design is a
sequence of standard parts whose SO roles must form a legal transcription unit. The minimal pinned production:

```
design        := part+
part          := promoter | rbs | cds | terminator | spacer
transcription_unit := promoter rbs cds+ terminator           # the legal ordering RFC10 parts compose into
```

A composition is valid iff (a) every part is a `SequenceFeature`/`SubComponent` with a recognized SO role, (b) the
ordered roles parse against the grammar (`Constraint.precedes` chain matches the production), and (c) RFC10
prefix/suffix compatibility holds. The CRISPR brush becomes **"insert / replace a standard part"** (a grammar-checked
transform), not "poke a base" — matching the synbio-sandbox direction ([[gameplay-sandbox-first]]). SO roles pinned:
promoter `SO:0000167`, RBS `SO:0000139`, CDS `SO:0000316`, terminator `SO:0000141`, gene `SO:0000704`
([data model v3.1.0](https://sbolstandard.org/datamodel-specification/version-3.1.0/)).

### 2.4 Registry-grounding approach

Pinned: **reference, do not redistribute.** Each catalog part carries an inert `bba_id: Option<String>` provenance
tag (iGEM Registry `BBa_*` id) + SO/GO/ChEBI **IRI string constants**. We do NOT bundle the iGEM Registry,
SynBioHub, or the SO/SBO/GO/ChEBI ontology files; we reference term/part IRIs (not redistribution → no
attribution/ShareAlike obligation, research §6). The already-baked species (`data/species/*.json`) are mapped to
registry/SO/GO entries **where a real mapping exists**, recorded as data in the part catalog; unmapped loci keep
their existing `so_term: 704` "gene" tag (already valid as a `Component.role`). The catalog is a new
`data/biobricks/*.json` (parts as data, like `cas_variants.ron`).

### 2.5 Pinned ontology / term sources + tool versions (inv #7)

| Artifact | Pinned identifier | License | Role |
|---|---|---|---|
| SBOL data model | **SBOL3 v3.1.0** | spec (open) | canonical in-core model |
| Sequence Ontology | **SO** term IRIs (referenced) | CC-BY (referenced, not redistributed) | `Component.role` |
| Systems Biology Ontology | **SBO** term IRIs (referenced) | open (referenced) | `Interaction`/`Participation` |
| Gene Ontology | **GO** term IRIs (referenced) | CC-BY (referenced) | cellular component / function |
| ChEBI | **ChEBI** IRIs (referenced) | CC-BY (referenced) | resource molecules via `ExternallyDefined` |
| Assembly standard | **BioBrick RFC10** | open spec | composition grammar |
| Registry | **iGEM Registry `BBa_*`** ids (referenced as provenance) | iGEM terms — verify before any export feature | part provenance |
| Subprocess validator | **pySBOL3 / libSBOLj3** (the verified path) and/or a Rust `sbol-cli` *(if it exists — unverified)* | permissive (confirm exact per-tool license at SB4 — all subprocess-only, so inv #1 holds regardless) | boundary conformance |
| In-core crate | **new `crates/sbol`** (this repo, `std`+`serde`) | repo license | the §2.2 subset |

---

## 3. The CLOSED-WORLD validation gate (the load-bearing ask)

**Principle:** every genetic component, edit, and interaction in the sim is grounded in an SBOL construct, and the
sim cannot execute a genetic process that has no valid SBOL grounding. The gate is a **deterministic pure function
of the design** (no RNG, no clock, ordered traversal → inv #3) placed **in front of the genotype→phenotype path**.

### 3.1 Grounding — how each real-model object becomes SBOL

| sim-core object | SBOL3 grounding | Validity condition |
|---|---|---|
| `Locus` (`genome/src/lib.rs:154`) | a `SubComponent`/`SequenceFeature` with `role = SoRole(locus.tags.so_term)`, a `Sequence` from `locus.sequence`, and a `Range` covering it | `so_term` is a real SO term usable as `Component.role`; `DnaSequence` is a valid IUPAC-DNA `Sequence` (already enforced by `DnaSequence::new`) |
| `Genome` (`lib.rs:165`) | one TopLevel `Component` (`type` = a DNA SO/SBO region term) whose `features` are the loci, ordered by `LocusId`, with `Constraint.precedes` chaining them | the ordered roles parse against the RFC10/transcription-unit grammar (§2.3) |
| `Edit` / `EditKind` (`crispr/src/lib.rs:512`/`:529`) | a transform of the SBOL design that must yield a **valid `SbolDocument`** (the edited `Component` still validates: roles, ranges, constraints, grammar) | post-edit document validates AND the `EditKind` verb is a grounded transform (Perturb/Knockout/Knockdown/Activate map to SBO regulatory interactions, not ad-hoc trait pokes) |
| trophic/regulatory effect (`FlowMatrix`, `TrophicRole`) | an `Interaction` (`type` = SBO) with SBO-typed `Participation`s over the species/resource `Component`s | the interaction is expressible with SBO roles (reactant `SBO:0000010`, product `SBO:0000011`, inhibitor `SBO:0000020`, …) |
| resource molecule | a `Component` (`type` = ChEBI IRI via `ExternallyDefined`) | the ChEBI IRI resolves to a referenced constant |

> **Honest semantic caveat (research §5, carried forward):** SBO has **no clean predation/trophic term**. "Species A
> consumes resource B" is *encoded* (and round-trips) as an `Interaction` with reactant/product `Participation`s over
> `Component`s, but it is **off-label** SBOL — an interoperability/semantics risk, not a hard blocker. The gate
> validates *well-formedness*, not biological correctness of the off-label mapping. This caveat is itself pinned so we
> never claim the trophic encoding is canonical SBOL.

### 3.2 Exactly when a process is REJECTED (deterministic)

The gate runs `InCoreValidator::validate(&doc)` and **rejects (returns `Err`, the process never reaches
genotype→phenotype)** iff ANY of:

1. **Unknown role** — a `Locus`/feature `so_term` is not in the pinned SO-role allow-set (not usable as
   `Component.role`).
2. **Malformed sequence/range** — a `Range` falls outside its `Sequence`, or a `Sequence` is not valid IUPAC-DNA
   (`DnaSequence::new` already guarantees the latter for in-repo data; the gate re-checks for imported designs).
3. **Grammar violation** — the ordered part roles do not parse against the RFC10 transcription-unit production
   (§2.3), or RFC10 prefix/suffix incompatibility.
4. **Ungrounded interaction** — a trophic/regulatory effect has no `Interaction`+`Participation` with valid SBO
   roles (an "effect with no SBOL interaction" is the canonical rejected case).
5. **Edit yields an invalid document** — applying the `Edit`/`EditKind` produces an `SbolDocument` that fails
   1–4 (the edited `Component` no longer validates).

On rejection the gate returns a typed `Vec<SbolViolation>` (mirrors `crispr::EditFailure`'s explicit-failure
discipline — a rejected process is never a silent success). **Placement:** the gate is called at exactly the two
points where a *new* genetic design can enter the sim — (a) `SpeciesSpec::build` / roster load, and (b)
`apply_edit` / region-edit — both *before* the `Genotype`/parameter mutation is committed. This is the synbio
analogue of "no sim bytes exist outside the seeded RNG": **no genetic process exists outside a valid SBOL document.**

---

## 4. Candidate inv #8 — the genetic vocabulary is closed over SBOL

**Exact proposed wording (for SPEC §2.1, sibling of inv #2):**

> **8. The genetic vocabulary is closed over SBOL.** Every genetic component, edit, and interaction the sim
> executes is grounded in a valid SBOL3 construct, and is validated by the deterministic in-core SBOL gate
> **before** it reaches the genotype→phenotype path. A `Locus`, `Edit`, or `Interaction` that does not ground to a
> well-formed SBOL document (valid SO/SBO roles, ranges, constraints, and the pinned assembly grammar) is
> **REJECTED** and cannot enter the sim. The genome model (`crates/genome`/`crates/sbol`) owns this vocabulary;
> `godot/` never defines or validates it. Closed-world is the synbio analogue of inv #3's determinism: just as no
> sim bytes exist outside the seeded RNG, no genetic process exists outside a valid SBOL construct.

**Recommendation: ADR-pin it now (ADR-037), elevate to the numbered invariant list at SB2 sign-off — do NOT add it
to the 7-invariant list in this design pass.** Rationale: (a) elevation to a numbered "STOP THE LINE" invariant is a
permanent, costly commitment the user must make explicitly (CLAUDE.md: invariants OVERRIDE default behavior);
(b) ADR-pinning makes it binding-but-reversible while SB1/SB2 prove the gate actually holds for every existing
baked species without forcing rejections of currently-valid designs; (c) the off-label trophic caveat (§3.1) means
the *interaction* half of closed-world is not yet on fully canonical ground — elevating before SB2 would harden a
rule we might need to soften. Concretely: **ADR-037 pins the rule as binding design law now; the SB2 sign-off ticket
asks the user to elevate it to inv #8 in SPEC §2.1 + CLAUDE.md.** Until elevated, a violation is an ADR-owned design
defect (fix or revert), not yet a numbered STOP-THE-LINE.

---

## 5. inv #1 LICENSING verdict — every SBOL tool / dataset touched

inv #1 is **about GPL at the process boundary**; the research brief confirms **no GPL/AGPL anywhere in the SBOL
stack** (unlike SLiM). The verdict below also protects determinism + footprint (the secondary reason to keep the
heavy/networked tools at the boundary even though they are permissive).

| Tool / dataset | License | Verdict | Rationale |
|---|---|---|---|
| **new `crates/sbol`** (this repo's subset) | repo license | **LINK (in-core)** | hand-rolled `std`+`serde`; no third-party copyleft |
| **`serde` / `serde_json`** | MIT/Apache-2.0 | **LINK** | already in-tree; the JSON-LD path |
| **pySBOL3** | **MIT** | **SUBPROCESS-ONLY** | permissive, but Python + heavy → boundary tool for full conformance / import |
| **libSBOLj3** | **Apache-2.0** | **SUBPROCESS-ONLY** | JVM reference impl → boundary only |
| **SBOL-utilities** (GenBank↔SBOL3/FASTA) | **MIT** | **SUBPROCESS-ONLY** | CLI conversion at the boundary |
| a third-party Rust `sbol`/`sbol-rs` crate (if it exists) + any `sbol-cli` | **license UNVERIFIED** (crates.io lookup unavailable — confirm at SB4) | **SUBPROCESS-ONLY; do NOT link as a core dep** | an RDF-engine-backed SBOL crate would pull Oxigraph + a network client → determinism + footprint + bus-factor risk; quarantined to the boundary regardless of its exact license/version (SB4 uses pySBOL3/libSBOLj3 as the verified fallback) |
| **Oxigraph stack** (`oxrdf`/`oxrdfio`/`oxjsonld`) | MIT OR Apache-2.0 | **AVOID in-core** (rides only inside `sbol-cli` at the boundary) | heavy RDF engine; not needed for the fixed JSON-LD subset |
| **SO / SBO / GO / ChEBI** ontologies | CC-BY / open | **REFERENCE-ONLY (link IRIs, do not bundle)** | referencing term IRIs is not redistribution → no attribution/ShareAlike load (research §6) |
| **iGEM Registry `BBa_*`** | iGEM terms (verify) | **REFERENCE-ONLY as provenance; VERIFY terms before any registry-data *export/redistribution* feature** | ids-as-provenance is fine; bundling registry *content* needs a license check (sits with [[no-monetization-noncommercial-data]] — non-commercial data is acceptable but must be recorded) |
| **SynBioHub** designs (SB5) | per-design varies | **SUBPROCESS-ONLY + per-design license check** | external round-trip is a boundary feature; imported design licenses are out of our control |

**Net:** inv #1 is **not threatened** by SBOL. One linked crate (`crates/sbol`, `std`+serde). Everything heavy,
networked, or churning stays subprocess-only. `scripts/check_license.sh` (wired into `tools/gate.sh` step 8) covers any new *linked* crate; the
`crates/oracle-slim` no-GPL-dep discipline is the template for the `sbol-cli` boundary.

---

## 6. DETERMINISM RE-PIN PLAN for SB2 (🔁 STOP-THE-LINE)

### 6.1 The genome's exact hash footprint (verified in `crates/sim-core/src/lib.rs:3385` `hash_world`)

`hash_world` folds in, from the genome, **only**:

- `genome_params = world.resource::<GenomeRes>().0.parameter_count() as u64` — a single u64 (the *total* parameter
  count across all loci), and
- per organism, the heritable f64s derived from the genome — `Genotype.0.to_bits()` **plus `DroughtTol` +
  `ThermalTol`** (`lib.rs:3408-3410`). (SBOL re-grounding touches **none** of `g`/`d`/`t`, so the hash-neutrality
  conclusion is unchanged — the SBOL view is an additive projection that preserves `parameter_count` + the
  `Genotype`/tolerance derivations.)

It does **NOT** hash: sequence bytes, `so_term`, `go_refs`, locus names, locus ids, or even the locus *count*. The
per-org sort key is `OrgId` only. **This is the decisive fact:** re-grounding `Locus`/`Genome` as SBOL Components —
adding IRIs, `Feature`/`Constraint`/`Interaction` wrappers, SO/SBO roles — is **hash-neutral by construction**,
because none of those fields feed `hash_world`. The pinned literal `0x47a0_3c8f_6701_f240`
(`lib.rs:3544`, `:3708`) stays byte-identical **as long as two quantities are preserved**:

1. `Genome::parameter_count()` is unchanged for every baked species, AND
2. the `Genome → Genotype` derivation (the f64 each org carries) is byte-identical.

### 6.2 Preserve-meaning migration (the safe path)

SB2 re-grounds the model as **an additive, parameter-count-preserving wrapper**:

- The SBOL view is **built from** the existing `Genome` (a *projection*), not a replacement that re-counts
  parameters. `Genome`/`Locus`/`Parameter` stay the canonical mutable state; `SbolDocument` is derived.
- **No parameter is added, removed, split, or reordered** during SB2. BioBrick `Measure`/datasheet fields (§3) that
  could add parameters are introduced **only** in SB3, and each is a separately-ledgered decision — SB2 itself
  touches zero parameters.
- The closed-world gate is a **pure pre-check** that returns `Ok`/`Err`; on the pinned single-species-plant config
  every baked design already validates (SO:704 genes, valid ACGT, no ungrounded interactions) → the gate **rejects
  nothing** → the genotype→phenotype path runs byte-identically.

**Expected result: SB2 is hash-neutral.** The re-pin machinery below is the *safety net* for the case where SB2
(or, more likely, SB3's datasheet parameters) is found to change `parameter_count()` or the `Genotype` derivation.

### 6.3 If the hash moves — the ADR-owned 🔁 re-pin procedure (precedent: ADR-013 F-series, ADR-021)

A hash move during SB2 is **only acceptable if it is the deliberate, meaning-preserving consequence of an explicit
decision** — never an accident. Procedure (mirrors the ledgered re-pins in `docs/llm/DECISIONS.md:422`/`:476`):

1. **STOP THE LINE** — surface to the human; do not work around a moved literal.
2. Prove the move is *intended*: a one-line statement of exactly which input changed (`parameter_count` delta or
   `Genotype` derivation change) and why meaning is preserved.
3. Run `cargo test -p sim-core determinism_hash_is_pinned -- --nocapture`, record OLD→NEW in an ADR re-pin entry
   (`🔁 RE-PIN #n: OLD → NEW`), update the literal in `lib.rs` (both `:3544` and `:3708`) + any mirrored value.
4. **Multi-ISA gate is authoritative:** `tools/check_determinism_multi_isa.sh` SKIPs locally (one arch reachable);
   the CI matrix (`determinism-multi-isa` + `assert-isa-match`) is the real cross-arch byte-equality assertion —
   the re-pin is not accepted until that matrix is green (matches [[repin-execute-not-stage]]: execute deliberate,
   already-designed re-pins; the multi-ISA CI is the safety net).
5. One re-pin = one commit, ADR-owned, with the OLD/NEW hashes ledgered.

**Pinned stance:** SB2 is engineered to be hash-neutral (§6.2). Any hash change is an **ADR-037-owned, multi-ISA-
validated 🔁 re-pin**, not an accident — and if it appears *unexpectedly* (no design decision called for it), it is
a STOP-THE-LINE defect to be reverted, not re-pinned.

---

## 7. Slice plan SB1..SB6

`SB-D` (this design pass) is done on sign-off. Hash-neutrality verdicts are derived from §6.1.

| Slice | Scope | Deps | Hash verdict | Sign-off |
|---|---|---|---|---|
| **SB1 — SBOL subset + in-core validator** | new `crates/sbol` (§2.2 structs), the deterministic well-formedness/role/grammar validator (§3.2), the `SbolValidator` trait seam + `InCoreValidator`, JSON-LD write/read of the subset. Unwired (no sim-core call yet). | SB-D | **✅ hash-neutral** — new crate, unused by the sim path; `0x47a0` untouched | normal slice |
| **SB2 — Genome⇄SBOL grounding + the closed-world GATE** | build `SbolDocument` from `Genome`/`SpeciesSpec`; call the gate before genotype→phenotype at `SpeciesSpec::build` + `apply_edit`; the §6 re-pin plan. | SB1 | **🔁🛑 expected hash-neutral, re-pin-net armed** — the ONE invariant-touching slice; STOP-THE-LINE if `0x47a0` moves | **REQUIRED — and the inv #8 elevation ticket (§4)** |
| **SB3 — BioBrick parts catalog + assembly grammar** | `data/biobricks/*.json` registry-grounded parts (§2.4), the RFC10 transcription-unit grammar wired into the gate, CRISPR-brush-as-part-insert. *Datasheet `Measure` params are introduced here — each that changes `parameter_count` is a ledgered 🔁.* | SB2 | **mostly data; ⚠️ conditional 🔁** if datasheet params change `parameter_count` (each ledgered per §6.3) | per-param if hash moves |
| **SB4 — subprocess reference validator** | optional `sbol-cli`/pySBOL3 conformance at the boundary (`SubprocessValidator`), the `oracle-slim` pattern; inv #1/#5. | SB1 | **✅ hash-neutral** — boundary tool, off the sim path | normal slice |
| **SB5 — SBOL import/export** | round-trip designs to/from SBOL3 documents / SynBioHub (subprocess); per-design license check. | SB4 | **✅ hash-neutral** — IO only | normal slice (license check) |
| **SB6 — synbio sandbox UI** | renderer-only: compose a species from standard parts, the grammar guides, read the SBOL design in the codex/specimen view. No genome logic in GDScript (inv #2). | SB3 | **✅ hash-neutral** — renderer-only (zero Rust sim-path change) | normal slice |

Critical-path order: **SB1 → SB2(🔁🛑) → SB3 → SB6**, with **SB4 → SB5** as a parallel boundary branch off SB1.
Only **SB2** is a STOP-THE-LINE / re-pin-armed slice; everything else is hash-neutral by construction.

---

## 8. ADR-DRAFT (reserve **ADR-037**)

> **ADR-number note:** `docs/llm/DECISIONS.md` (on `main`) ends at **ADR-034**. **ADR-035 is reserved on the pending
> branch `auto/discovery-steered-loop-2026-06-30`** (`06e8a7c`, D3-B.4 steered loop, HELD unmerged). **ADR-036 is
> reserved by the pending `worker-thread-parallelization-draft.md`** (also unmerged). To avoid a collision this
> proposal reserves the next free number **beyond both: ADR-037.** Confirm 035/036/037 at merge time and renumber
> if a pending branch landed something else.

---

### ADR-037 (DRAFT) — SBOL3-grounded closed-world genetic vocabulary + BioBrick assembly

- **Status:** DRAFT — awaiting human sign-off. SB2 touches inv #2/#3 and proposes **inv #8** → STOP-THE-LINE.
  Engineered hash-neutral (§6); any hash move is a ledgered 🔁 re-pin, not an accident.
- **Context:** the genome model is already ontology-first (`Locus.tags.so_term` is an SO term; real NCBI CDS;
  `crates/crispr` edits `DnaSequence`; `FlowMatrix`/`TrophicRole` are a typed-interaction layer). The user asked for
  *deep* SBOL integration with a **closed-world** rule: no genetic process may run that is not a defined SBOL
  construct, plus the BioBrick standard-parts discipline.
- **Decision:**
  1. Pin **SBOL3 (v3.1.0)** as canonical; serialize a fixed **JSON-LD subset via serde**; SBOL2 import-only at the
     boundary.
  2. Hand-roll the SBOL3 subset in a new **`crates/sbol` (`std`+serde)** — the exact structs in §2.2; do **not**
     core-depend on the `sbol`/`sbol-rs` crate. Full conformance, SBOL2, Turtle/RDF-XML, SynBioHub live in
     **`sbol-cli`/pySBOL3 as a subprocess** (the `oracle-slim` boundary).
  3. Pin **BioBrick RFC10** as the assembly grammar (Type-IIS/MoClo deferred); ground parts to **iGEM `BBa_*`** ids +
     SO/SBO/GO/ChEBI **IRIs by reference** (no bundling).
  4. Add the **deterministic closed-world validation gate** in front of genotype→phenotype (§3): an ungrounded
     `Locus`/`Edit`/`Interaction` is REJECTED before it can mutate sim state.
  5. **ADR-pin the closed-world rule now; elevate to inv #8 at SB2 sign-off** (§4) — do not add it to the numbered
     invariant list in this design pass.
- **Invariant audit:** inv #1 — no GPL in the SBOL stack; one linked crate (`crates/sbol`), all heavy/networked tools
  subprocess-only (§5). inv #2 — SBOL lives in the core genome layer; `godot/` consumes snapshots, never validates.
  inv #3 — the gate is a pure ordered function (no RNG/HashMap); SB2 is hash-neutral by the §6.1 footprint analysis;
  any move is a ledgered multi-ISA-validated 🔁 re-pin. inv #5 — validator behind a trait (in-core default +
  subprocess realistic). inv #7 — SBOL3 v3.1.0, RFC10, the ontology/registry/tool versions pinned in §2.5.
- **Consequences:** the genome becomes a validated SBOL design; the CRISPR brush becomes grammar-checked part
  insertion; designs are interoperable (importable by pySBOL3/libSBOLj3). **Risk:** SBO has no clean trophic term →
  the `Interaction` trophic encoding is **off-label** SBOL (well-formed + round-trips, but not canonical semantics) —
  pinned as a known caveat (§3.1). **Risk:** the `sbol`/`sbol-rs` crate is pre-1.0/solo-maintained → quarantined to
  the subprocess boundary.
- **Alternatives rejected:** (a) core-depend on `sbol-rs` — rejected for determinism/footprint/bus-factor (§5);
  (b) SBOL2 canonical — rejected, SBOL3 is current + serde-friendlier (§2.1); (c) elevate inv #8 in this pass —
  deferred to SB2 sign-off (§4).
