# Intervention rework — "BioBlocks": a block-based, library-driven intervention UI — design SEED

> **Status: SEED — design thinking for the `intervention-rework-bioblocks-design` workflow to expand into a spec +
> ADR-draft + slice plan. No production code. Builds ON the SBOL+BioBricks foundation
> ([[sbol-biobricks-closed-world]] / `proposals/sbol-biobricks-integration-draft.md`).**
>
> User brief (2026-06-30): *"rework interventions, aby byly založeny na příjemném UI, které staví na BioBricks a s
> možností použít 'připravené' editace z iGEM knihovny."* → rework the intervention system so it is a **pleasant,
> block-based UI built on BioBricks parts**, with a **library of ready-made edits from the iGEM Registry**.

## 1. The problem — interventions are low-level base-poking, not composition

Today (`godot/main.gd`) interventions are a **tool brush**: pick a tool (`TOOL_CRISPR` = `apply_edit_region` with
cas+target-locus+guide / `TOOL_PCR` / `TOOL_ANTIBIOTIC` cull / `TOOL_NUTRIENT` / `TOOL_TOXIN` / `TOOL_INOCULATE`),
set its params, paint a disc. CRISPR is *poke a locus with a guide* — powerful but not legible, not composable, not
**part-based**. The "saved edits" are the **Variant Lab** (`_saved_variants`: named snapshots of a species' *current*
post-edit genome + reseed) — player-authored, not a curated library of standard parts.

## 2. The vision — "BioBlocks": compose interventions from standard part BLOCKS + apply ready iGEM devices

**BioBricks** are the standard, characterized, composable genetic parts; **BioBlocks** is the **block-based visual
composer** the player drives — snap-together part blocks (the Scratch/Blockly idiom for genetic design), grammar-
guided, on the SBOL+BioBricks foundation. Two flows, one library:

1. **Compose (the BioBlocks canvas).** Drag standard part BLOCKS — promoter · RBS · CDS · terminator · regulator
   (Sequence-Ontology-typed, each a real characterized part with a datasheet: function, strength, conditions) — and
   **snap** them into a device. The **BioBrick assembly grammar** (RFC10 / the SBOL SB3 grammar) governs which
   blocks connect — only standard-compatible compositions snap (a *closed-world at the assembly level*, the visual
   echo of the SBOL closed-world rule). Live **effect preview** (predicted demand-factor / trophic shift) → **apply**.
2. **Apply a ready device (the iGEM library).** A browsable library of **pre-composed, named devices** grounded in
   real **iGEM Registry `BBa_*`** parts — a knockout, an overexpression cassette, a reporter, a logic gate, a
   metabolic switch — the *"připravené editace"*. One click applies it (no composition needed). The library + the
   player's **saved devices** (the Variant Lab, generalized) sit side by side.

## 3. The pleasant UI (the user's "příjemné UI")

- An **RCT-style library browser** (the scenario-selector idiom the user liked): left = a searchable list of
  parts/devices with iGEM ids + datasheets + descriptions; right = a big **BioBlocks canvas** (snap blocks) or the
  selected device's preview + apply.
- **Block aesthetics:** each part is a coloured, shaped block (shape encodes the SO role so only compatible shapes
  snap — the grammar is *felt*, not read); the device reads as a little gene cassette.
- **Effect preview** before commit; the **OVERSIGHT earned-edit economy** gates powerful devices (credit cost ∝ the
  device's part-count / strength — re-using the ADR-028 ledger).
- **Reworks the current tools:** `TOOL_CRISPR` → "apply the composed/selected device" (the BioBlocks output); the
  regional operators (PCR / Antibiotic / Nutrient / Toxin / Inoculate) stay but are **reskinned** consistently; the
  **Variant Lab** becomes the player's *saved-devices shelf* inside the library; the brush still places the device on
  a painted region (the Cities-Skylines colony bind, ADR-029, still applies).

## 4. Architecture — UI on the SBOL+BioBricks core (the invariants hold)

- **inv #2:** the BioBlocks canvas + the library browser are **renderer-side** (GDScript marshals inert part/device
  ids + the composition order); the parts catalog, the **assembly-grammar validation (the closed-world gate, SBOL
  SB1 validator)**, the device→genome resolution, and genotype→phenotype stay in the **core** (`crates/sbol` +
  `crates/genome`). Applying a device = the core validates the SBOL design (closed-world) then issues a **journaled,
  deterministic edit** (the existing `apply_edit`/`apply_edit_region` path, or a new `ApplyDevice` action resolving
  to those). No biology in GDScript.
