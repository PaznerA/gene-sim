//! `worker.rs` — the off-thread sim worker SCAFFOLD (ADR-036 / W1; `docs/llm/proposals/worker-thread-
//! parallelization-draft.md` §2). **STOP-THE-LINE-adjacent (inv #3): the determinism tests below are the
//! safety net.**
//!
//! This module is **pure Rust** — it imports NO `godot::` type, so its three determinism tests run with no
//! Godot runtime (the worker is plain Rust over [`harness::GeneSimEnv`]). The worker **OWNS** the
//! `GeneSimEnv` (the single mutator — the `&mut self` aliasing hazard is dissolved by *ownership*, not
//! locking, draft §2.1). The main thread holds only a [`WorkerHandle`] proxy: a `Sender<SimCommand>` (main→
//! worker, FIFO), an `Arc<Mutex<Option<FrameBundle>>>` latest-wins read slot (worker→main, an OFF-HASH read
//! copy), and the `JoinHandle`.
//!
//! ## What this slice is (and is NOT)
//! **SCAFFOLD ONLY.** `main.gd::_process` / `_publish_frame` are NOT rewritten (that is W2) and `LiveSim`
//! (`lib.rs`) is unchanged — the running game stays on its current synchronous path. The worker path is
//! proven by the pure-Rust determinism tests at the bottom of this file, not yet wired into the renderer.
//! Hence the module-level `#![allow(dead_code)]`: the proxy/commands are unreferenced until W2.
//!
//! ## Determinism (inv #3 — the load-bearing claim, draft §3)
//! The threaded run is byte-identical to the synchronous one because: (D1) the pinned oracle is a `sim-core`
//! path this design never touches; (D2) time advances ONLY by integer `Advance(LIVE_STEP)` — the worker's
//! `sleep`/`SetSpeed` is pure pacing, never sim content; (D3) single-producer→single-consumer FIFO commands
//! applied at the gen boundary reproduce the synchronous call order; (D4) the `FrameBundle` is a proven
//! off-hash read copy — moving *where* it is built (worker vs main) changes nothing about *what* it reads.
#![allow(dead_code)]

use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crispr::{CasVariantId, GuideSequence};
use genome::LocusId;
use harness::discover::ResolvedEdit;
use harness::{Action, EditAction, Env, GeneSimEnv, OversightStatus};
use sim_core::{Observation, SpeciesObservation};

/// Generations advanced per advance tick — the FIXED-integer cadence (inv #3). Byte-for-byte
/// `main.gd::LIVE_STEP` (`= 1`). Never wall-clock.
const LIVE_STEP: u64 = 1;
/// The snapshot grid the [`FrameBundle`] projects each publish. Byte-for-byte `main.gd::LIVE_GRID`
/// (`Vector2i(32, 32)`) so the relocated `snapshot()` build matches the shipped `_publish_frame`.
const LIVE_GRID: (u32, u32) = (32, 32);
/// The publish-cadence ceiling (publishes per second), mirroring `main.gd::RENDER_HZ`. The worker publishes
/// every `ceil(speed / RENDER_HZ)` gens (≥ 1), decoupling the publish rate from the sim rate exactly as the
/// shipped `_process` throttle does.
const RENDER_HZ: u32 = 30;

// ── SimCommand (main → worker, draft §2.2) ──────────────────────────────────────────────────────────────
/// The command queue the main thread sends the worker (the single consumer). Pacing is `SetSpeed`/`Pause`/
/// `Resume`/`RunTo`; deterministic advance is `Step`; journaled writes are `Apply`; `ArmGemSchedule` loads a
/// resolved gem; `Shutdown` ends the loop. This is the SCAFFOLD subset; W2 widens `Apply` to cover every
/// mutating `#[func]` (brush/inoculate/PCR/cull/nutrient/toxin/commit) and adds the blocking-query commands.
pub enum SimCommand {
    /// Set the pacing target in generations/second (`0` = manual, advance only on `Step`/`RunTo`).
    SetSpeed(u32),
    /// Stop self-pacing — the worker parks on the next blocking `recv()` (no Condvar, no spin).
    Pause,
    /// Resume free-running self-pacing (the W2 game path; unbounded — runs until `Pause`/`Shutdown`).
    Resume,
    /// Advance `n` whole generations deterministically, publishing at cadence; `ack` rendezvous fires when
    /// done. The manual/scripted contract (`_live.step(n)`).
    Step {
        /// Generations to advance (each one whole `advance_one_gen`).
        n: u64,
        /// One-shot rendezvous fired after the `n` gens complete.
        ack: SyncSender<()>,
    },
    /// Run (paced + publishing) until `env.generation() >= gen`, then auto-pause and fire `ack`. The
    /// deterministic-join form of `Resume` (a test/scrub bound) — same paced+publish loop, bounded so a
    /// caller can JOIN at a known generation without a wall-clock race.
    RunTo {
        /// Absolute generation to run to (inclusive). (`until`, not `gen` — `gen` is an edition-2024 keyword.)
        until: u64,
        /// One-shot rendezvous fired once the target is reached and the worker has parked.
        ack: SyncSender<()>,
    },
    /// Apply one journaled WRITE action at the next gen boundary (FIFO), replying the outcome for the toast.
    Apply {
        /// The journaled action (`ApplyEditRegion`/`RegionInoculate`/… — the brush/edit/seed click).
        action: Action,
        /// One-shot rendezvous carrying the post-apply outcome back to the UI.
        reply: SyncSender<ActionOutcome>,
    },
    /// Arm (replace) the resolved gem edit schedule the worker fires before each advance.
    ArmGemSchedule(Vec<ResolvedEdit>),
    /// End the loop; the worker returns its [`WorkerOutcome`] (final hash + journal) via `join()`.
    Shutdown,
}

