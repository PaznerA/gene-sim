//! D2a/D2b — the SEARCH RUNNER: the engine that turns the [`discovery::search`] data model into saved,
//! replay-verified [`Gem`](discovery::search::Gem)s (the emergent-run discovery harness: D2a random = ADR-024,
//! D2b evolutionary = ADR-025; the D0 scorer + D1 trace it builds on = ADR-023).
//!
//! ## What it does (the search loop)
//! [`discover`] is the meta-loop. For each `trial`:
//!   1. PROPOSE a [`SearchConfig`] from the meta-RNG ([`discovery::search::propose`] — std-only splitmix64, NOT
//!      the sim RNG; the pinned literal `0x47a0_3c8f_6701_f240` is untouched because the sim runs are unchanged
//!      pure functions of their config — inv #3).
//!   2. BUILD a [`GeneSimEnv`] from the config — `set_roster` (each `(key, count)` resolved through the SAME
//!      `data/species/<key>.json` boundary the menu/CLI uses, [`species::load_species_file`]), `set_environment`
//!      (temp/season), `set_containment` (the airborne immigration knob).
//!   3. CAPTURE an off-hash [`PerGenTrace`](discovery::trace::PerGenTrace) via [`capture::capture_trace`] and
//!      SCORE it ([`DefaultScorer`] → [`discovery::final_score`] vs the kept-gem fingerprints).
//!   4. CONSIDER it for the [`GemLibrary`] (deduped top-K by novelty-adjusted final score).
//!
//! After the search, for every KEPT gem it REBUILDS the `(seed, EnvConfig, journal)` and
//! `record_episode → assert replay() == recorded_hash` BEFORE writing the gem JSON; a gem that fails the
//! round-trip is DROPPED (logged), never written (the gem reproducibility contract, discovery-scorer-spec).
//!
//! ## Determinism (inv #3)
//! The SIM runs are pure functions of their `SearchConfig` (one master seed → all sub-seeds). The PROPOSAL
//! sampler is the META-RNG ([`propose`](discovery::search::propose)), a std-only splitmix over `(search_seed,
//! trial, field)` — it never touches `SimRng`. So a fixed `(search_seed, trials, keep, gens, species_dir)`
//! produces a byte-identical set of saved gems, and the search adds NO sim-path change → the pinned literal is
//! unmoved.

use std::io;
use std::path::Path;

use crispr::{default_cas_variants, GuideSequence};
use discovery::search::{
    caption, propose, propose_evolved, EvalRecord, Gem, GemLibrary, SearchConfig, SearchSpace,
    EDIT_GEN_Q16_DEN,
};
use discovery::{final_score, DefaultScorer};
use genome::spec::BuiltSpecies;
use sim_core::{ConsortiumConfig, ContainmentLevel, EnvParams};

use crate::capture::capture_trace;
use crate::replay::{record_episode, replay, EnvConfig};
use crate::species::load_species_file;
use crate::{Action, EditAction};

/// The pinned-build fingerprint stored on every gem (inv #7). Anchored to the determinism literal so a re-pin
/// (which moves the literal) self-invalidates stored scores — a gem carrying an OLD `build_id` must be
/// recomputed by replay before its score is trusted (discovery-scorer-spec gem-validity contract).
pub const BUILD_ID: &str = "ecology-d0@47a03c8f6701f240";

/// The fallback population spawned at `reset` for the env's non-roster bookkeeping. The roster's per-species
/// counts drive the actual spawn (a search config always proposes a non-empty roster), so this only feeds the
/// metadata `entity_count` folded into the run hash — fixed so a config's hash is a pure function of the config.
const DISCOVER_ENTITY_COUNT: u32 = 1000;

/// Build the replay [`EnvConfig`] for a proposed [`SearchConfig`]: resolve the roster keys through the
/// `data/species/<key>.json` boundary, map the temp/season knobs to [`EnvParams`], and map the containment
/// ordinal to a `(ContainmentLevel, ConsortiumConfig)` pair (Sealed → `None`, hash-neutral).
///
/// A roster entry with a zero count, or a key whose species file fails to load, is SKIPPED — so a config that
/// references an absent species degrades to the species it CAN resolve (never a panic). Returns `None` (in the
/// first tuple slot) if the resolved roster is empty (nothing to run); the second slot is the `(key, error)`
/// skip list. Public so the gem-replay boundary (renderer/CLI loading a saved gem) can rebuild the SAME env a
/// gem was scored under from its `SearchConfig` alone — the gem reproducibility contract.
#[must_use]
pub fn env_config_for(
    cfg: &SearchConfig,
    species_dir: &Path,
) -> (Option<EnvConfig>, Vec<(String, String)>) {
    let mut roster: Vec<(BuiltSpecies, u32)> = Vec::with_capacity(cfg.roster.len());
    let mut skipped: Vec<(String, String)> = Vec::new();
    // The consortium (loaded contaminant resolver) the containment schedule's keys resolve against. We load the
    // Mode-A airborne keys so a non-Sealed level can actually inoculate; an unresolved key is a logged skip.
    let mut consortium: Vec<BuiltSpecies> = Vec::new();

    for (key, count) in &cfg.roster {
        if *count == 0 {
            continue; // a zero-count axis contributes no organisms (the proposal allows count_lo == 0 in theory)
        }
        match load_species_file(species_dir.join(format!("{key}.json"))) {
            Ok(built) => roster.push((built, *count)),
            Err(e) => skipped.push((key.clone(), e.to_string())),
        }
    }
    if roster.is_empty() {
        return (None, skipped);
    }

    // Map the containment ordinal → (level, config). Sealed (0) → None (the hash-neutral default: empty
    // schedule, no events). A dirtier level arms the Mode-A airborne consortium so immigration actually fires.
    let containment = if cfg.containment_level == 0 {
        None
    } else {
        let level = match cfg.containment_level {
            1 => ContainmentLevel::Clean,
            2 => ContainmentLevel::Lab,
            _ => ContainmentLevel::Open,
        };
        let consortium_config = ConsortiumConfig::default_mode_a();
        // Pre-load the consortium keys so a scheduled RegionInoculate resolves a genome on replay (ADR-019 R2).
        for key in &consortium_config.species_keys {
            if consortium.iter().any(|b| &b.key == key) {
                continue;
            }
            match load_species_file(species_dir.join(format!("{key}.json"))) {
                Ok(built) => consortium.push(built),
                Err(e) => skipped.push((key.clone(), e.to_string())),
            }
        }
        Some((level, consortium_config))
    };

    let env = EnvParams {
        lat: 0.0,
        lon: 0.0,
        // temp_q is q16 permille (0..=1000 ↔ 0.0..=1.0); avg_temp is the normalized [0,1] climate knob.
        avg_temp: f64::from(cfg.temp_q) / 1000.0,
        season: i64::from(cfg.season),
    };

    let env_config = EnvConfig {
        entity_count: DISCOVER_ENTITY_COUNT,
        env,
        roster,
        species: None,
        consortium,
        containment,
    };
    (Some(env_config), skipped)
}

/// Map a [`SearchConfig`]'s mid-run edit schedule (Variant Lab D) to the harness `(gen, Action::ApplyEdit)`
/// POINT actions [`capture_trace`] consumes — the STAGE-2 wire from the std+serde [`discovery::search::EditGene`]
/// DESCRIPTIONS (inv #1/#5) onto the real `crispr` action surface. Inv #2 holds: the genotype→phenotype gate
/// runs in sim-core; the harness only SCHEDULES the already-existing [`Action::ApplyEdit`].
///
/// Each `EditGene` becomes one `(gen_abs, ApplyEdit)`:
/// - `gen_abs = edit.gen * gens / EDIT_GEN_Q16_DEN` — the q16 run-fraction mapped to an ABSOLUTE generation in
///   `[0, gens)`. Computed from the SAME `gens` on BOTH the capture (score) and the verify (replay) side, so the
///   two stay byte-identical even when a run early-stops at `gem.gens < gens` (an edit beyond `gem.gens` simply
///   never fires on either side). A `gen_abs == 0` edit never fires (the capture loop is `1..=gens`) — a no-op
///   on both sides, so it is round-trip-safe.
/// - `species` resolves `edit.species_index` (an index into the FULL proposed `cfg.roster`, incl. zero-count
///   axes) to the [`sim_core::SpeciesId`] ordinal the env uses. [`env_config_for`] builds the resolved roster as
///   a POSITIONAL, order-preserving filter of `cfg.roster` (it drops zero-count / unresolvable entries), and
///   `reset_with_roster` assigns ids `0..n` in that order — so the SpeciesId is the COUNT of non-zero entries
///   strictly before `species_index` (resolution is POSITIONAL, never by key: a roster key is a file stem, e.g.
///   `ecoli`, while the built species' `key` is the JSON id, e.g. `ecoli-core`). An edit aimed at an absent
///   (zero-count) or out-of-range species is SKIPPED — identically on the capture + verify side, so even an
///   imperfect resolution is round-trip-safe (both sides derive the SAME action list from the SAME inputs).
/// - `target` indexes the chosen species' ACTUAL loci (`edit.target mod loci_len` → that locus' real `LocusId`),
///   so the bare genome-agnostic search locus always resolves to a real locus (inv #2/#5).
/// - `cas` is the build's first seed Cas variant (an `EditGene` carries no Cas — the canonical default).
///
/// An EMPTY schedule (the default `edit_budget == 0`) yields an EMPTY `Vec`, so `capture_trace(.., &[])` is
/// recovered byte-for-byte and the pinned literal `0x47a0_3c8f_6701_f240` is untouched.
///
/// `pub(crate)` so the STARTER-MAP PROMOTE tool ([`crate::promote`]) can rebuild a gem's checkpoint journal
/// against the SAME edit→action mapping the capture/verify path uses (the gem reproducibility contract).
pub(crate) fn edits_to_actions(
    cfg: &SearchConfig,
    roster: &[(BuiltSpecies, u32)],
    gens: u32,
) -> Vec<(u32, Action)> {
    if cfg.edits.is_empty() {
        return Vec::new(); // hash-neutral: the default search (edit_budget 0) recovers capture_trace(.., &[]).
    }
    // The default Cas (the seed table's first variant) — an EditGene names no Cas, so the engine picks the
    // canonical default deterministically. An empty table (impossible by construction) → no edits.
    let Some(cas) = default_cas_variants().first().map(|v| v.id) else {
        return Vec::new();
    };
    let mut actions: Vec<(u32, Action)> = Vec::with_capacity(cfg.edits.len());
    for edit in &cfg.edits {
        // species_index (into the FULL proposed roster) → the SpeciesId the env uses, resolved POSITIONALLY: the
        // resolved roster is cfg.roster's non-zero entries in order, so the id is the count of non-zero entries
        // strictly before species_index — valid only when species_index itself is present (non-zero).
        let idx = edit.species_index as usize;
        let Some((_, count)) = cfg.roster.get(idx) else {
            continue; // index past the proposed roster (defensive) — skip.
        };
        if *count == 0 {
            continue; // the targeted species is absent (zero-count) — no-op, both sides.
        }
        let sid = cfg.roster[..idx].iter().filter(|(_, c)| *c > 0).count();
        let Some((built, _)) = roster.get(sid) else {
            continue; // the position fell outside the resolved roster (a load-failed earlier entry) — skip.
        };
        // Clamp the bare search locus onto a REAL locus of the chosen species' genome (always resolves).
        let loci = &built.genome.loci;
        if loci.is_empty() {
            continue;
        }
        let target = loci[edit.target as usize % loci.len()].id;
        // Rebuild the guide (draw_guide only emits ACGT → always valid; a corrupt config is skipped, not panicked).
        let Ok(guide) = GuideSequence::new(edit.guide.clone().into_bytes()) else {
            continue;
        };
        // q16 run-fraction → absolute generation (SAME `gens` on capture + verify; always < gens for gens >= 1).
        let gen_abs =
            ((u64::from(edit.gen) * u64::from(gens)) / u64::from(EDIT_GEN_Q16_DEN)) as u32;
        actions.push((
            gen_abs,
            Action::ApplyEdit(EditAction {
                cas,
                target,
                guide,
                species: sid as u16,
            }),
        ));
    }
    actions
}

