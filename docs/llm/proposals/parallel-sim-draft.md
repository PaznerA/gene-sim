# Proposal — Deterministic parallelization of the gene-sim tick (rayon, compute-parallel / apply-canonical)

> **Status:** COMMITTED — human sign-off received; **S0 LANDED** (rayon pinned dep + persistent global pool +
> `PAR_THRESHOLD` + `--no-parallel` escape hatch + ADR-020), hash-neutral (`0x47a0_3c8f_6701_f240` byte-identical,
> NO call sites yet). S1–S6 proceed strictly slice-by-slice with the hash as the gate (§9).
> **Scope:** ADR draft + slice plan for parallelizing the three RNG-free, cell-independent compute hotspots
> of the sim tick with `rayon`, inside the heavy systems, while the Bevy schedule stays single-threaded.
> **Touches an invariant** (inv #3 determinism, inv #7 pinned versions, inv #1 process boundary) → **STOP-THE-LINE**: this doc exists so the human signs off *before* code.
> **NOT a re-pin.** The pinned literal `0x47a0_3c8f_6701_f240` (asserted at `lib.rs:3227` and `lib.rs:3391`) is the **oracle that stays unchanged**. If any slice moves it, that slice is a bug and is reverted.

---

## 1. CONTEXT — the single-thread ceiling, and why parallelism is the only lever left

The post-F5 hot path has been driven to its single-thread floor. The current pinned baseline (DECISIONS.md
perf table, full F3→F4→F5 pipeline after the hash-neutral allocation-elimination sweep):

| Workload (spawn × gens) | Median wall | Throughput |
|---|---|---|
| 1 000 × 50  | **61.7 ms**  | ~0.81 M org-updates/s |
| 5 000 × 50  | **295.4 ms** | ~0.85 M org-updates/s |
| 10 000 × 50 | **590.8 ms** | ~0.85 M org-updates/s |

The headline is **~847 Kelem/s** (~0.85 M organism-updates/s), flat across N — the tick is compute-bound,
not allocation-bound. The micro-optimization levers are **exhausted**:

- The allocation-elimination sweep (reused `MetabolismScratch`/`ReproScratch`/`ChemEmitScratch` buffers,
  `apportion_into`/`split_budget_into`, precomputed `SolarLightCap`, reused `ChemField.src_buf`) already
  landed and bought 1k −13% / 5k −8% / 10k −6% — **byte-identical**, hash unmoved.
- The remaining per-tick `BTreeMap`s (`by_org`, `litterfall`, `toxin_mints`, …) are the only structural
  alloc target left, and DECISIONS.md explicitly defers them as a self-contained re-pin-risk slice worth
  ~single-digit %, not a step-change.
- LTO / codegen-units / target-cpu tuning is in the ~0–1% noise band.

**There is no remaining single-thread win of consequence.** The tick is ~0.85 M org-updates/s and will stay
there. The only lever that moves the bar by a *multiple* is **data parallelism inside the heavy systems** —
which the DECISIONS.md ADR-002/ADR-013 consequence note already anticipated: *"revisit if the perf gate
forces it — parallelism would then need a deterministic reduction."* **This is that reduction.**

### What is hot, and which passes are eligible

Profiling attributes the tick roughly as: **metabolism ~45%** (`lib.rs:691`), **reproduce_or_die ~22%**
(`lib.rs:1265`, RNG-bound), **diffuse_and_decay ~12–15%** (`chem.rs:315`), **mineralize ~5%**
(`trophic.rs:562`), the rest spread over emit_chem / germinate / solar_influx / the three per-tick asserts /
hash_world. The three **RNG-free, cell-independent** systems — metabolism, diffuse_and_decay, mineralize —
are the parallelization targets. `reproduce_or_die` is the immovable sequential ceiling.

---

## 2. DECISION

**Parallelize the three RNG-free, cell-independent compute hotspots with `rayon`, *inside* the heavy systems.
The Bevy `.chain()` schedule stays strictly single-threaded. Compute-parallel + apply-canonical.
`reproduce_or_die` — the sole `SimRng` consumer — stays 100% sequential.**

### 2.1 Which passes (and which stay sequential)

**PARALLELIZE** (all verified to take no `ResMut<SimRng>`, all read frozen start-of-tick snapshots):

1. **metabolism** (`lib.rs:691`) — the ~45% hotspot. Doc-pinned *"Draws ZERO `SimRng`"* at `lib.rs:681`.
2. **diffuse_and_decay** (`chem.rs:315`) — ~12–15%, pure `>>` shifts. **Requires the scatter→gather rewrite first** (§4).
3. **mineralize** (`trophic.rs:562`) — ~5%, structurally a smaller metabolism.

**DEFER to a tail slice (S6), multi-species only:**

- **predation** (`trophic.rs:777`) and **host_coupling** (`trophic.rs:1114`). Both are RNG-free,
  frozen-census, per-cell kernels that fit the same compute/apply split — **but** they run *after*
  `reproduce_or_die` in the chain (`lib.rs:1940–1941`, on the post-birth census) and **early-return as
  no-ops** on the pinned single-species plant roster (`predators.is_empty()` at `trophic.rs:812`). They
  cost nothing on the pinned bench → parallelize only if a *multi-species* bench shows them hot.

**STAY STRICTLY SEQUENTIAL, untouched:**

- **reproduce_or_die** (`lib.rs:1265`) — the **SOLE** `SimRng` consumer. Builds + sorts its rows serially
  (`lib.rs:1322`, canonical `(cell,species,org)`), then draws **exactly D+1 = 4** `next_u64` per
  threshold-passing birth, unconditionally, in canonical order, folded into `DrawCount` and the hash.
- emit_chem (RNG-free but tiny, OrgId-map apply), germinate, solar_influx / reset_flow / reset_chem_scratch
  (trivial O(cells)), the three per-tick asserts (chem-conservation, flow-closes, ledger), and
  `hash_world` (`lib.rs:3068`, OrgId-sorted).

The Bevy `.chain()` schedule (`lib.rs:1928–1946`) **stays single-threaded**. `rayon` lives **strictly
inside** the three heavy systems, never in the Bevy scheduler.

### 2.2 The core discipline — COMPUTE-PARALLEL / APPLY-CANONICAL

This is the discipline the code already half-uses (litterfall/toxin collected into sorted rows then applied
in canonical order, `lib.rs:1037/1056`):

- **PHASE A (PARALLEL, the expensive ~95%):** per cell-chunk, compute the Monod demand (`monod_demand`
  `lib.rs:669`, u128, no float), the largest-remainder apportion (`apportion_into` `fixed.rs:47`, pure),
  and the `split_budget` convert — writing into **disjoint** `granted[lo..hi]` sub-slices plus **per-task
  Vecs** of grant records, per-task litterfall/toxin records, and per-task FlowMatrix edge records.
- **PHASE B (SEQUENTIAL, the cheap ~5%, canonical order):** apply everything order-sensitive in the
  **exact current sequence** — the integer-add order is byte-for-byte unchanged.

### 2.3 Partition — a contiguous range of whole cells

**UNIT = a contiguous range of whole cells**, free from the existing canonical sort. The metabolism `items`
(`lib.rs:767`) and mineralize `rows` (`trophic.rs:591`) are already sorted by `(cell,species,org)` with
**cell as the primary key**, so each cell's orgs form a contiguous `items[i..j]` slice — exactly what the
Pass-2 apportion while-loop walks (`lib.rs:908–913`). Strategy differs by pass:

- **(A) Pass-1 DEMAND** (`lib.rs:786–892`) — purely per-**item** independent (each row's demand is a function
  of its own row + frozen stocks + read-only registry/chem-mod/edit-factor). Parallelizes as a flat
  `items.par_iter()` zipped with disjoint `demand[i]` writes — **no cell-boundary care needed.**
- **(B) Pass-2 APPORTION** (`lib.rs:894–944`) — per-**cell**. After the sort, scan once (the existing
  `while items[j].cell==cell` walk) to build a `Vec<(cell,lo,hi)>` of cell-group spans, then chunk **whole
  cell-groups**. A cell is the apportionment atom — the live pool is decremented **once** per `(channel,cell)`
  at `lib.rs:940` — so **no cell may straddle two chunks.** `par_iter` over chunks-of-cell-groups.
- **(C) Pass-3 CONVERT** (`lib.rs:968–1029`) — per-**org** given the grant total. Parallelizes over orgs
  producing per-org outputs collected for sequential apply.

**Chunk sizing:** target ~4×core-count tasks (~48 on a 12-core M4 Max) for work-stealing balance; size
chunks by **sum-of-orgs-in-cells**, not cell-count (orgs cluster — equal cell counts ≠ equal work).

**Sequential threshold:** if `items.len() < PAR_THRESHOLD` (~2000, bench-tuned), run the **existing serial
loop verbatim**. The pinned ~1k-org config (61.7ms) takes this serial path — an extra byte-identity guarantee.

For **diffuse_and_decay** the partition is the row-major 1024-cell **output** index space (the gather rewrite
makes each dst cell write-disjoint), chunked by contiguous cell index, with the **same small-grid guard**
(1024 cells is tiny — diffusion parallelism may only pay at larger grids; the gather is worth landing
serially regardless).

### 2.4 Per-task scratch (no shared mutable buffers)

The World-owned `MetabolismScratch` (`lib.rs:299`), `ReproScratch` (`lib.rs:313`), and
`PoolProvenance.scratch_w/scratch_s/scratch_rem` (`trophic.rs:122`, single-buffer) hold **one** set of reused
buffers and **cannot be `&mut`-shared across threads**.

- Keep the World-owned scratch **only for the sequential prologue**: the global sort, the frozen
  light/nutrient/detritus/toxin snapshots (read-only thereafter), the cell-span scan, and the
  `items`/`demand`/`granted` output Vecs (cross-tick backing preserved via the existing `std::mem::take`
  pattern; written by **disjoint index ranges**).
- For the **parallel inner work**, use rayon's `fold` / `map_init` with an **init closure** so each rayon
  task allocates its own small per-task scratch **once at task start** and reuses it across the cell-groups
  in that task: the apportion `weights/shares/rem_scratch` (`lib.rs:902–904`), the convert
  `split/split_w/split_rem` (`lib.rs:965–967`), **plus** a per-task provenance-withdraw scratch (mirroring
  `trophic.rs:122–129`) so the `PoolProvenance::withdraw` apportion is thread-local. These buffers are
  write-local, never escape, never feed the hash → **byte-identical to the serial buffer.**
- `demand[]`/`granted[]` sub-slices are taken via `split_at_mut` over the task's item range (disjoint `&mut`,
  **borrow-checker-enforced — no `RefCell`/`unsafe`**).