/// The reply to an [`SimCommand::Apply`] — the post-apply generation (the toast/marker stamp). A SCAFFOLD
/// shape; W2 widens it to the full edit/region outcome the `#[func]`s return today.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionOutcome {
    /// The env generation immediately after the action applied.
    pub generation: u64,
}

// ── FrameBundle (worker → main, draft §2.3) ─────────────────────────────────────────────────────────────
/// The OFF-HASH read-only projection the worker builds once per publish and swaps into the latest-wins slot
/// (no compute under the lock). Every field is a proven off-hash read of the live env (D4): moving *where*
/// it is built (worker vs main) changes nothing about *what* it reads. All-plain-`Send`-Rust — no Godot type
/// ever crosses the boundary.
#[derive(Clone)]
pub struct FrameBundle {
    /// The env generation this bundle was taken at.
    pub generation: u64,
    /// GSS6 snapshot bytes for [`LIVE_GRID`] (the HEAVY work, now off the main thread).
    pub snapshot: Vec<u8>,
    /// `observe()` — `{generation, population, allele_freq, phenotype}`.
    pub observe: Observation,
    /// `observe_species()` — one row per roster species.
    pub species: Vec<SpeciesObservation>,
    /// `flow_matrix()` — `(s, flat row-major i64)`.
    pub flow: (usize, Vec<i64>),
    /// `species_signatures()` — `(s, dims, flat u16, roles u8)` (main runs the std-only relations k-NN).
    pub signatures: (usize, usize, Vec<u16>, Vec<u8>),
    /// `oversight_state()` — the earned-credit ledger snapshot.
    pub oversight: OversightStatus,
}

/// What `join()` yields when the worker returns: the FINAL `run_stats().hash` (folded once at the end, like
/// the synchronous path) and the ordered journal (the session's saved progress).
#[derive(Clone, Debug, PartialEq)]
pub struct WorkerOutcome {
    /// The episode's final [`sim_core::RunStats::hash`].
    pub hash: u64,
    /// The ordered journal (Advance coalesced) — byte-identical to the synchronous path's journal.
    pub journal: Vec<Action>,
}

// ── GemCursor — the forward-pointer over a resolved gem schedule (matches `_gem_schedule_idx`) ────────────
/// A forward pointer over a `gen_abs`-sorted resolved gem schedule. `peek`/`advance` reproduce
/// `main.gd::_gem_schedule[_gem_schedule_idx]` + `_gem_schedule_idx += 1` — a single forward scan, NOT a
/// per-gen rescan (the schedule is sorted, draft §2.4).
struct GemCursor {
    edits: Vec<ResolvedEdit>,
    idx: usize,
}

impl GemCursor {
    fn new(edits: Vec<ResolvedEdit>) -> Self {
        Self { edits, idx: 0 }
    }
    fn peek(&self) -> Option<&ResolvedEdit> {
        self.edits.get(self.idx)
    }
    fn advance(&mut self) {
        self.idx += 1;
    }
}

/// Append `n` generations to the journal, COALESCING consecutive `Advance`s — byte-for-byte
/// `LiveSim::journal_advance` (`lib.rs:1309`). `Advance(a)+Advance(b)` is bit-identical to `Advance(a+b)` on
/// the single stream, so the saved file stays `O(edits)` and the replayed hash is unchanged.
fn journal_advance(journal: &mut Vec<Action>, n: u64) {
    if n == 0 {
        return;
    }
    if let Some(Action::Advance(last)) = journal.last_mut() {
        *last += n;
    } else {
        journal.push(Action::Advance(n));
    }
}

/// Build the `Action::ApplyEdit` for one resolved gem edit — byte-for-byte the renderer's
/// `_fire_one_gem_edit` → `LiveSim::apply_edit` `EditAction` construction (`lib.rs:557`). A malformed guide
/// (impossible from the core resolver, defensive) yields `None` so the worker never panics — matching
/// `apply_edit`, which returns a failed-outcome WITHOUT journaling on an invalid guide.
fn gem_edit_action(e: &ResolvedEdit) -> Option<Action> {
    let guide = GuideSequence::new(e.guide.clone().into_bytes()).ok()?;
    Some(Action::ApplyEdit(EditAction {
        cas: CasVariantId(e.cas),
        target: LocusId(e.target),
        guide,
        species: e.species,
    }))
}

