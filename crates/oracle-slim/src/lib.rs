//! SLiM subprocess driver (SPEC §8 Stage 2, §W9).
//!
//! **Invariant #1 (STOP THE LINE):** SLiM is GPL-3. This crate invokes the `slim` CLI as a separate
//! subprocess (`std::process::Command`) **only**, and must never link GPL code or depend on any GPL
//! crate. The license gate (`scripts/check_license.sh`) enforces a GPL-free dependency tree.
//!
//! Stage 0 placeholder — no driver yet. The real Eidos-model generation + `slim -seed <derived>` call
//! arrives in Stage 2; until then this crate deliberately depends on nothing.

#![forbid(unsafe_code)]

/// Placeholder marker for the not-yet-implemented driver. Returns the (future) CLI binary name so call
/// sites and tests can already reference the subprocess boundary without linking anything.
#[must_use]
pub fn slim_cli_name() -> &'static str {
    "slim"
}

#[cfg(test)]
mod tests {
    #[test]
    fn subprocess_boundary_is_named() {
        assert_eq!(super::slim_cli_name(), "slim");
    }
}
