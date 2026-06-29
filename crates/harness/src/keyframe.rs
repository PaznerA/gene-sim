//! The off-hash KEY-EVENT detector for the scenario GIF preview (inv #3).
//!
//! Given a saved [`Gem`](discovery::search::Gem) (or its [`SearchConfig`]), [`gem_keyframes`] /
//! [`config_keyframes`] CAPTURE the SAME off-hash D1 [`PerGenTrace`](discovery::trace::PerGenTrace) the D0 scorer
//! consumes ([`capture::capture_trace`]) and pick the KEY generations to snapshot so the preview GIF reads as a
//! short, coherent STORY of the run:
//!   - the BOOM / CRASH / TAKEOVER gens — REUSED from [`discovery::ecology::detect_events`], the SAME event logic
//!     the scorer's M5 rewards, so the GIF keys off exactly the events that made the run interesting;
//!   - the scheduled EDIT gens, from `config.edits` via the SAME [`discover::edits_to_actions`] q16→absolute-gen
//!     mapping the capture/verify path uses (so the snapshot lands on the gen the edit actually fires);
//!   - the IMMIGRATE-established gens (journaled inoculations that took, the M5 immigration event);
//!   - plus structural anchors so the clip is always coherent: gen-1 (START), a few evenly-spaced CONTEXT frames,
//!     and the FINAL captured gen.
//!
//! The result is an ORDERED, GEN-DEDUPED, CAPPED (`<= `[`MAX_FRAMES`]) list of [`FrameKey`]s.
//!
//! ## Off-hash (inv #3)
//! The schedule is PURE analysis of the trace. The only sim work is [`capture_trace`], which is PROVEN
//! hash-neutral (`observe_all`/`flow_matrix` draw ZERO `SimRng` and are never folded into `hash_world`) — so
//! computing a preview, exactly like capturing a trace, CANNOT move the pinned literal `0x47a0_3c8f_6701_f240`.
//! No `HashMap` is iterated on any ordered path (the gen→label set is a `BTreeMap`, the output is gen-sorted).

use std::collections::BTreeMap;
use std::path::Path;

use discovery::ecology::{detect_events, EventKind};
use discovery::search::{Gem, SearchConfig};
use discovery::trace::PerGenTrace;
use discovery::ScoreParams;

use crate::capture::capture_trace;
use crate::discover::{build_env, edits_to_actions, env_config_for};

/// The hard cap on snapshot frames in a preview GIF — a short story, not the full timeline.
pub const MAX_FRAMES: usize = 12;

/// The number of evenly-spaced INTERIOR context frames interleaved between START and FINAL. Lowest priority, so
/// they fill gaps when there is room and are the FIRST frames dropped when the budget is tight.
const CONTEXT_FRAMES: u32 = 3;

/// Why a generation was selected as a key frame — the label drawn alongside the snapshot. The variant also drives
/// dedup + over-budget trimming via [`FrameLabel::priority`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameLabel {
    /// Generation 1 — the opening frame of the run (a structural anchor, always kept).
    Start,
    /// A scheduled mid-run CRISPR edit fires at this gen (always kept — the schedule is always represented).
    Edit,
    /// The rank-1 (most-populous) species changed here (a dominance flip — `EventKind::Takeover`).
    Takeover,
    /// A species' population crashed here (`EventKind::Crash`).
    Crash,
    /// A species' population boomed here (`EventKind::Boom`).
    Boom,
    /// A journaled inoculation that ESTABLISHED (alive at the final gen) was introduced at this gen.
    Immigrate,
    /// An evenly-spaced interior context frame (the lowest-priority filler).
    Context,
    /// The final captured generation — the closing frame (a structural anchor, always kept).
    Final,
}

impl FrameLabel {
    /// Salience used for (a) per-gen dedup — a gen keeps its HIGHEST-salience label — and (b) over-budget
    /// trimming — the LOWEST-salience non-[`protected`](Self::protected) frames are dropped first. The anchors
    /// (START/FINAL) sit highest so the first/last frame labels are never overridden; EDIT sits just below.
    #[must_use]
    fn priority(self) -> u8 {
        match self {
            FrameLabel::Start | FrameLabel::Final => 250, // structural anchors — never overridden, never dropped
            FrameLabel::Edit => 200, // the scheduled edits — always represented
            FrameLabel::Takeover => 60,
            FrameLabel::Crash => 50,
            FrameLabel::Boom => 40,
            FrameLabel::Immigrate => 30,
            FrameLabel::Context => 1, // filler — dropped first when over budget
        }
    }

