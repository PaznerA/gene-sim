//! STARTER-MAP PROMOTE — turn a verified emergent-run [`Gem`] into a committed, gallery-ready STARTER for the
//! renderer's starter library (the discovery → curated-content bridge). Meta-level only: the sim runs stay pure
//! functions of their configs (inv #3), so promoting a gem never moves the pinned literal
//! `0x47a0_3c8f_6701_f240` — both shapes only ever drive the SAME deterministic [`record_episode`]/[`replay`]
//! contract (a GEN-1 starter records the EDIT-FREE pristine config; a GEN-N checkpoint records the gem's
//! edit-interleaved journal).
//!
//! ## Two shapes (mirroring the gem's two reproducibility tiers)
//! - **GEN-1 (pristine fresh config)** — [`promote_gen1`] writes `<starters_dir>/<slug>.json`: a
//!   primordial.json-shaped [`StarterConfig`] (roster + climate + containment, **NO edits** — gen-1 is the
//!   pristine pre-edit starting point) plus gallery metadata + provenance (`source_seed`, `gens`). The stored
//!   `source_hash` is RECOMPUTED from an edit-free replay of the pristine config over `gens` (NOT a blind copy
//!   of the gem's `recorded_hash`): the gen-1 starter drops the gem's edits, so its hash MUST be what the
//!   edit-free config actually produces. For an edit-free gem this equals `gem.recorded_hash` (unchanged); for
//!   a gem that carried (today hash-neutral) edits it is the pristine hash, so the doc stays self-consistent
//!   even once edits become hash-active (ADR-031 known trap → fixed). The doc is self-contained re-verifiable:
//!   re-run the stored config for `gens` → `source_hash`. The player picks the actual run length.
//! - **GEN-N CHECKPOINT** — [`promote_checkpoint`] records the gem's edit-interleaved journal up to generation
//!   `N` into `<starters_dir>/<slug>/` as the SAME session format [`crate::replay::save_journal`] /
//!   `load_session` read (seed.json + actions.ndjson), plus a sibling `starter.json` metadata. The recorded
//!   session is round-trip-verified (`record_episode → replay`, the gem reproducibility contract) before the
//!   metadata is written — a non-reproducible checkpoint is an error, never a silently-broken starter.
//!
//! ## The index ([`rebuild_index`])
//! `<starters_dir>/index.json` is a flat, slug-sorted list of [`StarterIndexEntry`] so the renderer gallery can
//! enumerate the library WITHOUT scanning + parsing every starter. It is rebuilt (a pure function of the dir
//! contents) after every promote, so its order is deterministic regardless of promote order.

use std::io;
use std::path::{Path, PathBuf};

use discovery::search::{Gem, SearchConfig};
use serde::{Deserialize, Serialize};

use crate::discover::{build_journal, edits_to_actions, env_config_for};
use crate::replay::{record_episode, replay, save_journal};

/// The pristine fresh-config view of a gem — a primordial.json-shaped subset: the roster `(key, count)` pairs
/// plus the climate/containment knobs, with **NO mid-run edits** (gen-1 is pristine). This is exactly the
/// fields [`env_config_for`] reads to rebuild a run's env, minus the seed (carried separately as `source_seed`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarterConfig {
    /// Roster: `(species key/stem, starting count)` in proposal order (the SAME shape a gem's config carries).
    pub roster: Vec<(String, u32)>,
    /// Containment ladder ordinal (`0` Sealed → `3` Open) — drives deterministic airborne immigration.
    pub containment_level: u8,
    /// Temperature as q16 permille (`0..=1000` ↔ `0.0..=1.0`).
    pub temp_q: u16,
    /// Season ordinal (`0..=3`: Spring/Summer/Autumn/Winter).
    pub season: u8,
}

impl StarterConfig {
    /// The pristine fresh-config view of a gem (drops the gem's mid-run edit schedule — gen-1 is pristine).
    #[must_use]
    pub fn from_gem(gem: &Gem) -> Self {
        Self {
            roster: gem.config.roster.clone(),
            containment_level: gem.config.containment_level,
            temp_q: gem.config.temp_q,
            season: gem.config.season,
        }
    }

    /// Rebuild a [`SearchConfig`] from this fresh config for `master_seed`, with an EMPTY edit schedule. For an
    /// edit-free source gem this is byte-identical to the gem's own config, so [`env_config_for`] rebuilds the
    /// IDENTICAL env (the gen-1 reproducibility contract — the fresh config replays to the source hash).
    #[must_use]
    pub fn to_search_config(&self, master_seed: u64) -> SearchConfig {
        SearchConfig {
            master_seed,
            roster: self.roster.clone(),
            containment_level: self.containment_level,
            temp_q: self.temp_q,
            season: self.season,
            edits: Vec::new(),
        }
    }
}

/// A GEN-1 starter doc (`<starters_dir>/<slug>.json`): the pristine [`StarterConfig`] + gallery metadata +
/// provenance. `source_hash` is the hex of the EDIT-FREE replay of the pristine config over `gens` (inv #7
/// traceability + a self-contained reproducibility anchor): a reader can re-run the config for `gens` and
/// assert the hash, with NO access to the source gem.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gen1Starter {
    /// Human-readable starter name (the gallery title).
    pub name: String,
    /// The source gem's auto one-liner caption (e.g. `"drift · 1 spp · steady"`).
    pub caption: String,
    /// The dynamics shape (the caption's leading word, e.g. `"drift"`) — the gallery's category facet.
    pub dynamics: String,
    /// Hex of the EDIT-FREE replay hash of the pristine config over `gens` (16 lowercase hex digits) — the
    /// reproducibility anchor (== `gem.recorded_hash` for an edit-free source gem; the pristine hash otherwise).
    pub source_hash: String,
    /// The source gem's master seed (so the fresh config can be re-run to `source_hash`, edit-free).
    pub source_seed: u64,
    /// The horizon `source_hash` was recomputed over (the source gem's `gens_requested`, falling back to its
    /// early-stopped `gens`). Stored so the doc is SELF-CONTAINED re-verifiable. `#[serde(default)]` → `0` for a
    /// pre-fix committed starter that predates this field (those are edit-free; their stored hash is correct).
    #[serde(default)]
    pub gens: u32,
    /// Did the SOURCE gem carry a non-empty edit schedule (provenance — the gen-1 starter still drops the edits;
    /// `source_hash` is always the edit-free replay). `#[serde(default)]` → `false` for a pre-fix committed
    /// starter (all of which were promoted from edit-free / no-op-edit gems).
    #[serde(default)]
    pub source_had_edits: bool,
    /// The pristine fresh config (roster + climate + containment, NO edits).
    pub config: StarterConfig,
}

