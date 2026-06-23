# Midnight Review — Sandbox / Contamination UI, Pre-Manual-Testing

> Status: review draft, **not committed**. Source: VERIFIED findings (4 reviewers, adversarially re-checked
> with the live tree). Scope: the SP-2 sandbox composer + SP-3 intervention panel + S3 contamination UI,
> as the tester would touch them. Determinism/gate confirmed by running the core suites (see §5).

---

## 1. VERDICT

**The sandbox/contamination UI is ready for a *single-session, live* manual test — but do NOT save a
contaminated / multi-species / non-default-species run and reload it, and do NOT trust `--replay` on
those runs.** Everything you compose and do *within one live session* (compose roster → run → all 6
tools → containment → seed a contaminant → cull → spores regerminate → symbiont-onto-host via test)
behaves correctly and conserves joules deterministically. The **one true blocker is replay/load
divergence** (R2): save/load and `--replay` silently drop the roster, the selected species, and the
registered contaminant consortium, so a saved contaminated or multi-species session reloads to a
*different* hash — an invariant #3 break. Until that is fixed, the tester must treat **Load and
`--replay` of any non-default-plant session as broken**, not test them, and not file the resulting
divergence as a surprise. Live composition and intervention are good to go.

---

## 2. 🔴 MUST-FIX BEFORE TESTING

### R2 — Save/load + `--replay` drop the roster, selected species, and consortium → different hash (inv #3)
**`crates/harness/src/replay.rs:58-105, 153-160, 227-233`** · **`crates/godot-sim/src/lib.rs:1008-1015` (load), `976-988` (save)** · **`crates/harness/src/lib.rs:641-643`**

`SeedJson`/`EnvConfig` persist only `seed + entity_count + climate`. Both the harness replay paths
(`run_episode`, `replay()`) and the godot-sim `load_session`/`save_session` rebuild
`GeneSimEnv::new(entity_count)` → `set_environment` → `reset` → replay actions, and **never** call
`set_roster` / `set_species` / `register_contaminant` / `set_containment`. So on replay the env has an
**empty consortium**, and a journaled `RegionInoculate` resolves its `species_key` against nothing and
hits the clean NO-OP at `crates/harness/src/lib.rs:641-643` (`let Some(built) = resolved else { … return; }`)
— minting and spawning **nothing**. In the live path the UI registers the key first
(`godot-sim lib.rs:643`) so the same action *does* spawn; on replay it does not → `run_stats().hash`
diverges. Same divergence for a multi-species roster (never re-applied) and a non-plant selected species.

**Why the test suite misses it:** the only replay-ish inoculate test
(`region_inoculate_conserves_j_and_is_replay_reproducible`, `harness/src/lib.rs:1486-1517`) calls
`env.register_contaminant()` *inside* its run closure on every invocation (line 1493) and re-runs that
same in-process closure for the "replay" assert (1512-1516). It passes precisely *because* it manually
re-registers; it never crosses the file-based `record_episode → replay` boundary or the
save→reload-without-reregister path. 169 sim-core + 69 harness tests green, **none** cover a
resolved-`RegionInoculate` or a roster across the real replay boundary.

**Fix (pick the persistence cut):** extend `SeedJson`/the saved session to also persist the roster, the
selected species, the registered consortium (the contaminant keys + endowments), and the containment
setting; on `run_episode` / `replay()` / `load_session`, re-apply them (`set_roster` /
`register_contaminant` / `set_containment`) **before** replaying the journal. Then add a real
file-boundary round-trip test: record a contaminated multi-species episode → replay from disk →
assert identical `hash` (and a save→reload-without-manual-reregister variant).

**Tester instruction until fixed:** stay in one live session. Do not exercise Save→Load or `--replay`
on contaminated / multi-species / non-default-species runs.

---

## 3. 🟡 NICE-TO-FIX (quick wins, none block a live session)