- *Fallback if per-task alloc churn shows in the bench:* a `Vec<ChunkScratch>` pool indexed by chunk, grown
  to max-chunks-seen, cleared each tick, so backing allocations survive cross-tick. The `fold`-with-init form
  is simpler, equally deterministic, and is the **default**.

### 2.5 Thread-safe writes — the two-tier write rule

- **(1) DISJOINT-CELL pool writes** — the live `PoolStock` decrement (`pool_channel_mut` `lib.rs:940`),
  free_nutrient mint, detritus deposit are per-`(channel,cell)` and each cell belongs to exactly one chunk →
  disjoint by construction. **The conservative default for the first parallel landing collects *even these*
  into per-task records and replays them sequentially in canonical `(channel,cell)` order** — this makes the
  byte-identity proof one line (the apply sequence is *verbatim* the current loop) and only the
  Monod/apportion/convert arithmetic moves off-thread. Promote to direct Phase-A writes as a later perf
  slice once the hash is proven stable.
  - *Note:* the current Pass-2 loop nests channels **outside** cells (`for c in 0..RESOURCE_CHANNELS { for cell … }`)
    and decrements `pool_channel_mut(c)[cell]` once per `(channel,cell)`. To keep byte-identity trivially
    provable, parallelize **over cell-groups within each channel pass**, so the per-`(channel,cell)`
    decrement stays a single write owned by one task.
