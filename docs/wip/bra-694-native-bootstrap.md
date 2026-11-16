# Native bootstrap contracts

Issue: https://linear.app/bray-lang/issue/BRA-694.
Draft PR: https://github.com/lejmer/bray/pull/717.
Branch: `feature/bra-694-native-bootstrap-contracts`, based on develop 5de46243b.
Pushed checkpoint: 1430835b0. PR #710 and its prerequisites are merged.
Reviewer: `/root/bra694_review`, published clean review through e9c174764:
https://github.com/lejmer/bray/pull/717#pullrequestreview-5914521927.
Later changes still require official incremental review.

## Contract

Make supported Linux and Windows bootstrap products check and lower for x86_64 and AArch64.
Preserve shared fields, disjoint owners, source/imported contracts and exact negative rejection sites.
Reject fabrication, replacement, previous epochs, actual raw mutation and retirement. Compiler defects panic.
Execution and shutdown are BRA-695 and BRA-696. Performance is separate work.

Trace all equivalent producers and consumers before semantic edits. No historical reconstruction, freshness from
owned_allocation, exact cells from finite alternatives or region bounds, layout from alias bounds, predicate
reassertion or blanket loan exemption. Preserve conditional masks and authoritative empty ownership records.
Stop on unexplained fixed-suite regressions. Keep coherent good changes even when another defect remains.
Commit/push checkpoints and publish independent PR reviews. Style remains deferred until finalization.

## Current mechanisms

- Physical ownership uses current whole-value storage paths, segmented Uninit/OwnedTarget traversal and complete
  cleanup effects. Retain actual types and AnyElement bounds. Do not project layout onto inexact Returned bounds.
- Static host storage can also come from an admitted external View.host pointer. A BINDING lifetime anchor does
  not identify that external allocation. Preserve all actual origins rather than only the local static arm.
- Six authorized bodyless OS leaves preserve Bray storage: pthread_self, pthread_getspecific, GetCurrentThreadId,
  FlsGetValue, GetLastError and SetLastError. Imported summaries and real bodies take precedence. Calls stay impure,
  pointer results independent, and foreign-call capability nontransitive. Retaining setters, callbacks, output
  writes, FLS destruction and allocation retirement keep their actual effects.
- Anonymous owned discards clean up at scope exit. Nested child blocks finish owned cleanup before releasing
  dependent runtime resources. ArenaAdmission encloses an inner cleanup block so its exit runs after those locals.
- Native readiness, composition, std tests, smoke and performance preparation reuse canonical per-target std/runtime
  artifacts. Carry the builder's Package inventory. Explicit and pinned inputs stay exact. Avoid competing artifact
  configurations. Native toolchain processes run outside the sandbox.

## Finite typed assignment repair

The original untyped Write in publish_event_wait came from state.head/state.tail assignments. The first has two
current exact Entry targets for Event and ExecutionEvent, both declared State. The single-target resolver rejected
that finite choice and published/applied a global mutation. That erased state before the second assignment.

The current batch carries finite typed target sets through stored paths, calls, assignments, effect publication,
observations, terminal checks, borrows and physical writes. Strict one-target proof queries remain strict. Unknown,
displaced, previous and region-only references cannot supply layout. No synthetic Returned identity replaces a set.

Evaluate the RHS once and capture before mutation. Ambiguous or dynamic assignments mark pending outputs weak.
Completion reuses the existing raw-write weak commit, preserving possible dependencies without initializing every
candidate or installing a definite pointer at each. Pending joins retain weakness monotonically. No state cloning or
new alias representation. Net growth before final formatting: 65 production Rust lines and 102 test lines.

Oracles extend existing tests: direct branch and call-return choices publish both bounded writes, disjoint facts
survive, either overwritten candidate loses its old fact, and weak staged writes initialize neither candidate.
Reconsider on authority widening, strong updates to alternatives, loop instability, count growth or regressions.

## Evidence and next action

- Current batch: all 563 checker tests pass in 240 ms, all 57 returned-dependency tests in 2.55 seconds. This includes
  the new finite-choice controls, imported contracts, terminal ownership and negative raw/owner cases.
- Expanded platform source/import controls pass in 390 ms. Protected nested cleanup passes in 4.90 seconds,
  including incoming raw reads through returns, nullable record buffers and exact negative sites.
- Both normal native controls still fail in 39.79 seconds at dispatch_ready/context(view_owned) and
  start_frame/commit_start/scoped-exit authority. These observations are unchanged. Do not claim completion.
  Trace remaining effects against the protected marker and real owner graph before another semantic patch.
- Earlier current controls: all 62 catalog and 136 bound-tree tests pass. All 27 named production probes pass in
  16.92 seconds with one immutable compilation and a CPU-sized worker pool. First cold dependency check was
  10.25 seconds. Probes only check/lower source, without executing native products.
- Frozen 1,718 baseline on 1ceb765eb: three regressions and seven panic timeouts. The three pass focused checks
  after 1440b142a. The corrected broad comparison remains unrun. c606aaf83 removes four redundant provider-test
  identities, replaced by actual native product gates. Record that ledger explicitly, never count removals as passes.
- Five artifact-reuse controls and thirteen boundary/totality/cache controls pass. Actual native product gates remain
  unverified. Resolve test binaries from Cargo's emitted paths rather than retained filenames.

No temporary tracing remains. Build and focused output retain only current evidence. Required remaining gates:
four-target actual checking/lowering and event layout, corrected frozen comparison, owned/deferred/projection/cleanup
suites, fixed-five/loop counts and timings, maintainability/DRY audit, mutating style and Bray formatting.
Update the final PR description and move PR/BRA-694 to Review only after acceptance, verification and published review.
