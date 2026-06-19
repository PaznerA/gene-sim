---
name: slice-done
description: Close a completed, gated slice (ADR if needed, changelog, conventional commit, mark done).
invocation: user
---
Preconditions: /gate is fully green.
1. If a load-bearing decision was made, append an ADR to docs/llm/DECISIONS.md (context, decision, consequences).
2. Update CHANGELOG.
3. Conventional commit (feat/fix/docs/refactor/test/chore), one slice per commit.
4. Mark the slice done in docs/llm/TASKS.md; surface a 3-line summary.
