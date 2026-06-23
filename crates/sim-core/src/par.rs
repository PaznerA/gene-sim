//! Deterministic data-parallelism plumbing (ADR-020 / parallel-sim §8).
//!
//! This module is the S0 scaffold for the parallelization epic: a **persistent global rayon
//! [`ThreadPool`] built exactly ONCE** plus the two knobs every later slice depends on — a
//! bench-tuned [`PAR_THRESHOLD`] and a [`force_serial`] escape hatch for differential debugging.
//!
//! **There are NO call sites at S0** — the three RNG-free, cell-independent hotspots
//! (`metabolism` / `diffuse_and_decay` / `mineralize`) are parallelized in S1–S4. Until then this
//! is built-but-unused on purpose, so the pinned literal `0x47a0_3c8f_6701_f240` is byte-identical
//! (the parallel region does not yet exist).
//!
//! Determinism contract (inv #3, argued in full in `docs/llm/proposals/parallel-sim-draft.md` §3):
//! - rayon's work-stealing schedule + worker count are nondeterministic in **timing** but the
//!   **result must never depend on them** — the parallel region is RNG-free, disjoint-cell, and its
//!   only cross-task reductions are associative-AND-commutative `i64` adds, so any task order yields
//!   the same bytes. The pool here only *runs* closures; it grants no license to make a result
//!   depend on scheduling.
//! - the Bevy `.chain()` schedule stays strictly single-threaded — rayon lives *inside* the heavy
//!   systems, never in the scheduler (we never use Bevy's multi-threaded executor / query
//!   `par_iter`, which would scramble the canonical `(cell, species, org)` order the hash depends on).
//!
//! Worker count is pinned for **stable benches** (correctness does not depend on it): honor
//! `RAYON_NUM_THREADS` when set, else fall back to [`DEFAULT_NUM_THREADS`].

use std::sync::OnceLock;

use rayon::{ThreadPool, ThreadPoolBuilder};

/// Bench-tuned sequential cutoff (parallel-sim §2.3). Below this many work items a heavy system
/// runs its **proven serial loop verbatim** — the rayon fork/join + per-task scratch alloc + the
/// collect Vecs exceed the arithmetic win at low N, and the pinned ~1k-org config (61.7 ms) stays
/// on the serial path, an extra byte-identity guarantee. The exact value is re-tuned in the slice
/// that first wires a call site (S1/S3); ~2000 is the design starting point.
pub const PAR_THRESHOLD: usize = 2000;

/// Default rayon worker count when `RAYON_NUM_THREADS` is unset. Pinned so benches are reproducible
/// run-to-run (correctness is schedule-independent by design; only bench variance cares). Chosen to
/// leave a little headroom on the 12-core reference platform — rayon past the physical core count
/// gives diminishing returns and the 10k case is memory-bandwidth-capped.
pub const DEFAULT_NUM_THREADS: usize = 10;

/// Environment variable that, when set to a truthy value (`1`/`true`/`yes`, case-insensitive),
/// forces every (future) parallel call site onto its serial path — for differential debugging
/// (parallel-sim §8). Read once and cached; correctness must be identical either way, so this only
/// changes *which* code path runs, never the bytes it produces.
pub const NO_PARALLEL_ENV: &str = "GENESIM_NO_PARALLEL";

/// The persistent global pool, built exactly once (NEVER per tick — per-tick thread-creation cost +
/// a nondeterministic worker count).
static POOL: OnceLock<ThreadPool> = OnceLock::new();

/// Cached `--no-parallel` escape-hatch flag (read from [`NO_PARALLEL_ENV`] once).
static FORCE_SERIAL: OnceLock<bool> = OnceLock::new();

