//! FBA (flux-balance-analysis) KO oracle — the E. coli "deep edit" boundary crate (ADR-017 S3).
//!
//! **Invariant #1 (STOP THE LINE):** a real FBA solve uses a GPL/heavy solver (GLPK via cobrapy). Exactly like
//! [`oracle-slim`](../../oracle-slim) shells out to the GPL `slim` CLI, this crate keeps the solver at the
//! PROCESS BOUNDARY: it depends on **nothing at all** (std-only) and links no GPL code. The license gate
//! (`scripts/check_license.sh`, `oracle-fba` in `BOUNDARY_CRATES`) enforces the dependency-free tree.
//!
//! For a SINGLE-GENE edit the "solve" is collapsed to a **frozen-table lookup** of `data/ecoli_ko_table.json`
//! — a tiny table of quantized KO growth ratios produced OFFLINE by `scripts/bake_ecoli_ko_table.py` (a real
//! cobrapy FBA bake on BiGG `e_coli_core`, glucose-minimal aerobic). The offline solver's float
//! non-determinism is frozen into `u16` permille at bake time and never reaches this crate — this lookup
//! returns the QUANTIZED `u16` directly (floats never escape), the one-way integer crossing the ADR-017
//! firewall pins. A future "realistic" impl can `resolve_fba_bin` + `Command::new` an FBA CLI exactly as
//! `oracle-slim` does for `slim`, still emitting already-quantized integers.
//!
//! This crate is the PRODUCER side of the firewall (off-thread, off-hash). The deterministic sim
//! (`crates/harness` firewall) only ever consumes the `u16` it returns, committed at a fixed future epoch via
//! a journaled Action — never re-running this lookup on replay (the committed integer rides in the journal).

#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

/// The bare CLI name used when neither `FBA_BIN` nor the pinned location resolves (PATH lookup). Reserved for a
/// future subprocess-backed "realistic" impl; the frozen-table path does not spawn anything.
const FBA_CLI_NAME: &str = "fba-oracle";

/// The frozen KO table the single-gene lookup reads, relative to the crate manifest dir. Baked OFFLINE by
/// `scripts/bake_ecoli_ko_table.py`; committed as integer data so no float crosses into the sim (inv #3).
const FROZEN_KO_TABLE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../data/ecoli_ko_table.json"
);

/// The permille scale of a growth ratio: 1000 = wild-type, 0 = growth-lethal. The table stores `u16` permille;
/// callers quantize against this denominator (mirrors `sim_core::fixed::PERMILLE`, kept local to stay dep-free).
pub const GROWTH_RATIO_PERMILLE_SCALE: u16 = 1000;

/// How a deep edit acts on its target gene's transcription. Mirrors `crispr::EditKind` (commit 41a7f48) by VALUE
/// so this boundary crate carries no dependency on `crispr`. The frozen table only encodes a full KNOCKOUT
/// ratio; a graded edit scales it deterministically with integer math (see [`single_gene_growth_ratio_q`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    /// Full loss of function — the frozen KO ratio applies directly.
    Knockout,
    /// Partial loss of function — the KO effect is halved toward wild-type (integer midpoint, deterministic).
    Knockdown,
    /// Gain of function / over-expression — modeled as wild-type for the frozen single-gene table (no KO data).
    Activate,
}

/// Parameters for one single-gene FBA KO lookup. Plain data — the gene is named by its frozen-table `b_number`
/// (the BiGG/NCBI locus tag, e.g. `"b0720"` for gltA), the canonical key the offline bake indexes by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FbaParams {
    /// The target gene's b-number locus tag (e.g. `"b0720"`), as keyed in `data/ecoli_ko_table.json`.
    pub b_number: String,
    /// How the edit acts on the gene (Knockout uses the frozen ratio; Knockdown halves it; Activate = neutral).
    pub edit_kind: EditKind,
}

