# Rendering & Platform Architecture — ONE deterministic core, MANY read-only renderers

> **Status:** DRAFT proposal (not a slice). Parallel-safe — touches no code, no gate, no commit.
> **Scope:** formalize the core-as-library boundary; design the UE5 (realistic) and web (simple iso)
> renderers as read-only consumers of the SAME contract the godot PoC already consumes; lay out the
> performance + determinism roadmap and a paste-ready TASKS.md block.
> **Verified against the tree:** `crates/godot-sim/src/lib.rs` (the `LiveSim` boundary),
> `crates/sim-core/src/snapshot.rs` (GSS4), `crates/sim-core/src/lib.rs` (`hash_world`, `mutate_unit`,
> `mean_genotype`), `.github/workflows/ci.yml` (the multi-ISA determinism gate), `CLAUDE.md` (invariants).

---

## 0. The correction that anchors everything (read this first)

A tempting framing — and the one the source research repeatedly used — is *"the core is a pure i64
fixed-point pipeline that touches no float, therefore wasm/parallel/SIMD determinism comes for free."*
**That premise is FALSE in this codebase, and the difference is load-bearing.** The real determinism
foundation is narrower and must be stated correctly before any platform conclusion rests on it.

What the code actually does (verified):

- **`hash_world` folds `f64` traits into the determinism hash via `.to_bits()`.**
  `crates/sim-core/src/lib.rs:2513-2515` hashes `Genotype(f64).to_bits()`, `DroughtTol(f64).to_bits()`,
  `ThermalTol(f64).to_bits()` per organism (OrgId-sorted), and `:2598` hashes
  `allele_freq.to_bits()`. The hash is **not** integers-only.
- **Those `f64`s come from real float arithmetic.** `mutate_unit` (`lib.rs:1507-1515`) computes
  `(value + delta).clamp(0.0, 1.0)`; `mean_genotype` (`lib.rs:1555-1567`) does an `f64` `sum / len`
  over OrgId-sorted rows. Cross-platform bit-stability rests on `f64` add/clamp/sum being
  **bit-identical across ISAs**, not on integer associativity.
- **CI is explicit about why.** `.github/workflows/ci.yml:68` pins
  `RUSTFLAGS: "-C target-cpu=generic -C llvm-args=-fp-contract=off"` and runs **x86_64 + aarch64**
  legs (`ci.yml:54-75`) precisely because *"the f64 / fp-contraction divergence only manifests at
  RUNTIME on the real ISA"* (`ci.yml:46`). The pinned literal is `0x64a3_ed4f_7bb1_2779`
  (`lib.rs:2653`).

**Therefore the determinism moat is: "IEEE-754 round-to-nearest + NO FMA contraction + bounded
no-NaN f64", NOT "no floats."** Every downstream claim ("parallelism is free because integers are
associative", "wasm is *more* guaranteed than native") must be re-derived against this real hazard.
The good news: the architecture's *conclusions* survive the correction (one core → many read-only
renderers is sound and determinism-safe). But the wasm guarantee in particular becomes a thing to
**PROVE in CI**, not a property to assume — see §3 and §5.

The integer parts (Energy/Biomass J ledger, PoolStock, ChemField i32 planes, FlowMatrix i64) ARE
associative and bit-exact, and that genuinely makes the integer-heavy hot path safe to optimize. But
the `f64` trait path threads through `hash_world`, and it is the part every platform argument must
respect.

---

## 1. THE PRINCIPLE — the core-as-library contract

### 1.1 One core, many renderers

```
                         ┌─────────────────────────────────────────────┐
                         │  crates/sim-core + crates/harness            │
                         │  • the deterministic tick (single ChaCha8)   │
                         │  • ALL genotype→phenotype biology (inv #2)    │
                         │  • hash_world (IEEE-754 + fp-contract=off)    │
                         │  • depends on NO renderer, NO engine          │
                         └───────────────┬─────────────────────────────┘
                                         │  THE CONTRACT (read-only, off-hash)
          ┌──────────────────────────────┼──────────────────────────────┐
          │                              │                              │
   ┌──────┴───────┐              ┌───────┴────────┐             ┌───────┴────────┐
   │ godot-sim    │              │ unreal-sim     │             │ web-sim        │
   │ (PoC, TODAY) │              │ (realistic)    │             │ (simple iso)   │
   │ gdext #[func]│              │ C-ABI / IPC    │             │ wasm-bindgen   │
   │ Godot 4.6    │              │ UE5 plugin     │             │ + thin canvas  │
   └──────────────┘              └────────────────┘             └────────────────┘
        read-only                    read-only                      read-only
```

**The core never depends on any renderer.** This is invariant #4 (headless-first) made structural:
`sim-core` + `harness` are `std`-only and engine-free; `godot-sim` is a thin `cdylib` binding that
*embeds* them. The two future renderers are additional thin bindings over the SAME headless handle —
not forks of biology.

### 1.2 What "the contract" is, concretely

Every renderer — the godot PoC today, UE5 and web tomorrow — consumes EXACTLY these, and nothing
else. All are **off the SimRng stream and off `hash_world`** (verified: `snapshot.rs` module docs;
the `#[func]`s in `godot-sim/src/lib.rs` draw no RNG and mutate nothing):

