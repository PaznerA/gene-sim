# QUEUE — the workflow zásobník for looped development

> The stack `/roadmap-iterate` pops from and `/roadmap-plan` refills. One queue item = one multi-agent
> **Workflow** (`.claude/workflows/*.js`) = one merge to `main`. Keep **≥5** forward items defined at all times.
> Guardrails: `autonomous-roadmap.md §0` + SPEC §2.1. The pinned determinism literal is
> `0x47a0_3c8f_6701_f240` — hash-neutral items must leave it byte-identical; a 🔁 re-pin moves it deliberately.
>
> **Status:** `[ ]` READY (tracked `.js` exists, or driver `direct`/`slice`) — runnable now ·
> `[def]` DEFINED (robust spec below, `.js` not yet authored — `/roadmap-plan` converts it to READY) ·
> `[~]` in progress · `[x]` done · `RED` failed gate/verify (left for human) · 🛑 needs human sign-off.
> **Driver:** `workflow` = run the named `.js` · `slice` = one implementer+gate+reviewer pass · `direct` = trivial inline edit.
>
> **Lead thrust (chosen 2026-06-28): Discovery / auto-research.** The first brute-force batch validated the whole
> pipeline (21 verified gems in ~60s/run; the Variant Lab D edit axis produced the #1 gem; 19/21 distinct community
> shapes; M1 saturates → validates the drama-weighted target). Next: make the search SCENARIO-targeted over multiple
> starters, branch from discovered gems, and let the player WATCH a gem replay. **Frontier: `main` @ `b865644`.**

---

## ▶ ACTIVE QUEUE (discovery / auto-research)

| # | Status | Item | Driver | Goal | Hash | Deps |
|---|--------|------|--------|------|------|------|
| 1 | `[x]` | **discovery-scenarios-impl** | workflow | Named `SearchSpace` SCENARIO presets (predator-prey / decomposer / contamination-open / spore-resilience / edit-rescue / extreme-climate) biasing species set + count/containment/temp ranges + `edit_budget`, + a `--space <name>` CLI flag + a multi-starter batch — **the "more starters" ask** | ✅ | discovery D2a/D2b + Variant Lab D (done) |
| 2 | `[x]` | **discovery-continue-from-gem-impl** | workflow | A runner that LOADS a saved gem → seeds a fresh evolutionary search FROM it (branch + keep evolving/editing the discovered community); every continued gem round-trips — **the "continuation after -X gens" ask** | ✅ | gems exist · discovery infra (done) |
| 3 | `[x]` | **discovery-load-gem-replay-impl** | workflow | Renderer reads a saved gem → configures a live run + replays the gem edits → the player WATCHES the scenario. (v1 RED on edit-replay divergence; **v2 PASS** — resolution moved to a read-only core `gem_edit_schedule` reusing `edits_to_actions` + off-hash `Gem.gens_requested`. ADR-030.) | ✅ | gems exist · Variant Lab D (done) |
| 4 | `[x]` | **starter-map-library-impl** | workflow | Promote the curated gems into named committed starter maps: **gen-1** (fresh config) + **gen-N checkpoints** (edits recorded in the scrub-back timeline) + an RCT-style selector. 7 shipped (6 gen-1 + 1 checkpoint). ADR-031. | ✅ | #2 continue-from-gem + #3 load-gem-replay |
| 5 | `[x]` | **scenario-gif-preview-impl** | workflow | Auto-GIF of a scenario's KEY EVENTS (off-hash D1 trace: booms/crashes/takeovers + edit gens) → macOS-safe frame capture → an animated GIF via the linked MIT `gif` crate (ADR-032, GPL-clean + pinned) for the RCT selector. **Scenarios arc COMPLETE.** | ✅ | #4 starter-map-library |

**Queue depth (forward READY, non-done): 5** — the **scenarios arc** (`scenarios` → `continue-from-gem` →
`load-gem-replay` → `starter-map-library` [RCT-style selector] → `scenario-gif-preview`). ≥5 ✅. All ✅ hash-neutral.
Grounded in the wave-1+2 research (`proposals/starter-map-research.md` + `starter-candidates.json`). **Right after this
arc → the VISUAL-POLISH epic below** (the user: the screen is "spammed"/cluttered — declutter it).

---

## ▶ VISUAL-POLISH EPIC — leads right after the scenarios arc (the screen is cluttered)

> User brief (2026-06-28): the play screen is "zaspamovaná" (per-organism dot spam) + unreadable. Develop **COLONIES**
> (map polygons that layer better than individual organisms + unify a species/variant; a CRISPR brush edit creates a NEW
> colony — Cities-Skylines DISTRICTS); each zoom scope "pops" a selected colony open to individual organisms by organism
> size; **plants** always-visible + most-realistic, in ≥1 colony. Colonies are an OFF-HASH render aggregation (a per-cell
> variant/colony channel on the snapshot, like `dominant_species_id`) → inv #2/#3, `0x47a0` untouched. Also the LOD lever
> for bigger maps (`[[perf-bigger-maps-needs-structural-change]]`).