/// Resolve the FBA "binary" path **robustly** (reserved for a future subprocess-backed realistic impl), so a
/// later swap to a live FBA CLI works regardless of `PATH` — mirroring [`oracle_slim::resolve_slim_bin`]:
///
/// 1. `$FBA_BIN`, if set (explicit override);
/// 2. else `$HOME/.local/bin/fba-oracle`, if it exists (the pinned install location);
/// 3. else the bare name `"fba-oracle"`, resolved via `PATH` by the OS.
///
/// The frozen-table lookup does not spawn a process; this exists so S3's structure mirrors `oracle-slim` and the
/// realistic impl is a drop-in (inv #5 — science pluggable behind the same shape).
#[must_use]
pub fn resolve_fba_bin() -> PathBuf {
    if let Some(bin) = std::env::var_os("FBA_BIN") {
        return PathBuf::from(bin);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let candidate = Path::new(&home).join(".local/bin/fba-oracle");
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from(FBA_CLI_NAME)
}

/// Error returned by the FBA lookup. **Mirrors [`oracle_slim::SlimError`]** variant-for-variant so the two
/// boundary crates have the same failure surface (inv #5): an I/O failure reading the frozen table, a spawn
/// failure (reserved for the subprocess impl), a non-zero exit (subprocess impl), and a missing/unparsable
/// output (here: the gene is absent from the frozen table, or the table is malformed).
#[derive(Debug)]
pub enum FbaError {
    /// Reading the frozen KO table (or a future work directory) failed.
    Io(io::Error),
    /// The FBA process could not be spawned (reserved for the subprocess-backed realistic impl).
    Spawn { bin: PathBuf, source: io::Error },
    /// The FBA process ran but exited non-zero (reserved for the subprocess impl). `stderr` is its captured err.
    NonZeroExit { status: String, stderr: String },
    /// The lookup produced no usable result: the gene is not in the frozen table, or the table is malformed
    /// (mirrors `SlimError::MissingOutput` — "ran but produced nothing usable"). `detail` says which.
    MissingOutput { detail: String },
}

impl fmt::Display for FbaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FbaError::Io(e) => write!(f, "io error preparing fba lookup: {e}"),
            FbaError::Spawn { bin, source } => {
                write!(f, "failed to spawn fba binary {}: {source}", bin.display())
            }
            FbaError::NonZeroExit { status, stderr } => {
                write!(f, "fba exited {status}; stderr:\n{stderr}")
            }
            FbaError::MissingOutput { detail } => {
                write!(f, "fba produced no usable growth ratio: {detail}")
            }
        }
    }
}

impl Error for FbaError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            FbaError::Io(e) => Some(e),
            FbaError::Spawn { source, .. } => Some(source),
            FbaError::NonZeroExit { .. } | FbaError::MissingOutput { .. } => None,
        }
    }
}

impl From<io::Error> for FbaError {
    fn from(e: io::Error) -> Self {
        FbaError::Io(e)
    }
}

/// Look up the QUANTIZED single-gene KO growth ratio (u16 permille of wild-type) for `params`, reading the
/// committed frozen table at the pinned `data/ecoli_ko_table.json`.
///
/// This is the public boundary entry point: **a `u16` permille comes out, a float never does** — the offline
/// FBA solve was frozen to integers at bake time. `Knockout` returns the table value directly; `Knockdown`
/// halves the KO *effect* toward wild-type with integer math; `Activate` returns wild-type (1000) since the
/// single-gene KO table has no over-expression data. The result is the producer payload the firewall later
/// commits at a fixed epoch (UNREAD by selection until S6 — coefficient zero, hash-neutral).
///
/// # Errors
/// [`FbaError::Io`] if the frozen table cannot be read; [`FbaError::MissingOutput`] if the gene is absent or the
/// table is malformed.
pub fn single_gene_growth_ratio_q(params: &FbaParams) -> Result<u16, FbaError> {
    let table = std::fs::read_to_string(FROZEN_KO_TABLE)?;
    lookup_in_table(&table, &params.b_number, params.edit_kind)
}