- **(2) SHARED-TARGET writes go through Phase B:** the FlowMatrix provenance withdrawals
  (`prov.withdraw_nutrient→flow` `lib.rs:932`; `prov.withdraw_detritus→flow` `trophic.rs:693`) are collected
  per-task and merged in canonical order (§5). The per-org Energy/Biomass mutation stays driven by the
  OrgId-keyed `by_org` map (`lib.rs:949,968`) — already order-independent because each org's new value is a
  pure function of its `granted_total`. The litterfall/toxin_mints stay in their
  collect→sort-by-`(cell,species,org)`→serial-apply path (`lib.rs:1037,1056`) **UNTOUCHED**, because
  cap-overflow routing is **order-sensitive** (a saturated cell routes overflow to whichever deposit lands
  first).

### 2.6 RNG handling

The three parallelized passes draw **zero** `SimRng` — verified by their signatures (metabolism
`lib.rs:691` has no rng/draws param; mineralize `trophic.rs:562` none; diffuse_and_decay `chem.rs:315` none)
and their doc pins (`lib.rs:681` *"Draws ZERO `SimRng`"*; chem.rs RNG-free). `monod_demand` is pure u128
(`lib.rs:669`); `apportion_into`/`split_budget_into` draw nothing (`fixed.rs`).

Because **no parallelized pass holds a `&mut SimRng`, the ChaCha8 stream is physically untouchable by the
parallelism** — no worker can advance it out of order. The only stream advancer, `reproduce_or_die`
(`lib.rs:1265`, `ResMut<SimRng>` + `ResMut<DrawCount>`), stays sequential and untouched: it builds+sorts its
rows serially (`lib.rs:1322`), then draws **exactly 4** `next_u64` per threshold-passing org unconditionally
in canonical order with `draws.0 += 4`. Since the parallel passes produce a **byte-identical** post-metabolism
Energy/Biomass/pool state, the survivor list + threshold tests reproduce_or_die walks are byte-identical → the
draw count, draw order, `DrawCount` (folded at `lib.rs:3103`), and the `final_word` fold (`lib.rs:3127`) are
all bit-for-bit preserved → `0x47a0_3c8f_6701_f240` is preserved. **The cleanest possible separation: the
(potentially) nondeterministic-order region is RNG-free; the RNG region is strictly deterministic-order.**