/// Build the round-trip JOURNAL that reproduces a captured run BYTE-IDENTICALLY: it mirrors [`capture_trace`]'s
/// interleave EXACTLY — for `gen` in `1..=gens`, push every POINT action scheduled at that `gen` (in list order,
/// skipping any `Advance` — the loop owns time), then one [`Action::Advance`]`(1)`. With NO scheduled edits this
/// collapses to `Advance(1)*gens`, which is byte-identical (in `run_stats().hash`) to the historical single
/// `Advance(gens)` — proven by `tests/trace_capture.rs::capture_is_hash_neutral_on_a_real_multi_species_run`
/// (one seeded stream, no re-seed) — so an UNEDITED gem's round-trip is unchanged. `gens` here is the CAPTURED
/// `gem.gens` (the capture early-stops at it), so an edit scheduled past it is naturally excluded.
///
/// `pub(crate)` so the STARTER-MAP PROMOTE tool ([`crate::promote`]) can build a gem's GEN-N checkpoint
/// journal (Advance up to gen N with the scheduled edits interleaved) the same way the verify path does.
pub(crate) fn build_journal(actions: &[(u32, Action)], gens: u32) -> Vec<Action> {
    let mut journal: Vec<Action> = Vec::with_capacity(gens as usize + actions.len());
    for gen in 1..=gens {
        for (g, a) in actions {
            if *g == gen && !matches!(a, Action::Advance(_)) {
                journal.push(a.clone());
            }
        }
        journal.push(Action::Advance(1));
    }
    journal
}

/// A RESOLVED mid-run edit — one entry of a loaded gem's edit schedule mapped to the bare, display-ready ids a
/// renderer can show / replay (the LOAD-GEM-REPLAY v2 read-only surface). It is EXACTLY the `(gen_abs,
/// ApplyEdit)` [`edits_to_actions`] produces, flattened: the ABSOLUTE generation, the default Cas id, the REAL
/// resolved [`genome::LocusId`] integer, the ACGT guide string, and the resolved [`sim_core::SpeciesId`] ordinal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedEdit {
    /// ABSOLUTE generation the edit fires at (`gen * gens_requested / EDIT_GEN_Q16_DEN`).
    pub gen_abs: u32,
    /// The Cas-variant id — the build's default (an `EditGene` names no Cas).
    pub cas: u16,
    /// The REAL target locus id (`edit.target mod loci_len` resolved to the chosen species' actual `LocusId`).
    pub target: u32,
    /// The ACGT guide string.
    pub guide: String,
    /// The resolved species ordinal (`SpeciesId`) the edit targets.
    pub species: u16,
}

/// Resolve a loaded [`Gem`]'s mid-run edit schedule to its ABSOLUTE-generation, REAL-locus, REAL-species form —
/// EXACTLY as the capture/verify path ([`edits_to_actions`]) does, for the renderer's READ-ONLY gem-replay
/// preview (LOAD-GEM-REPLAY v2). Resolves the gem's roster through `species_dir` ([`env_config_for`]) and maps
/// the q16 edit fractions against the gem's REQUESTED horizon ([`Gem::gens_requested`], falling back to
/// [`Gem::gens`] for a pre-fix gem where it is `0` — a documented divergence). The result is the SAME ordered
/// schedule [`build_journal`] fires, so a renderer shows precisely the edits the run replays.
///
/// READ-ONLY (inv #2/#3): draws NO `SimRng`, mutates nothing, never folded into the determinism hash — the
/// resolution math lives HERE in the core/harness boundary (NOT in the renderer) and is the ONE definition
/// shared with [`edits_to_actions`]. An empty/unresolvable roster (or a gem with no edits) yields an empty `Vec`.
#[must_use]
pub fn gem_edit_schedule(gem: &Gem, species_dir: &Path) -> Vec<ResolvedEdit> {
    let (env_config, _skipped) = env_config_for(&gem.config, species_dir);
    let Some(env_config) = env_config else {
        return Vec::new(); // the roster no longer resolves — nothing to schedule (guarded, like the verify path).
    };
    // The horizon the capture mapped the q16 fractions against: `gens_requested` for a v2 gem; `gem.gens` (the
    // early-stopped count) for a pre-fix gem whose `gens_requested` defaulted to 0 (the documented divergence).
    let horizon = if gem.gens_requested == 0 {
        gem.gens
    } else {
        gem.gens_requested
    };
    edits_to_actions(&gem.config, &env_config.roster, horizon)
        .into_iter()
        .filter_map(|(gen_abs, action)| match action {
            Action::ApplyEdit(EditAction {
                cas,
                target,
                guide,
                species,
            }) => Some(ResolvedEdit {
                gen_abs,
                cas: cas.0,
                target: target.0,
                // draw_guide only emits ACGT, so the bytes are valid UTF-8 (lossy is a defensive no-op).
                guide: String::from_utf8_lossy(guide.bases()).into_owned(),
                species,
            }),
            // edits_to_actions only ever yields ApplyEdit POINT actions — any other variant is unreachable.
            _ => None,
        })
        .collect()
}

/// [`gem_edit_schedule`] from a gem's JSON TEXT — the renderer/CLI boundary entry (the binding hands the gem
/// file bytes; the core does the parse + resolution so NO biology/resolution math leaks into GDScript, inv #2).
///
/// # Errors
/// Returns a [`serde_json::Error`] if `gem_json` is not a valid serialized [`Gem`].
pub fn gem_edit_schedule_from_json(
    gem_json: &str,
    species_dir: &Path,
) -> serde_json::Result<Vec<ResolvedEdit>> {
    let gem: Gem = serde_json::from_str(gem_json)?;
    Ok(gem_edit_schedule(&gem, species_dir))
}

/// Build a fresh [`GeneSimEnv`] from a resolved [`EnvConfig`] — the SHARED env construction the per-trial scorer
/// ([`score_config`]) and the off-hash key-frame detector ([`crate::keyframe::config_keyframes`]) both run BEFORE
/// [`capture_trace`] (roster + climate + containment + consortium), EXACTLY as `record_episode`/`replay` rebuild
/// it. One definition so a capture's env can never drift between the score path and the preview path (inv #3).
#[must_use]
pub(crate) fn build_env(env_config: &EnvConfig) -> crate::GeneSimEnv {
    let mut env = crate::GeneSimEnv::new(env_config.entity_count);
    env.set_environment(env_config.env);
    env.set_roster(env_config.roster.clone());
    for built in &env_config.consortium {
        env.register_contaminant(built.clone());
    }
    if let Some((level, config)) = &env_config.containment {
        env.set_containment(*level, config.clone());
    }
    env
}