/// The pure, I/O-free core of [`single_gene_growth_ratio_q`]: given the raw frozen-table JSON text, the target
/// `b_number`, and the `edit_kind`, return the quantized growth ratio. Split out so tests can exercise the
/// quantization + missing-gene + graded-edit logic without touching the filesystem.
///
/// # Errors
/// [`FbaError::MissingOutput`] if the gene is absent or the table is malformed.
pub fn lookup_in_table(
    table_json: &str,
    b_number: &str,
    edit_kind: EditKind,
) -> Result<u16, FbaError> {
    let ko_permille = frozen_ko_permille(table_json, b_number)?;
    Ok(apply_edit_kind(ko_permille, edit_kind))
}

/// Apply the [`EditKind`] grading to a frozen KNOCKOUT permille, deterministically (integer math only):
/// * `Knockout` → the KO ratio as-is.
/// * `Knockdown` → halfway between the KO ratio and wild-type (1000): `ko + (1000 - ko)/2`, floor — a partial
///   loss of function is less severe than a full KO. Pure integer, so no float ever participates.
/// * `Activate` → wild-type (1000): the single-gene KO table carries no over-expression magnitude.
#[must_use]
fn apply_edit_kind(ko_permille: u16, edit_kind: EditKind) -> u16 {
    match edit_kind {
        EditKind::Knockout => ko_permille,
        EditKind::Knockdown => {
            let ko = u32::from(ko_permille);
            let wt = u32::from(GROWTH_RATIO_PERMILLE_SCALE);
            // Halfway toward wild-type (floor); ko <= wt always, so (wt - ko) is non-negative.
            (ko + (wt - ko) / 2) as u16
        }
        EditKind::Activate => GROWTH_RATIO_PERMILLE_SCALE,
    }
}

/// Extract the `growth_ratio_permille` for `b_number` from the frozen table JSON with a **minimal std-only
/// scan** (this crate carries no serde dependency — inv #1 dependency-free boundary). The frozen table is a
/// small, fixed-shape file this project bakes, so a tolerant field scan is sufficient and avoids pulling in a
/// JSON crate. Returns [`FbaError::MissingOutput`] if the gene or its permille field is not found.
fn frozen_ko_permille(table_json: &str, b_number: &str) -> Result<u16, FbaError> {
    // Find the gene object whose "b_number" equals the target, then read the nearest following
    // "growth_ratio_permille" integer. The bake writes one object per gene with these two fields, so scoping the
    // permille read to AFTER the matched b_number key is unambiguous for this fixed shape.
    let needle = format!("\"b_number\": \"{b_number}\"");
    let Some(at) = table_json.find(&needle) else {
        return Err(FbaError::MissingOutput {
            detail: format!("gene b_number {b_number:?} not in frozen KO table"),
        });
    };
    let rest = &table_json[at..];
    let key = "\"growth_ratio_permille\":";
    let Some(kpos) = rest.find(key) else {
        return Err(FbaError::MissingOutput {
            detail: format!("no growth_ratio_permille after gene {b_number:?} in frozen KO table"),
        });
    };
    let after = &rest[kpos + key.len()..];
    parse_leading_u16(after).ok_or_else(|| FbaError::MissingOutput {
        detail: format!(
            "growth_ratio_permille for {b_number:?} is not a valid integer in [0,1000]"
        ),
    })
}

