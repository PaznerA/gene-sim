# Worker-thread sim parallelization — verdict: `std-channel-mutex` (DRAFT)

> **DESIGN ONLY — no production code; awaiting human sign-off.**
>
> **Status:** DRAFT. Synthesis of three design lenses (`std-channel-mutex`, command-queue ownership,
> read-copy handoff). **Touches inv #3 (determinism) at the W1 boundary slice → STOP-THE-LINE-adjacent:
> W1 cannot land without the determinism guard green + human sign-off.** W2–W4 are renderer-only.
> **NOT a re-pin.** The pinned literal `0x47a0_3c8f_6701_f240` (asserted at `crates/sim-core/src/lib.rs:3544`
> and `:3708`) is the oracle that **stays byte-identical**. If any slice moves it, that slice is a bug and is reverted.
>
> **Dep stance:** **ZERO new crates.** std `mpsc` (incl. blocking `recv()` for the paused park — no Condvar) + `Arc<Mutex<_>>` + `thread` only. No `Cargo.toml`
> change, no new pinned version (inv #5 / inv #7). A render-only dep (e.g. `crossbeam-channel`) was **evaluated and
> rejected** — see §INV-AUDIT.

---

## 1. PROBLEM — the single-thread ceiling the decoupled loop already hit

The current live loop (`godot/main.gd::_process`, lines 1031–1050) is the **decoupled-single-thread** design from
`live-ui-parallelization-draft.md` (already shipped): one main-thread per-frame callback does (a) input, (b) a
bounded step loop, (c) a throttled `_publish_frame()`. Everything — sim step **and** the heavy
`snapshot()`+`observe()`+`observe_species()`+`flow_matrix()`+`species_relations()` projection — runs on Godot's
main thread:

```gdscript
for _i in steps:                      # MAX_STEPS_PER_FRAME = 64 cap (main.gd:102)
    _fire_due_gem_edits()             # gem edits at the TOP of the gen (BEFORE the advance)
    _live.step(LIVE_STEP)             # LIVE_STEP = 1 — one whole generation
    _fire_due_immigration()           # drain the deterministic immigration schedule due this gen
if steps > 0 and _render_carry >= 1.0 / RENDER_HZ:   # RENDER_HZ = 30
    _publish_frame()                  # the HEAVY work: snapshot()+parse+observe()+redraw
```

That draft's own risk list flagged the wall this proposal removes:

> *"Single thread throughput ceiling: if one simulation step ever exceeds the per-frame time budget … throughput
> falls … a worker thread would then be warranted."*
> *"If the publish-frame helper is later moved onto a thread it reintroduces the aliasing hazard, so guard it with a comment."*

That comment exists today (`main.gd:1053–1055`): *"Main-thread only: a future worker-thread migration would
reintroduce the `&mut` aliasing hazard (every `LiveSim` method is `&mut self`)."* **This proposal is that migration**,
done the safe way — by **ownership, not locking**.

### The goal — a steady ≥30 FPS UI, ≥60 FPS achievable, decoupled from sim speed

| | Today (main-thread) | Target (worker) |
|---|---|---|
| **Sim step** | on main thread, capped at 64/frame; backlog dropped | on the worker, self-paced; never blocks the UI |
| **Heavy snapshot+observe** | on main thread, throttled to 30 Hz but **stealing input-frame time** | on the worker; main only **clones the latest bundle** |
| **UI / brush / camera** | shares the frame budget with the sim+snapshot | gets the **whole** main-thread budget |
| **Frame rate** | drops when the sim is fast or the world is large | **≥30 FPS floor / ≥60 FPS** independent of sim speed |

This is **not** sim-step parallelism. ADR-020 / `[[perf-bigger-maps-needs-structural-change]]` already proved rayon
inside the tick does not pay; the step **stays single-threaded**. The only thing we move off the
input/render thread is *where* the single-threaded step + the heavy read-only projection run.

---

## 2. ARCHITECTURE — `std-channel-mutex`, ownership resolves the `&mut` hazard

```
  GODOT MAIN THREAD                                    OWNED WORKER THREAD
  ┌───────────────────────────┐                       ┌──────────────────────────────┐
  │ LiveSim (RefCounted)       │   SimCommand (mpsc)   │ SimWorker (plain Rust)        │
  │  cmd_tx: Sender ───────────┼──────────────────────▶│  rx: Receiver<SimCommand>     │
  │  frame: Arc<Mutex<Option   │◀───── *slot = bundle ─┤  env: GeneSimEnv  (SOLE owner)│
  │          <FrameBundle>>>   │   (latest-wins swap)  │  journal, gem_schedule        │
  │  worker: JoinHandle        │                       │  slot: Arc<Mutex<Option<…>>>  │
  │  last_frame: Option<…>     │  reply: sync_channel  │  loop { drain → step → publish}│
  │  + config-builder state    │◀── ActionOutcome ─────┤                               │
  └───────────────────────────┘  (rendezvous one-shot)└──────────────────────────────┘
        NO Godot type ever crosses the boundary — all payloads are plain Send Rust.
```

### 2.1 The worker OWNS the env — the `&mut` hazard is dissolved, not locked

Every `LiveSim` `#[func]` is `&mut self`; that is the aliasing hazard the current comment avoids. The fix is
**single ownership**: the `GeneSimEnv` moves into `SimWorker` on spawn and **never leaves**. The worker is the
**sole mutator**. The main thread holds **no reference** to the env — it holds a `Sender` (to ask for mutations)
and an `Arc<Mutex<Option<FrameBundle>>>` (to read a *copy* of results). There is no shared `&mut`, so there is
nothing to alias. `GeneSimEnv` is `Send` (bevy_ecs `World` is `Send`; `ChaCha8Rng` is `Send`; all fields are plain
owned types — the only `*const` in sim-core is a *local* pointer-comparison in `par.rs`, the abandoned-rayon
experiment, never a stored field, so it does not affect `Send`).

### 2.2 The command queue (main → worker) — covers EVERY current `_live.*` mutation

`std::sync::mpsc::Sender<SimCommand>` (worker is the single consumer). Replies/acks are
`mpsc::sync_channel(0)` rendezvous one-shots (std-only — no extra crate). The enum is the **complete** set of
mutating/blocking `#[func]`s on `LiveSim` today:

```rust
// crates/godot-sim/src/worker.rs  (NEW module; pure Rust, NO `godot::` imports)
enum SimCommand {
    // ── pacing (the speed slider becomes a command; the worker self-paces) ──
    SetSpeed(u32),                 // generations/sec target; 0 = manual (advance only on Step)
    Pause, Resume,
    // ── deterministic advance (scripted / gem fast-forward keep their SYNCHRONOUS contract) ──
    Step { n: u64, ack: SyncSender<()> },           // ⇐ _live.step(n)
    // ── journaled WRITE actions — each carries a one-shot reply for the UI toast ──
    Apply { action: Action, reply: SyncSender<ActionOutcome> },
        //  covers, by Action variant, EVERY mutating #[func]:
        //    apply_edit            → Action::ApplyEdit
        //    apply_edit_region     → Action::ApplyEditRegion
        //    inoculate             → Action::RegionInoculate
        //    pcr_amplify           → Action::RegionPcrAmplify
        //    cull                  → Action::RegionCull
        //    nutrient              → Action::RegionNutrient
        //    toxin                 → Action::RegionToxin
        //    commit_ecoli_edit     → Action::RequestEcoliEdit + Action::CommitEcoliImpact (the journaled pair)
    RegisterContaminant(BuiltSpecies),               // ⇐ register_contaminant_json (JSON parsed on MAIN, BuiltSpecies moved over)
    SetContainmentLive(ContainmentLevel, ConsortiumConfig),  // ⇐ set_containment (live)
    ArmGemSchedule(Vec<ResolvedGemEdit>),            // ⇐ the gem schedule (resolved by the existing read-only gem_edit_schedule)
    // ── blocking queries that take args (can't be precomputed into the bundle) ──
    RegionAllele   { region: Region, w: u32, h: u32, reply: SyncSender<RegionAlleleDto> }, // ⇐ region_allele
    PreviewEcoliEdit { species: u16, q: u16, reply: SyncSender<PreviewDto> },              // ⇐ preview_ecoli_edit
    // ── session ──
    SaveSession { dir: String, reply: SyncSender<Result<(), String>> },   // ⇐ save_session
    Shutdown,                                                             // ⇐ quit / reset / load (respawn)
}
```

**Read-only `#[func]`s do NOT need commands.** They split two ways:
- **Per-frame projections → precomputed into `FrameBundle`** (worker builds them once per publish):
  `snapshot`, `observe`, `observe_species`, `flow_matrix`, `oversight_state`, and the *raw*
  `species_signatures()` (the main thread then runs the **std-only** `relations_index::InRustIndex` k-NN that
  `species_relations` runs today — no env access needed, just the signature bytes).
- **Static / config reads → served on the MAIN thread from binding state** (they never touch the live env):
  `cas_variants` (static `crispr::default_cas_variants()`), `loci` (reads `self.species`/`sample_genome()` — the
  binding's config), `species_key`, `preview_climate`, `gem_edit_schedule` (pure resolution, read-only), `entity_count`.
  These stay exactly where they are.
- **TWO reads the first draft mis-filed here (the adversarial review caught both) — they DO touch the live env, so
  they move:** `export_species_json` (`godot-sim/src/lib.rs:694` → `env.export_species_json` — the LIVE, post-edit
  env) becomes a **blocking worker query command** (like `RegionAllele`/`PreviewEcoliEdit`: send + reply-rendezvous),
  OR a `FrameBundle` field if it's wanted every publish. `is_ready` (`:1201`, reads `self.env.is_some()`) becomes a
  **main-side `ready: bool` flag** set after the spawn rendezvous (the env no longer lives on main, so `self.env`
  is gone). Neither can be "served from binding state."

### 2.3 The snapshot handoff (worker → main) — latest-wins read copy

```rust
struct FrameBundle {              // OFF-HASH read-only projection, built once per publish, swapped atomically
    generation: u64,
    snapshot: Vec<u8>,            // GSS6 bytes for LIVE_GRID  — the HEAVY work, now off-main
    observe: ObservationDto,      // {generation, population, allele_freq, phenotype}
    species: Vec<SpeciesObsDto>,  // observe_species()
    flow: (u32, Vec<i64>),        // flow_matrix()
    signatures: SignaturesDto,    // species_signatures() raw → main runs the std-only relations k-NN
    oversight: OversightDto,      // oversight_state()
    immigration_fired: u32,       // scheduled events fired since last publish (timeline markers)
    gem_fired: Vec<GemFiredDto>,  // resolved edits fired since last publish (toasts/markers)
}
```

The worker builds the **whole** bundle, then takes the `Arc<Mutex<Option<FrameBundle>>>` lock only to
`*slot = Some(bundle)` — **no compute under the lock**. The main thread, each frame, takes the same lock, `take()`s
or clones the bundle out, and caches it in `last_frame`. **Latest-wins**: if the worker publishes 5 bundles between
two render frames, the main thread sees only the newest — exactly the throttle `_publish_frame` does today, now free.
Atomic swap ⇒ no torn reads. The main thread **never blocks on the sim**; the worst case is it re-displays
`last_frame` for one frame.

### 2.4 Pacing — the worker self-paces, the speed slider is a command

The speed slider (`main.gd:3360`, `_steps_per_second`) becomes `SetSpeed(gens_per_sec)`. The worker:
1. **drains every pending command** at the generation boundary via `try_recv()` (FIFO — deterministic apply order),
2. **when idle/paused, parks on blocking `rx.recv()`** — NO Condvar. **(Review finding: the earlier Condvar +
   notify-on-enqueue had a classic LOST-WAKEUP — "no pending command" is `mpsc` queue state, NOT under the
   Condvar's mutex, so a `send`+`notify` landing between the worker's `try_recv` predicate check and `wait()` is
   lost → the worker parks forever → a later `Shutdown` notify is lost in the same window → `join()` hangs.)** std
   `mpsc::Receiver::recv()` has **no lost-wakeup**: a command queued *before* `recv()` is still returned. So the
   park is simply: while paused (speed 0 / `Pause`), block on `rx.recv()` — the next command (incl. a blocking
   `Apply*`/`Step{ack}`/`RegionAllele`/`PreviewEcoliEdit`/`SaveSession`/`export query`, `Resume`, or `Shutdown`)
   wakes it cleanly, it applies at the gen boundary, replies, and re-parks. No notify rule, no deadlock, no spin.
3. runs **one orchestrated generation** (`advance_one_gen`),
4. **publishes** at the render cadence (publish every `ceil(speed / RENDER_HZ)` gens — the same decoupling as today),
5. sleeps to hit the target gens/sec (when running; a paused worker is blocked in `recv()`, consuming no CPU).

```rust
fn advance_one_gen(env, journal, gem) {       // the ordering MOVES from main.gd::_process INTO Rust
    let g = env.generation();                  // == G now; this call advances to G + LIVE_STEP
    // (1) GEM EDITS fire BEFORE the advance, at the SHIPPED threshold: gen_abs in (G, G+LIVE_STEP],
    //     i.e. gen_abs <= G + LIVE_STEP — byte-for-byte main.gd::_fire_due_gem_edits (`due = observe().generation
    //     + LIVE_STEP`; the gem schedule is gen_abs-SORTED → a single forward pointer, NOT a per-gen rescan).
    while let Some(e) = gem.peek() {
        if e.gen_abs > g + LIVE_STEP { break; }
        let a = Action::ApplyEdit(e.into());
        env.step(a.clone()); journal.push(a);
        gem.advance();                         // forward pointer (matches `_gem_schedule_idx += 1`)
    }
    env.step(Action::Advance(LIVE_STEP));       // ONE whole generation; env.generation() is now G + LIVE_STEP
    journal_advance(journal, LIVE_STEP);        // journal_advance coalesces (unchanged)
    // (2) IMMIGRATION drains AFTER the advance, at the SHIPPED bound: drain_due_inoculations(current_gen + 1)
    //     where current_gen == env.generation() == G+1 (POST-step) → drains due_epoch < G+2 (i.e. <= G+1) —
    //     byte-for-byte godot-sim::fire_due_inoculations (`drain_due_inoculations(current_gen + 1)`, current_gen
    //     read AFTER the step) + harness::drain_due_inoculations (`due_epoch < up_to_generation`).
    for a in env.drain_due_inoculations(env.generation() + 1) {  // = _fire_due_immigration
        env.step(a.clone()); journal.push(a);
    }
}
```

**The two gen-boundary predicates are LOAD-BEARING (the adversarial review caught both as off-by-one in the first
draft).** They must be byte-for-byte the shipped GDScript interleave: gem edits at `gen_abs <= env.generation() +
LIVE_STEP` *before* the advance (`main.gd:1219`), immigration at `drain_due_inoculations(env.generation() + 1)`
*after* the advance (`godot-sim/src/lib.rs:972-973`, `current_gen` read post-step; `harness` drains
`due_epoch < up_to_generation`). Getting either wrong shifts the generation a scheduled edit/arrival lands on and
**moves `0x47a0_3c8f_6701_f240` for any gem-armed or scheduled-immigration run** (the pinned single-plant config has
neither, so it would stay green — which is exactly why §3's determinism test MUST drive both paths).

Moving the `_fire_due_gem_edits → step → _fire_due_immigration` interleave out of GDScript and into
`advance_one_gen` **centralizes the deterministic action ordering in Rust** — a net inv #2 improvement (less
orchestration in GDScript) and a guarantee that the manual `Step{n}` path and the paced path emit the **identical**
journal. Once it lands, `fire_due_inoculations`/`fire_due_gem_edits` are no longer driven from GDScript — remove the
orphaned `#[func]` entry points so there is no divergent out-of-band path.

---

## 3. §DETERMINISM — the airtight argument that the threaded run is byte-identical

**Claim:** the threaded run produces the **exact same journal and the exact same `0x47a0_3c8f_6701_f240`** as today's
single-thread loop. Four independent reasons, each sufficient:

**(D1) The pinned literal is computed by a code path this design never touches.** The hash is produced by
`crates/sim-core` `run_headless(&cfg)` (`lib.rs:3544`) and the `Simulation` reproducibility test (`lib.rs:3708`) —
neither involves the renderer, threads, channels, or `GeneSimEnv`'s renderer wrapper at all. The worker calls the
**same** `env.step(Action::…)` methods in the **same** order the GDScript loop calls them. The sim core is
unchanged; the gate's determinism oracle is structurally immune.

**(D2) No wall-clock ever enters the sim.** Time advances **only** by `Action::Advance(LIVE_STEP)` — a fixed integer,
exactly as today (`main.gd` cadence rule, inv #3). The worker's `sleep()` and the `SetSpeed` target choose *how many*
generations run per second; they **never** choose the *content* of a generation and never feed a delta into the sim.
Identical to the current `_steps_per_second * delta → int(steps)` accumulator, which is likewise pure pacing.

**(D3) FIFO commands at the same generation boundary = the same interleave.** The worker drains all pending commands
at the gen boundary **before** the advance, in `mpsc` FIFO order — the same point and order the GDScript loop applies
a brush click (between whole `step()` calls) and fires gem/immigration edits. An `Apply{action}` is `env.step(action)`
+ `journal.push(action)` — byte-identical to the synchronous `apply_edit`/`inoculate`/… path. Single producer (main)
+ single consumer (worker) + FIFO ⇒ a total order on writes identical to the single-thread call order. The
brush-between-whole-gens contract is preserved because commands are only applied at gen boundaries, never mid-step.

**(D4) The snapshot is a READ copy — it cannot perturb the stream.** `FrameBundle` is built from `snapshot()` /
`observe()` / `observe_species()` / `flow_matrix()` / `species_signatures()` / `oversight_state()` — all already
proven OFF-HASH read-only projections (the `signature export must be hash-neutral` test, `lib.rs:3700–3712`, asserts
exactly this: reading the projection mid-run leaves the hash at `0x47a0_3c8f_6701_f240`). Moving *where* that read
runs (worker vs main) changes nothing about *what* it reads. The `Mutex` guards the slot pointer only; the sim never
waits on it.

**The pinned single-species-plant config issues zero brush actions, zero gem edits, zero immigration ⇒ the journal is
a single coalesced `Advance(50)` on both paths ⇒ `0x47a0_3c8f_6701_f240` byte-identical.** STOP-THE-LINE: if W1's
guard ever shows the literal moved, **halt** — do not work around it.

### The determinism TEST that proves it (the W1 gate)

A new **pure-Rust** test in `crates/godot-sim` (no Godot runtime needed — the worker is plain Rust):

```rust
#[test]
fn worker_run_is_byte_identical_to_synchronous() {
    // SAME (seed, gens, entity_count) as the pinned oracle, driven two ways.
    let seed = 13_679_457_532_755_275_413u64;
    // (a) synchronous: build env exactly as reset() does, step Advance(50), hash.
    let sync_hash = run_synchronous(seed, /*gens*/50);
    // (b) threaded: spawn the worker, run via SetSpeed(target)+Resume so it PUBLISHES the full FrameBundle at
    //     cadence DURING the run (NOT a Step{50}-then-hash that could skip the publish path) — this exercises the
    //     relocated observe()/snapshot()/flow_matrix()/oversight_state() bundle builds (each takes &mut env, so the
    //     test must prove the WHOLE worker-side projection is byte-neutral, not only species_signatures), JOIN,
    //     hash the worker's env.
    let worker_hash = run_via_worker(seed, /*gens*/50);
    assert_eq!(sync_hash, worker_hash);
    assert_eq!(worker_hash, 0x47a0_3c8f_6701_f240); // the oracle CANNOT move
}

#[test]
fn worker_journal_matches_synchronous_with_interleaved_actions() {
    // Drive BOTH paths with the SAME ordered script: Advance(10), brush ApplyEditRegion, Advance(10),
    // inoculate, Advance(10). Assert the two journals are Vec-equal AND the two final hashes are equal.
    // Proves FIFO-at-the-gen-boundary == synchronous call order (D3).
}

#[test]
fn worker_matches_synchronous_through_the_gem_and_immigration_boundaries() {
    // REQUIRED (review finding): the two off-by-one paths the first draft got wrong are EXACTLY the gem-schedule
    // fire boundary (gen_abs <= G+LIVE_STEP, BEFORE the advance) and the scheduled-immigration drain boundary
    // (drain_due_inoculations(G+1), AFTER the advance). The other two tests never exercise them (gem-free config
    // + manual point actions), so an off-by-one there would pass the gate GREEN. This test closes that gap:
    //   (a) ARM a gem schedule with mid-run edits whose gen_abs lands ON the boundary (e.g. gen 1, gen N, and an
    //       edit at the exact LIVE_STEP step) AND set a scheduled-immigration schedule (a Clean/Lab/Open
    //       containment level → non-empty drain_due_inoculations) with arrivals on adjacent generations;
    //   (b) run the SAME (seed, schedules, gens) through run_synchronous (the shipped GDScript interleave
    //       reproduced in Rust: _fire_due_gem_edits → step → _fire_due_immigration) AND run_via_worker;
    //   (c) assert the two JOURNALS are Vec-equal (same action at the same gen) AND the two final hashes are equal.
    // A one-gen shift in either boundary makes the journals diverge here — so this test is the actual guard that
    // advance_one_gen's predicates match `main.gd:1219` + `godot-sim:972-973`. (Until it is GREEN, the determinism
    // argument is NOT airtight beyond the trivial gem-free pinned config.)
}
```

Wired into `tools/gate.sh` alongside the existing determinism asserts. **W1 does not land until both are green.**

---

## 4. §LIFECYCLE — spawn / pause / reset / load_session / quit, with a clean JOIN

| Event | Main thread | Worker |
|---|---|---|
| **spawn** (first `reset(seed)`) | build `ResetConfig` from binding state (entity_count, env_params, roster, species, containment); `thread::spawn(worker_main)`; **block on a `ready` rendezvous** (so `reset` keeps its synchronous "env is up, gen-0 observable" contract). **If the worker PANICS in `apply_reset` (bad roster/config/containment), the `ready` Sender drops → main's `recv()` returns `Err(Disconnected)` → `reset` surfaces a `godot_error!` + leaves `ready=false` (no `unwrap`, no hang).** | `apply_reset(env, cfg, seed)` (identical to today's `reset`: roster > species > default; containment; `enable_oversight`); publish the gen-0 bundle; `ready.send(())`; enter the loop **paused** |
| **pause / resume** | `cmd_tx.send(Pause/Resume)` (or `SetSpeed(0)`) | sets `paused`; when paused the worker blocks on `rx.recv()` (race-free, no spin, **no Condvar**) — the next command (incl. `Resume`/`Shutdown`/a blocking query) wakes it cleanly |
| **set speed** | `cmd_tx.send(SetSpeed(n))` | updates the pace target |
| **brush / edit / inoculate** | `cmd_tx.send(Apply{action, reply})`; block on `reply` for the toast `ActionOutcome` | applies at the next gen boundary, replies |
| **reset (re-run)** | `send(Shutdown)` → **`worker.take().join()`** (env drops on the worker) → spawn a fresh worker with the new `ResetConfig` | old worker returns (env drops); new worker starts clean |
| **load_session(dir)** | read the journal on MAIN (`harness::replay::read_journal`, the existing path) → build a `LoadConfig{env_config, seed, actions}` → `send(Shutdown)` + JOIN the old worker → spawn a worker that does `apply_reset` + replays the journal actions, **then** publishes | replays `reset(seed)` + each journaled action (byte-identical to today's `load_session`), parks |
| **save_session(dir)** | `send(SaveSession{dir, reply})`; block on `reply` (the worker owns the journal + env) | `harness::replay::save_journal(dir, &env_config, seed, &journal)`; replies `Result` |
| **quit / node drop** (`Drop for LiveSim`) | `cmd_tx.send(Shutdown).ok()`; `if let Some(h)=worker.take() { h.join().ok() }` | returns from the loop; env drops |

**No leak, no use-after-free.** The worker holds **no** `Gd<LiveSim>` and **no** reference back into the Godot scene
— only the env, the journal, the `Receiver`, and an `Arc` clone of the slot. `LiveSim` (`RefCounted`) implementing
`Drop` to `Shutdown`+`join()` guarantees the worker is **joined before the node frees** — the env cannot outlive the
node, and the node cannot free while the worker still mutates it (it can't — the worker owns its own env). If the
`Sender` is dropped without a `Shutdown` (panic on main), the worker's `rx.recv()` returns `Disconnected` and it
returns cleanly. A `Shutdown`-then-`join` on every respawn (reset/load) prevents thread accumulation across re-runs.

---

## 5. §INV-AUDIT

**inv #2 — Genome lives in the core; render reads bytes only.** ✅ **Strengthened.** The worker is *pure Rust in the
`godot-sim` crate* (it already embeds the core), not GDScript — no genome logic moves to GDScript. In fact the
`_fire_due_gem_edits → step → _fire_due_immigration` **orchestration moves OUT of GDScript into Rust**
(`advance_one_gen`), so `main.gd` does *less* sim driving, not more. The main thread still only sends opaque commands
and reads off-hash bytes (`FrameBundle`). No `Gd<…>`/Godot type ever crosses into the worker; no genotype→phenotype
runs on the main thread.

**inv #3 — Determinism.** ✅ See §3 (D1–D4) + the two new tests. One master seed still derives the single
`ChaCha8Rng` stream, threaded explicitly through `env.step`. No `HashMap` iteration is added (the bundle is built
from the existing ordered projections; the FIFO `mpsc` is the only new ordering and it is total + single-consumer).
No thread-local/global RNG. The hash is the gate.

**inv #5 — Science pluggable / inv #7 — pinned versions.** ✅ **No dep change.** std `mpsc`/`Arc`/`Mutex`/`Condvar`/
`thread` only — `cargo tree -p godot-sim` is unchanged, no new pinned version, no `Cargo.toml` edit. Trait impls are
untouched (the worker calls the same `Simulation`/`GeneSimEnv` surface).

**inv #1 — GPL boundary.** ✅ Untouched (no subprocess, no new dep, no SLiM/FBA path change).

**Rejected dep — `crossbeam-channel`.** It would buy a faster MPMC channel and a cleaner `select!`. But: (1) the
worker is a **single** consumer, so MPMC is unused; (2) the latest-frame slot has **one** writer + **one** reader and
zero compute under the lock, so `Mutex` contention is a non-issue (a `triple_buffer`/`arc-swap` lock-free slot would
also work but adds a pinned dep for a benefit measured in nanoseconds against a per-gen sim step of tens of µs);
(3) inv #7 makes every new pinned crate a cost. **std clearly suffices and the Mutex does not lose to any candidate
here → reject the dep.** (Revisit only if profiling ever shows the slot lock on the hot path, which the design
structurally prevents.)

---

## 6. §SLICES

**W1 — the Rust worker scaffold (`crates/godot-sim/src/worker.rs`).** 🛑 STOP-THE-LINE-adjacent (boundary +
determinism). Add `SimWorker`, `SimCommand`, `FrameBundle`, the worker loop + `advance_one_gen`, spawn/JOIN, the slot.
**Keep the existing `#[func]` API working** (the methods route through the worker, but the GDScript-visible behavior
is identical — `reset` blocks on `ready`, `step` blocks on `ack`, edits block on `reply`). **Gate = the two new
determinism tests (§3) green + `0x47a0_3c8f_6701_f240` unmoved.** **Needs the determinism guard + human sign-off
before merge** (it is the slice that could, in principle, perturb the stream). No GDScript change yet.

**W2 — migrate `main.gd` to the command/read API (renderer-only).** Replace the per-frame `for steps { … }` body:
instead of driving steps, the loop sends `SetSpeed`/`Pause`/`Resume` once on slider/pause change, and each frame
**reads `last_frame`** (clone-out of the slot) to feed `_snaps`, sparklines, timeline, colonies. Brush/edit handlers
send `Apply{…}` and await the reply for the toast. Delete `_fire_due_gem_edits`/`_fire_due_immigration` from
`_process` (now in `advance_one_gen`); keep the gem schedule resolution (`gem_edit_schedule`) on main → `ArmGemSchedule`.
Inv #2 only (no Rust biology). Untestable in-env (no Godot) → relies on the W1 Rust gate + a manual Godot smoke.

**W3 — pause / reset / load_session / shutdown lifecycle (Rust + GDScript).** Implement `Drop for LiveSim` (Shutdown +
join), the reset/load respawn path (Shutdown→join→spawn), `SaveSession`/`load_session` via the worker, the `Condvar`
pause-park. Test: spawn→shutdown→join leaves no thread; reset twice does not accumulate threads; a save→load round-trip
replays to the **same** hash as the live run (the existing R2 round-trip property, now across the thread boundary).

**W4 — OPTIONAL: presentation interpolation (renderer-only).** Because publish cadence (≤30 Hz) decouples from render
(≥60 FPS), optionally tween organism/colony positions between the last two `FrameBundle`s for visual smoothness. Pure
presentation (inv #2), off-hash, no sim contact. Deferred — only if the discrete 30 Hz steps read as choppy.

---

## 7. ADR-DRAFT (reserve **ADR-036**)

> **ADR-number note:** DECISIONS.md ends at **ADR-034**. **ADR-035 is reserved on a pending branch** (not yet in
> `main`'s DECISIONS.md), so this proposal reserves the next free number **beyond** it: **ADR-036**. Confirm
> ADR-035/036 are still free at merge time and renumber if the pending branch landed something else.

---

### ADR-036 (DRAFT) — Off-thread sim worker: `std-channel-mutex`, ownership-resolved `&mut` hazard, byte-identical

- **Status:** DRAFT — awaiting human sign-off (W1 touches inv #3 → STOP-THE-LINE-adjacent). **NOT a re-pin**: the
  pinned literal `0x47a0_3c8f_6701_f240` (`crates/sim-core/src/lib.rs:3544`, `:3708`) stays byte-identical.
- **Context:** The decoupled-single-thread live loop (prior `live-ui-parallelization-draft.md`) hit its documented
  ceiling — the sim step **and** the heavy `snapshot()`+`observe()`+projection share Godot's main-thread frame
  budget, so a fast/large sim steals input-frame time and FPS drops. Sim-step parallelism is closed (ADR-020 / rayon
  doesn't pay). The remaining lever is to move the *single-threaded* step + the heavy read off the input/render thread.
- **Decision:** Move `GeneSimEnv` onto **one owned worker thread** that is its **sole mutator**. `LiveSim` becomes a
  main-thread proxy holding a `std::sync::mpsc::Sender<SimCommand>` (main→worker, FIFO, covering every current
  mutating `#[func]`), an `Arc<Mutex<Option<FrameBundle>>>` latest-wins read slot (worker→main, off-hash read copy),
  and a `JoinHandle`. The per-gen `gem→step→immigration` interleave moves from GDScript into Rust `advance_one_gen`.
  **Zero new crates** (std `mpsc`/`Arc`/`Mutex`/`Condvar`/`thread`).
- **The `&mut` hazard:** Every `LiveSim` method is `&mut self`; a shared worker reference would alias. **Resolved by
  ownership, not locking** — the env *moves into* the worker and the main thread holds no reference to it. The `Mutex`
  guards only the frame-slot pointer (one writer, one reader, no compute under lock).
- **Determinism guarantee (the load-bearing claim):** byte-identical because (D1) the hash oracle is a sim-core path
  this design never touches; (D2) time advances only by integer `Advance(LIVE_STEP)` — wall-clock/`sleep` is pure
  pacing, never sim content; (D3) single-producer→single-consumer FIFO commands applied at the gen boundary reproduce
  the synchronous call order exactly (brush-between-whole-gens preserved); (D4) the snapshot is a proven off-hash read
  copy. Guarded by two new pure-Rust tests (`worker_run_is_byte_identical_to_synchronous`,
  `worker_journal_matches_synchronous_with_interleaved_actions`) wired into `tools/gate.sh`; both assert
  `0x47a0_3c8f_6701_f240`.
- **Rejected alternatives:**
  - *Stay single-threaded (status quo).* Rejected: the FPS-vs-sim-speed coupling is the problem; the prior draft
    itself flagged the worker as the warranted next step.
  - *Sim-step data-parallelism (rayon inside the tick).* Rejected: ADR-020 / `[[perf-bigger-maps-needs-structural-change]]`
    proved it doesn't pay; orthogonal to UI responsiveness anyway.
  - *Shared `Arc<Mutex<GeneSimEnv>>` (lock the env, both threads mutate).* Rejected: reintroduces the `&mut` hazard
    under a lock, risks main-thread stalls behind a long step, and invites lock-ordering bugs. Ownership is simpler
    and stall-free.
  - *`crossbeam-channel` / `triple_buffer` / `arc-swap` for the channel/slot.* Rejected: single-consumer + a one-writer
    one-reader zero-compute slot means std `mpsc` + `Mutex` suffice; a new pinned crate (inv #7) buys nanoseconds
    against a tens-of-µs step. Revisit only if profiling shows slot-lock contention (structurally prevented).
- **Consequences:** UI gets the whole main-thread budget → ≥30 FPS floor / ≥60 FPS achievable, independent of sim
  speed; history/timeline granularity is the publish cadence (≤30 Hz) as today. `main.gd` drives *less* (orchestration
  moved to Rust) → inv #2 net-positive. New surface: thread lifecycle (Drop=Shutdown+join, respawn on reset/load) must
  be correct — covered by W3 + its leak/round-trip tests.

---

## 8. Open questions for the human

1. **Sign-off on W1 as inv #3-adjacent** — the boundary slice that *could* perturb the stream. Gate = the two new
   determinism tests + the unmoved literal. OK to proceed once green?
2. **Publish cadence at high speed** — keep history/timeline sampled at publish (≤30 Hz, as today), or record cheap
   per-gen history inside the worker loop (finer granularity, slightly more main-thread bundle data)?
3. **W4 interpolation** — wanted, or leave the discrete 30 Hz step as-is?