/// GEN-N checkpoint metadata (`<starters_dir>/<slug>/starter.json`), sibling to the recorded session
/// (seed.json + actions.ndjson). The session IS the reproducibility artifact; this is the gallery metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointStarter {
    /// Human-readable starter name (the gallery title).
    pub name: String,
    /// The source gem's auto one-liner caption.
    pub caption: String,
    /// The dynamics shape (the caption's leading word) — the gallery category facet.
    pub dynamics: String,
    /// The generation the session was recorded up to (the scrub-back timeline horizon).
    pub checkpoint_gen: u32,
    /// Hex of the source gem's `recorded_hash` — provenance (inv #7).
    pub source_hash: String,
    /// The source gem's master seed.
    pub source_seed: u64,
}

/// One row of the starters index (`<starters_dir>/index.json`) — enough for the renderer gallery to list +
/// categorise a starter without parsing the full doc/session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarterIndexEntry {
    /// The starter slug (the `<slug>.json` stem for gen-1, the `<slug>/` dir name for a checkpoint).
    pub slug: String,
    /// Human-readable name.
    pub name: String,
    /// `"gen1"` (a pristine fresh-config starter) or `"checkpoint"` (a recorded GEN-N session).
    pub kind: String,
    /// The source gem's caption.
    pub caption: String,
    /// The dynamics shape facet.
    pub dynamics: String,
    /// Hex of the source gem's `recorded_hash` (provenance).
    pub source_hash: String,
}

/// The dynamics shape of a gem — its caption's leading word (e.g. `"drift · 1 spp · steady"` → `"drift"`).
/// An empty/odd caption degrades to `"unknown"` (never a panic). This is the gallery's category facet.
#[must_use]
pub fn dynamics_from_caption(caption: &str) -> String {
    caption
        .split('·')
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("unknown")
        .to_string()
}

/// 16 lowercase hex digits of a `u64` — the canonical `source_hash` text form (matches the gem-file scheme).
#[must_use]
fn hex16(h: u64) -> String {
    format!("{h:016x}")
}

/// Title-case a slug for a default starter name: `"limit-cycle"` → `"Limit Cycle"` (split on `-`/`_`, cap each
/// word). Used when a caller passes no explicit `--starter-title`.
#[must_use]
pub fn title_from_slug(slug: &str) -> String {
    slug.split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(cap_first)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Capitalise the first character of `w` (ASCII-aware; leaves the rest untouched).
fn cap_first(w: &str) -> String {
    let mut chars = w.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Sanitise a string into a slug: lowercase alphanumerics, every other run collapsed to a single `-`, trimmed.
/// `"limit-cycle"` → `"limit-cycle"`, `"boom-bust"` → `"boom-bust"`. Used to derive a default-set slug from a
/// gem's dynamics word.
#[must_use]
fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !out.is_empty() && !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// Promote a gem as a GEN-1 (pristine fresh-config) starter into `<starters_dir>/<slug>.json`. Writes the
/// [`Gen1Starter`] doc (pretty JSON, git-friendly); the dir is created if absent. Returns the written path.
///
/// The stored `source_hash` is RECOMPUTED from an EDIT-FREE replay of the pristine [`StarterConfig`] (NOT a
/// blind copy of `gem.recorded_hash`): the gen-1 starter drops the gem's edits, so its hash MUST equal what the
/// edit-free config actually produces. The config is rebuilt via [`env_config_for`] (the SAME `data/species`
/// boundary the verify path uses) and run over `gens` (the gem's `gens_requested`, falling back to its
/// early-stopped `gens` for a pre-fix gem) with an edit-free journal ([`build_journal`]`(&[], gens)` →
/// `Advance(1)*gens`); the recorded hash is replay-verified (`record_episode == replay`, the gem
/// reproducibility contract, inv #3) BEFORE it is stored. For an edit-free gem this hash equals
/// `gem.recorded_hash` (unchanged behaviour); for a gem that carried (today hash-neutral) edits it is the
/// pristine hash — so the doc stays self-consistent even once edits become hash-active (ADR-031 known trap →
/// fixed). The gem still NEVER moves the pinned literal: this is a pure function of the (edit-free) config.
///
/// # Errors
/// An [`io::Error`] if the gem's roster no longer resolves under `species_dir`, if recording/replaying fails,
/// if the edit-free run does not replay stably (non-reproducible), or from any file write/serialization.
pub fn promote_gen1(
    gem: &Gem,
    slug: &str,
    name: &str,
    species_dir: &Path,
    starters_dir: &Path,
) -> io::Result<PathBuf> {
    std::fs::create_dir_all(starters_dir)?;

    let config = StarterConfig::from_gem(gem);
    let source_seed = gem.config.master_seed;
    let source_had_edits = !gem.config.edits.is_empty();
    // The horizon the source gem ran/scored over (its requested `gens_requested` for a v2 gem, the early-stopped
    // `gem.gens` for a pre-fix gem whose `gens_requested` defaulted to 0) — mirrors `promote_checkpoint` +
    // `gem_edit_schedule`. Stored so the gen-1 doc is self-contained re-verifiable (re-run the config for `gens`).
    let gens = if gem.gens_requested == 0 {
        gem.gens
    } else {
        gem.gens_requested
    };

    // Rebuild the EDIT-FREE env for the pristine config (drops the gem's edit schedule). `env_config_for` resolves
    // the roster through `data/species/<key>.json` exactly as the verify path does; a roster that no longer
    // resolves is a hard error (never a silently-broken starter), like `promote_checkpoint`.
    let search_cfg = config.to_search_config(source_seed);
    let (env_config, skipped) = env_config_for(&search_cfg, species_dir);
    for (key, err) in &skipped {
        eprintln!("promote: gen-1 {slug:?}: skipped species {key:?} ({err})");
    }
    let env_config = env_config.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "gem (seed {source_seed:016x}) roster does not resolve under {}",
                species_dir.display()
            ),
        )
    })?;

    // RECOMPUTE the source hash: record the edit-free journal (Advance(1)*gens) via a THROWAWAY stage and
    // replay-verify the recorded run reproduces it (record == replay — the gem reproducibility contract, inv #3).
    // This is meta-level (the SAME machinery as the verify path); the pinned literal is unmoved.
    let journal = build_journal(&[], gens);
    let stage = starters_dir.join(format!(".verify-gen1-{slug}"));
    let _ = std::fs::remove_dir_all(&stage);
    let recorded = record_episode(&env_config, source_seed, &journal, &stage)?;
    let canonical = recorded.hash;
    let replayed = replay(&recorded.dir)?;
    let _ = std::fs::remove_dir_all(&stage);
    if replayed != canonical {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "gen-1 starter {slug:?} does not replay stably (recorded {canonical:016x}, replay {replayed:016x})"
            ),
        ));
    }

    let doc = Gen1Starter {
        name: name.to_string(),
        caption: gem.caption.clone(),
        dynamics: dynamics_from_caption(&gem.caption),
        source_hash: hex16(canonical),
        source_seed,
        gens,
        source_had_edits,
        config,
    };
    let path = starters_dir.join(format!("{slug}.json"));
    let json = serde_json::to_string_pretty(&doc).map_err(to_io)?;
    std::fs::write(&path, format!("{json}\n"))?;
    Ok(path)
}

