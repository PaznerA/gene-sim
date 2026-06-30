# SBOL + BioBricks deep integration — design SEED

> **Status: SEED — design thinking + open questions for the `sbol-biobricks-integration-design` research/design
> workflow to expand into a pinned spec + ADR-draft. No production code. Foundational (touches the genome model,
> inv #2, and likely a NEW invariant) → needs the user's sign-off before any implementation.**
>
> User brief (2026-06-30): *"hluboká integrace s SBOL (Synthetic Biology Open Language) — nesmí proběhnout proces,
> který není v tomto jazyce definovaný; promysli i BioBricks Foundation přístup."* → **deep** SBOL integration with
> a **closed-world** constraint: **no genetic process may execute that is not defined as an SBOL construct.** Plus
> the BioBricks Foundation approach (standard, characterized, composable parts + a real registry + an assembly grammar).

## 1. Why this is a small leap, not a rewrite — the model is already ontology-first

`crates/genome` already encodes the SBOL substrate (this is the key finding):
- `Locus { id, name, sequence: DnaSequence (validated ACGT), parameters: [Parameter], tags: OntologyTags }`.
- `OntologyTags { so_term: SoTermId, go_refs: [GoTermId] }` — **a locus's "kind" is a Sequence Ontology term**
  (the module doc: *"Loci are data, not code — a locus's kind is an ontology tag (`SoTermId`), never a Rust enum"*).
  The baked `ecoli.json` tags `talB` with `so_term: 704` = **SO:0000704 "gene"**; sequences are real NCBI CDS.
- `crates/crispr` already operates on `DnaSequence` (PAM finding, edits) → edits are sequence-level operations.

**SBOL is built on exactly these ontologies.** An SBOL `Component`'s `role` IS a Sequence Ontology term; molecular
function uses GO / the Systems Biology Ontology (SBO); a `Component` carries `Sequence`s; composition is `Feature`s +
`Constraint`s; behaviour is `Interaction`s + `Participation`s (with SBO roles). So gene-sim's `Locus`(SO) +
`DnaSequence` + `go_refs` + the trophic `Interaction`s (the FlowMatrix) map **almost 1:1** onto SBOL. Deep SBOL is
therefore a **formalization + a closed-world gate**, not a new biology.

## 2. The closed-world principle (the load-bearing ask → a candidate NEW invariant)

**Every genetic component, edit, and interaction in the sim is grounded in an SBOL construct; the sim cannot
execute a genetic process that has no SBOL grounding.** Concretely:
- A `Locus` is valid only if its `so_term` is a real SO term usable as an SBOL `Component.role`, and its `sequence`
  is a valid SBOL `Sequence`.
- A trophic/regulatory/metabolic effect is valid only if it is expressed as an SBOL `Interaction` (with SBO-typed
  `Participation`s) — no ad-hoc "trait → effect" that isn't an SBOL interaction.
- A CRISPR edit is valid only if it is a transformation of the SBOL design that yields a **valid SBOL document**
  (the edited Component still validates: roles, ranges, constraints, assembly grammar).
- **A design/edit/interaction that fails SBOL validation is REJECTED — it cannot enter the sim.** This is a
  deterministic VALIDATION GATE in front of the genotype→phenotype path.

This is foundational enough to be a **new project invariant** (sibling of inv #2 "genome lives in the core"):
*inv #8 (candidate) — the genetic vocabulary is closed over SBOL.* That elevation is the user's call (the design
workflow proposes it; sign-off elevates it). It is the synbio analogue of the determinism invariant: just as no
sim bytes exist outside the seeded RNG, no genetic process exists outside SBOL.

## 3. The BioBricks Foundation layer — standard, characterized, composable parts

SBOL is the *language*; BioBricks is the *engineering discipline* on top:
- **Standard parts with defined interfaces.** Loci become typed, reusable PARTS — promoter / RBS / CDS / terminator
  (SO roles) — with **characterization** (a datasheet: function, strength, conditions) carried as SBOL `Measure`s /
  parameters. gene-sim's `Parameter` (numeric, domain-bounded) is already the datasheet field.
- **A registry grounding (evidence-based, the user's value).** Seed the parts catalog from the **iGEM Registry of
  Standard Biological Parts** (`BBa_*` ids) + real functions, so a part in-game is a real part. Map the
  already-baked species' genes to registry/SO/GO entries where possible.
- **An assembly GRAMMAR = the closed-world at the composition level.** A species design is *assembled* from parts
  under an assembly standard (classic BioBrick RFC10 prefix/suffix, or a modern Type-IIS / MoClo / 3A standard).
  Only standard-compatible compositions are valid → the player composes designs from parts (a true synbio
  **sandbox**, matching [[gameplay-sandbox-first]]) and the grammar rejects invalid assemblies. The CRISPR brush
  becomes "insert/replace a standard part," not "poke a base."
- **Datasheet-driven phenotype.** A part's characterized parameters feed the deterministic execution (the FBA /
  trophic / metabolism layer) — so "what a part does" is evidence-based + composable, not hand-tuned.

## 4. Architecture (fits the invariants)

- **Design layer (SBOL) vs execution layer (the deterministic sim) — separated** (like the discovery
  design/execution split). SBOL is the canonical *design*; the integer/seeded sim *executes* it.
- **A Rust-native SBOL data model** (a focused SBOL3 subset) in `crates/genome` (or a new `crates/sbol`):
  `Component` / `Feature` / `Sequence` / `Interaction` / `Participation` / `Constraint`, serde-serializable;
  import/export SBOL3 (RDF/JSON-LD) + ideally SBOL2 (XML). The existing `Genome`/`Locus` becomes a **view of / built
  from** an SBOL document. (SBOL libs are Python/C++/Java; a usable Rust crate likely doesn't exist → hand-roll a
  focused model — the research workflow confirms maturity/licensing.)
- **Pluggable validation (inv #5).** An in-core SBOL validator (well-formedness + SO/SBO role checks + the assembly
  grammar) is the default; an optional **subprocess-backed "reference" validator** (pySBOL3 / libSBOLj / a
  SynBioHub query) sits at the process boundary — **GPL/heavy tools stay subprocess-only (inv #1)**; the in-core
  crate stays light. Same trait seam as the on-/off-target scorers + the discovery surrogate.
- **The validation gate** runs before the genotype→phenotype path; deterministic (a pure function of the design) →
  inv #3 safe.

## 5. Invariant audit

- **inv #1 (GPL at the boundary):** SBOL tooling licenses must be checked (libSBOL/pySBOL3/libSBOLj are believed
  Apache-2.0; the registry/SynBioHub data licensing must be verified). Anything copyleft → subprocess-only, never
  linked. The license gate (`check_license.sh`) covers any new linked crate.
- **inv #2 (genome in the core):** SBOL lives in the core genome layer; the renderer still consumes snapshots. The
  player composes parts via journaled operator actions (like the brush) — no genome logic in GDScript.
- **inv #3 (determinism):** SBOL is data (deterministic parse/validate); the sim execution stays integer/seeded. BUT
  **re-grounding the `Genome`/`Locus` model could change birth-enumeration / the hash → a deliberate RE-PIN
  (🔁, STOP-THE-LINE).** The migration must be staged so the *meaning* is preserved and any hash move is an
  ADR-owned, multi-ISA-validated re-pin — not an accident.
- **inv #5 (pluggable science):** the validator + the SBOL-import behind a trait; in-core default + subprocess
  "realistic" impl. The headless crates stay `std`+`serde` (+ a minimal RDF/JSON-LD parser, pinned + justified).
- **inv #7 (pinned):** pin the SBOL version (SBOL2 vs SBOL3), the validator/library version, the BioBrick assembly
  standard (RFC), and the registry snapshot.

## 6. Open questions (for the design/research workflow)

1. **SBOL3 (current, RDF/JSON-LD) vs SBOL2 (XML, more tooling)** — which to pin as canonical? (Lean SBOL3, with an
   SBOL2 import path.)
2. **Rust feasibility** — hand-rolled SBOL3 subset vs a (likely absent) Rust crate vs leaning on a subprocess
   validator for full conformance. How much SBOL must be in-core vs at the boundary?
3. **Depth of "deep"** — full round-trip with SynBioHub / external designs, or a closed in-core SBOL-grounded model
   with import/export? (The closed-world ask implies the *internal* model IS SBOL-grounded + validated; external
   round-trip is a bonus.)
4. **Assembly standard** — classic BioBrick RFC10 vs Type-IIS/MoClo. Which grammar governs valid composition?
5. **Registry grounding** — which real `BBa_*` parts seed the catalog; how to map the existing baked species
   (ecoli/bacillus/…) onto registry/SO/GO entries.
6. **Is the closed-world constraint a NEW INVARIANT (inv #8)?** — elevate it, or keep it an ADR-pinned design rule?
7. **The migration + re-pin** — how to re-ground `Genome`/`Locus` as SBOL without an accidental hash move; what is
   the determinism re-pin plan + the multi-ISA gate.
8. **Licensing** — confirm SBOL libs + registry data licenses against inv #1 + the no-monetization stance
   ([[no-monetization-noncommercial-data]]).

## 7. Slice sketch (the design workflow refines)

- **SB-D `sbol-biobricks-integration-design`** (research + design; THIS seed → the pinned spec + ADR-draft +
  the inv #8 proposal + the re-pin plan). *Web-research SBOL3/BioBricks/registry/licensing; adversarial review.*
- **SB1 — SBOL data model + validator (core, `std`+serde + pinned parser).** The Rust SBOL model + the
  well-formedness/role/grammar validator + the trait seam. Hash-neutral (new model unused until wired).
- **SB2 — Genome ⇄ SBOL grounding + the closed-world GATE (🔁 likely a re-pin, STOP-THE-LINE).** `Locus`/`Genome`
  expressed as SBOL Components; the validation gate in front of genotype→phenotype; the determinism re-pin +
  multi-ISA gate. *Needs sign-off.*
- **SB3 — BioBrick parts catalog + assembly grammar** (registry-grounded standard parts; the composition grammar;
  CRISPR-brush-as-part-insert). Mostly data + the grammar.
- **SB4 — subprocess reference validator (inv #1/#5)** — optional pySBOL3/libSBOLj conformance at the boundary.
- **SB5 — SBOL import/export** (round-trip designs to/from SBOL3 documents / SynBioHub).
- **SB6 — synbio sandbox UI** (renderer-only) — compose a species from standard parts; the assembly grammar guides;
  read the SBOL design in the codex/specimen view.