---

## 3. BYTE-IDENTITY GUARANTEE — the determinism proof

Byte-identity rests on **five independent legs**. The pinned literal `0x47a0_3c8f_6701_f240` (asserted at
`lib.rs:3227` and `lib.rs:3391`) is the oracle and **stays unchanged — this is NOT a re-pin.**

1. **INDEPENDENCE.** Each parallel unit computes a cell-chunk whose cells are **disjoint** from every other
   chunk's, and each per-cell computation is a **pure function** of frozen read-only inputs (frozen
   light/nutrient/detritus/toxin snapshots `lib.rs:301–304/771–779`, the chem frozen `src` plane, the
   read-only registry `Strategy`, the org's own pre-sorted row). No worker reads another worker's output;
   `demand[i]`/`granted[lo..hi]` writes go to disjoint index ranges → same inputs → same outputs regardless
   of scheduling.
2. **RNG-FREE.** No parallelized pass touches `SimRng`/`DrawCount`, so the only stream advancer (sequential
   `reproduce_or_die`) is unperturbed (§2.6).
3. **CANONICAL APPLY ORDER.** All order-sensitive mutations to shared state (PoolStock decrements per
   `(channel,cell)`, PoolProvenance, FlowMatrix, litterfall/toxin cap-overflow routing at `lib.rs:1037/1056`,
   org Energy/Biomass via the OrgId `by_org` map) are applied **sequentially in the exact current order**, so
   the integer-add sequence is byte-for-byte unchanged.
4. **ORDER-INDEPENDENT REDUCTIONS.** The only true cross-task reductions (the FlowMatrix merge, the diffusion
   `decayed` sum, ledger taps) are **i64 ADD — associative AND commutative** on i64 (no float, no
   saturating-reorder) — so per-task partials summed in fixed task order equal the serial sum; the gather
   diffusion's per-cell value is a fixed set of ≤5 integer adds on the frozen `src`. The **one float** on the
   path (the soil/climate `match_permille` f64, `lib.rs:742–747`) is **quantized once via `to_unit_u16`
   per-org BEFORE any parallel region**, so **no f64 reduction ever crosses a thread.**
