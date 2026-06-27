---
name: roadmap-plan
description: Fill the workflow queue (docs/llm/QUEUE.md) for looped development. Survey the roadmap + the real frontier state, then keep ≥5 robustly-defined, ready-to-run workflows queued in priority order — authoring/refreshing the .claude/workflows/*.js as needed. PLANS ONLY — never writes production code. Run when the queue drops below the floor, after a direction change, or to re-prioritize.
argument-hint: "[--min N] [epic/thread to lead]"
---
Keep the **workflow queue** (`docs/llm/QUEUE.md`) stocked with ≥N (default **5**) robustly-defined, ready-to-run
workflows so `/roadmap-iterate` always has something to pop. This is the "plnění zásobníku" half of the loop;
`/roadmap-iterate` is the consumer. Honors SPEC §2.1 invariants and the `autonomous-roadmap.md §0` guardrails.

## Procedure
1. **LOAD (read-only).** SPEC §2.1 invariants; `docs/llm/autonomous-roadmap.md` §0 (guardrails) + the live
   frontier; `docs/llm/TASKS.md`; the latest pins in `DECISIONS.md`; `git log --oneline -25`; the **current
   `QUEUE.md`**. Determine, per active thread, what is genuinely **DONE** vs the **next unstarted** unit (cite a
   commit/changelog as evidence — the roadmap prose lags the code; trust git). When unsure, spawn an `Explore`
   agent to map a thread rather than guessing.
2. **PRIORITIZE.** Order by: (a) unblocks a ship/beta blocker, (b) advances the chosen lead epic
   (`$ARGUMENTS`, else the roadmap's current thrust), (c) hash-neutral & dependency-free first, (d) atomic or a
   *precisely-sequenced* chain (B→C→D where each builds on the last). Never queue past a hard dependency.
3. **DEFINE each entry robustly.** For every queued item write: `name` · `driver` (`workflow`=a `.js` |
   `slice`=one implementer+gate+reviewer pass | `direct`=trivial inline edit) · one-line **goal** · **hash-risk**
   (✅ neutral / 🔁 re-pin / 🛑 invariant) · **deps** · **acceptance** (the gate + the verify claims) · `status`
   (`[ ]`/`[~]`/`[x]`/`RED`). A 🛑-invariant or *undesigned* re-pin item is **queued but FLAGGED** — it stops the
   line for human sign-off; do not let `/roadmap-iterate` auto-run it. (A *designed/reviewed* re-pin is executed,
   not staged — `[[repin-execute-not-stage]]` — but is multi-ISA-aware: the local gate proves same-build
   reproducibility, the CI matrix proves cross-arch.)
4. **AUTHOR / REFRESH the `.js`** for `driver: workflow` entries that don't yet have a ready script, in the
   **house style** below. Confirm any pre-existing script still matches the current code surface (file:line
   pointers drift) — if stale, fix the READ-FIRST anchors. A queued `workflow` MUST be **tracked in git** (an
   untracked `.js` won't exist on `main`, so `/roadmap-iterate` can't find it).
5. **WRITE `QUEUE.md`** — ordered table + per-entry detail; record the queue depth and the date.
6. **CLOSE.** Report: queue depth (must be ≥N), the next runnable item, and any 🛑/🔁 flagged for sign-off.

## Workflow `.js` house style (matches `.claude/workflows/*`)
```js
export const meta = {
  name: '<name>', description: '<scope + ADR/epic + crates touched + the hash-neutrality argument>',
  whenToUse: '<roadmap context>', phases: [{ title: 'Impl' }, { title: 'Gate' }, { title: 'Verify' }],
}
phase('Impl')
const s1 = await agent(`Implement <slice>. READ FIRST: <file:line anchors>. Honor inv #2/#3. ...
  VERIFY the pinned literal 0x47a0_3c8f_6701_f240 is UNMOVED (read-only ⇒ hash-neutral). Do NOT commit. Report ...`,
  { label: 'impl', phase: 'Impl', agentType: 'implementer' })
phase('Gate')
const gate = await agent(`Run \`bash tools/gate.sh\` (~15 min). determinism MUST stay 0x47a0... Report every PASS/FAIL.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' })
phase('Verify')
const VSCHEMA = { type: 'object', required: ['<claim>', 'issues'], properties: {
  '<claim>': { type: 'boolean', description: 'inv #X: <specific>' }, issues: { type: 'array', items: { type: 'string' } } } }
const skeptics = (await parallel([0,1,2].map(i => () =>
  agent(`Adversarially verify <slice>. Default each boolean FALSE unless confirmed. Hunt: <threats>. Do NOT edit.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' }))) ).filter(Boolean)
const tally = k => skeptics.filter(s => s[k]).length
const keys = ['<claim>']; const confirmed = keys.every(k => tally(k) >= 2)
return { gate, tallies: Object.fromEntries(keys.map(k => [k, tally(k)])),
  all_issues: skeptics.flatMap(s => s.issues || []), verdict: confirmed ? 'CONFIRMED' : 'NEEDS WORK' }
```
**Rules:** the workflow **does NOT commit and does NOT itself run `git`** — its Gate phase *instructs* the
gatekeeper to run `tools/gate.sh`, and it returns a structured `{ verdict, tallies, issues }`. The **caller**
(`/roadmap-iterate`) commits + merges. Implementers READ the real surface first (anchors drift — point at
files, not just line numbers). Renderer/`godot/*.gd` work is hash-neutral by construction (inv #2); core work
must prove the pinned literal is unmoved or be an explicit, designed 🔁 re-pin.

## Hard rules (SPEC §2.1)
PLANS ONLY — never write production Rust/GDScript here (only workflow scripts + `QUEUE.md` + planning docs).
GPL stays at the subprocess boundary; no genome logic in `godot/`; seeded ChaCha8 only; species-granular agents;
pinned versions. State lives in `QUEUE.md` + git → resumable.
