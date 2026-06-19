# TAXONOMY — canonical genome & ontology data model

> The **data-model source of truth** (SPEC §4). `crates/genome` implements these types; `crates/crispr`
> implements `CasVariant`/`Edit`; `crates/sim-core` implements `GenotypePhenotypeMap`/`Trait`.
> When code and this file diverge, fix both in the same slice (SPEC closing note).
>
> **Invariants this model bakes in:**
> - *Loci are data, not code.* New locus **kinds** are new ontology nodes (SO/GO terms), **never** new Rust
>   enums (SPEC §4). There is deliberately no `LocusKind` enum.
> - *Determinism (inv. #3).* All containers are **ordered/indexed** (`Vec` + newtype index IDs). No `HashMap`
>   is iterated in sim logic. IDs are stable small integers, not hashed strings.
> - *Pluggable science (inv. #5).* Scoring is a trait, not baked into the genome.

Legend: ✅ implemented in Stage 0 (slice S0) · 🔭 modeled now, lands in a later stage.

---

## 1. Genome ✅ (Stage 0)

A **Genome** is an ordered set of **Loci** plus a version stamp (for the determinism/replay contract).

```rust
/// Stable, ordered handle into a Genome's locus list. Index-based for deterministic iteration.
pub struct LocusId(pub u32);

pub struct Genome {
    /// Schema/model version — recorded in seed.json for replay (SPEC §5/§6).
    pub version: u16,
    /// Loci in a fixed order. Iterate this, never a HashMap (inv. #3).
    pub loci: Vec<Locus>,
}
```

### 1.1 Locus ✅
A Locus has a stable id, a DNA-ish sequence (for PAM/edit realism), typed Parameters, and ontology tags.

```rust
pub struct Locus {
    pub id: LocusId,                 // stable; equals its index in Genome.loci
    pub name: String,                // human-readable label (display/debug only, never iterated for state)
    pub sequence: DnaSequence,       // ACGT bytes — used by crates/crispr for PAM finding / edits
    pub parameters: Vec<Parameter>,  // typed, ordered
    pub tags: OntologyTags,          // SO feature type + GO function refs
}

/// DNA-ish sequence: validated upper-case ACGT bytes (rust-bio operates on these in Stage 1).
/// The inner buffer is PRIVATE and built via `DnaSequence::new(bytes) -> Result<Self, usize>`, which
/// enforces the invariant (every byte ∈ {A,C,G,T}) at construction and returns the first bad index on
/// failure. Read access via `.bases()` / `.len()` / `.is_empty()`.
pub struct DnaSequence(/* private */ Vec<u8>);
```

### 1.2 Parameter ✅
Typed value carrying its own valid domain. **Invariant: a Parameter's value is always within its domain.**

```rust
pub struct ParamId(pub u32);

pub struct Parameter {
    pub id: ParamId,
    pub value: ParamValue,
}

pub enum ParamValue {
    /// Continuous; invariant: min <= value <= max.
    Numeric { value: f64, min: f64, max: f64 },
    /// Categorical; invariant: value < cardinality.
    Enum { value: u16, cardinality: u16 },
    Bool(bool),
}
```

`ParamValue` exposes `is_valid()` (range/cardinality check) and `clamp_into_domain()`. Edits (Stage 1) must
leave every Parameter valid — *an edit never yields an invalid genome* (SPEC §10.4 property test).

### 1.3 Ontology tags ✅
A locus's "kind" and function are **references into the ontology**, not Rust types.

```rust
pub struct SoTermId(pub u32);   // Sequence Ontology feature type, e.g. gene / exon / CDS / promoter
pub struct GoTermId(pub u32);   // Gene Ontology function reference (MF/BP/CC)

pub struct OntologyTags {
    pub so_term: SoTermId,       // the feature type — the locus "kind"
    pub go_refs: Vec<GoTermId>,  // function references (ordered)
}
```

In Stage 0 these are opaque ids seeded from a tiny built-in table; Stage 5 loads the real SO/GO/NCBI-tax
graphs and lets the LLM add schema-validated subclasses (§4 below).

---

## 2. GenotypePhenotypeMap & Traits 🔭 (lands in S1.5)

A transparent function turning Parameters → **Traits**. Start: weighted-sum / simple GRN; later optional
indirect encoding. Traits feed **selection** in the sim and **morphology** in the renderer (via L-system
rule params). Deterministic for a fixed genome.

```rust
pub enum Trait {
    GrowthRate, Reflectance, DroughtTolerance, Fecundity, KillSwitchLinkage, /* … extensible via ontology */
}
pub struct Phenotype { pub values: Vec<(Trait, f64)> }   // ordered

pub trait GenotypePhenotypeMap {
    fn express(&self, genome: &Genome) -> Phenotype;     // pure, deterministic
}
```

Property invariant (SPEC §10.4): trait-derived allele/penetrance frequencies stay in `[0, 1]`.

---

## 3. CRISPR: CasVariant & Edit 🔭 (lands in Stage 1)

### 3.1 CasVariant — a **data row**, not code (kept in `data/cas_variants.ron`, SPEC §4)
```rust
pub struct CasVariantId(pub u16);
pub struct CasVariant {
    pub id: CasVariantId,
    pub name: String,            // "SpCas9", "SaCas9", "Cas12a", "SpRY", …
    pub pam: String,             // IUPAC PAM pattern: NGG, NNGRRT, TTTV, NG, …
    pub cut_offset: i16,         // bp relative to PAM (blunt vs staggered)
    pub edit_window: (i16, i16), // base-/prime-editor window (relative positions); (0,0) for pure DSB
    pub edit_type: EditType,
}
pub enum EditType { Dsb, BaseEdit, Prime }
```
Seed rows (from literature, SPEC §4 / research §2): SpCas9 `NGG`, SaCas9 `NNGRRT`, Cas12a `TTTV` (staggered),
PAM-relaxed `NG`/SpRY, plus base/prime editors.

### 3.2 Edit
```rust
/// Validated upper-ACGT, mirroring `DnaSequence`: PRIVATE inner buffer, built via
/// `GuideSequence::new(bytes) -> Result<Self, usize>` (first bad-byte index on failure); read via
/// `.bases()` / `.len()` / `.is_empty()`. (Implemented in S1.3; lives in `crates/crispr`.)
pub struct GuideSequence(/* private */ Vec<u8>);
pub struct Edit {
    pub cas: CasVariantId,
    pub target: LocusId,
    pub guide: GuideSequence,
}
```
**Application algorithm (SPEC §4):**
1. Find the PAM for the Cas variant in the target locus (rust-bio).
2. Compute on-target efficiency (`OnTargetScore`) and off-target hit count (`OffTargetScore`).
3. If it passes gating thresholds → mutate the target Parameter(s) (and/or add an ontology modifier node).
4. Else → **partial/failed edit with realistic consequences**: off-target side effects perturb Parameters
   *elsewhere*. A failed edit is **never** a silent success (SPEC §10.4).

### 3.3 Score traits 🔭 (S1.3) — pluggable (inv. #5)
```rust
pub trait OnTargetScore  { fn efficiency(&self, locus: &Locus, guide: &GuideSequence, cas: &CasVariant) -> f64; } // [0,1]
pub trait OffTargetScore { fn hit_count(&self, genome: &Genome, guide: &GuideSequence, cas: &CasVariant) -> u32; }
```
Stage 1 ships in-core default impls (heuristic eff, naive count); Stage 2+ adds subprocess-backed
"realistic" impls (Crisflash off-target; crisprScore on-target) without touching sim-core logic.

---

## 4. Ontology graph & the safe extension boundary 🔭 (Stage 5)

The ontology is **data**: nodes are SO/GO/NCBI-tax terms; edges are `is_a` / `part_of`. The LLM may add new
nodes (subclasses of existing SO/GO terms) and new modifier functions **at runtime**, validated against a
fixed JSON schema **and** the ontology graph *before admission*. This is the **only** place new "genes"
enter the system — the safe extension boundary (SPEC §4, §8 Stage 5).

```rust
pub enum OntologyRel { IsA, PartOf }
pub struct OntologyNode { pub id: u32, pub term: String, pub label: String }   // SO:/GO:/NCBITaxon: namespaced
pub struct OntologyGraph {
    pub nodes: Vec<OntologyNode>,            // ordered
    pub edges: Vec<(u32, OntologyRel, u32)>, // (child, rel, parent)
}
```
Admission rule (property invariant, SPEC §10.4): an LLM-added node must (a) validate against the JSON schema
and (b) be a subclass (`is_a`) of an existing SO/GO term in the graph, **before** it is admitted.

---

## 5. Determinism notes (cross-cutting, inv. #3)
- Genome/Locus/Parameter/Ontology iteration is over `Vec`s in stable order. `HashMap`/`HashSet` may be used
  for lookup caches but are **never iterated** to produce state or hashes (use sorted keys / `IndexMap`).
- IDs are stable small integers assigned at load; replays reuse the same assignment (recorded via `version`).
- The end-of-run **stats hash** (harness) hashes ordered fields deterministically — see SNIPPETS.md.