/// Promote a gem as a GEN-N CHECKPOINT into `<starters_dir>/<slug>/`: record the gem's edit-interleaved journal
/// up to generation `checkpoint_gen` as the SAME session format `load_session` reads (seed.json +
/// actions.ndjson), plus a sibling `starter.json` metadata. Returns the session dir.
///
/// The journal MIRRORS the capture/verify interleave: the gem's scheduled edits ([`edits_to_actions`] mapped
/// against the gem's REQUESTED horizon — `gens_requested`, falling back to `gens` for a pre-fix gem) are placed
/// at their absolute generations, [`build_journal`] interleaves them with `Advance(1)` up to `checkpoint_gen`
/// (an edit scheduled past `N` simply never fires). The recorded session is round-trip-verified (`record_episode
/// → replay`, the gem reproducibility contract, inv #3) BEFORE the metadata is written; a non-reproducible
/// checkpoint is an error, never a silently-broken starter.
///
/// # Errors
/// An [`io::Error`] if the gem's roster no longer resolves under `species_dir`, if recording/replaying fails,
/// if the recorded session does not replay to the recorded hash (non-reproducible), or from any file write.
pub fn promote_checkpoint(
    gem: &Gem,
    slug: &str,
    name: &str,
    checkpoint_gen: u32,
    species_dir: &Path,
    starters_dir: &Path,
) -> io::Result<PathBuf> {
    let (env_config, skipped) = env_config_for(&gem.config, species_dir);
    for (key, err) in &skipped {
        eprintln!("promote: checkpoint {slug:?}: skipped species {key:?} ({err})");
    }
    let env_config = env_config.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "gem (seed {:016x}) roster does not resolve under {}",
                gem.config.master_seed,
                species_dir.display()
            ),
        )
    })?;

    // The horizon the capture mapped the q16 edit fractions against: `gens_requested` for a v2 gem, `gem.gens`
    // (the early-stopped count) for a pre-fix gem whose `gens_requested` defaulted to 0 (mirrors gem_edit_schedule).
    let horizon = if gem.gens_requested == 0 {
        gem.gens
    } else {
        gem.gens_requested
    };
    let actions = edits_to_actions(&gem.config, &env_config.roster, horizon);
    // Interleave the scheduled edits with Advance(1) up to `checkpoint_gen` — the scrub-back timeline the session
    // records (edits at their absolute generations; an edit past N never fires).
    let journal = build_journal(&actions, checkpoint_gen);

    let session_dir = starters_dir.join(slug);
    std::fs::create_dir_all(&session_dir)?;

    // Canonical hash via a THROWAWAY record_episode stage (it adds a run_id subdir), then write the SAME session
    // FLAT into <slug>/ via save_journal (no run_id subdir — exactly what load_session reads), and replay-verify
    // the written dir reproduces the canonical hash. record == replay is the gem reproducibility contract (inv #3).
    let stage = session_dir.join(".verify");
    let _ = std::fs::remove_dir_all(&stage);
    let canonical = record_episode(&env_config, gem.config.master_seed, &journal, &stage)?.hash;
    let _ = std::fs::remove_dir_all(&stage);

    save_journal(&session_dir, &env_config, gem.config.master_seed, &journal)?;
    let replayed = replay(&session_dir)?;
    if replayed != canonical {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "checkpoint session {slug:?} does not replay stably (recorded {canonical:016x}, replay {replayed:016x})"
            ),
        ));
    }

    let meta = CheckpointStarter {
        name: name.to_string(),
        caption: gem.caption.clone(),
        dynamics: dynamics_from_caption(&gem.caption),
        checkpoint_gen,
        source_hash: hex16(gem.recorded_hash),
        source_seed: gem.config.master_seed,
    };
    let meta_path = session_dir.join("starter.json");
    let json = serde_json::to_string_pretty(&meta).map_err(to_io)?;
    std::fs::write(&meta_path, format!("{json}\n"))?;
    Ok(session_dir)
}

