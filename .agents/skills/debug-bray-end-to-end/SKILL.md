---
name: debug-bray-end-to-end
description: >
  Use for Bray compiler, runtime, standard-library, native-product, and test failures, including panics, generic
  diagnostics, stale toolchains or artifacts, long rebuild loops, native execution mismatches, and choosing the cheapest
  faithful reproduction or rerun path. Do not use for isolated edits with no failing cross-layer behavior to diagnose.
---

# Debug Bray end to end

Find the first broken contract with the smallest faithful producer and fewest expensive runs.

## Freeze the observation

Checkpoint the exact command and failure, source or build identity, selected product, and output root. Preserve exact leaf errors before wrappers replace them. Put verbose manifests, inventories, and logs only in a verified, ignored, task-owned directory inside the repository, never HOME, a repository parent, system temp, or another external path. Load only what the current decision needs.

Prove that the invoked compiler, toolchain, and artifacts match the source, target, profile, runtime ABI, and standard-library build under investigation. Use paths plus identities, digests, or source fingerprints. Timestamps are insufficient. Do not debug an unverified product mixture.

## Find the first failing layer

Locate the earliest boundary that can explain the observation:

1. parsing, binding, checking, or checked-fact publication.
2. lowering, MIR validation, or generated-helper realization.
3. code generation, artifact selection, emission, or linking.
4. runtime startup, ABI role resolution, or platform binding.
5. standard-library behavior.
6. test discovery, native host execution, protocol handling, or result reporting.

Maximize uncertainty removed per expensive run. Narrow through code, data flow, logs, and retained artifacts first. For costly runs, batch independent noninterfering probes or bisect boundaries. Use one-variable reruns only for necessary causal isolation after narrowing. Track only changed hypotheses. Do not search boundaries serially when one instrumented run can find the first divergence.

For cross-layer concepts, trace the authoritative declaration through closed inventories, enums, text identities, ABI symbols and signatures, artifact ownership, compiler mappings, serialization, validation, and consumers. Prefer one authority with exhaustive projections over parallel lists.

## Keep the loop cheap

Escalate from a focused boundary test to a minimal real compiler/runtime/standard-library product, selected integration test, affected conformance group, then broad suites. Keep the smallest real producer and consumer. Do not replace them with mocks.

Retain products and intermediates. When inputs and identities are unchanged, validate freshness and rerun the selected test or published executable without rebuilding. If no no-build command exists, use the retained product directly when faithful. Never add a production bypass for missing debug tooling.

For an expensive command, add elapsed time, exit status, relevant identities and paths, and the leaf failure to the compact checkpoint. Keep full logs in the repository-local task directory, load only needed slices, and report its path at handoff. Never remove logs with `rm`, `Remove-Item`, `rmdir`, `del`, filesystem scripts, or equivalent raw deletion. Repeat only when changed inputs or unresolved hypotheses can make the result informative. Do not narrate routine metadata or reasoning. Never rerun a broad suite whose unchanged inputs make the result predictable.

## Preserve exact failures

Trace `InfrastructureFailure` and other generic results to first detection. Preserve the narrow cause and typed context through `map_err`, `ok_or`, protocol, artifact, and diagnostic conversions. Retain enough identity to retrieve failure-only context lazily.

For cleanup, lifecycle, runtime-role, and generated-helper failures, verify the producer's totality contract before patching its final consumer. Keep partial recovery facts distinct from complete codegen-ready facts and repair the owning boundary.

## Verify the repair

Run the focused regression, adjacent boundary coverage, affected conformance group, then broad suites if needed. Finish with repository checks and the `AGENTS.md` audit.

Report concisely: first broken contract, freshness proof, missed earlier validation, focused coverage, retained-product path, and any Linear-worthy tooling gap.