5. **NO HashMap iterated in sim logic** (inv #3) — the BTreeMaps stay sequential, applied in sorted-Vec order;
   rayon iterates only Vec index ranges; rayon's work-stealing is nondeterministic in **timing** but the
   **result** depends only on the disjoint-cell decomposition, not on which thread ran which chunk.

**The net:** if `0x47a0` comes out byte-identical, the parallelization is provably correct — one reordered
i64 accumulation on the hashed path would move it.

### 3.1 The two hash oracles

- **Local:** `tools/check_determinism.sh` runs the seed twice and asserts identical bytes.
- **Cross-platform:** `tools/check_determinism_multi_isa.sh` — the **multi-ISA CI gate** (x86_64 hash ==
  aarch64 hash, `--features determinism` with the HARD asserts at `chem.rs:398` / `trophic.rs:701`
  `assert_flow_rows_sum_zero`). This is the safety net for any **latent platform-dependent reduction** the
  single-arch M4 run would miss. **It must run on every push for these slices.**

---

## 4. THE DIFFUSION SCATTER→GATHER REWRITE (prerequisite for parallel diffusion)

Current `diffuse_and_decay` (`chem.rs:339–362`, verified) is a **SCATTER**: each source cell `c` with
`cc=src[c]` computes `share = cc>>shift` and **pushes** it to up-to-4 von-Neumann neighbours in pinned
`[N,E,S,W]` order (`chem.scratch[n] += share`), reflecting each off-grid direction's share back to self
(`chem.scratch[c] += share`, `chem.rs:357`), and keeping the remainder (`chem.scratch[c] += cc - 4*share`).
**Two sources writing a shared neighbour = a write conflict → not parallelizable by source cell.**

**REWRITE to GATHER:** for each **output** cell `d`, compute `new[d]` purely by **reading the frozen `src`
snapshot** (the existing `src_buf`, `chem.rs:336–338`):

```
new[d] =   (src[d] - 4*(src[d]>>shift))                          // kept remainder
         + Σ over in-grid von-Neumann neighbours nb of d  ( src[nb]>>shift )   // received quanta
         + (count of d's OWN off-grid edges) * (src[d]>>shift)   // the reflect term
```

Each `d` writes **only** `new[d]` from read-only `src` → embarrassingly parallel by output cell, zero write
conflict → `new.par_iter_mut().enumerate()` / `par_chunks_mut` over row bands.

**Byte-identical proof:** the scatter sends, for each ordered in-grid pair `(c,nb)`, exactly `src[c]>>shift`
to `nb`; the gather has `d` **receive** `src[nb]>>shift` from each in-grid neighbour `nb` naming `d`.
Von-Neumann adjacency is **symmetric** (`c` neighbours `nb` iff `nb` neighbours `c`) and the per-edge quantum
`src[source]>>shift` is computed with the **same `>>shift` floor on the same frozen `src`**, so every i64 the
gather sums equals exactly what the scatter accumulated into `scratch[d]` — the kept remainder and the
reflect-to-self (one share per off-grid direction of `d`) are reproduced identically. Integer add of the same
≤5 terms is order-independent → `new[d]` is bit-identical to the scatter's `scratch[d]`. The
Σ-conservation assert (`chem.rs:372`, `assert_chem_conserved`) holds by construction.

The **decay tap** (`chem.rs:378–385`) is already per-cell independent (`lost = cell>>DECAY_SHIFT`) and
parallelizes trivially; its `decayed` accumulator becomes per-task partial i64 sums merged in fixed chunk
order (integer add associative → identical i64 → identical `ledger.chem_decay`).

> **CRITICAL TEST GATE:** land the gather rewrite **SERIALLY** first and prove the hash unmoved **BEFORE**
> parallelizing. The reflect term (count of `d`'s **own** off-grid edges × `src[d]>>shift`, **NOT** the
> neighbours' reflects) is the one easy-to-get-wrong spot.

---

## 5. THE FLOWMATRIX REDUCTION

`FlowMatrix::record` (`trophic.rs:101–107`, verified) does `j[dest*s+src] += amount; j[dest*s+dest] -= amount`
— a paired integer accumulation into a flat S×S i64 Vec, keeping each dest row summing to zero. Under
parallelism the per-cell provenance withdrawals (`withdraw_nutrient` `lib.rs:932`; `withdraw_detritus`
`trophic.rs:693`) would race the shared matrix.

- **DEFAULT (simplest, the first landing):** each rayon **task** collects its `(dest_species, src_species,
  amount)` withdrawal records into a per-task Vec; Phase B concatenates them in **fixed task/chunk order**
  (ascending = ascending cell index) and replays `prov.withdraw_*`/`flow.record` **sequentially** — the
  matrix mutates in the **identical sequence** to today, byte-identical, trivially auditable.
- **PROMOTE if Phase B shows hot:** each task owns a zeroed local S×S i64 matrix, accumulates into it, and
  Phase B sums the per-task matrices into the World FlowMatrix entry-by-entry in fixed task order.
  Byte-identity: i64 add is **associative AND commutative**, so each final `A[i][j] = Σ_tasks local[t][i][j]`
  is order-independent → identical regardless of task scheduling, as long as the same set of quanta is added
  (guaranteed — disjoint cells, each withdraw computed exactly once). The row-sum-zero invariant
  (`assert_flow_rows_sum_zero`, `trophic.rs:701`) survives any summation order because every `record()` pairs
  its `+amount` off-diagonal with its `−amount` diagonal within the same matrix. We pin the merge to canonical
  chunk order anyway as defensive discipline (mathematically irrelevant, belt-and-suspenders).

**Scope note:** S = registry length = **1 for the pinned single-plant run** (a 1×1 matrix, a constant
contribution → moot for the pinned `0x47a0` hash); the reduction **matters only for multi-species runs**,
which the multi-ISA gate also covers. The per-cell `PoolProvenance` reads/decrements (`trophic.rs:220`) are
disjoint-cell so the provenance plane access is thread-local too — but the per-task withdraw scratch (§2.4)
must be local.

---

## 6. HONEST SPEEDUP — Amdahl, reconciled

Parallelizable share `X = metabolism (~45%) + diffuse_and_decay (~13%) + mineralize (~5%) ≈ 0.63` of the tick
— **but only the COMPUTE phase parallelizes**; the sequential prologue (global sort, frozen snapshots,
cell-span scan) and the canonical apply tail (FlowMatrix merge, ECS Energy/Biomass mutate, litterfall/toxin
deposit, ledger) claw back ~15–20% of those systems' own cost, and diffusion over only 1024 cells is tiny and
may not pay a dispatch at all. So the **effective parallel fraction** `X_eff ≈ 0.63 × 0.85 ≈ 0.54`.

Sequential remainder `(1 − X_eff) ≈ 0.46` = `reproduce_or_die` (~22%, RNG-bound, **IMMOVABLE — the hard
ceiling**) + emit_chem + germinate + the three asserts + `hash_world` + the apply tails.

On a 12-core M4 Max with `P ≈ 10` effective workers (leave headroom; rayon past P-cores gives diminishing
returns; unified-memory bandwidth caps the 10k case):

```
max speedup = 1 / ((1 − X_eff) + X_eff/P) = 1 / (0.46 + 0.54/10) ≈ 1 / 0.514 ≈ 1.95×
```

**By workload** against the verified baseline:

| Workload | Baseline | Projected | Speedup |
|---|---|---|---|
| 1 000 × 50  | 61.7 ms  | ~62 ms (serial path, below PAR_THRESHOLD) | **~1.0×** |
| 5 000 × 50  | 295.4 ms | ~155–175 ms | **~1.7–1.9×** |
| 10 000 × 50 | 590.8 ms | ~250–290 ms | **~2.0–2.4×** |

**HONEST HEADLINE: ~2–2.5× at 5k–10k orgs, NOT 4×.** `reproduce_or_die`'s mandatory-sequential per-birth
4-word RNG draw plus the per-tick asserts are the hard Amdahl ceiling, and the parallel fraction is barely
over half the tick. If emit_chem and predation/host_coupling are later parallelized on a multi-species
roster, `X_eff` rises toward ~0.65 and the 10k ceiling moves to ~2.6–2.8×, still RNG-bound. **At 1k orgs the
PAR_THRESHOLD deliberately keeps the serial path — possibly a slight regression avoided, and a free
byte-identity guarantee.**

---

## 7. INVARIANTS

- **Inv #3 (Determinism) — the load-bearing one.** Argued in full in §3 (five legs) + §4 (gather proof) +
  §5 (reduction proof) + §2.6 (RNG isolation). The summary: parallel region is RNG-free + compute-pure +
  disjoint-cell; every order-sensitive mutation is applied sequentially in canonical order; the only
  cross-task reductions are associative-commutative i64 adds; the single float is quantized upstream of any
  thread. **No HashMap is iterated in sim logic** — rayon iterates Vec index ranges only; the BTreeMaps stay
  sequential. The two hash oracles (local double-run + multi-ISA CI) catch any latent reorder. **This is the
  ADR-002/ADR-013 "deterministic reduction" the consequence note explicitly anticipated.**
- **Inv #1 (GPL at the process boundary).** `rayon` is **MIT/Apache-2.0 dual-licensed** — inv #1's
  process-boundary rule is about **GPL only**, so rayon **linked into the game binary is fine**. No GPL crate
  is added; `oracle-slim` is untouched. The boundary discipline is preserved as hygiene.
- **Inv #7 (Versions pinned).** rayon **IS a new pinned dependency** → inv #7 **requires** recording the
  exact rayon version in DECISIONS.md alongside the bevy/rand_chacha pins, and locking `Cargo.lock`. A rayon
  minor bump is a cross-version reproducibility event to re-gate (low-risk given schedule-result-independence,
  but pinned like `bevy_ecs`/`rand_chacha`).
  - **Build-profile note:** the determinism-feature artifact (HARD asserts) must keep building and hashing
    cross-arch in CI; pin a reproducible worker count (`RAYON_NUM_THREADS` / explicit `num_threads`) for
    **stable benches** — correctness does not depend on it, but bench variance does.

---

## 8. RAYON INTEGRATION (the mechanics)

- Build a **persistent global rayon ThreadPool ONCE** (`OnceLock`, or a World resource holding the pool, or
  the default global pool with a pinned `num_threads`/`RAYON_NUM_THREADS`). **NEVER spawn/teardown a pool per
  tick** (per-tick thread-creation cost + nondeterministic worker counts).
- Run the passes via `pool.install(|| …par_iter/par_chunks_mut/fold…)` **inside** the three systems, over the
  cell-group span Vecs, with `map_init` for per-task scratch (§2.4).
- The pool's work-stealing order is nondeterministic — which is **exactly why results must not depend on it**;
  they don't, by the compute-pure + order-independent-reduction design.
- The Bevy schedule (`lib.rs:1928–1946`) **stays single-threaded `.chain()`** — system order is the
  determinism backbone (ADR-002/ADR-013). **Do NOT use Bevy's `par_iter()` / multi-threaded executor:**
  (a) it parallelizes at the **system** level, breaking the `.chain()` backbone; (b) the systems write shared
  resources (PoolStock, FlowMatrix, ChemField, Ledger, PoolProvenance) so Bevy's automatic parallelism would
  either serialize them on the resource access graph (no gain) or race them; (c) Bevy query `par_iter` yields
  arbitrary archetype-chunk order, **scrambling the canonical `(cell,species,org)` sort** that every apportion
  AND the hash depend on. rayon-inside-a-serial-system gives intra-system data parallelism over a **pre-sorted
  index space we fully control.**
- Add a **`--no-parallel` escape hatch** (forces the serial path) for differential debugging.

---

## 9. SLICE PLAN

Each slice is **independently revertable** and **independently provable against the hash oracle**.

- **S0 — rayon dep + pool + threshold + escape hatch + ADR.** Add rayon as a pinned workspace dep + a
  persistent global ThreadPool init (`OnceLock`) + the `PAR_THRESHOLD` const + a `--no-parallel` escape hatch
  + record the ADR / inv #7 entry in DECISIONS.md (exact rayon version, `Cargo.lock` pinned). **ZERO call
  sites yet** → trivially hash-neutral, gate green, bench unchanged, `0x47a0` untouched.
- **S1 — diffusion scatter→gather, then parallel.** Rewrite `diffuse_and_decay` SCATTER→GATHER, **STILL
  SERIAL** — self-contained (touches only ChemField, no orgs/FlowMatrix/ledger except the decay sum). Prove
  byte-identical against `0x47a0` with **ZERO threads first** (gather == scatter integer-for-integer; a clean
  hash-neutral commit landing the determinism-clarity win even if diffusion is never parallelized). THEN add
  rayon over dst-cell chunks + per-task decay partial sums merged in chunk order, behind the small-grid guard.
  Bench at 1k/5k/10k; multi-ISA CI green.
- **S2 — metabolism compute/apply split, STILL SEQUENTIAL.** Refactor Pass-1 demand + Pass-2 apportion +
  Pass-3 convert into a pure per-cell-chunk compute fn (writing per-chunk grant/litter/toxin/flow records into
  disjoint slices) and a sequential canonical apply phase. **Verify `0x47a0` UNMOVED with zero parallelism** —
  the riskiest correctness step done with **no threads** so any hash move is a refactor bug, not a race. Bench
  (expect ~flat).
- **S3 — parallelize the S2 compute phase. The big win.** Introduce the per-task scratch (`map_init`) +
  per-task local FlowMatrix records + the sequential-threshold fallback; parallelize over disjoint cell-group
  chunks, keep apply sequential+canonical. **Prove `0x47a0` unchanged + multi-ISA gate + bench at 1k/5k/10k
  (the 1k row must NOT regress).**
- **S4 — parallelize mineralize.** Reuse S3's per-task-scratch + local-FlowMatrix + compute/apply pattern
  (structurally a smaller metabolism). Prove `0x47a0` + multi-ISA + bench.
- **S5 (optional) — permanent parallel diffusion.** Parallelize the S1 gather permanently **if the bench
  justifies it** at the current/larger grid; otherwise leave the gather serial.
- **S6 (deferred, multi-species only) — predation + host_coupling.** (`trophic.rs:777` + `trophic.rs:1114`)
  using the same compute/apply + local-matrix discipline. Hash-neutral on the pinned plant config
  (early-return no-op) but bench + prove on a **separate multi-species fixture**; only land if that fixture's
  hash is stable on multi-ISA.

---

## 10. RISKS + ROLLBACK

**Rollback is trivial and universal: if a slice moves `0x47a0`, revert that slice.** The hash catches every
determinism bug. Each slice is one commit, independently revertable, independently provable.

1. **A single reordered integer accumulation on the hashed path moves `0x47a0`.** MITIGATION: the
   collect-then-sequential-canonical-apply discipline keeps every order-sensitive mutation (pool decrement,
   FlowMatrix, litterfall/toxin cap-routing, Energy/Biomass) in the existing sequence; the only true parallel
   reductions are i64 add (assoc+comm) and the fixed per-cell gather. The pinned hash is the local oracle
   (`check_determinism.sh` runs the seed twice); the multi-ISA CI gate catches latent platform-dependent
   reorders the M4-only run misses.
2. **Rayon pool nondeterminism** (work-stealing schedule + worker count vary run-to-run / machine-to-machine)
   **MUST NOT affect results, and doesn't** by the compute-pure + disjoint-cell + order-independent-reduction
   design. The danger is **only** if a result ever depended on task order (a stray non-commutative reduce, or
   a task reading a neighbour's output). Pin `num_threads` for stable benches; correctness does not depend on
   it; the two oracles catch any accidental dependence.
3. **APPLY-ORDER DISCIPLINE is load-bearing and easy to violate in a refactor.** Cap-spill routing for
   litterfall/toxin/carcass (`lib.rs:1037/1056`), the `(channel,cell)` apportion decrement order, and the
   OrgId-keyed Energy/Biomass apply MUST stay byte-for-byte the current order. The compute/apply split is the
   guardrail; a reviewer confirms the sort keys per slice; **accidentally parallelizing the APPLY (not just
   the compute) would silently move the hash.**
4. **Double-count / miss a quantum at a chunk boundary** if a cell is split across two chunks — PREVENTED by
   partitioning on cell-group boundaries (the sorted vector's contiguous cell runs), **never mid-cell**;
   assert in debug that each chunk starts/ends on a cell boundary. The cell is the apportionment atom (pool
   decremented once per `(channel,cell)`).
5. **False sharing** on the `demand[]`/`granted[]` disjoint sub-slices and adjacent per-task matrices when
   neighbouring `i64`/`[i64;3]` entries share a 64-byte cache line at chunk boundaries — MITIGATION:
   `par_chunks_mut` at cell-group granularity (≫ a cache line) so each task owns a contiguous run; whole-cell-
   group boundaries (already required for correctness) make boundaries rare; pad only if the bench shows a
   hotspot. **Perf-only, never correctness.**
6. **Overhead at low N:** at 1k orgs / sparse cells the rayon fork/join + per-task scratch alloc + collect
   Vecs exceed the arithmetic win (could regress) — MITIGATED by `PAR_THRESHOLD` (~2000) running the proven
   serial path below it; the pinned ~1k config takes the serial path (extra byte-identity guarantee); the 1k
   bench row must not regress.
7. **Hidden shared mutable state:** the World-owned `MetabolismScratch`/`ReproScratch` and
   `PoolProvenance.scratch_w/s/rem` (`trophic.rs:122–129`) are single-buffer and cannot be `&mut`-shared —
   they MUST become per-task scratch; a missed one is a **data race (UB)**. The borrow checker enforces `&mut`
   disjointness; **the design must not smuggle sharing via `RefCell`/`unsafe`.**
8. **Float on the hashed path:** the soil/climate `match_permille` f64 (`lib.rs:742–747`) is the only float —
   quantized once per-org via `to_unit_u16` **BEFORE** the parallel split, so no f64 reduction ever crosses a
   thread; **keep that quantization strictly upstream.**
9. **The gather reflect term** (`chem.rs:357`): `new[d]` must add (count of `d`'s **own** off-grid edges) ×
   (`src[d]>>shift`), **NOT** the neighbours' reflects — verify with a SERIAL-only byte-identity check in S1
   before parallelizing.
10. **Diffusion small-grid trap:** 1024 cells is tiny; a naive `par_iter` dispatch can be net-negative —
    guard with a grid-size threshold or leave the gather serial (S5 is optional).
11. **CI cost/scope + new pinned dep (inv #7):** the whole correctness claim rests on the multi-ISA gate, so
    it must run on **every push** for these slices, building the determinism-feature artifact (HARD asserts)
    hashed cross-arch; a rayon minor bump is a cross-version reproducibility event to re-gate.

---

## 11. GO / NO-GO RECOMMENDATION

**Recommendation: GO — conditional on human sign-off of the invariant-touch (inv #3, #7, #1), and proceed
strictly slice-by-slice with the hash as the gate.**

The case is strong: the single-thread ceiling is genuinely exhausted (~0.85 M org-updates/s, micro-opts at
~0–1%), parallelism is the only remaining multiple-moving lever, and the design isolates **all**
nondeterminism risk into an RNG-free, compute-pure, disjoint-cell region whose only cross-task reductions are
associative-commutative i64 adds — with the pinned hash + multi-ISA gate as a mechanical, mathematical
correctness oracle. The expected payoff (~2–2.5× at 5k–10k orgs) is honest and worth it; the 1k path stays
serial and byte-identical by the threshold.

**What needs human sign-off (STOP-THE-LINE, this proposal is the surface):**

1. **Inv #3 / #7 / #1 touch** — approval to add `rayon` (a new pinned dep, inv #7), to parallelize inside the
   sim systems (the determinism-reduction inv #3 explicitly anticipated), and confirmation that
   MIT/Apache-2.0 rayon-in-binary is acceptable under inv #1's GPL-only boundary rule. **This is the gate.**
2. **The slice ordering & "land-serial-first" discipline** — S1 gather and S2 compute/apply split land
   **serially** and must prove `0x47a0` unmoved *before* any threads are introduced (S1-parallel, S3). Confirm
   this incremental gating is acceptable (it is the whole safety story).
3. **Multi-ISA CI on every push** for these slices (cost/scope), since the cross-platform claim rests on it.
4. **NOT a re-pin** — confirm the expectation that `0x47a0_3c8f_6701_f240` **stays byte-identical throughout**;
   any slice that moves it is a bug to revert, not a ledgered re-pin.

**NO-GO triggers (any one → halt the relevant slice and surface):** S2's serial compute/apply refactor moves
the hash (a refactor bug, fix before any parallelism); the multi-ISA gate diverges (latent platform reduction);
the 1k bench regresses despite the threshold; or a borrow-checker-forced `unsafe`/`RefCell` is needed to share
scratch (re-design, do not smuggle sharing).

---

*Design/research artifact for human review before implementation. No code committed. File:line references
verified against the current tree (`crates/sim-core/src/{lib,chem,trophic,fixed}.rs`). The pinned literal
`0x47a0_3c8f_6701_f240` is the oracle and stays unchanged.*
