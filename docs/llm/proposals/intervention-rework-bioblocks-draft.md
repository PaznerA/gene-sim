# Intervention rework — "BioBlocks": a block-based, library-driven intervention UI — PINNED spec + ADR-draft

> **DESIGN ONLY — sign-off required for the SBOL-dependent + any hash-touching slices.** This is a buildable spec,
> NOT an implementation. The renderer slices (IR2/IR3/IR5) are hash-neutral by construction; the ONE core slice
> (**IR4 apply-device-as-journaled-edit**) introduces a device→edit resolution + (optionally) a new `ApplyDevice`
> action and is **gated on the SBOL foundation SB1–SB3** ([[sbol-biobricks-closed-world]] /
> `proposals/sbol-biobricks-integration-draft.md`, **ADR-037 DRAFT**). No production code or Cargo changes accompany
> this doc. Per CLAUDE.md per-slice loop step 2 (*touches an invariant / depends on an un-shipped invariant slice →
> STOP, ask the human*), IR4 and anything SBOL-gated needs explicit sign-off before implementation.
>
> User brief (2026-06-30): *"rework interventions, aby byly založeny na příjemném UI, které staví na BioBricks a s
> možností použít 'připravené' editace z iGEM knihovny."* → rework the intervention system into a **pleasant,
> block-based UI built on BioBricks parts**, with a **library of ready-made edits from the iGEM Registry**.

---

## 0. Thesis — felt grammar + a dual-read cassette, library-first

The distinctive contribution of BioBlocks is **felt grammar**: the RFC10 transcription-unit production
(`promoter rbs cds+ terminator`, SBOL draft §2.3) is encoded as **block connector SHAPES** (the Scratch/Blockly
puzzle-piece idiom), so an illegal cassette physically cannot assemble — the player learns the assembly grammar by
handling it, never by reading it. The finished cassette is simultaneously a **Scratch script** (felt grammar) and a
**publishable SBOL-Visual diagram** (legible biology, the bent-arrow/half-circle/pentagon/T-bar glyphs synbio people
recognise from papers). That dual-read is the payoff. The pleasantness ("příjemné") comes from **library-first**: the
panel opens on one-click curated iGEM devices; the composer is the opt-in depth path, reachable in one click via "edit
a copy", with ready devices doubling as tutorials.

**The load-bearing inv-#2 rule for the whole epic:** *block-shape compatibility is a renderer affordance, never the
validator.* The authority for "is this a legal genetic device" is the core SBOL SB1 gate, run at preview/apply time.
The renderer only snaps shapes against a **baked hint table the core authored** (`data/biobricks/grammar_hints.json`)
— GDScript does table lookups (does connector-class A mate B?), never grammar parsing, never genotype→phenotype.

---

## 1. The problem — interventions are low-level base-poking, not composition