- **inv #3:** a device-apply is a journaled action like the brush today → deterministic + replayable. New action
  variants are **hash-relevant only for runs that use them**; the pinned single-plant config issues none → the
  pinned literal `0x47a0_3c8f_6701_f240` stays byte-identical (the colony-brush precedent, ADR-029). The composer/
  library are pure renderer.
- **inv #5:** the iGEM library + the parts are **data** (registry-grounded); validation behind the trait.
- **inv #1:** **iGEM Registry data licensing** must be checked (the SBOL epic flagged it: `BBa_*` ids + functions by
  reference; verify terms before bundling sequences; the non-commercial stance, [[no-monetization-noncommercial-data]]).

## 5. Dependency + scope — this is the gameplay payoff of the SBOL foundation

This epic **builds on** SBOL [[sbol-biobricks-closed-world]]: the part BLOCKS **are** SBOL `Component`s (SB3
catalog), the grammar is the SB3 assembly grammar, the snap-validation is the SB1 validator, and a device is an SBOL
design. It **absorbs/refines SBOL SB6 (synbio-sandbox-ui)** into the fuller intervention-UX vision. So the IR
implementation slices are **gated on SBOL SB1–SB3**; only the **design (IR-D)** can run now (doc-only).

## 6. Slice sketch (the design workflow refines)

- **IR-D `intervention-rework-bioblocks-design`** (design + light iGEM-library research) — the BioBlocks composer UX,
  the iGEM ready-edits library curation + data/licensing plan, the rework of the current tools / Variant Lab /
  OVERSIGHT, the SBOL grounding, the apply-as-journaled-edit, the invariant audit + an ADR-draft. *(Can run now,
  doc-only; the rest gate on SBOL SB1–SB3.)*
- `[def]` **IR1 igem-library-data** — curate real iGEM `BBa_*` parts + ready devices as data, grounded in SBOL
  Components (datasheets via `Parameter`/SBOL `Measure`); inv #1 licensing. *dep: SBOL SB3.*
- `[def]` **IR2 bioblocks-composer-ui** (renderer) — the block-based snap canvas + the assembly-grammar guidance
  (shape-compatible snaps) + the effect preview. *dep: IR1 + SBOL SB1 validator.*
- `[def]` **IR3 ready-edits-library-ui** (renderer) — the RCT-style browser of ready devices + the player's saved
  devices (Variant Lab generalized); one-click apply. *dep: IR1.*
- `[def]` **IR4 apply-device-as-journaled-edit** (core) — a device → a validated (closed-world) SBOL-grounded
  journaled edit; OVERSIGHT credit cost by complexity. *dep: SBOL SB2 (the grounding/gate). Hash-relevant only for
  device runs (pinned config neutral).*
- `[def]` **IR5 rework-current-tools** (renderer) — migrate `TOOL_CRISPR` onto the composer; reskin the regional
  operators; the Variant Lab → the saved-devices shelf. *dep: IR2+IR3+IR4.*

## 7. Open questions (for the design workflow)

1. **Term:** "BioBlocks" = the block-based composer on BioBricks parts (confirm the framing; the block idiom = the
   pleasant, grammar-felt UX).
2. **How much composition vs library?** Lead with the **ready iGEM device library** (one-click, evidence-based) +
   offer the BioBlocks canvas for power users? (Lean: library-first for "příjemné", composer for depth.)
3. **iGEM data:** which `BBa_*` parts/devices seed the library; the licensing/data-use verdict (reference vs bundle).
4. **The current tools:** fully replace the CRISPR brush with the composer, or keep both (a "raw edit" expert mode)?
5. **OVERSIGHT economy:** how the credit cost maps to device complexity (part-count / strength).
6. **Determinism:** the device-apply action shape — reuse `apply_edit`/`apply_edit_region`, or a new `ApplyDevice`
   that resolves to them; keep the pinned config neutral.
