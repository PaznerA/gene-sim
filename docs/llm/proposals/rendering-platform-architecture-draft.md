# Rendering & Platform Architecture — ONE deterministic core, MANY read-only renderers

> **Status:** DRAFT vision (not a slice). No UE5/web renderer exists yet — all unbuilt.
> Parallel-safe: touches no code, no gate, no commit. Verified against the tree
> (`crates/godot-sim/src/lib.rs` `LiveSim`, `crates/sim-core/src/{snapshot,lib}.rs`, `ci.yml`).

---

## 1. The principle — one core, many renderers

```
   crates/sim-core + crates/harness        ← std-only, engine-free
   • deterministic tick (single ChaCha8)     depends on NO renderer
   • ALL genotype→phenotype biology (inv #2)
   • hash_world (IEEE-754 + fp-contract=off)
              │  THE CONTRACT (read-only, off-hash, off-SimRng)
   ┌──────────┼───────────────┐
 godot-sim   unreal-sim      web-sim
 (PoC TODAY) (realistic)     (simple iso)
 gdext #[func] C-ABI / IPC   wasm-bindgen
```

The core never depends on any renderer (invariant #4 made structural). Each renderer is a thin
binding over the SAME headless handle — not a fork of biology.

**The read-only boundary contract** — every renderer consumes EXACTLY these, all off `SimRng` and
off `hash_world`:

- **GSS4 snapshot** (`snapshot.rs:98`) — 28-byte LE header + 12 channel-major f32-LE planes
  (density, allele_freq, fitness, soil×3, pool×3, chem×3).
- **observe() / observe_all()** — per-species `{generation, population, allele_freq, phenotype}`.
- **flow_matrix()** — NET joules species→species, i64, row-sum==0 (relations heatmap).
- **species_relations() / region_allele() / edit verdicts / save-load journal** (engine-independent ndjson).

**The contract IS the `LiveSim` `#[func]` surface.** Each future binding is a 1:1 mirror in a
different ABI; only the container type changes. **Invariant #2 firewall (for N renderers):** a
renderer reads pre-projected channels — it may NEVER read a raw genotype and re-derive a phenotype,
and NO quantity may flow from a snapshot back into the core. The snapshot is the one-way valve.

## 2. Determinism-for-portability — the load-bearing correction

The tempting framing — *"the core is pure i64, so wasm/parallel/SIMD determinism is free"* — is
**FALSE here.** `hash_world` folds `f64` traits into the hash via `.to_bits()` (Genotype, DroughtTol,
ThermalTol per OrgId-sorted org at `lib.rs:2513-2515`; `allele_freq` at `:2598`), produced by real
`f64` arithmetic (`mutate_unit` clamp, `mean_genotype` sum-over-sorted). The pinned literal is
`0x64a3_ed4f_7bb1_2779`; CI runs x86_64 + aarch64 with `-fp-contract=off`.

**So the moat is: IEEE-754 round-to-nearest + NO FMA contraction + bounded no-NaN f64 — NOT "no
floats."** Consequences every platform claim must respect:

- **wasm32 is a THIRD float environment** (no FTZ/denormal control; relaxed-SIMD is nondeterministic).
  Its reproduction of the hash must be **CI-PROVEN, not assumed** — a wasm32 gate leg is the deliverable.
- **A parallel f64 fold is BANNED** (f64 add is non-associative): only order-preserving
  `collect`-into-indexed-Vec + sequential fold is permitted on anything feeding `hash_world`.
- The integer parts (J ledger, PoolStock, ChemField i32, FlowMatrix i64) ARE associative and bit-exact
  — those are the safe optimization targets.

## 3. Target renderers (one-line sketches — detail deferred until each is planned)

- **godot (PoC, TODAY)** — gdext `#[func]` cdylib embedding core+harness; MultiMesh organisms +
  per-channel field-texture overlays. The working reference.
- **UE5 (realistic)** — C-ABI cdylib (`crates/unreal-sim`, flat `extern "C"` mirror of `LiveSim`,
  scalars + opaque LE buffers only per the Rust-1.87 wasm32 C-ABI change) and/or an IPC snapshot
  shared-memory stream; Niagara GPU particles for populations + Grid2D for the chem/pool fields.
  *See §UE5 detail (cdylib vs IPC/shm, Niagara/Grid2D/SVT, review discipline) when planned.*
- **web (simple iso)** — either gdext-Emscripten export of the iso frontend (Path A; pin Emscripten
  3.1.74, blocked to ONE cdylib by gdext #968) OR wasm-bindgen over `wasm32-unknown-unknown` core +
  a thin wgpu/canvas renderer reusing GSS4 verbatim (Path B). *See §web detail + the wasm determinism
  proof when planned.*

## 4. Roadmap (headless-first; each its own gated slice)

1. **W0 — PROVE wasm32 determinism (gate leg, NO renderer).** Add a wasm32 leg asserting the pinned
   hash. Highest-value, lowest-risk first step; do BEFORE any web renderer.
2. **W1 — perf: BTreeMap → sorted-Vec hot-path swap.** Hash-neutral (drain order already
   canonicalized); biggest cache win. Then SIMD-normalize the off-hash snapshot pack.
3. **U0/U1 — UE5 binding spike then renderer** (IPC pattern preferred; `check_unreal_snapshot` parity gate).
4. **X0/X1 — web path decision + renderer** (after W0 green).
5. **Perf levers, hold until needed:** rayon parallel-MAP (collect-into-indexed-Vec, f64 fold stays
   serial; ONLY if grid>64² or N>50k and single-thread misses 60fps); split renderer to a separate
   process behind an mmap SPSC ring (strengthens inv #2).

**ADR-019 (draft, append when a phase lands):** formalize the core-as-library contract; UE5 binds via
C-ABI/IPC, web via gdext-Emscripten or wasm-bindgen; the determinism correction above is load-bearing;
the CI matrix grows to x86_64 + aarch64 + wasm32.

## 5. Bottom line

The "one deterministic core → many read-only renderers" architecture is SOUND: inv #2 holds by
construction (renderers are structural mirrors of the `LiveSim` `#[func]`s), inv #3 by every pattern
(snapshot off-hash, integer cadence, GPU/IPC non-determinism never re-enters the hash). But it must
NOT rest on the false "pure i64" premise — the moat is IEEE-754 + no-FMA + bounded no-NaN f64, which
is why wasm needs its OWN CI proof (W0) first, a parallel f64 fold is banned, and the C-ABI must cross
scalars + opaque LE buffers only.