    /// A short, stable, lowercase tag for the label — the WHY printed alongside the gen by the `--keyframes` CLI
    /// (the capture script reads the gen; a human reads the tag) and a renderer caption seam. Never a `Debug`
    /// string (which a refactor could silently change).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            FrameLabel::Start => "start",
            FrameLabel::Edit => "edit",
            FrameLabel::Takeover => "takeover",
            FrameLabel::Crash => "crash",
            FrameLabel::Boom => "boom",
            FrameLabel::Immigrate => "immigrate",
            FrameLabel::Context => "context",
            FrameLabel::Final => "final",
        }
    }

    /// Anchors (START/FINAL) and the scheduled EDITs are PROTECTED from over-budget trimming, so the clip always
    /// opens on gen-1, closes on the final gen, and shows every edit that fired within the captured horizon.
    #[must_use]
    fn protected(self) -> bool {
        matches!(
            self,
            FrameLabel::Start | FrameLabel::Final | FrameLabel::Edit
        )
    }
}

/// One selected preview frame: the generation to snapshot and WHY it was chosen. `Eq` so a schedule is a
/// byte-for-byte determinism assertion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameKey {
    /// The generation to snapshot (always within `[1, g]` of the captured trace).
    pub gen: u32,
    /// Why this generation is a key frame.
    pub label: FrameLabel,
}

/// Insert `(gen, label)` into the gen→label set, UPGRADING an existing entry only when `label` outranks it (so a
/// gen that is both an event AND a context frame keeps the more salient label). Deterministic, order-independent.
fn upsert(map: &mut BTreeMap<u32, FrameLabel>, gen: u32, label: FrameLabel) {
    map.entry(gen)
        .and_modify(|cur| {
            if label.priority() > cur.priority() {
                *cur = label;
            }
        })
        .or_insert(label);
}

/// Build the ORDERED, gen-deduped, capped frame schedule from an already-captured [`PerGenTrace`], the resolved
/// absolute EDIT gens, and the scoring params. PURE (no sim, no RNG) — the off-hash core both [`gem_keyframes`]
/// and [`config_keyframes`] share. `edit_gens` are ABSOLUTE generations (the [`edits_to_actions`] mapping); only
/// gens that fired within the captured horizon `[1, g]` (`g == trace.g`) are represented. An empty trace (a run
/// that died at gen 0) yields an empty schedule.
#[must_use]
pub fn keyframes(trace: &PerGenTrace, edit_gens: &[u32], params: &ScoreParams) -> Vec<FrameKey> {
    let g = trace.g;
    if g == 0 {
        return Vec::new(); // a run that died at gen 0 captured no frames to key off
    }
    let in_range = |gen: u32| gen >= 1 && gen <= g;

    // gen → highest-salience label. A BTreeMap keeps the gens ORDERED and dedups — never a HashMap (inv #3).
    let mut keyed: BTreeMap<u32, FrameLabel> = BTreeMap::new();

    // Structural anchors: the opening + closing frames are always present (g == 1 collapses them onto one gen).
    upsert(&mut keyed, 1, FrameLabel::Start);
    upsert(&mut keyed, g, FrameLabel::Final);

    // Evenly-spaced interior context frames at k/(CONTEXT_FRAMES+1) of the run (lowest priority — pure filler).
    for k in 1..=CONTEXT_FRAMES {
        let gen = ((u64::from(g) * u64::from(k)) / u64::from(CONTEXT_FRAMES + 1)) as u32;
        if in_range(gen) {
            upsert(&mut keyed, gen, FrameLabel::Context);
        }
    }

    // Scheduled mid-run edits (only those that fired within [1, g] — a gen-0 or post-early-stop edit never fires).
    for &gen in edit_gens {
        if in_range(gen) {
            upsert(&mut keyed, gen, FrameLabel::Edit);
        }
    }

    // BOOM / CRASH / TAKEOVER — the SAME events the D0 scorer's M5 rewards (the single shared detector).
    for ev in detect_events(params, trace) {
        if !in_range(ev.gen) {
            continue;
        }
        let label = match ev.kind {
            EventKind::Boom => FrameLabel::Boom,
            EventKind::Crash => FrameLabel::Crash,
            EventKind::Takeover => FrameLabel::Takeover,
        };
        upsert(&mut keyed, ev.gen, label);
    }

    // IMMIGRATE-established: a journaled inoculation that is still alive at the final captured gen (the M5
    // immigration event) — snapshot the gen it arrived at.
    if let Some(last) = trace.rows.last() {
        let s = trace.s as usize;
        for inoc in &trace.inoculations {
            let i = inoc.species_id as usize;
            let established = i < s && u64::from(last.pop.get(i).copied().unwrap_or(0)) > 0;
            if established && in_range(inoc.gen) {
                upsert(&mut keyed, inoc.gen, FrameLabel::Immigrate);
            }
        }
    }

    // Flatten in gen order (BTreeMap iterates ascending), then trim to the cap.
    let mut frames: Vec<FrameKey> = keyed
        .into_iter()
        .map(|(gen, label)| FrameKey { gen, label })
        .collect();
    trim_to_cap(&mut frames);
    frames
}