// ── advance_one_gen — the deterministic per-gen interleave, MOVED from `main.gd::_process` into Rust ──────
/// Advance the env by exactly one [`LIVE_STEP`] generation, reproducing `main.gd::_process`'s per-frame
/// interleave **byte-for-byte** and pushing every applied [`Action`] onto `journal`.
///
/// **The two gen-boundary predicates are LOAD-BEARING (draft §2.4 — the adversarial review caught both as
/// off-by-one):** gem edits fire BEFORE the advance at `gen_abs <= env.generation() + LIVE_STEP`
/// (`main.gd:1219`, `due = observe().generation + LIVE_STEP`); immigration drains AFTER the advance at
/// `drain_due_inoculations(env.generation() + 1)` with the generation read POST-step (`godot-sim/src/lib.rs:
/// 972-973`, `current_gen` read after the step; `harness` drains `due_epoch < up_to_generation`). Getting
/// either wrong shifts the generation a scheduled edit/arrival lands on and moves the hash for any gem-armed
/// or scheduled-immigration run — which is exactly what test (c) below guards.
fn advance_one_gen(env: &mut GeneSimEnv, journal: &mut Vec<Action>, gem: &mut GemCursor) {
    // env.generation() == G now; this call advances to G + LIVE_STEP.
    let g = env.generation();
    // (1) GEM EDITS fire BEFORE the advance, at the shipped threshold gen_abs in (G, G+LIVE_STEP], i.e.
    //     gen_abs <= G + LIVE_STEP — the forward-pointer cursor (main.gd::_fire_due_gem_edits, :1216-1225).
    let due = g + LIVE_STEP;
    while let Some(e) = gem.peek() {
        if u64::from(e.gen_abs) > due {
            break;
        }
        if let Some(action) = gem_edit_action(e) {
            env.step(action.clone());
            journal.push(action);
        }
        gem.advance(); // forward pointer (matches `_gem_schedule_idx += 1`)
    }
    // (2) ONE whole generation; env.generation() is now G + LIVE_STEP (main.gd:1044, `_live.step(LIVE_STEP)`).
    env.step(Action::Advance(LIVE_STEP));
    journal_advance(journal, LIVE_STEP);
    // (3) IMMIGRATION drains AFTER the advance, at drain_due_inoculations(current_gen + 1) with current_gen
    //     read POST-step (godot-sim::fire_due_inoculations, :972-973 → harness drains due_epoch < up_to).
    let current_gen = env.generation();
    for action in env.drain_due_inoculations(current_gen + 1) {
        env.step(action.clone());
        journal.push(action);
    }
}

// ── SimWorker — the owned worker thread (the sole mutator) ───────────────────────────────────────────────
/// The plain-Rust worker that OWNS the [`GeneSimEnv`]. Built on the worker thread by [`WorkerHandle::spawn`];
/// runs [`SimWorker::run`] until `Shutdown`/`Disconnected`, then returns its [`WorkerOutcome`].
struct SimWorker {
    env: GeneSimEnv,
    seed: u64,
    journal: Vec<Action>,
    gem: GemCursor,
    rx: Receiver<SimCommand>,
    slot: Arc<Mutex<Option<FrameBundle>>>,
    speed: u32,
    running: bool,
    target: Option<u64>,
    run_ack: Option<SyncSender<()>>,
    gens_since_publish: u64,
}

impl SimWorker {
    fn new(
        env: GeneSimEnv,
        seed: u64,
        gem: Vec<ResolvedEdit>,
        rx: Receiver<SimCommand>,
        slot: Arc<Mutex<Option<FrameBundle>>>,
    ) -> Self {
        Self {
            env,
            seed,
            journal: Vec::new(),
            gem: GemCursor::new(gem),
            rx,
            slot,
            speed: 0,
            running: false,
            target: None,
            run_ack: None,
            gens_since_publish: 0,
        }
    }

    /// `reset(seed)` on the worker — identical to `LiveSim::reset`'s build (the env was configured on main:
    /// entity_count / containment / contaminants / oversight; the RNG-seeding RESET runs HERE on the sole
    /// mutator). Mirrors the draft's `apply_reset`.
    fn apply_reset(&mut self) {
        let _ = self.env.reset(self.seed);
    }

    /// `ceil(speed / RENDER_HZ)` generations between publishes (≥ 1) — the shipped `_process` throttle.
    /// Manual mode (`speed == 0`) publishes every gen so a scripted `Step` still feeds the slot.
    fn publish_every(&self) -> u64 {
        if self.speed <= RENDER_HZ {
            1
        } else {
            (u64::from(self.speed)).div_ceil(u64::from(RENDER_HZ))
        }
    }

    /// Build + swap the latest-wins [`FrameBundle`] — the HEAVY off-hash read, now off the main thread. NO
    /// compute under the lock (the lock guards only the slot pointer; the sim never waits on it, D4).
    fn publish(&mut self) {
        let bundle = self.build_bundle();
        if let Ok(mut slot) = self.slot.lock() {
            *slot = Some(bundle); // latest-wins atomic swap
        }
    }