| # | Finding | File:line | Severity | Quick fix |
|---|---------|-----------|----------|-----------|
| R1 | Menu Open/Clean/Lab containment is a **silent no-op** — `_apply_menu_containment` always pushes an *empty* consortium (`PackedStringArray()`), and the core's `expand_schedule` returns `Vec::new()` for `n_species==0` regardless of level. Selecting "☣ Open" in the main menu + START schedules **zero** immigration. Documented as intended (the in-run CONTAMINATION panel is the consortium surface), but it's a menu dead-end. | `godot/main.gd:597` (core: `crates/sim-core/src/immigration.rs:185`) | major (downgraded from blocker — documented design choice, not a determinism break) | Seed `default_mode_a`'s keys when `level>0`, **or** disable/grey "Open" in the menu with a hint pointing at the in-run panel. |
| R1/R4 | Consortium checkboxes + Inoculate brush expose **only 2 of 7** baked airborne contaminants (and 0/2 symbionts): `CONTAINMENT_KEYS := ["mycoplasma","bacillus"]` is the sole source. 7 airborne Mode-A specs exist on disk; the docstring ("Discovered at UI build so a new bake lights up automatically") **lies** — the list is hardcoded. Also disagrees with the core default (`default_mode_a` = bacillus/pseudomonas/aspergillus-niger). `register_contaminant_json`/`inoculate` accept any key, so it's purely the GDScript constant. | `godot/main.gd:225` | major | Replace the constant with a real `res://data/species/` directory scan (filter to Mode-A airborne role), or at minimum extend the list to the 7 baked keys + fix the docstring. Hash-neutral GDScript. |
| R1 | Inoculate is the **6th** palette tool but stale prose still says "5 tools" (`main.gd:183, 205, 206, 750, 761`). Wiring is correct (`TOOL_INOCULATE:=5`, 6 `TOOL_KEYS`, `_tool_panels.resize(6)`, dispatch at 1251-1255). | `godot/main.gd:750` | minor (comment drift only) | s/5/6/ in the five comment sites. |
| R1 | INTERVENE (`Vector2(maxf(240,fs.x-290), 70)`) and CONTAMINATION (`…, 320`) dock at the **same x, 250px apart** — a tall INTERVENE body (6-button palette + CRISPR params + brush + wrapped status) can overlap the CONTAMINATION title bar on the 820×680 floor window. Both are draggable PanelChrome so it's recoverable. | `godot/main.gd:887` | minor (speculative — geometry verified, render height not measured) | Increase the gap, or anchor CONTAMINATION below INTERVENE's measured height. |
| R2 | PCR/Antibiotic picker **defaults to species 0** when unselected and preserves selection by *positional index*, not species_id/key — a reordered roster silently retargets. Core still does a conserved, journaled op vs species 0 and reports its verdict, so the tester sees *what fired*, just not a target mismatch. | `godot/main.gd:1085, 1101-1104` | minor (mistarget, not a determinism break) | Preserve selection by species_key; require an explicit pick (no implicit 0) or show the resolved target in the status line. |
| R1/R4 | Timeline intervention markers **vanish on save→load**: `_rebuild_markers_from_journal` probes `_live.has_method("journal_actions")` but **no such export exists** in `godot-sim/src/lib.rs`, so `_injections` stays `[]`. State still replays correctly (rebuilt from seed+journal); only the *visual* markers are absent. Already on the NEXT-SESSION list; GDScript documents the graceful degradation. | `godot/main.gd:2042` | minor (cosmetic, documented deferral — **see §4 known-deferred**) | Export `journal_actions` from godot-sim; the probe lights up automatically. |
| R2 | Symbiont host must be **registered before** the symbiont (`register_species` resolves host once at registration, `lib.rs:2354`; never re-resolved). Inoculating carsonella before its host = a clean total no-op (`lib.rs:2457-2459`). Order-dependent usability trap, deterministic (no J minted). | `crates/sim-core/src/lib.rs:2354, 2457-2459` | minor | Re-resolve host lazily at inoculation, or surface the ordering constraint in the UI/error. Only bites a hand-fired Mode-B inoculation. |
| R2 | Airborne symbiont block keys on **hardcoded** `role_for(key)` (`gp.rs:385-392`), not the JSON-declared `niche.trophic_role`. The only 2 baked symbionts *are* in `role_for` so it works today; a future symbiont under a non-carsonella/syn3 key would leak. Backstopped by the `region_inoculate` host-presence gate. | `crates/sim-core/src/immigration.rs:178-182` | minor (latent) | Read `trophic_role` from the JSON niche (the `role_from_override` data path) instead of the hardcoded match. |
| R1 | Symbionts (carsonella, syn3) are baked but reachable from **no UI surface** (`main_menu.gd ALL_SPECIES:17-28` omits both; `CONTAMINANT_KEYS` has neither). Correctly excluded from airborne immigration (a symbiont can't airborne-arrive), but no host-coupled UI entry point either. Exercisable only via `cargo test` (`s5_*`). | `godot/main_menu.gd:17` | minor (deferred content — **see §4**) | Add a host-coupled inoculate entry point when symbiont gameplay is in scope; out of scope for this session. |

---

## 4. ✅ WHAT-TO-TEST + WHAT-TO-WATCH-FOR

### Happy-path flows (all live, single session)

1. **Compose a roster.** Main menu → pick multiple species (plant + E. coli + Bdellovibrio + any
   reachable contaminant) with per-species starting pops, set climate + seed + ContainmentLevel → START.
   - **Correct:** the run starts with the composed populations; the per-species panels light up for
     each species (population/allele/mean-energy), not just the primary.

2. **Run.** Let it advance; UI stays responsive (the live loop is decoupled).
   - **Correct:** populations move on the conserved joule economy. Plant ↔ E. coli decomposer ↔
     Bdellovibrio predator should show *dynamic* (not flat, not instantly collapsed) coexistence with
     legible oscillation.

3. **Each of the 6 intervention tools** (CRISPR, PCR amplify, Antibiotic cull, Nutrient feed, Toxin
   spike, Inoculate). Pick a tool, set params, brush onto a region — **position matters**.
   - **Correct:** every action is journaled, conserved (J from/into a named ledger tap), and the status
     line reports the core's verdict. A timeline marker appears for each action *in the live session*.
     PCR makes faithful local clones; Antibiotic cull turns killed orgs' biomass into detritus;
     Nutrient/Toxin inject into the pool/F5 field at the brushed cell.

4. **Containment (in-run CONTAMINATION panel).** Compose a consortium (the checkboxes), set the level,
   confirm scheduled immigration arrives over time.
   - **Correct:** with a non-empty consortium + level>0, immigration events fire on schedule and show as
     timeline markers; establish-vs-displace-vs-die-out **emerges** from the joule economy (it is *not*
     scripted — a contaminant that can't pay maintenance dies out; that is correct, not a bug).
   - **Watch:** only mycoplasma + bacillus are offered (R1/R4) — that's the known 2-of-7 limit, not a
     missing bake.

5. **Seed a contaminant** (Inoculate brush) at a region → **cull it** (Antibiotic tool) → confirm the
   carcass→detritus flow and the population drop.
   - **Correct:** inoculate spawns orgs and mints immigration J; cull conserves (biomass → detritus pool),
     and the FlowMatrix/pools reflect it.

6. **Spores regerminate.** Drive a spore-former into sporulation (starvation), then restore conditions.
   - **Correct:** `germinate` withdraws exactly Σ from the spore bank (conserved), orgs re-appear; the
     run stays deterministic.

7. **Symbiont onto host** — *test-only this session.* The interactive "inoculate carsonella/syn3 onto
   its co-located host" flow has **no UI surface** (R1). Exercise it via `cargo test -p sim-core` (`s5_*`).
   - **Correct:** host→symbiont parasitic DRAW couples only when the host was registered *first* and is
     co-located; otherwise a clean no-op (R2 ordering).

### Known-deferred — DO NOT file these as bugs

- **SP-4 codex is absent.** No `data/codex/` dir; `run.sh` stages only species. Documented, deferred
  (gate-RED last session). Not introduced by this work.
- **Predator (Bdellovibrio) overshoot-and-crash / self-extinction is CORRECT emergence**, not a bug —
  it's open-system dynamics on the conserved ADR-013 joule economy (§0.6). The persistence/limit-cycle
  work is a separate SP-1 item.
- **Loaded-session timeline markers are pending** (R1/R4 — `journal_actions` export not yet landed).
  A loaded session **replays correctly**; only the *visual* markers are missing. Documented graceful
  degradation.
- **Save/Load + `--replay` on contaminated/multi-species/non-default runs** — **blocked by R2 (§2).**
  Don't test it; the hash divergence is the known blocker, not a new finding.
- **Menu "Open/Clean/Lab" with no in-run consortium = no immigration** (R1) — the menu sets only the
  *level*; compose the consortium in the in-run CONTAMINATION panel.
- **Single-species harness CLI** (no `--roster` flag) is a design boundary — the multi-species sandbox is
  reached through the Godot UI, by design.
- **Bidirectional/mutualistic symbiont provisioning** is an intentional v1 scope cut (parasitic-drain
  only); a documented future S5b stretch.

---

## 5. STATE — determinism / gate confirmation

- **Core suites green (re-run this pass):** `cargo test -p sim-core --lib` → **169 passed, 0 failed**
  (`determinism_hash_is_pinned` ok); `cargo test -p harness --lib` → **69 passed, 1 ignored**
  (the ignored campaign test predates this work, unrelated).
- **Pinned literal intact:** `0x47a0_3c8f_6701_f240` at **`crates/sim-core/src/lib.rs:3227` and `:3391`**.
  All the new contamination/sandbox mechanics (immigration tap, intervention tap, host_coupling,
  germinate, sporulation, region_inoculate) are gated on non-default state (Sealed knob / no contaminant /
  no symbiont-spore-former registered), so the all-zero pinned plant run is **byte-identical** — this work
  is **hash-neutral**.
- **Note on "0x64a3":** a `0x64a3` literal mentioned in a skill *description* does **not** match the live
  tree (which pins `0x47a0…`). It was correctly **rejected** — do not treat it as the current literal.
- **Not independently re-run this pass:** the full `tools/gate.sh` (fmt/clippy/oracle/livesim/godot-reader,
  SKIP-multi-ISA / SKIP-bench lines). Reviewer 4's "gate fully green" readiness note is corroborated by the
  green core suites + intact literal but not independently re-confirmed end-to-end. **Run `tools/gate.sh`
  green before committing the R2 fix.**

---

### TL;DR
1. **Ready for a single LIVE session** — compose → run → all 6 tools → containment → inoculate → cull →
   spores → (symbiont via test) all work and conserve J deterministically.
2. **One blocker (R2):** Save/Load + `--replay` drop the roster/species/consortium → contaminated &
   multi-species sessions reload to a *different* hash (inv #3). Don't test reload until persistence is
   fixed (`replay.rs` + `godot-sim/src/lib.rs` re-apply roster/consortium/containment before replaying).
3. **Core is solid:** 169+69 tests green, literal `0x47a0_3c8f_6701_f240` intact, this work hash-neutral;
   the rest is GDScript nits (2-of-7 contaminants, "5 tools" prose, panel overlap, picker default) +
   known deferrals (SP-4 codex, predator crash = correct, loaded-session markers).
