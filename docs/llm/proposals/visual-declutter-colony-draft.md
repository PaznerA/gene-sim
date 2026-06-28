# Visual de-clutter — COLONIES as an off-hash render aggregation (draft)

> **Status:** DESIGN ONLY — sign-off-ready draft. No production code.
> **Date:** 2026-06-28 · **Author:** synthesis of three lens proposals (render-arch / data-determinism / ux-lod).
> **Brief:** the play screen is *zaspamovaná* (spammed with per-organism dots) and unreadable. Introduce
> **COLONIES** — a map polygon that layers better than individual organisms and **unifies a species**
> (incl. **variations** after brush edits — *a CRISPR brush edit CREATES A NEW COLONY*). Each zoom scope
> should, by organism **size**, "pop" selected colonies open to individual organisms. **Plants** are the
> most-realistic + always-visible aggregate, belong to ≥1 colony, and the brush splits them into
> **districts** (Cities-Skylines).
> **Hard constraints:** inv #2 (render is read-only — no genotype→phenotype in GDScript) and inv #3
> (off-hash; the pinned literal `0x47a0_3c8f_6701_f240` stays byte-identical).

---

## 1. The problem + goals

**The problem.** At Field scope the renderer draws up to `MAX_DOTS_PER_CELL` (= 5, `organisms.gd:21`)
markers **per non-empty cell**. A multi-species field is then a haze of thousands of near-overlapping
dots — *zaspamovaná*. You cannot read where a species is, how big it is, that a brush edit happened, or
which sub-population it produced. The map fails as an information surface, and it is also the wall that
[[perf-bigger-maps-needs-structural-change]] hits: drawing every organism does not scale to bigger maps.

**Goals (acceptance bar for the epic):**

1. **De-spam.** Replace per-cell dot spam with a small number of legible **colony polygons** at the
   zoomed-out scopes (`O(#colonies)` draws, not `O(cells × 5)`).
2. **Unify a species.** Each colony is one polygon that visually groups a contiguous population —
   "a polygon that unifies a species." Every living organism belongs to **≥1 colony** by construction.
3. **Variations are first-class.** A CRISPR brush edit **mints a new colony** that peels off the parent
   species as a **nested district** (Cities-Skylines "split a city into districts"), and that district
   **keeps its identity** as its members disperse and reproduce.
4. **Pop open by size.** Each zoom scope "pops" selected / large-organism colonies open to individual
   organisms; tiny-microbe colonies stay aggregated until you are close enough that individuals are legible.
5. **Plants:** the most-realistic aggregate (reads as vegetation, not an abstract zone), **always visible**,
   and the first to pop.
