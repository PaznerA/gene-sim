# ADR-017: Layered evidence-based ecosystem — multi-fidelity coupling (fast deterministic sim ⇄ deep E. coli background), the earned-edit game mode, the third species, the boundary vector-DB

> **Re-grounding note.** ADR-017 is **ACCEPTED** (see `docs/llm/DECISIONS.md`). The licensing block
> (ADR-018: non-commercial BiGG accepted) and the first pieces have **merged**: the fast 30 FPS abstract
> sim (ADR-013 CHEMOSTAT-J), the Decomposer 3rd species, the measured `FlowMatrix` relations (ADR-014,
> view-only, supersedes the retired fabricated-cosine coupling). This doc is the surviving **design
> reference for the UNBUILT work**: the deep **E. coli earned-edit GAME MODE** and the **multi-fidelity
> precompute firewall**.

## The vision (what we are still building toward)

Three coexisting layers on one engine:
- **L1 — fast abstract sim (MERGED).** The 30 FPS bit-deterministic ChaCha8 core. *Done — see ADR-013.*
- **L2 — deep real E. coli (UNBUILT, the prize).** A real K-12 MG1655 metabolic model edited as an
  **earned game action**, whose ecosystem impact is computed **SLOWLY in the BACKGROUND** and fed back
  into the fast sim through the existing **selection-modifier seam** — never on the hash path.
- **L3 — third species + relations.** A 3rd soil/gut species (Decomposer merged; predator/mutualist
  next) closing the detritus→nutrient cycle; a process-boundary **vector DB** indexing all specimens'
  embeddings for view-only KNN relations (similarity / lineage / synergy / predation).

## The multi-fidelity firewall (the key unbuilt design)

The hard constraint: an E. coli edit MUST change the ecosystem, yet FBA/MOMA LP/QP solves are **not
bit-reproducible** across solver/platform/threads. So the slow deep compute is **always the PRODUCER**,
the fast sim **only ever CONSUMES** a quantized integer committed at a deterministic boundary. ONE-way,
integer-only crossing.

**Precompute-to-frozen-table** is the move that collapses the float hazard: the E. coli **CORE model is
~134 genes**, so the **entire single-gene-KO landscape is ~134 LP solves**. Bake it **OFFLINE** into an
inert, content-hashed, **ORDERED** `Vec<KoImpact>` keyed by `LocusId` (never HashMap):
`{delta_growth_q:u16, exchange_flux_q:[u16;K], essential:bool}`. At runtime a single-gene edit is a
**deterministic table LOOKUP** — the LP solver never enters the game binary. Live boundary FBA is
reserved for novel multi-gene combos and returns through the same quantized channel.

## The 5-stage producer/consumer pipeline (all reusing existing seams)

1. **TRIGGER** (in-core, RNG-free, hash-neutral) — player spends an earned credit, emitting journaled
   `Action::RequestEcoliEdit{species, locus, edit_kind, due_epoch}`. Records the request + when its
   result is due; does not block or alter selection.
2. **DEEP COMPUTE** (boundary subprocess, OFF-hash, async) — `crates/oracle-fba` (an oracle-slim clone,
   inv #1) shells out to a COBRA/FBA CLI: edit → GPR rules → flux bounds → FBA/MOMA → read-outs
   {growth ratio, key exchange fluxes = the L3 mineralization rates, essential flag}. Single-gene = the
   frozen-table lookup; live solves only for novel combos.
3. **QUANTIZE + FREEZE** (the firewall) — float result NEVER crosses into hashed logic. Emit integers
   only via `fixed::to_unit_u16`; content-hash the quantized BYTES. Past here only integers exist, so
   the result is bit-portable even though the FBA was not.
4. **FEEDBACK** (buffer + scheduled commit — kills wall-clock leakage) — the result lands at an
   unpredictable wall-clock time, so it is NEVER applied on completion. The harness writes a second
   journaled `Action::CommitEcoliImpact{content_hash}` tagged with `due_epoch`; the core applies it at
   that epoch boundary in ascending `(SpeciesId, req_id)` order. If the job is not done by `due_epoch`
   the commit **deterministically SLIPS** (the slip is journaled). On REPLAY the impact is read straight
   from `actions.ndjson` — the deep compute is **never re-run**.
5. **GAME MODE = OVERSIGHT** (apply into the fast sim, the deliberate re-pin) — `growth_ratio_q` becomes
   one more strictly-positive `[0.5,1.5]` factor in `selection()`'s product via `EcoliEditModifier`
   (structurally identical to soil/climate modifiers, looked up by GroupId in stable order); the signed
   exchange-deltas become an ordered `ResourceField` tap (detritus→free_nutrient) routed through the
   Ledger's named taps. The `content_hash` folds into `hash_world`. OVERSIGHT is a third GameMode beside
   Sandbox/Mission: a RNG-free, hash-neutral score→credit accrual over the per-gen stats stream earns a
   two-tier edit budget (cheap fast region edits vs the rare expensive deep ApplyEcoliEdit).

**Determinism boundary.** INSIDE the hash: only quantized integers committed at a tick/epoch boundary
via a journaled Action; the content_hash. OUTSIDE: the FBA/MOMA solve, raw fluxes, the subprocess, the
ANN, the wall-clock arrival time, any float before `to_unit_u16`. **Acceptance gate:** the sim hash is
byte-identical whether oracle-fba is present, absent, or returns different bytes run-to-run — until the
operator deliberately commits a value (then a recorded, replayable input); replay never re-runs FBA.

## Status of the slices

- **MERGED / un-gated:** the licensing ruling, the fast sim, the Decomposer 3rd species, the measured
  FlowMatrix relations. See **ADR-017 / ADR-018 in `DECISIONS.md` + the merged code**.
- **UNBUILT (this doc's scope):** `crates/oracle-fba` + the frozen KO-table bake (S2/S3); the OVERSIGHT
  credit economy (S4); the journaled-Action firewall + due_epoch buffer/slip (S5); the load-bearing
  `EcoliEditModifier` re-pin (S6, 🛑 sign-off); the predator kernel (Bdellovibrio, first Interaction
  kernel, KO-ompF→predation-resistant); `crates/relations-index` view-only vector DB (S8).

## Sequencing gate (do not skip)

A structurally-distinct ~134-locus genome cannot EXPRESS correctly until **F2-ontology-rekey** lands
(`gp.rs` hardwires the 9 traits to flat param indices 0..8). VALUE-only species and the earned-edit
**economy** can prototype against a VALUE-only E. coli stand-in **before F2**; the **evidence-complete
deep E. coli cannot**. Each load-bearing wire (EcoliEditModifier, the exchange-flux tap, the
content_hash fold) is a deliberate, ledgered hash re-pin kept selection-neutral on introduction — only
activation re-pins. The predator kernel's prey-contention/visibility (frozen start-of-tick snapshot vs
immediate ordered apply) MUST be pinned as a design decision before impl, not discovered in it.
