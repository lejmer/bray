# Native bootstrap contracts

Issue: https://linear.app/bray-lang/issue/BRA-694.
Draft PR: https://github.com/lejmer/bray/pull/717.
Branch: feature/bra-694-native-bootstrap-contracts, develop base5de46243b.
Pushed:29d407094. Reviewer:/root/bra694_review.
Verified published review through84a377c79:
https://github.com/lejmer/bray/pull/717#pullrequestreview-5918123029. No findings.

## Required outcome

Check/lower actual Linux and Windows bootstrap products on x86_64 and AArch64. Preserve source/imported contracts,
exact negative sites, conditional masks and authoritative empty ownership. Reject fabrication, replacement, old epochs,
actual mutation and retirement. Internal defects panic. BRA-695 execution and BRA-696 shutdown remain separate.
Performance is separate except shared setup reuse. No historical reconstruction, freshness from owned_allocation,
layout/exactness from alias bounds, predicate reassertion or blanket loan exemption. Stop on unexplained regressions.
Commit/push coherent checkpoints. Style remains deferred until finalization.

## Established comparison and mechanisms

All1714 retained frozen identities pass on the current pending-state source batch:1712 in the grouped/independent
run and the final two slow cases pass75.40s together without other test contention. No reported failures. Baseline1718 identities on1ceb765eb stays fixed. Four c606 provider-test removals are
open replacements with actual native product gates, never passes. Latest published review covers the semantic repair.

Logical-only physical transitions retain prepared descriptors and keep all candidates whenever physical tracking is
needed. Finite assignments capture the RHS before weak commit, never initialize every candidate or fabricate exact
identity. Observation suffixes require agreement across overlapping targets. Closed alias bounds confer no layout.

Ambient roots exclude only original mutable entry borrows at layer1 with empty reference path and ordinary inline
projections. Shared/nested/deeper/protected/backing origins remain conservative. Explicit escapes and raw aliases still
traverse current stored edges. Closed Returned bounds use the same root policy. Direct opaque [Ambient,Escape]
remains unbounded. Await/private-owner, scoped-exit negatives and raw/source/import controls all pass.

Repeated exact storage-subtree walks now share one helper, preserving selector-overlap and retirement rules separately.
Net production reduction12 lines, checker563 pass190ms and returned-dependency57 pass2.84s. Catastrophic-abort test
keeps all four source variants/assertions in one compilation, reducing isolated time43.12s to12.02s and peak576.8MiB
to389.4MiB. No full comparison rerun is needed for these behavior-preserving consolidations.

## Active pending-state contract

The native trace found Domain.arena.pointer correctly seeded before take_ready but erased before retain_task by a raw
Context.pending write that could alias Domain. Source retain_task/retain_locked preserve the deep backing route.
Do not restore entry contents or add demands. Replace the raw access pattern at its owner.

- Current owns a counted view, not exclusive mutation. Context.pending is now existing SpinLock<Observation?>.
  Both constructors create the lock once. Remove pending_storage and all raw slot read/write callers.
- context/view_context publish lock validity through the existing typed Context borrow, as Domain does. No new
  validity lambda, protocol predicate or wrapper type. Producer proofs must pass directly.
- pending_observation returns a copied observation/empty choice after its guard ends, never an escaping borrow.
  It no longer claims pure/total execution. Both acquisition writers assign under short guards after scheduling.
  take_ready copies and clears under one guard. Failure recording uses the copied snapshot.
- Never hold guards across scheduling, callbacks, task retention, dispatch or failure recording. Normal/panic/cancel
  cleanup releases them. The guard retains the actual Context view. Retirement keeps the zero-view requirement.
- Direct checks exposed missing lock validity at the retirement Context move. The current batch factors the existing
  raw admission into owned_context(ExecutionOwner)->&Context, reused by borrow_execution and retirement. It publishes
  actual typed resource conditions without a new axiom. Retirement captures the actual address during inspection,
  then moves after inspection ends. Do not acquire a counted Current merely to recover a predicate.