    /// Build the whole projection bundle. Every call is a proven OFF-HASH read of the live env (D4) — the
    /// test (a) `0x47a0`-anchored equality is the guard that this WHOLE build (not only `species_signatures`)
    /// is byte-neutral mid-run.
    fn build_bundle(&mut self) -> FrameBundle {
        let snapshot = self
            .env
            .snapshot(LIVE_GRID.0, LIVE_GRID.1)
            .write_snapshot_bytes();
        let observe = self.env.observe();
        let species = self.env.observe_all();
        let flow = self.env.flow_matrix();
        let signatures = self.env.species_signatures();
        let oversight = self.env.oversight_status();
        FrameBundle {
            generation: self.env.generation(),
            snapshot,
            observe,
            species,
            flow,
            signatures,
            oversight,
        }
    }

    /// Pure pacing (D2): sleep to approach the `SetSpeed` gens/sec target. Wall-clock NEVER feeds the sim —
    /// it only chooses *how many* gens run per second, never the *content* of a generation.
    fn pace(&self) {
        if self.speed > 0 {
            thread::sleep(Duration::from_secs_f64(1.0 / f64::from(self.speed)));
        }
    }

    /// Handle one command. Returns `true` iff it was `Shutdown` (the loop must return its outcome).
    fn handle(&mut self, cmd: SimCommand) -> bool {
        match cmd {
            SimCommand::SetSpeed(s) => self.speed = s,
            SimCommand::Pause => self.running = false,
            SimCommand::Resume => {
                self.running = true;
                self.target = None;
            }
            SimCommand::RunTo { until, ack } => {
                self.running = true;
                self.target = Some(until);
                self.run_ack = Some(ack);
            }
            SimCommand::Step { n, ack } => {
                for _ in 0..n {
                    advance_one_gen(&mut self.env, &mut self.journal, &mut self.gem);
                    self.gens_since_publish += 1;
                    if self.gens_since_publish >= self.publish_every() {
                        self.publish();
                        self.gens_since_publish = 0;
                    }
                }
                let _ = ack.send(());
            }
            SimCommand::Apply { action, reply } => {
                // FIFO at the gen boundary == the synchronous call order (D3): step + journal, exactly the
                // `apply_edit_region`/`inoculate`/… path. Brush-between-whole-gens is preserved (commands are
                // applied only at gen boundaries, never mid-step).
                self.env.step(action.clone());
                self.journal.push(action);
                let _ = reply.send(ActionOutcome {
                    generation: self.env.generation(),
                });
            }
            SimCommand::ArmGemSchedule(edits) => self.gem = GemCursor::new(edits),
            SimCommand::Shutdown => return true,
        }
        false
    }

    /// The worker loop. When paced (running) it drains pending commands FIFO at the gen boundary, advances
    /// one gen, publishes at cadence, and sleeps to pace. When idle/paused it **parks on a blocking
    /// `rx.recv()`** — NO Condvar, NO spin: std `mpsc::recv()` has no lost-wakeup (a command queued *before*
    /// `recv()` is still returned), so the next command (a blocking `Step`/`Apply`/`RunTo`, `Resume`, or
    /// `Shutdown`) wakes it cleanly (draft §2.4). A dropped `Sender` (main panicked) → `recv()` returns
    /// `Disconnected` → the worker returns cleanly.
    fn run(mut self) -> WorkerOutcome {
        loop {
            // A bounded run (RunTo) that reached its target: publish the final bundle, ack, and fall to park.
            if self.running
                && let Some(t) = self.target
                && self.env.generation() >= t
            {
                self.running = false;
                self.target = None;
                self.publish();
                if let Some(ack) = self.run_ack.take() {
                    let _ = ack.send(());
                }
            }

            if self.running {
                // Drain every pending command at the gen boundary, FIFO (deterministic apply order, D3).
                loop {
                    match self.rx.try_recv() {
                        Ok(cmd) => {
                            if self.handle(cmd) {
                                return self.finish();
                            }
                        }
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => return self.finish(),
                    }
                }
                if !self.running {
                    continue; // a drained Pause stopped us before advancing
                }
                advance_one_gen(&mut self.env, &mut self.journal, &mut self.gem);
                self.gens_since_publish += 1;
                if self.gens_since_publish >= self.publish_every() {
                    self.publish();
                    self.gens_since_publish = 0;
                }
                self.pace();
            } else {
                // PARK on blocking recv — no Condvar, no spin, no lost-wakeup.
                match self.rx.recv() {
                    Ok(cmd) => {
                        if self.handle(cmd) {
                            return self.finish();
                        }
                    }
                    Err(_) => return self.finish(), // Sender dropped → Disconnected → exit cleanly
                }
            }
        }
    }

    /// Fold the FINAL `run_stats().hash` once (like the synchronous path) and return the outcome.
    fn finish(mut self) -> WorkerOutcome {
        let hash = self.env.run_stats().hash;
        WorkerOutcome {
            hash,
            journal: self.journal,
        }
    }
}

// ── WorkerHandle — the main-thread proxy (draft §2) ─────────────────────────────────────────────────────
/// The main-thread proxy `LiveSim` becomes (W2 folds these fields into the node). Holds the `Sender`, the
/// latest-wins frame slot, the `JoinHandle`, and a main-side `ready` flag (the env no longer lives on main —
/// `is_ready` reads this flag, draft §2.2).
pub struct WorkerHandle {
    cmd_tx: Sender<SimCommand>,
    frame: Arc<Mutex<Option<FrameBundle>>>,
    worker: Option<JoinHandle<WorkerOutcome>>,
    ready: bool,
}