/// Capture + score one [`SearchConfig`] into a [`Gem`] (the per-trial scoring step). Runs the off-hash
/// [`capture_trace`] over `gens` generations of the freshly-built env, threading the config's mid-run CRISPR
/// edit schedule ([`edits_to_actions`] — EMPTY for the default `edit_budget == 0`, so byte-identical to the
/// pre-edit search), scores the trace vs the already-kept fingerprints, and packages the full integer signal
/// set + the reproducible config into a gem.
fn score_config(
    cfg: &SearchConfig,
    env_config: &EnvConfig,
    gens: u32,
    saved_fps: &[[u16; discovery::FP_DIMS]],
) -> Gem {
    // Build the env from the config (roster + climate + containment), exactly as record_episode/replay rebuild it.
    let mut env = build_env(env_config);

    // Off-hash capture of the config's run (with its scheduled mid-run edits), then the D0 score + the save-time
    // novelty multiplier vs the kept set. The actions resolve against the SAME resolved roster the verify side
    // rebuilds, so the captured `recorded_hash` round-trips through `record_episode → replay`.
    let actions = edits_to_actions(cfg, &env_config.roster, gens);
    let trace = capture_trace(&mut env, cfg.master_seed, gens, &actions);
    let scorer = DefaultScorer::default();
    let scored = final_score(&scorer, &trace, saved_fps);
    let sv = scored.score;

    Gem {
        config: cfg.clone(),
        score: scored.final_score,
        quality: sv.quality,
        novelty: scored.novelty_bp.min(u64::from(u16::MAX)) as u16,
        breakdown: sv.breakdown,
        fingerprint: sv.fingerprint,
        recorded_hash: trace.recorded_hash,
        build_id: BUILD_ID.to_string(),
        caption: caption(&sv, cfg),
        gens: trace.g,
        // LOAD-GEM-REPLAY v2: stamp the REQUESTED horizon (the SAME `gens` `edits_to_actions` mapped the q16 edit
        // fractions against) so a loaded gem resolves its mid-run-edit schedule to the IDENTICAL absolute
        // generations — even when the run early-stopped (`trace.g < gens`). Off-hash metadata (inv #3).
        gens_requested: gens,
    }
}

/// The deterministic gem file name: `<final_score>-<master_seed>.json` (zero-padded score so a lexical listing
/// of `data/runs/gems/` is also a rank ordering). No wall-clock — the path is reproducible (mirrors the replay
/// run-id discipline).
#[must_use]
pub fn gem_file_name(gem: &Gem) -> String {
    format!("{:020}-{:016x}.json", gem.score, gem.config.master_seed)
}

/// Run the emergent-run RANDOM SEARCH and write the verified top-`keep` gems to `out_dir` (ADR-024 D2a).
///
/// For `trial` in `0..trials`: PROPOSE a [`SearchConfig`] from the [`SearchSpace::default`] Primordial anchor
/// via the meta-RNG, BUILD a [`GeneSimEnv`] from it (roster via the `data/species/<key>.json` boundary, climate,
/// containment), CAPTURE an off-hash trace over `gens` generations, SCORE it, and CONSIDER it for the deduped
/// top-K [`GemLibrary`]. After the search, for every KEPT gem rebuild the `(seed, EnvConfig, journal)` and
/// `record_episode → assert replay() == recorded_hash` BEFORE writing `<out_dir>/<final_score>-<seed>.json`; a
/// gem that fails the round-trip is DROPPED (logged to stderr), never written.
///
/// Returns the [`GemLibrary`] of gems that PASSED the round-trip and were written (so a dropped gem is absent
/// from the returned library too). Deterministic: same `(search_seed, trials, keep, gens, species_dir)` →
/// identical saved files + scores (the proposal is the only RNG and it is the meta-RNG; the sim runs are pure
/// functions of their configs — the pinned literal is untouched).
///
/// `species_dir` is the `data/species` root the roster keys resolve against (the filesystem boundary; the core
/// stays filesystem-free, inv #2). `out_dir` is created if absent; existing files with a colliding name are
/// overwritten (the name is a pure function of the gem). `evals_path`, when `Some`, writes EVERY evaluated
/// `(config → ScoreVec)` as one JSONL line to that path (D3-A surrogate training data; OFF-HASH — read-only
/// over already-computed gem fields, in evaluation order; `data/runs/*` is gitignored).
///
/// # Errors
/// Returns an [`io::Error`] only for a failure to create `out_dir` or write a gem file (or the eval log). A
/// per-config species resolution failure or a per-gem round-trip failure is handled internally (skip / drop +
/// log), never an error.
pub fn discover(
    search_seed: u64,
    trials: u64,
    keep: usize,
    gens: u32,
    species_dir: &Path,
    out_dir: &Path,
    evals_path: Option<&Path>,
) -> io::Result<GemLibrary> {
    // The Primordial anchor with NO mid-run-edit axis (edit_budget 0) — byte-identical to the pre-Variant-Lab-D
    // search. A caller that wants the edit axis ON calls [`discover_in_space`] with a raised `edit_budget`.
    discover_in_space(
        &SearchSpace::default(),
        search_seed,
        trials,
        keep,
        gens,
        species_dir,
        out_dir,
        evals_path,
    )
}

/// [`discover`] over an EXPLICIT [`SearchSpace`] — the Variant-Lab-D opt-in seam. The space's
/// [`SearchSpace::edit_budget`] turns the mid-run CRISPR edit axis ON (`> 0`) or OFF (`0`, the
/// [`SearchSpace::default`] — byte-identical to [`discover`]). Everything else matches [`discover`]: a fixed
/// `(space, search_seed, trials, keep, gens, species_dir)` produces a byte-identical set of saved gems (the
/// proposal is the meta-RNG; the sim runs are pure functions of their configs — the pinned literal is untouched,
/// and a config with an empty schedule schedules no actions, so its run + round-trip are byte-for-byte the old
/// behaviour).
#[allow(clippy::too_many_arguments)] // independent run parameters; grouping would obscure the CLI mapping
pub fn discover_in_space(
    space: &SearchSpace,
    search_seed: u64,
    trials: u64,
    keep: usize,
    gens: u32,
    species_dir: &Path,
    out_dir: &Path,
    evals_path: Option<&Path>,
) -> io::Result<GemLibrary> {
    let mut lib = GemLibrary::new(keep);
    let mut evals: Vec<EvalRecord> = Vec::with_capacity(usize::try_from(trials).unwrap_or(0));

    // --- SEARCH: propose → build → capture → score → consider, in trial order (deterministic) ---
    for trial in 0..trials {
        let cfg = propose(search_seed, trial, space);
        capture_and_consider(
            &cfg,
            species_dir,
            gens,
            &mut lib,
            &mut evals,
            "trial",
            trial,
        );
    }

    // OFF-HASH eval log: write ALL evaluations in order (D3-A surrogate training data). Independent of gem
    // verification — written as soon as the search completes. `data/runs/*` is gitignored.
    if let Some(path) = evals_path {
        write_eval_log(path, &evals)?;
    }

    verify_and_write_library(&lib, keep, gens, species_dir, out_dir)
}

/// Build, capture, score, and CONSIDER one [`SearchConfig`] into `lib` — the shared per-config step both the
/// random ([`discover`]) and evolutionary ([`discover_evolved`]) loops use. Resolves the roster through the
/// `data/species` boundary (a skip/empty roster is LOGGED + dropped, never a panic), scores against `lib`'s
/// CURRENTLY-kept fingerprints (the save-time novelty multiplier), and folds the gem in. `phase`/`step` only
/// flavour the log line. Returns `true` iff a gem was produced (the config resolved to a non-empty roster).
///
/// **Eval recording (D3-A):** every produced gem is pushed onto `evals` as an [`EvalRecord`] BEFORE
/// `lib.consider` moves the gem, in EVALUATION ORDER — the surrogate trains on the sequence of evaluations as
/// they happened, not just the kept top-K. The record is OFF-HASH: read-only over the fields `score_config`
/// already computed (no `SimRng`/`hash_world` touched; the pinned literal is unmoved).
fn capture_and_consider(
    cfg: &SearchConfig,
    species_dir: &Path,
    gens: u32,
    lib: &mut GemLibrary,
    evals: &mut Vec<EvalRecord>,
    phase: &str,
    step: u64,
) -> bool {
    let (env_config, skipped) = env_config_for(cfg, species_dir);
    for (key, err) in &skipped {
        eprintln!("discover: {phase} {step}: skipped species {key:?} ({err})");
    }
    let Some(env_config) = env_config else {
        eprintln!("discover: {phase} {step}: empty resolved roster — skipped");
        return false;
    };
    let gem = score_config(cfg, &env_config, gens, &lib.fingerprints());
    // OFF-HASH eval record: read-only over the Gem fields score_config already computed. Pushed BEFORE
    // lib.consider moves the gem, in EVALUATION ORDER, so the log captures EVERY evaluation (not just the
    // kept top-K). No SimRng/hash_world change — the pinned literal 0x47a0_3c8f_6701_f240 is untouched.
    evals.push(EvalRecord {
        config: gem.config.clone(),
        quality: gem.quality,
        breakdown: gem.breakdown,
        fingerprint: gem.fingerprint,
        recorded_hash: gem.recorded_hash,
    });
    lib.consider(gem);
    true
}

/// Write the eval log: one [`EvalRecord`] per line as JSON (JSONL), in EVALUATION ORDER. OFF-HASH (read-only
/// over already-computed gem fields — no `SimRng`/`hash_world` touched; the pinned literal is unmoved). The
/// parent dir is created if absent; an existing file is OVERWRITTEN so the log is deterministic per
/// `search_seed` (same seed + same build → byte-identical bytes). `serde_json::to_string` emits struct fields
/// in declaration order, so the bytes are stable across runs (inv #3).
fn write_eval_log(path: &Path, evals: &[EvalRecord]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut buf = String::with_capacity(evals.len() * 256);
    for rec in evals {
        let line = serde_json::to_string(rec)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        buf.push_str(&line);
        buf.push('\n');
    }
    std::fs::write(path, buf)
}