/// Resolve the pinned worker count: `RAYON_NUM_THREADS` if a valid positive integer, else
/// [`DEFAULT_NUM_THREADS`]. (rayon also reads `RAYON_NUM_THREADS` for its *default* global pool, but
/// we own a private pool, so we parse it ourselves to keep the fallback explicit and pinned.)
fn resolve_num_threads() -> usize {
    std::env::var("RAYON_NUM_THREADS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_NUM_THREADS)
}

/// Get the process-wide rayon [`ThreadPool`], building it on first use and reusing it forever after.
///
/// Built with a pinned worker count (see [`resolve_num_threads`]) and a stable thread-name prefix.
/// Run parallel work via [`ThreadPool::install`] on the returned pool — `pool.install(|| { … })`.
/// `install` blocks the caller until the closure (and any nested rayon work) completes, so the
/// surrounding single-threaded Bevy system sees an ordinary synchronous call.
#[must_use]
pub fn pool() -> &'static ThreadPool {
    POOL.get_or_init(|| {
        ThreadPoolBuilder::new()
            .num_threads(resolve_num_threads())
            .thread_name(|i| format!("genesim-par-{i}"))
            .build()
            .expect("failed to build the global rayon ThreadPool")
    })
}

/// Whether the `--no-parallel` escape hatch is engaged (cached). When `true`, call sites MUST take
/// their serial path; the result is byte-identical either way (this only aids differential debugging
/// by removing the parallel code path from the equation).
#[must_use]
pub fn force_serial() -> bool {
    *FORCE_SERIAL.get_or_init(|| {
        std::env::var(NO_PARALLEL_ENV)
            .ok()
            .map(|v| {
                let v = v.trim().to_ascii_lowercase();
                matches!(v.as_str(), "1" | "true" | "yes" | "on")
            })
            .unwrap_or(false)
    })
}

/// Run `op` on the global pool, unless the [`force_serial`] escape hatch is engaged, in which case
/// run it on the current thread (rayon's `par_*` adapters then execute serially, since no rayon
/// scope is entered). Either way the closure observes the same inputs and must produce the same
/// bytes — `install` only governs *where* the work runs.
///
/// Unused at S0 (the heavy systems gain call sites in S1–S4); `#[allow(dead_code)]` keeps the
/// built-but-unused scaffold warning-free until then.
#[allow(dead_code)]
pub(crate) fn run<R, OP>(op: OP) -> R
where
    OP: FnOnce() -> R + Send,
    R: Send,
{
    if force_serial() {
        op()
    } else {
        pool().install(op)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rayon::prelude::*;

    #[test]
    fn pool_is_built_once_and_reused() {
        // Same `&'static` pool every call — the persistent-pool invariant (never per-tick rebuild).
        let a = pool() as *const ThreadPool;
        let b = pool() as *const ThreadPool;
        assert_eq!(a, b, "pool() must return the same persistent ThreadPool");
    }

    #[test]
    fn worker_count_is_pinned() {
        // The resolver falls back to the pinned default when RAYON_NUM_THREADS is unset/invalid; the
        // built pool reports a positive worker count. (We don't assert the exact number because the
        // surrounding CI env may legitimately set RAYON_NUM_THREADS for its own reasons.)
        assert!(resolve_num_threads() > 0);
        assert!(pool().current_num_threads() > 0);
    }

    #[test]
    fn par_threshold_is_documented_constant() {
        assert_eq!(PAR_THRESHOLD, 2000);
        const { assert!(DEFAULT_NUM_THREADS > 0) };
    }

    #[test]
    fn run_executes_the_closure_and_is_order_independent() {
        // A disjoint-index parallel write + an associative-commutative i64 reduction — the exact
        // shape every later call site uses — yields the canonical result regardless of scheduling.
        let n = 4096usize;
        let mut out = vec![0i64; n];
        run(|| {
            out.par_iter_mut()
                .enumerate()
                .for_each(|(i, slot)| *slot = i as i64);
        });
        let sum: i64 = run(|| out.par_iter().copied().sum());
        assert_eq!(sum, (0..n as i64).sum::<i64>());
    }

    #[test]
    fn force_serial_reads_env_flag_idempotently() {
        // The cached flag is stable across calls (its value depends on the ambient env at first read;
        // we only assert determinism of the read, not a specific value).
        assert_eq!(force_serial(), force_serial());
    }
}