| Contract surface | Producer (core) | Renderer reads | Bytes/shape |
|---|---|---|---|
| **GSS4 snapshot** | `GridSnapshot::write_snapshot_bytes` (`snapshot.rs:98`) | per-cell data textures | 28-byte LE header (`"GSS4"`, w, h, channel_count=12, u64 gen, u32 pop) + 12 channel-major `f32`-LE planes (density, allele_freq, fitness, soil×3, pool×3, chem×3) |
| **Observation** | `observe()` / `observe_all()` | vitals / specimen view | `{generation, population, allele_freq, phenotype{trait:value}}` per species |
| **FlowMatrix** | `flow_matrix()` (`lib.rs:308`) | relations heatmap | `{s: u32, j: flat row-major i64}` — NET joules species→species, row-sum==0 |
| **species_relations** | `species_relations()` (`lib.rs:340`) | guild/nearest overlay | `{s, guild_of: i32[], nearest: {focal → [sid,dist,…] i32}}` (computed in the off-hash `relations-index` crate) |
| **region_allele** | `region_allele()` (`lib.rs:411`) | mission/zone read | `{mean: f32, populated: i32}` |
| **edit verdicts** | `apply_edit{,_region}()` | intervention feedback | `{applied, detail, generation[, covered]}` |
| **save/load** | `save_session`/`load_session` (journal: seed + ordered Actions) | persistence/replay | engine-independent ndjson |

**The contract is the `LiveSim` `#[func]` surface.** Each future binding is a 1:1 mirror in a
different ABI (C-ABI for UE5, wasm-bindgen for web). The marshalling shape is identical; only the
container type changes (`VarDictionary`/`PackedArray` → C struct + opaque LE buffer → JS Object +
typed array).

### 1.3 The invariant #2 firewall, restated for N renderers

**Biology is written ONLY in `sim-core`/`genome`/`crispr`. A renderer may not compute
genotype→phenotype — ever, in any language.** Today GDScript is reviewed for this. The discipline
generalizes verbatim:

- A renderer reads the **pre-projected** GSS4 channels and `observe*()` dictionaries. It must NEVER
  read a raw genotype and re-derive a phenotype (e.g. recompute a color from a raw allele).
- **Presentation math is fine** (color-mapping a channel, LOD blending, interpolating between two
  integer frames for smooth display, instancing). The rule is: *no quantity that feeds back into sim
  state may originate in the renderer.* The one-way valve is the snapshot.
- **Never fold a snapshot `f32` back into the core.** GSS4 carries off-hash `f32` LE bytes
  deliberately; a renderer mis-parse is cosmetic, never a hash threat — UNLESS someone routes a
  snapshot value back into a sim input. Don't.

This firewall is structural for the godot PoC (every `#[func]` is thin marshalling over
`harness::GeneSimEnv`). The C-ABI and wasm-bindgen mirrors preserve it **by construction** because
they expose the identical projection functions. The new risk is human, not structural: C++/Blueprint
(UE5) makes it *easier* than GDScript to accidentally re-derive biology renderer-side — so the UE5
plugin gets the SAME code-review discipline as GDScript (§2.5).

---

## 2. UNREAL ENGINE 5 — the realistic renderer

### 2.1 Integration: two complementary patterns (use both)

**Pattern A — C-ABI `cdylib` + a thin hand-rolled UE5 C++ plugin (the direct analog of `godot-sim`).**
Add `crates/unreal-sim` compiled `crate-type = ["cdylib"]`, exporting a flat `extern "C"` surface that
is the **C-ABI mirror of the `LiveSim` `#[func]`s**:

```
gs_reset(seed: u64) -> Handle            gs_observe(h, *out_json_ptr, *out_len)
gs_step(h, n: i64)                        gs_observe_species(h, *out_ptr, *out_len)
gs_snapshot(h, w, h_, *out_ptr, *out_len) gs_flow_matrix(h, *out_s, *out_ptr, *out_len)
gs_apply_edit(h, cas, target, *guide, …)  gs_set_species_json(h, *json_ptr, len) -> bool
gs_save_session(h, *dir) / gs_load_session(h, *dir)
```

A `cbindgen`-generated C header + a `.uplugin` third-party module with a `Build.cs` that references
the staged `.dll/.so/.dylib`; the plugin `dlopen`s the lib and dispatches through a function-pointer
table. **Verified pattern:** both live UE5↔Rust projects (`unreal-rust`'s `unreal-ffi` layer and
`Uika`) do exactly this — *"compile Rust to a cdylib loaded by a small UE C++ plugin; all calls cross
through a function-pointer table, no C++ recompile during Rust iteration."*

> **CAVEAT (verified, 2026):** `unreal-rust` and `Uika` are both *"not production-ready, APIs change
> without notice"*, and Uika's maintainer paused work noting Verse/UE6 may supersede Rust gameplay.
> **Do NOT adopt either framework wholesale.** Hand-roll a minimal flat-C plugin — the surface is ~12
> functions and we already own the equivalent in `godot-sim`.

> **CAVEAT (verified, Rust 1.87, May 2025):** the wasm32 C-ABI changed (aggregates passed indirectly).
> The flat `extern "C"` surface must cross **only scalars + opaque LE byte buffers** (mirror GSS4's
> `PackedByteArray → *const u8 + len`), never a `#[repr(C)]` aggregate by value. This is also what
> keeps the UE5 and web C-ABIs identical. Positions for Large-World-Coordinates cross as `f64`/UE
> `Position` to avoid precision-driven *visual* drift (cosmetic, off-hash).

**Pattern B — snapshot IPC / shared-memory stream (recommended for the heavy render path).**
Keep `harness` as a **separate headless process** and stream GSS4 frames over shared memory. This is
the recommended primary pattern for UE5 because it buys three invariants at once:

- **inv #1 (GPL boundary)** — a process split keeps any future SLiM/oracle subprocess cleanly at the
  OS boundary.
- **inv #2 (read-only)** — the renderer process can only *map and read* the snapshot ring; it
  physically cannot call into sim mutation.
- **inv #3 (cadence)** — the integer core runs at its own `step(n: i64)` integer cadence, fully
  decoupled from the 60 fps render thread (exactly the "decoupled live loop" commit `d45238c`
  already enforces). Wall-clock never leaks into the tick.