/// Rebuild `<starters_dir>/index.json` by SCANNING the dir: every `<slug>.json` (other than `index.json`) is a
/// gen-1 starter; every `<slug>/starter.json` is a checkpoint. The index is a pure function of the dir contents,
/// sorted by slug (deterministic regardless of promote order). Returns the written index path.
///
/// # Errors
/// An [`io::Error`] from reading the dir, parsing a starter doc, or writing the index.
pub fn rebuild_index(starters_dir: &Path) -> io::Result<PathBuf> {
    std::fs::create_dir_all(starters_dir)?;
    let mut paths: Vec<PathBuf> = std::fs::read_dir(starters_dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    paths.sort(); // deterministic scan order (the final list is re-sorted by slug below regardless)

    let mut entries: Vec<StarterIndexEntry> = Vec::new();
    for path in paths {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if path.is_file() {
            if file_name == "index.json" || !file_name.ends_with(".json") {
                continue;
            }
            let slug = file_name.trim_end_matches(".json").to_string();
            let doc: Gen1Starter =
                serde_json::from_str(&std::fs::read_to_string(&path)?).map_err(to_io)?;
            entries.push(StarterIndexEntry {
                slug,
                name: doc.name,
                kind: "gen1".to_string(),
                caption: doc.caption,
                dynamics: doc.dynamics,
                source_hash: doc.source_hash,
            });
        } else if path.is_dir() {
            let meta_path = path.join("starter.json");
            if !meta_path.exists() {
                continue; // a stray dir without metadata — skip (defensive)
            }
            let doc: CheckpointStarter =
                serde_json::from_str(&std::fs::read_to_string(&meta_path)?).map_err(to_io)?;
            entries.push(StarterIndexEntry {
                slug: file_name,
                name: doc.name,
                kind: "checkpoint".to_string(),
                caption: doc.caption,
                dynamics: doc.dynamics,
                source_hash: doc.source_hash,
            });
        }
    }
    entries.sort_by(|a, b| a.slug.cmp(&b.slug));

    let path = starters_dir.join("index.json");
    let json = serde_json::to_string_pretty(&entries).map_err(to_io)?;
    std::fs::write(&path, format!("{json}\n"))?;
    Ok(path)
}

/// Read every `*.json` gem in `gems_dir` (skipping `.verify-*` throwaway stage dirs), parse it as a [`Gem`],
/// and return them name-sorted. A missing dir yields an empty list (the default-set promote degrades to "no
/// gems found" rather than an error — `data/runs/gems` is gitignored and may be absent on a fresh checkout).
fn read_gems(gems_dir: &Path) -> io::Result<Vec<Gem>> {
    if !gems_dir.exists() {
        return Ok(Vec::new());
    }
    let mut names: Vec<PathBuf> = std::fs::read_dir(gems_dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.is_file()
                && p.extension().is_some_and(|x| x == "json")
                && !p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with('.'))
        })
        .collect();
    names.sort();

    let mut gems = Vec::with_capacity(names.len());
    for path in names {
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(gem) = serde_json::from_str::<Gem>(&text) {
                gems.push(gem);
            }
        }
    }
    Ok(gems)
}

/// Pick a DEFAULT set covering DISTINCT dynamics shapes: the single BEST gem per dynamics word (by `score` then
/// `novelty` then `recorded_hash`, all deterministic), in alphabetical dynamics order, capped at `max`. Returns
/// `(slug, name, gem)` triples — `slug` is the slugified dynamics word, `name` its title-case form.
fn select_distinct_dynamics(mut gems: Vec<Gem>, max: usize) -> Vec<(String, String, Gem)> {
    // Sort so that, within each dynamics group, the BEST gem comes first (score desc, novelty desc, hash asc);
    // groups are ordered alphabetically. Taking the first gem of each not-yet-seen dynamics then yields the best
    // representative per shape, in a deterministic order.
    gems.sort_by(|a, b| {
        let da = dynamics_from_caption(&a.caption);
        let db = dynamics_from_caption(&b.caption);
        da.cmp(&db)
            .then(b.score.cmp(&a.score))
            .then(b.novelty.cmp(&a.novelty))
            .then(a.recorded_hash.cmp(&b.recorded_hash))
    });

    let mut out: Vec<(String, String, Gem)> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for gem in gems {
        let dynamics = dynamics_from_caption(&gem.caption);
        if seen.iter().any(|s| s == &dynamics) {
            continue;
        }
        seen.push(dynamics.clone());
        out.push((slugify(&dynamics), title_from_slug(&dynamics), gem));
        if out.len() >= max {
            break;
        }
    }
    out
}

/// Promote a sensible DEFAULT set of GEN-1 starters from `gems_dir` (one per distinct dynamics shape, best
/// first, capped at `max`) into `starters_dir`, then rebuild the index. Returns the promoted slugs (empty when
/// no gems are found — the index is still rebuilt). The default fallback when a caller passes no explicit
/// candidate selection. Each gen-1 promote recomputes its `source_hash` from an edit-free replay (so the
/// roster is resolved through `species_dir`).
///
/// # Errors
/// An [`io::Error`] from reading gems, recomputing/writing a starter (incl. an unresolvable roster), or
/// rebuilding the index.
pub fn promote_default_set(
    gems_dir: &Path,
    species_dir: &Path,
    starters_dir: &Path,
    max: usize,
) -> io::Result<Vec<String>> {
    let gems = read_gems(gems_dir)?;
    let chosen = select_distinct_dynamics(gems, max);
    let mut slugs = Vec::with_capacity(chosen.len());
    for (slug, name, gem) in &chosen {
        promote_gen1(gem, slug, name, species_dir, starters_dir)?;
        slugs.push(slug.clone());
    }
    rebuild_index(starters_dir)?;
    Ok(slugs)
}