impl WorkerHandle {
    /// Spawn the worker, **MOVING `env` onto it as the sole mutator**, and block on a `ready` rendezvous so
    /// the caller's `reset` keeps its synchronous "env is up, gen-0 observable" contract. `env` is configured
    /// on main (entity_count / containment / contaminants / oversight); the RNG-seeding RESET runs on the
    /// worker (`apply_reset`). **Panic-safe:** if the worker panics in `apply_reset`/the gen-0 publish, the
    /// `ready` Sender drops → `recv()` returns `Err(Disconnected)` → `ready` stays `false` (no `unwrap`, no
    /// hang).
    pub fn spawn(env: GeneSimEnv, seed: u64, gem: Vec<ResolvedEdit>) -> Self {
        let (cmd_tx, rx) = mpsc::channel::<SimCommand>();
        let frame: Arc<Mutex<Option<FrameBundle>>> = Arc::new(Mutex::new(None));
        let frame_worker = Arc::clone(&frame);
        let (ready_tx, ready_rx) = mpsc::sync_channel::<()>(0);
        let worker = thread::spawn(move || {
            let mut sw = SimWorker::new(env, seed, gem, rx, frame_worker);
            sw.apply_reset(); // reset(seed) on the sole mutator
            sw.publish(); // the gen-0 bundle
            let _ = ready_tx.send(()); // ready rendezvous (drops on a panic above → caller sees Disconnected)
            sw.run()
        });
        let ready = ready_rx.recv().is_ok();
        Self {
            cmd_tx,
            frame,
            worker: Some(worker),
            ready,
        }
    }

    /// Whether the worker came up (the spawn `ready` rendezvous succeeded) — the main-side `is_ready` flag.
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// Set the pacing target (gens/sec) — the speed slider becomes this command.
    pub fn set_speed(&self, gens_per_sec: u32) {
        let _ = self.cmd_tx.send(SimCommand::SetSpeed(gens_per_sec));
    }

    /// Pause (the worker parks on the next `recv()`).
    pub fn pause(&self) {
        let _ = self.cmd_tx.send(SimCommand::Pause);
    }

    /// Resume free-running self-pacing (the W2 game path; unbounded).
    pub fn resume(&self) {
        let _ = self.cmd_tx.send(SimCommand::Resume);
    }

    /// Advance `n` whole generations, blocking on the ack (the synchronous `step(n)` contract).
    pub fn step(&self, n: u64) {
        let (tx, rx) = mpsc::sync_channel::<()>(0);
        let _ = self.cmd_tx.send(SimCommand::Step { n, ack: tx });
        let _ = rx.recv();
    }

    /// Run (paced + publishing) until the worker reaches `gen` and parks, blocking on the ack. The
    /// deterministic-join form of `Resume` — no wall-clock race.
    pub fn run_to(&self, until: u64) {
        let (tx, rx) = mpsc::sync_channel::<()>(0);
        let _ = self.cmd_tx.send(SimCommand::RunTo { until, ack: tx });
        let _ = rx.recv();
    }

    /// Apply one journaled action at the next gen boundary, blocking on the reply (the toast outcome).
    pub fn apply(&self, action: Action) -> ActionOutcome {
        let (tx, rx) = mpsc::sync_channel::<ActionOutcome>(0);
        let _ = self.cmd_tx.send(SimCommand::Apply { action, reply: tx });
        rx.recv().expect("worker reply for Apply")
    }

    /// Arm (replace) the resolved gem edit schedule the worker fires before each advance.
    pub fn arm_gem(&self, edits: Vec<ResolvedEdit>) {
        let _ = self.cmd_tx.send(SimCommand::ArmGemSchedule(edits));
    }

    /// Clone out the latest published [`FrameBundle`] (latest-wins; `None` until the first publish). The main
    /// thread NEVER blocks on the sim — the worst case is it re-displays the last frame.
    pub fn latest_frame(&self) -> Option<FrameBundle> {
        self.frame.lock().ok().and_then(|g| g.clone())
    }

    /// `Shutdown` + `join()` — returns the worker's [`WorkerOutcome`] (final hash + journal). Consumes the
    /// handle so [`Drop`] does not double-join.
    pub fn shutdown(mut self) -> WorkerOutcome {
        let _ = self.cmd_tx.send(SimCommand::Shutdown);
        self.worker
            .take()
            .expect("worker present")
            .join()
            .expect("worker thread joins cleanly")
    }
}