/// VERIFY + WRITE the kept gems: each must round-trip (`record_episode → replay == recorded_hash`) before it is
/// written, so the on-disk library only ever holds reproducible gems; a gem that fails is DROPPED (logged),
/// never written. Returns the [`GemLibrary`] of the gems that PASSED and were written. Shared by [`discover`]
/// and [`discover_evolved`] so the round-trip contract has ONE implementation. `gens` is the REQUESTED horizon
/// (used to map each gem's q16 edit schedule to the SAME absolute generations the capture used — see
/// [`edits_to_actions`]).
fn verify_and_write_library(
    lib: &GemLibrary,
    keep: usize,
    gens: u32,
    species_dir: &Path,
    out_dir: &Path,
) -> io::Result<GemLibrary> {
    std::fs::create_dir_all(out_dir)?;
    let mut verified = GemLibrary::new(keep);
    for gem in &lib.gems {
        let (env_config, _skipped) = env_config_for(&gem.config, species_dir);
        let Some(env_config) = env_config else {
            eprintln!(
                "discover: dropping gem (seed {:016x}): roster no longer resolves",
                gem.config.master_seed
            );
            continue;
        };
        // The gem's journal MIRRORS the capture's interleave (Variant Lab D): per generation `1..=gem.gens`, the
        // edits scheduled at that gen (in list order) then one `Advance(1)`. With NO edits this collapses to
        // `Advance(1)*gem.gens`, byte-identical to one `Advance(gem.gens)` (one seeded stream, no re-seed — proven
        // in tests/trace_capture.rs), so an UNEDITED gem's round-trip is unchanged. The edit actions resolve
        // against the SAME resolved roster + the SAME requested `gens` the capture used (so the absolute edit
        // generations match), and the capture EARLY-STOPS at `gem.gens` (== `trace.g`) — reproducing it exactly.
        let actions = edits_to_actions(&gem.config, &env_config.roster, gens);
        let journal: Vec<Action> = build_journal(&actions, gem.gens);

        // Round-trip the gem through the on-disk record/replay contract into a TEMP subdir, then compare.
        let stage = out_dir.join(format!(".verify-{:016x}", gem.config.master_seed));
        let _ = std::fs::remove_dir_all(&stage);
        let recorded = match record_episode(&env_config, gem.config.master_seed, &journal, &stage) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "discover: dropping gem (seed {:016x}): record failed ({e})",
                    gem.config.master_seed
                );
                let _ = std::fs::remove_dir_all(&stage);
                continue;
            }
        };
        let replayed = match replay(&recorded.dir) {
            Ok(h) => h,
            Err(e) => {
                eprintln!(
                    "discover: dropping gem (seed {:016x}): replay failed ({e})",
                    gem.config.master_seed
                );
                let _ = std::fs::remove_dir_all(&stage);
                continue;
            }
        };
        let _ = std::fs::remove_dir_all(&stage);

        if replayed != recorded.hash || recorded.hash != gem.recorded_hash {
            eprintln!(
                "discover: dropping gem (seed {:016x}): round-trip mismatch (recorded {:016x}, replay {:016x}, gem {:016x})",
                gem.config.master_seed, recorded.hash, replayed, gem.recorded_hash
            );
            continue;
        }

        // The gem reproduces — write it (pretty JSON, git-friendly), keyed by <final_score>-<seed>.
        let path = out_dir.join(gem_file_name(gem));
        let json = serde_json::to_string_pretty(gem)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        std::fs::write(&path, format!("{json}\n"))?;
        verified.consider(gem.clone());
    }

    Ok(verified)
}

/// The fraction of each generation's population that is FRESH RANDOM exploration (vs evolved from the kept
/// parents), in basis points — `2_500` ≈ 1/4 random, 3/4 evolved. Keeps the search from collapsing onto the
/// current pool (a deterministic, seeded explore/exploit split — NOT an RNG coin; the index threshold below).
const EVOLVE_EXPLORE_BP: u64 = 2_500;
/// The basis-point denominator for [`EVOLVE_EXPLORE_BP`] (== `discovery::fixed::SCALE`, kept local so this
/// crate stays free of a fixed-point import for one constant). `10_000` bp = 100%.
const BP_SCALE: u64 = 10_000;

/// Run the EVOLUTIONARY emergent-run SEARCH (D2b STAGE 2) and write the verified top-`keep` gems to `out_dir`.
///
/// GENERATION 0 proposes `pop_size` RANDOM configs (the D2a [`propose`]), builds/captures/scores each, and
/// folds them into a running [`GemLibrary`] (the kept gems are the PARENTS). For each subsequent generation
/// (`1..generations`) it proposes `pop_size` NEW configs: a leading EXPLORE fraction ([`EVOLVE_EXPLORE_BP`])
/// is fresh [`propose`] (so the search never collapses onto the pool), the rest are
/// [`propose_evolved`](discovery::search::propose_evolved) of the CURRENT kept gems' configs (mutate/crossover
/// of the parents). Every individual is built/captured/scored and folded in; the library carries the elites
/// forward (elitist — a strong parent survives until beaten). After all generations the kept gems are
/// round-trip-verified and written (the UNCHANGED [`verify_and_write_library`] contract — a gem that fails the
/// `record_episode → replay == recorded_hash` check is DROPPED, never written).
///
/// `generations == 0` reduces to a single random generation of `pop_size` trials — i.e. exactly the D2a
/// [`discover`] behaviour with `trials == pop_size` (the non-evolutionary base case).
///
/// ## Determinism (inv #3)
/// Every proposal/operator draw is the META-RNG (splitmix over `(search_seed, step, field)` — see
/// [`discovery::search`]); the per-generation `step` is a fixed function of `(generation, individual)`
/// (`generation * pop_size + i`), so a fixed `(search_seed, pop_size, generations, keep, gens, species_dir)`
/// produces a byte-identical set of saved gems. The sim runs are pure functions of their configs — the search
/// adds NO sim-path change, so the pinned literal `0x47a0_3c8f_6701_f240` is untouched.
///
/// # Errors
/// Returns an [`io::Error`] only for a failure to create `out_dir` or write a gem file (mirrors [`discover`]);
/// or the eval log when `evals_path` is `Some`.
#[allow(clippy::too_many_arguments)] // the 8 knobs are all independent run parameters; grouping would obscure the CLI mapping
pub fn discover_evolved(
    search_seed: u64,
    pop_size: u64,
    generations: u64,
    keep: usize,
    gens: u32,
    species_dir: &Path,
    out_dir: &Path,
    evals_path: Option<&Path>,
) -> io::Result<GemLibrary> {
    // The widened Primordial anchor with NO mid-run-edit axis (edit_budget 0) — byte-identical to pre-Variant-
    // Lab-D. A caller wanting the edit axis ON calls [`discover_evolved_in_space`] with a raised `edit_budget`.
    discover_evolved_in_space(
        &SearchSpace::default(),
        search_seed,
        pop_size,
        generations,
        keep,
        gens,
        species_dir,
        out_dir,
        evals_path,
    )
}

/// [`discover_evolved`] over an EXPLICIT [`SearchSpace`] — the Variant-Lab-D opt-in seam (mirrors
/// [`discover_in_space`]). The space's [`SearchSpace::edit_budget`] turns the mid-run CRISPR edit axis ON
/// (`> 0`) or OFF (`0`, the [`SearchSpace::default`] — byte-identical to [`discover_evolved`]); every proposal
/// AND every evolutionary operator child draws its edit schedule from the same space, so a raised budget threads
/// edits through the whole generational loop. Determinism + the round-trip contract are unchanged.
#[allow(clippy::too_many_arguments)] // independent run parameters; grouping would obscure the CLI mapping
pub fn discover_evolved_in_space(
    space: &SearchSpace,
    search_seed: u64,
    pop_size: u64,
    generations: u64,
    keep: usize,
    gens: u32,
    species_dir: &Path,
    out_dir: &Path,
    evals_path: Option<&Path>,
) -> io::Result<GemLibrary> {
    // The COLD start: a fresh (empty) library and NO gen-0 anchor — generation 0 is all random proposals. This
    // is the byte-identical historical behaviour (the [`discover_evolved_core`] explorer cut + step coordinates
    // are unchanged for `anchor_gen0 == false` over an empty pool).
    discover_evolved_core(
        space,
        search_seed,
        pop_size,
        generations,
        keep,
        gens,
        species_dir,
        out_dir,
        evals_path,
        GemLibrary::new(keep),
        false,
    )
}