/// Map a `serde_json` error into an [`io::Error`] so the public API surfaces a single error type (mirrors the
/// `replay` module's helper).
fn to_io(e: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Action;
    use discovery::search::EditGene;
    use discovery::FP_DIMS;

    /// The repo-root `data/species` dir (the byte-mover boundary; mirrors the discover/replay test helpers).
    fn species_dir() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/species"))
    }

    /// The repo-root committed starter library (`data/presets/starters`) — the already-shipped gen-1 docs.
    fn committed_starters_dir() -> PathBuf {
        PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/presets/starters"
        ))
    }

    /// A RAII temp dir guard (std-only cleanup — the harness has no `tempfile` dep). Removes the dir on drop.
    struct TempDir(PathBuf);
    impl TempDir {
        fn new(label: &str) -> Self {
            let mut p = std::env::temp_dir();
            p.push(format!("gene-sim-promote-{label}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).expect("create temp dir");
            TempDir(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A tiny edit-free config (resolves through the data dir → a real off-hash run → a real recorded_hash).
    fn edit_free_config(seed: u64) -> SearchConfig {
        SearchConfig {
            master_seed: seed,
            roster: vec![("default".to_string(), 300)],
            containment_level: 0,
            temp_q: 500,
            season: 0,
            edits: Vec::new(),
        }
    }

    /// A config carrying two real mid-run edits on two present species (maps to two ApplyEdit actions).
    fn edited_config(seed: u64) -> SearchConfig {
        SearchConfig {
            master_seed: seed,
            roster: vec![("default".to_string(), 500), ("ecoli".to_string(), 200)],
            containment_level: 0,
            temp_q: 500,
            season: 0,
            edits: vec![
                EditGene {
                    gen: 20_000, // ~0.30 of the run → fires early
                    species_index: 0,
                    target: 0,
                    guide: "ACGTACGTACGTACGTACGT".to_string(),
                },
                EditGene {
                    gen: 40_000, // ~0.61 of the run → fires later
                    species_index: 1,
                    target: 3,
                    guide: "TTTTGGGGCCCCAAAATTTT".to_string(),
                },
            ],
        }
    }

    /// Build a REAL gem over `cfg`: rebuild its env, record its full journal (edits interleaved) to derive a
    /// real `recorded_hash`, and stamp a caption so `dynamics_from_caption` resolves. The gem's `recorded_hash`
    /// is COMPUTED from the journal, so it is internally consistent (a real score_config would match for a run
    /// that does not early-stop). `caption` is supplied so the gem reads as a specific dynamics shape.
    fn build_gem(cfg: &SearchConfig, gens: u32, caption: &str, stage: &Path) -> Gem {
        let (env_config, skipped) = env_config_for(cfg, &species_dir());
        assert!(skipped.is_empty(), "fixture roster resolves: {skipped:?}");
        let env_config = env_config.expect("fixture roster resolves");
        let actions = edits_to_actions(cfg, &env_config.roster, gens);
        let journal = build_journal(&actions, gens);
        let _ = std::fs::remove_dir_all(stage);
        let recorded =
            record_episode(&env_config, cfg.master_seed, &journal, stage).expect("record fixture");
        let hash = recorded.hash;
        let _ = std::fs::remove_dir_all(stage);
        Gem {
            config: cfg.clone(),
            score: 0,
            quality: 0,
            novelty: 0,
            breakdown: [0; 6],
            fingerprint: [0; FP_DIMS],
            recorded_hash: hash,
            build_id: crate::discover::BUILD_ID.to_string(),
            caption: caption.to_string(),
            gens,
            gens_requested: gens,
        }
    }

    /// Recompute a gem's EDIT-FREE replay hash exactly as [`promote_gen1`] does — rebuild the PRISTINE config's
    /// env, run `Advance(1)*horizon`, take the hash — via the public surface, so a test can assert the stored
    /// `source_hash` matches WITHOUT trusting `promote_gen1`'s internals. The horizon mirrors `promote_gen1`
    /// (`gens_requested`, falling back to `gens`).
    fn edit_free_replay_hash(gem: &Gem, stage: &Path) -> u64 {
        let cfg = StarterConfig::from_gem(gem);
        let horizon = if gem.gens_requested == 0 {
            gem.gens
        } else {
            gem.gens_requested
        };
        let search = cfg.to_search_config(gem.config.master_seed);
        let (env, _) = env_config_for(&search, &species_dir());
        let env = env.expect("pristine env resolves");
        let journal = build_journal(&[], horizon);
        let _ = std::fs::remove_dir_all(stage);
        let h = record_episode(&env, gem.config.master_seed, &journal, stage)
            .expect("record edit-free")
            .hash;
        let _ = std::fs::remove_dir_all(stage);
        h
    }

    #[test]
    fn dynamics_facet_is_the_caption_leading_word() {
        assert_eq!(dynamics_from_caption("drift · 1 spp · steady"), "drift");
        assert_eq!(
            dynamics_from_caption("limit-cycle · 3 spp · 2 takeovers"),
            "limit-cycle"
        );
        assert_eq!(dynamics_from_caption(""), "unknown");
    }

    #[test]
    fn gen1_starter_rebuilds_same_env_and_replays_to_source_hash() {
        // TEST (a) — THE EDIT-FREE GEN-1 CONTRACT: a promoted gen-1 starter's stored config rebuilds the SAME
        // EnvConfig as the source gem AND its RECOMPUTED source_hash == the edit-free replay hash == the gem's
        // recorded_hash (an edit-free gem → unchanged behaviour). source_had_edits is false; gens is the horizon.
        let tmp = TempDir::new("gen1");
        let gens = 40u32;
        let cfg = edit_free_config(0x57A1_0001);
        let gem = build_gem(
            &cfg,
            gens,
            "drift · 1 spp · steady",
            &tmp.path().join("stage"),
        );

        let starters = tmp.path().join("starters");
        let path =
            promote_gen1(&gem, "drift", "Drift", &species_dir(), &starters).expect("promote gen-1");

        // The written doc round-trips with faithful provenance. For an EDIT-FREE gem the recomputed source_hash
        // is byte-identical to gem.recorded_hash (the edit-free journal IS the gem's journal).
        let doc: Gen1Starter =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("parse");
        assert_eq!(doc.source_hash, hex16(gem.recorded_hash));
        assert_eq!(doc.source_seed, gem.config.master_seed);
        assert_eq!(doc.dynamics, "drift");
        assert_eq!(doc.config, StarterConfig::from_gem(&gem));
        assert!(
            !doc.source_had_edits,
            "an edit-free gem promotes a source_had_edits=false starter"
        );
        assert_eq!(doc.gens, gem.gens, "gens is the recompute horizon");

        // (1) the starter config rebuilds the SAME SearchConfig as the (edit-free) gem → the SAME EnvConfig.
        let rebuilt = doc.config.to_search_config(doc.source_seed);
        assert_eq!(
            rebuilt, gem.config,
            "the edit-free gem's config is recovered byte-for-byte from the starter"
        );
        let (env_from_starter, _) = env_config_for(&rebuilt, &species_dir());
        let env_from_starter = env_from_starter.expect("starter env resolves");
        let (env_from_gem, _) = env_config_for(&gem.config, &species_dir());
        let env_from_gem = env_from_gem.expect("gem env resolves");
        assert_eq!(env_from_starter.entity_count, env_from_gem.entity_count);
        assert_eq!(env_from_starter.env, env_from_gem.env);
        let keys = |e: &crate::replay::EnvConfig| -> Vec<(String, u32)> {
            e.roster.iter().map(|(b, n)| (b.key.clone(), *n)).collect()
        };
        assert_eq!(
            keys(&env_from_starter),
            keys(&env_from_gem),
            "the starter rebuilds the SAME resolved roster"
        );

        // (2) it REPLAYS to source_hash: the edit-free journal (Advance(1)*gens) reproduces the gem's hash.
        let journal = vec![Action::Advance(1); gem.gens as usize];
        let stage = tmp.path().join("replay-stage");
        let recorded = record_episode(&env_from_starter, doc.source_seed, &journal, &stage)
            .expect("record from starter");
        let replayed = replay(&recorded.dir).expect("replay");
        assert_eq!(
            recorded.hash, gem.recorded_hash,
            "the starter config replays to the source hash (gen-1 reproducibility)"
        );
        assert_eq!(replayed, recorded.hash, "record == replay (inv #3)");
        assert_eq!(
            hex16(replayed),
            doc.source_hash,
            "the replayed hash equals the doc's source_hash text"
        );
    }

    #[test]
    fn gen1_from_an_edited_gem_stores_the_edit_free_hash_and_flags_the_source() {
        // TEST (b) — THE ADR-031 FIX: a gem that CARRIES a (today hash-neutral) firing edit promotes a gen-1
        // starter whose stored source_hash is the EDIT-FREE replay of the PRISTINE config (NOT a blind copy of
        // gem.recorded_hash) + source_had_edits=true. Once edits become hash-active, the stored hash still equals
        // what the edit-free gen-1 config actually produces, so the doc never silently stops replaying.
        let tmp = TempDir::new("gen1-edited");
        let gens = 60u32;
        let cfg = edited_config(0xED17_0003);
        assert!(
            !cfg.edits.is_empty(),
            "fixture carries a firing edit schedule"
        );
        let gem = build_gem(
            &cfg,
            gens,
            "eventful · 2 spp · edits",
            &tmp.path().join("stage"),
        );

        let starters = tmp.path().join("starters");
        let path = promote_gen1(&gem, "eventful", "Eventful", &species_dir(), &starters)
            .expect("promote gen-1");
        let doc: Gen1Starter =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();

        // The pristine config drops the edits; source_had_edits records the PROVENANCE (the gem had edits).
        assert!(doc.source_had_edits, "the source gem carried edits");
        assert!(doc.config.roster.iter().any(|(_, n)| *n > 0));
        assert_eq!(
            doc.gens, gem.gens_requested,
            "gens is the recompute horizon"
        );

        // The stored hash equals an INDEPENDENT edit-free replay of the pristine config (the meaningful
        // invariant, robust to edits becoming hash-active) — it is NOT defined as gem.recorded_hash.
        let editfree = edit_free_replay_hash(&gem, &tmp.path().join("ef-stage"));
        assert_eq!(
            doc.source_hash,
            hex16(editfree),
            "source_hash is the edit-free replay of the pristine config"
        );
    }

    #[test]
    fn committed_gen1_starters_still_load_under_the_new_struct() {
        // BACKWARD COMPAT PROOF: every ALREADY-COMMITTED gen-1 doc in data/presets/starters/ (which lacks the new
        // gens/source_had_edits fields) STILL deserializes as Gen1Starter via the serde defaults — so the gallery
        // + rebuild_index keep loading the shipped library un-re-promoted. They are edit-free, so the defaults
        // (gens=0, source_had_edits=false) are the truthful values; their existing source_hash is preserved.
        let dir = committed_starters_dir();
        let mut loaded = 0usize;
        for entry in std::fs::read_dir(&dir).expect("read committed starters dir") {
            let path = entry.expect("dir entry").path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if !path.is_file() || name == "index.json" || !name.ends_with(".json") {
                continue; // skip the index + the checkpoint session dirs (e.g. branch-point/)
            }
            let doc: Gen1Starter =
                serde_json::from_str(&std::fs::read_to_string(&path).expect("read committed doc"))
                    .unwrap_or_else(|e| panic!("committed starter {name:?} deserializes: {e}"));
            assert_eq!(doc.source_hash.len(), 16, "{name}: 16 hex source_hash");
            assert_eq!(doc.gens, 0, "{name}: pre-fix doc → gens defaults to 0");
            assert!(
                !doc.source_had_edits,
                "{name}: pre-fix doc → source_had_edits defaults to false"
            );
            loaded += 1;
        }
        assert!(
            loaded >= 6,
            "the committed gen-1 library is present and loads ({loaded} docs)"
        );
    }

    #[test]
    fn gen1_doc_round_trips_through_serde_with_the_new_fields() {
        // TEST (c) — the new `gens` + `source_had_edits` fields survive a serialize → deserialize round trip.
        let doc = Gen1Starter {
            name: "Eventful".to_string(),
            caption: "eventful · 2 spp · edits".to_string(),
            dynamics: "eventful".to_string(),
            source_hash: "0123456789abcdef".to_string(),
            source_seed: 0xDEAD_BEEF,
            gens: 60,
            source_had_edits: true,
            config: StarterConfig {
                roster: vec![("default".to_string(), 300)],
                containment_level: 1,
                temp_q: 500,
                season: 2,
            },
        };
        let json = serde_json::to_string_pretty(&doc).unwrap();
        let back: Gen1Starter = serde_json::from_str(&json).unwrap();
        assert_eq!(back, doc, "the doc round-trips byte-for-byte through serde");
        assert_eq!(back.gens, 60);
        assert!(back.source_had_edits);
    }

    #[test]
    fn pre_fix_committed_starter_shape_still_deserializes() {
        // TEST (d) — an OLD committed starter doc (NO gens / source_had_edits) deserializes via the serde
        // defaults: gens → 0, source_had_edits → false. This is the EXACT shape of the already-committed starters
        // (e.g. data/presets/starters/drift.json), so rebuild_index + the gallery still load them un-re-promoted.
        let old = r#"{
            "name": "Drift",
            "caption": "drift · 3 spp · steady",
            "dynamics": "drift",
            "source_hash": "518347049d204604",
            "source_seed": 16777275962096678196,
            "config": {
                "roster": [["default", 1227], ["ecoli", 110]],
                "containment_level": 0,
                "temp_q": 617,
                "season": 1
            }
        }"#;
        let doc: Gen1Starter = serde_json::from_str(old)
            .expect("the pre-fix committed starter shape still deserializes");
        assert_eq!(doc.gens, 0, "a missing gens defaults to 0");
        assert!(
            !doc.source_had_edits,
            "a missing source_had_edits defaults to false"
        );
        assert_eq!(
            doc.source_hash, "518347049d204604",
            "the existing source_hash is preserved verbatim"
        );
        assert_eq!(doc.config.roster.len(), 2);
    }

    #[test]
    fn gen1_doc_is_self_contained_re_verifiable() {
        // TEST (e) — the gen-1 doc is SELF-CONTAINED re-verifiable: rebuild the stored config for the stored
        // seed, run it EDIT-FREE for the stored `gens`, and the hash reproduces source_hash — with NO source gem.
        let tmp = TempDir::new("gen1-reverify");
        let gens = 50u32;
        let gem = build_gem(
            &edit_free_config(0x5E1F_0005),
            gens,
            "drift · 1 spp · steady",
            &tmp.path().join("stage"),
        );
        let starters = tmp.path().join("starters");
        let path =
            promote_gen1(&gem, "drift", "Drift", &species_dir(), &starters).expect("promote gen-1");
        let doc: Gen1Starter =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();

        // Re-run STRICTLY from the doc (config + source_seed + gens) — the gem is not consulted.
        let search = doc.config.to_search_config(doc.source_seed);
        let (env, _) = env_config_for(&search, &species_dir());
        let env = env.expect("the stored config resolves");
        let journal = build_journal(&[], doc.gens);
        let stage = tmp.path().join("reverify-stage");
        let recorded = record_episode(&env, doc.source_seed, &journal, &stage).expect("record");
        let replayed = replay(&recorded.dir).expect("replay");
        assert_eq!(replayed, recorded.hash, "record == replay (inv #3)");
        assert_eq!(
            hex16(replayed),
            doc.source_hash,
            "re-running the stored config for `gens` reproduces source_hash"
        );
    }

    #[test]
    fn checkpoint_session_is_hash_stable_and_carries_edits_at_the_right_generations() {
        // THE GEN-N CONTRACT: a promoted checkpoint records a session that replays to a STABLE hash (record ==
        // replay), and its actions.ndjson carries the gem's edits interleaved at the right absolute generations.
        let tmp = TempDir::new("checkpoint");
        let gens = 60u32;
        let checkpoint_gen = 45u32;
        let cfg = edited_config(0xC4ED_0002);
        let gem = build_gem(
            &cfg,
            gens,
            "boom-bust · 2 spp · crashes",
            &tmp.path().join("stage"),
        );

        let starters = tmp.path().join("starters");
        let session_dir = promote_checkpoint(
            &gem,
            "branch-point",
            "Branch Point",
            checkpoint_gen,
            &species_dir(),
            &starters,
        )
        .expect("promote checkpoint");

        // The session is the SAME format load_session reads: seed.json + actions.ndjson, sibling starter.json.
        assert!(session_dir.join("seed.json").exists(), "seed.json written");
        assert!(
            session_dir.join("actions.ndjson").exists(),
            "actions.ndjson written"
        );
        let meta: CheckpointStarter = serde_json::from_str(
            &std::fs::read_to_string(session_dir.join("starter.json")).unwrap(),
        )
        .expect("parse starter.json");
        assert_eq!(meta.checkpoint_gen, checkpoint_gen);
        assert_eq!(meta.source_hash, hex16(gem.recorded_hash));
        assert_eq!(meta.dynamics, "boom-bust");

        // (1) HASH-STABLE: replaying the written session twice yields the same hash, and it equals a fresh
        // record of the same journal (the gem reproducibility contract — record == replay, inv #3).
        let h1 = replay(&session_dir).expect("replay 1");
        let h2 = replay(&session_dir).expect("replay 2");
        assert_eq!(h1, h2, "the checkpoint session replays to a stable hash");

        // (2) the actions.ndjson carries the gem's edits at the right generations. Reconstruct the expected
        // schedule and assert each ApplyEdit lands at its absolute generation < checkpoint_gen.
        let (env_config, _) = env_config_for(&gem.config, &species_dir());
        let env_config = env_config.expect("env resolves");
        let expected = edits_to_actions(&gem.config, &env_config.roster, gem.gens_requested);
        assert_eq!(expected.len(), 2, "both edits resolve within the horizon");

        let (_seed_json, actions) =
            crate::replay::read_journal(&session_dir).expect("read journal");
        // Count the Advance(1)s before each ApplyEdit to recover its absolute generation in the journal.
        let mut gen_cursor = 0u32;
        let mut fired: Vec<u32> = Vec::new();
        for a in &actions {
            match a {
                Action::Advance(_) => gen_cursor += 1,
                Action::ApplyEdit(_) => fired.push(gen_cursor + 1), // applied at the TOP of the next gen's step
                _ => {}
            }
        }
        // Only the edits scheduled strictly before checkpoint_gen fire in the session.
        let want: Vec<u32> = expected
            .iter()
            .filter(|(g, _)| *g >= 1 && *g <= checkpoint_gen)
            .map(|(g, _)| *g)
            .collect();
        assert_eq!(
            fired, want,
            "actions.ndjson carries the gem's edits at the right generations"
        );
        assert!(
            !fired.is_empty(),
            "at least one edit fires within the checkpoint horizon (the session genuinely carries edits)"
        );
    }

    #[test]
    fn index_enumerates_gen1_and_checkpoint_starters_deterministically() {
        // The index is a slug-sorted list of {slug, name, kind, caption, dynamics, source_hash}, a pure function
        // of the dir contents — deterministic regardless of promote order.
        let tmp = TempDir::new("index");
        let starters = tmp.path().join("starters");
        let stage = tmp.path().join("stage");

        let g_drift = build_gem(
            &edit_free_config(0x1DEA_0001),
            40,
            "drift · 1 spp · steady",
            &stage,
        );
        let g_boom = build_gem(
            &edited_config(0xB00B_0002),
            60,
            "boom-bust · 2 spp · crashes",
            &stage,
        );

        // Promote a checkpoint FIRST, a gen-1 SECOND — the index must still be slug-sorted.
        promote_checkpoint(
            &g_boom,
            "boom-bust",
            "Boom Bust",
            50,
            &species_dir(),
            &starters,
        )
        .expect("promote checkpoint");
        promote_gen1(&g_drift, "drift", "Drift", &species_dir(), &starters).expect("promote gen-1");
        rebuild_index(&starters).expect("rebuild index");

        let idx: Vec<StarterIndexEntry> = serde_json::from_str(
            &std::fs::read_to_string(starters.join("index.json")).expect("read index"),
        )
        .expect("parse index");
        assert_eq!(idx.len(), 2, "both starters indexed");
        // Slug-sorted: "boom-bust" < "drift".
        assert_eq!(idx[0].slug, "boom-bust");
        assert_eq!(idx[0].kind, "checkpoint");
        assert_eq!(idx[0].dynamics, "boom-bust");
        assert_eq!(idx[1].slug, "drift");
        assert_eq!(idx[1].kind, "gen1");
        assert_eq!(idx[1].source_hash, hex16(g_drift.recorded_hash));

        // Rebuilding again is byte-stable (deterministic).
        let bytes_a = std::fs::read(starters.join("index.json")).expect("a");
        rebuild_index(&starters).expect("rebuild again");
        let bytes_b = std::fs::read(starters.join("index.json")).expect("b");
        assert_eq!(bytes_a, bytes_b, "the index is byte-stable across rebuilds");
    }

    #[test]
    fn default_set_covers_distinct_dynamics_one_per_shape() {
        // promote_default_set picks ONE gem per distinct dynamics shape (best first), so a pile of mostly-drift
        // gems + a couple boom-bust yields exactly two starters (drift + boom-bust), slug-sorted in the index.
        let tmp = TempDir::new("defset");
        let gems_dir = tmp.path().join("gems");
        std::fs::create_dir_all(&gems_dir).unwrap();
        let stage = tmp.path().join("stage");

        // Three drift gems (distinct novelty) + two boom-bust + one flat → 3 distinct shapes.
        let write = |cfg: &SearchConfig, cap: &str, nov: u16, name: &str| {
            let mut gem = build_gem(cfg, 40, cap, &stage);
            gem.novelty = nov;
            std::fs::write(
                gems_dir.join(format!("{name}.json")),
                serde_json::to_string_pretty(&gem).unwrap(),
            )
            .unwrap();
        };
        write(
            &edit_free_config(0xD1),
            "drift · 1 spp · steady",
            4000,
            "g1",
        );
        write(
            &edit_free_config(0xD2),
            "drift · 1 spp · steady",
            9000,
            "g2",
        ); // best drift
        write(
            &edit_free_config(0xD3),
            "drift · 1 spp · steady",
            5000,
            "g3",
        );
        write(
            &edit_free_config(0xB1),
            "boom-bust · 1 spp · steady",
            3000,
            "g4",
        );
        write(
            &edit_free_config(0xB2),
            "boom-bust · 1 spp · steady",
            7000,
            "g5",
        ); // best boom
        write(&edit_free_config(0xF1), "flat · 1 spp · steady", 6000, "g6");

        let starters = tmp.path().join("starters");
        let slugs =
            promote_default_set(&gems_dir, &species_dir(), &starters, 8).expect("default set");
        // One starter per distinct dynamics shape, alphabetical: boom-bust, drift, flat.
        assert_eq!(slugs, vec!["boom-bust", "drift", "flat"]);

        // The chosen drift starter is the BEST drift gem (novelty 9000 → seed 0xD2).
        let drift: Gen1Starter =
            serde_json::from_str(&std::fs::read_to_string(starters.join("drift.json")).unwrap())
                .unwrap();
        assert_eq!(drift.source_seed, 0xD2, "the best drift gem was chosen");

        // The index lists all three, slug-sorted, all gen-1.
        let idx: Vec<StarterIndexEntry> =
            serde_json::from_str(&std::fs::read_to_string(starters.join("index.json")).unwrap())
                .unwrap();
        assert_eq!(idx.len(), 3);
        assert!(idx.iter().all(|e| e.kind == "gen1"));
        assert_eq!(
            idx.iter().map(|e| e.slug.as_str()).collect::<Vec<_>>(),
            vec!["boom-bust", "drift", "flat"]
        );

        // A missing gems dir degrades to an empty set (the index is still rebuilt).
        let empty = promote_default_set(
            &tmp.path().join("absent"),
            &species_dir(),
            &tmp.path().join("st2"),
            8,
        )
        .expect("empty default set");
        assert!(empty.is_empty(), "no gems → no starters");
        assert!(
            tmp.path().join("st2").join("index.json").exists(),
            "the index is rebuilt even when empty"
        );
    }

    #[test]
    fn title_from_slug_is_human_readable() {
        assert_eq!(title_from_slug("limit-cycle"), "Limit Cycle");
        assert_eq!(title_from_slug("drift"), "Drift");
        assert_eq!(title_from_slug("boom_bust"), "Boom Bust");
    }
}