impl Drop for WorkerHandle {
    /// Lifecycle hygiene (draft §4): if the handle is dropped without an explicit `shutdown()`, send
    /// `Shutdown` and `join()` so the worker is joined before the proxy frees — no thread leak. (If `Shutdown`
    /// cannot be sent because the worker already exited, the dropped Sender's `recv()` already returned
    /// `Disconnected`; the `join()` still completes.)
    fn drop(&mut self) {
        if let Some(h) = self.worker.take() {
            let _ = self.cmd_tx.send(SimCommand::Shutdown);
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crispr::default_cas_variants;
    use genome::spec::{BuiltSpecies, SpeciesSpec};
    use harness::RegionSpec;
    use sim_core::{ConsortiumConfig, ContainmentLevel, SimConfig};

    /// The pinned-oracle config (`crates/sim-core/src/lib.rs:3539`). `run_headless` of this is
    /// `0x47a0_3c8f_6701_f240` — the literal W1 must NOT move.
    const ORACLE_SEED: u64 = 13_679_457_532_755_275_413;
    const ORACLE_ENTITIES: u32 = 1000;
    const ORACLE_HASH: u64 = 0x47a0_3c8f_6701_f240;

    /// The configuration BOTH paths build their env from (so the synchronous reference and the worker run an
    /// IDENTICAL env). The worker resets it on its own thread; the synchronous path resets it inline.
    struct RunConfig {
        entity_count: u32,
        containment: Option<(ContainmentLevel, ConsortiumConfig)>,
        contaminants: Vec<BuiltSpecies>,
        gem: Vec<ResolvedEdit>,
    }

    impl RunConfig {
        /// The pinned single-species plant: no roster, no species, no containment, no gem (the hash-neutral
        /// baseline — issues zero brush actions, zero gem edits, zero immigration).
        fn default_plant() -> Self {
            Self {
                entity_count: ORACLE_ENTITIES,
                containment: None,
                contaminants: Vec::new(),
                gem: Vec::new(),
            }
        }
    }

    /// Build the env EXACTLY as `LiveSim::reset` does (entity_count → contaminants → containment → enable
    /// oversight), but WITHOUT calling `reset` — the caller (synchronous) or the worker (`apply_reset`) does
    /// the RNG-seeding reset. Oversight is enabled on BOTH paths (it is off-hash, so it never moves the
    /// hash), so the worker is a faithful proxy of the renderer's env.
    fn build_env(cfg: &RunConfig) -> GeneSimEnv {
        let mut env = GeneSimEnv::new(cfg.entity_count);
        for built in &cfg.contaminants {
            env.register_contaminant(built.clone());
        }
        if let Some((lvl, cc)) = &cfg.containment {
            env.set_containment(*lvl, cc.clone());
        }
        env.enable_oversight(harness::oversight::CreditPolicy::default());
        env
    }

    /// A synthetic contaminant `BuiltSpecies` (key `"contaminant"`, decomposer) off the wired sample genome —
    /// the same shape the harness replay tests use. A `RegionInoculate` keyed on `"contaminant"` resolves it.
    fn contaminant_built() -> BuiltSpecies {
        let mut spec =
            SpeciesSpec::from_genome(&genome::sample_genome(), "contaminant", "Contaminant");
        spec.niche.trophic_role = Some("decomposer".to_string());
        spec.build().expect("contaminant builds")
    }

    /// A valid 20-base ACGT guide (the one the harness replay tests reuse).
    fn guide() -> GuideSequence {
        GuideSequence::new(b"ACGTGGACGTTTTAGGCCGG".to_vec()).expect("valid guide")
    }

    /// A region brush edit (the selective-brush click — `ApplyEditRegion`).
    fn brush_action() -> Action {
        Action::ApplyEditRegion(
            EditAction {
                cas: default_cas_variants()[0].id,
                target: LocusId(0),
                guide: guide(),
                species: 0,
            },
            RegionSpec {
                cx: 16,
                cy: 16,
                radius: 6,
            },
        )
    }

    /// A hand-fired inoculate of the synthetic contaminant (the seed-tool click — `RegionInoculate`).
    fn inoculate_action() -> Action {
        Action::RegionInoculate {
            species_key: "contaminant".to_string(),
            region: RegionSpec {
                cx: 16,
                cy: 16,
                radius: 6,
            },
            count: 10,
            endow_j: 800_000,
        }
    }

    /// One resolved gem edit at `gen_abs` (species 0 = the resident primary), the renderer's `_fire_one_gem_edit`
    /// payload shape.
    fn gem_edit(gen_abs: u32) -> ResolvedEdit {
        ResolvedEdit {
            gen_abs,
            cas: default_cas_variants()[0].id.0,
            target: 0,
            guide: "ACGTGGACGTTTTAGGCCGG".to_string(),
            species: 0,
        }
    }

    /// One scripted step: advance `n` whole gens, or apply a journaled point action between whole gens.
    enum ScriptStep {
        Advance(u64),
        Apply(Action),
    }

    /// THE SYNCHRONOUS REFERENCE — reproduces `main.gd::_process`'s per-gen interleave **directly** (NOT via
    /// `advance_one_gen`), so the tests prove the worker's `advance_one_gen` reproduces the SHIPPED GDScript
    /// interleave byte-for-byte. Returns `(final hash, journal)`.
    fn run_synchronous(seed: u64, cfg: &RunConfig, script: &[ScriptStep]) -> (u64, Vec<Action>) {
        let mut env = build_env(cfg);
        env.reset(seed);
        let mut journal: Vec<Action> = Vec::new();
        let mut gem_idx = 0usize;
        for step in script {
            match step {
                ScriptStep::Advance(n) => {
                    for _ in 0..*n {
                        // (1) main.gd::_fire_due_gem_edits (:1216-1225): due = observe().generation + LIVE_STEP;
                        //     fire each pending edit with gen_abs <= due via apply_edit (lib.rs:542).
                        let due = env.observe().generation + LIVE_STEP;
                        while gem_idx < cfg.gem.len() {
                            let e = &cfg.gem[gem_idx];
                            if u64::from(e.gen_abs) > due {
                                break;
                            }
                            if let Some(action) = gem_edit_action(e) {
                                env.step(action.clone());
                                journal.push(action);
                            }
                            gem_idx += 1;
                        }
                        // (2) main.gd::_live.step(LIVE_STEP) (:1044) → Advance + journal_advance (lib.rs:347/1309).
                        env.step(Action::Advance(LIVE_STEP));
                        journal_advance(&mut journal, LIVE_STEP);
                        // (3) main.gd::_fire_due_immigration → fire_due_inoculations (godot-sim:967-981):
                        //     current_gen = env.generation() POST-step; drain_due_inoculations(current_gen + 1).
                        let current_gen = env.generation();
                        for action in env.drain_due_inoculations(current_gen + 1) {
                            env.step(action.clone());
                            journal.push(action);
                        }
                    }
                }
                ScriptStep::Apply(a) => {
                    // A brush/inoculate click applied between whole gens (lib.rs: step + journal.push).
                    env.step(a.clone());
                    journal.push(a.clone());
                }
            }
        }
        let hash = env.run_stats().hash;
        (hash, journal)
    }

    /// Drive the SAME script through the worker in MANUAL mode (speed 0 → parks on recv): `Advance` → `Step`,
    /// `Apply` → `Apply`. Single-producer + single-consumer + blocking-ack rendezvous ⇒ a total FIFO order
    /// identical to the synchronous call order (D3). Returns `(final hash, journal)`.
    fn run_via_worker_manual(
        seed: u64,
        cfg: &RunConfig,
        script: &[ScriptStep],
    ) -> (u64, Vec<Action>) {
        let env = build_env(cfg);
        let handle = WorkerHandle::spawn(env, seed, cfg.gem.clone());
        assert!(handle.is_ready(), "worker must come up (ready rendezvous)");
        for step in script {
            match step {
                ScriptStep::Advance(n) => handle.step(*n),
                ScriptStep::Apply(a) => {
                    let _ = handle.apply(a.clone());
                }
            }
        }
        let out = handle.shutdown();
        (out.hash, out.journal)
    }

    #[test]
    fn worker_run_is_byte_identical_to_synchronous() {
        // (a) The SAME (seed, 50 gens) driven two ways. The worker runs PACED (SetSpeed + RunTo) so it
        // PUBLISHES the full FrameBundle at cadence DURING the run — exercising the relocated
        // observe()/snapshot()/observe_all()/flow_matrix()/species_signatures()/oversight_state() builds
        // (each touches the live env). Byte-identical final hash proves the WHOLE worker-side projection is
        // off-hash (D4), not only species_signatures.
        let cfg = RunConfig::default_plant();
        let (sync_hash, sync_journal) =
            run_synchronous(ORACLE_SEED, &cfg, &[ScriptStep::Advance(50)]);

        let env = build_env(&cfg);
        let handle = WorkerHandle::spawn(env, ORACLE_SEED, cfg.gem.clone());
        assert!(handle.is_ready());
        handle.set_speed(480); // paced: publish_every = ceil(480/30) = 16 gens → several mid-run publishes
        handle.run_to(50); // runs + publishes at cadence DURING the run, then parks at gen 50
        let mid = handle
            .latest_frame()
            .expect("a FrameBundle was published during the paced run");
        assert_eq!(
            mid.generation, 50,
            "the final published bundle is at gen 50"
        );
        assert!(
            !mid.snapshot.is_empty(),
            "the relocated snapshot build produced GSS6 bytes"
        );
        let out = handle.shutdown();

        assert_eq!(
            sync_hash, out.hash,
            "the threaded run must be byte-identical to the synchronous run (inv #3 / D1-D4)"
        );
        assert_eq!(
            sync_journal, out.journal,
            "both paths coalesce 50 advances to a single Advance(50)"
        );
        assert_eq!(out.journal, vec![Action::Advance(50)]);

        // ORACLE ANCHOR: the underlying sim-core run (the gate's determinism oracle) is the pinned literal —
        // godot-sim sees the UNMOVED 0x47a0. (The GeneSimEnv path's own hash differs from this literal ONLY by
        // the folded `config.generations` metadata: GeneSimEnv records 0, run_headless records 50 — see
        // hash_world, lib.rs:3448. The byte-identity that matters for W1 is sync == worker, asserted above.)
        let oracle = sim_core::run_headless(&SimConfig {
            seed: ORACLE_SEED,
            generations: 50,
            entity_count: ORACLE_ENTITIES,
        })
        .hash;
        assert_eq!(
            oracle, ORACLE_HASH,
            "the pinned determinism oracle 0x47a0_3c8f_6701_f240 must be unmoved (W1 is godot-sim only)"
        );
    }

    #[test]
    fn worker_journal_matches_synchronous_with_interleaved_actions() {
        // (b) The SAME ordered script through both paths: Advance(10), brush ApplyEditRegion, Advance(10),
        // inoculate (a REAL spawn — the contaminant is registered), Advance(10). Vec-equal journals AND equal
        // hashes prove FIFO-at-the-gen-boundary == the synchronous call order (D3); the brush-between-whole-
        // gens contract holds (commands apply only at gen boundaries).
        let cfg = RunConfig {
            entity_count: 256,
            containment: None,
            contaminants: vec![contaminant_built()],
            gem: Vec::new(),
        };
        let script = vec![
            ScriptStep::Advance(10),
            ScriptStep::Apply(brush_action()),
            ScriptStep::Advance(10),
            ScriptStep::Apply(inoculate_action()),
            ScriptStep::Advance(10),
        ];
        let seed = 2024u64;
        let (sync_hash, sync_journal) = run_synchronous(seed, &cfg, &script);
        let (worker_hash, worker_journal) = run_via_worker_manual(seed, &cfg, &script);

        assert_eq!(
            sync_journal, worker_journal,
            "the FIFO worker journal must Vec-equal the synchronous interleave (D3)"
        );
        assert_eq!(
            sync_hash, worker_hash,
            "interleaved brush+inoculate must hash identically on both paths (D3)"
        );
        // The journal carries the brush + inoculate at the right positions (a real run, not a no-op).
        assert!(
            worker_journal
                .iter()
                .any(|a| matches!(a, Action::ApplyEditRegion(..))),
            "the brush is journaled"
        );
        assert!(
            worker_journal
                .iter()
                .any(|a| matches!(a, Action::RegionInoculate { .. })),
            "the inoculate is journaled"
        );
    }

    #[test]
    fn worker_matches_synchronous_through_the_gem_and_immigration_boundaries() {
        // (c) THE off-by-one guard. ARM a gem schedule with edits ON the boundary (gen_abs 1, 2, 4 — fired
        // BEFORE the advance) AND a NON-EMPTY scheduled-immigration drain (Lab containment, horizon 6 → 6
        // events, drained AFTER the advance). The synchronous reference reproduces main.gd:1219 +
        // godot-sim:972-973 directly; the worker uses advance_one_gen. A one-gen shift in either boundary makes
        // the journals diverge HERE. Vec-equal journals + equal hashes ⇒ advance_one_gen matches the shipped
        // interleave.
        let containment = (
            ContainmentLevel::Lab,
            ConsortiumConfig {
                species_keys: vec!["bacillus".to_string(), "pseudomonas".to_string()],
                radius: 3,
                endow_j: 500_000,
                horizon: 6,
            },
        );
        let cfg = RunConfig {
            entity_count: 256,
            containment: Some(containment),
            contaminants: Vec::new(),
            gem: vec![gem_edit(1), gem_edit(2), gem_edit(4)], // gen_abs-sorted, ON the boundaries
        };
        let seed = 90_125u64;

        // Guard the test's own premise: the immigration schedule is actually non-empty (else the AFTER-advance
        // boundary would not be exercised at all).
        {
            let mut probe = build_env(&cfg);
            probe.reset(seed);
            assert!(
                !probe.immigration_schedule().is_empty(),
                "test premise: Lab containment must expand a non-empty immigration schedule"
            );
        }

        let script = vec![ScriptStep::Advance(8)];
        let (sync_hash, sync_journal) = run_synchronous(seed, &cfg, &script);
        let (worker_hash, worker_journal) = run_via_worker_manual(seed, &cfg, &script);

        assert_eq!(
            sync_journal, worker_journal,
            "gem + immigration boundaries must produce a Vec-equal journal on both paths"
        );
        assert_eq!(
            sync_hash, worker_hash,
            "gem + immigration boundaries must hash identically on both paths"
        );
        // The journal carries BOTH boundary kinds (the test is exercising what it claims to).
        assert!(
            worker_journal
                .iter()
                .any(|a| matches!(a, Action::ApplyEdit(_))),
            "gem edits fired (BEFORE the advance)"
        );
        assert!(
            worker_journal
                .iter()
                .any(|a| matches!(a, Action::RegionInoculate { .. })),
            "scheduled immigration drained (AFTER the advance)"
        );
    }

    #[test]
    fn worker_parks_on_recv_and_a_command_wakes_it() {
        // The worker starts PARKED (paused) on a blocking rx.recv() — no Condvar, no spin, no lost-wakeup. A
        // Step queued while it parks wakes it cleanly; it advances, re-parks; Shutdown joins (no leak).
        let cfg = RunConfig::default_plant();
        let env = build_env(&cfg);
        let handle = WorkerHandle::spawn(env, 7, cfg.gem.clone());
        assert!(handle.is_ready());
        // Parked: the only published bundle is the gen-0 one.
        assert_eq!(handle.latest_frame().expect("gen-0 bundle").generation, 0);
        handle.pause(); // explicit pause is idempotent on an already-parked worker
        handle.step(3); // wakes the parked worker; it advances 3 then re-parks
        assert_eq!(
            handle.latest_frame().expect("bundle at gen 3").generation,
            3
        );
        let out = handle.shutdown();
        assert_eq!(out.journal, vec![Action::Advance(3)]);
    }
}