/// The shared EVOLUTIONARY loop body behind [`discover_evolved_in_space`] (COLD start) and [`discover_from_gem`]
/// (CONTINUE-FROM-GEM). It runs `generations + 1` generations of propose → capture → score → consider over the
/// running `lib`, then round-trip-verifies + writes the kept gems via the UNCHANGED [`verify_and_write_library`].
///
/// `lib` is the INITIAL library: empty for a cold start, or PRE-SEEDED with the anchor gem (the elite gen-0
/// parent) for a continue-from-gem run. `anchor_gen0` selects gen-0's behaviour:
/// - `false` (cold) → generation 0 is ALL fresh random [`propose`] (the historical D2b base case). With an empty
///   `lib` this reproduces [`discover_evolved_in_space`]'s exact byte stream.
/// - `true` (anchored) → generation 0's EXPLOIT fraction (`i >= explore`) draws [`propose_evolved`] off the
///   pre-seeded pool (mutate/crossover of the anchor), so the search branches OFF the gem from the very first
///   generation instead of a cold random start; only the leading `explore` cut stays fresh exploration.
///
/// ## Determinism (inv #3)
/// Every proposal/operator draw is the META-RNG over `(search_seed, step, field)`; `step = generation*pop_size+i`
/// is a fixed function of `(generation, individual)`. The pre-seeded anchor + `anchor_gen0` only change WHICH
/// proposal function each gen-0 individual calls (both fed the SAME `step`) and the initial parent pool — never
/// the sim path. The sim runs stay pure functions of their configs, so the pinned literal `0x47a0_3c8f_6701_f240`
/// is untouched.
#[allow(clippy::too_many_arguments)] // independent run parameters; grouping would obscure the call sites
fn discover_evolved_core(
    space: &SearchSpace,
    search_seed: u64,
    pop_size: u64,
    generations: u64,
    keep: usize,
    gens: u32,
    species_dir: &Path,
    out_dir: &Path,
    evals_path: Option<&Path>,
    mut lib: GemLibrary,
    anchor_gen0: bool,
) -> io::Result<GemLibrary> {
    let total_evals = pop_size.saturating_mul(generations + 1);
    let mut evals: Vec<EvalRecord> = Vec::with_capacity(usize::try_from(total_evals).unwrap_or(0));

    // The explore cut: the leading `explore` individuals of every post-0 generation are fresh random proposals,
    // the rest are evolved from the parents. At least 1 explorer per generation (a degenerate pop_size still
    // injects fresh blood); never more than the whole population.
    let explore = ((pop_size * EVOLVE_EXPLORE_BP / BP_SCALE).max(1)).min(pop_size);

    // The total number of individuals (== meta-RNG steps) is `pop_size * (generations + 1)`: one (random or
    // anchored) generation 0 plus `generations` evolved generations. `step` is monotonic across the whole run so
    // no two individuals share a proposal stream coordinate.
    let total_generations = generations + 1;
    for generation in 0..total_generations {
        // The PARENTS for this generation are the CURRENTLY-kept gems' configs (the elites). Snapshotted before
        // proposing this generation's children so the pool is stable within the generation (deterministic). For a
        // continue-from-gem run the pre-seeded anchor is already in `lib`, so it is the gen-0 parent.
        let parents: Vec<SearchConfig> = lib.gems.iter().map(|g| g.config.clone()).collect();

        for i in 0..pop_size {
            // A globally-monotonic meta-RNG step so every individual draws an independent proposal stream.
            let step = generation * pop_size + i;
            // COLD generation 0 (no anchor) → all fresh random. ANCHORED generation 0 → the leading `explore`
            // cut stays fresh, the rest branch off the pre-seeded pool. Every later generation: explore cut fresh,
            // the rest evolved. `i < explore || (generation == 0 && !anchor_gen0)` collapses to the historical
            // `generation == 0 || i < explore` when `anchor_gen0 == false` (byte-identical cold path).
            let cold = i < explore || (generation == 0 && !anchor_gen0);
            let cfg = if cold {
                // Fresh random proposal (cold start / the per-generation EXPLORE fraction).
                propose(search_seed, step, space)
            } else {
                // The EXPLOIT fraction → mutate/crossover of the current elite pool (empty pool → cold propose,
                // handled inside propose_evolved). Drawn off the evolve stream salt (disjoint from propose).
                propose_evolved(&parents, search_seed, step, space)
            };
            capture_and_consider(
                &cfg,
                species_dir,
                gens,
                &mut lib,
                &mut evals,
                "gen",
                generation,
            );
        }
    }

    // OFF-HASH eval log: write ALL evaluations in generation×individual order (D3-A surrogate training data).
    if let Some(path) = evals_path {
        write_eval_log(path, &evals)?;
    }

    verify_and_write_library(&lib, keep, gens, species_dir, out_dir)
}

/// CONTINUE-FROM-GEM (the auto-research lead): branch the EVOLUTIONARY search OFF a saved gem instead of a cold
/// random start. Reads + serde-parses the [`Gem`] at `gem_path`, PRE-SEEDS the [`GemLibrary`] with its
/// [`SearchConfig`] as the gen-0 ANCHOR/elite, then runs the SAME [`discover_evolved_core`] machinery with
/// `anchor_gen0 = true` so generation 0's exploit fraction is already [`propose_evolved`] (mutate/crossover) of
/// the gem's roster + env + edits. The kept gems (the anchor + its branched descendants) are round-trip-verified
/// and written by the UNCHANGED [`verify_and_write_library`] contract — a gem that fails `record_episode →
/// replay == recorded_hash` is DROPPED, never written.
///
/// The SOURCE gem is re-verified on the CURRENT build before the search ([`verify_source_gem`]): a `build_id`
/// mismatch or a round-trip mismatch is LOGGED as stale/incompatible but does NOT abort — its config still
/// anchors the branching (a stale anchor is simply dropped at write time while its fresh-hashed children are
/// written). The gem's [`SearchSpace`] is kept consistent: `space` when `Some`, else the widened
/// [`SearchSpace::default`] with `edit_budget` set to the anchor's edit count (so the operators can reproduce the
/// gem's mid-run-edit axis when it carries edits).
///
/// ## Determinism (inv #3)
/// The proposal is the META-RNG and the sim runs are pure functions of their configs, so a fixed `(gem,
/// search_seed, pop_size, generations, keep, gens, space, species_dir)` produces a byte-identical set of saved
/// gems. This runner is std/serde meta-level only — the pinned literal `0x47a0_3c8f_6701_f240` is untouched.
///
/// # Errors
/// Returns an [`io::Error`] for a failure to READ/PARSE the gem file, to create `out_dir`, or to write a gem
/// file / the eval log. A stale/unreproducible SOURCE gem is logged (not an error); a per-child round-trip
/// failure is handled internally (dropped + logged) by [`verify_and_write_library`].
#[allow(clippy::too_many_arguments)] // independent run parameters; grouping would obscure the CLI mapping
pub fn discover_from_gem(
    gem_path: &Path,
    space: Option<&SearchSpace>,
    search_seed: u64,
    pop_size: u64,
    generations: u64,
    keep: usize,
    gens: u32,
    species_dir: &Path,
    out_dir: &Path,
    evals_path: Option<&Path>,
) -> io::Result<GemLibrary> {
    // (a) READ + serde-parse the saved gem (std/serde meta-level — no sim run touched).
    let text = std::fs::read_to_string(gem_path)?;
    let gem: Gem = serde_json::from_str(&text).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("parse gem {}: {e}", gem_path.display()),
        )
    })?;

    // Resolve the search space consistent with the anchor: an explicit `space`, else the widened default with the
    // gem's edit budget folded in (so mutate/propose can reproduce the anchor's mid-run-edit axis when present).
    let owned_default;
    let space: &SearchSpace = match space {
        Some(s) => s,
        None => {
            let edit_budget = u8::try_from(gem.config.edits.len()).unwrap_or(u8::MAX);
            owned_default = SearchSpace {
                edit_budget,
                ..SearchSpace::default()
            };
            &owned_default
        }
    };

    // Re-verify the SOURCE gem on this build (the gem reproducibility contract) — logs stale/incompatible but
    // never aborts; the config still anchors the branching. `out_dir` hosts the throwaway stage subdir, so make
    // sure it exists first (verify_and_write_library also creates it — idempotent).
    std::fs::create_dir_all(out_dir)?;
    verify_source_gem(&gem, gens, species_dir, out_dir);

    // (b) PRE-SEED the library with the gem as the gen-0 ANCHOR/elite (an empty library's first candidate is
    // always kept: novelty_l1 over an empty set is SCALE == dedup_min, not < it), then run the evolutionary
    // generations with `anchor_gen0 = true` so propose_evolved branches off the gem from generation 0.
    let mut lib = GemLibrary::new(keep);
    lib.consider(gem);

    discover_evolved_core(
        space,
        search_seed,
        pop_size,
        generations,
        keep,
        gens,
        species_dir,
        out_dir,
        evals_path,
        lib,
        true,
    )
}

