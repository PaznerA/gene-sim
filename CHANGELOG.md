# Changelog

All notable changes per slice. One slice = one entry. Format loosely follows Keep a Changelog.

## [Unreleased]

### Starter-promote hardening — gen-1 `source_hash` recomputed from an edit-free replay — HASH-NEUTRAL, off-hash tooling (ADR-031 trap closed)
Closes the ADR-031 latent trap: `promote_gen1` used to copy `source_hash = hex16(gem.recorded_hash)` while dropping
the gem's edits — correct only while CRISPR edits stay hash-neutral; a gen-1 starter promoted from an *edited* gem
would silently stop replaying to its `source_hash` once edits become hash-active. Now `promote_gen1` **RECOMPUTES**
the gen-1 `source_hash` from an EDIT-FREE replay of the pristine `StarterConfig` (`build_journal(&[], gens)` =
`Advance(1)·gens` → `record_episode` → replay-verified `record == replay`, the existing deterministic contract — no
hand-rolled hash path), so the stored hash always equals what the edit-free config actually produces, whether or not
the source gem carried edits. `Gen1Starter` gains `gens` (the source horizon) + `source_had_edits: bool`, both
`#[serde(default)]` so the committed starter library still deserializes + the gallery stays green (the committed
docs are **not** re-promoted/rewritten — their source gems are gone from disk and they're already edit-free).
Off-hash harness tooling → pinned literal `0x47a0_3c8f_6701_f240` byte-identical (sim-core untouched; the recompute
runs the *source* config, never the pinned determinism config); `cargo tree -p harness` adds no dependency. Tests:
an edited-gem fixture stores the edit-free hash + flags `source_had_edits`, a gen-1 doc is self-contained
re-verifiable (re-run from `config`+`source_seed`+`gens` reproduces `source_hash`), and the committed library loads
under the new struct (11/11 promote tests). Gate GREEN (sim-core 187/187, determinism OK); 3-skeptic verify 3/3 on
all four invariant booleans. (ADR-031's "known trap" marked RESOLVED — no new ADR.)

### OVERSIGHT UI polish — safe q default + honest due-epoch label + ledger resumes after load — RENDERER-ONLY, zero Rust (ADR-028 follow-ups)
The three ADR-028 #3-verify follow-ups, all in `godot/main.gd` (renderer-only). (1) The growth-ratio `q` SpinBox now
**defaults to `1000` (wild-type / no-op)** instead of `0` (growth-lethal KO), so opening the OVERSIGHT panel and
committing without touching the knob is a no-op — the control default + both the preview and commit fallbacks agree
on `1000`. (2) The timeline marker + status labels now read **"applied now / effective epoch %d"** / **"committed
now … effective epoch %d"** instead of the old "due epoch N" wording that implied a deferral the renderer's
immediate-commit path never performs (the epoch reads as the effect/accounting epoch; `due_epoch` still sourced from
the core dict). (3) OVERSIGHT **resumes after `load_session`** — `_resync_to_live` re-activates the panel +
`_refresh_oversight_panel()` on both load paths (starter checkpoint + `_on_load_pressed`), `has_method`-guarded so an
older cdylib degrades gracefully — no dead/stale ledger on a loaded checkpoint. inv #2: GDScript marshals only
ints/strings; the credit economy / FBA→factor map stays a core `#[func]` read. **Zero Rust diff** → pinned literal
`0x47a0_3c8f_6701_f240` byte-identical by construction (the `q` knob is a UI default, not the headless determinism
config). Gate GREEN (sim-core 187/187, determinism OK); 3-skeptic verify 3/3 on all four invariant booleans; both
reviewers APPROVE. (ADR-028's follow-up bullet is now resolved — no new ADR required.)

### Discovery D3-B.3 — `RidgeInt` integer ridge regressor + pluggable `Surrogate` trait — HASH-NEUTRAL, zero f64 (ADR-034)
The surrogate model the steered loop (D3-B.4) will fit on the eval log to predict the drama target `D` (ADR-033).
Off-hash, pure-integer (`crates/discovery/src/surrogate.rs`). (1) **`Surrogate` trait** (inv #5 seam) — `fit(&mut,
x:&[FeatureVec], y:&[u64], seed)` / `predict(&FeatureVec)->u64` / `id()` / `min_samples()`, object-safe; impls swap
without touching search. (2) **`NullSurrogate`** — base case (`predict` 0, `min_samples()==usize::MAX` so a steered
run cold-starts to passthrough = byte-identical to `discover_evolved`). (3) **`RidgeInt`** — integer ridge LINEAR
regression: `θ` is `i64` on `THETA_SHIFT=16`, predict = pure-integer `(θ·x) >> THETA_SHIFT` clamped to `[0,SCALE]`;
fit **sorts the rows once** (`(y,features)` → row-order-independent via commutative i128 batch sums) then runs a
**pinned `N_ITERS=2000`** fixed loop of fixed-point gradient descent on the ridge MSE (i128 accumulators, data step
`/2^LR_SHIFT` `LR_SHIFT=11`, decoupled L2 decay `/2^RIDGE_LAMBDA_SHIFT` `RIDGE_LAMBDA_SHIFT=8`). **Zero f64** on train
or predict. Serde + `RIDGE_BUILD_ID="ridgeint-v1@dims28-shift16-iters2000"` self-invalidation anchor. Tests
(verified): deterministic + **row-order-independent** (reverse + coprime-stride permutations → identical θ),
**recovers a planted signal** (`D = a·predator×prey + b·temp-extremity + noise` → θ[16]/θ[27] within 20% of ideal,
held-out err<500), serde round-trip + build-id mismatch detection, NullSurrogate passthrough. **Not wired into the
search** (`discover_evolved`/`search.rs` untouched — that's D3-B.4). Hash-neutral: pinned literal
`0x47a0_3c8f_6701_f240` byte-identical (sim-core 187/187); `cargo tree -p discovery` stays `std`+`serde` (no
`ndarray`/`nalgebra`/`linfa`/heavy-ML; inv #5). Gate GREEN; 3-skeptic verify 3/3 on all four invariant booleans. The
upgrade path (`BoostStumpInt`, heavy ML at a subprocess boundary) stays behind the trait, deferred. **ADR-034.**
(Provisional, self-invalidating: `LR_SHIFT` global rate, spec open-question #4 — a real-eval-log convergence retune
is D3-B.4-adjacent.)

### Discovery D3-B.2 — the drama-weighted steering target `D` — HASH-NEUTRAL, off-hash integer (ADR-033)
The first brute-force batch showed **M1 (coexistence) saturates** → raw quality `Q` (≈46% weight on M1/M2) stops
separating *dramatic* runs from *placid* ones. New off-hash, pure-integer steering target the surrogate (D3-B.3/B.4)
will predict: a serde `DramaWeights` struct (`crates/discovery/src/surrogate.rs`, modelled on `ScoreParams`,
retune-without-code, `version`/`DRAMA_WEIGHTS_VERSION=1` re-pin anchor) with the **pinned default
`{w1=8, w2=4, w3=40, w4=8, w5=32}`** (sum 92; `w3+w5 = 72/92 = 78%` of the weight on dynamism M3 + events M5, vs ~46%
in Q), and `drama_target(breakdown:[u16;6], &DramaWeights) -> u64 = (Σ wᵢMᵢ for i∈1..5)/wsum · M6/SCALE` — exactly the
`Q` combine shape (`ecology.rs:70-71`) with the drama weights, pure integer (zero f64), no RNG, M6 the unchanged
multiplicative instant-death gate. **Clean steer/curate separation** (load-bearing): `Q`/`final_score`/`ScoreParams`/
gem curation are **unchanged** — `D` is a separate STEERING target; a test proves `D` ranks a dynamic run above a
placid-coexisting one where `Q` ranks them opposite. Encodes the standing memory `no-hardcoded-balance-open-system`
(steer toward living dynamics, not forced stability). Defines the target only — changes no search behaviour, not yet
wired into the loop (the steered sibling lands in D3-B.4; the behavioural steering sign-off is sequenced to D4 per
ADR-033). Hash-neutral: pinned literal `0x47a0_3c8f_6701_f240` byte-identical (the target reads the inert `[u16;6]`
breakdown; `discovery` has no `sim-core` dep); `cargo tree -p discovery` stays `std`+`serde` (inv #5). Gate GREEN
(discovery 82→90 tests incl. 8 new monotonicity/M6-gate/serde/Q-unchanged tests, sim-core 187/187, determinism OK);
3-skeptic verify 3/3 on all four invariant booleans. **ADR-033.**

### Colonies S6 — perf-lever verification + select-pop cull + district inspect + label declutter — RENDERER-ONLY, zero Rust (ADR-029)
Closes the colony epic with the perf story + UX polish. (1) **Perf-lever verification** (the headline) — a new
headless `godot/colony_s6_test.gd` builds the colony layer at Field scope on a 48² (2304 cells) and a 96² (9216
cells) grid and asserts the district draw count is **identical and bounded** (`48²=4, 96²=4`, ≤24) — `O(#colonies)`,
**independent of map size**: `PERF_LEVER_OK … old per-organism dots @96²=46080 → 11520× fewer draws`. This is the
structural draw-count win [[perf-bigger-maps-needs-structural-change]] called for, now under the gate. (2)
**Select-pop cull + budget hardening** — `organisms.gd._selected_pop_plan` caps a map-spanning selected colony at
`SELECTED_POP_BUDGET=700` AND viewport-culls (`world_rect.has_point`), with the budget meter growing only from
on-screen in-budget cells, so an off-screen cell never consumes budget nor re-spams (test: 9216-cell colony pops
exactly 700; a 60×60 window pops 25 / culls 9191 / budget_capped=0). (3) **District inspect panel** —
`_on_click → _show_colony_inspect → _fill_colony_inspect` renders `{species, label, variant, cells, gen_created,
parent}` into the **reused** `_detail_box`/`_detail_panel` saved-variant-naming idiom (no new panel, no biology — a
display-name lookup + inert cell count). (4) **Label declutter** — `colonies.gd._label_plan` keeps the selected +
above-`LABEL_MIN_CELLS` districts and de-overlaps by centroid (`LABEL_MIN_SEP_CELLS`), highest-priority-first
(deterministic total sort). inv #2: cull/inspect/declutter are presentation-only reads of built render state — no
genotype→phenotype. inv #3: ordered total sorts throughout (`_draw_order` iso-depth/x/y; `_label_plan`
selected/count/seq; registry keyed reads), no `randf`/`randi`/`Time`/`OS`, no `_process`/Timer. **Zero Rust diff** →
pinned literal `0x47a0_3c8f_6701_f240` byte-identical by construction; snapshot stays GSS6/channels=14. Gate GREEN
(sim-core 187/187, determinism OK; full godot snapshot gate green incl. COLONY S4/S5/S6, re-verified on the committed
tree); 3-skeptic verify 3/3 on all four invariant booleans. **→ ADR-029 colony epic COMPLETE (S1–S6).** (Non-blocking
note: the perf metric counts district entries, not the mid-zoom per-cell stipple/quad cost, which is inactive at the
Field scope where the O(#colonies) claim lives.)

### Colonies S5 — plant realism (always-visible canopy hulls + ≥1-colony guarantee) — RENDERER-ONLY, zero Rust (ADR-029)
Completes the visual-declutter epic. Plant colonies are now **always-visible** and **most-realistic**.
(1) **Always-visible floor** — a plant colony skips the sub-`MIN_COLONY_CELLS` haze-speck path
(`if count < MIN_COLONY_CELLS and not is_plant`), so it renders as a full district even at 1–2 cells, and never
collapses below fill+outline at any zoom: the plant full-pop ghost fill floors at `PLANT_GHOST_FILL_FACTOR=0.40`
(vs the microbe `GHOST_FILL_FACTOR=0.15`) with a `PLANT_MIN_OUTLINE_WIDTH=2.0` floor — the plant district frame
always reads. (2) **Canopy hull vs hard district** — plant colonies draw as a soft canopy hull: extra Chaikin
smoothing (`PLANT_CHAIKIN_PASSES=2`, with a triangle-collapse guard) + a radial green gradient fill
(`_draw_canopy_fill`, a pure per-vertex `Color.lerp`); microbe colonies keep the hard-edged single-Chaikin
Cities-Skylines district (`draw_colored_polygon`). No plant morphology is duplicated — `organisms.gd._draw_plant`
(the L-system canopy) is reused untouched for the popped trees; `colonies.gd` owns only the aggregate hull.
(3) **≥1-colony guarantee** — every non-empty plant cell lands in exactly one connected-component colony
(structural: the row-major union-find labels every non-empty cell). A new headless proof `godot/colony_s5_test.gd`
(wired into the snapshot gate) asserts FLOOR / CANOPY / COLONY_GUARANTEE / GHOST_FLOOR (verified locally:
`COLONY S5 OK`, plant_cells=37/0 unlabeled, plant 20 pts > microbe 10 pts, ghost 0.40>0.15). (4) Plants pop first to
L-system trees (already true via `SIZE_PLANT 2.2` in the S3 footprint). inv #2: `is_plant` is a keyed read of the
core species table — no genotype→phenotype in GDScript. inv #3: ordered passes only (no hash-order iteration), no
`randf`/`randi`/`Time`/`OS`, no `_process`/Timer. **Zero Rust diff** → pinned literal `0x47a0_3c8f_6701_f240`
byte-identical by construction; snapshot stays GSS6/channels=14. Gate GREEN (sim-core 187/187, determinism OK, the
full godot snapshot gate green incl. COLONY S4/S5). Verify 3/3 on the three feature booleans; the renderer-only
boolean's only open conjunct was "test committed" (resolved by staging `colony_s5_test.gd` — re-verified green on the
committed tree). (Deferred cosmetic: a brushed plant *parent* with holes takes the S4 frame path, skipping the canopy
gradient for that case — floor still holds.)

### Colonies S4 — brush→district render surface (nested family district + select-pop) — RENDERER-ONLY, zero Rust (ADR-029)
The CRISPR brush now reads as the Cities-Skylines "create district" verb (the core brush→`Variant` bind shipped in
S1; this is its render surface). (1) **Nested district** — closes the S2 hole-cut deferral: `_trace_boundaries`
returns the outer loop + interior holes and `_draw_holed_fill`/`_eliminate_holes` cut the brushed child region out of
the parent species fill so the parent renders as a *frame* around the child; the child draws with a bounded
intra-species hue shift (`_variant_hue_shift`, ±0.09 around the same species base hue, matched by `main.gd._family_color`)
so it reads as **family**, not a foreign species. The district key is `sid*65536 + heritable dominant_variant_id`, so
it tracks its members across steps (the heritable S1 tag — proven: centroid moved `(16.5,12.5)→(20.5,14.5)`).
(2) **Colony registry** — a renderer-side `variant_id → {species, label, color, gen_created, parent}` assembled from
`observe_species()` + the renderer's own journaled `ApplyEditRegion` strokes (`_resolve_pending_colonies`: the
core-minted child variant id is read from the inert `dominant_variant_id` plane via ordered `_pending_brush` +
sorted-key tally + row-major disc — no hash-order iteration); drives the district label ("Wheat v7 · 49") + a stable
family color. (3) **Select-pop** — world-click → `set_selected_colony(sid*65536+vid)` forces that district to pop
open regardless of zoom (overrides the S3 footprint rung for its cells) while neighbours stay aggregated, **capped**
to a viewport-rect clamp + a `SELECTED_POP_BUDGET=700` per-colony sprite budget (spent on visible cells, in a total
draw order) so a map-spanning selected colony cannot re-spam (S6 hardens the perf cap). A new headless code-level
proof `godot/colony_s4_test.gd` (wired into the snapshot gate) asserts HOLE_CUT / PERSIST / REGISTRY / SELECTION. inv
#2: registry/hue/hole-cut/selection are presentation only — no genotype→phenotype (the brush still calls the existing
`apply_edit_region` `#[func]`). inv #3: no `randf`/`randi`/`Time`/`OS` in any S4 addition, ordered iteration only, no
`_process`/Timer; the selected-pop viewport clamp is deterministic given camera state (stable for `--shot`). **Zero
Rust diff** → pinned literal `0x47a0_3c8f_6701_f240` byte-identical by construction; snapshot stays GSS6/channels=14.
Gate GREEN (sim-core 187/187, determinism OK, COLONY_S4_TEST_OK); 3-skeptic verify 3/3 on all four invariant
booleans; all reviewers APPROVE. (Closes the S2/S3 deferred cosmetics.)

### Colonies S3 — LOD pop ladder (zoom→footprint, plants pop first) — RENDERER-ONLY, zero Rust (ADR-029)
Replaces the binary scope-layer swap (Field=colonies XOR closer=organisms) with a per-colony LOD ladder keyed on the
**on-screen organism footprint** `footprint_px = _cell * cam.zoom.x * size_scale(species)` (§4.1 — fixes the wiring
bug where `organisms.gd` keyed its LOD on the raw field-space `_cell`). A new `set_zoom(zoom)` on both `colonies.gd`
and `organisms.gd` (each stores `_zoom` + `queue_redraw`, guarded by `is_equal_approx` so an unchanged zoom can't
thrash a redraw) is threaded from `main.gd._set_zoom`/`_update_scope_layers`. Per colony the rung is a **closed-form
pure function of footprint** (`pop_t = clampf((foot-POP_LO)/(POP_HI-POP_LO),0,1)`): `<6 px` district polygon only;
mid-band polygon + row-major density stipple; `≥8 px` POP OPEN to per-cell morph sprites (the existing
`organisms.gd` `_draw_plant`/`_draw_morph`, reused untouched — single source of truth for morphology) while the
polygon crossfades to a thin outline (fill alpha `1.0→0.15`, sprite alpha `0→1` over the same 6–8 px band). Because
`size_scale` is **in** the footprint, plant colonies (`SIZE_PLANT 2.2`) cross the pop threshold at a lower zoom than
microbes → "by organism size, pop open" for free; un-popped microbe cells emit **zero** sprites (`if pop_t <= 0.0:
continue`), so the de-spam holds. **No per-frame redraw**: neither layer defines `_process`/Timer; `queue_redraw`
fires only on `set_snapshot`/`set_zoom`/`set_iso`/scope change (time-based easing stays deferred behind a future
"allow animated redraw" decision — inv #3 renderer discipline). **Zero Rust diff** → pinned literal
`0x47a0_3c8f_6701_f240` byte-identical by construction; snapshot stays GSS6/channels=14. inv #2: the ladder computes
only footprint/alpha — no genotype→phenotype. Gate GREEN (sim-core 187/187, determinism OK); 3-skeptic verify 3/3 on
all four invariant booleans; all reviewers APPROVE. (Non-blocking nits for later cleanup: dead `LOD_MIN_CELL` const;
a degenerate-path fallback-default mismatch.)

### Colonies S2 — `colonies.gd` district polygons (the visible de-spam) — RENDERER-ONLY, zero Rust (ADR-029)
The first VISIBLE colony slice: a new `godot/colonies.gd` (a `Node2D` colony layer) turns the spammed dot map into
readable district polygons at Field scope. Deterministic connected-components (4-connectivity, two-pass union-find
over a single row-major `width*height` `PackedInt32Array` — **no Dictionary/hash-order iteration**, inv #3 in the
renderer; union-by-smaller-root so labelling is order-independent) over the per-cell key
`species_id*65536 + variant_id` from the GSS6 snapshot (`dominant_species_id` + `dominant_variant_id`). Per colony:
marching-squares boundary trace → Douglas-Peucker simplify → one Chaikin smoothing → `draw_colored_polygon` fill
(species color via `SpeciesVisualMap.color_for`, brightness by mean fitness, bounded intra-species hue shift per
variant) + `draw_polyline` outline (width by cell_count) + a centered glyph+population label; a `MIN_COLONY_CELLS`
haze floor for specks (anti-flicker). `main.gd._update_scope_layers` swaps layers by zoom: at Field scope
(`zoom.x < 1.8`) it shows the colony layer and HIDES the per-organism dot layer (the `MAX_DOTS_PER_CELL` spam); the
closer scopes keep `organisms.gd` (S3 adds the LOD pop crossfade). Guards a pre-GSS6 snapshot (treats variant as
all-0) so nothing crashes. **Zero Rust diff** → pinned literal `0x47a0_3c8f_6701_f240` byte-identical by
construction; snapshot format stays GSS6/channels=14 (S1, unchanged). inv #2: geometry only — reads the two off-hash
identity channels + the visual table, computes no genotype→phenotype. Gate GREEN (sim-core 187/187, determinism OK);
de-spam proven both ways (a Field-scope `--shot` renders one district polygon, Patch-scope shows individual sprites =
layer swap works; + a green headless render-scene smoke over a real 14-channel snapshot). 3-skeptic verify 3/3 on all
four invariant booleans. (Deferred to S4: hole-cutting for nested districts; a brushed-disc `--shot`.)

### Colonies S1 — off-hash `Variant` tag + `dominant_variant_id` GSS6 channel + the brush→district bind — HASH-NEUTRAL (ADR-029)
The single core/snapshot slice of the colony epic (the 🛑 STOP-THE-LINE slice, human-signed-off; S2–S6 are
renderer-only and build on it). `crates/sim-core`: a heritable, spawn-assigned `Variant(u16)` component (default 0 =
founding colony of the species) minted from a monotonic `NextVariantId` resource — modelled byte-for-byte on the
off-hash `Species` tag + `NextOrgId`, inherited through `ReproRow`/`Child`/spawn exactly as `Species` is (zero
SimRng). `apply_edit_region` mints one fresh id and stamps it on every covered organism — the Cities-Skylines
"create district" bind, a 2-line extension of the existing covered loop with **no new action, no new wire field, no
new RNG draw**. A `dominant_variant_id` snapshot channel (snapshot format magic GSS5→GSS6, `CHANNEL_COUNT` 13→14,
appended LAST so offsets 0..12 never reorder): the per-cell most-populous Variant ordinal, computed by an
ordinal-sorted per-cell tally (no HashMap, lowest-id tiebreak, zero SimRng) — line-for-line the `dominant_species_id`
block. `godot/snapshot.gd` reads GSS6/14 (dynamic channel_count, plane appended last); `tools/check_godot_snapshot.sh`
+ `livesim_smoke.gd` move to channels=14/GSS6 in the same slice (no stale 13-channel reader). The hash-neutrality is
airtight: `hash_world` OMITS `Species` (the off-hash proof) and sorts by `OrgId`, so a spawn-assigned off-SimRng tag
never reaches the hash — the pinned single-species-plant config issues zero `ApplyEditRegion` → every org stays
`Variant(0)` → channel uniformly 0.0 → pinned literal `0x47a0_3c8f_6701_f240` **BYTE-IDENTICAL** (NOT a re-pin; both
pins green unchanged at `lib.rs:3544`/`:3708`; `actions.ndjson` byte-identical, ids derived from event order). Tests:
u16-in-f32 byte round-trip, single-species/no-edit → uniformly 0.0, brush mints a DISTINCT in-region
`dominant_variant_id` WHILE `run_headless().hash` is byte-identical to the no-brush run, replay reproduces identical
district ids. 187/187 sim-core determinism tests pass. Corrected the stale `lib.rs` doc comment (Species is off-hash).
Gate GREEN; 3-skeptic verify 3/3 on all five invariant booleans.

### Scenario GIF preview — CAPTURE + ASSEMBLE on the off-hash key-event schedule — HASH-NEUTRAL
The renderer-side half of the scenario preview (the schedule itself is `harness::keyframe`, ADR-032). `--keyframes
<gem>` prints the off-hash KEY generations to snapshot (boom/crash/takeover/edit/immigrate + start/context/final
anchors). `tools/make_starter_gif.sh` REPLAYS the gem (the discovery-load-gem-replay loader, incl. its mid-run
edits) and shoots ONE renderer `--shot` PNG per key gen — **macOS-safe** (timeout + FILE capture, never a `$(godot…)`
pipe that hangs; WINDOWED since `--shot` needs a GPU; SKIPs cleanly on a no-display box). A minimal renderer hook
(`--gem --shot --steps N` advances the loaded gem N gens firing due edits before the shot — renderer-only, no biology
in GDScript). `--assemble-gif` then encodes the frames into a looping animated GIF with the **in-process MIT `gif`
encoder** (`crates/harness/src/gifenc.rs`; `png` decode + `color_quant` quantize; pinned `gif = 0.13`, `png = 0.17`
— GPL stays at the process boundary, inv #1) at `data/presets/starters/<slug>.gif`, next to the starter so
`gallery.gd` shows it (it already reads `res://data/presets/starters/<slug>.gif`); staged by the existing recursive
`cp -R data/presets/.` + byte-gate. The `.gif` is a generated artifact (gitignored, never committed). Off-hash
(`--keyframes` runs only the hash-neutral trace capture; the encoder is inert PNG post-processing) → pinned literal
`0x47a0_3c8f_6701_f240` unmoved (sim-core 184/184, harness 101/101 + 5 new `gifenc` tests). Headless smoke asserts a
valid >1-frame GIF without a GPU; the full pipeline was run end-to-end (6 real `--shot` frames → a 480×404 looping
GIF). Gate GREEN.

### Starter-map library — committed gen-1 + gen-N starters from the auto-research + an RCT selector — HASH-NEUTRAL
The capstone of the discovery → playable-content loop (ADR-031). A `promote` tool (`crates/harness/src/promote.rs` +
`--promote-gem`/`--promote-default-set` CLI) turns a curated gem into a **committed** starter under
`data/presets/starters/`: **gen-1** (`<slug>.json`, a fresh config + `source_hash` provenance, edits dropped) or
**gen-N checkpoint** (`<slug>/`, the gem replayed to gen N via `record_episode` so the scheduled edits sit in the
session journal — a developed state scrubbable BACK through its interventions, loaded via `load_session`;
round-trip-verified before write). 7 starters shipped (6 gen-1 across the dynamics taxonomy + 1 `branch-point`
checkpoint). An RCT-style scenario selector (`godot/gallery.gd`): a left list + a big right description + an animation
preview (the GIF if present, else a live replay) + a thick scrub slider. Meta-level promote + renderer-only gallery →
pinned literal `0x47a0_3c8f_6701_f240` unmoved. Gate GREEN; 3-skeptic verify CONFIRMED (5/5 at 3/3). QUEUE item #4.
*(Follow-up tracked `starter-promote-hardening`: enforce edit-free gen-1 promotion / recompute its source_hash + store
`gens` — the gen-1-drops-edits provenance is safe today only because edits are hash-neutral.)*

### Discovery load-gem-replay — watch a discovered scenario live (renderer + read-only core resolver) — HASH-NEUTRAL
A "💎 Load Gem" picker reconstructs + plays a saved gem live: `reset(master_seed)` + roster keys → `set_roster` (via
`res://data/species`, the Load Starter path) + `temp_q`/season → `set_environment` + containment → `set_containment`,
then fires the gem's CRISPR edits at their generations so the discovered (possibly edited) scenario plays out
(ADR-030). The edit resolution lives in CORE — a read-only `godot-sim` `gem_edit_schedule(gem_json)` `#[func]` that
**reuses `edits_to_actions`** (`loci[edit.target % loci.len()].id`, `gen_abs = edit.gen * gens_requested / 65536`,
`species_index → SpeciesId`) — so the renderer replays byte-faithfully to what the search scored; no biology in
GDScript (inv #2). `Gem.gens_requested` is serialized off-hash (`#[serde(default)]`) so early-stopped gems use the
search horizon; old gems fall back to `gem.gens`. Pinned literal `0x47a0_3c8f_6701_f240` unmoved. (The first
renderer-only attempt was RED — it resolved edits in GDScript and diverged from `edits_to_actions` [81/147 edits
failed]; the 3-skeptic verify caught it, the v2 core-resolver fix is CONFIRMED 4/4 at 3/3.) QUEUE item #3.

### Discovery continue-from-gem — branch the search from a discovered gem (harness) — HASH-NEUTRAL
`discover_from_gem(gem_path, …)` + a `--from-gem <path>` CLI flag: load a saved gem JSON → pre-seed a fresh
evolutionary `GemLibrary` from its config (the gem becomes the gen-0 elite the mutate/crossover pool branches off,
optionally with new edits via `edit_budget`) so the auto-research keeps developing the discovered community — the
"continuation after -X gens" ask. Every continued gem is round-trip-verified (`record_episode → replay ==
recorded_hash`) by the unchanged `verify_and_write_library`; a stale source gem (build_id mismatch) is logged + used
as an anchor but dropped at write time, so no irreproducible gem reaches disk. Meta-level (std/serde + splitmix
meta-RNG, no SimRng) → pinned literal `0x47a0_3c8f_6701_f240` unmoved. Gate GREEN; 3-skeptic verify CONFIRMED (4/4 at
3/3). QUEUE item #2 (scenarios arc).

### Discovery scenario starters — named `SearchSpace` presets + `--space` CLI (discovery + harness) — HASH-NEUTRAL
Six named `SearchSpace` scenario presets (predator-prey / decomposer / contamination-open / spore-resilience /
edit-rescue / extreme-climate) that bias the brute-force search toward a drama type (per-species `include_bp` + count
ranges + containment/temp/season + `edit_budget`), behind `SearchSpace::scenario(name)` + a `--space <name>` CLI flag.
The default/absent path is byte-identical (golden-pinned `default_space_propose_is_byte_identical_golden`); a named
space is a meta-level alternative search config (std+serde, no SimRng) → pinned literal `0x47a0_3c8f_6701_f240`
unmoved. Unknown name degrades to the default space with a note (no panic). The 7-axis order is fixed + test-guarded
(`scenario_spaces_keep_the_fixed_seven_axis_order`). Gate GREEN; 3-skeptic verify CONFIRMED (4/4 at 3/3). QUEUE item
#1 (scenarios arc — the "more starters" ask, grounded in the wave-1+2 research).

### Codex browse panel — scrollable in-game species/gene/role/flow browser (renderer) — HASH-NEUTRAL
A 4th "Codex" VIEW (full-window, modelled on the relations/specimen views) over `res://data/codex/codex.json` via the
EXISTING `godot/codex.gd` loader (no inline JSON parse) — a left `ItemList` (declared order) + a case-insensitive
filter + a right scrollable detail pane (taxonomy / ontology / GO-SO ids / trophic role / traits + flow descriptions).
Reachable via the top-right VIEW switcher, the `V` cycle, and `--view codex`. Renderer-only (inv #2: static
`codex.gd` lookups, zero biology in GDScript); zero Rust → pinned literal `0x47a0_3c8f_6701_f240` unmoved. Degrade
guarded ("Codex unavailable" / "No matches"). Gate GREEN (`CODEX MIRROR OK` / `CODEX INSPECT OK`); 3-skeptic verify
CONFIRMED (4/4 claims at 3/3). QUEUE item #4 (gameplay/sandbox lead).

### OVERSIGHT in-game UI — earn → preview → commit E. coli edits (godot-sim + renderer) — HASH-NEUTRAL
The player-agency payoff (ADR-017 S4/S5/S6 surface, ADR-028): surfaces the earned-credit OVERSIGHT loop in `--live`.
- `godot-sim` thin marshalling `#[func]`s — `oversight_state` / `preview_ecoli_edit` (read-only) / `commit_ecoli_edit`;
  the economy/biology stays in harness/core (inv #2: GDScript moves only ints + a `VarDictionary`).
- A commit journals `RequestEcoliEdit` (zero SimRng) + `CommitEcoliImpact` (reads a committed int) — replay-reproducible;
  `due_epoch` is a generation count (no wall-clock leak). Pinned literal `0x47a0_3c8f_6701_f240` unmoved on a no-commit
  run (`oversight_plumbing_is_hash_neutral`); a committed edit moves the hash DELIBERATELY + replays byte-equal
  (`renderer_committed_edit_is_replay_equal`).
- Renderer panel (modelled on the CRISPR intervention panel): the credit ledger + request/preview(FBA-KO)/commit +
  timeline markers. The renderer applies the commit immediately (vs the headless firewall's `due_epoch` deferral —
  both deterministic + replay-equal; the divergence is recorded in ADR-028 + a `oversight-ui-polish` follow-up).
- Gate GREEN; 3-skeptic verify CONFIRMED (5/5 claims at 3/3). ADR-028. QUEUE item #3 (gameplay/sandbox lead).

### Variant Lab D — the auto-research mid-run-EDIT search axis (discovery + harness) — HASH-NEUTRAL
The brute-force discovery search can now propose mid-run CRISPR edits, not just initial configs (ADR-027) — so an
edited lineage can be discovered + saved as a replayable gem (the user: the auto-research must ALSO get the edit action).
- `SearchConfig.edits: Vec<EditGene>` (LAST field, `#[serde(default, skip_serializing_if = "Vec::is_empty")]`) + an
  `edit_budget` SearchSpace knob **defaulting to 0**; `draw_edits` returns empty BEFORE drawing when budget==0, so the
  default search + every existing discovery test + the eval-log bytes stay byte-identical (the edit draws use a disjoint
  `EDIT_SALT` stream at field indices after season; q16 span-independent gen encoding).
- `harness::discover::edits_to_actions` maps each `EditGene` onto the EXISTING `Action::ApplyEdit` (no new sim action;
  genotype→phenotype stays in core, inv #2/#6); `verify_and_write_library` rebuilds the journal to MATCH
  `capture_trace`'s per-gen interleave so an edited gem round-trips (`replay==recorded==gem.recorded_hash`) or is dropped.
  `--edit-budget N` CLI flag opts the axis in.
- Pinned literal `0x47a0_3c8f_6701_f240` unmoved (`edit_budget_zero_is_byte_identical_to_the_no_edit_search`). Gate
  GREEN; 3-skeptic verify CONFIRMED (5/5 claims at 3/3). ADR-027. QUEUE item #2 (Variant Lab epic, gameplay/sandbox lead).

### Variant Lab B+C — save→name→reseed loop (renderer + read-only core export) — HASH-NEUTRAL
The player can save a roster species' CURRENT (post-edit) genome as a named variant and reseed it like a contaminant.
- **Slice B (save):** read-only `Simulation::export_species_spec(sid) -> SpeciesSpec` (`&self`, zero SimRng, never
  folded into `hash_world`) carrying the niche — `entity_count`, `trophic_role` (via the new exhaustive
  `gp::role_to_str`, the proven inverse of `role_from_str`), and `host_key`; `godot-sim`
  `export_species_json(species_id) -> GString`; a specimen-view "💾 Save variant" action storing
  `{name, json, key, species_id, role, traits}` in a renderer `_saved_variants` registry (the JSON is opaque text —
  never parsed in GDScript, inv #2).
- **Slice C (reseed):** a "Saved variants" section (mirrors the contaminant consortium menu) that registers a variant
  via the EXISTING `register_contaminant_json` + arms it for the `inoculate`/`TOOL_INOCULATE` brush — no new core action
  (manual inoculation works at any containment, ADR-019).
- Round-trip test (`export_species_json_round_trips_to_the_live_phenotype`): export → `build_species_from_str` → a
  `BuiltSpecies` whose expressed phenotype + role + host match the live species. Pinned literal
  `0x47a0_3c8f_6701_f240` unmoved (`export_species_json_is_hash_neutral_and_guarded`). Gate GREEN; 3-skeptic verify
  CONFIRMED (5/5 claims at 3/3). From the QUEUE gameplay/sandbox lead (item #1).

### Roadmap loop infra — `/roadmap-plan` + `/roadmap-iterate` skills + the workflow queue (tooling/docs) — HASH-NEUTRAL
A two-skill loop over the prepared `.claude/workflows/*.js` orchestrations, one tier above the existing per-slice
`/iterate` (one queue item = one multi-agent Workflow = one merge). State lives in `docs/llm/QUEUE.md` + git → resumable.
- **`docs/llm/QUEUE.md`** — the workflow zásobník: an ordered table (status/driver/goal/hash-risk/deps) + per-entry detail.
  Seeded with the **gameplay/sandbox** lead (`[[gameplay-sandbox-first]]`): LICENSE → variant-lab-save-reseed →
  oversight-ingame-ui → codex-browse-panel → sandbox-load-starter → live-session-save-load, with the discovery/ML chain
  + the beta-hardening remainder defined behind it. ≥5 forward items maintained.
- **`/roadmap-plan`** — surveys the roadmap + the real frontier (trust git, not the prose), keeps ≥5 robustly-defined
  workflows queued, authoring/refreshing the `.js` in house style. Plans only — never production code.
- **`/roadmap-iterate`** — pops the next ready item → `Workflow({name})` → gate GREEN + 3-skeptic verify CONFIRMED →
  merge to `main` (`--no-ff` temp-worktree, on the LOCAL gate per `[[no-ci-wait-autonomous-roadmap]]`) → mark `[x]`.
  Autonomous until red / 🛑 / empty; recommends `/roadmap-plan` when depth `< 3`.
- Tracked `variant-lab-save-reseed.js` (was untracked) + authored `oversight-ingame-ui-impl.js` → 2 immediately-runnable.

### LICENSE — dual `LICENSE-MIT` + `LICENSE-APACHE` at repo root (docs) — beta-distribution blocker
Materializes the `MIT OR Apache-2.0` SPDX already declared in the workspace `Cargo.toml` (GitHub showed "No license";
SPDX alone is insufficient for distribution). Apache-2.0 copied verbatim from the canonical toolchain text. From the
`glmTakeover/` audit (B0.1).

### PERF-2 follow-up — golden-hash pins on the predator/symbiont byte-paths (test, sim-core) — HASH-NEUTRAL
Closes the ADR-026 coverage caveat. PERF-2 converted the predation/host_coupling OrgId-keyed maps/sets
(`pred_credit`/`symb_credit`, the `prey_debit`/`host_debit` struct maps, the `despawn_set`s) to sorted-`Vec`s, but
the plant-only `0x47a0_3c8f_6701_f240` config early-returns out of those kernels, so the literal never locked them —
they rested on construction-equivalence + the run==run `f6_`/`s5_` tests. This adds two GOLDEN-literal pins that DO
exercise those paths, so any future byte-drift fails CI:
- **`predation_roster_hash_is_pinned`** → `0xd4eb_7676_531f_b2bf` (the f6 3-species predator roster: plant + decomposer
  + vigorous Bdellovibrio, seed 57 / 50 gens / 600).
- **`host_coupling_roster_hash_is_pinned`** → `0xf723_26af_466e_bb64` (the s5 inoculate→couple run: plant +
  Carsonella symbiont via `register_symbiont` + `region_inoculate`, seed 47).
- Both are NEW pins on NEW configs → **hash-neutral to `0x47a0…`** (test-only, no sim-logic change); 182/182 sim-core.

### PERF-2 — per-tick OrgId-keyed `BTreeMap`/`BTreeSet` → reused sorted-`Vec` (perf, sim-core) — HASH-NEUTRAL
ADR-026. The hot path built a fistful of OrgId-keyed `BTreeMap`s + `BTreeSet`s fresh EVERY tick over the whole living
set; profiling noted the `items`/`rows` vectors are already sorted by `(cell, species, OrgId)`, so each map is
reproducible from a sorted `Vec`. Swapped them all for REUSED sorted-`Vec` scratch buffers — **the pinned literal
`0x47a0_3c8f_6701_f240` is byte-identical** (full `tools/gate.sh` GREEN, 180/180 sim-core incl. `--features
determinism`); adversarially verified (the CODE) on every dimension. Rebased onto + composed with PERF-1 (both land together; the compose is byte-identical — the `determinism` gate stays `0x47a0`).
- **Helpers (lib.rs):** `sort_merge_org_i64` (sort + sum-merge dup keys == `entry().or_insert(0)+=v`) and `org_lookup`
  (`binary_search` == `BTreeMap::get`). The reusable pattern for any future OrgId-keyed collect/apply map.
- **Sites:** `by_org`/`maint_energy`/`parent_debit` (lib.rs), `spent` (chem.rs, genuine kin+alarm dup-sum),
  `pred_credit`/`symb_credit` + the `prey_debit`/`host_debit` struct maps (`PreyDebit{eaten,dead}`/`HostDebit{drawn}`,
  three-phase build→get_mut→get preserved) + the two `despawn_set`s + `dead_set` membership sets (sorted `Vec` +
  `binary_search` == `BTreeSet::contains`); `litterfall`/`toxin_mints` stay collect-then-iterate (lookup-free). Two
  new scratch resources `PredationScratch`/`HostCouplingScratch` (trophic.rs), registered in `Simulation::new`.
- **Result:** tick_loop **−48 %** — re-benched BACK-TO-BACK on the same machine after rebasing onto PERF-1 (criterion
  `--baseline`): marginal **−47.4 % / −48.9 % / −47.8 %** (p < 0.05) at 1 k / 5 k / 10 k = 32.2 / 151.8 / 308.2 ms vs
  PERF-1's own 61.4 / 297.3 / 590.9 ms (~1.6 M updates/s, ≈1.9×). PERF-1's scratch-Vec hoist was itself perf-neutral on
  this bench (it eliminated allocations off the critical path), so the −48 % is genuinely PERF-2's marginal gain. The
  zero-lookup position-indexed `Vec` is NOT achievable for the maps applied via the arbitrary-order `q.iter_mut()` ECS
  query — `binary_search` is the correct ceiling there.
- **Follow-up:** the plant-only pinned config early-returns out of predation/host_coupling, so a golden-hash pin on a
  predator/symbiont roster would lock those byte-paths in CI (today covered by the green `f6_`/`s5_` determinism tests).

### PERF-1 — hoist metabolism/mineralize scratch Vecs into reused resources (perf, sim-core) — HASH-NEUTRAL
The first perf slice: eliminate per-tick `Vec` allocations in the two heaviest systems (`metabolism` +
`mineralize`) by hoisting their scratch buffers into reused Bevy resources (the `MetabolismScratch` /
`ReproScratch` / `ChemEmitScratch` discipline already proven on the F5 allocation sweep). **The pinned literal
`0x47a0_3c8f_6701_f240` is byte-identical** (the buffers are cleared + refilled each tick — never carried
state, never folded into `hash_world`). Full `tools/gate.sh` GREEN; the `determinism_hash_is_pinned` test PASS.
- **`MetabolismScratch`** gains 6 reused buffers: `weights`/`shares`/`rem_scratch` (Pass-2 apportion) +
  `split`/`split_w`/`split_rem` (Pass-3 convert-split). Previously allocated fresh `Vec::new()` per tick.
- **`MineralizeScratch`** (NEW resource): `rows` + `frozen_detritus` (replaces the per-tick
  `pools.detritus.clone()` — a full 1024-i64 plane copy eliminated) + `demand`/`granted`/`weights`/`shares`/
  `rem_scratch`/`split`/`split_w`/`split_rem`. All previously allocated fresh or cloned per tick.
- **Next (PERF-2):** replace the 10 OrgId-keyed `BTreeMap`s in the hot path (`by_org`, `maint_energy`,
  `parent_debit`, `spent`, `litterfall`, `toxin_mints`, `prey_debit`, `pred_credit`, `host_debit`,
  `symb_credit`) with reused sorted-`Vec`/indexed buffers — the deferred F1-pattern perf win
  (DECISIONS.md:1097-1100).

### Emergent-discovery D3-B.1 — the surrogate feature encoder (feat, discovery) — HASH-NEUTRAL
D3-B.1 (first sub-slice of D3-B from `docs/llm/proposals/surrogate-model-spec.md` §"Feature encoding").
The pure integer feature encoder the D3 `RidgeInt` surrogate will train on. **The pinned literal
`0x47a0_3c8f_6701_f240` is byte-identical** (the encoder is OFF-HASH: a pure function of
`SearchConfig`/`SearchSpace` numbers; no `SimRng`/`hash_world` touched). Full `tools/gate.sh` GREEN; reviewer
APPROVE on every invariant.
- **`crates/discovery::surrogate`** (new module, still std+serde): `encode(cfg, space) -> FeatureVec([i32; 28])`
  with a PINNED layout (guarded by `ENCODER_ID = "encode-v1@28"`): `[0]` bias · `[1..=7]` presence bit per
  species axis · `[8..=14]` normalized count per axis (bp) · `[15]` richness · `[16]` predator×prey
  (AND-gated bdellovibrio × prey-share) · `[17]` autotroph share · `[18..=21]` containment one-hot ·
  `[22..=25]` season one-hot · `[26]` temp · `[27]` temp-extremity. `master_seed` is EXCLUDED (entropy, not
  steerable — two configs differing only in seed encode identically).
- 24 tests pin the layout, the bounds, the `master_seed` exclusion, the determinism (byte-identical for the
  same `(cfg, space)`), and the serde round-trip.
- **Next (D3-B.2):** `DramaWeights` + the drama target `D`; then D3-B.3 the `RidgeInt` model + `Surrogate`
  trait; then D3-B.4 the `discover_evolved_steered` loop.

### Emergent-discovery D3-A — the eval log prerequisite (feat, discovery) — HASH-NEUTRAL
D3-A (the prerequisite for the D3 surrogate model — `docs/llm/proposals/surrogate-model-spec.md` §D3-A). The discover
loop previously saved only the top-K *kept* gems; the surrogate needs ALL evaluations. This adds the
`(config → ScoreVec)` evaluation record + a `--save-evals` CLI flag that writes every evaluated config to a
byte-reproducible JSONL log. **The pinned literal `0x47a0_3c8f_6701_f240` is byte-identical** (the eval log is
OFF-HASH: read-only over already-computed gem fields; no `SimRng`/`hash_world` touched). Full `tools/gate.sh` GREEN;
reviewer APPROVE on every invariant.
- **`crates/discovery::search`** (still std+serde): `EvalRecord { config, quality, breakdown, fingerprint,
  recorded_hash }` — mirrors `Gem` minus the novelty/score/caption/build_id/gens fields (the raw training row the
  surrogate trains on). Re-exported from the crate root. 2 tests pin the serde round-trip + the declaration-order
  JSON shape the surrogate trains on.
- **`crates/harness::discover`**: `capture_and_consider` now pushes an `EvalRecord` onto a caller-provided `Vec`
  BEFORE `lib.consider` (in EVALUATION ORDER — trial order for `discover`, gen×individual order for
  `discover_evolved`). `write_eval_log` emits one JSON per line (serde declaration-order → byte-stable). 4 tests
  including byte-reproducibility per `search_seed` (the spec contract) + the `None`-path writes no log.
- **CLI:** `--save-evals` → `data/runs/evals/<search_seed:016x>.jsonl` (gitignored; deterministic per seed).
- **Next (D3-B):** the `RidgeInt` surrogate + the `discover_evolved_steered` loop (oversample→predict→pre-filter).

### Variant Lab A — per-species CRISPR edit (whole-species inject targets ANY roster species) (feat, core/renderer) — HASH-NEUTRAL
The whole-species CRISPR inject (and the journaled `Action::ApplyEdit`) now target a CHOSEN species, not just the
resident primary — the foundation for the edit→save-variant→reseed loop (Variant Lab) + the auto-research's mid-run
edit space. **The pinned literal `0x47a0_3c8f_6701_f240` is byte-identical** (full `tools/gate.sh` GREEN, 180/180
sim-core); adversarially verified 5/5 on every dimension.
- **Core:** `EditAction` gains `species: u16` with `#[serde(default)]` (absent → 0 = primary), resolved in the env's
  `Action::ApplyEdit` via the same `species:u16 → SpeciesId` boundary the SP-3 interventions use. A species-0 edit is
  PROVEN byte-identical to the legacy hook (`species0_edit_via_targeted_hook_is_byte_identical_to_legacy_hook`); the
  RNG threading is unchanged (a non-primary edit draws the same words); an out-of-range species id is panic-free.
  `#[serde(default)]` keeps OLD journals (no field → species 0) + the recorded-episode golden + the R2 save/load
  round-trip byte-identical (no journal-format break). No new ADR — it extends the SP-3 `species:u16` + serde-default
  patterns.
- **Boundary:** godot-sim `apply_edit(cas, target, guide, species)` gains the `species:i64` param (mirroring
  `pcr_amplify`/`cull`).
- **UI:** the CRISPR inject panel gains a target-species picker (`_crispr_species`, populated from `observe_species`
  like `_pcr_species`); `_on_inject_pressed` passes the chosen species (default the active/primary), and the appended
  specimen variant attributes to the EDITED species.
- **Next (Variant Lab):** B save-named-variant (`export_species_json` via `from_built`), C reseed (register +
  inoculate), D auto-research scheduled edits (then D3 surrogate can steer over edits).

### Emergent-discovery D2b — widened search space + the evolutionary proposer (feat, discovery) — HASH-NEUTRAL
ADR-025. D2a clustered (the narrow Primordial space kept ~1 distinct gem); D2b widens the space + adds an
evolutionary proposer so the search surfaces a DIVERSE gem library. **The pinned literal `0x47a0_3c8f_6701_f240` is
UNTOUCHED** (meta-level search, no sim-path change; a dedicated test asserts it). Full `tools/gate.sh` GREEN;
adversarially verified 3/3 on every dimension.
- **`crates/discovery::search`** (still std+serde, no `rand`): `SearchSpace::default` widened to 7 free-living
  species with a per-species PRESENCE knob (`include_bp` — rosters differ in species MIX, not just counts) + broader
  count/temp ranges; deterministic std-only `mutate`/`crossover`/`propose_evolved` operators (salted splitmix64;
  `ensure_autotroph` keeps every config non-empty + in-bounds).
- **`crates/harness::discover_evolved`** + `--evolve-gens G` / `--pop-size P` CLI: gen 0 random → keep top-K → each
  generation proposes a new population (25% fresh-explore + 75% mutate/crossover of the elites) folded into the
  `GemLibrary`. Gems still written only after the `record_episode → replay == recorded_hash` round-trip. `G=0`
  reduces exactly to the D2a random `discover`.
- **Diversity win pinned by a test:** `evolutionary_keeps_more_distinct_gems_than_same_budget_random` (matched budget,
  STRICT `evo_distinct > rnd_distinct`, same widened space → the win is the explore/exploit machinery). 32 discovery
  tests + 6 harness `discover_evolved` tests (determinism, round-trip, diversity, the pinned-literal guard).
- **Next:** D3 surrogate model; D4 night-batch + showcase. At D3/D4 scale, a behind-the-boundary sqlite-vec gem-index
  sidecar (the ADR-014 pattern — derived, rebuildable from the JSON gems) is the trigger.

### Relations FULL-WINDOW view + always-on top-right VIEW+SCOPE switcher (feat, renderer) — HASH-NEUTRAL
Two UI reworks (renderer-only `godot/main.gd`). **ZERO Rust touched → pinned literal `0x47a0_3c8f_6701_f240`
byte-identical** (full `tools/gate.sh` GREEN). Adversarially verified 3/3 on every dimension.
- **Relations view is now FULL-WINDOW** (like the specimen view): the node-link graph + the heatmap render in a
  full-rect `_relations_full` Control that fills the field area between the title bar and the timeline, gated visible
  in `VIEW_RELATIONS` like `_specimen_root` — not a cramped panel on a black screen. The Graph/Matrix toggle + flow
  summary + legend + nearest moved to a compact floating card; the full container + graph/heatmap are
  `MOUSE_FILTER_IGNORE` so the card still receives the toggle clicks. The heatmap/graph `_draw` already size to their
  rect, so both scale to the window.
- **Always-on top-right VIEW + SCOPE panel:** a segmented `Ecosystem / Specimen / Relations` switcher with
  `Field / Patch / Cells` below it, anchored top-right and **separated from the CONTROLS deck** (which shed the old
  single cycling view button + the scope buttons). `_set_view_mode → _sync_view_buttons()` keeps the switcher in step
  with the `KEY_V` cycle / the `--view` shot flag / a button press; `KEY_1/2/3` scope shortcuts intact. The per-view
  top-right cards (INTERVENE/CONTAMINATION/SPECIMEN/RELATIONS) shifted down (y=176) to clear the always-on panel.

### Emergent-discovery D2a — the random-search GEM loop (propose → run → score → save replayable gems) (feat, discovery) — HASH-NEUTRAL
ADR-024. Makes the D0 scorer + D1 trace actually PRODUCE gems: the autonomous "find the dramatic runs" loop. **The
pinned literal `0x47a0_3c8f_6701_f240` is UNTOUCHED** (the search adds no sim-path change; the proposal RNG is a
meta-level splitmix, distinct from the sim `ChaCha8Rng`; a dedicated test asserts the literal is unmoved). Gate GREEN;
adversarially verified 5/5 (std+serde, sim-hash-untouched, gems-round-trip, search-deterministic, novelty-dedup-real).
- **`crates/discovery::search`** (still std+serde ONLY): `SearchConfig` + a `SearchSpace` pinning the Primordial
  proposal ranges + a std-only DETERMINISTIC `propose()` sampler (splitmix64 + Lemire range — no `rand` crate) + a
  `Gem` record (config + score + fingerprint + `recorded_hash` + `build_id` + an integer-derived caption) + a
  `GemLibrary` keeping top-K by score and rejecting near-duplicates via integer `novelty_l1`.
- **`crates/harness::discover`** (`discover(...)` + a `--discover --trials N --keep K --search-seed S` CLI): builds each
  config, runs `capture_trace`, scores via `DefaultScorer`, keeps the top-K novel, and writes each gem to
  `data/runs/gems/<score>-<seed>.json` **only after** a `record_episode → replay == recorded_hash` round-trip (a failed
  round-trip is dropped). `data/runs/*` is gitignored. `BUILD_ID` anchors every gem to the pinned sim hash (a re-pin
  self-invalidates stored scores). 6 discovery search tests + 6 harness discover tests (determinism, round-trip,
  novelty-dedup, the pinned-literal-unmoved guard).
- **Next (D2b):** widen the search space (broader count ranges / species mixes / scheduled mid-run edits) + the
  evolutionary proposer (the Primordial space currently clusters → ~1 distinct gem). Then D3 surrogate, D4 showcase.

### Emergent-discovery D0 scorer + D1 trace — `crates/discovery` + harness capture seam (feat, discovery) — HASH-NEUTRAL
ADR-023. The first phase of the autonomous emergent-run discovery epic: a reproducible INTERESTINGNESS SCORER + the
per-generation trace it reads. **ZERO sim hash impact → pinned literal `0x47a0_3c8f_6701_f240` byte-identical**
(capture READS only `observe_all()`/`flow_matrix()`, both proven zero-`SimRng`/off-`hash_world`; a real predator/prey
run scored both ways asserts captured-hash == plain-hash; full `tools/gate.sh` GREEN). Adversarially verified 3/3 on
every dimension; the metric set was pinned by a 3-lens design panel.
- **`crates/discovery`** (NEW, std + serde ONLY — no sim-core/harness dep, GPL-clean): a `PerGenTrace` it is handed →
  an integer interestingness score. Six basis-point, RNG-free metrics over the stable window — M1 coexistence, M2
  integer-Simpson evenness, M3 amp+turns **dynamism** (single-boom-capped), M4 FlowMatrix-aggregate trophic structure,
  M5 saturating **events** (booms/crashes/takeovers/established-immigrations), M6 a **multiplicative survival gate**
  (anti-instant-death, does NOT penalize end-state extinction). Combined `Q=(ΣWᵢmᵢ/86)·m6 ∈ [0,1_000_000]`. The
  `InterestingnessScorer` trait (pluggable, inv #5), `DefaultScorer` (`"ecology-d0"`), a 12-dim fingerprint +
  `novelty_l1` + `final_score`. Weights `[14,14,22,18,18]` favour drama over forced stability (memory:
  `no-hardcoded-balance-open-system`); all thresholds live in a tunable `ScoreParams` (ADR-pinned). The only `f64` is
  the fenced `q16` capture quantization; no RNG, no HashMap-iteration; `ScoreVec` is `Eq` (byte-reproducible).
- **`crates/harness/src/capture.rs`** (`capture_trace`): the D1 seam — drives a live `GeneSimEnv` into a `PerGenTrace`
  off-hash. The harness owns the engine touch so `discovery` stays clean (`harness → discovery` is the only new edge).
- **Test oracle:** a 7-archetype synthetic contract + a real grounded run. Live limit-cycle **A=784,500** STRICTLY
  beats frozen coexistence **F=355,000** (a 429k margin) — formally encoding "a living system beats a tuned-stable
  one". Instant-death / monoculture / single-boom all score ~0. 12 discovery tests + the harness hash-neutrality test.
- **Next (D2+):** the gradient-free→evolutionary search loop + the gem library, the surrogate model, the night-batch
  showcase gallery — anchored on the Primordial starter.

### Ecosystem map — per-cell MORPHOTYPE glyphs at the Cells scope (feat, renderer) — HASH-NEUTRAL
Completes the ADR-021 follow-up (per-zoom-scope refinement). The map sized + coloured each cell by its dominant
species but drew them all as the SAME rod (the primary species' template) → microbe cells read as uniform coloured
dots. Now each non-plant cell draws its dominant species' MORPHOTYPE glyph — the field-scale echo of the specimen
view. **ZERO Rust touched → pinned literal `0x47a0_3c8f_6701_f240` byte-identical** (full `tools/gate.sh` GREEN).
Adversarially verified 3/3 clean (zero issues) on every dimension.
- `godot/species_visual_map.gd` `build_table` now carries `morph` (the `morph_for` lookup) per species id.
- `godot/organisms.gd` `_draw_morph` dispatches a non-plant cell to one of 5 new field-scale glyphs — **cocci**
  (staph grape-cluster), **vibrioid** (Bdellovibrio comma), **pleomorph** (Mycoplasma blob), **symbiont**
  (Carsonella/Syn3 speck), **mold** (Aspergillus/Penicillium filament tuft) — plus the existing **rod** for
  E. coli/Bacillus. All trait-free `draw_circle`/`draw_line`/`draw_polyline` primitives (no triangulation trap),
  jittered only by `_hash01` (inv #3 byte-identical), modulated by the already-expressed fitness/density.
- **Per-zoom refinement:** the glyphs draw ONLY in the Cells-scope branch (`_sprites_on and not lod_dots_only`);
  the Field scope still falls to the sized colored `_draw_dot` (Field = density dots, Cells = morphotype community).
  Pure presentation (inv #2): morphotype is a per-species lookup, never computed in GDScript.
- Visually verified by a multi-species `--zoom` shot: large tan mold tufts, green plant canopies, magenta
  vibrioid/cocci microbe clusters, a blue E. coli rod — distinct shapes + the plant/mold ≫ microbe size hierarchy.

### Relations node-link GRAPH (default) + `--roster` / `--steps` shot flags (feat, renderer/tooling) — HASH-NEUTRAL
ADR-022. The Relations view shipped only the S×S FlowMatrix heatmap; users expected a node-link GRAPH of the
trophic web. **ZERO Rust touched → pinned literal `0x47a0_3c8f_6701_f240` byte-identical** (full `tools/gate.sh`
GREEN; godot `channels=13`/`glyphs=13`/`codex=OK`). Adversarially verified 3/3 on every dimension.
- **`godot/relations_graph.gd`** (NEW) — species as ring-laid NODES (radius ∝ √population, colour via the shared
  `species_visual_map.gd` morphotype table so the graph + field agree), EDGES = the core-MEASURED FlowMatrix net
  joule flows drawn source→sink (arrowhead at the gainer, thickness/opacity ∝ |J|/max-abs), oriented exactly like
  `_format_flow_summary`. Pure projection of core exports (inv #2); the only arithmetic is display scaling + ring
  layout. Degrades to nodes-only on a degenerate matrix; file-replay mirrors the heatmap placeholder.
- **`🕸 Graph / ▦ Matrix` toggle** in the Relations panel — Graph is the DEFAULT representation (the user's
  expectation); both read the same measured data. Fed by `_refresh_relations` from `observe_species()`
  (names/keys/roles/population in SpeciesId order = FlowMatrix index order).
- **`--roster "stem:count,…"` + `--steps N` shot flags** — the headless/`--shot` paths were single-species
  (`--species`); these arm a MULTI-species roster (via the existing `_apply_roster`, BEFORE `_do_reset` — the
  load-bearing seed-once order) + advance N gens so the ecosystem develops. Lets `--shot` render a real
  multi-species map + the relations graph with measured predator/prey flows (also unblocks map size-contrast
  verification). Opt-in → the no-flag pinned config hash is untouched.
- Visually verified by a multi-species shot: plant→E.coli (thick detritus) + E.coli→Bdellovibrio (thin predation)
  edges matching the narrated Primary-flows line, nodes sized by population + colored by morphotype.

### Specimen testing-unblock — inject button + brush→variant + extinct-struck-through + Load Starter (feat, renderer/tooling) — HASH-NEUTRAL
Three presentation/gameplay quick-wins that unblock manual testing of the specimen view. **ZERO Rust touched →
pinned literal `0x47a0_3c8f_6701_f240` byte-identical** (`determinism_hash_is_pinned` + reproducible-at-pinned-config
green; full `tools/gate.sh` GREEN; godot UI gate `channels=13` / `glyphs=13` / `codex=OK`). All on the read-only
side of inv #2 — pure projections of core exports (observe_species phenotype, GSS5 `dominant_species_id`,
`species_key`, `population_size`) into pixels.
- **Item 1 — explicit `💉 Inject (whole species)` button** (`godot/main.gd` `_build_crispr_params`): the
  whole-species CRISPR inject (the only edit that appends a new specimen variant) used to fire ONLY on Enter in the
  Guide field — undiscoverable. Now a labelled button in the CRISPR sub-panel calls the same `_on_inject_pressed`.
- **Item 2 — brush stroke surfaces a variant + extinct = struck-through-but-kept**: a region CRISPR brush now
  force-appends a `region edit N` variant to the dominant species at the painted cell (`_dominant_species_at` reads
  the GSS5 plane; `_append_edit_variant_for` generalizes the old whole-species path). A species whose population
  crashes to 0 is tracked (`_ever_alive`/`_extinct` in `_poll_population_alerts`, un-struck on spore regermination)
  and rendered struck-through + greyed (`✟ … — extinct` via a `[s]` RichTextLabel + dimmed glyph) — KEPT in the
  grid for investigation, never removed.
- **Item 3 — `📂 Load Starter — "Primordial Soil"`** (`godot/main_menu.gd`): reads `res://data/presets/primordial.json`
  → prefills the roster rows + env (seed/lat/lon/temp/season) + containment level, so a legible multi-species map is
  one click away. Preset staged into the res:// mirror (`run.sh`, `tools/check_godot_snapshot.sh` byte-equality gate,
  `release.yml` PCK + .deb).
- **Tooling — macOS-robust godot gate**: `tools/check_godot_snapshot.sh` captured godot via `OUT="$(godot …)"`, which
  hangs forever on macOS (a headless-godot child keeps the stdout pipe open after exit). Switched to a `timeout` +
  file-capture `run_godot` helper — Linux CI unaffected, local gating no longer hangs.
- Adversarially verified by a 3-skeptic workflow (3/3 on no-biology-in-GDScript, zero-Rust/hash-neutral, graceful
  degrade, UX-faithful); one skeptic independently caught the same GDScript Variant-inference parse error the godot
  load did (fixed: `var i: int`), proving the verify pass.

### GSS5 — ecosystem-map species visualization: per-cell `dominant_species_id` snapshot channel + per-species sizing (feat, sim-core/godot) — HASH-NEUTRAL
ADR-021. The map sized every organism the same → unusable on a multi-species roster. Added a per-cell
`dominant_species_id` channel to `GridSnapshot` (most-populous species per cell, sorted-Vec argmax, no HashMap),
`SNAPSHOT_MAGIC` GSS4→GSS5 / `CHANNEL_COUNT` 12→13, every GSS reader updated (snapshot.gd, livesim_smoke.gd,
check_godot_snapshot.sh). Renderer: `species_visual_map.gd` (size/color per species on a real cell-scale —
plant ≫ rod ≫ predator ≫ symbiont) + organisms.gd sizes/colors each cell by its dominant species. **HASH-NEUTRAL**
— the snapshot is off `hash_world` + draws no `SimRng`; pinned literal `0x47a0_3c8f_6701_f240` byte-identical
(178 sim-core tests green incl. the single-species-uniform-zero + multi-species-argmax asserts).

### PAR-S0 — Deterministic parallelization scaffold: rayon pinned dep + persistent pool + threshold + escape hatch (feat, sim-core/build) — HASH-NEUTRAL
The S0 slice of the parallelization epic (`docs/llm/proposals/parallel-sim-draft.md`, now COMMITTED; ADR-020).
**ZERO call sites yet → pinned literal `0x47a0_3c8f_6701_f240` BYTE-IDENTICAL** (the parallel region does not yet
exist; `determinism_hash_is_pinned` + `species_signatures_export_is_hash_neutral` green; `check_determinism.sh` OK).
- **rayon pinned (inv #7)** — `rayon = "1.12"` (→ `1.12.0`, `Cargo.lock` locked; transitive `rayon-core 1.13.0` +
  `crossbeam-{deque,epoch,utils}` + `either`) in `[workspace.dependencies]`, wired into `crates/sim-core/Cargo.toml`.
  All MIT/Apache-2.0 — inv #1's process boundary is GPL-ONLY, so linking rayon into the sim binary is fine;
  `oracle-slim` untouched.
- **Persistent global pool (NEVER per-tick)** — `crates/sim-core/src/par.rs`: an `OnceLock<rayon::ThreadPool>`
  built EXACTLY ONCE (`par::pool()`), pinned worker count (`RAYON_NUM_THREADS` else `DEFAULT_NUM_THREADS = 10`,
  for stable benches; correctness is schedule-independent). `par::run(op)` = `pool().install(op)` is the helper
  every future call site invokes.
- **`PAR_THRESHOLD = 2000`** — bench-tuned sequential cutoff; below it a heavy system runs its proven serial loop
  verbatim (the pinned ~1k config stays serial = an extra byte-identity guarantee).
- **`--no-parallel` escape hatch** — env var `GENESIM_NO_PARALLEL` (`par::force_serial()`, cached) forces the
  serial path for differential debugging; the result is byte-identical either way.
- **Determinism contract documented in-module** (compute-parallel / apply-canonical, RNG-free + disjoint-cell +
  associative-commutative i64 reductions). 5 new `par::tests` (174 sim-core tests green). The built-but-unused
  scaffold is warning-free (`#[allow(dead_code)]` on `run` + exercised by tests); fmt + clippy clean.

### SP-4 — Specimen view upgrade: evidence-driven morphology + rich inspect + codex (feat, renderer/content) — HASH-NEUTRAL
The specimen view becomes a real per-species encyclopedia. **Pinned literal `0x47a0_3c8f_6701_f240` unchanged**
(all-RENDERER + CONTENT on the read-only side of inv #2/#3; the one core touch is a purely-additive off-hash export).
- **Evidence-based morphology** — `godot/glyph_factory.gd`: a key-led `MORPH_BY_KEY` table (role-fallback for an
  un-tabled key) dispatches each of the 12 baked species to a morphotype. `godot/microbe.gd` GENERALIZED from the
  E. coli rod into rod / coccus / vibrioid / wall-less with `shape`/`curvature`/`flagella_layout`/`biofilm`/
  `endospore` params (E. coli peritrichous rod · Bdellovibrio comma w/ sheathed polar flagellum · staph
  grape-cluster cocci · Bacillus rod + refractile endospore · Cutibacterium short non-motile rod · Pseudomonas
  rod + polar flagella + biofilm halo · Mycoplasma wall-less pleomorph · Carsonella/Syn3 symbiont speck w/
  host-containment ring + SymbiosisCapacity tether). NEW `godot/mold.gd` — hyphal mycelium + conidiophore
  (Aspergillus globose vesicle / Penicillium brush), conidia density driven by SporulationCapacity. The plant
  L-system is unchanged; `_render_specimens` rewired to `GlyphFactory.make()` with adaptive per-glyph-bounds
  spacing + per-morphotype chrome emoji (🦠/🍄/🫧/🔬/🌱). All geometry precomputed in build() (inv #4 / #3).
- **Rich INSPECT card** — `_fill_specimen_detail`: a 6-section card (header + codex blurb + genome loci with
  anchors-first + traits-with-gloss + trophic role + gene-anchors/edit-lineage) reading the FOCUSED species.
  **FIXES the confirmed live-mode bug** (the old genome block read loci only from `_specimens.genome.loci` = the
  file-replay plant, so --live showed zero/wrong loci) **and** the title-only specimen pin (`_fill_detail(label,[])`).
  Lazy codex tooltip one-liners on hovered specimens.
- **Trait-set bug fix** — `PredationCapacity`/`SporulationCapacity`/`SymbiosisCapacity` added to `TRAIT_KEY_MAP`
  (silently dropped before) + per-morphotype `_active_trait_keys()`, so predator/spore-former/symbiont rows render.
- **SP-4 codex** — `data/codex/codex.json` (committed source of truth; format_version 1, 12 species + 12 genes +
  6 roles + 4 flows, keyed on the real ids: species `key`, gene `go`/`so`, role `role_from_str` ids, flow
  from/to roles). `godot/codex.gd` ordered-array loader (graceful `{}`). Joined in the inspect card + tooltips.
- **Core export widening (hash-neutral, additive)** — `LiveSim::loci()` now also marshals `so_term` + `go_refs`
  from the already-loaded Genome (the `{id,name}` fields + order UNCHANGED), unblocking the live-mode ontology join.
- **res:// STAGING FIX (the SP-4 blocker)** — `run.sh`, `tools/check_godot_snapshot.sh` (byte-equality
  `CODEX MIRROR OK` guard + a headless `--check` that BUILDS every species' glyph + exercises the codex inspect
  join → `glyphs=13`, `codex=OK`), and `release.yml` (BOTH exports now stage species **and** codex into the PCK
  before `--export-release`, closing the pre-existing species-PCK hole, + beside-binary copies). The original
  SP-4 RED (parse error + unstaged JSON) cannot recur — verified: all 5 GDScripts parse clean headless.

### ADR-019 S5 — obligate-symbiont mode (feat, Mode B, 🔁 RE-PIN pending; sim-core/genome) — likely HASH-NEUTRAL
The first Mode-B obligate symbiont: a host-dependent endosymbiont that **cannot free-live** and earns its joules
ONLY by drawing kept-J from a co-located host. Emergent host-dependence, NOT a forced equilibrium (§0.6).
- **New role** `gp::TrophicRole::ObligateSymbiont` (APPENDED after `Predator` → existing discriminants
  unperturbed). The "cannot free-live" guarantee is STRUCTURAL + FREE: a new variant falls THROUGH all three
  `metabolism` abiotic taps (gated on `Autotroph|Heterotroph|Mixotroph|Decomposer`) → taps no abiotic channel, no
  metabolism edit needed. Declared as DATA (`niche.trophic_role: "symbiont"`) resolved by `role_from_str`/
  `role_from_override`; key defaults `"carsonella"`/`"syn3"`.
- **Host-coupling kernel** `trophic::host_coupling` — a per-cell, RNG-free, `(cell,SpeciesId,OrgId)`-ordered,
  integer/`fixed::apportion` paired-conserved transfer modeled on `predation`: frozen start-of-tick HOST census →
  Monod demand on `host_draw_rate·body·edit` → host debited Energy-first-then-Biomass, symbiont credited
  `kept = drawn·7/10`, the tax → `respired`; records a MEASURED `flow[symbiont][host]` off-diagonal (rows still
  sum to 0). V1 = the host→symbiont DRAW arm only (benign-low net draw; bidirectional credit-back is an S5b
  stretch). Pinned schedule slot: immediately BEFORE `predation`, both on independently-frozen snapshots → a clean
  one-tick-lag "host dies → symbiont starves" cascade. `Strategy.host_draw_rate: u16` (gene-anchored on new
  `Trait::SymbiosisCapacity`, NOT in `Trait::ALL`) — inert `0` for every non-symbiont (the predation_rate precedent).
- **Host-required inoculation gate** (`region_inoculate`): a symbiont ESTABLISHES only where its declared host is
  co-located in the disc (else a clean no-op — the `region_cull`/`region_pcr` no-template precedent), placed ON
  host-occupied cells. **Structural cull-immunity** (`region_cull`): a role-only categorical guard — a generic
  antibiotic CANNOT clear an endosymbiont (the forced counter-play is to cull the HOST). **Airborne block**
  (`immigration::expand_schedule`): a symbiont key is HARD-FILTERED from any airborne schedule (Mode B, not Mode A).
- **Data** (real provenance, build+round-trip tested): `scripts/bake_carsonella_species.py` → `carsonella.json`
  (*Ca.* Carsonella ruddii Pv, RefSeq GCF_000010365.1, curated translation core + amino-acid-provisioning roster,
  16 real CDS) and `scripts/bake_syn3_species.py` → `syn3.json` (JCVI-Syn3.0, baked off the M. genitalium G37
  minimal-cell template — provenance documented honestly; 16 CDS). Both `niche.trophic_role: "symbiont"` +
  `niche.host_key`; godot mirrors written. New `niche.host_key` (serde-default `None`) on `Niche`/`BuiltSpecies`.
- **Conserved + deterministic:** all `i64`/fixed-point, no `HashMap` iterated, ZERO `SimRng` draws (births stay
  the sole consumer); `ledger_closes` holds every tick (a paired internal move, no new tap). Tests: symbiont
  establishes only with a host, is cull-immune at the environment layer, dies when its host dies, the host↔symbiont
  flux is conserved + appears in the FlowMatrix, run-to-run stable.
- **🔁 RE-PIN pending (Repin phase decides empirically):** the pinned single-species PLANT config registers no
  symbiont → the `host_coupling` row vector is empty → early `return`; the new variant is appended + never
  instantiated; `SymbiosisCapacity` is NOT in `Trait::ALL`; `host_draw_rate` is `0` for the plant; `niche.host_key`
  serde-defaults `None`. Pinned literal `0x47a0_3c8f_6701_f240` **VERIFIED UNCHANGED** by `determinism_hash_is_pinned`
  + the determinism gate. STOP for human review before merge.

### ADR-019 — contamination & immigration CORE (feat, S1+S2, HASH-NEUTRAL)
The SP-3-deferred seed/inoculate tool, promoted into the clean-room epic: deterministic, journaled arrivals.
- **S1** `Action::RegionInoculate { species_key, region, count, endow_j }` (serde-additive — existing
  `actions.ndjson` unchanged) + `Simulation::region_inoculate` / `register_species`: spawn `count` orgs of a
  baked `SpeciesSpec` into the region disc, RNG-FREE deterministic cell-fill in `(cell_index, slot)` order,
  OrgIds from `NextOrgId`. Each org's starting J is MINTED from a NEW named `immigration` ledger tap;
  `ledger_closes` extends to `Σlive == initial + influx + immigration − respired − overflow − chem_decay`.
  Journaled into replay so a contaminated run replays bit-identically.
- **S2** `ContainmentLevel` knob (ISO-14644 ladder; default **Sealed/OFF**) + `ConsortiumConfig` (the menu set)
  expand at run start — off a NEW off-stream `IMMG_STREAM_BASE` ("IMMG") `derive_seed` family, ZERO `SimRng`
  draws — into a sorted `Vec` of journaled `(due_epoch, RegionInoculate)` events, fired at their epochs
  (Tick-clocked). `GeneSimEnv::set_containment` / `drain_due_inoculations`; `LiveSim::inoculate` /
  `set_containment` / `register_contaminant_json` / `fire_due_inoculations` expose it for the later panel.
- **Emergent, not scripted:** establish/displace/die-out emerges from the ADR-013 metabolism→trophic→
  reproduce_or_die economy (the open-system test: a well-adapted decomposer establishes, a near-inert one dies,
  decided by the ledger).
- **HASH-NEUTRAL:** Action inert until invoked, `immigration` tap zero at rest (not folded into `hash_world`),
  knob default Sealed → empty schedule. Pinned literal `0x47a0_3c8f_6701_f240` **UNCHANGED**.

## [0.1.0-beta] — 2026-06-20
First public beta — a coherent playable build. Released via `release.yml`: installable **Linux `.deb`** +
**Windows `.zip`** (`gene-sim.exe` + `godot_sim.dll`) attached to the GitHub Release, plus per-OS dev bundles
(harness CLI + LiveSim cdylib). macOS `.dmg` deferred (needs Apple signing/notarization). Everything below is
in this release.

### ADR-012 — climate environment + pre-run main menu (feat, Phase E, E1…E4)
The player now sets a **real world** instead of a bare seed; climate shapes selection deterministically.
Built off-stream-first (like the soil substrate) as four gated slices, one deliberate re-pin:
- **E1** `crates/sim-core/src/climate.rs`: `EnvParams { lat, lon, avg_temp, season }` → `ClimateField`
  (`insolation` / `temperature` / `day_length`), pure +,-,*,clamp,abs,match — **no transcendentals** (inv #3
  cross-platform bit-identity). Off the sim RNG stream → **hash-neutral**.
- **E2** threaded env through `harness::GeneSimEnv` + the replay journal (`SeedJson` gains lat/lon/avg_temp/
  season with `#[serde(default)]` → old saves load as the neutral world) + `LiveSim.set_environment`; saved
  sessions replay under their climate. Still **hash-neutral**.
- **E3** coupling: heritable `ThermalTol` (4th spawn draw) ↔ `TemperatureMatchModifier` behind the
  `ClimateModifier` seam (inv #5); pressure scales with climate **extremity** so a temperate default is
  selection-neutral (soil signal undisturbed). **Single RE-PIN** → `0x9fad_2c9f_d298_f73a` (ledgered).
- **E4** pre-run **MAIN MENU** (`godot/main_menu.gd`, CanvasLayer overlay): seed (or random), lat/lon/temp/
  season/population, **core-computed preview** via `LiveSim.preview_climate` (inv #2 — the renderer never
  computes climate). Start reseeds in place via the proven `_do_reset` path; CLI `--lat/--lon/--temp/--season/
  --entities` is byte-identical to the menu path (shared 1000 default). Menu-free for headless/gate runs.
- **Review fixes** (adversarial workflow): the menu is now a true modal — `_unhandled_input` swallows sim
  hotkeys while it is open (ESC no longer quits the app behind it); the seed field writes back the actually-used
  value on invalid input (no silent fallback surprise).

### Save/load progress + sandbox-default live mode (feat, roadmap R6 follow-up)
The live session is now persistable, and free-play is the default:
- **Save/load via the replay contract** (deterministic, no new hash literal): `harness::replay::save_journal`
  / `read_journal` write/read a journal (`seed.json` + `actions.ndjson`) to an exact dir.
  `LiveSim` now JOURNALS every driven action (reset seed + Advance/ApplyEdit/ApplyEditRegion, with consecutive
  Advances coalesced — O(edits) not O(generations), hash-neutral). `LiveSim.save_session(dir)` writes the
  journal (it does NOT fold a hash on the LIVE env — that would draw `next_u64` and desync the stream);
  `load_session(dir)` restores the exact state by building a FRESH env and replaying `reset(seed)` + the
  actions. Verified: a saved session reloads byte-identical (same generation + allele_freq).
- **Renderer**: 💾 Save / 📂 Load buttons in the run-lifecycle row → `LiveSim.save_session/load_session` +
  resync. Round-trip test in `harness` (save → read → replay reproduces the direct-run hash).
- **Sandbox is the live default** (free play, unlimited edits); the suppress-the-zone mission (S-G2) is now
  opt-in behind `--mission` "until deeper tasks exist". Designed via a save/load design workflow.


### ADR-011 S-A…S-F — real spatial dynamics + the selective CRISPR brush (feat, roadmap R1.2/R1.3 + R5)
Designed via a multi-agent understand→design→ADR workflow; landed as gated, individually-re-pinned slices.
The grid stops being a visualization and becomes real biology, on which a *selective* brush can act:
- **S-A** per-organism `Position` on a canonical world grid (= soil dims), placed off the `SimRng` stream
  (disjoint `PLACEMENT` derive_seed family) + folded into the hash. **RE-PIN #1** (`8722…` → `3ba0…`).
- **S-B** Wright-Fisher offspring inherit the parent's cell + one bounded dispersal step → lineages cluster
  into emergent regions. **RE-PIN #2** (`3ba0…` → `0413…`).
- **S-C** snapshot aggregates by REAL position (retires the OrgId-hash layout) — hash-neutral.
- **S-D** region-scoped edit: `crispr::evaluate_region_edit` runs the same gate but returns a signed allele
  delta (no genome mutation); `sim_core::Region` + `Simulation::apply_edit_region` apply it to in-region
  organisms; `harness::Action::ApplyEditRegion(EditAction, RegionSpec)` (cells, no organism handle). The gate
  draws RNG **once** regardless of brushed area; hash-neutral on the no-edit pinned run.
- **S-E** `LiveSim.apply_edit_region(cas,target,guide,cx,cy,radius)` gdext binding → `{applied,detail,
  generation,covered}`.
- **S-F** the **brush UI**: `brush.gd` highlights the disc (iso + ortho); B toggles, wheel/`[ ]` set radius,
  click paints a region edit via the binding. `LIVE_GRID` = 32×32 so a render cell maps 1:1 to a world cell.

Invariant #6 was human-adjudicated (ADR-011): a region edit is sub-species but cell-scoped (no organism
handle, min radius) and allowed in an AI policy's action space. All deterministic (inv #3), headless-tested
(inv #4), biology in the core (inv #2). Full gate green at every slice.
- **S-G1** LOCAL soil-coupled selection: each parent reads its OWN cell's soil (`SoilField::sample_at`) instead
  of the field mean, so drought-tolerant lineages win in arid cells — real spatial adaptation (tested:
  driest-quartile drought > wettest; per-cell mismatch shrinks). Behind the `EnvironmentModifier` seam (inv #5).
  **RE-PIN #3** (`0413…` → `c01e…`).
- **S-G2** the game loop: a SUPPRESS-THE-ZONE mission (drive the cyan target zone's mean allele below a
  threshold within a deadline, on a limited edit budget — the brush lowers allele, selection raises it),
  win/lose banner + score. Renderer-side game rules over the core-exported snapshot (inv #2), not in the hash.

### CI — GitHub Actions: the gate on every push + release executables (ci, roadmap §7)
- `.github/workflows/ci.yml`: runs the single quality gate (`tools/gate.sh`) on every push to main + PR —
  fmt, clippy, full tests, determinism (inv #3), proptests, license (inv #1), the Godot headless reader, and
  the LiveSim gdext smoke. Installs the pinned Godot 4.6; the SLiM oracle + bench self-skip (no SLiM on CI).
- `.github/workflows/release.yml` (on `v*` tag / dispatch): builds distributable executables —
  the headless `harness` CLI + the `godot_sim` cdylib (LiveSim) for Linux/macOS/Windows (matrix, guaranteed),
  plus a best-effort Godot Linux game-executable export (`continue-on-error`) that bundles the cdylib.
- `godot/export_presets.cfg`: Linux + Windows export presets (Godot 4.6). The export step stages the LiveSim
  cdylib into `res://` so the GDExtension ships beside the executable. Verified locally: workflows are valid
  YAML, the release builds produce harness + cdylib at the expected paths, and Godot recognises the "Linux"
  preset (fails only on the missing template, which CI installs).

### S1–S8 / P8 — coherent game pass: sprites, game shell, run lifecycle (feat, roadmap UI)
Designed via a parallel design workflow, landed as gated slices:
- **S1+S2** trait-driven plant sprites (forb/grass-tuft/shrub by allele/fitness/density/soil; dots demoted to
  foot pips; 'S' toggle; ortho + iso on the relief). **S3** title bar + Vitals scoreboard from
  `LiveSim.observe()` (population/fitness/allele + ▲▼ trend + sparkline). **S4+S5** run-lifecycle controls
  (Restart / New run / Seed; dropped the redundant Gen slider). **S8a** on-screen notice when the live cdylib
  is missing. *(S6 user-set gen/tick cadence is the one 🛑 invariant slice — deferred for sign-off; S7
  extinction is unreachable under ADR-005 constant-N.)*

### P4 + P6 — live CRISPR interventions: apply edits to a running sim (feat, roadmap R6/R5)
The live sim becomes interactive — apply a CRISPR edit while it runs and watch the effect:
- **P4 (`crates/godot-sim`):** `LiveSim.apply_edit(cas, target, guide) -> {applied, detail, generation}` —
  builds a species-granular `harness::EditAction` (no organism handle, inv #6) and steps it through the env's
  single seeded stream (inv #3, exactly as the gym env). Never a silent no-op (explicit Applied/Failed). Plus
  `cas_variants()` / `loci()` returning `[{id,name}]` so the UI offers real choices. Authoritative PAM/score/
  gate logic stays in `crispr` (inv #2) — GDScript only assembles ids + a guide and reads the verdict.
- **P6 (`main.gd`, `timeline.gd`):** a live-mode **CRISPR Intervention** panel (Cas / locus dropdowns
  populated from the core, a guide field, an Inject button) → `LiveSim.apply_edit` → the outcome is shown and
  a green/red marker is placed on the timeline at the injection generation. Renderer **requests**, core
  **applies**. A `--inject` CLI hook fires one demo injection for `--shot` verification.

Verified: the panel populates from the core (Cas: SpCas9, Locus: growth_locus), `apply_edit` applies
(SpCas9→growth, gen 21) and rejects a malformed guide. godot-sim clippy clean; full gate green (10/10);
determinism untouched.

### P5 — `--live` mode: the renderer drives an open-ended live sim (feat, roadmap R6)
The renderer can now run the simulation LIVE via the LiveSim gdext node, instead of replaying pre-baked
snapshot files (read-only presentation — biology stays in Rust, inv #2):
- `main.gd --live [--seed N]`: loads the LiveSim extension at **runtime** via `GDExtensionManager.load_extension`
  (a temp `user://` .gdextension pointing at the built cdylib) — so the default project + gate stay
  extension-free. Instantiates `LiveSim`, `reset(seed)`, then a timer advances a **fixed integer** generations
  per tick (deterministic cadence, inv #3), pulling `LiveSim.snapshot()` GSS2 bytes each tick.
- `snapshot.gd::parse_bytes(PackedByteArray)`: parse a GSS2 snapshot from an in-memory buffer (the live path)
  rather than a file, so the existing render (organisms / data overlay / **isometric**) is reused unchanged.
- Open-ended run with a rolling snapshot history (timeline + scrubbing over recent generations); play/pause/step
  drive the live sim; the HUD shows `● LIVE`. Falls back to file replay if the cdylib is not built.
  Composes with `--iso`/`--layer`. Verified windowed (live loop steps clean) + `--shot`. Manual interventions
  (apply_edit) + save are P4/P6. Full gate green (10/10); determinism untouched.

### P1b — LiveSim gdext GDExtension: the renderer can drive the sim live (feat, roadmap R6/P1)
The Rust live-sim binding (ADR-010), built by a parallel agent + integrated here:
- `crates/godot-sim` — a **godot-rust (gdext) cdylib** (godot `=0.5.3`, `api-4-6`, edition 2024) embedding
  `harness::GeneSimEnv`/`sim_core`, registering a `LiveSim` node with `reset(seed)`, `step(n)`, `observe()`,
  `snapshot(w,h)->PackedByteArray` (GSS2 bytes the existing `snapshot.gd` reads). GDScript only **calls** it
  → all biology stays in Rust (inv #2); no new RNG (inv #3); fixed-integer cadence.
- **Forward-compat confirmed:** the api-4-6 cdylib **loads + runs under the installed Godot 4.7** (gdext rule
  runtime ≥ API; init line `API v4.6.stable, runtime v4.7.stable`) — so dev needs no separate 4.6 install. The
  crate is workspace-**detached** (own `Cargo.lock`) so the main gate is unaffected; gdext is MPL-2.0, no GPL
  (inv #1 intact — separate link unit).
- `tools/check_livesim.sh` (gate **10/10**, skip-if-absent): builds the cdylib + loads `LiveSim` in an
  ISOLATED temp project + drives reset→step→observe→snapshot, asserting `LIVESIM_SMOKE_OK`. The renderer
  project `godot/` stays extension-free so the other gates never touch the dylib. `apply_edit`/`save_session`
  + the renderer `--live` mode are the next phases (P4/P5).

### P1a — replay CLI: the live-session determinism contract, headless (feat, roadmap R6/P1)
The pure-Rust, no-Godot foundation of the live-sim epic (ADR-010) — the replay-equality that the gdext
`LiveSim` node will satisfy, exposed on the CLI:
- `harness --record-episode <DIR>` records a journaled `reset + Advance + ApplyEdit` episode (the shape a live
  `LiveSim` session produces) to `<DIR>/<run_id>/` (`seed.json` + `actions.ndjson`) and prints its hash.
- `harness --replay <DIR>` replays it and prints the stats hash — **bit-identical** to the recorded one on the
  same build (SPEC §6, inv #3). Both wrap the existing `harness::replay` contract (S3.2).
- `crates/harness/tests/replay_cli.rs` drives the binary end-to-end (record → replay → identical hash). This
  is the **gate-blocking proof of the live architecture** and needs no Godot; the gdext crate + Godot load
  smoke (P1b) follow once Godot 4.6 is installed. Full gate green; determinism hash unchanged.

### Gameplay batch P0 — live-sim architecture decision (ADR-010; multi-agent designed, signed off)
Decision gate (no code) for the live/continuous-sim + interventions + multi-species + isometric batch:
- **Architecture (signed off):** Option A — a `crates/godot-sim` **gdext GDExtension** embedding the
  already-stepwise/edit-able `sim-core`/`harness::GeneSimEnv`, exposing a `LiveSim` node
  (reset/step/apply_edit/observe/snapshot/save_session). GDScript only *calls* it → biology stays in Rust
  (inv #2). Determinism via the existing `actions.ndjson` replay contract (replay-equality, not a 2nd hash
  literal); a pure-Rust replay test is the gate-blocking proof. The `run_stats()` RNG-draw impurity gets a
  clone-fold fix; the play loop uses a fixed integer generations/tick cadence.
- **Repin Godot 4.7→4.6** (`tools/install_godot.sh`, DECISIONS) so the cdylib targets stable gdext **api-4-6**
  (inv #7); the renderer uses no 4.7-only API. gdext is MPL-2.0 (license gate unaffected; cdylib is a separate
  link unit → inv #1 GPL boundary intact).
- Sequenced into phases **P0–P8** (TASKS §Gameplay batch): renderer phases (timeline markers, isometric,
  sprites) ride the normal loop hash-neutrally while the live-sim crate is built; multi-species (P7) is
  sequenced last (it rewrites the same `selection()` as R1.2/R1.3). ADR-010.

### R1.0a + R1.1 — soil-coupled selection: terrain shapes evolution (feat, roadmap R1)
The terrain stops being inert — it now drives selection (extends ADR-005):
- **R1.0a:** a per-organism heritable `DroughtTol(f64)` ECS component — standing variation seeded once at
  spawn from `SimRng` (fixed draw order), **inherited** (not resampled) from the fitness-sampled parent, and
  folded into `hash_world`. Independent of the species GP map (the dead DroughtTolerance trait is bypassed).
- **R1.1:** `selection()` weight = `fitness(base, genotype) × EnvironmentModifier::fitness_factor(soil,
  drought)` using the in-core `LinearTraitMatchModifier` (drought-tolerant favoured on drier soil) fed the
  field-wide **mean** soil (`MeanSoil` resource — "global" coupling). The factor is strictly positive, so
  ADR-005's constant-N / no-extinction holds; the loop draws exactly N words (offspring inherit, never
  resample), so determinism stays reproducible.
- **Proven:** a test shows the population's mean drought tolerance moves toward the terrain target
  `(1 − mean_moisture)`. New pinned hash literal `8722…44aa` (was `c530…7ab1`). Perf re-baselined in-slice
  (~+6 % at 1 k entities from the per-parent modifier call; within noise at the 10 k headline ~19 M
  updates/s). ADR-009.

### R1.0 — terrain/soil substrate: hash-neutral static SoilField (feat, roadmap R1; multi-agent designed)
Multi-agent designed (3 scoping lenses → adversarial vetting against determinism/ADR-005/perf/snapshot →
synthesis) + human sign-off. First slice of the terrain epic — **substrate only, provably hash-neutral**:
- `crates/sim-core/src/soil.rs`: a static `SoilField` (moisture / nutrients / pH, each `[0,1]`) generated
  once in `Simulation::reset` from `derive_seed` (value-noise over a 5×5 lattice, multiply-add only) — **zero
  `SimRng` draws**, never folded into `hash_world`. Plus an `EnvironmentModifier` trait (invariant-#5 seam) +
  `LinearTraitMatchModifier` default, present but **unwired** (coupling is R1.1+).
- Snapshot gains **3 read-only soil channels**: `CHANNEL_COUNT` 3→6, magic **GSS1→GSS2** (loud bad-magic on a
  stale reader). `godot/snapshot.gd` is **parse-only**; the click-detail panel now shows per-cell soil values
  (no shader/overlay — "Godot LAST" respected).
- **Determinism proven:** a new test pins the exact pre-soil hash literal (`0xc530…7ab1`); matching it on the
  with-soil build proves soil is hash-neutral (guards the `check_determinism.sh` silent-change gap). Perf
  within criterion noise (no re-baseline; soil gen is off the hot loop). ADR-008 + a `derive_seed` stream registry.

### UI/controls + visual polish round (A+C; feat/refinement, Stage 4) — multi-agent designed
Designed + adversarially vetted by a multi-agent **workflow** (parallel design → invariant-#2/Godot-4.7-API
review → synthesized gated plan), then implemented serially (one slice → headless `--check` → `tools/gate.sh`
9/9 → windowed `--shot` visual check → commit). All read-only presentation (invariant #2); the determinism
hash is unchanged throughout.
- **S1 / C1 — plant polish** (`lsystem.gd`): leaves render as teardrop polygons oriented along the live tip
  heading; fecundity-driven flowers (petal ring + centre); ground line + 16-gon shadow under each base. All
  geometry precomputed in `build()` so the headless gate catches malformed polygons; `bounds()` unchanged.
- **S2 / A1 — specimen UX** (`main.gd`): a top-right panel — specimen selector (`OptionButton`) + a 5-trait
  readout (ProgressBar + value + **delta-vs-baseline** arrow ▲/▼/=). Focusing brightens the chosen plant,
  dims the rest, and frames the camera. Tab cycles; `--focus <i>` for deterministic `--shot`.
- **S3 / A2 — ecosystem controls** (`main.gd`): a second control-bar row — playback-speed slider (runtime
  `_frame_seconds`), zoom-scope toggle buttons (Field/Patch/Cells, synced to the camera), and a generation
  scrubber (bidirectional, `set_value_no_signal` + a re-entrancy guard). Step/scrubber disable in the
  specimen view; window margin bumped so the two-row bar is fully on-screen.
- **S4 / C2 — ecosystem polish** (`organisms.gd`, `main.gd`, `data_layer.gdshader`): softer organism markers
  (halo + core); richer grass (per-pixel blade streaks); a screen-space edge **vignette** (CanvasLayer 1
  below the UI at layer 2; hidden in the specimen view); and an overlay **alpha-gamma** curve in the shader
  (smoother heat — the `inferno(v)` colour mapping stays byte-identical, only alpha is shaped).

### S4.5 — L-system plant morphology + UI controls (feat, Stage 4) — **Stage 4 COMPLETE**
- **Core export** (`harness --specimens <DIR>` → `specimens.json`): the species-genome **trait vector**
  (baseline) plus one per demo CRISPR edit, each expressed by the core's `WeightedSumMap` GP map via a
  separate `GeneSimEnv` (its own seeded RNG — never the hashed run, so **no determinism-hash impact**,
  inv. #3). Any edit outcome (Applied *or* Failed) mutates the genome, so every specimen's traits differ
  from baseline — genotype→phenotype stays in the core (inv. #2).
- **L-system renderer** (`godot/lsystem.gd`): a parametric bracketed turtle-graphics plant (ABOP grammar)
  drawn from **numeric params only** — pure presentation, zero biology. `main.gd::_plant_params_from_traits`
  maps each trait → a visual param (growth→size/reach, reflectance→spread+leaf hue, drought→taper+tip colour,
  fecundity→leaf size, kill-switch→jitter). The genome→trait math is the core's; trait→appearance is the
  renderer's job (SPEC "L-system rule params").
- **Specimen view** (key `V` / the View button): renders baseline + edited plants side by side with captions
  — an edit **visibly** stunts (growth knockdown) or greens-and-grows (kill-switch/reflectance) the plant.
- **UI control bar:** view toggle (Ecosystem ⇄ Specimen), play/pause, step ◀/▶, and a data-layer dropdown —
  all change *view* state only (no biology). Keyboard shortcuts still work and stay in sync.
- The gate's headless `--check` now also builds the L-system specimens (catches GDScript errors in CI); the
  gate generates `specimens.json` for the check. Full gate green; determinism hash unchanged. ADR-007.

### S4.3/S4.4 visual polish (refinement, Stage 4)
- **Heatmap palette:** the data-layer shader now uses an *inferno* ramp (indigo→purple→red→orange→yellow)
  that contrasts with the green field instead of the muddy blue→cyan over grass.
- **Organisms** (`organisms.gd`): markers get a white specular core + darker rim and a palette off the grass
  green (cyan→magenta→red by allele_freq); fitter cells render slightly larger — far more legible.
- **Grass** (`main.gd`): terrain shade comes from a coarse block (grassy patches, not per-tile checker noise)
  with an occasional single-cell speckle and a darker soil tone.
- **HUD:** the status line sits in a translucent panel; a new bottom-left **legend** shows the active layer
  name + the colormap gradient (low → high). All read-only presentation (invariant #2); gates unaffected.

### S4.4 — data-layer shaders + zoom scopes (feat, Stage 4)
- `godot/data_layer.gdshader` (canvas_item): samples the per-cell data texture the core produced
  (R=density, G=allele_freq, B=fitness via `snapshot.gd::to_data_image`) and maps the channel chosen by a
  `layer` uniform through a heat colormap on the GPU — replacing the S4.3 CPU `_heat` loop. INVARIANT #2
  intact: the shader only **visualises** values the core already computed.
- **≥2 toggleable data layers:** `D` cycles off → density → allele_freq → fitness (the shader `layer`
  uniform); the overlay `Sprite2D` uses NEAREST filtering so each texel is one crisp cell.
- **Viewport zoom scopes:** mouse-wheel = continuous zoom; keys `1`/`2`/`3` jump to scope presets
  (field ×1 / patch ×2.6 / cells ×6); arrows pan. HUD shows the live layer + scope + magnification. The
  zoomed "cells" scope makes individual organism dots and per-cell data legible.
- `--shot` gains `--layer <0..3>` and `--zoom <f>` so each layer/scope can be captured for visual review.
  Verified by windowed screenshots of the allele_freq, fitness and zoomed-density views; the headless
  `--check` render smoke (gate 9/9) now also builds the `ShaderMaterial` path. Cargo gates + determinism
  hash unaffected. (Renderer architecture: ADR-006.)

### S4.3 — 2D ecosystem view: live run render from snapshots (feat, Stage 4)
- `godot/main.gd` now builds a **2D ecosystem view of one scope** in code (all read-only — invariant #2):
  a tiled **grass field** (`TileMapLayer` from a procedurally-generated shade atlas), a per-cell **data
  overlay** (`Sprite2D` heat texture: density / allele_freq / fitness), an **organism dot layer**
  (`godot/organisms.gd`: per-cell markers, hue=allele_freq, brightness=fitness, count∝density — hash-jittered
  scatter is presentation only, not a spatial model), a framing `Camera2D`, and a HUD (gen / pop / grid / layer).
- **Live run playback:** `--run <dir>` loads every `snap_*.bin` ordered by generation and auto-advances on a
  timer (loops); with no args + a display it auto-discovers the newest `data/runs/<id>/` holding snapshots.
  Keys: Space pause · D cycle overlay (off/density/allele/fitness) · `,`/`.` step. The gen-0→gen-60 render
  visibly tracks selection (more amber organisms + warmer overlay as allele_freq shifts).
- **Verification harness:** windowed `--shot <png> [--gen N]` captures the real viewport to PNG (human/agent
  eyeballing); headless `--check` builds the scene and prints `render scene OK` (no GPU). The Godot gate
  (`tools/check_godot_snapshot.sh`, step 9/9) now runs **both** the S4.2 reader check and the S4.3 render
  smoke — catching GDScript parse/logic errors in CI. Fixed a `:=` type-inference parse error (untyped
  `Array` index → `Variant`). Determinism hash unchanged; cargo gates unaffected. See ADR-006.

### S4.2 — snapshot reader: Rust→GDScript render bridge (feat, Stage 4)
- `crates/sim-core/src/snapshot.rs`: `GridSnapshot` — a **derived, read-only** per-cell grid
  (`density` / `allele_freq` / `fitness`, each `[0,1]` row-major) produced by `Simulation::snapshot(w,h)`.
  Placement is a pure function of `OrgId` (splitmix, no RNG draw, no mutation) → byte-identical for a fixed
  `(seed, generation, grid)` and **cannot** change the determinism hash (invariant #3). `std`-only binary
  format `"GSS1"` (LE header + 3 channel-major `f32` planes); round-trip + read-only tests in-crate.
- `harness --snapshots <DIR> --grid WxH`: writes `snap_<gen>.bin` per epoch + final, off the hash path (additive).
- `godot/snapshot.gd` (**read-only**, invariant #2): parses `GSS1` bytes → channels + `to_data_image()`
  (RGBF data texture for the S4.4 shader). `godot/main.gd --snap <file>` loads one headless and reports
  `WxH, gen, population, cells, channels`.
- **Headless robustness fix:** dropped the `class_name Snapshot` global (only registered by an editor import
  pass, so unresolved under a fresh `--headless` run) in favour of `preload` + a self-preload const — the
  reader now parses cleanly with no `.godot/` cache.
- New gate **9/9** `tools/check_godot_snapshot.sh`: generates a snapshot with the headless core and asserts
  the Godot reader reports `snapshot OK`; SKIPs when godot is absent (mirrors the slim oracle gate). Enforces
  invariant #4 for the first UI feature and locks in the headless fix. Determinism hash unchanged.

### S4.1 — Godot UI skeleton + headless smoke (chore, Stage 4; human-signed-off 🛑)
- `godot/` thin 2D project (Godot **4.7**, GL-compatibility): `project.godot`, `Main.tscn`, `main.gd`. The
  script is **read-only** — boots, prints version, exits under headless (invariant #2: no biology in GDScript).
- `tools/install_godot.sh` (SPEC §W3): brew-cask install + version check + `godot --headless --path godot --quit`
  smoke. Godot pinned 4.7 in DECISIONS (commit `5b4e0cb0`). Build-order gate satisfied — core is headless +
  deterministic through Stage 3 (invariant #4). UI-only slice; cargo gates unaffected, verified via the Godot
  headless smoke (`UI booted … headless smoke OK`).

### S3.3 — parallel batch runner + columnar Parquet stats (feat, Stage 3)
- `harness --per-gen-stats`: drives the stepwise `Simulation` and writes `data/runs/<run_id>/per_gen.csv`
  (run_index, generation, population_size, allele_freq + 5 trait columns), additive — final stats hash
  unchanged (proven). `run_id` for `--run-index` now keyed `_i{index}` so parallel jobs don't collide.
- `tools/run_batch.sh [MASTER] [RUNS] [GENS]` (SPEC §W7): builds release once, runs `target/release/harness`
  in parallel via `xargs -P $(nproc)` over derived seeds. **Two batches → byte-identical per_gen.csv** (reproducible).
- `scripts/aggregate_parquet.py` (pyarrow): globs `data/runs/*/per_gen.csv` → one columnar **Parquet**
  (pinned schema, lossless concat). Verified: 8 runs → 400 rows × 9 cols.
- `pyarrow 24.0.0` pinned (`scripts/requirements.txt` + DECISIONS row; Apache-2.0, analysis-only, never linked).
  Determinism hash unchanged (`fde0e0b6…`). Loop: implementer (Rust+shell) + orchestrator (Python) → gate
  (GREEN) → reviewer (send-back for the pyarrow pin → recorded → APPROVE).

### S3.2 — replay logs: seed.json + actions.ndjson (feat, Stage 3)
- `crates/harness/src/replay.rs`: `record_episode(config, seed, actions, dir)` writes `data/runs/<run_id>/`
  `seed.json` (master seed + config + pinned tool versions, SPEC §5) + `actions.ndjson` (one `Action`/line);
  `replay(dir)` re-runs and returns the final stats hash. Record & replay share one private `run_episode`, so
  **replay is bit-identical by construction** (SPEC §6). Deterministic `run_id` (no wall-clock).
- serde plumbing: `genome::LocusId` (`#[serde(transparent)]` u32), `crispr::GuideSequence` (hand-rolled serde —
  deserialize routes through `GuideSequence::new`, so a non-ACGT guide in a log fails to load), `Action`/
  `EditAction` derive serde. `serde_json` added (workspace dep, MIT/Apache; DECISIONS row).
- Determinism hash unchanged (`fde0e0b6…`). Tests: record→replay bit-identical, malformed-guide rejected,
  action_count mismatch rejected, serde round-trips. Loop: implementer → gate (GREEN) → reviewer (send-back
  for the `serde_json` pin → recorded → APPROVE).

### S3.1 — gym-like environment (reset/step/seed) (feat, Stage 3)
- `crates/sim-core`: public stepwise `Simulation` handle (`reset`/`step`/`observe`/`species_genome`/
  `with_genome_and_rng`) + public `Observation { generation, population_size, allele_freq, phenotype }`.
  `run_headless` reimplemented on top of it — **bit-identical** (determinism hash unchanged `fde0e0b6…`).
- `crates/harness` (now lib+bin): `Env` trait (`reset/step/seed`) + `GeneSimEnv`; `Action { Advance(u64),
  ApplyEdit(EditAction) }` — **species/operator-granular only** (invariant #6; per-organism actions
  unrepresentable). `ApplyEdit` runs `crispr::apply_edit` on the species genome and re-expresses phenotype.
- Determinism (inv. #3): one ChaCha8Rng seeded once in `reset`, threaded through step + edit via
  `std::mem::replace` (stream position preserved — no re-seed/clone). reward = `allele_freq` ∈ [0,1].
- Tests: stepwise==one-shot, observe-is-pure, edit-changes-phenotype, reset/step/seed cycles, replay
  determinism (+proptest). Loop: implementer → gate (GREEN) → reviewer (APPROVE).

### S2.4 + S2.5 — golden oracle gate + license gate (feat, Stage 2; **Stage 2 complete**)
- **S2.4** golden oracle gate (SPEC §10.6): `data/golden/slim_case1.json` records the stats for a pinned case
  (seed 1234 + the produce_trees params, SLiM v5.2). `slim_analyze.py --check` compares a fresh run to the
  golden (integer fields exact, floats within rel-tol 1e-6); `tools/check_slim_oracle.sh` drives it and skips
  gracefully if slim/.venv/golden are absent. Wired into `tools/gate.sh` as gate 7/8. Verified: passes on a
  fresh run, fails on a tampered golden. This pins the genetics to SLiM v5.2 (re-record + ADR on a version bump).
- **S2.5** license gate — already delivered in the dev-loop hardening (`scripts/check_license.sh`, gate 8/8):
  SPDX-OR-aware GPL detector + `oracle-slim` depless assertion. Marked done; no new work.
- `tools/gate.sh` is now an 8-gate suite (added the oracle gate); the `gate` skill lists it.

### S2.3 — tskit `.trees` analysis (feat, Stage 2)
- `scripts/slim_analyze.py` (tskit): reads a SLiM `.trees` → JSON stats (num_samples/individuals/trees/sites/
  mutations, segregating sites, mean+max derived-allele freq ∈ [0,1], nucleotide diversity). Stats come from
  the genealogy, not file bytes (provenance timestamps differ).
- `crates/oracle-slim/examples/produce_trees.rs`: runs the S2.2 driver → writes `data/runs/slim_demo/out.trees`
  → prints path; chains S2.2 → S2.3 (`cargo run -p oracle-slim --example produce_trees <seed>`).
- **Verified SLiM genetics are reproducible** for a fixed seed (identical stats twice; different seed differs)
  — de-risks the S2.4 golden gate.
- Python stack pinned in `scripts/requirements.txt` (`.venv`, gitignored): tskit 1.0.3 / pyslim 1.1.1 /
  numpy 2.4.6 (MIT/MIT/BSD) + msprime 1.4.2 (**GPL-3, standalone-analysis-only — never linked**, invariant #1
  unaffected; same pattern as the SLiM subprocess). DECISIONS rows added.

### S2.2 — oracle-slim SLiM subprocess driver (feat, Stage 2)
- `crates/oracle-slim`: **dependency-free** (std-only) driver — `SlimParams` → `write_model` generates a
  self-contained SLiM 5 Eidos model (params baked via `defineConstant`, `initializeTreeSeq()`, final
  `<gen> late()` → `treeSeqOutput` + `simulationFinished`) → `run_model` shells out
  `Command::new(slim).arg("-seed").arg(seed).arg(model)` and returns the `.trees` path. `SlimError` carries
  SLiM's stderr; `resolve_slim_bin` = `SLIM_BIN` → `~/.local/bin/slim` → PATH.
- **Invariant #1 verified (adversarial review):** zero deps (`cargo tree` shows the crate alone), no FFI/
  `#[link]`/`build.rs`/linkage — `slim` is invoked as a subprocess only, never linked. Seed passed in
  (caller derives via `sim-core::derive_seed`); oracle-slim adds no entropy.
- Tests: model-generation unit tests (no slim needed) + an integration test that actually runs slim
  (fixed seed → non-empty `.trees`) and **skips gracefully** when slim is absent. Does not byte-compare
  `.trees` (SLiM provenance timestamps differ). Loop: implementer → gate (GREEN) → reviewer (APPROVE).

### S2.1 — build SLiM from source, pinned (chore, Stage 2; human-signed-off 🛑)
- `tools/install_slim.sh`: clones MesserLab/SLiM, checks out the pinned tag (`v5.2`), CMake Release build,
  symlinks the CLI to `~/.local/bin/slim`. GPL-subprocess-only contract documented at the top (inv. #1).
- Built + installed **SLiM v5.2** (commit `f11de0d`); `slim -version` confirmed. Recorded in DECISIONS
  (SLiM row flipped to installed). Invariant #1 verified: license gate green, `oracle-slim` still depless,
  no GPL crate in the workspace tree (SLiM is purely an external binary — never linked).

### S1.5 — genotype→phenotype map + selection (feat, Stage 1; **Stage 1 complete**)
- `crates/sim-core/gp.rs`: `Trait`/`Phenotype`/`GenotypePhenotypeMap` (TAXONOMY §2) + `WeightedSumMap` (transparent
  weighted sum of genome param unit-scalars → traits, clamped [0,1]). Pure/deterministic; trait boundary (inv. #5).
- Selection wired into the tick loop: per-organism `Genotype∈[0,1]` (seeded), constant-N **Wright-Fisher**
  resampling ∝ fitness (`0.05 + base_growth·genotype`), drawn from the single `SimRng` in `OrgId` order (inv. #3;
  ordered cumulative table + binary search; BTreeMap write-back). `allele_freq` (mean genotype) in `RunStats`,
  folded into the hash, surfaced by the harness. No extinction (constant N).
- Determinism hash updated `3393…`→`fde0e0b61b9e23e6` (expected; gate compares two runs, still GREEN).
- Perf re-baselined at Stage 1 exit (~175 M→~19 M organism-updates/s at 10k; selection added — DECISIONS table).
- ADR-005 (selection model). Tests: express-deterministic, selection-responds-to-trait (directional allele_freq),
  proptest allele_freq+traits ∈ [0,1], same-seed-same-stats. Loop: implementer → gate (GREEN incl. bench) →
  reviewer APPROVE. Follow-ups F1/F2 tracked in TASKS.

### S1.4 — gated edit application (feat, Stage 1)
- `crates/crispr`: `apply_edit(genome, edit, variants, on, off, thresholds, rng)` — the core CRISPR mechanic
  (SPEC §4): resolve cas+locus → find PAM → score (on/off) → gate. Pass ⇒ mutate the target Parameter
  (magnitude from on-eff); fail ⇒ realistic off-target perturbations on *other* loci. `Edit`,
  `EditThresholds {min_on_target, max_off_target}` (default 0.5/5), `EditFailure`, `EditOutcome {Applied|Failed}`.
- Determinism (inv. #3): the passed-in `&mut ChaCha8Rng` is the ONLY randomness source (same `rng_unit` as
  sim-core); ordered-Vec selection, no HashMap. Generic over the S1.3 score traits (inv. #5 preserved).
- §10.4 property gates: `genome.is_valid()` always holds after a valid-input edit (every mutation clamps);
  forced-fail edits never return `Applied` and never touch the target Parameter. 30 unit + 5 proptests.
- Dep edge: `rand_chacha` added to crispr (already workspace-pinned; no new crate, no DECISIONS change).
  Loop: implementer → gate (GREEN) → reviewer (adversarial APPROVE).

### S1.3 — pluggable Score traits + in-core default impls (feat, Stage 1)
- `crates/crispr`: `OnTargetScore`/`OffTargetScore` traits (match TAXONOMY §3.3) — the invariant-#5 swappable
  science boundary (object-safe + generic-usable; proven by an alternate impl substituting with no trait/
  sim-core change). `GuideSequence` (validated ACGT, mirrors `DnaSequence`).
- `DefaultOnTargetScore`: pure heuristic `clamp_[0,1](0.5·gc + 0.3·length + 0.2·pam)` (gc peaks at 50%, length
  favors 17–24 nt, pam = valid PAM adjacent to the guide's locus match). `DefaultOffTargetScore { mismatch_budget=3 }`:
  naive Hamming near-match count across all loci, both strands, iterating the ordered `Vec` (inv. #3).
- No new deps. Tests: efficiency ∈ [0,1], off-target absent=0/present>0/monotone-in-budget, determinism,
  pluggability (generic + `dyn`), proptest (efficiency always in unit interval). Loop: implementer → gate
  (GREEN) → reviewer (APPROVE). TAXONOMY §3.2 `GuideSequence` synced to the validated form.

### S1.2 — PAM finding via rust-bio (feat, Stage 1)
- `crates/crispr`: `find_pam_sites(seq, variant)` (+ `_in` for `genome::DnaSequence`) returning ordered,
  `(position, strand)`-sorted `PamSite { position, strand, cut_site }` on both strands. `Strand` enum;
  public `iupac_matches` (full IUPAC set, case-insensitive, U→T). Reverse strand via `bio::alphabets::dna::revcomp`.
- Cut-site convention documented on `PamSite` (forward frame; forward `position+cut_offset`, reverse
  `(position+pam_len-1)-cut_offset`). Determinism preserved (sorted Vec, no HashMap; inv. #3).
- Dep: `bio` (rust-bio) `4.0`, MIT, GPL-free tree verified (ADR-004 — rust-bio for seq ops, IUPAC degeneracy
  kept in-house per SPEC §0.4).
- Tests: NGG/TTTV known sequences incl. reverse hit + cut math, TTTT-excluded, IUPAC table, determinism;
  proptest: every reported site truly matches the PAM (no false positives). Loop: implementer → gate (GREEN)
  → reviewer (send-back for the missing `bio` pin → fixed → APPROVE).

### S1.1 — Cas-variant data table + loader (feat, Stage 1)
- `data/cas_variants.ron`: seed table of 7 Cas variants (SpCas9 NGG, SaCas9 NNGRRT, AsCas12a TTTV, Cas9-NG,
  SpRY NRN, BE4 base editor, PE2 prime editor) — *data, not code* (SPEC §4).
- `crates/crispr`: `CasVariant`/`CasVariantId`/`EditType` matching TAXONOMY §3.1; `load_cas_variants_from_str`
  (clean `LoadError`) + `default_cas_variants()` embedding the RON via `include_str!`. Ordered `Vec` (inv. #3).
- Deps pinned: `serde = "1"`, `ron = "0.12"` (both MIT/Apache; ADR-003 — 0.8 not in registry, 0.12 is current).
- Tests: round-trip (+proptest), ≥5 variants, literature PAMs, all edit types, PAM-relaxed, non-zero base
  window, malformed-RON error. Driven through the multi-agent loop (implementer → gate → reviewer: APPROVE).

### Dev loop hardened (chore)
- `tools/gate.sh`: single robust gate runner — fmt · clippy `-D warnings` · test · determinism · proptest ·
  bench (opt-in `GATE_BENCH=1`) · license; PASS/FAIL/SKIP/N-A per item, non-zero exit on any red.
- `scripts/check_license.sh`: real licensing gate (promoted from the S2.5 stub) — SPDX-`OR`-aware GPL
  detector via `jq` (flags only crates with no GPL-free choice; allows `MIT OR … OR LGPL`) + asserts
  `crates/oracle-slim` is dependency-free. Guards invariant #1 from day one.
- `docs/llm/LOOP.md`: durable runbook for the robust loop — roles, per-slice procedure, **autonomous-until-
  red/invariant** mode, stop conditions, resumability (state in TASKS.md + git), and the skill/agent
  mid-session registration gotcha.
- Skills fixed: removed the invalid `invocation: user` frontmatter field (silently ignored by Claude Code —
  the cause of `/iterate` not registering); `gate` now calls `tools/gate.sh`; `iterate` encodes autonomous
  multi-agent mode. CLAUDE.md / SNIPPETS.md point at the new machinery.

### S0 — Stage 0: headless deterministic core skeleton (feat)
- Cargo workspace with 5 crates: `genome`, `crispr` (stub), `sim-core`, `harness`, `oracle-slim` (stub).
- `crates/genome`: parametric `Genome` model — `Locus` / `Parameter` / `ParamValue` (Numeric/Enum/Bool with
  domains) / `DnaSequence` (validated ACGT) / `OntologyTags`, plus a deterministic `sample_genome()`.
  Mirrors docs/llm/TAXONOMY.md §1.
- `crates/sim-core`: empty-but-deterministic Bevy ECS tick loop (`bevy_ecs` 0.19) — single seeded
  `ChaCha8Rng` resource, explicit `.chain()` system order, id-sorted end-of-run hash, `derive_seed`
  splitmix64 sub-seeding. `genome` wired into the core.
- `crates/harness`: headless CLI (`--seed/--master-seed/--run-index/--runs/--generations/--entities/
  --hash-only`); per-run derived seeds; writes `data/runs/<run_id>/{seed.json,stats.ndjson}`.
- `tools/check_determinism.sh` (SPEC §W8); criterion bench `crates/sim-core/benches/tick.rs`.
- Property tests behind the `proptest` feature (genome domain invariants; same-config-same-hash).
- **Gates green:** fmt, clippy `-D warnings`, 12 unit tests, determinism, 3 property tests, bench baseline
  recorded in DECISIONS.md (~175 M organism-updates/s on M4 Max). License gate N/A until Stage 2 (S2.5).
- Fixed a seed-derivation collision (`stream | 1` collapsed streams 0 and 1) caught while verifying DoD.

### Meta / scaffolding
- Repo bootstrapped: `CLAUDE.md` (7 invariants + per-slice loop), `docs/llm/SPEC.md` moved to its canonical
  location, and the persistent context files (`TASKS.md`, `DECISIONS.md`, `TAXONOMY.md`, `GLOSSARY.md`,
  `SNIPPETS.md`).
- `.claude/skills/{iterate,gate,slice-done}` and `.claude/agents/{planner,implementer,gatekeeper,reviewer}` added.
- ADR-001 (native macOS Apple-Silicon toolchain; SLiM-from-source; Crisflash off-target oracle) and
  ADR-002 (Stage 0 determinism strategy) recorded.