/// Parse the leading integer of `s` (skipping leading whitespace), clamped to a valid permille `[0, 1000]`.
/// Returns `None` if no digits are present or the value exceeds 1000 (a malformed/out-of-range table entry).
fn parse_leading_u16(s: &str) -> Option<u16> {
    let trimmed = s.trim_start();
    let digits: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    let v: u32 = digits.parse().ok()?;
    if v > u32::from(GROWTH_RATIO_PERMILLE_SCALE) {
        return None;
    }
    Some(v as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A miniature frozen-table fixture in the exact shape `bake_ecoli_ko_table.py` emits.
    const FIXTURE: &str = r#"{
      "format_version": 1,
      "source": "cobrapy-fba",
      "genes": [
        { "gene": "gltA", "b_number": "b0720", "go_id": 4108, "locus_id": 10, "growth_ratio_permille": 0 },
        { "gene": "ptsG", "b_number": "b1101", "go_id": 8982, "locus_id": 32, "growth_ratio_permille": 1000 }
      ]
    }"#;

    #[test]
    fn knockout_returns_frozen_permille() {
        // gltA is growth-lethal (0); ptsG is neutral (1000) — the frozen integers come straight back.
        assert_eq!(
            lookup_in_table(FIXTURE, "b0720", EditKind::Knockout).unwrap(),
            0
        );
        assert_eq!(
            lookup_in_table(FIXTURE, "b1101", EditKind::Knockout).unwrap(),
            1000
        );
    }

    #[test]
    fn knockdown_is_halfway_toward_wild_type_integer() {
        // A partial loss is less severe than the full KO: ko=0 -> 0 + (1000-0)/2 = 500.
        assert_eq!(
            lookup_in_table(FIXTURE, "b0720", EditKind::Knockdown).unwrap(),
            500
        );
        // A neutral KO stays neutral under knockdown (1000 -> 1000 + 0/2 = 1000).
        assert_eq!(
            lookup_in_table(FIXTURE, "b1101", EditKind::Knockdown).unwrap(),
            1000
        );
    }

    #[test]
    fn activate_is_wild_type() {
        assert_eq!(
            lookup_in_table(FIXTURE, "b0720", EditKind::Activate).unwrap(),
            1000
        );
    }

    #[test]
    fn missing_gene_is_missing_output() {
        let err = lookup_in_table(FIXTURE, "b9999", EditKind::Knockout).unwrap_err();
        assert!(matches!(err, FbaError::MissingOutput { .. }), "got {err:?}");
        assert!(err.to_string().contains("b9999"));
    }

    #[test]
    fn parse_leading_u16_handles_whitespace_and_bounds() {
        assert_eq!(parse_leading_u16(" 0,").unwrap(), 0);
        assert_eq!(parse_leading_u16(" 1000 }").unwrap(), 1000);
        assert_eq!(parse_leading_u16("999\n"), Some(999));
        assert_eq!(parse_leading_u16("1001,"), None, "out of permille range");
        assert_eq!(parse_leading_u16("  ,"), None, "no digits");
    }

    /// The SHIPPED frozen table must parse and yield the biologically-frozen anchor values (gate-enforced data,
    /// not code). gltA KO is lethal (0); the other four anchors are neutral aerobically (1000).
    #[test]
    fn shipped_frozen_ko_table_loads_anchor_values() {
        // gltA b0720 — TCA entry, growth-lethal on glucose-minimal aerobic.
        let glt_a = single_gene_growth_ratio_q(&FbaParams {
            b_number: "b0720".to_string(),
            edit_kind: EditKind::Knockout,
        })
        .expect("frozen KO table should load and contain gltA");
        assert_eq!(glt_a, 0, "gltA KO is growth-lethal aerobically");

        for b in ["b1101", "b0903", "b2297", "b1380"] {
            let q = single_gene_growth_ratio_q(&FbaParams {
                b_number: b.to_string(),
                edit_kind: EditKind::Knockout,
            })
            .unwrap_or_else(|e| panic!("frozen KO table missing {b}: {e}"));
            assert_eq!(q, 1000, "{b} KO is growth-neutral aerobically");
        }
    }

    #[test]
    fn fba_error_mirrors_slim_error_shape() {
        // Structural mirror of SlimError: an io::Error converts via From, and Display carries context.
        let e: FbaError = io::Error::new(io::ErrorKind::NotFound, "no table").into();
        assert!(matches!(e, FbaError::Io(_)));
        assert!(e.to_string().contains("io error preparing fba lookup"));
    }
}
