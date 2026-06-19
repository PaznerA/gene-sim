---
name: slice-done
description: Close a completed, gated slice (ADR if needed, changelog, conventional commit, mark done). Run only after tools/gate.sh is fully green.
---
Preconditions: `tools/gate.sh` is fully GREEN (run the `gate` skill first).
1. If a load-bearing decision was made, append an ADR to docs/llm/DECISIONS.md (context, decision, consequences).
2. Update CHANGELOG.md.
3. Conventional commit (feat/fix/docs/refactor/test/chore), one slice per commit. End the message with the
   `Co-Authored-By` trailer.
4. Mark the slice done in docs/llm/TASKS.md; surface a 3-line summary.
