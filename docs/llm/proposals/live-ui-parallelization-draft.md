# Live UI parallelization — verdict: decoupled-single-thread (DRAFT)

> **`live-ui-parallelization-design` workflow (9 agents, adversarially verified). Renderer-side only (inv #2), no Rust changes. UNTESTABLE in-env (no Godot) → Rust replay gate + manual Godot smoke.**

## Why

The single threaded budget loop keeps the interface responsive while the simulation runs fast because it caps the cost of drawing each frame and leaves spare time for the brush and the mouse clicks.


## Mechanism

A per frame callback runs three phases. Input arrives first. Then a bounded loop advances the simulation by whole steps until a time budget is reached. Then a display rate phase reads a snapshot and redraws. The brush stays direct and lands between whole steps.


## Determinism

Everything runs on one thread in call order so the random number stream and the action log are never reordered. Each step advances a fixed whole number and elapsed time only chooses how many steps run, so the recorded session replays exactly and the existing replay check stays green.


## Thread-safety

There is no concurrency at all, so the shared mutable aliasing problem disappears. All simulation and all scene calls stay on the one main thread. The worker thread alternative would have to move many call sites and any miss would be undefined behaviour, while this design moves none.


## Brush/click responsiveness

The engine delivers input each frame before the callback runs, the per frame work is strictly bounded, and the heavy redraw is limited to the display rate, so the brush and the clicks stay responsive at any simulation speed.


## LiveSim (Rust) additions

None. There are no changes to the Rust side at all and the replay contract is unchanged, so the existing replay check proves correctness without modification.


## GDScript edits
- `godot main file` — Add the steps per second target, the step and render carries, and the display rate and budget constants.
- `godot main file` — Keep the timer only for file replay and enable per frame processing for the live mode after the menu closes.
- `godot main file` — Add the per frame callback with the existing pause guards, a bounded step loop, and a display rate redraw call.
- `godot main file` — Move the old live tick redraw body into a publish frame helper, drop its step call, and keep it on the main thread.
- `godot main file` — Change the speed handler to set the steps per second target instead of the timer interval.
- `godot main file` — Zero both carries on reset and leave the brush apply path direct and unchanged.

## Test Plan
- Run the gate script and confirm the pure Rust replay equality test stays green; it is unaffected because the Rust side and the action log are untouched.
- Manual Godot smoke: launch the live mode, raise the speed slider to maximum, and confirm the brush preview follows the cursor and the left click paints with no lag while generations advance fast.
- Toggle pause, the menu, and the view mode and confirm the new callback guard stops and resumes stepping.
- Confirm the timer no longer fires for the live mode, only for file replay.
- Save a live session with clicks at various speeds, run the replay, and confirm the final hash matches a single threaded run.
- Tuning pass: log the steps per frame and the frame time and set the display rate, the time budget, and the maximum steps per frame on the target hardware.
- Regression: confirm the sparklines and the timeline still populate, now sampled at the display rate, and the missions that read the region allele and the observe call still evaluate.

## Risks
- Single thread throughput ceiling: if one simulation step ever exceeds the per frame time budget, for example a future large multi species world, the steps per frame drop and throughput falls, though input stays responsive; a worker thread would then be warranted.
- History and timeline are now sampled at the display rate rather than once per generation, so granularity drops at high speed; this can be mitigated by recording cheap history inside the step loop while only redrawing at the display rate.
- The budget constants need empirical tuning on the target hardware.
- This cannot be validated in this environment because there is no Godot runtime; correctness rests on the Rust replay gate plus a manual Godot smoke, and because there is no thread there is no deadlock to test.
- If the publish frame helper is later moved onto a thread it reintroduces the aliasing hazard, so guard it with a code comment.
- Reset must zero both carries or the first frame after a reset advances an unintended but still deterministic number of steps.

## Open Questions For Human
- Will a single simulation step exceed roughly six milliseconds once the multi species work lands? If so a worker thread is needed later; should this slice leave a clean seam so that migration is easy?
- Is per generation sparkline and timeline granularity required for the missions or the analysis, or is per display frame sampling acceptable?
- Should the new path ship behind a flag with the old synchronous loop as a trusted fallback until the Godot smoke passes, or is a direct cutover acceptable since the determinism gate is the real proof?
- What are the preferred defaults for the steps per second and the budget constants, and what maximum steps per second should the slider target on the reference hardware?
- Does any non live path still depend on the old live tick so that it must be kept as a forwarder, or can it be deleted?