/// Trim a gen-sorted frame list down to [`MAX_FRAMES`] by dropping the LOWEST-salience non-protected frames
/// first (deterministic: ties broken by ascending gen). Anchors + edits are protected, so the opening/closing
/// frames and the whole edit schedule always survive — even when that means a pathological edit budget keeps the
/// list slightly over the cap (documented; realistic budgets are a handful).
fn trim_to_cap(frames: &mut Vec<FrameKey>) {
    if frames.len() <= MAX_FRAMES {
        return;
    }
    // Candidate indices to drop, ordered by (priority asc, gen asc) — the least salient go first.
    let mut removable: Vec<usize> = (0..frames.len())
        .filter(|&i| !frames[i].label.protected())
        .collect();
    removable.sort_by(|&a, &b| {
        frames[a]
            .label
            .priority()
            .cmp(&frames[b].label.priority())
            .then(frames[a].gen.cmp(&frames[b].gen))
    });
    let excess = frames.len() - MAX_FRAMES;
    let mut drop: Vec<usize> = removable.into_iter().take(excess).collect();
    drop.sort_unstable(); // remove from the back so earlier indices stay valid
    for &i in drop.iter().rev() {
        frames.remove(i);
    }
}

/// Compute the preview frame schedule for a [`SearchConfig`] over a capture horizon `gens` — resolve the roster
/// through the `data/species/<key>.json` boundary, CAPTURE the off-hash trace (with the config's scheduled
/// edits), derive the absolute edit gens via the SAME [`edits_to_actions`] mapping the score/verify path uses,
/// and pick the key frames ([`keyframes`]). An empty / unresolvable roster yields an empty schedule (guarded,
/// like the verify path). PURE OFF-HASH (inv #3): the only sim work is the hash-neutral [`capture_trace`].
#[must_use]
pub fn config_keyframes(cfg: &SearchConfig, gens: u32, species_dir: &Path) -> Vec<FrameKey> {
    let (env_config, _skipped) = env_config_for(cfg, species_dir);
    let Some(env_config) = env_config else {
        return Vec::new(); // the roster no longer resolves — nothing to preview
    };
    // The SAME edit→action mapping the capture/verify path uses (so the snapshot gen == the gen the edit fires).
    let actions = edits_to_actions(cfg, &env_config.roster, gens);
    let mut env = build_env(&env_config);
    let trace = capture_trace(&mut env, cfg.master_seed, gens, &actions);
    let edit_gens: Vec<u32> = actions.iter().map(|(gen, _)| *gen).collect();
    keyframes(&trace, &edit_gens, &ScoreParams::default())
}

/// [`config_keyframes`] for a loaded [`Gem`] — the renderer/CLI boundary entry. Maps the edit schedule against
/// the gem's REQUESTED horizon ([`Gem::gens_requested`], falling back to [`Gem::gens`] for a pre-v2 gem where it
/// is `0` — the documented divergence) so the edits resolve to the IDENTICAL absolute generations the
/// capture/verify path used (matches [`crate::discover::gem_edit_schedule`]).
#[must_use]
pub fn gem_keyframes(gem: &Gem, species_dir: &Path) -> Vec<FrameKey> {
    let horizon = if gem.gens_requested == 0 {
        gem.gens
    } else {
        gem.gens_requested
    };
    config_keyframes(&gem.config, horizon, species_dir)
}