/// Re-verify a SOURCE gem on the CURRENT build before continuing from it (the gem reproducibility contract,
/// inv #3/#7). LOGS a clear note for each of: a `build_id` mismatch (stale/incompatible build), a roster that no
/// longer resolves, a record/replay failure, or a round-trip hash mismatch — but NEVER aborts: a stale anchor is
/// still a valid branching point (its descendants get fresh, reproducible hashes; the stale anchor itself is
/// dropped at write time by [`verify_and_write_library`]). Off-hash meta-level only (record/replay into a
/// throwaway `out_dir/.verify-source` stage that is removed) — the pinned literal is untouched.
fn verify_source_gem(gem: &Gem, gens: u32, species_dir: &Path, out_dir: &Path) {
    let seed = gem.config.master_seed;
    if gem.build_id != BUILD_ID {
        eprintln!(
            "discover: source gem (seed {seed:016x}) build_id {:?} != current {BUILD_ID:?} — stale/incompatible; branching anyway (its score is recomputed by replay; a stale anchor is dropped at write time)",
            gem.build_id
        );
    }

    let (env_config, skipped) = env_config_for(&gem.config, species_dir);
    for (key, err) in &skipped {
        eprintln!("discover: source gem (seed {seed:016x}): skipped species {key:?} ({err})");
    }
    let Some(env_config) = env_config else {
        eprintln!(
            "discover: source gem (seed {seed:016x}): roster no longer resolves — branching off the raw config"
        );
        return;
    };

    // Mirror verify_and_write_library's journal exactly (so a re-verify here matches the write-time check).
    let actions = edits_to_actions(&gem.config, &env_config.roster, gens);
    let journal: Vec<Action> = build_journal(&actions, gem.gens);

    let stage = out_dir.join(".verify-source");
    let _ = std::fs::remove_dir_all(&stage);
    let outcome = record_episode(&env_config, seed, &journal, &stage).and_then(|recorded| {
        // `replay` now yields a typed `ReplayError`; bridge it back to `io::Error` so this `and_then` chain
        // (whose error type is `record_episode`'s `io::Error`) keeps its single error type (off-hash meta-level).
        replay(&recorded.dir)
            .map(|replayed| (recorded.hash, replayed))
            .map_err(io::Error::from)
    });
    let _ = std::fs::remove_dir_all(&stage);

    match outcome {
        Ok((recorded_hash, replayed))
            if replayed == recorded_hash && recorded_hash == gem.recorded_hash =>
        {
            eprintln!(
                "discover: source gem (seed {seed:016x}) re-verified on this build (hash {recorded_hash:016x}) — anchoring"
            );
        }
        Ok((recorded_hash, replayed)) => {
            eprintln!(
                "discover: source gem (seed {seed:016x}) does NOT reproduce on this build (recorded {recorded_hash:016x}, replay {replayed:016x}, gem {:016x}) — stale; branching off the config anyway",
                gem.recorded_hash
            );
        }
        Err(e) => {
            eprintln!(
                "discover: source gem (seed {seed:016x}) record/replay failed ({e}) — branching off the config anyway"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The repo-root `data/species` dir (the byte-mover boundary; mirrors the species/replay test helpers).
    fn species_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/species"))
    }

    #[test]
    fn env_config_maps_roster_climate_containment() {
        // A proposed config resolves its roster keys through the data dir and maps temp/season/containment.
        let cfg = SearchConfig {
            master_seed: 7,
            roster: vec![("default".to_string(), 400), ("ecoli".to_string(), 200)],
            containment_level: 2, // Lab → a (level, config) pair, Mode-A consortium pre-loaded
            temp_q: 600,
            season: 1,
            edits: Vec::new(),
        };
        let (env_config, skipped) = env_config_for(&cfg, &species_dir());
        let env_config = env_config.expect("roster resolves");
        assert!(skipped.is_empty(), "all keys resolve: {skipped:?}");
        assert_eq!(env_config.roster.len(), 2);
        assert!(
            (env_config.env.avg_temp - 0.6).abs() < 1e-9,
            "temp_q 600 → 0.6"
        );
        assert_eq!(env_config.env.season, 1);
        assert!(
            matches!(env_config.containment, Some((ContainmentLevel::Lab, _))),
            "containment ordinal 2 → Lab"
        );
        assert!(
            !env_config.consortium.is_empty(),
            "a non-Sealed level pre-loads the Mode-A consortium so immigration resolves"
        );
        // Sealed (0) → None (hash-neutral).
        let sealed = SearchConfig {
            containment_level: 0,
            ..cfg
        };
        let (sealed_cfg, _) = env_config_for(&sealed, &species_dir());
        assert!(
            sealed_cfg.unwrap().containment.is_none(),
            "Sealed → no containment"
        );
    }

    // ---- D3-A: EvalRecord emission + byte-reproducibility ----

    /// A tiny but real config (resolves through the data dir → a real off-hash run → a real recorded_hash).
    fn tiny_config(seed: u64) -> SearchConfig {
        SearchConfig {
            master_seed: seed,
            roster: vec![("default".to_string(), 300)],
            containment_level: 0, // Sealed → hash-neutral default
            temp_q: 500,
            season: 0,
            edits: Vec::new(),
        }
    }

    #[test]
    fn capture_and_consider_emits_eval_record_mirroring_gem() {
        // The EvalRecord pushed onto `evals` must carry EXACTLY the Gem's (config, quality, breakdown,
        // fingerprint, recorded_hash) — the surrogate trains on the same numbers gems carry. OFF-HASH: built
        // from fields score_config already computed (no SimRng/hash_world change).
        let cfg = tiny_config(0xABCD_1234);
        let (env_config, skipped) = env_config_for(&cfg, &species_dir());
        assert!(skipped.is_empty(), "all keys resolve: {skipped:?}");
        let env_config = env_config.expect("roster resolves");

        // Score the config directly to get the reference Gem.
        let gens = 40;
        let gem = score_config(&cfg, &env_config, gens, &[]);

        // Drive capture_and_consider and capture the emitted EvalRecord.
        let mut lib = GemLibrary::new(8);
        let mut evals: Vec<EvalRecord> = Vec::new();
        let produced =
            capture_and_consider(&cfg, &species_dir(), gens, &mut lib, &mut evals, "test", 0);
        assert!(produced, "the config resolves to a non-empty roster");
        assert_eq!(evals.len(), 1, "exactly one eval record is emitted");
        let rec = &evals[0];

        // The record mirrors the Gem's (config, quality, breakdown, fingerprint, recorded_hash).
        assert_eq!(rec.config, gem.config, "config must mirror the gem");
        assert_eq!(rec.quality, gem.quality, "quality must mirror the gem");
        assert_eq!(
            rec.breakdown, gem.breakdown,
            "breakdown must mirror the gem"
        );
        assert_eq!(
            rec.fingerprint, gem.fingerprint,
            "fingerprint must mirror the gem"
        );
        assert_eq!(
            rec.recorded_hash, gem.recorded_hash,
            "recorded_hash must mirror the gem"
        );
    }

    /// A RAII temp dir guard (the harness has no `tempfile` dep — std-only cleanup). Removes the dir on drop.
    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let mut p = std::env::temp_dir();
            p.push(format!("gene-sim-eval-test-{label}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).expect("create temp dir");
            TempDir(p)
        }
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn discover_eval_log_is_byte_reproducible_per_seed() {
        // The D3-A contract: same `search_seed` → byte-identical `<search_seed>.jsonl`. Two independent runs
        // over the SAME seed produce the SAME bytes (determinism inv #3 — the eval log is OFF-HASH: read-only
        // over already-computed gem fields; no SimRng/hash_world change).
        let tmp = TempDir::new("repro");
        let path_a = tmp.path().join("evals.jsonl");
        let path_b = tmp.path().join("evals_b.jsonl");
        let out_dir = tmp.path().join("gems");
        let trials = 6; // small + fast, but enough to exercise multiple distinct configs
        let gens = 40;

        // Run A.
        discover(42, trials, 4, gens, &species_dir(), &out_dir, Some(&path_a)).expect("discover A");
        let bytes_a = std::fs::read(&path_a).expect("read A");

        // Run B (fresh lib, fresh file — overwrites path_b).
        discover(42, trials, 4, gens, &species_dir(), &out_dir, Some(&path_b)).expect("discover B");
        let bytes_b = std::fs::read(&path_b).expect("read B");

        assert_eq!(
            bytes_a, bytes_b,
            "same search_seed → byte-identical eval log (D3-A determinism contract)"
        );

        // A different seed produces a DIFFERENT log (so we're not trivially writing a constant).
        let path_c = tmp.path().join("evals_c.jsonl");
        discover(43, trials, 4, gens, &species_dir(), &out_dir, Some(&path_c))
            .expect("discover C (different seed)");
        let bytes_c = std::fs::read(&path_c).expect("read C");
        assert_ne!(
            bytes_a, bytes_c,
            "different search_seed should produce a different eval log"
        );

        // Every line is valid JSON (an EvalRecord) and the line count == trials (every evaluation logged).
        let text = std::str::from_utf8(&bytes_a).expect("utf8");
        let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(
            lines.len(),
            trials as usize,
            "expected one eval record per trial, got {}",
            lines.len()
        );
        for (i, line) in lines.iter().enumerate() {
            serde_json::from_str::<discovery::search::EvalRecord>(line)
                .unwrap_or_else(|e| panic!("line {i} is not a valid EvalRecord: {e} — {line:?}"));
        }
    }

    #[test]
    fn discover_evolved_eval_log_is_byte_reproducible_per_seed() {
        // The evolutionary loop must also produce a byte-identical eval log per seed (generation×individual
        // order). generations=0 reduces to a single random generation (the D2a base case).
        let tmp = TempDir::new("evolved");
        let path_a = tmp.path().join("evals.jsonl");
        let path_b = tmp.path().join("evals_b.jsonl");
        let out_dir = tmp.path().join("gems");
        let pop_size = 4;
        let gens = 40;

        discover_evolved(
            99,
            pop_size,
            1, // one evolved generation (gen 0 random + gen 1 evolved)
            4,
            gens,
            &species_dir(),
            &out_dir,
            Some(&path_a),
        )
        .expect("discover_evolved A");
        let bytes_a = std::fs::read(&path_a).expect("read A");

        discover_evolved(
            99,
            pop_size,
            1,
            4,
            gens,
            &species_dir(),
            &out_dir,
            Some(&path_b),
        )
        .expect("discover_evolved B");
        let bytes_b = std::fs::read(&path_b).expect("read B");

        assert_eq!(
            bytes_a, bytes_b,
            "same search_seed → byte-identical evolved eval log"
        );

        // Line count == pop_size * (generations + 1) — every individual is logged, in gen×individual order.
        let text = std::str::from_utf8(&bytes_a).expect("utf8");
        let n = text.lines().filter(|l| !l.is_empty()).count();
        assert_eq!(
            n,
            (pop_size * 2) as usize,
            "expected pop_size*2 eval records, got {n}"
        );
    }

    #[test]
    fn discover_without_evals_path_writes_no_log() {
        // `evals_path = None` → no eval log written (the feature is opt-in via --save-evals).
        let tmp = TempDir::new("none");
        let out_dir = tmp.path().join("gems");
        let stray = tmp.path().join("evals.jsonl");
        discover(7, 4, 4, 30, &species_dir(), &out_dir, None).expect("discover");
        assert!(
            !stray.exists(),
            "no eval log should be written when evals_path is None"
        );
    }

    // ---- Variant Lab D (STAGE 2): the mid-run edit wire ----

    /// A config carrying a real mid-run edit schedule: two edits at distinct relative points, two present
    /// species. Maps to two `ApplyEdit` actions at gens ~18 and ~36 over a 60-gen run.
    fn edited_config(seed: u64) -> SearchConfig {
        use discovery::search::EditGene;
        SearchConfig {
            master_seed: seed,
            roster: vec![("default".to_string(), 600), ("ecoli".to_string(), 300)],
            containment_level: 0,
            temp_q: 500,
            season: 0,
            edits: vec![
                // ~0.30 of the run, species 0 (default), locus index 0.
                EditGene {
                    gen: 20_000,
                    species_index: 0,
                    target: 0,
                    guide: "ACGTACGTACGTACGTACGT".to_string(),
                },
                // ~0.61 of the run, species 1 (ecoli), locus index 3.
                EditGene {
                    gen: 40_000,
                    species_index: 1,
                    target: 3,
                    guide: "TTTTGGGGCCCCAAAATTTT".to_string(),
                },
            ],
        }
    }

    #[test]
    fn no_edits_yields_empty_actions_and_an_unedited_journal() {
        // (a) edit_budget 0 (empty schedule) recovers EXACTLY the old behaviour: no scheduled actions, and the
        // journal is `Advance(1)*gens` — byte-identical-in-hash to the historical single Advance(gens).
        let cfg = tiny_config(0x55); // edits: Vec::new()
        let (env_config, _) = env_config_for(&cfg, &species_dir());
        let env_config = env_config.expect("roster resolves");
        let actions = edits_to_actions(&cfg, &env_config.roster, 40);
        assert!(actions.is_empty(), "an empty schedule schedules no actions");
        let journal = build_journal(&actions, 7);
        assert_eq!(
            journal,
            vec![Action::Advance(1); 7],
            "no edits → Advance(1)*gens"
        );
    }

    #[test]
    fn edited_config_round_trips_and_genuinely_edits() {
        // (b) THE LOAD-BEARING PROOF: a config carrying mid-run edits captures a recorded_hash that ROUND-TRIPS
        // through the on-disk record_episode → replay contract (inv #3), AND the edits genuinely change the run
        // (a different hash than the SAME config with NO edits — the wire is load-bearing, not a silent no-op).
        let gens = 60u32;
        let cfg = edited_config(0xED17);
        let (env_config, skipped) = env_config_for(&cfg, &species_dir());
        assert!(skipped.is_empty(), "roster resolves: {skipped:?}");
        let env_config = env_config.expect("roster resolves");

        // The two edits resolve to two real ApplyEdit actions at distinct in-range generations.
        let actions = edits_to_actions(&cfg, &env_config.roster, gens);
        assert_eq!(actions.len(), 2, "both edits map to actions");
        assert!(actions
            .iter()
            .all(|(g, a)| *g >= 1 && *g < gens && matches!(a, Action::ApplyEdit(_))));

        // Score the EDITED config (capture WITH the edits) → its recorded_hash.
        let gem = score_config(&cfg, &env_config, gens, &[]);

        // The EDITS GENUINELY CHANGE THE RUN: the same config with NO edits captures a different hash.
        let unedited = SearchConfig {
            edits: Vec::new(),
            ..cfg.clone()
        };
        let unedited_gem = score_config(&unedited, &env_config, gens, &[]);
        assert_ne!(
            gem.recorded_hash, unedited_gem.recorded_hash,
            "the mid-run edits must genuinely change the run (the wire is load-bearing)"
        );

        // ROUND-TRIP: build the verify journal exactly as verify_and_write_library does, record + replay it,
        // and assert it reproduces the captured hash bit-for-bit.
        let journal = build_journal(&actions, gem.gens);
        let tmp = TempDir::new("edited_rt");
        let recorded =
            record_episode(&env_config, cfg.master_seed, &journal, tmp.path()).expect("record");
        let replayed = replay(&recorded.dir).expect("replay");
        assert_eq!(
            recorded.hash, gem.recorded_hash,
            "the verify journal reproduces the captured recorded_hash"
        );
        assert_eq!(
            replayed, recorded.hash,
            "record → replay is bit-identical (inv #3) for an edited gem"
        );
    }

    // ---- LOAD-GEM-REPLAY v2: gem_edit_schedule fidelity (gens_requested) ----

    /// A config with mid-run edits on the LOW-LOCI `default` species (4 loci) AND on `ecoli` (136 loci), with
    /// target indices that exercise the `target mod loci_len` resolution (target 5 on a 4-loci genome → locus 1).
    fn low_loci_edited_config(seed: u64) -> SearchConfig {
        use discovery::search::EditGene;
        SearchConfig {
            master_seed: seed,
            roster: vec![("default".to_string(), 500), ("ecoli".to_string(), 200)],
            containment_level: 0,
            temp_q: 500,
            season: 0,
            edits: vec![
                // default species (4 loci): target 0 → real LocusId 0.
                EditGene {
                    gen: 10_000,
                    species_index: 0,
                    target: 0,
                    guide: "ACGTACGTACGTACGTACGT".to_string(),
                },
                // default species (4 loci): target 5 → 5 % 4 = 1 → real LocusId 1 (exercises the low-loci modulo).
                EditGene {
                    gen: 30_000,
                    species_index: 0,
                    target: 5,
                    guide: "TTTTGGGGCCCCAAAATTTT".to_string(),
                },
                // ecoli species (136 loci): target 7 → real LocusId 7.
                EditGene {
                    gen: 50_000,
                    species_index: 1,
                    target: 7,
                    guide: "GGGGCCCCAAAATTTTGGGG".to_string(),
                },
            ],
        }
    }

    /// A hand-built [`Gem`] over `cfg` with explicit `(gens, gens_requested)` — to exercise the horizon
    /// fallback/fidelity branches without forcing a real early-stop (the score fields are inert here).
    fn gem_from(cfg: &SearchConfig, gens: u32, gens_requested: u32) -> Gem {
        Gem {
            config: cfg.clone(),
            score: 0,
            quality: 0,
            novelty: 0,
            breakdown: [0; 6],
            fingerprint: [0; discovery::FP_DIMS],
            recorded_hash: 0,
            build_id: BUILD_ID.to_string(),
            caption: String::new(),
            gens,
            gens_requested,
        }
    }

    /// The expected schedule as comparable tuples, straight from [`edits_to_actions`] at `horizon` (the
    /// canonical resolver gem_edit_schedule must reproduce).
    fn expected_schedule(
        cfg: &SearchConfig,
        roster: &[(BuiltSpecies, u32)],
        horizon: u32,
    ) -> Vec<(u32, u16, u32, String, u16)> {
        edits_to_actions(cfg, roster, horizon)
            .into_iter()
            .filter_map(|(g, a)| match a {
                Action::ApplyEdit(EditAction {
                    cas,
                    target,
                    guide,
                    species,
                }) => Some((
                    g,
                    cas.0,
                    target.0,
                    String::from_utf8_lossy(guide.bases()).into_owned(),
                    species,
                )),
                _ => None,
            })
            .collect()
    }

    /// gem_edit_schedule's output as the same comparable tuples.
    fn actual_schedule(gem: &Gem) -> Vec<(u32, u16, u32, String, u16)> {
        gem_edit_schedule(gem, &species_dir())
            .into_iter()
            .map(|e| (e.gen_abs, e.cas, e.target, e.guide, e.species))
            .collect()
    }

    #[test]
    fn gem_edit_schedule_matches_edits_to_actions() {
        // A REAL v2 gem (score_config stamps gens_requested = gens): the resolver reproduces edits_to_actions
        // field-for-field (gen_abs + resolved LocusId + species + cas + guide), incl. the low-loci modulo and
        // through the JSON-text boundary the renderer uses.
        let gens = 80u32;
        let cfg = low_loci_edited_config(0x5CED_0001);
        let (env_config, skipped) = env_config_for(&cfg, &species_dir());
        assert!(skipped.is_empty(), "roster resolves: {skipped:?}");
        let env_config = env_config.expect("roster resolves");

        let gem = score_config(&cfg, &env_config, gens, &[]);
        assert_eq!(
            gem.gens_requested, gens,
            "score_config stamps the requested horizon"
        );

        let expected = expected_schedule(&cfg, &env_config.roster, gem.gens_requested);
        let actual = actual_schedule(&gem);
        assert_eq!(
            actual, expected,
            "gem_edit_schedule must match edits_to_actions exactly"
        );

        // The low-loci modulo resolved correctly: default has 4 loci, so target 5 → real LocusId 1.
        assert_eq!(actual.len(), 3, "all three edits resolve");
        assert_eq!(
            actual[1].2, 1,
            "default target 5 mod 4 loci → real LocusId 1"
        );
        // species ordinals: default → SpeciesId 0, ecoli → SpeciesId 1 (positional resolution).
        assert_eq!((actual[0].4, actual[2].4), (0, 1), "resolved SpeciesIds");

        // build_journal fires the SAME resolved edits at their gen_abs (the schedule the verify path replays).
        let journal = build_journal(
            &edits_to_actions(&cfg, &env_config.roster, gem.gens),
            gem.gens,
        );
        for (gen_abs, _, _, _, _) in &actual {
            assert!(*gen_abs < gem.gens, "every edit fires within the run");
        }
        assert_eq!(
            journal
                .iter()
                .filter(|a| matches!(a, Action::ApplyEdit(_)))
                .count(),
            actual.len(),
            "the journal carries one ApplyEdit per resolved edit"
        );

        // The JSON-text boundary (the godot path) yields the identical schedule.
        let json = serde_json::to_string(&gem).expect("serialize gem");
        let via_json: Vec<_> = gem_edit_schedule_from_json(&json, &species_dir())
            .expect("parse")
            .into_iter()
            .map(|e| (e.gen_abs, e.cas, e.target, e.guide, e.species))
            .collect();
        assert_eq!(via_json, expected, "the JSON-text resolver matches too");
    }

    #[test]
    fn gem_edit_schedule_uses_requested_horizon_not_early_stop() {
        // THE FIDELITY FIX: an EARLY-STOPPED gem (gem.gens < gens_requested) resolves its schedule against the
        // REQUESTED horizon (what the capture/verify path used), NOT the early-stopped count — else the edits
        // land at the WRONG absolute generations.
        let requested = 200u32;
        let early_stop = 50u32;
        let cfg = low_loci_edited_config(0x5CED_0002);
        let (env_config, _) = env_config_for(&cfg, &species_dir());
        let env_config = env_config.expect("roster resolves");

        let gem = gem_from(&cfg, early_stop, requested);
        let actual = actual_schedule(&gem);

        // Matches edits_to_actions against the REQUESTED horizon (the verify path's mapping).
        let expected_requested = expected_schedule(&cfg, &env_config.roster, requested);
        assert_eq!(
            actual, expected_requested,
            "resolves against gens_requested (the verify path's horizon)"
        );
        // ...and is NOT the (wrong) early-stop mapping — proving the fidelity fix is load-bearing.
        let wrong_early = expected_schedule(&cfg, &env_config.roster, early_stop);
        assert_ne!(
            actual, wrong_early,
            "must NOT resolve against the early-stopped gem.gens (the v1 bug)"
        );
    }

    #[test]
    fn gem_edit_schedule_falls_back_to_gens_for_pre_fix_gem() {
        // A PRE-FIX gem (written before this slice) carries no gens_requested → it deserializes to 0; the resolver
        // falls back to gem.gens (the documented divergence — the best available horizon for an old gem).
        let cfg = low_loci_edited_config(0x5CED_0003);
        let (env_config, _) = env_config_for(&cfg, &species_dir());
        let env_config = env_config.expect("roster resolves");

        let gem = gem_from(&cfg, 60, 0); // gens_requested == 0 → pre-fix gem
        let actual = actual_schedule(&gem);
        let expected = expected_schedule(&cfg, &env_config.roster, gem.gens); // fallback horizon = gem.gens
        assert_eq!(
            actual, expected,
            "a pre-fix gem (gens_requested 0) resolves against gem.gens"
        );

        // An OLD gem JSON with NO gens_requested key deserializes to 0 (serde-default) + resolves the same way.
        let mut value = serde_json::to_value(&gem).expect("to value");
        value
            .as_object_mut()
            .expect("gem is a JSON object")
            .remove("gens_requested");
        let old_json = serde_json::to_string(&value).expect("old json");
        assert!(
            !old_json.contains("gens_requested"),
            "the simulated old gem JSON omits the field"
        );
        let via_json: Vec<_> = gem_edit_schedule_from_json(&old_json, &species_dir())
            .expect("parse old gem")
            .into_iter()
            .map(|e| (e.gen_abs, e.cas, e.target, e.guide, e.species))
            .collect();
        assert_eq!(
            via_json, expected,
            "an old gem JSON (no field) deserializes to 0 and falls back to gem.gens"
        );
    }

    // ---- CONTINUE-FROM-GEM (discover_from_gem) ----

    /// Build a tiny but REAL gem (scored on THIS build → a real `recorded_hash` that re-verifies) and write it
    /// to `dir/<name>`, returning its path. Self-contained fixture — no dependency on `data/runs/gems`.
    fn write_fixture_gem(
        dir: &std::path::Path,
        cfg: &SearchConfig,
        gens: u32,
        name: &str,
    ) -> std::path::PathBuf {
        let (env_config, skipped) = env_config_for(cfg, &species_dir());
        assert!(skipped.is_empty(), "fixture roster resolves: {skipped:?}");
        let env_config = env_config.expect("fixture roster resolves");
        let gem = score_config(cfg, &env_config, gens, &[]);
        let path = dir.join(name);
        let json = serde_json::to_string_pretty(&gem).expect("serialize fixture gem");
        std::fs::write(&path, format!("{json}\n")).expect("write fixture gem");
        path
    }

    /// Every saved gem JSON in `dir` as `(file_name, bytes)`, name-sorted (the throwaway `.verify-*` stage dirs
    /// are filtered out — only real `*.json` files). The deterministic on-disk artifact set the runner writes.
    fn saved_gems(dir: &std::path::Path) -> Vec<(String, Vec<u8>)> {
        let mut out: Vec<(String, Vec<u8>)> = Vec::new();
        for entry in std::fs::read_dir(dir).expect("read out_dir") {
            let entry = entry.expect("dir entry");
            let name = entry.file_name().to_string_lossy().to_string();
            if entry.file_type().expect("file type").is_file() && name.ends_with(".json") {
                out.push((name, std::fs::read(entry.path()).expect("read saved gem")));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    #[test]
    fn discover_from_gem_is_byte_reproducible() {
        // (a) DETERMINISM: the SAME (gem, search_seed, pop, gens) into two temp dirs → byte-identical saved gems
        // (inv #3). The proposal is the meta-RNG and the sim runs are pure functions of their configs.
        let tmp = TempDir::new("from-gem-determinism");
        let gens = 40u32;
        let cfg = tiny_config(0x6E4D_0001);
        let gem_path = write_fixture_gem(tmp.path(), &cfg, gens, "fixture.json");

        let out_a = tmp.path().join("out_a");
        let out_b = tmp.path().join("out_b");

        let lib_a = discover_from_gem(
            &gem_path,
            None,
            11,
            4,
            1,
            8,
            gens,
            &species_dir(),
            &out_a,
            None,
        )
        .expect("from-gem A");
        let lib_b = discover_from_gem(
            &gem_path,
            None,
            11,
            4,
            1,
            8,
            gens,
            &species_dir(),
            &out_b,
            None,
        )
        .expect("from-gem B");

        assert_eq!(
            lib_a, lib_b,
            "same (gem, seed, pop, gens) → identical returned library"
        );
        let gems_a = saved_gems(&out_a);
        let gems_b = saved_gems(&out_b);
        assert!(
            !gems_a.is_empty(),
            "the continued search writes at least one verified gem"
        );
        assert_eq!(
            gems_a, gems_b,
            "byte-identical saved gems across two runs (inv #3)"
        );
    }

    #[test]
    fn discover_from_gem_children_round_trip() {
        // (b) ROUND-TRIP: every continued/branched gem the runner writes replays to its recorded_hash (the gem
        // reproducibility contract — verify_and_write_library enforces it before writing; we re-prove it here).
        let tmp = TempDir::new("from-gem-roundtrip");
        let gens = 40u32;
        let cfg = tiny_config(0x9A71_0002);
        let gem_path = write_fixture_gem(tmp.path(), &cfg, gens, "fixture.json");
        let out = tmp.path().join("out");

        let lib = discover_from_gem(
            &gem_path,
            None,
            5,
            4,
            1,
            8,
            gens,
            &species_dir(),
            &out,
            None,
        )
        .expect("from-gem");
        assert!(
            !lib.is_empty(),
            "the continued search keeps at least one gem"
        );

        let saved = saved_gems(&out);
        assert!(!saved.is_empty(), "at least one verified gem is written");
        for (name, bytes) in &saved {
            let gem: Gem = serde_json::from_slice(bytes).expect("parse saved gem");
            let (env_config, _) = env_config_for(&gem.config, &species_dir());
            let env_config = env_config.expect("saved gem roster resolves");
            let actions = edits_to_actions(&gem.config, &env_config.roster, gens);
            let journal = build_journal(&actions, gem.gens);
            let stage = out.join(format!(".rt-{name}"));
            let _ = std::fs::remove_dir_all(&stage);
            let recorded = record_episode(&env_config, gem.config.master_seed, &journal, &stage)
                .expect("record saved gem");
            let replayed = replay(&recorded.dir).expect("replay saved gem");
            let _ = std::fs::remove_dir_all(&stage);
            assert_eq!(replayed, recorded.hash, "record == replay (inv #3)");
            assert_eq!(
                recorded.hash, gem.recorded_hash,
                "saved gem {name} replays to its recorded_hash"
            );
        }
    }

    #[test]
    fn discover_from_gem_anchors_gen0_on_the_loaded_gem() {
        // (c) ANCHORING: the gen-0 pool genuinely derives from the loaded gem (NOT a cold random start).
        let tmp = TempDir::new("from-gem-anchor");
        let gens = 40u32;
        let cfg = tiny_config(0xA0C0_0003);
        let gem_path = write_fixture_gem(tmp.path(), &cfg, gens, "fixture.json");

        let search_seed = 21u64;
        let pop_size = 4u64;
        let generations = 1u64;
        let keep = 16usize; // large enough that the keep-cut never evicts the pre-seeded anchor

        // CONTINUE-FROM-GEM run: the anchor is pre-seeded + branched off from generation 0.
        let out = tmp.path().join("out");
        let anchored = discover_from_gem(
            &gem_path,
            None,
            search_seed,
            pop_size,
            generations,
            keep,
            gens,
            &species_dir(),
            &out,
            None,
        )
        .expect("from-gem");

        // RUNNER-LEVEL proof: the anchor's exact config survives into the continued library (it was pre-seeded as
        // the gen-0 elite, re-verified on this build, and carried forward) — the search KNEW the gem.
        assert!(
            anchored.gems.iter().any(|g| g.config == cfg),
            "the continued library must contain the loaded gem's config (the gen-0 anchor)"
        );

        // CONTRAST: a COLD evolutionary run over the SAME (seed, pop, gens, keep, space) never saw the gem, so its
        // library does NOT contain the anchor's config — the difference is exactly the anchoring.
        let cold_out = tmp.path().join("cold");
        let space = SearchSpace::default();
        let cold = discover_evolved_in_space(
            &space,
            search_seed,
            pop_size,
            generations,
            keep,
            gens,
            &species_dir(),
            &cold_out,
            None,
        )
        .expect("cold evolved");
        assert!(
            !cold.gems.iter().any(|g| g.config == cfg),
            "a cold start must NOT contain the gem's config (anchoring is load-bearing)"
        );
        assert_ne!(
            anchored, cold,
            "anchoring the gen-0 pool on the gem changes the discovered library vs a cold start"
        );

        // OPERATOR-LEVEL proof: the gen-0 EXPLOIT individual the runner proposes is a mutate (a genuine branch)
        // of the gem — NOT an unrelated cold propose. The runner uses `propose_evolved(&[anchor], seed, step)`
        // for `i >= explore`; with one parent that delegates to `mutate`, which preserves the parent's roster.
        let explore = ((pop_size * EVOLVE_EXPLORE_BP / BP_SCALE).max(1)).min(pop_size);
        let step = explore; // generation 0, first exploit individual (i == explore)
        let branched = propose_evolved(std::slice::from_ref(&cfg), search_seed, step, &space);
        let mutated = discovery::search::mutate(&cfg, search_seed, step, &space);
        assert_eq!(
            branched, mutated,
            "the gen-0 exploit child is a mutate of the gem (a genuine branch off the anchor)"
        );
        assert_ne!(
            branched,
            propose(search_seed, step, &space),
            "the gen-0 exploit child is NOT an unrelated cold propose"
        );
    }
}
