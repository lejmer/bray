---
name: debug-bray-end-to-end
description: >
  Use for Bray compiler, runtime, standard-library, native-product, and test failures, including panics, generic
  diagnostics, stale toolchains or artifacts, long rebuild loops, native execution mismatches, and choosing the cheapest
  faithful reproduction or rerun path. Do not use for isolated edits with no failing cross-layer behavior to diagnose.
---

# Debug Bray end to end

1. Thoroughly map out architecture: Begin by building a map of the entire architecture (if one does not already exist in the session). Keep this updated as you change things. This map will be your shortcut for understanding how things fit together.
2. Use the architectural map to plan your traces, then add probes to trace the bug(s). Some bugs are local, others can span wide chunks of the codebase. Do not assume that just because you find a boundary that looks wrong, that this is the only place that matters, as many bugs can span many boundaries simultaneously.
3. Once you have located the boundaries where the bug(s) is introduced, and traced how the bug(s) affects down stream consumers, apply the right fixes at the right places to fix the issue end-to-end, which often means to not only fix it at the earliest boundary.
4. Once you have fixed the bug(s), make sure there are tests that you can use to test if it solved the problem. If the tests are lacking, fix that too. Then verify the bug(s).
5. Look out for unintended consequences and possible regressions, and fix anything that broke that you missed.
6. Remove any tracing code and/or probes you might have added to help you trace the problem through the code.
7. Let an independent subagent review your fix(es). Fix any findings that reviewer finds. Do not give the subagent your context, only give it the original problem and tell it you have solved it. Let it discover what you did on its own, so that it cannot be biased.

Keep your findings in a record. Do not keep stale records, builds or probes around.