/// [`gem_keyframes`] from a gem's JSON TEXT — the renderer/CLI boundary (mirrors
/// [`crate::discover::gem_edit_schedule_from_json`]: the binding hands the gem file bytes, the core parses +
/// analyses so no resolution math leaks into GDScript, inv #2).
///
/// # Errors
/// Returns a [`serde_json::Error`] if `gem_json` is not a valid serialized [`Gem`].
pub fn gem_keyframes_from_json(
    gem_json: &str,
    species_dir: &Path,
) -> serde_json::Result<Vec<FrameKey>> {
    let gem: Gem = serde_json::from_str(gem_json)?;
    Ok(gem_keyframes(&gem, species_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use discovery::search::EditGene;
    use discovery::trace::{GenRow, SpeciesMeta};

    /// The repo-root `data/species` dir (the byte-mover boundary; mirrors the discover/replay test helpers).
    fn species_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/species"))
    }

    /// A real predator/prey/producer config (plant + E. coli + Bdellovibrio — the trace-capture roster) with two
    /// scheduled mid-run edits. The q16 fractions map to absolute gens 60 and 90 at `gens = 120`
    /// (`32768*120/65536 = 60`, `49152*120/65536 = 90`).
    fn edited_config() -> (SearchConfig, u32) {
        let gens = 120u32;
        let guide = "ACGTACGTACGTACGTACGT".to_string(); // 20 ACGT bases (EDIT_GUIDE_LEN)
        let cfg = SearchConfig {
            master_seed: 0x00C0_FFEE,
            roster: vec![
                ("default".to_string(), 600),
                ("ecoli".to_string(), 400),
                ("bdellovibrio".to_string(), 120),
            ],
            containment_level: 0,
            temp_q: 500,
            season: 0,
            edits: vec![
                EditGene {
                    gen: 32_768,
                    species_index: 0,
                    target: 0,
                    guide: guide.clone(),
                },
                EditGene {
                    gen: 49_152,
                    species_index: 1,
                    target: 0,
                    guide,
                },
            ],
        };
        (cfg, gens)
    }

    #[test]
    fn detector_is_deterministic_in_range_and_includes_edit_gens() {
        let (cfg, gens) = edited_config();
        let dir = species_dir();

        // DETERMINISTIC per config: two independent computations agree byte-for-byte (inv #3).
        let a = config_keyframes(&cfg, gens, &dir);
        let b = config_keyframes(&cfg, gens, &dir);
        assert_eq!(
            a, b,
            "the detector must be deterministic per config (inv #3)"
        );

        assert!(
            !a.is_empty(),
            "a living multi-species run yields key frames"
        );
        assert!(
            a.len() <= MAX_FRAMES,
            "capped at MAX_FRAMES, got {}",
            a.len()
        );

        // Recover the captured horizon g + the resolved edit gens via the SAME mapping the detector uses.
        let (env_config, skipped) = env_config_for(&cfg, &dir);
        assert!(skipped.is_empty(), "all roster keys resolve: {skipped:?}");
        let env_config = env_config.expect("roster resolves");
        let actions = edits_to_actions(&cfg, &env_config.roster, gens);
        let mut env = build_env(&env_config);
        let trace = capture_trace(&mut env, cfg.master_seed, gens, &actions);
        let g = trace.g;
        assert!(
            g >= 90,
            "the predator/prey roster survives past the last edit (g={g})"
        );

        // Every key gen is within [1, g] ⊆ [1, gens], and ordered ascending.
        for w in a.windows(2) {
            assert!(w[0].gen < w[1].gen, "frames must be gen-sorted + deduped");
        }
        for f in &a {
            assert!(
                f.gen >= 1 && f.gen <= g && f.gen <= gens,
                "frame gen {} out of [1, g={g}] (gens={gens})",
                f.gen
            );
        }

        // gen-1 (START) and the final captured gen are always present.
        assert!(a.iter().any(|f| f.gen == 1), "gen-1 is always a frame");
        assert!(
            a.iter().any(|f| f.gen == g && f.label == FrameLabel::Final),
            "the final captured gen is always a frame"
        );

        // The scheduled edit gens (60, 90) fired within [1, g] and are REPRESENTED as key frames.
        let edit_gens: Vec<u32> = actions.iter().map(|(gen, _)| *gen).collect();
        assert_eq!(
            edit_gens,
            vec![60, 90],
            "q16 fractions map to absolute gens 60 and 90 at gens=120"
        );
        for eg in &edit_gens {
            assert!(
                a.iter().any(|f| f.gen == *eg),
                "edit gen {eg} must be a key frame (got {a:?})"
            );
        }
    }

    /// Build a tiny synthetic trace (no filesystem / no sim) to exercise the pure [`keyframes`] assembly: a boom,
    /// a takeover-over-a-boom, and a crash on a 2-species, 12-gen series, plus the START/CONTEXT/FINAL anchors.
    fn synthetic_trace() -> PerGenTrace {
        // s0 booms at gen5 (10→50), crashes at gen10 (50→5); s1 booms+takes-over at gen7 (5→100).
        let s0 = [10u32, 10, 10, 10, 50, 50, 50, 50, 50, 5, 5, 5];
        let s1 = [5u32, 5, 5, 5, 5, 5, 100, 100, 100, 100, 100, 100];
        let rows: Vec<GenRow> = (0..12)
            .map(|k| GenRow {
                gen: (k + 1) as u32,
                pop: vec![s0[k], s1[k]],
                allele_q: vec![0, 0],
                flow: vec![],
            })
            .collect();
        PerGenTrace {
            s: 2,
            g: 12,
            gens_requested: 12,
            species: vec![
                SpeciesMeta {
                    id: 0,
                    key: "s0".to_string(),
                    role: 0,
                },
                SpeciesMeta {
                    id: 1,
                    key: "s1".to_string(),
                    role: 1,
                },
            ],
            rows,
            inoculations: vec![],
            seed: 1,
            recorded_hash: 0,
        }
    }

    #[test]
    fn keyframes_label_the_scorer_events_with_anchors() {
        let t = synthetic_trace();
        let frames = keyframes(&t, &[], &ScoreParams::default());

        // The shared detector's booms/crashes/takeovers land on the right gens with the right labels, and the
        // structural anchors bracket the clip. (Takeover at gen7 outranks the coincident boom.)
        let label_at = |gen: u32| frames.iter().find(|f| f.gen == gen).map(|f| f.label);
        assert_eq!(label_at(1), Some(FrameLabel::Start), "gen-1 is START");
        assert_eq!(label_at(5), Some(FrameLabel::Boom), "s0 booms at gen5");
        assert_eq!(
            label_at(7),
            Some(FrameLabel::Takeover),
            "takeover outranks the coincident boom at gen7"
        );
        assert_eq!(label_at(10), Some(FrameLabel::Crash), "s0 crashes at gen10");
        assert_eq!(
            label_at(12),
            Some(FrameLabel::Final),
            "the final gen is FINAL"
        );

        // The stable lowercase tags the --keyframes CLI prints alongside each gen.
        assert_eq!(FrameLabel::Start.name(), "start");
        assert_eq!(FrameLabel::Takeover.name(), "takeover");
        assert_eq!(FrameLabel::Final.name(), "final");

        // The detector's events agree with the scorer's M5 detector (same shared pass).
        let evs = discovery::ecology::detect_events(&ScoreParams::default(), &t);
        for ev in &evs {
            assert!(
                frames.iter().any(|f| f.gen == ev.gen),
                "every detected event gen {} is a key frame",
                ev.gen
            );
        }
        assert!(frames.len() <= MAX_FRAMES);
    }

    #[test]
    fn cap_keeps_anchors_and_edits_drops_filler() {
        // A long synthetic run with MANY context candidates but only the anchors + a couple of edits to keep.
        let g = 240u32;
        let rows: Vec<GenRow> = (0..g)
            .map(|k| GenRow {
                gen: k + 1,
                pop: vec![100, 100], // flat → no booms/crashes/takeovers
                allele_q: vec![0, 0],
                flow: vec![],
            })
            .collect();
        let t = PerGenTrace {
            s: 2,
            g,
            gens_requested: g,
            species: vec![
                SpeciesMeta {
                    id: 0,
                    key: "a".to_string(),
                    role: 0,
                },
                SpeciesMeta {
                    id: 1,
                    key: "b".to_string(),
                    role: 1,
                },
            ],
            rows,
            inoculations: vec![],
            seed: 7,
            recorded_hash: 0,
        };
        // 20 edit gens — more than the cap on their own would already be tight; protected edits survive trimming.
        let edit_gens: Vec<u32> = (1..=10).map(|i| i * 11).collect();
        let frames = keyframes(&t, &edit_gens, &ScoreParams::default());

        assert!(
            frames.len() <= MAX_FRAMES,
            "respects the cap, got {}",
            frames.len()
        );
        assert!(frames.iter().any(|f| f.gen == 1), "START survives the cap");
        assert!(
            frames
                .iter()
                .any(|f| f.gen == g && f.label == FrameLabel::Final),
            "FINAL survives the cap"
        );
        for eg in &edit_gens {
            assert!(
                frames.iter().any(|f| f.gen == *eg),
                "edit gen {eg} survives the cap (protected)"
            );
        }
    }
}