- `[x]` **visual-declutter-colony-design** (`workflow`, DESIGN) — DONE → `proposals/visual-declutter-colony-draft.md`
  (470 lines: ADR-029 draft + the airtight off-hash argument — `hash_world` omits `Species`, so a heritable `Variant`
  tag is hash-neutral the same way — + the 6-slice plan). The colony impl slices below come from §7 of the draft.
- `[x]` **S1 `colony-snapshot-channel-impl`** 🛑→**DONE (2026-06-29, ADR-029)** — the off-hash heritable `Variant(u16)` tag +
  `NextVariantId` + `dominant_variant_id` GSS6 channel + brush mint/stamp in `apply_edit_region` + the `snapshot.gd`/byte-gate
  bump (channels 13→14). **Gate GREEN; `0x47a0_3c8f_6701_f240` BYTE-IDENTICAL (NOT a re-pin); 3-skeptic verify 3/3 on all
  five invariant booleans; 187/187 sim-core tests.** Merged `--no-ff`. ADR-029 + CHANGELOG written.
- `[x]` **S2 `colony-polygon-render-impl`** ✅ — **DONE (2026-06-29)** — `godot/colonies.gd`: deterministic row-major union-find CC → marching-squares contour → DP+Chaikin → fill/outline/label; `main.gd` scope-layer swap (Field=polygons, hides dot-spam). **Gate GREEN; zero Rust (literal byte-identical); verify 3/3.** Merged `--no-ff`. *(deferred to S4: hole-cut nested districts; brushed `--shot`.)*
- `[x]` **S3 `lod-pop-impl`** ✅ — **DONE (2026-06-29)** — per-colony footprint (`cell×zoom×size_scale`) pop ladder (closed-form, no per-frame redraw); plants pop first by `size_scale`; un-popped microbe cells emit zero sprites (de-spam holds). **Gate GREEN; zero Rust (literal byte-identical); verify 3/3.** Merged `--no-ff`.
- `[x]` **S4 `brush-colony-binding-impl`** ✅ — **DONE (2026-06-29)** — hole-cut nested family district (`_trace_boundaries`+`_draw_holed_fill`, ±0.09 hue shift) tracking its members via the heritable variant key; renderer-side colony registry; click→`set_selected_colony` select-pop capped (viewport + 700-sprite budget); new headless `colony_s4_test.gd` in the gate. **Gate GREEN; zero Rust (literal byte-identical); verify 3/3.** Merged `--no-ff`. Closes the S2/S3 deferred cosmetics.
- `[x]` **S5 `plant-realism-impl`** ✅ — **DONE (2026-06-29)** — plant always-visible floor (skip haze, `PLANT_GHOST_FILL_FACTOR=0.40`, outline floor), canopy-hull (2× Chaikin + radial green gradient) vs hard microbe district, ≥1-colony guarantee + new `colony_s5_test.gd` in the gate. **Gate GREEN; zero Rust (literal byte-identical); features verify 3/3 (re-verified green on the committed tree).** Merged `--no-ff`. → **visual-declutter epic COMPLETE.**
- `[x]` **S6 `colony-polish-impl`** ✅ — **DONE (2026-06-29)** — perf-lever verified (`colony_s6_test.gd`: Field-scope draw = O(#colonies), 48²=96²=4, 11520× fewer @96²), select-pop cull+700-budget hardened (off-screen never consumes budget), district inspect panel (reuses `_detail_box` naming UI), label declutter (`_label_plan`). **Gate GREEN; zero Rust (literal byte-identical); verify 3/3 (re-verified green on committed tree).** Merged `--no-ff`. → **ADR-029 COLONY EPIC COMPLETE (S1–S6).**

---

## ▶ NEXT PIPELINE (defined; promote when the active queue drains)

**Discovery / ML chain** (precisely-sequenced; `surrogate-model-spec.md`; all ✅ hash-neutral, `crates/discovery`).
**D3-A (eval log) + D3-B.1 (feature encoder) DONE** (`3ad7b9e` / `370d888`). The first batch's **M1 saturation**
empirically validates the drama-weighted target → `discovery-dramaweights-impl` is the **next to promote**:
- `[x]` **discovery-dramaweights-impl** — **DONE (2026-06-30, ADR-033)** — D3-B.2: `DramaWeights {8,4,40,8,32}` (M3+M5=78%) + pure-integer `drama_target` (M6-gated), clean steer/curate separation (Q/curation unchanged). **Gate GREEN; `0x47a0` byte-identical; verify 3/3.** Merged `--no-ff`. Defines the target only (no search-behaviour change yet).
- `[x]` **discovery-ridgeint-impl** — **DONE (2026-06-30, ADR-034)** — D3-B.3: pluggable `Surrogate` trait + `NullSurrogate` base + integer `RidgeInt` (fixed-point GD, zero f64, row-order-independent, recovers planted signal, `RIDGE_BUILD_ID` anchor). **Gate GREEN; `0x47a0` byte-identical; verify 3/3.** Merged `--no-ff`. Not wired into search (D3-B.4 boundary respected).
- `[~]` **discovery-steered-loop-impl** — D3-B.4: **BUILT + GATE-GREEN + VERIFY 3/3, HELD unmerged (2026-06-30, ADR-035).** Opt-in `--steer` surrogate-guided search (oversample→predict→select, 25% un-vetoable explore floor, retrain per gen); `NullSurrogate` base case byte-identical to `discover_evolved`; `0x47a0` byte-identical. On branch `auto/discovery-steered-loop-2026-06-30` (`06e8a7c`), **NOT merged to main.** **✅ Steering target signed off = drama-`D` (user, 2026-06-30); user chose to HOLD the merge + D4 for their own behavioural review** ([[discovery-steering-signoff-hold]]). Do NOT auto-merge or run D4. *dep: ridgeint ✓.*
- `[def]` **discovery-batch-showcase** — D4: night-cron batch (over the named scenario spaces) + a gem-index sidecar + a curated, committed showcase gallery (the replayable gems the player browses). *dep: steered-loop + scenarios; ADR on the steering target.*

**Beta-hardening remainder** (`glmTakeover/` audit folded in; ✅ infra/docs):
- `[x]` **beta-contributing-md** — **DONE (2026-06-30)** — `CONTRIBUTING.md`: the 7 invariants, build/run, the 10-step `tools/gate.sh` table, determinism discipline (hash-neutral vs re-pin), the per-slice loop, the branch→gate→merge-`--no-ff` workflow + conventional commits, the ADR process, the review self-check, licensing (TBD per README). Doc-only; gate unaffected. Merged `--no-ff`.
- `[def]` **slim-hermeticity-impl** — `env_clear()` + `LC_ALL=C` on the SLiM subprocess (oracle golden-file robustness, inv #1-adjacent).
- `[x]` **replay-error-handling-impl** — **DONE (2026-06-30)** — typed `ReplayError` enum (5 variants) over `read_journal`/`replay`/`env_config` + `From<ReplayError> for io::Error` (callers unchanged) + a proptest proving the parse path never panics on corrupt `seed.json`/`actions.ndjson` (18/18 replay tests). `proptest` pre-pinned dev-dep (no new crate). **Gate GREEN; `0x47a0` byte-identical (happy path bit-identical); verify 3/3.** Merged `--no-ff`.
- `[def]` **unsafe-policy-adr** (`direct`) — ADR documenting the `forbid(unsafe_code)` rule + the one `godot-sim` `unsafe impl` exception.
- `[def]` **docs-housekeeping** (`direct`) — delete the stale untracked `docs/llm/weakspots.md` (hallucinates a non-existent Python project) + triage `docs/llm/glmTakeover/`; add `ADR-INDEX.md`.

**⚡ PERF EPIC — worker-thread sim parallelization (user-chosen 2026-06-30; DESIGN DONE → awaiting sign-off):**
> Diagnosis: the sim steps SYNCHRONOUSLY on the Godot main/render thread (`main.gd._process` `for _i in steps:
> _live.step()`) + the world repaints only at the step rate — under load the step+publish hitches the frame; the UI
> isn't parallelized. User chose the full fix (not the band-aid). Design: `proposals/worker-thread-parallelization-draft.md`.
- `[x]` **worker-thread-parallelization-design** (`workflow`, DESIGN) — **DONE (2026-06-30) — sound, ready for sign-off.**
  The worker owns `LiveSim` (sole mutator); main = input+render only; main→worker FIFO `SimCommand` queue; worker→main
  latest-wins `FrameBundle` (Mutex slot, no compute under lock); paced worker, paused parks on blocking `rx.recv()`
  (race-free, no Condvar). **Adversarial review (2 rounds) caught + fixed: 2 determinism off-by-ones (gem-fire
  `gen_abs ≤ G+LIVE_STEP` before advance; immigration `drain(G+1)` after — now byte-match the shipped interleave),
  a read-routing miss, a Condvar lost-wakeup. Re-verify: `determinism_argument_airtight 3/3`, `&mut`-resolved 3/3,
  deps 3/3.** Zero new crates (std-only). ADR-036 draft in §7.
- `[x]` **W1 worker-scaffold-impl** 🛑 — **DONE (2026-06-30, ADR-036, signed-off)** — `crates/godot-sim/worker.rs`: `SimWorker` (owns env, single-mutator) + `SimCommand` + `FrameBundle` + `advance_one_gen` (gem/immigration interleave into Rust, exact shipped predicates) + `recv()`-park. **SCAFFOLD UNWIRED** (`lib.rs` = `mod worker;`; live loop unchanged → W2 wires it). **4/4 worker determinism tests GREEN (incl. the gem+immigration boundary test); `0x47a0` byte-identical (sim-core untouched); zero new crates; gate.sh gained HARD step 4c to enforce the tests.** 3-skeptic verify 3/3. Merged `--no-ff`.
- `[ ]` **W2 main-gd-command-api-impl** (renderer-only) — `main.gd._process`/`_publish_frame` → post commands + read the latest `FrameBundle` each frame at 60 FPS; brush/edit/oversight → `SimCommand`. *dep: W1 ✓ — READY NEXT.*
- `[def]` **W3 lifecycle-impl** — pause/reset/`load_session`/quit clean JOIN + spawn-panic handling. *dep: W1.*
- `[def]` **W4** *(optional)* — presentation interpolation between generations (visual 60 FPS smoothness over a 2 Hz sim). *dep: W2.*

**Polish & QoL:**
- `[x]` **starter-promote-hardening** — **DONE (2026-06-30, ADR-031 trap closed)** — `promote_gen1` RECOMPUTES the gen-1 `source_hash` from an edit-free replay (`build_journal(&[], gens)` → `record_episode` → replay-verified), removing the blind `gem.recorded_hash` copy; `Gen1Starter` gains `gens` + `source_had_edits` (`#[serde(default)]`, committed library still loads). **Gate GREEN; `0x47a0` byte-identical; verify 3/3.** Merged `--no-ff`.
- `[x]` **oversight-ui-polish** — **DONE (2026-06-30)** — the ADR-028 #3-verify follow-ups (renderer-only): q knob defaults to `1000` (wild-type, not `0`/lethal KO; control + both fallbacks agree); due-epoch label → "applied now / effective epoch %d" (immediate-commit honest); oversight re-activates in `_resync_to_live` after `load_session` (both load paths, has_method-guarded). **Gate GREEN; zero Rust (`0x47a0` byte-identical); verify 3/3.** Merged `--no-ff`.
- `[x]` **live-session-sparkline-impl** — **DONE (2026-06-30)** — per-gen effect sparkline (mean fitness over the window after a marker, from `_fit_history`) drawn for the HOVERED marker only (bounded 60×24 card, no re-clutter); `_injections` never mutated. **Gate GREEN; zero Rust (`0x47a0` byte-identical); verify 3/3.** Merged `--no-ff`.

**Sign-off granted 2026-06-28 ("zelená všem blockerům") — but gated by readiness, not approval:**
- 🛑 **R3-F3 resource coupling** — SIGNED OFF, **but still UNDESIGNED** (blocked on the R1.2/R1.3 spatial-`Cell` design
  collision; a re-pin + an ADR-005 change). An undesigned invariant rewrite is NOT auto-run even with sign-off — it needs
  a **design workflow first** (`r3-f3-spatial-cell-design`, to author), then the executed re-pin. Lower priority than the
  scenarios/colony epics; queue the design when those drain.
- 🔁 **Rel-4 sqlite-vec sidecar** — SIGNED OFF; designed; executes when the roster size crosses the trigger (conditional —
  not warranted now).

**🧬 FOUNDATIONAL EPIC — SBOL + BioBricks deep integration (user brief 2026-06-30; DESIGN-FIRST, needs sign-off):**
> *"hluboká integrace s SBOL — nesmí proběhnout proces, který není v tomto jazyce definovaný; promysli i BioBricks
> Foundation přístup."* Make **SBOL (Synthetic Biology Open Language)** the canonical genetic-design substrate with a
> **closed-world** rule: **no genetic process executes unless it is defined as an SBOL construct** (a deterministic
> validation gate in front of genotype→phenotype). Plus the **BioBricks** discipline: standard, characterized,
> composable parts (registry-grounded `BBa_*`) under an assembly grammar → a real synbio sandbox. KEY: the model is
> **already ontology-first** (`Locus.tags.so_term` = Sequence Ontology; real NCBI CDS; `crispr` edits `DnaSequence`) →
> SBOL is a *formalization + gate*, not new biology. Candidate **new invariant (inv #8): the genetic vocabulary is
> closed over SBOL.** Seed: `proposals/sbol-biobricks-integration-draft.md`.
- `[ ]` **sbol-biobricks-integration-design** (`workflow`, RESEARCH+DESIGN) — `.js` authored, READY. Web-research SBOL3
  vs SBOL2 / BioBricks assembly standards / iGEM registry / SBOL-tool licensing (inv #1) → adversarially verify the bio
  claims → expand the seed into a pinned spec + ADR-draft + the inv #8 proposal + the **determinism re-pin plan** for
  SB2. **DESIGN ONLY, doc-only, hash-neutral.** *(Run on the user's go — foundational; folds into the autonomous loop or on request.)*
- `[def]` **SB1 sbol-model-validator-impl** — Rust SBOL3-subset data model (`Component`/`Feature`/`Sequence`/`Interaction`)
  + the in-core well-formedness/role/grammar validator behind the inv #5 trait; `std`+serde (+ a pinned RDF/JSON-LD parser, justified). Hash-neutral (unwired). *dep: design.*
- `[def]` **SB2 genome-sbol-grounding + closed-world GATE** 🔁🛑 — express `Genome`/`Locus` as SBOL Components + the validation gate before genotype→phenotype. **Likely a determinism RE-PIN (STOP-THE-LINE) — needs the design's re-pin plan + multi-ISA gate + sign-off.** *dep: SB1.*
- `[def]` **SB3 biobrick-parts-catalog + assembly-grammar** — registry-grounded standard parts (datasheets via `Parameter`/SBOL `Measure`) + the composition grammar (brush = insert/replace a standard part). Mostly data + grammar. *dep: SB2.*
- `[def]` **SB4 sbol-reference-validator-subprocess** (inv #1/#5) — optional pySBOL3/libSBOLj conformance at the process boundary. *dep: SB1.*
- `[def]` **SB5 sbol-import-export** — round-trip designs to/from SBOL3 documents / SynBioHub. *dep: SB1.*
- `[def]` **SB6 synbio-sandbox-ui** (renderer-only) — compose a species from standard parts (grammar-guided); read the SBOL design in the codex/specimen view. *dep: SB3.* **→ absorbed/refined by the INTERVENTION REWORK epic below.**

**🧩 INTERVENTION REWORK — "BioBlocks" (user brief 2026-06-30; the gameplay payoff of the SBOL foundation):**
> *"rework interventions … příjemné UI, které staví na BioBricks a s možností použít 'připravené' editace z iGEM
> knihovny."* Rework today's low-level tool brush (CRISPR `apply_edit_region` poke-a-locus + the player-snapshot
> Variant Lab) into a **block-based ("BioBlocks") composer** — snap standard part blocks (promoter/RBS/CDS/terminator,
> grammar-guided so only compatible shapes connect = the closed-world *felt*) — PLUS a **library of ready-made iGEM
> `BBa_*` devices** (one-click "připravené" edits). RCT-style browser, datasheets, effect preview, OVERSIGHT credit
> cost by complexity. Renderer-side UI (inv #2) over the SBOL core (parts = SBOL Components SB3, snap-validation =
> SB1 validator, apply = a journaled SBOL-grounded edit). Seed: `proposals/intervention-rework-bioblocks-draft.md`.
- `[x]` **intervention-rework-bioblocks-design** (`workflow`, DESIGN) — **DONE (2026-06-30) — APPROVE, verify 4/4.**
  Expanded `proposals/intervention-rework-bioblocks-draft.md` into a buildable spec + ADR-038 draft: RCT-style composer
  (shape-encodes-SO-role snap canvas over a core `grammar_hints.json`; the authoritative check stays the SBOL
  validator at preview+apply), real `BBa_*` parts by REFERENCE (knockout/toggle `[placeholder]` to resolve at IR1),
  apply desugars to the EXISTING journaled `ApplyEdit`/`ApplyEditRegion` (pinned config byte-identical). **Conditions
  for the impl slices:** IR1 MUST web-confirm iGEM Terms before bundling any sequence (the page 403'd); the whole epic
  gates on the unsigned SBOL foundation. Merged `--no-ff`. (inv #2/#3/#1/SBOL-coherence all 4/4.)
- `[def]` **IR1 igem-library-data** — curate real iGEM `BBa_*` parts + ready devices as data, grounded in SBOL Components (datasheets); inv #1 licensing. *dep: SBOL SB3.*
- `[def]` **IR2 bioblocks-composer-ui** (renderer) — the block snap canvas + assembly-grammar guidance (shape-compatible) + effect preview. *dep: IR1 + SBOL SB1.*
- `[def]` **IR3 ready-edits-library-ui** (renderer) — RCT-style browser of ready devices + the player's saved devices (Variant Lab generalized); one-click apply. *dep: IR1.*
- `[def]` **IR4 apply-device-as-journaled-edit** (core) — device → validated (closed-world) SBOL-grounded journaled edit; OVERSIGHT cost by complexity. Hash-relevant only for device runs (pinned config neutral). *dep: SBOL SB2.*
- `[def]` **IR5 rework-current-tools** (renderer) — migrate `TOOL_CRISPR` onto the composer; reskin the regional operators; Variant Lab → the saved-devices shelf. *dep: IR2+IR3+IR4.*

---

## ▶ LOG (append per item: date · item · PASS/RED · merge sha · note)

- 2026-06-30 — **`beta-contributing-md` DONE** (doc-only). New `CONTRIBUTING.md` codifying the actual workflow: 7 invariants, the 10-step gate, hash-neutral/re-pin determinism discipline, the per-slice loop, branch→gate→merge-`--no-ff` + conventional commits, the ADR process, licensing (TBD). Authored inline (no code → gate trivially green). Merged `--no-ff`.
- 2026-06-30 — **`replay-error-handling-impl` PASS** (beta-hardening, off-hash). Gate GREEN; 3-skeptic verify 3/3; `0x47a0` byte-identical (happy path bit-identical, sim-core untouched). Typed `ReplayError` enum (5 variants) + `From<ReplayError> for io::Error` (callers unchanged) + a proptest proving the replay parse path never panics on corrupt `seed.json`/`actions.ndjson`; `proptest` pre-pinned dev-dep (no new crate, no ADR). Merged `--no-ff`.
- 2026-06-30 — **`starter-promote-hardening` PASS** (off-hash harness tooling). Gate GREEN; 3-skeptic verify 3/3; `0x47a0` byte-identical (sim-core untouched). Closed the ADR-031 gen-1 trap: `promote_gen1` RECOMPUTES `source_hash` from an edit-free replay (no more blind `gem.recorded_hash` copy) + `Gen1Starter` gains `gens`/`source_had_edits` (`#[serde(default)]` → committed library unbroken). 11/11 promote tests. Merged `--no-ff`. ADR-031 marked RESOLVED.
- 2026-06-30 — **`oversight-ui-polish` PASS** (renderer-only). Gate GREEN; 3-skeptic verify 3/3; `0x47a0` byte-identical (zero Rust, `godot/main.gd` only). The 3 ADR-028 follow-ups: q knob default `0`→`1000` (wild-type, fallbacks agree), due-epoch label → "applied now / effective epoch" (immediate-commit honest), oversight resumes in `_resync_to_live` after `load_session`. Merged `--no-ff`. (Also: sent the user sample scenario-GIFs — coexistence/limit-cycle-steady/limit-cycle-crashes — generated from real gems via `make_starter_gif.sh`; `--shot` works headless on macOS.)
- 2026-06-30 — **Discovery/ML chain D3-B.2→B.4.** `discovery-dramaweights-impl` PASS (`83c2614`, **ADR-033** — drama-weighted target `D`, M3+M5=78%, steer/curate separation) · `discovery-ridgeint-impl` PASS (`9518681`, **ADR-034** — integer `RidgeInt` + pluggable `Surrogate` trait, fixed-point GD, zero f64, row-order-independent) · `discovery-steered-loop-impl` BUILT+VERIFIED but **HELD** (`06e8a7c` on branch, **ADR-035** — opt-in `--steer`, NullSurrogate base-case byte-identical). Each gate GREEN + verify 3/3, `0x47a0` byte-identical. **Steering target signed off = drama-`D` (user); user HELD the D3-B.4 merge + D4 for their own behavioural review** ([[discovery-steering-signoff-hold]]). Pivoted to Polish/QoL.
- 2026-06-29 — **🎉 ADR-029 COLONY EPIC COMPLETE (S1–S6) — the whole visual-polish/colony brief shipped.** Six slices, six `--no-ff` merges, the pinned literal `0x47a0_3c8f_6701_f240` **byte-identical throughout** (S1 the 🛑 core slice proven hash-neutral + re-confirmed on `main`; S2–S6 zero-Rust by construction; godot gate re-verified green after S5/S6). **S1** `colony-snapshot-channel` (`2363cb5`) off-hash `Variant(u16)` + `dominant_variant_id` GSS6 channel + brush bind, ADR-029 accepted · **S2** `colony-polygon-render` (`283777d`) `colonies.gd` district polygons = the de-spam · **S3** `lod-pop` (`a153e8c`) per-colony footprint pop ladder, plants pop first, no per-frame redraw · **S4** `brush-colony-binding` (`40a5297`) hole-cut nested family district + colony registry + capped select-pop + `colony_s4_test.gd` · **S5** `plant-realism` (`10eba65`) canopy hulls + always-visible floor + ≥1-colony guarantee + `colony_s5_test.gd` · **S6** `colony-polish` (`2c20bd0`) **perf-lever verified O(#colonies)** (48²=96²=4 districts, 11520× fewer draws @96²) + cull/budget hardening + district inspect panel + label declutter + `colony_s6_test.gd`. Each: gate GREEN + 3-skeptic verify 3/3 (feature booleans), all reviewers APPROVE. Delivers the [[perf-bigger-maps-needs-structural-change]] structural draw-count lever. **Next: promote the Discovery/ML chain — `discovery-dramaweights-impl` (D3-B.2).**
- 2026-06-29 — **#5 `scenario-gif-preview-impl` PASS → SCENARIOS ARC COMPLETE (5/5).** Gate GREEN; 3-skeptic verify CONFIRMED 4/4 at 3/3; `0x47a0` UNMOVED (sim-core untouched; the off-hash `ecology.rs` `detect_events` refactor is byte-identical, scorer 73/73). New `keyframe.rs` (off-hash key-event detector) + `gifenc.rs` (PNG→GIF via the LINKED MIT `gif` 0.13.3 + `png` 0.17.16 + color_quant — GPL-clean per `check_license.sh`, pinned, **ADR-032**) + macOS-safe `make_starter_gif.sh` (timeout+file, no pipe); `*.gif` gitignored. (Fixed a workflow-parse bug — a raw backtick in a prompt — before launch.) Merged `--no-ff`. **Next: S1 `colony-snapshot-channel-impl` 🛑 (signed off) — the visual-polish epic.**
- 2026-06-29 — **#4 `starter-map-library-impl` PASS** (gate GREEN, 10/10 incl. the new GALLERY gate; 3-skeptic verify CONFIRMED 5/5 at 3/3; `0x47a0` UNMOVED; committed library empirically replay-verified). **7 starters shipped** to `data/presets/starters/` (6 gen-1 across the dynamics taxonomy + 1 `branch-point` gen-N checkpoint with a recorded edit) + the RCT selector (`gallery.gd`). **ADR-031**. One non-blocking latent trap (gen-1 drops edits but copies the edited `source_hash` — safe today, hash-neutral edits) tracked as `starter-promote-hardening`. Merged `--no-ff`. Next ready: #5 `scenario-gif-preview-impl`.
- 2026-06-28 — **#3-v2 `discovery-load-gem-replay-impl` PASS** (the RED re-run, fixed). Gate GREEN; 3-skeptic verify CONFIRMED 4/4 at 3/3; edit replay now byte-faithful to `edits_to_actions` (resolution in a read-only core `gem_edit_schedule` #[func] + off-hash `Gem.gens_requested`); `0x47a0` UNMOVED. **ADR-030** appended. Merged `--no-ff`. Next ready: #4 `starter-map-library-impl`.
- 2026-06-28 — **(parallel b) `visual-declutter-colony-design` DONE** (ran concurrently with #3 v2). Delivered `proposals/visual-declutter-colony-draft.md` (ADR-029 draft + 6-slice plan). Headline: colonies are an off-hash heritable `Variant(u16)` tag + a `dominant_variant_id` GSS6 channel (sibling of `dominant_species_id`); the inv #3 case is airtight (`hash_world` omits `Species`, so `Variant` is hash-neutral too; single-plant config → all `Variant(0)` → `0x47a0` byte-identical, NOT a re-pin); brush = a 2-line `ApplyEditRegion` extension (Cities-Skylines districts, survives replay); renderer derives the polygon geometry (inv #2). **S1 `colony-snapshot-channel-impl` flagged 🛑 STOP-THE-LINE** (the only core/snapshot touch — needs human sign-off). Merged `--no-ff`.
- 2026-06-28 — **#3 `discovery-load-gem-replay-impl` RED → v2 fix authored.** Gate GREEN but verify refuted `replays_gem_config_and_edits` 0/3 (config replay sound; EDIT replay diverged from `edits_to_actions`: (1) raw target vs `loci[edit.target % loci.len()].id` → 81/147 edits failed `UnknownTargetLocus`; (2) `gem.gens` vs the unserialized `gens_requested` → wrong gen on early-stopped gems). The gate missed it (the `--gem` smoke reported *dispatched*, not *applied*) — the adversarial verify caught it. WIP preserved on `auto/discovery-load-gem-replay-2026-06-28` (`6e48a35`, NOT merged). **v2 authored** = renderer + a read-only core `gem_edit_schedule` #[func] (resolves via `edits_to_actions`) + off-hash `Gem.gens_requested`; hash-neutral. STOPPED the run (verify-refute) — awaiting human go to re-run v2 (a renderer→renderer+tiny-core re-scope).
- 2026-06-28 — **#2 `discovery-continue-from-gem-impl` PASS** (gate GREEN; verify CONFIRMED, 4/4 at 3/3; `0x47a0` UNMOVED — meta-level; `discover_from_gem` pre-seeds from the gem + branches; children round-trip, stale anchors dropped at write). Merged `--no-ff`. Next ready: #3 `discovery-load-gem-replay-impl`.
- 2026-06-28 — **#1 `discovery-scenarios-impl` PASS** (gate GREEN; 3-skeptic verify CONFIRMED, 4/4 at 3/3; pinned literal `0x47a0_3c8f_6701_f240` UNMOVED — default `--space` path golden-byte-identical; 6 named presets fixed-order/in-bounds/distinct; unknown name degrades with a note). Merged `--no-ff` to `main`. Next ready: #2 `discovery-continue-from-gem-impl`.
- 2026-06-28 — **User brief folded in (scenarios + GIF + RCT selector + visual-polish/colony epic).** Refined `starter-map-library` gallery → RCT-style scenario selector (left list / big right desc + animation + thick scrub slider). Authored `scenario-gif-preview-impl` (auto-GIF of key events; off-hash + macOS-safe + GPL-clean) → active #5. Authored `visual-declutter-colony-design` (DESIGN: colonies as off-hash render aggregation, brush-creates-colony à la Cities-Skylines districts, LOD pop by zoom×size, plants always-visible/realistic; ADR-029 draft) → leads the new VISUAL-POLISH epic right after the scenarios arc. `oversight-ui-polish` → Polish pipeline.
- 2026-06-28 — **Research waves 1+2 + starter-map capstone queued.** Ran 60 evolutionary runs (8 640 configs, 572 verified gems) over the default space. Findings (`proposals/starter-map-research.md`): decomposer keystone (Δqual +303k), a sustainability cliff on long horizons (boom-bust 16%→38%; sustainable core = plant+ecoli), predator regulates not oscillates, edits +quality, M3/M5 discriminate (validates dramaweights). Curated 11 starter candidates → `proposals/starter-candidates.json`. Authored `starter-map-library-impl` (gen-1 + gen-N-checkpoint maps with recorded-intervention timelines) → queued #4 (dep on #2 continue-from-gem + #3 load-gem-replay). `beta-contributing-md` → pipeline.
- 2026-06-28 — **Re-plan #2 @ `main` b865644 → discovery/auto-research lead.** First brute-force batch validated the pipeline (21 verified gems, ~60s/run, edit axis produced the #1 gem, 19/21 distinct shapes, M1 saturates). Authored 3 discovery-research workflows (`discovery-scenarios-impl`, `discovery-continue-from-gem-impl`, `discovery-load-gem-replay-impl`) → READY; active queue rebuilt (5 READY: 3 research + `oversight-ui-polish` + `beta-contributing-md`). `discovery-dramaweights-impl` flagged next-to-promote (M1-saturation-validated). The 5 completed gameplay items are in the entries below.
- 2026-06-28 — **#5 `sandbox-load-starter-impl` ALREADY SHIPPED** (no new merge). The feature landed earlier in `597a8d4` (`main_menu.gd:295-365`). Workflow VERIFIED the as-committed impl: gate GREEN; verify 4/4 at 3/3; `data/presets` res:// staged + byte-gated; `0x47a0` unmoved.
- 2026-06-28 — **#4 `codex-browse-panel-impl` PASS** (gate GREEN, `CODEX MIRROR/INSPECT OK`; verify 4/4 at 3/3; ZERO Rust — `0x47a0` byte-identical; reuses `codex.gd`). Merged `1ba13b8`.
- 2026-06-28 — **#3 `oversight-ingame-ui-impl` PASS** (gate GREEN; verify 5/5 at 3/3; `0x47a0` unmoved on no-commit, a committed edit moves it deliberately + replays byte-equal). **ADR-028** appended. Merged `b4e368f`. UX follow-ups tracked as `oversight-ui-polish`.
- 2026-06-28 — **#2 `variant-lab-autoresearch-edits` PASS** (Variant Lab D; gate GREEN; verify 5/5 at 3/3; `0x47a0` UNMOVED — `edit_budget` default-0 + disjoint `EDIT_SALT`; edited gems round-trip). **ADR-027**. Merged `7fb3150`.
- 2026-06-28 — **#1 `variant-lab-save-reseed` PASS** (gate GREEN; verify 5/5 at 3/3; `0x47a0` UNMOVED — read-only export + renderer save/reseed). Merged `5f43c28`.
- 2026-06-27 — QUEUE seeded (gameplay/sandbox lead). `beta-license-dual` done (`8415199`).