**Verified UE5 transports:** the first-party **SharedMemoryMedia** plugin (UE 5.3+, cross-GPU shared
textures, frame-locked) and **Live Link** (the canonical "stream + consume external data each frame,
extensible via plugins" producer/consumer interface — designed for exactly this split); third-party
**ObjectDeliverer** adds TCP/UDP/WebSocket/SharedMemory. Pattern: producer writes the 12-channel GSS4
grid + a per-organism position/attribute buffer into a **double-buffered shm ring** (the same
double-buffer `ChemField` already uses, `chem.rs:150`); the UE plugin maps it and uploads to a Niagara
position array + a Grid2D each frame. The 2026 *SplatBus* paper documents the GPU-IPC variant
(zero-copy large per-frame datasets via CUDA/GPU IPC) for the millions-of-cells ceiling.

**Pattern C — offline volume export (for cinematic/replay only):** bake snapshot history → OpenVDB →
UE5 Sparse Volume Texture (the 2025 volume-survey pipeline). Read-only, deterministic-replay-friendly,
not live.

### 2.2 Realistic rendering design (the microbial joule-economy, by data layer)

**POPULATIONS (per-organism) → Niagara GPU particles.**
- PoC scale (~1k–10k orgs): the simple recipe — a Niagara **"Position Array" user parameter** +
  `UNiagaraDataInterfaceArrayFunctionLibrary::SetNiagaraArrayPosition` (MUST be **Position**, not
  Vector, for Large-World-Coordinates), "Direct Set → Select Position from Array → Particle ID Index"
  in Initialize Particle, **Persistent IDs ON**, Spawn Burst count = an int32 user param.
  *Gotcha (verified):* particle count is fixed at spawn, so a population change requires resetting the
  system.
- Ecosystem scale (10^5–10^6): a **custom Niagara Data Interface** (`UNiagaraDataInterface` subclass)
  uploading the per-organism position+attribute buffer to GPU buffers via an NDI proxy + injected HLSL
  (`GetFunctionHLSL`/`SetShaderParameters`) — Magnopus's Gaussian-splat viewer renders **~2M GPU
  particles** this way ("20× the CPU cap of 100K").
- Per-particle color/size/glyph driven by **already-exported** attributes (allele_freq, energy,
  species role/key) — sprite far, Niagara mesh renderer (instanced static mesh) mid, detail mesh near.
- Colony clusters can alternatively use ISM/HISM, but *verified caveat:* dynamic Add/Remove Instance
  is slow >10K in UE 5.4/5.5, so a churning population favors Niagara spawn/kill over ISM rebuilds.

**POOL & CHEM FIELDS (the 12 GSS4 channels) → Niagara Grid2D Collection (rich) or runtime textures
(simple).**
- **Grid2D route (the F5 diffusion fields' native home):** a Grid2D Collection driven by Simulation
  Stages (iteration source = the grid, runs independent of particle count), `Grid2D.SetFloatValue` to
  upload the 12 channels, double-buffered read/write attrs. The reaction-diffusion / Gray-Scott
  Grid2D + Laplacian-over-neighbors workflow is the *same shape* as the conserved 4-neighbour chem
  diffusion (toxin/kin/alarm), so they render as smooth volumetric heatmaps. **The core still computes
  the authoritative integer diffusion; UE only re-displays (or optionally interpolates between integer
  frames — presentation only).**
- **Texture route (simple):** upload each channel's `w×h f32` plane as a render target → translucent
  data-layer plane material. This is the UE port of the Godot "selectable data-layer overlay".

**MULTI-SCALE ZOOM (cell → colony → ecosystem) → an LOD ladder keyed on camera distance, NOT new
biology.** The snapshot `resample` already supports it (`snapshot(w,h)` resamples real Positions onto
an arbitrary render grid, `snapshot.rs:14-17`):
- **far / ecosystem** = coarse-grid field heatmap planes + density particles (cheap, GSS4 is already a
  downsampled projection);
- **mid / colony** = Niagara mesh particles / ISM colonies;
- **near / specimen** = a detailed cell mesh with a volumetric/translucent material for cytoplasm,
  optionally a **Heterogeneous Volume** (Sparse Volume Texture) for a true volumetric interior.
  *Verified survey finding:* HV is the most promising large-volume path (page-table sparse tiles,
  32k³-class, proper lighting), beating Niagara Fluids (heavy VRAM) and TBRayMarcher (chunk-border
  artifacts) — BUT **SVT has no first-party runtime-streaming-from-live-sim path yet**, so reserve
  SVT/HV for **baked replay or the static specimen scope**; do live volumetric chem clouds with
  Grid2D/3D-texture + a volumetric material.

**FlowMatrix → animated flux lines** between species guilds (the relations view), driven by the
already-exported `flow_matrix()` i64 buffer.

### 2.3 Reuse — what UE5 inherits with ZERO core change

The headless core is engine-agnostic by design (inv #4), so the UE5 port is *almost entirely* a new
thin binding + a new renderer:

- **REUSED AS-IS:** `sim-core` + `harness` (the whole determinism pipeline, still multi-ISA gated);
  the GSS4 byte format (`snapshot.rs` — the same LE buffer that crosses into `godot/snapshot.gd`);
  `flow_matrix()`, `observe*()`, `species_signatures()`, `species_relations()`; the journal save/load;
  the species-JSON path (`harness::species::build_species_from_str`).
- **NEW (the only work):** (a) `crates/unreal-sim` cdylib (a near-mechanical translation of
  `godot-sim/src/lib.rs`'s `#[func]`s to flat `extern "C"` returning POD + opaque buffers + cbindgen
  header); (b) a minimal UE5 C++ plugin (`.uplugin`+`Build.cs`) exposing a `UGeneSimSubsystem` /
  `ALiveSimActor` mirroring `LiveSim`; (c) the UE renderer (Niagara + Grid2D + relations widget);
  (d) a `check_unreal_snapshot` channel-count assert mirroring the existing `check_godot_snapshot`, so
  a GSS magic/channel bump fails loudly on BOTH renderers.

**Recommendation:** keep `godot-sim` AND `unreal-sim` as two thin bindings over the one core — this
*validates* that the boundary is genuinely engine-agnostic — and prefer Pattern B (IPC) for UE so
inv #1 and the decoupled cadence both come for free.

### 2.4 Determinism impact (UE5)

UNTHREATENED and arguably better-isolated. The snapshot is off-hash/read-only; the IPC split actively
*helps* (integer cadence can never absorb 60 fps wall-clock); GPU rendering is non-deterministic
across vendors but **never re-enters the hash** — only the CPU pipeline is hashed. The multi-ISA gate
is **unaffected** because the core is unchanged; UE5 just adds Windows-x64/arm64-mac *render* targets
consuming identical bytes. The one new concern is **f32 endianness in the C-ABI buffer** — keep GSS4's
explicit little-endian contract and the bytes are identical to the Godot path.

### 2.5 Review discipline (the one human risk)

C++/Blueprint makes accidental biology re-derivation easier than GDScript. **Mitigation:** the
`unreal-sim` plugin gets the SAME reviewer pass GDScript gets — assert it consumes ONLY pre-projected
GSS4 channels + `observe*()`/`flow_matrix()` exports, never a raw genotype. This is a structural
mirror of today's `godot-sim` review; flag it explicitly in the reviewer agent's checklist.

---

## 3. WEB build — the simple iso / top-down version

### 3.1 Three paths, ranked by determinism risk × reuse

**PATH A — gdext WASM export of the EXISTING iso frontend + the `godot-sim` cdylib (highest reuse,
experimental).** Reuses the iso scenes + `snapshot.gd` + the `LiveSim` surface verbatim. Verified
recipe (2025/26):

- Target **`wasm32-unknown-emscripten`** (NOT `-unknown-unknown` — Godot's web runtime is
  Emscripten-linked); nightly + `-Zbuild-std`; **Emscripten 3.1.74** for Godot 4.3+.
- gdext features `["api-custom","experimental-wasm","lazy-function-tables"]`; rustflags
  `-C link-args=-sSIDE_MODULE=2`, `-Zdefault-visibility=hidden`, `-Zlink-native-libraries=no`,
  `-Zemscripten-wasm-eh=false` (load-bearing: without it → "tag import requires a WebAssembly.Tag").
- `.gdextension` gains `web.{debug,release}.wasm32` lib rows.
- Godot web is **Compatibility/WebGL2 only** (Forward+/Mobile need Vulkan; WebGPU not yet wired).

> **VERIFIED BLOCKERS (belong in inv #7 DECISIONS):**
> - gdext web export is flagged **"experimental and should be understood as such"** (godot-rust book).
> - **gdext #968 (still OPEN):** MULTIPLE Rust GDExtensions in one web export panic at
>   `BindingStorage::initialize` / OOM. **This repo ships ONE `godot-sim` cdylib, so #968 is satisfied
>   TODAY — but adding ANY second gdext lib (e.g. a separate relations cdylib) would break the web
>   build.** This is a hard constraint on future crate-splitting.
> - Pins the experimental **Emscripten 3.1.74** toolchain → inv #7.

**PATH B — `wasm32-unknown-unknown` core + a thin custom web renderer (lowest determinism risk, most
renderer rewrite).** Compile `harness`+`sim-core` (std-only, no Godot) to `wasm32-unknown-unknown`
behind a **wasm-bindgen** shim exposing the SAME logical surface. The GSS4 buffer crosses to JS as a
`Uint8Array` (zero-copy view into wasm linear memory) — JS parses the **identical** 28-byte header +
12 `f32` planes that `godot/snapshot.gd` parses today, format reused unchanged. Renderer = canvas2D
for a quick iso/top-down, or `wgpu`/WebGL2 for scale. wasm-bindgen sidesteps the Rust-1.87 wasm32
C-ABI change (only hand-rolled `extern "C"` is affected).

**PATH C — recommended SEQUENCING (hybrid):** do **Path B's deterministic-core-to-wasm PROOF FIRST**
(it is the load-bearing, low-risk half and extends the gate — §3.2), THEN decide between Path A
(reuse the Godot iso frontend, accept experimental Emscripten + WebGL2) and a bespoke thin renderer
for the "simple iso/top-down" web build. **This matches the headless-first discipline (inv #4): prove
the core reproduces on wasm BEFORE any web renderer touches it.**

### 3.2 Determinism on wasm32 — UNCERTAIN, must be CI-PROVEN (do NOT assume)

**Correcting the research:** it claimed wasm makes determinism *"MORE guaranteed than native"*
because the core is "integer-only." That is **wrong** (see §0): `hash_world` folds real `f64` traits,
so wasm reproduction rests on the SAME "IEEE-754 + no-FMA + no-NaN" moat as native — wasm is a
**third, unproven float environment**, not a free guarantee.

What is genuinely true (web-verified):

- **GOOD:** the WebAssembly spec (Numerics; design/Nondeterminism.md) makes all float results
  deterministic, round-to-nearest-ties-to-even, and **forbids contraction/fusion that would elide
  intermediate rounding** — i.e. wasm enforces the same `-fp-contract=off` discipline this repo
  already pins natively. The ONLY enumerated float nondeterminism is **NaN bit-patterns** (plus
  relaxed-SIMD and threads).
- **This code avoids the NaN hazard:** the `f64` path is bounded to `[0,1]` by `clamp` (`mutate_unit`,
  `lib.rs:1514`), has no div-by-zero / `0*inf` / transcendental, and `mean_genotype` guards the
  empty-population case (`lib.rs:1561`) — so it should never produce a NaN. Reproduction is therefore
  **PLAUSIBLE**.
- **NEW risks the research missed (web-verified, why it stays UNPROVEN):**
  1. **No flush-to-zero / no denormal control on wasm** (users.rust-lang.org #112200, WebAssembly
     design #148) — wasm always keeps full subnormals; native x86/aarch64 defaults also keep them, so
     it *likely* agrees, but it is a DIFFERENT float environment than the multi-ISA gate has proven.
  2. **rust-lang/rust #117597 (OPEN):** wasm32 `.wasm` bytes differing by build host — about *build*
     reproducibility, not proven runtime float divergence, but open/unresolved, and it flatly
     contradicts "wasm makes determinism more guaranteed."
  3. **Relaxed-SIMD lowerings are explicitly nondeterministic** (design/Nondeterminism.md). Any future
     `f64` fold that autovectorizes under a relaxed-SIMD target-feature would diverge.

**THE GATE TO ADD (this is the deliverable, not a claim):** extend the multi-ISA gate with a
**wasm32 leg** — compile `sim-core`+`harness` to `wasm32-unknown-unknown`, **conservative
target-feature (NO relaxed-simd)**, run the pinned-hash test under **wasmtime** (wasm32-wasip1) or
**wasm-bindgen-test** (Node / headless browser), and assert `== 0x64a3_ed4f_7bb1_2779`. This converts
"does wasm reproduce the hash" from a claim into a CI invariant. It may pass first try (the float ops
are tame); it may need fixes. **It is a gate to PROVE, not a property to assume — and it lands BEFORE
any web renderer.**

### 3.3 Web rendering design

- **Populations** → WebGL2 core instancing (`drawArraysInstanced` + attribute divisors): one organism
  = one instance, position/genotype/energy from the snapshot; single draw call for the whole
  population. In Path A this is Godot **MultiMesh**; in Path B `wgpu` (one API → WebGL2 today, WebGPU
  when stable; `downlevel_webgl2_defaults()` as the portable floor) or hand-rolled WebGL2.
- **Pool/chem fields** → one float **data texture per channel** (R32F), sampled with `texelFetch`,
  colormapped in a shader — the existing data-layer-overlay model 1:1 onto bound textures.
- **Scale ceiling (verified):** WebGL2 instancing + transform-feedback/float-texture GPGPU comfortably
  draws hundreds-of-thousands of points; WebGL2 has **NO portable compute** (the Compute extension
  never shipped), so GPGPU there is ping-pong textures. True millions / GPU-side chem advection → the
  WebGPU compute path (~1M interactive particles @60fps demonstrated). `wgpu` is the pragmatic choice
  (WebGPU when available, WebGL2 fallback).
- **Multi-scale zoom** = the same LOD ladder as UE5, driven by the `snapshot(w,h)` resample — all
  scopes read the same off-hash projection, so zoom never perturbs the sim (inv #2).

### 3.4 Trade-offs (web)

| | Path A (gdext Emscripten) | Path B (wasm-bindgen + thin renderer) |
|---|---|---|
| Reuse | iso scenes + snapshot.gd + LiveSim verbatim | core + GSS4 format only; renderer rewritten |
| Determinism risk | core unchanged; same wasm proof needed | core unchanged; same wasm proof needed |
| Toolchain | experimental Emscripten 3.1.74 + nightly + `-Zbuild-std` | stable-ish wasm-bindgen; no Emscripten |
| Blockers | gdext #968 (single-cdylib only), experimental-web, WebGL2-only | none structural; renderer is greenfield |
| Renderer | Godot WebGL2 Compatibility (MultiMesh/ImageTexture) | wgpu/WebGL2 or canvas2D |
| Best for | shipping the existing iso UI to browser fast | a lean, controllable web demo + the gate proof |

---

## 4. PERFORMANCE roadmap

Bench against the standing **post-F5 baseline** (the perf-optimize skill's reference;
`61.7 / 295.4 / 590.8 ms` for the three sizes). Each item below is its own gated slice with a hash
check.

### 4.1 Deterministic parallelism — collect-into-indexed-Vec, NEVER par-reduce

**Re-derived against the REAL f64 hazard (§0):** the research banned rayon `reduce`/`sum` (unspecified
merge order) — correct — but justified it with "values are integers so associative", which is wrong
for the `f64` trait paths. The accurate statement:

- `mean_genotype` (`lib.rs:1565`) does an **`f64` `sum`** that IS order-sensitive (`f64` add is
  non-associative). It is safe TODAY *only because it iterates OrgId-sorted* (`lib.rs:1564`). **Any
  "parallelize the fold" optimization on this f64-summing pass would break the hash.**
- The integer ledger/pool/chem/flow folds ARE associative and bit-exact — those are the genuinely safe
  targets.

**The safe pattern (verified, rayon #210 + the `IndexedParallelIterator` contract):** parallelize the
**embarrassingly-parallel per-cell/per-org MAP** (Monod uptake, `split_budget` convert, maintenance
debit) with `par_iter().collect::<Vec<_>>()` into an **index-stable Vec** (`collect` is
order-preserving by construction → bit-identical to the serial Vec regardless of thread count), then
do the **sequential** `sort_unstable_by_key((cell,species,org))` + fold + `hash_world` on the main
thread. The accumulation maps (`by_org`, `maint_energy`, …) stay sequential or merge in fixed
`(cell,species,org)` order. **rayon's native `reduce`/`sum`/`par_extend`-into-map must NOT touch
anything feeding `hash_world`.**

**Verdict for current scale (32×32, ≤10k orgs):** stay single-threaded — thread spawn/join overhead
dwarfs a sub-ms tick. Hold parallel-MAP as the lever for when a single 60 fps tick can't hold at the
larger grids (§4.5), and land it as its own slice with a multi-ISA hash check.

### 4.2 The deferred BTreeMap → sorted-Vec win (highest-value, lowest-risk)

Flagged in DECISIONS/TASKS (F1) and backed by the research: sorted Vec has excellent cache locality;
"sorting once is cheaper than a tree if you insert in bulk and query rarely" — exactly these per-tick
maps (built once, drained once in canonical order). The codebase **already sorts the drained rows**
(`lib.rs:1057/1076` etc.), so the BTreeMap's ordering is redundant work. There are **9 `BTreeMap`
usages** in `sim-core/src/lib.rs` alone. Replace `BTreeMap<u64,T>` → `Vec<(u64,T)>` +
`sort_unstable_by_key` + dedup-merge (or a dense-indexed Vec). **Hash-NEUTRAL** because drain order is
already canonicalized — making it the single best first perf slice. Quarantine to its own slice (any
mis-step moves the hash).

### 4.3 SIMD on the render/snapshot path ONLY (off-hash)

State of stable Rust SIMD (verified, 2025): `std::simd` is nightly-only "for the foreseeable future";
the compiler "won't autovectorize anything involving floats." Use `wide` (mature; NEON+x86+wasm) or
`pulp` (multiversioning) on stable. Integer SIMD (u8/u16/i32) DOES autovectorize.

- **`write_snapshot_bytes`** (`snapshot.rs:98`) does 12 channels × cells of i32→f32-normalize→
  `to_le_bytes` — the f32 conversion the compiler won't autovectorize, BUT it is **READ-ONLY and
  OFF-HASH** (`snapshot.rs:6-9`). SIMD here is **invariant-safe by construction**; `wide`/`pulp` give
  4–8 f32 lanes for the normalize+pack. This lets the snapshot run every frame if ever needed.
- **The chem `diffuse_and_decay` i32 4-neighbour stencil IS in `hash_world`** (the chem planes are
  folded, `lib.rs:2580`). Integer SIMD is bit-exact (unlike float), so SIMD there is *permissible* IF
  lane order matches the exact integer sequence — but it MUST be validated by the multi-ISA gate
  (x86 AVX2 vs aarch64 NEON) and is a deliberate re-pin if any value shifts. Lower priority than the
  off-hash snapshot SIMD.

### 4.4 Snapshot throughput for 60 fps

Today's path is in-process FFI (`godot-sim` `#[func] snapshot → PackedByteArray`). Verified gdext FFI
overhead is **~9 ns per cached call**, so the per-call cost is negligible; the cost is the **byte
copy** of GSS4 = 12 ch × w×h × 4B ≈ **196 KB at 64×64** → ~12 MB/s at 60 fps, trivial. **Rebuild the
snapshot only when the generation advances, not per render frame** (decouple snapshot cadence from
frame cadence). If ever split to a separate process (UE Pattern B / web), the canonical transport is
an **mmap'd lock-free SPSC ring buffer** (~102 ns small-message latency, RAM-bandwidth-bound,
128-byte cache-line padding to avoid false sharing — KREN/ipmpsc/fdringbuf), double-buffered so the
renderer reads frame N while the sim writes N+1.

### 4.5 Larger worlds / entity ceilings + bench targets

Current: 32×32 (`RESOURCE_DIMS`, `WORLD_DIMS == RESOURCE_DIMS` asserted at reset) at ~0.85M
org-updates/s. The ABM literature (EpiRust → 100M agents; AgentScope 1M in 12 min; spatial-partition
wins at 50k) shows the cell-grid architecture scales orders of magnitude further. PoC bumps:
**32 → 64 → 128 grid; N → 50–100k**, gated by the per-tick wall-time budget.

**Ordered roadmap (lowest-risk first; each its own gated slice, re-bench vs post-F5 baseline):**

1. **BTreeMap → sorted-Vec** (§4.2) — hash-neutral, biggest cache win, no rendering change.
2. **SIMD-normalize the snapshot pack** (§4.3, `wide`/`pulp`, off-hash) — speeds the render path only.
3. **Renderer: per-species MultiMesh + field-texture overlays** (godot, GDScript-only, hash-neutral).
4. **ONLY if grid > 64×64 or entities > 50k AND single-thread can't hold 60 fps:** rayon parallel-MAP
   (§4.1, collect-into-indexed-Vec; sequential f64 fold stays serial) — its own slice + multi-ISA
   hash check.
5. **ONLY if an OS-level read-only boundary is wanted:** split the renderer to a separate process
   behind an mmap SPSC ring carrying GSS4 frames (strengthens inv #2).

---

## 5. DETERMINISM STRATEGY across platforms

inv #3 today: one master seed → single `ChaCha8Rng` threaded explicitly; `hash_world` folds in fixed
OrgId/(cell,species,org) order; the multi-ISA gate (`ci.yml` `determinism-multi-isa`, x86_64 +
aarch64, `-fp-contract=off`) asserts a byte-identical `0x64a3_ed4f_7bb1_2779`. **The moat is IEEE-754
+ no-FMA + bounded no-NaN f64, NOT integers (§0).** Here is how it extends:

| Platform / change | Effect on the hash | Hash-neutral vs needs proving |
|---|---|---|
| **UE5 renderer (A or B)** | core unchanged; snapshot off-hash; GPU non-determinism never re-enters | **HASH-NEUTRAL by construction.** Add `check_unreal_snapshot` (channel-count parity). |
| **UE5 IPC process split** | integer cadence decoupled from 60 fps; renderer can't mutate | **HASH-NEUTRAL; STRENGTHENS inv #2/#3.** |
| **web renderer (A or B)** | renderer reads the identical off-hash GSS4 bytes | **HASH-NEUTRAL** (renderer); the CORE on wasm32 → **NEEDS PROVING (§3.2)**. |
| **wasm32 core** | a third float env (no FTZ control; relaxed-SIMD hazard) | **NEEDS PROVING** — the wasm32 gate leg asserting `0x64a3_…` is the proof; conservative target-feature, no relaxed-simd. |
| **parallel MAP (rayon collect)** | only the integer per-cell/org map parallelizes; f64 fold stays serial | **HASH-NEUTRAL IFF collect-order-preserving + sequential f64 fold**; multi-ISA hash check required. |
| **par-reduce / par f64 sum** | unspecified merge order; f64 non-associative | **BREAKS THE HASH — banned on anything feeding `hash_world`.** |
| **SIMD on snapshot (f32)** | off-hash, read-only | **HASH-NEUTRAL by construction.** |
| **SIMD on chem i32 stencil** | in-hash; integer SIMD is bit-exact | **NEEDS PROVING** via multi-ISA (AVX2 vs NEON lane order); deliberate re-pin if it shifts. |
| **BTreeMap → sorted-Vec** | drain order already canonicalized | **HASH-NEUTRAL** (own slice + hash check). |

**The CI gate matrix grows by one real leg: x86_64 + aarch64 + wasm32.** The read-only-renderer rule
(inv #2) protects the hash for every renderer because no renderer output ever folds back into the
core — the GSS4/observe/flow exports are off-`SimRng`, off-`hash_world`, one-way.

**The golden rule for all platform/perf work:** the f32 boundary is the snapshot. Everything UPSTREAM
of `write_snapshot_bytes` stays {integer + fixed-order} or {bounded no-NaN f64 + fp-contract=off} and
is multi-ISA-(and-soon-wasm)-gated; everything DOWNSTREAM (normalize, SIMD, GPU, IPC, Niagara, HLSL)
is free real estate — off-hash and read-only.

---

## 6. MIGRATION + ADR DRAFT + SLICE PLAN

### 6.1 Phasing (the godot PoC keeps working throughout — the core boundary is the stable seam)

Because every renderer is a thin binding over the unchanged core, the godot PoC is never disturbed:

- **Phase W0 — PROVE wasm determinism (no renderer).** Add the wasm32 gate leg. Headless-first; lands
  before any web/UE renderer. *This is the single highest-value, lowest-risk first step.*
- **Phase W1 — perf foundation (hash-neutral).** BTreeMap→Vec (§4.2), then snapshot SIMD (§4.3).
- **Phase U0 — UE5 binding spike.** `crates/unreal-sim` cdylib (C-ABI mirror of `LiveSim`) +
  `cbindgen` header + a smoke that loads it and round-trips one `gs_snapshot`. No engine yet.
- **Phase U1 — UE5 renderer.** Niagara populations + Grid2D field overlays + relations widget
  (Pattern B IPC preferred). `check_unreal_snapshot` parity gate.
- **Phase X0 — web renderer.** Decide Path A (gdext Emscripten, pin toolchain) vs Path B
  (wasm-bindgen + thin wgpu/canvas). Reuse GSS4 verbatim.

Each phase is independently shippable and leaves the gate green; none touches `sim-core` biology.

### 6.2 ADR-019 (DRAFT) — to append to DECISIONS.md when a phase lands

> **## ADR-019 — One deterministic core, many read-only renderers (godot PoC / UE5-realistic / web)**
>
> **Status:** Proposed. **Stage:** 4+ (renderer platform). **Supersedes:** nothing; extends ADR-010
> (gdext binding) and ADR-013 (the determinism gate).
>
> **Context.** The deterministic headless core (`sim-core`+`harness`) is engine-agnostic (inv #4). The
> godot PoC consumes it through thin `LiveSim` `#[func]` bindings (ADR-010). We want a realistic
> renderer (UE5) and a simple web build WITHOUT forking biology (inv #2) or weakening determinism
> (inv #3).
>
> **Decision.** Formalize the **core-as-library contract**: the GSS4 snapshot + `observe*()` +
> `flow_matrix()` + `species_relations()` + `region_allele()` + edit verdicts + the journal — all
> off-`SimRng`, off-`hash_world`, read-only — are THE contract every renderer consumes. UE5 binds via
> a `crates/unreal-sim` C-ABI cdylib (a structural mirror of the `LiveSim` `#[func]`s; scalars + opaque
> LE buffers only) and/or an IPC snapshot stream; web binds via gdext-Emscripten (Path A) or
> wasm-bindgen over `wasm32-unknown-unknown` (Path B). The core depends on no renderer.
>
> **Determinism correction (load-bearing).** The moat is **IEEE-754 + no-FMA-contraction + bounded
> no-NaN f64**, NOT "integers only": `hash_world` folds `f64` Genotype/DroughtTol/ThermalTol/allele_freq
> via `.to_bits()` (`lib.rs:2513-2515, 2598`), produced by real `f64` arithmetic (`mutate_unit`,
> `mean_genotype`); CI pins `-fp-contract=off` for exactly this reason. Consequences: (a) wasm32 is a
> THIRD float environment and its reproduction of `0x64a3_ed4f_7bb1_2779` MUST be CI-proven, not
> assumed; (b) any parallel `f64` fold (e.g. par-summing `mean_genotype`) is BANNED — only
> order-preserving `collect`-into-indexed-Vec + sequential fold is permitted.
>
> **Pinned (inv #7).** Emscripten 3.1.74 (Path A); gdext `experimental-wasm`; wasm-bindgen
> (Path B); a single gdext cdylib (gdext #968 blocks multi-extension web export); UE5 minor + the
> SharedMemoryMedia/Live Link transport when Pattern B lands; the wasm32 target-feature set
> (conservative, NO relaxed-simd).
>
> **Consequences / risks.** gdext web export is EXPERIMENTAL; gdext #968 forbids a second gdext lib;
> Rust 1.87 changed the wasm32 C-ABI (cross scalars + opaque buffers only); UE5 C++/Blueprint needs the
> same inv-#2 review GDScript gets. The CI determinism matrix grows to x86_64 + aarch64 + wasm32.

### 6.3 Paste-ready TASKS.md block (the roadmap entries)

```markdown
## ROADMAP — Rendering & platform: one deterministic core, many read-only renderers (ADR-019 draft)
> Proposal: docs/llm/proposals/rendering-platform-architecture-draft.md. The core boundary is the
> stable seam — the godot PoC keeps working throughout. DETERMINISM MOAT = IEEE-754 + no-FMA +
> bounded no-NaN f64 (NOT "integers only": hash_world folds f64 traits via .to_bits()).

### Phase W — wasm determinism + perf foundation (headless-first; do FIRST)
- [ ] **W0 — PROVE wasm32 determinism (gate leg, NO renderer).** Add a `wasm32-unknown-unknown` leg to
      the multi-ISA gate: compile sim-core+harness (conservative target-feature, NO relaxed-simd), run
      the pinned-hash test under wasmtime (wasm32-wasip1) or wasm-bindgen-test, assert
      `== 0x64a3_ed4f_7bb1_2779`. AC: the wasm leg runs in CI and the hash is byte-identical to the
      x86_64/aarch64 legs (fix if it diverges; subnormal/FTZ + relaxed-simd are the suspects).
- [ ] **W1 — BTreeMap → sorted-Vec hot-path swap (perf, hash-neutral).** Replace the per-tick
      `BTreeMap<u64,T>` accumulators in sim-core with `Vec<(u64,T)>` + `sort_unstable_by_key` (drain
      order already canonicalized). AC: `determinism_hash_is_pinned` stays green unmodified; re-bench
      vs the post-F5 baseline (61.7/295.4/590.8 ms) shows a win.
- [ ] **W2 — SIMD-normalize the snapshot pack (perf, off-hash).** `wide`/`pulp` the i32→f32 normalize
      in `write_snapshot_bytes`. AC: GSS4 bytes byte-identical to scalar; snapshot wall-time drops;
      hash untouched (off-hash by construction).

### Phase U — UE5 realistic renderer (read-only; reuses GSS4 + LiveSim contract)
- [ ] **U0 — `crates/unreal-sim` C-ABI cdylib + cbindgen header (binding spike, no engine).** Flat
      `extern "C"` mirror of the LiveSim #[func]s (scalars + opaque LE buffers only — Rust 1.87 wasm
      C-ABI discipline). AC: a Rust smoke loads the lib, round-trips `gs_reset`/`gs_step`/`gs_snapshot`,
      and the GSS4 bytes match `godot-sim`'s for the same seed.
- [ ] **U1 — UE5 plugin + renderer (Pattern B IPC preferred).** `.uplugin`+`Build.cs` dlopen'ing
      unreal-sim; Niagara populations (Position Array → custom NDI at scale) + Grid2D field overlays +
      relations flux widget; a `check_unreal_snapshot` channel-count parity gate. AC: UE displays a
      live run reading only off-hash GSS4/observe/flow exports; reviewer confirms ZERO biology
      renderer-side (inv #2); core hash unaffected.
- [ ] **U2 — multi-scale zoom LOD (cell→colony→ecosystem).** Field-heatmap-far → Niagara-mesh-mid →
      detail-mesh/HV-near, driven by the snapshot(w,h) resample. AC: zoom switches LOD without any
      core call beyond `snapshot`; reserve SVT/HV for baked replay.

### Phase X — web build (the simple iso/top-down version)
- [ ] **X0 — DECIDE + scaffold the web path (after W0 is green).** Path A (gdext Emscripten export of
      the iso frontend — pin Emscripten 3.1.74, keep ONE gdext cdylib per gdext #968) OR Path B
      (wasm-bindgen over wasm32 core + thin wgpu/canvas renderer reusing GSS4 verbatim). AC: an ADR
      records the choice + pins (inv #7); a "hello world" snapshot renders in a browser.
- [ ] **X1 — web renderer: instanced populations + field-texture overlays.** WebGL2 instancing (or
      Godot MultiMesh in Path A) for organisms; one R32F data texture per channel for the fields. AC:
      the browser build shows a deterministic run; the GSS4 parse matches `godot/snapshot.gd`.

### Perf levers (hold until needed; each its own slice + multi-ISA hash check)
- [ ] **P-PAR — rayon parallel-MAP (ONLY if grid>64² or N>50k AND single-thread misses 60fps).**
      collect-into-indexed-Vec for the integer per-cell/org map; the f64 `mean_genotype` fold STAYS
      sequential (f64 add is non-associative). AC: hash byte-identical across thread counts AND across
      x86_64/aarch64/wasm32; NEVER use par-reduce/par-sum on anything feeding hash_world.
- [ ] **P-IPC — split the renderer to a separate process (ONLY if an OS-level read-only boundary is
      wanted).** mmap SPSC double-buffered ring carrying GSS4 frames. AC: renderer can only read;
      strengthens inv #2; core hash unaffected.
```

---

## 7. Bottom line

The "one portable deterministic core → many read-only renderers" architecture is **SOUND and
determinism-safe in its conclusions** — inv #2 holds by construction for the C-ABI/wasm-bindgen/UE5
mirrors (they are structural copies of the `LiveSim` `#[func]`s), and inv #3 is preserved by every
pattern (snapshot off-hash, integer cadence, GPU/IPC non-determinism never re-entering the hash, the
multi-ISA gate untouched). **But it must NOT be sold on the false "pure i64 pipeline" premise:** the
moat is IEEE-754 + no-FMA + bounded no-NaN f64, which is exactly why **wasm needs its OWN CI proof
(W0) before any web reproducibility claim is made**, why a parallel f64 fold is banned, and why the
four pins (wasm gate leg · Emscripten/gdext/wasm-bindgen toolchain · single gdext cdylib · scalars +
opaque LE buffers across every C-ABI) are load-bearing. Do W0 first; everything else is downstream of
a proven seam.