6. **Determinism untouched.** The pinned hash `0x47a0_3c8f_6701_f240` stays byte-identical (inv #3); no
   biology in GDScript (inv #2). This is **not** a re-pin.
7. **Perf lever.** Colony aggregation is the LOD / data-layout structural change that lets bigger maps render.

**Non-goals (deferred):** time-based animated transitions (would break the no-per-frame-redraw rule —
§4); species-restricted brush coverage (the brush currently stamps all covered orgs — §3); a `u32`
variant-id space (the `u16`-in-`f32` discipline is load-bearing — §6).

---

## 2. The COLONY model — an off-hash render aggregation

### 2.1 The synthesis decision (which lens wins where, and why)

The three lenses agreed on the render path and on hash-neutrality; they diverged on **where colony
*identity* lives**:

- **Lens A / Lens B:** a **heritable, off-hash per-organism `Variant(u16)` tag** in `sim-core`, projected
  to a `dominant_variant_id` snapshot channel — modelled byte-for-byte on the existing off-hash `Species`
  tag + `dominant_species_id` channel.
- **Lens C:** **no new core tag** — derive colony identity in the renderer (or core) by connected-component
  labeling over `(dominant_species_id, allele_band(allele_freq))`; the brush "creates a colony" implicitly
  because it shifts the in-region genotype into a new allele band.

**We adopt the Lens A/B heritable `Variant(u16)` tag as the authoritative model**, and keep Lens C's
**render LOD contribution** (the footprint ladder, morph-aware hulls, crossfade, selected-pop cap, the
no-per-frame-redraw discipline). Rationale — the brief's wording is decisive:

- *"a CRISPR brush edit CREATES A NEW COLONY"* and *"keeps its identity … as it grows"* require a **stable
  lineage** that **follows organisms as they disperse and reproduce**. An allele-band grouping (Lens C)
  loses the district the moment the edited disc mixes spatially or drifts back into the parent's band — and
  natural per-step drift makes the band split/merge flicker. A heritable tag is immune to both.
- *"model it exactly like the existing off-hash `dominant_species_id` channel"* is a **per-cell dominant
  ordinal** projection, **not** a connected-component label. `dominant_species_id` carries the most-populous
  `SpeciesId` per cell; the faithful sibling is `dominant_variant_id` = the most-populous `Variant` per
  cell. **Connected-components (cells → polygon geometry) then lives in the renderer**, which is the clean
  inv #2 split: *core decides per-cell colony **identity** (a read-only projection of tags); renderer derives
  colony **geometry** (CC → contour → fill/label).*

Lens C's allele-band-CC-in-core is recorded as the rejected alternative in the ADR (§6) — it conflates
identity with geometry, puts presentation geometry in the core, and is flicker-prone.

### 2.2 The channel — `dominant_variant_id`, byte-for-byte on `dominant_species_id`

`dominant_species_id` is the 13th snapshot channel (GSS5): per render cell, the **most-populous
`SpeciesId` ordinal**, computed at `snapshot()` time by an **ordinal-sorted per-cell tally** (no `HashMap`,
lowest-id tiebreak), drawing **zero `SimRng`**, **never** folded into `hash_world`
(`snapshot.rs:46/97`, `lib.rs:2191-2257`). The colony channel is the same object one level finer:

```
Species : dominant_species_id  ::  Variant : dominant_variant_id
```

**Core additions (all off-hash):**

| Element | Modelled on | Off-hash because |
|---|---|---|
| `#[derive(Component)] Variant(u16)` on every organism, default `0` (the founding/wild-type colony of its species) | the existing off-hash `Species(SpeciesId)` (`lib.rs:646`) | not in the `hash_world` tuple (§2.4); assigned with zero `SimRng` |
| `#[derive(Resource)] NextVariantId(u16)` monotonic counter | `NextOrgId(u64)` (`lib.rs:540`) | minted by `+= 1`, zero `SimRng`; not a folded resource |
| `dominant_variant_id: Vec<f32>` — the 14th channel | `dominant_species_id` (`snapshot.rs:97`) | snapshot is downstream of the tick, never in `hash_world` |

**Channel mechanics (`snapshot.rs` + `snapshot()`):** bump magic `GSS5` → `GSS6` (`snapshot.rs:46`), bump
`CHANNEL_COUNT` 13 → 14 (`snapshot.rs:51`), **append** `dominant_variant_id` *after* `dominant_species_id`
(offsets 0..12 never reorder — the same append discipline every prior bump used). In `snapshot()`, add
`&Variant` to the existing query tuple (`lib.rs:2195`), keep a per-cell **ordinal-sorted
`Vec<(variant_id, count)>`** beside the species tally, emit the most-populous id with a lowest-id tiebreak,
write `f32::from(best_variant)`. `u16`-in-`f32` round-trips exactly; `0.0` = base/wild-type. The block is
line-for-line the `dominant_species_id` block (no RNG, no mutation, sorted not hashed).

**Inheritance.** A colony must survive reproduction: copy the parent's `Variant` to offspring exactly as
`Species` is copied today — add a `variant: u16` field to `ReproRow` (`lib.rs:1177`) and `Child`
(`lib.rs:1496`), populate it from the org's `Variant` in the canonical-order pass, and spawn
`Variant(c.variant)` alongside `Species(SpeciesId(c.species))` at `lib.rs:1586`. At reset, every org spawns
`Variant(0)`.

**Renderer parse:** `snapshot.gd` bumps `MAGIC` → `"GSS6"`, `channel_count` → 14, parses
`dominant_variant_id` last in `load_from` + `parse_bytes`, adds it to `_channels_complete`. The byte gate
`tools/check_godot_snapshot.sh` moves its assertion `channels=13` → `channels=14` (`:94`). **These land in
the same slice as the producer change** — a GSS6 producer with a GSS5 reader / a `channels=13` gate would
go RED in `tools/gate.sh` (exactly the coupling ADR-021 GSS4→GSS5 already managed).

### 2.3 inv #2 argument — no biology in render

The genotype→phenotype expression already ran in the Rust core. The renderer receives **inert per-cell
integers** (`dominant_species_id`, `dominant_variant_id`) plus existing channels (`density`, `fitness`,
`allele_freq`) and emits pixels. Connected-component labeling, marching-squares contouring, Douglas-Peucker
/ Chaikin smoothing, and the fill/outline/label are **pure presentation geometry** — the identical class of
work as the existing `dominant_species_id → SpeciesVisualMap → pixels` mapping in `organisms.gd`. No
genotype is read, no phenotype is computed, no biology decision is made in GDScript. Colony *display*
metadata (district name, color, lineage) lives in a renderer-side registry, never the sim, never the hash.

### 2.4 inv #3 argument — the pinned literal is unmoved (the airtight case)

`hash_world` (`lib.rs:3284-3312`) folds, per organism sorted by `OrgId`:
`OrgId, Energy, Biomass, Age, Genotype, DroughtTol, ThermalTol, Position`, plus the folded resources
(`Tick, GenomeRes, DrawCount, PoolStock, …`). **It does not fold `Species`** — verified against the tuple.
That is the working, shipped proof that a heritable, spawn-assigned, off-`SimRng` per-organism tag is
hash-neutral. `Variant` is the same kind of tag, so it is hash-neutral for the same reason.

| New element | In `hash_world`? | Draws `SimRng`? | Verdict |
|---|---|---|---|
| `Variant(u16)` component | No — the per-org tuple omits it, exactly as it omits `Species` | No — set in the existing covered/spawn loops | hash-neutral |
| `NextVariantId` resource | No — folded resources are `Tick/GenomeRes/DrawCount/PoolStock/…` only | No | hash-neutral |
| brush stamp in `apply_edit_region` | No tuple field changes; `DrawCount`/`final_word` unchanged | **No new draw** — a pure data write | hash-neutral |
| `dominant_variant_id` channel | Snapshot is **never** in `hash_world`; downstream of the tick | No | hash-neutral |
| adding a component to the archetype | `hash_world` **and** `snapshot` both **sort by `OrgId`** before folding/emitting, so archetype iteration order never reaches the hash | No | hash-neutral |

**Why the pinned literal stays byte-identical.** The pinned config is **single-species PLANT with no
edit**: it issues zero `ApplyEditRegion`, so `NextVariantId` never increments, every org stays `Variant(0)`,
`dominant_variant_id` is **uniformly `0.0`** (exactly as `dominant_species_id` is uniformly `0` for a
single-species run — the `snapshot_single_species_dominant_id_is_uniformly_zero` template), and
`run_headless().hash` is computed from a tuple that never saw `Variant`. The two pins at `lib.rs:3443`
and `lib.rs:3607` stay green **unchanged**. **This is not a re-pin.** If any implementation step would move
that literal → **STOP THE LINE**.

**Replay/journal byte-identity.** The variant id is **derived**, not journaled — it is a pure function of
the *order* of `ApplyEditRegion` events in the stream (each increments `NextVariantId` in deterministic
action order). `actions.ndjson` stays **byte-identical** (no new wire field), so replaying the same journal
re-mints identical ids at identical Tick positions → the snapshot byte-identity test still passes.

**Doc fix to land in the same slice:** the comment at `lib.rs:1708-1709` claims the `Species`/`OrgId`
components are "part of `hash_world`'s row tuple." That is **false for `Species`** (it is off-hash; only
`OrgId` is the sort key). Anyone reasoning about this design from that comment would wrongly conclude
`Variant` must be hashed. Correct the comment — the truth (`Species` is off-hash) is precisely what
licenses `Variant` to be off-hash. Comment-only, hash-neutral.

---

## 3. BRUSH-CREATES-COLONY — the "create district" verb

`Action::ApplyEditRegion(EditAction, RegionSpec)` **already exists** (`harness/src/lib.rs:118`), is
**already serde-journaled** (godot-sim pushes it; the harness replays `actions.ndjson`), and already
mutates **only** the covered organisms via `Simulation::apply_edit_region` (`lib.rs:3198`), whose covered
loop iterates `query::<(&OrgId, &Position, &mut Genotype)>()` and shifts the allele of every in-region org
with **no RNG draw and no `HashMap`** (`lib.rs:3217-3225`).

The colony bind is a **two-line extension of that existing covered loop** — no new action, no new serde
field, no new RNG draw:

```
// before the loop — mint one id (zero SimRng, exactly like NextOrgId minting):
let cid = next_variant.0; next_variant.0 = next_variant.0.wrapping_add(1);
// inside the loop — add &mut Variant to the query and stamp every covered org:
variant.0 = cid;
```

The species is now split into **colony 0** (untouched parent) + **colony `cid`** (the brushed disc) —
Cities-Skylines districts. Because `Variant` is heritable (§2.2), the district **keeps its identity** as
its members disperse and reproduce — a property a pure renderer region-stamp could never have. The brushed
disc's `dominant_variant_id` cells now differ from their neighbours → in the renderer they group into
their **own connected-component polygon nested inside the parent species' territory**, tinted by a bounded
intra-species hue shift (reuse the `ALLELE_HUE_SHIFT` idiom) so the district reads as **family, not a
foreign species**.

**Inoculation as a natural extension:** `region_inoculate` (`lib.rs:2578`) — a contaminant arriving — is
naturally a new colony; it can stamp the spawned orgs with a fresh `Variant` id from the same
`NextVariantId` counter (zero `SimRng`, same discipline). Optional, off-hash, in the same core slice.

**Survives replay (inv #3):** the bind is journaled-event-order-derived, not a journaled value
(§2.4) — replay re-mints identical district ids, so a recorded-then-replayed brushed run reproduces its
districts byte-for-byte. The edit stays a regional **operator** action (inv #6 — agents act at the
operator/species level); only its *display grouping* is new.

**Known scope limit (flag, not a blocker):** `apply_edit_region` currently stamps/shifts *all* in-region
orgs regardless of `EditAction.species` (the godot binding hardcodes `species:0`). The first cut stamps
exactly the covered set (colony = the brushed organisms) to stay byte-aligned with the existing
allele-shift. Species-restricted districts are a later, still-off-hash refinement (thread `edit.species`
into the covered filter — no RNG).

---

## 4. The LOD "POP" ladder + plants

### 4.1 The decisive metric — on-screen organism footprint

The pop trigger is the **on-screen organism footprint**, not the zoom scope alone:

```
footprint_px = _cell (world cell px) × cam.zoom.x × size_scale(species)
```

`size_scale` is the existing `SpeciesVisualMap` multiplier (`species_visual_map.gd:21-27`):
`SIZE_PLANT 2.2 · SIZE_MOLD 1.9 · SIZE_ROD 0.9 · SIZE_COCCUS 0.75 · SIZE_PLEOMORPH 0.6 · SIZE_VIBRIOID 0.5
· SIZE_SYMBIONT 0.34`. Because `size_scale` is **in the formula**, plant colonies cross the pop threshold
at a far lower zoom than microbe colonies — so **by organism size, the big things resolve first** and the
microbe haze stays aggregated. That is the brief's "by organism size, pop open" requirement, for free.

> **Wiring fix all three lenses flagged:** `organisms.gd`'s current LOD test (`_cell < LOD_MIN_CELL`,
> `:23/:186`) keys on the field-space `_cell`, **not** the effective on-screen cell. The pop ladder needs
> the effective on-screen cell (`_cell × cam.zoom × size_scale`) threaded from `main.gd._show` /
> `_set_zoom` into **both** `organisms.gd` and the new `colonies.gd`, so the two layers agree on when to pop.

### 4.2 The per-colony ladder (thresholds are tuning knobs, px on-screen)

| footprint | render | rationale |
|---|---|---|
| `< ~3 px` | **District polygon only** (fill + boundary stroke, species-tinted, labelled name·variant·count) | individuals would be sub-pixel spam → 1 polygon replaces up to `cells × 5` dots |
| `~3–7 px` | **Polygon + density stipple** (internal heat from the `density` channel) | read the shape + where it's dense, still no individuals |
| `≥ ~7 px` | **POP OPEN:** per-cell morph sprites via the **existing** `organisms.gd` `_draw_plant`/`_draw_morph`, clipped to the colony footprint; the polygon fades to a thin **district outline** | members visible, district frame retained |

- **Selected-colony pop:** click (or hover) a colony → it is forced to POP-OPEN regardless of zoom (one
  district "explodes" to its members) while neighbours stay aggregated. Implemented as a
  `_selected_colony_id` override of the footprint test, **capped to the viewport rect / a per-colony sprite
  budget** so a map-spanning species cannot re-spam.
- **Smooth transition (no per-frame timer):** crossfade across the 6–8 px band — polygon alpha ramps
  `1.0 → 0.15` while sprite alpha ramps `0 → 1`, both as a **pure function of footprint**. Zoom only
  changes on a wheel/scope event (each calls `queue_redraw`), so the transition is smooth on scroll
  **without** a per-frame timer — preserving `organisms.gd`'s "redraw only on state change, never per
  frame" discipline (inv #3 in the renderer). Time-based easing is explicitly **deferred** behind a
  separate "allow animated redraw" decision.

The pop-open level **reuses** `organisms.gd`'s morph glyphs untouched — `colonies.gd` owns only the
polygon/hull/label geometry (the single source of truth for morphology stays in one place).

### 4.3 Plants — always-visible, most-realistic, ≥1-colony

- **≥1 colony by construction:** every non-empty cell has a dominant `(species, variant)` → every plant
  lands in exactly one connected-component colony (singletons included). The guarantee is structural.
- **Always visible:** a plant colony's polygon gets a **minimum on-screen alpha + size floor** — a plant
  district never decays into background haze the way a sub-3 px microbe colony does, and never collapses
  below its fill+outline at any zoom. (Future-proofs the "plants always visible" requirement.)
- **Pops first:** with `size_scale 2.2`, plant colonies hit the ~7 px pop threshold at the lowest zoom —
  at Patch/Cells you already see individual L-system trees beside microbe colonies that are still solid
  polygons.
- **Most-realistic aggregate (morph-aware polygon):** plant colonies draw as a **soft canopy hull**
  (organic blobby green mass from the cell footprint, gradient by mean fitness/density), while microbe
  colonies draw as **hard-edged districts** (clean Cities-Skylines boundaries). The *aggregate* of a plant
  reads as vegetation, not an abstract zone — "plants most realistic" at every LOD.

---

## 5. The perf link — [[perf-bigger-maps-needs-structural-change]]

Colony aggregation is the **LOD / data-layout lever** that memory item demands. At Field scope the draw
count drops from `O(cells × MAX_DOTS_PER_CELL)` (thousands of dot primitives) to `O(#colonies)` (tens of
polygons) — the per-organism cost is paid **only** near the camera or on a selected, popped district, and
capped to the viewport. The colony-polygon count is **independent of map size** (bounded by the number of
distinct connected regions, not by `cells`), so bigger maps render at the zoomed-out scopes without
drawing every organism. (Precise scope: this is a **draw-primitive** reduction on the render side; the
snapshot transfer is still `W·H` per channel — colonies are the lever for the *draw* wall, which is the one
the spammed map hits first. Rayon compute-parallelism was measured not to pay, ADR-020.)

---

## 6. ADR-029 draft block

> Paste into `docs/llm/DECISIONS.md` when the channel slice (S1) lands. Append-only; ADR-028 is the
> current highest.

```
## ADR-029 — COLONIES: off-hash `dominant_variant_id` channel (GSS6) + heritable `Variant` tag,
              renderer-derived district polygons + size/zoom LOD pop

Status: Proposed (design pinned; S1 is the hash-touching slice — STOP-THE-LINE gate before merge).

Context:
  The play map draws up to MAX_DOTS_PER_CELL per non-empty cell → unreadable "spam" at Field scope, and
  the per-organism draw cost is the wall bigger maps hit (perf-bigger-maps-needs-structural-change). The
  core already ships an off-hash per-cell `dominant_species_id` projection (GSS5, ADR-021) of the off-hash
  `Species` tag. A "colony" is the same construction one level finer: group a contiguous population (incl.
  brush-created variations) into one Cities-Skylines district polygon.

Decision:
  1. Add an off-hash, heritable, spawn-assigned `Variant(u16)` component (default 0 = founding colony of
     the species), minted from a monotonic `NextVariantId` resource — modelled byte-for-byte on the
     off-hash `Species` tag + `NextOrgId`. Inherited by offspring exactly as `Species` is.
  2. Project it to a `dominant_variant_id` snapshot channel (GSS6: magic GSS5→GSS6, CHANNEL_COUNT 13→14,
     appended last) — the per-cell most-populous Variant ordinal, computed in snapshot() by an
     ordinal-sorted per-cell tally (no HashMap, lowest-id tiebreak, zero SimRng), exactly like
     `dominant_species_id`.
  3. A CRISPR brush (Action::ApplyEditRegion, already journaled) mints one fresh `Variant` id and stamps
     it on the covered organisms — a 2-line extension of the existing covered loop, no new action, no new
     wire field, no new RNG draw. The disc becomes a nested district; the district keeps its identity as
     members disperse/reproduce. (region_inoculate may stamp a fresh id too.)
  4. The renderer (new colonies.gd, sibling under organisms.gd) derives colony GEOMETRY: deterministic
     connected-components over (dominant_species_id, dominant_variant_id) → marching-squares/hull contour →
     fill + outline + label. A size×zoom footprint ladder pops selected/large colonies open to the existing
     organisms.gd morph glyphs; plants are always-visible, pop first, and render as a soft canopy hull.

Invariant audit:
  inv #2 — core decides per-cell colony IDENTITY (a read-only projection of tags); renderer derives GEOMETRY
           (CC/contour/label = presentation, not biology). No genotype→phenotype in GDScript.
  inv #3 — `Variant`/`NextVariantId`/`dominant_variant_id` are NOT in hash_world (which omits Species too);
           assigned with zero SimRng; snapshot is downstream of the tick; hash_world+snapshot sort by OrgId
           so archetype order never reaches the hash. The pinned single-species-plant config keeps all orgs
           Variant(0) → channel uniformly 0.0 → 0x47a0_3c8f_6701_f240 BYTE-IDENTICAL. NOT a re-pin.
           actions.ndjson stays byte-identical (ids are derived from event order, not journaled).
  inv #6 — the brush stays a regional operator action; only its display grouping is new.

Consequences:
  + de-spam (O(#colonies) draws vs O(cells×5)); the bigger-maps LOD draw lever.
  + brush edits are legible as nested districts that survive replay.
  - snapshot binary format bumps GSS5→GSS6 (render format, independent of hash_world); every 13-channel
    reader/gate moves to 14 in the same slice (snapshot.gd + check_godot_snapshot.sh).
  - u16 variant-id ceiling (65 535 brush edits/run) to preserve exact u16-in-f32 round-trip; per-run; documented.

Alternatives considered:
  - Lens C: derive colony_id by connected-components over (dominant_species_id, allele_band) with NO new
    core tag. REJECTED: loses district identity when the edited disc mixes/drifts; band thrashing flicker;
    conflates identity with geometry and puts presentation geometry in the core. (Kept its render LOD ideas.)
  - u32 variant id: REJECTED — silently loses precision in the f32 channel unless widened to two planes
    (overkill for a PoC).
  - Hold variants in a parallel off-hash Vec<u16> side-table instead of an ECS component: fallback only
    (fiddlier across births/deaths); the component is provably outside the hashed set, so it is preferred.
```

---

## 7. Slice breakdown

**Hash-risk legend:** ✅ hash-neutral (renderer / data only) · 🔁 deliberate re-pin · 🛑 STOP-THE-LINE
(touches the core/snapshot determinism path — must prove the literal unmoved at the gate before merge).

Each slice leaves the build green; one slice = one commit. The **core change is deliberately kept in a
single slice (S1)** so there is exactly **one** STOP-THE-LINE surface; S2–S6 are pure renderer.

### S1 — `colony-snapshot-channel-impl`  🛑  *(touches core+snapshot; expected ✅ hash-neutral — NOT a re-pin)*

Core + snapshot + the GDScript reader + the byte gate, as one cohesive unit:
- `Variant(u16)` component (default 0) + `NextVariantId(u16)` resource; spawn `Variant(0)` at reset.
- Inherit through `ReproRow` (`lib.rs:1177`) / `Child` (`lib.rs:1496`) / spawn (`lib.rs:1586`).
- Mint + stamp in `apply_edit_region` (`lib.rs:3198`) — the brush→colony bind (§3); optionally in
  `region_inoculate` (`lib.rs:2578`).
- `dominant_variant_id` as the GSS6 14th channel: magic `GSS5→GSS6`, `CHANNEL_COUNT 13→14`, appended last
  in `snapshot.rs`; per-cell ordinal-sorted tally in `snapshot()` (`lib.rs:2195-2257`).
- `snapshot.gd` GSS6 reader (`load_from`+`parse_bytes`+`_channels_complete`);
  `tools/check_godot_snapshot.sh` `channels=13 → 14`.
- Correct the `lib.rs:1708-1709` doc comment (`Species` is off-hash).

**Acceptance:**
- `tools/gate.sh` green, including the godot snapshot byte gate now asserting `channels=14` / GSS6.
- The two pins `0x47a0_3c8f_6701_f240` (`lib.rs:3443`, `:3607`) stay green **unchanged** (no re-pin).
- New tests (clone the `dominant_species_id` precedents): `dominant_variant_id` byte round-trip;
  single-species/no-edit → `dominant_variant_id` uniformly `0.0`; a brush mints a distinct in-region
  `dominant_variant_id` **while** `run_headless().hash` is byte-identical; replay of a brushed
  `actions.ndjson` reproduces identical district ids.
- **STOP-THE-LINE:** this is the only slice that *could* move the literal — halt at the gate for explicit
  human determinism verification before merge; if the literal moves, **stop, do not work around it.**

### S2 — `colony-polygon-render-impl`  ✅  *(renderer-only)*

New `colonies.gd` (`Node2D` sibling **under** `organisms.gd`): deterministic connected-components
(4-connectivity, two-pass union-find over a `width*height` int array, row-major, **no Dictionary/hash-order
iteration** — inv #3 in the renderer) over `(dominant_species_id, dominant_variant_id)`; marching-squares
contour + Douglas-Peucker + one Chaikin smoothing → fill (`draw_colored_polygon`, species color via
`SpeciesVisualMap.color_for`, value by mean fitness) + outline (`draw_polyline`, width by cell_count) +
centered label (glyph + pop count); a `MIN_COLONY_CELLS` haze floor for specks. Wired in `main.gd._show`
with `snap.dominant_variant_id` + `snap.dominant_species_id` + the visual table.

**Acceptance:** at Field scope the map renders as **polygons, not dots** (de-spam visible in a `--shot`);
single-species → 1 territory; two species → two; a brushed disc → a nested sub-polygon. No determinism
exposure (zero Rust).

### S3 — `lod-pop-impl`  ✅  *(renderer-only)*

Thread the effective on-screen cell / `cam.zoom` from `main.gd` (`_set_zoom`/`_set_scope`/`_show`) into both
`colonies.gd` and `organisms.gd`; implement the §4.2 footprint ladder (polygon → density stipple →
pop-to-sprites) with the 6–8 px crossfade as a pure function of footprint (no per-frame timer); gate
`organisms.gd`'s per-cell draw to popped colonies clipped to the footprint.

**Acceptance:** zooming in pops large colonies to individuals while microbe colonies stay polygons;
plants (`size 2.2`) pop first; redraw fires only on zoom/scope/state change (no per-frame redraw).

### S4 — `brush-colony-binding-impl` (render surface)  ✅  *(renderer-only; the core bind shipped in S1)*

Switch the colony key to `(species, variant)`; render the edited disc as a **nested district inside the
parent territory** with a bounded **intra-species hue shift** (`ALLELE_HUE_SHIFT` idiom); a renderer-side
colony registry `variant_id → {species, label, color, gen_created, parent}` assembled from
`observe_species()` + the already-journaled `ApplyEditRegion` edits; world-click → `set_selected_colony(id)`
for the selected-pop.

**Acceptance:** a brush stroke visibly **creates a new district** that reads as family (not a foreign
species) and **persists/moves** with its organisms across steps; click selects + pops a district.

### S5 — `plant-realism-impl`  ✅  *(renderer-only)*

Plant colonies: minimum on-screen alpha + size floor (always-visible, never collapse below fill+outline);
morph-aware aggregate (plant **canopy hull** vs microbe **hard district**); confirm the ≥1-colony guarantee
holds for every non-empty plant cell.

**Acceptance:** plant districts visible at every scope, read as vegetation, and pop to L-system trees
first; a `--shot` shows canopy hulls beside hard microbe districts.

### S6 *(optional)* — `colony-polish-impl`  ✅  *(renderer-only)*

Viewport-cull / per-colony sprite budget for selected-pop on map-spanning species; label declutter; a
district inspect panel reusing the saved-variant naming UI; big-map draw-count verification (the perf
lever).

**Acceptance:** a map-spanning species cannot re-spam when popped; measured draw count at Field scope is
`O(#colonies)`.

---

## 8. Risks (carried from the lenses)

1. **The offspring `Variant` copy must not perturb the hash path (load-bearing, S1).** Keep the copy a pure
   label assignment inside the existing canonical-order spawn/covered passes — never route it through an
   RNG-feeding or hashed pass. The determinism test + the STOP-THE-LINE gate catch any slip.
   Ultra-conservative fallback: a parallel off-hash `Vec<u16>` side-table instead of a component.
2. **Colony churn / flicker** as boundary cells flip dominant near edges (S2). Mitigate with the
   `MIN_COLONY_CELLS` floor + 4-connectivity (bounded count) + optional renderer-only contour hysteresis.
   The heritable tag (vs Lens C's allele band) already removes the worst flicker source.
3. **Variant-id `u16` ceiling** (65 535 edits/run) — required for exact `f32` round-trip; per-run;
   documented; reject `u32` (precision loss) for the PoC.
4. **Renderer non-determinism if `colonies.gd` iterates in Dictionary/hash order** — build all aggregates in
   a single row-major pass into arrays indexed by colony id; iterate ordered structures only.
5. **Selected-pop re-spam on a map-spanning colony** — cap popped sprites to the viewport rect / a
   per-colony budget (S6).
6. **Snapshot-format consumers** hard-coding 13 channels — only `snapshot.gd` + `check_godot_snapshot.sh`;
   both move to 14 in S1 (the GSS6 magic bump turns any stale reader into a loud bad-magic error).

---

### Critical files (for the implementer)

- `crates/sim-core/src/snapshot.rs` — `GSS5→GSS6`, `CHANNEL_COUNT 13→14`, `dominant_variant_id` channel +
  round-trip tests.
- `crates/sim-core/src/lib.rs` — `Variant` component + `NextVariantId` (model on `Species` :646 /
  `NextOrgId` :540); inheritance `ReproRow` :1177 / `Child` :1496 / spawn :1586; brush mint/stamp
  `apply_edit_region` :3198 (+ `region_inoculate` :2578); per-cell variant tally in `snapshot()`
  :2195-2257; the `hash_world` tuple :3284 (the off-hash proof — omits `Species`); pinned pins :3443 /
  :3607; doc-fix :1708.
- `crates/harness/src/lib.rs` — `Action::ApplyEditRegion` :118 (the already-journaled brush, unchanged on
  the wire).
- `godot/snapshot.gd` — GSS6 magic, `channel_count 14`, parse `dominant_variant_id`, `_channels_complete`.
- `godot/organisms.gd` — reuse `_draw_plant`/`_draw_morph` for popped colonies; LOD currently field-space
  (`:23/:186`) — thread the effective on-screen cell.
- `godot/main.gd` — `_show` feed, `SCOPES` :104, `_set_zoom`/`_set_scope` :2725; wire `cam.zoom` into the
  colony layer.
- `godot/species_visual_map.gd` — `SIZE_*` :21-27, `size_for`/`color_for` for the visual table + the
  footprint ladder.
- `tools/check_godot_snapshot.sh` — `channels=13 → 14` :94 (the snapshot-format byte gate).
- New: `godot/colonies.gd` — the colony polygon + LOD layer.