- Preserve borrow_execution's exposed-address pointer publication. Its counted Current owns a view independently of
  the temporary ExecutionOwner borrow. The typed helper is for access/counting and retirement. Callers include binding,
  closing, native provider, publication, worker creation and current-worker return. Trace these handoffs too.
- Context layout is private and computed from its Bray type. No native size table or ABI shim changes.

Oracles: expanded existing native gates check15 actual producer/access/writer/retirement/composition bodies per target,
with body-start/finish timing. Exact module lookup shares one test helper with observer callbacks. Source/imported
lock origins, loop cleanup, escaped-guard rejection and nested raw invalidation stay fixed. Reconsider if predicates
must be reasserted, values reconstructed, authority widened, or ownership/lifetime handoffs change. Production growth
must stay under20 lines, and the current source batch instead removes code.

## Current evidence and unresolved paths

- Pending-state batch before owned_context: constructors, context/view_context, snapshot/take, both acquisition
  writers and failure recording check/lower on Linux/Windows. Native gates48.01s still fail at dispatch_ready's later
  recovery/context call, start_frame/commit_start and scoped exit, and retire_driver's pending-lock validity.
- Guard-origin, loop-cleanup and exact-module observer controls3/3 pass44.96s. Current owned_context/retirement
  refinement is built and both helpers pass directly. Retirement still loses validity across the raw whole-context
  move, so the lifecycle contract remains open. Keep whole-context destruction rather than a manual field teardown list.
- Fixed-five timings on the fresh executable:471/451/456/459/461ms. Node counts have not been remeasured. Retain the
  count baseline and instrument only once for the final frozen source. User resource contention can affect timings.
- Current checker policy evidence:563 pass220ms, async composition and invalid variants plus scoped exit/tag controls
  pass8.27s, exact opaque negative40ms, returned57 pass3.50s, platform2 pass440ms, protected nested cleanup7.26s.
- All27 production probes previously pass17.29s, first cold dependency10.445s. Catalog62, bound-tree136, artifact reuse5,
  boundary/cache13 pass. Actual four-target native product gates remain open.

Remaining consumer gap: stage_reference InputRegions and stage_reference_value both flat-project target_projections,
including OwnedTarget, instead of following current nested backing through stored_locations. This is not permission to
recover layout from inexact regions. Trace the authoritative prepared relation and every consumer before changing it.
BoundCallReference currently keeps relation/addressed_type but no prepared target path. No semantic patch started.

Ignored evidence keeps one current output per purpose. comparison-baseline.json stays frozen. comparison-current and
comparison-pending record the complete retained comparison across the two code states. Old marker/source-summary
and composition traces were removed. No diagnostic-only compiler branches remain. Test executables come from Cargo's
emitted paths. Native subprocess workflows run outside the sandbox. Never clean shared Cargo storage.

Next: checkpoint/push this source batch with its open retirement proof, then publish incremental WIP review.
Trace moving Read/VolatileRead at Attempt: exact physical target, surviving valid fact, pending result/normal snapshots,
then validity immediately after completion. observe_memory_read only captures a moved snapshot when
StorageTarget.value_observation_path exists, and that method correctly rejects Entry Address origins. Confirm the actual
target before editing. Equivalent consumers are borrowed_value, observe_references, refresh_returned_references,
refresh_borrowed_arguments and observe_memory_write. No general current physical-target-to-proof-value resolver exists.
A new association must retain an actual current typed snapshot at exact view publication and invalidate/consume it with
that physical storage. Never synthesize owner.resource contents or reuse historical address_of arguments. Capture before
consumption, publish only on valid completion, preserve second-read rejection. Temporary tracing has not been added yet. Continue the outstanding native consumers with the contract gate.
Final gates: actual four-target check/lower/event layout, affected owned/deferred/projection/cleanup suites and fixed
comparison after material changes, fixed-five/loop counts, maintainability/DRY audit, mutating style and Bray format,
fresh native compiler products, final PR description and published review. Only then PR ready and BRA-694 In Review.