Today (`godot/main.gd`) interventions are a **tool brush**: pick a tool (`TOOL_CRISPR` = `apply_edit_region` with
cas+target-locus+guide / `TOOL_PCR` / `TOOL_ANTIBIOTIC` cull / `TOOL_NUTRIENT` / `TOOL_TOXIN` / `TOOL_INOCULATE`,
`main.gd:244-249`), set its params, paint a disc. CRISPR is *poke a locus with a guide* (`_build_crispr_params`
`:1790`, `_on_inject_pressed` `:2036`, `_apply_brush` `:2202`, `_apply_active_tool` `:2382`) — powerful but not
legible, not composable, not **part-based**. The "saved edits" are the **Variant Lab** (`_saved_variants` `:318`,
`_refresh_saved_variants_section` `:1728`: named snapshots of a species' *current* post-edit genome + reseed) —
player-authored, not a curated library of standard parts.

---

## 2. The BioBlocks composer UX + the RCT-style library browser

Reuses the scenario/gallery selector idiom the user liked (`_on_menu_open_gallery` `:666`, the gem gallery, the codex
browse panel): a scrollable left rail + a big right pane.

### 2.1 Left rail — the catalog (three sections, searchable/filterable)

- **Ready Devices** (the iGEM library, the *"připravené editace"*): named pre-composed cassettes — see the seed set
  in §3. Each row: cassette glyph · name · `BBa_*` provenance badge · one-line datasheet headline (role/strength) ·
  credit-cost pill.
- **Parts** (the BioBlocks palette): the standard part blocks grouped by SO role (Promoters / RBS / CDS /
  Terminators / Regulators), each a coloured shaped chip with its `BBa_*` id + datasheet headline. Dragged onto the
  canvas.
- **My Devices** (the generalized Variant Lab): player-composed devices + the existing saved whole-species variants
  (`_saved_variants`), side by side.

### 2.2 Right pane — the big canvas/preview (mode-dependent)

- **Browse a ready device** → a large read-only cassette render (snapped blocks, a finished script) + datasheet
  (function, characterized strength, conditions, BBa provenance + attribution line) + **effect preview** (predicted
  demand-factor / trophic shift, §7) + credit cost + one-click **Apply (whole-species / region-brush)**.
- **Compose mode** → the **BioBlocks snap canvas**: a 5′→3′ transcription rail; drag part blocks from the left
  palette; they snap only where connector shapes mate; a live mini-datasheet + effect-preview updates as the cassette
  grows; **Apply** + **Save as device** (→ My Devices).

### 2.3 The block-shape grammar — the felt RFC10 production

The SO role determines the block's notch shapes, so the grammar is physical (derived from SBOL draft §2.3's pinned SO
roles — promoter `SO:0000167`, RBS `SO:0000139`, CDS `SO:0000316`, terminator `SO:0000141`, gene `SO:0000704`,
[SBOL3 data model v3.1.0](https://sbolstandard.org/datamodel-specification/version-3.1.0/)):

| Block | SO role | Left edge (5′) | Right edge (3′) | Idiom |
|---|---|---|---|---|
| **Promoter** | SO:0000167 | flat "hat" (nothing precedes) | transcription-start male chevron | Scratch event-hat |
| **RBS** | SO:0000139 | accepts promoter chevron | ribosome-ready tab | statement block |
| **CDS** | SO:0000316 | accepts RBS tab *or* a CDS poly-cistron tab | coding-continue tab (→ CDS or terminator) | repeatable (cds+) |
| **Terminator** | SO:0000141 | accepts CDS coding-continue | flat "cap" (nothing follows) | end-cap block |
| **Spacer/scar** | (scar) | pass-through | pass-through | BioBrick scar |
| **Regulator/operator** | (SBO interaction) | clamps *above/below* a promoter | — | Scratch C-block wrap → an SBO regulatory `Interaction` |

A malformed device (RBS with no upstream promoter, two terminators, CDS with no RBS) cannot snap. Blockly-gentle
guidance, not punishment: only sockets that fit light up; a ghost-preview shows the next legal part. Connector
compatibility is the **baked lookup table** `data/biobricks/grammar_hints.json`, authored by the core from its RFC10
production (§8) — GDScript does table lookups, never grammar parsing.

### 2.4 Reads-as-a-cassette — SBOL Visual

Block *bodies* carry **SBOL Visual (SBOLv) glyphs** (bent-arrow promoter, half-circle RBS, pentagon CDS, T-bar
terminator — [sbolstandard.org/visual](https://sbolstandard.org/visual-about/),
[SBOL Visual 3.0 spec](https://sbolstandard.org/docs/SBOL-Visual-3.0.pdf)) + part name + `BBa_*` id; block *connectors*
carry the grammar. So the finished device is at once a Scratch script and a recognizable genetic-circuit diagram. **Pin
SBOL Visual 3.0** (the glyph set defined against the SBOL3 data model, consistent with the parent doc's SBOL3 pin) at
IR2, and pin the exact glyph-asset release tag + its asset license (the official SVG/PNG/PDF glyphs ship from the SBOL
Visual GitHub releases) under inv #7. ([SBOL Visual glyphs](https://sbolstandard.org/visual-glyphs/).)

---

## 3. The iGEM ready-edits library — the seed device set

The library is **data** (`data/biobricks/devices.json` + `data/biobricks/parts.json`), grounded as SBOL `Component`s
(SBOL draft §2.2): each part is an SBOL `SequenceFeature`/`SubComponent` with `role = SoRole(so_term)`; each device is
a `Component` whose `features` are the parts ordered by a `Constraint.precedes` chain (the RFC10 transcription unit).
Each part/device carries an inert `bba_id: Option<String>` provenance tag (SBOL draft §2.4 *reference, do not
redistribute*).

**Confidence policy.** `BBa_*` ids below marked **[real]** are well-known, long-standing iGEM Registry parts cited by
identifier (verify each live part page at IR1 — [parts.igem.org](https://parts.igem.org/),
[registry.igem.org](https://registry.igem.org/)). Ids marked **[placeholder]** are *not* asserted as real registry ids
and MUST be replaced with a verified `BBa_*` (or left as an explicit local id) at IR1 — they stand for a class, not a
specific catalogued part. **No real Registry sequence bytes are bundled in this design pass** (see §4).

### 3.1 Seed PARTS (the palette)

| Part | SO role | `BBa_*` | Conf. | Datasheet headline |
|---|---|---|---|---|
| Anderson constitutive promoter (strong) | promoter SO:0000167 | **BBa_J23100** | [real] | constitutive, family J23100–J23119 strength ladder |
| Anderson constitutive promoter (weak) | promoter | **BBa_J23114** | [real] | constitutive, lower strength (same family) |
| pLac (LacI-repressible) | promoter | **BBa_R0010** | [real] | repressed by LacI, induced by IPTG |
| pTet (TetR-repressible) | promoter | **BBa_R0040** | [real] | repressed by TetR, induced by aTc |
| RBS (community standard, strong) | RBS SO:0000139 | **BBa_B0034** | [real] | Elowitz strong RBS, the de-facto standard |
| RBS (weak) | RBS | **BBa_B0032** | [real] | weaker translation initiation |
| GFP (GFPmut3b) | CDS SO:0000316 | **BBa_E0040** | [real] | green reporter CDS |
| RFP (mRFP1) | CDS | **BBa_E1010** | [real] | red reporter CDS |
| TetR repressor | CDS | **BBa_C0040** | [real] | represses pTet promoters (regulator) |
| LacI repressor | CDS | **BBa_C0012** | [real] | represses pLac promoters (regulator) |
| Double terminator | terminator SO:0000141 | **BBa_B0015** | [real] | the canonical strong double terminator |

### 3.2 Seed DEVICES (the ready library)

| Device | Composition | `BBa_*` provenance | Conf. | Sim effect (grounded) |
|---|---|---|---|---|
| **Constitutive reporter** | J23100 · B0034 · E0040 · B0015 | parts all [real] | [real parts] | reporter overexpression → demand-factor ↑ on the chosen locus |
| **Lac-inducible reporter** | R0010 · B0034 · E0040 · B0015 | parts all [real] | [real parts] | conditional expression (IPTG) → context-gated factor shift |
| **Tet-repressible reporter** | R0040 · B0034 · E1010 · B0015 | parts all [real] | [real parts] | repressible expression → factor shift under TetR |
| **Constitutive overexpression** | J23119 · B0034 · ⟨target CDS⟩ · B0015 | promoter/RBS/term [real]; ⟨target⟩ resolves to a species locus | [real scaffold] | strong constitutive drive on the targeted locus → factor ↑ |
| **Knockdown (repressor cassette)** | J23114 · B0034 · C0040(TetR) · B0015 + a pTet operator C-block over the target promoter | parts [real] | [real] | SBO regulatory `Interaction` represses the target → factor ↓ |
| **Knockout cassette** | (CRISPR disruption — NOT a transcription unit) | knockout is a *process*, not a registry part | **[placeholder provenance]** | grounds to `EditKind::Knockout` on the target locus → factor → neutral/loss-of-function |
| **Toggle switch** | two mutually-repressing promoter+repressor TUs (Gardner–Collins topology) | composite; exact iGEM composite id varies | **[placeholder — composite]** | bistable regulatory motif; mark composite, verify/curate at IR1 |

**Honest grounding caveat (carried from SBOL draft §3.1):** SBO has no clean predation/trophic term — a device's
*ecological* effect (demand-factor / trophic delta) is encoded as an SBO `Interaction` that round-trips but is
**off-label** SBOL. The gate validates well-formedness, not biological correctness of the off-label trophic mapping.
The "knockout" and "toggle" rows are honest about being a *process* and a *composite motif* respectively, not single
catalogued `BBa_*` parts.

---

## 4. inv #1 / data-licensing VERDICT — iGEM Registry data (reference-only vs bundle)

**inv #1 as worded ("GPL stays at the process boundary") is NOT triggered** — there is no GPL/AGPL anywhere in the
iGEM/SBOL stack (the SBOL draft §5 confirmed this for the whole SBOL toolchain). The concern here is a **data-use
license**, audited under the same banner the SBOL draft used and the project's non-commercial data stance
([[no-monetization-noncommercial-data]], ADR-018 precedent).

**The two regimes:**

- **REFERENCE-ONLY (the DEFAULT, and what this design ships) — SAFE.** Storing a `BBa_*` id + the part name + a
  published characterized value (e.g. "B0034 = strong RBS") and *linking* to the live part page is **referencing a
  public identifier, not redistributing the Registry database** → no Registry license obligation is triggered (the
  same posture the SBOL draft §2.4/§5 pinned for `BBa_*` ids + SO/SBO/GO/ChEBI IRIs). All seed parts/devices in §3 are
  authored this way; **no real Registry sequence bytes are bundled.** A device's actual DNA payload resolves to the
  *already-baked, NCBI-sourced* species loci (`data/species/*.json`, e.g. `ecoli.json` `GCF_000005845.2`), never to
  copied Registry content.

- **BUNDLE Registry sequence content (copying the DNA bytes off a `BBa_*` page into `data/biobricks/*.json`) — GATED,
  verify at IR1 before any such byte lands.** The governing terms:
  - The **BioBrick™ Public Agreement (BPA)** is a *contract* framework (Contributor + User agreements), not a standard
    open-source/CC license: contributors "irrevocably agree … not to assert or threaten to assert" patents or
    proprietary rights against users, and once a user "execute[s] the BioBrick™ User Agreement [they] are free to use
    all contributed material made via the BPA" ([BioBricks Foundation:BPA, OpenWetWare](https://openwetware.org/wiki/The_BioBricks_Foundation:BPA);
    [biobricks.org/bpa](https://biobricks.org/bpa/contributions/);
    [BioBrick Public Agreement v1a, DSpace@MIT](https://dspace.mit.edu/handle/1721.1/50999)). The BPA is a
    *patent-non-assert* instrument — it does **not** by itself grant a clean redistribution-of-the-database right.
  - The **iGEM Registry "Get & Give (& Share)"** philosophy + the parts.igem.org Terms of Use historically gate use to
    research/academic and require checking before commercial use / bulk redistribution
    ([Registry of Standard Biological Parts, Wikipedia](https://en.wikipedia.org/wiki/Registry_of_Standard_Biological_Parts);
    [parts.igem.org Help:Parts](https://parts.igem.org/Help:Parts)). *(The live parts.igem.org Terms page returned 403
    to automated fetch during this design pass — IR1 MUST web-confirm the current Terms of Use text before bundling.)*

  **Verdict for the GATED case:** bundling is **acceptable ONLY** (a) under the project's **non-commercial stance**
  ([[no-monetization-noncommercial-data]] — the app is not monetized, the same basis that un-gated the BiGG/FBA data
  under ADR-018), (b) after IR1 **web-confirms** the live parts.igem.org Terms of Use + honors the BPA User terms, and
  (c) with each bundled sequence carrying its `BBa_*` provenance + a recorded attribution/share line. This is a
  **per-part ledgered decision**, not a blanket grant.

**Net:** the seed library is reference-only and ships with zero Registry license exposure. Any future *bundle* or any
*export/redistribution* feature (e.g. SBOL export to SynBioHub, SBOL draft SB5) is a separate, ledgered,
non-commercial-stance + Terms-verified decision. `scripts/check_license.sh` (gate step 8) covers new *linked* crates;
this data verdict is recorded here + in the ADR (§12).

---

## 5. Rework of the current surfaces (concrete `main.gd` mapping)

- **`TOOL_CRISPR` (`:244`, `_apply_brush` `:2202` / `_on_inject_pressed` `:2036`) → "Apply Device".** The primary
  surface becomes apply-the-composed/selected-device. The raw Cas/Locus/Guide pickers (`_build_crispr_params` `:1790`)
  are DEMOTED to an **"Advanced: raw edit" expander** — kept, not deleted (OQ#4: keep both; raw = expert mode).
  Critical: the harness, gem-replay (`_apply_gem_edit` / `gem_edit_schedule` path, `main.gd:1180-1265`), and discovery
  all depend on the raw `apply_edit`/`apply_edit_region` `#[func]`s; a device **RESOLVES TO those same journaled
  actions** (device = sugar over raw edits, §6), so journal/replay/hash semantics are unchanged.
- **The brush is unchanged as PLACEMENT.** Device = payload; brush = placement. "Apply (region)" →
  `apply_edit_region` (the ADR-029 colony bind — a named, family-coloured district — still applies); "Apply (whole
  species)" → `apply_edit`. Exactly today's `_apply_brush` vs `_on_inject_pressed` split.
- **PCR / Antibiotic / Nutrient / Toxin / Inoculate (`:245-249`, `_apply_active_tool` `:2382`) stay, reskinned, as a
  sibling "Operators" group.** They are environmental/population operators, NOT transcription units — forcing them into
  blocks would be dishonest. They keep their sub-panels, get the shaped-pill/datasheet-card visual language. Honest
  scope: **BioBlocks reworks the genetic-edit tool only.**
- **Variant Lab (`_saved_variants` `:318`, `_refresh_saved_variants_section` `:1728`, `_on_reseed_variant_pressed`
  `:1766`) → "My Devices" shelf.** Generalized to hold BOTH composed BioBlocks devices AND the existing post-edit
  SpeciesSpec snapshots (`export_species_json`, `godot-sim/src/lib.rs:701`). The arm→Inoculate bridge
  (`_armed_variant_idx` `:323` → `_inoculate_at` `:2441`, via `register_contaminant_json` `lib.rs:674`) is
  UNCHANGED — the inv-#2 opaque-JSON boundary is reused verbatim. A saved device → seedable variant (Inoculate
  payload) is the one real bridge between the composer and the environmental operators.
- **OVERSIGHT (`_on_oversight_preview_pressed` `:1616`, `_on_oversight_commit_pressed` `:1641`; core
  `preview_ecoli_edit` `lib.rs:1044`, `oversight_status` `lib.rs:999`) gates device cost.** A *device apply* spends
  OVERSIGHT credit; cost = `device_cost(device_json)` ∝ part-count × characterized strength — a deterministic-integer
  read-only core `#[func]` (inv #2: GDScript reads the marshaled int). The preview shows cost-vs-credit via the
  existing `affordable` gate, unifying today's separate "free CRISPR brush" and "earn→spend deep edit" surfaces.

---

## 6. The APPLY path — device → a validated SBOL-grounded JOURNALED edit

A device apply must (a) resolve the inert device JSON to concrete sim mutation(s), (b) pass the SBOL closed-world gate
(SBOL draft §3, the SB1 validator), and (c) journal a deterministic action — **with the pinned config staying neutral
so `0x47a0_3c8f_6701_f240` is byte-identical** (`crates/sim-core/src/lib.rs:3544`, `:3708`). Two paths, recommend
shipping (A) then layering (B):

### 6.1 Path A (recommended IR4 core) — desugar via a read-only core `#[func]`, reuse the existing journaled actions

Mirror the **ADR-030 `gem_edit_schedule` precedent** (`main.gd:1180`, the core resolves; GDScript moves inert data).
Add a **read-only, RNG-free** `#[func] device_resolve(device_json) -> [{cas, target, guide, species, region?}]` to
`crates/godot-sim` that runs the device→edits resolution **in the core** (closed-world validated), and the renderer
fires the **EXISTING** `apply_edit(cas, target, guide, species)` (`lib.rs:549`) / `apply_edit_region(cas, target,
guide, cx, cy, radius)` (`lib.rs:605`) per resolved entry. Consequences:
- **inv #2 holds** — biology/resolution is in the core; GDScript moves inert ids + fires the existing wires.
- **inv #3 trivially-neutral** — *no new Action variant.* A device apply's `actions.ndjson` is just the desugared
  `ApplyEdit`/`ApplyEditRegion` entries — **byte-identical** to a hand-applied raw edit with the same params → replay
  + `hash_world` semantics unchanged. The pinned single-plant config issues no device applies → `0x47a0` byte-identical.
- `device_resolve` is a *deterministic pure function of the device JSON* (ordered traversal, integer, zero SimRng, no
  HashMap iteration), and runs the SB1 closed-world gate before returning (a shape-legal-but-core-illegal device
  returns a typed violation, never a partial apply).

### 6.2 Path B (SBOL-SB2-gated upgrade) — `Action::ApplyDevice` for a device-aware journal

Once the SBOL SB2 closed-world GATE lands in `SpeciesSpec::build` + `apply_edit` (SBOL draft §3.2/SB2), introduce a
new **`Action::ApplyDevice(DeviceRef, Option<RegionSpec>)`** that the core validates (closed-world) and resolves to
the same Genotype-param mutation, journaling the **device identity** (not the desugared edits). This makes a replay
self-document "applied device X" and lets the SB1 gate run authoritatively at step time. Hash posture:
- A *new* hash-relevant action variant, but **hash-relevant ONLY for runs that use it** (the colony-brush /
  `ApplyEditRegion` precedent, ADR-029) — the pinned single-plant config issues none → `0x47a0` byte-identical.
- Because `ApplyDevice` ultimately mutates the *same* `Genotype` param a raw `ApplyEdit` would, a device run's
  `hash_world` matches the equivalent raw-edit run (`hash_world` folds `Genotype.0.to_bits()` + DroughtTol/ThermalTol,
  not the action vocabulary — SBOL draft §6.1).

**Recommendation:** ship **Path A** in IR4 (hash-trivially-neutral, no new action, reuses `gem_edit_schedule`'s proven
pattern), and add **Path B** as the device-aware-journal refinement once SBOL SB2's gate exists. Either way the pinned
config is neutral and `0x47a0_3c8f_6701_f240` stays byte-identical.

---

## 7. Effect preview before commit

Reuse the OVERSIGHT `preview_ecoli_edit` pattern (`_on_oversight_preview_pressed` `:1616`; core `lib.rs:1044` — a
read-only `#[func]`, zero SimRng, returns `predicted_factor_q` / `current_factor_q` / `affordable`). Add a read-only
**`preview_device(device_json) -> {predicted_factor_q, current_factor_q, trophic_delta, affordable, violations}`** that
returns the device's predicted demand-factor / trophic delta WITHOUT mutating, surfaced as the existing
`q=… → demand factor X× (now Y×)` before/after bar. **The preview also dry-runs the SB1 closed-world gate** → a
shape-legal-but-core-illegal device surfaces its typed `Vec<SbolViolation>` BEFORE the player spends, never as an
apply-time surprise. Like `preview_ecoli_edit` it draws zero SimRng and never mutates → off-hash, inv #3 neutral.

---

## 8. Data model (renderer-consumable, core-authored, all inert)

All under a new `data/biobricks/` (parts as data, like `data/cas_variants.ron` / `data/ecoli_ko_table.json`):

1. `data/biobricks/parts.json` — the palette: `{ part_id, name, so_role, bba_id?, datasheet_headline, strength_q,
   sbolv_glyph, connector_class }[]`. Authored by the core from SBOL `Component`s (SBOL draft §3.1 grounding).
2. `data/biobricks/devices.json` — the ready library (§3.2): `{ device_id, name, bba_provenance[], parts[] (ordered),
   constraints[] (precedes chain), effect_class, cost_hint_q, datasheet }[]`.
3. `data/biobricks/grammar_hints.json` — **the core-authored connector compatibility table** (§2.3): `{ left_class →
   [legal right_class…] }`. The renderer does pure table lookups; it is a *baked projection of the core's RFC10
   production*, never the grammar authority.

All three are inert data on disk until a config/device-apply references them (the SBOL draft §2.4 catalog posture) →
hash-neutral by construction. `device_cost`, `device_resolve`, and `preview_device` are the three core `#[func]`s the
renderer reads; the validator is the SBOL `InCoreValidator` (SBOL draft §2.2, behind the `SbolValidator` trait,
inv #5).

---

## 9. Invariant audit

- **inv #2 (UI renderer-only).** The BioBlocks canvas + RCT library browser are GDScript moving **inert** part/device
  ids + composition order + the **core-authored** `grammar_hints.json`. Block-shape compatibility is a renderer
  affordance over a baked hint table — *never* the validator. The parts catalog, SBOL SB1 validation / closed-world
  gate, `device_resolve`, `device_cost`, `preview_device`, and all genotype→phenotype live in the core (`crates/sbol`
  + `crates/genome` + `godot-sim` `#[func]`s). **No genome logic in GDScript.**
- **inv #3 (journaled + hash-neutral for the pinned config).** Device applies desugar to the existing journaled
  `ApplyEdit`/`ApplyEditRegion` (Path A — *no new action*, byte-identical journal) or a new `ApplyDevice` (Path B —
  hash-relevant only for device runs). The pinned single-plant config issues no device applies → `hash_world` folds an
  unchanged `Genotype` + tolerances → **`0x47a0_3c8f_6701_f240` byte-identical** (`lib.rs:3544`, `:3708`). All
  resolution/cost/preview functions are ordered, integer/deterministic, zero SimRng, no HashMap iteration.
- **inv #1 (iGEM data licensing).** No GPL → inv #1-as-GPL not triggered. The data-use license is gated in §4:
  reference-only (default, ships) = safe; bundling Registry sequence bytes = per-part ledgered, non-commercial-stance
  + parts.igem.org-Terms-verified at IR1.
- **inv #5 (library + science as data behind a trait).** The iGEM library + parts are `data/biobricks/*.json`
  (registry-grounded data, like `cas_variants.ron`); validation behind the SBOL `SbolValidator` trait (in-core default
  + subprocess realistic). Swapping the validator impl touches no sim-core logic.
- **inv #8 (candidate, SBOL closed-world — SBOL draft §4, ADR-037).** BioBlocks is a *consumer* of inv #8: every
  device that applies is closed-world-validated before it mutates state. BioBlocks does not define or alter the
  closed-world rule; it gates on it (IR4 dep on SBOL SB2).
- **inv #7 (versions pinned).** SBOL3 v3.1.0 + RFC10 (inherited from the SBOL draft); SBOL Visual 3.0 + the glyph-asset
  release tag pinned at IR2.

---

## 10. Slice plan IR1..IR5 (deps on SBOL SB1–SB3; hash verdicts)

`IR-D` (this design pass) is done on sign-off. **All of IR1–IR5 gate on the SBOL foundation** — the part BLOCKS *are*
SBOL `Component`s (SB3 catalog), the snap-validation *is* the SB1 validator, a device *is* an SBOL design closed-world
gated by SB2. This epic **absorbs/refines SBOL SB6 (synbio-sandbox-ui)** into the fuller intervention-UX.

| Slice | Scope | Deps | Hash verdict | Sign-off |
|---|---|---|---|---|
| **IR1 igem-library-data** | curate the §3 seed parts + ready devices as `data/biobricks/{parts,devices,grammar_hints}.json`, grounded as SBOL `Component`s (datasheets via SBOL `Measure`); **IR1 web-confirms the iGEM Terms + resolves the [placeholder] BBa ids**; inv #1 §4 verdict applied. | **SBOL SB3** | **✅ hash-neutral** — inert data on disk; reference-only sequences (devices resolve to existing baked loci, **no new `parameter_count`**). *⚠️ conditional 🔁 only if a bundled datasheet `Measure` adds a param (SBOL draft §6.3 — each ledgered).* | data + licensing sign-off |
| **IR2 bioblocks-composer-ui** (renderer) | the block-based snap canvas + shape-compatible snaps (reads `grammar_hints.json`) + the SBOLv-glyph block bodies + the live effect preview; pin SBOL Visual 3.0 + glyph release. | IR1 + **SBOL SB1** (validator for the preview dry-run gate) | **✅ hash-neutral** — renderer-only, zero Rust sim-path | normal slice |
| **IR3 ready-edits-library-ui** (renderer) | the RCT-style browser of ready devices + the player's saved devices (Variant Lab generalized → My Devices); one-click apply; the arm→Inoculate bridge unchanged. | IR1 | **✅ hash-neutral** — renderer-only | normal slice |
| **IR4 apply-device-as-journaled-edit** (core) | `device_resolve` + `device_cost` + `preview_device` `#[func]`s; **Path A** desugar-via-core-reuse (recommended, no new action); optional **Path B** `Action::ApplyDevice`; OVERSIGHT credit cost by complexity. | **SBOL SB2** (the closed-world gate) + IR1 | **✅ hash-neutral for the pinned config** (no device applies); device runs hash-relevant. Path A trivially-neutral (no new action); Path B = new variant, neutral for pinned. | **REQUIRED — core slice, SBOL-SB2-gated, STOP-THE-LINE if `0x47a0` moves** |
| **IR5 rework-current-tools** (renderer) | migrate `TOOL_CRISPR` onto Apply-Device (raw edit → "Advanced" expander); reskin PCR/Antibiotic/Nutrient/Toxin/Inoculate as the Operators group; Variant Lab → My Devices. | IR2 + IR3 + IR4 | **✅ hash-neutral** — renderer-only | normal slice |

Critical path: **SBOL SB1 → SB2(🔁🛑) → SB3**, then **IR1 → (IR2 ∥ IR3) → IR4 → IR5**. Only **IR4** touches core, and
it is hash-neutral for the pinned config. Nothing in IR1–IR5 is a re-pin under the recommended Path A.

---

## 11. Open-question resolutions (seed §7)

1. **Term.** "BioBlocks" = the block-based composer on BioBricks parts. Confirmed framing; the block idiom = the
   pleasant, grammar-felt UX.
2. **Composition vs library.** **Library-first, composer one click away, ready devices as tutorials** (§0, §2): default
   landing = Ready Devices (zero composition, evidence-based, "příjemné"); "✎ Edit a copy" opens any device
   pre-populated on the canvas (the Blockly remix idiom); progressive disclosure hides regulators/operators/spacers
   behind an "advanced parts" expander.
3. **iGEM data.** Seed set in §3 (real `BBa_*` where verifiable, else clearly [placeholder]); licensing verdict in §4
   (reference-only ships; bundling is a per-part non-commercial-stance + Terms-verified gate).
4. **Current tools.** **Keep both** — Apply-Device is primary; the raw Cas/Locus/Guide brush is demoted to an
   "Advanced: raw edit" expander (the harness/gem-replay/discovery all still need it). Operators stay, reskinned.
5. **OVERSIGHT economy.** `device_cost(device_json)` ∝ part-count × characterized strength, a deterministic-integer
   read-only `#[func]`; gated by the existing `affordable` check (`oversight_status` `lib.rs:999`).
6. **Determinism.** **Path A** (desugar to the existing `apply_edit`/`apply_edit_region`) recommended →
   hash-trivially-neutral; **Path B** (`Action::ApplyDevice`) is the SBOL-SB2-gated device-aware-journal upgrade,
   hash-relevant only for device runs. Pinned config neutral either way.

---

## 12. ADR-DRAFT (reserve **ADR-038**)

> **ADR-number note (refreshed 2026-06-30):** `docs/llm/DECISIONS.md` on `main` now ends at **ADR-036** (the
> off-thread sim worker / W1, ACCEPTED + landed). **ADR-035** is reserved on the held branch
> `auto/discovery-steered-loop-2026-06-30` (D3-B.4 steered loop, unmerged — it slots in above ADR-036 when that
> branch merges). **ADR-037** is reserved by the SBOL foundation (`sbol-biobricks-integration-draft.md`, still a
> DRAFT awaiting sign-off). So this epic reserves the next free number **ADR-038**. Confirm 037/038 are still free at
> merge time and renumber if a pending branch landed something else.

### ADR-038 (DRAFT) — BioBlocks: block-based, library-driven intervention UI on the SBOL+BioBricks foundation

- **Status:** DRAFT — awaiting human sign-off. The renderer slices (IR2/IR3/IR5) are hash-neutral; the core slice
  **IR4** is **gated on SBOL SB1–SB3 (ADR-037)** and touches the apply path → STOP-THE-LINE if `0x47a0` moves.
- **Context:** today's interventions are a low-level CRISPR brush + 5 environmental operators + the Variant Lab
  snapshots (`godot/main.gd`). The user asked to rework them into a *pleasant, block-based UI built on BioBricks* with
  a *library of ready-made iGEM edits*. The SBOL foundation (ADR-037) already makes the genome an SBOL-shaped,
  closed-world-validated design — BioBlocks is the gameplay payoff of that foundation.
- **Decision:**
  1. A **block-based snap composer** (BioBlocks) + an **RCT-style two-pane library browser** (Ready Devices / Parts /
     My Devices), **library-first** with the composer one click away.
  2. **Shape-encodes-role grammar:** the RFC10 production is felt as connector shapes; compatibility is a
     **core-authored baked hint table** (`grammar_hints.json`), *never* the validator. Block bodies carry **SBOL Visual
     3.0** glyphs → the cassette is a dual-read Scratch-script-and-SBOLv-diagram.
  3. The **iGEM ready-edits library** as data (`data/biobricks/{parts,devices,grammar_hints}.json`), grounded as SBOL
     `Component`s; real `BBa_*` by reference where verifiable, [placeholder] otherwise.
  4. **inv #1 / data-licensing:** reference-only ships (safe); bundling Registry sequence bytes is a per-part,
     non-commercial-stance + parts.igem.org-Terms-verified gate (BPA + iGEM Terms; §4).
  5. **Apply path:** a device → a **validated (SBOL closed-world) SBOL-grounded journaled edit.** Ship **Path A**
     (core `device_resolve` desugars to the existing journaled `apply_edit`/`apply_edit_region` — no new action,
     hash-trivially-neutral); add **Path B** `Action::ApplyDevice` as the SBOL-SB2-gated device-aware-journal upgrade.
     The pinned single-plant config issues no device applies → `0x47a0_3c8f_6701_f240` byte-identical.
  6. Rework `TOOL_CRISPR` → "Apply Device" (raw edit demoted to an "Advanced" expander, kept for
     harness/gem-replay/discovery); reskin the 5 operators; generalize the Variant Lab → **My Devices**; OVERSIGHT
     gates `device_cost`.
- **Invariant audit:** inv #1 — no GPL; iGEM data reference-only (bundling gated, §4). inv #2 — composer/browser are
  renderer-only over a core-authored hint table; all biology/validation/resolution in the core. inv #3 — Path A reuses
  the existing journaled actions byte-identically; pinned config neutral → `0x47a0` byte-identical; any move is
  STOP-THE-LINE. inv #5 — library as data; validation behind the `SbolValidator` trait. inv #7 — SBOL3 v3.1.0, RFC10,
  SBOL Visual 3.0 + glyph release pinned. inv #8 (candidate, ADR-037) — BioBlocks *consumes* the closed-world gate; it
  does not define it.
- **Consequences:** interventions become legible, composable, part-based, and one-click for the library path; the
  Variant Lab generalizes; the OVERSIGHT economy unifies the free-brush and earned-edit surfaces. **Dependency:** all
  of IR1–IR5 gate on SBOL SB1–SB3 — only IR-D (this design) can run now. **Risk:** the off-label SBO trophic encoding
  (SBOL draft §3.1) means a device's *ecological* effect is well-formed-but-off-label SBOL — pinned as a known caveat.
  **Risk:** iGEM Terms-of-Use text must be web-confirmed at IR1 before any sequence bundling.
- **Alternatives rejected:** (a) fully replace the raw CRISPR brush — rejected, the harness/gem-replay/discovery need
  it (kept as an expert expander); (b) composer-first instead of library-first — rejected for "příjemné" (library-first
  is the immediately-gratifying default); (c) a renderer-side grammar parser — rejected (inv #2: the renderer reads a
  baked hint table, the core owns the grammar); (d) Path B as the *only* apply path — deferred behind Path A so IR4 can
  ship hash-trivially-neutral before SBOL SB2.
