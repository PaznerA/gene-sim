# SNIPPETS — reusable patterns & gotchas

> Patterns proven in a slice; copy them instead of reinventing. Each is tied to an invariant or gate.

## Determinism (invariant #3) — the load-bearing pattern

**Seeded RNG as a single threaded resource.** One `ChaCha8Rng` per run, stored as a Bevy resource, advanced
only through explicit access. Never `rand::thread_rng()`, never a per-system RNG.

```rust
use bevy_ecs::prelude::*;
use rand_chacha::ChaCha8Rng;
use rand::SeedableRng;

#[derive(Resource)]
pub struct SimRng(pub ChaCha8Rng);

// Master seed → sub-seed: derive, don't reuse. (Same scheme for SLiM's -seed in Stage 2.)
pub fn derive_seed(master: u64, stream: u64) -> u64 {
    // splitmix64 step — deterministic, well-distributed, no external state.
    let mut z = master.wrapping_add(0x9E37_79B9_7F4A_7C15).wrapping_mul(stream | 1);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
```

**Fixed system ordering, single-threaded sim schedule.** Don't rely on Bevy's parallel scheduler for sim
logic — order systems explicitly with `.chain()` so execution is reproducible.

```rust
schedule.add_systems((system_a, system_b, system_c).chain());
```

**Never iterate a `HashMap` to produce state or a hash.** Use `Vec` in stable order, sorted keys, or `IndexMap`.
A `HashMap` is fine as a *lookup cache* as long as iteration order never affects output.

**Stable end-of-run hash.** Hash ordered fields with a fixed hasher; print it for `--hash-only`.

```rust
use std::hash::{Hash, Hasher};
fn stats_hash(values: &[u64]) -> u64 {     // values gathered in a deterministic order
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for v in values { v.hash(&mut h); }     // order is fixed by construction, not by HashMap iteration
    h.finish()
}
```
> Gotcha: `DefaultHasher` is stable within a build/platform — exactly the PoC determinism scope (SPEC §6).
> If we ever need cross-run stability independent of std, switch to an explicit algorithm (e.g. FNV/xxHash) — record it in an ADR.

## GPL boundary (invariant #1) — subprocess only

`crates/oracle-slim` must carry **zero** GPL dependencies and only ever shell out:

```rust
use std::process::Command;
let status = Command::new("slim")            // resolved from PATH / a pinned install
    .args(["-seed", &derived.to_string(), "-d", "param=1", "model.slim"])
    .status()?;
```
Verify with `cargo tree -p oracle-slim` (no GPL crate) — the license gate (`scripts/check_license.sh`, §10.8)
enforces this. Same pattern for Crisflash (off-target oracle, Stage 2+).

## Genome-in-core (invariant #2)
Genotype→phenotype lives in `crates/genome` / `crates/sim-core` only. `godot/` reads bincode snapshots and
computes no biology. If a GDScript file needs a trait value, the value must already be in the snapshot.

## Pluggable science (invariant #5)
On/off-target scores are traits (`OnTargetScore`/`OffTargetScore`). sim-core depends on the trait, never a
concrete impl — swapping the in-core default for a subprocess-backed one touches no sim-core logic.

## The gates (SPEC §10) — run before every commit
One command runs them all (PASS/FAIL per item, hard exit on red — see docs/llm/LOOP.md §4):
```bash
tools/gate.sh                 # fmt · clippy -D warnings · test · determinism · proptest · (bench skip) · license
GATE_BENCH=1 tools/gate.sh    # + criterion perf bench (slow) — at stage exits (§11)
```
HARD gates (never skip): determinism (`tools/check_determinism.sh`) and license (`scripts/check_license.sh`).

## Gotchas
- **Cargo.lock is committed** (reproducibility, inv. #7). Don't gitignore it.
- **Source the Rust env in fresh shells**: `. "$HOME/.cargo/env"` (the harness shell doesn't persist PATH).
- **SLiM reproducibility is version-scoped** (SPEC §12) — pin the tag; same seed reproduces only within one SLiM version.
- **Apple deprecated OpenCL** → off-target oracle is Crisflash (CPU), not Cas-OFFinder (ADR-001).
- **Bevy `App::update()` vs a manual `Schedule`**: for a headless fixed-N-generations loop, driving a
  `Schedule` over a `World` in a plain loop is simpler and more obviously deterministic than the full `App` runner.
