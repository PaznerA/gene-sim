---
name: iterate
description: Run one vertical development slice end to end (plan → implement → gate → reflect → commit).
invocation: user
---
Execute the per-slice loop from docs/llm/SPEC.md §7.2 on the top unstarted slice in docs/llm/TASKS.md.
Hard rules (docs/llm/SPEC.md §2.1): GPL stays at the subprocess boundary; no genome logic in godot/;
seeded ChaCha8 RNG only; AI agents at species granularity; pin versions.
If the slice exceeds ~1 day or touches an invariant, STOP and ask the human before writing code.
After IMPLEMENT, you MUST run the gate skill and pass before committing.
End with a 3-line summary and stop, unless the human passed --auto.
