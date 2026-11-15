# BRA-567 current work

Branch: feature/bra-567-async-root-orchestration. PR: https://github.com/lejmer/bray/pull/710.
Continue through implementation, verification and review. Checkpoint pushes and bounded matrices are authorized.
Performance optimization is deferred. Versions remain 1 / 1.0. Use multiline control-flow bodies.
After restart, read this record and the active worktree's work-on-bray-issues contract gate.

## Current batch and evidence

Baseline: 74b79209f, with 1,717 fixed comparison identities passing. Preserve identities and rejection locations.
Retained reviewer: /root/pr_review. Last formal reviewed commit: 74b79209f, review 5910316653 verified.
The current repair batch still needs formal PR re-review after its checkpoint is pushed.

Current batch passes all 561 checker tests and 24 focused integration controls. Those include actual observer
poll/destroy on Linux and Windows, all 13 projection cases affected by the removed publication experiment,
source/import witness transport, nullable extraction, raw retirement and allocation disposal. No temporary
tracing remains. Mutating style and format pass. Fixed five case bodies are 307-344 ms and loop is 345 ms.
Node counts were not instrumented. The previous loop baseline is 1,485,955 comparisons / 38,484 nodes.

Next: run the fixed 1,718-identity comparison on this batch. Its current retained report describes the earlier
failed experiment, not this repaired code. Keep four whole-provider checks isolated from ordinary proof shards.
Use bounded groups, preserve every source/import and negative case, and stop migration on regressions.
Do not repeat broad verification before explaining failures. Current binary fingerprints are in target/audit.

Native execution/archive/link remain unverified. The last full native run stopped in source checking. The
bounded service probe cleared native_timeout_prepare but retained owner_entry_scope and dispatch_ready
marker failures plus reserve_start's registration postcondition. Refresh them after comparison passes.
Bound-tree's 136 tests remain valid for unchanged code. No implementation completion is claimed.

## Repair contracts

RawAllocation is an ordinary record. Its actual pointer field carries allocation backing through formation,
move/call, destructor effects and terminal deallocation. Prepare that checked field/type once. Raw pointers
and RawBuffer keep their established behavior. Unknown pointers remain conservative. owned_allocation does
not imply freshness. The allocation control retains an unrelated allocation and rejects the retired pointer
at its final observe. Do not infer allocator freshness from a trusted predicate.

Every versioned inline projection uses explicit Projection nodes over its epoch-qualified root, including
compact Input paths. Stop at Borrowed identities and retain mutation/field identity. Explicit preserved
overrides remain. Join rewriting, selector expiry, borrowed-slot invalidation and numeric field lookup consume
the same representation. Never equate pre/post contents, unrelated fields or separate mutations. Existing
loop-slot coverage checks a changed versioned field and its unchanged sibling.

Stored-value capture stages actual subtrees. An aggregate/scalar root is a replacement marker, not a pointer.
Only typed reference slots carry reference uncertainty. Raw reads use the same rule. Missing alternatives
remain unknown and whole-source lineage stays separate. Assignment fallback replaces the old subtree using
its planned reached_type and cached outermost reference shape. It must discard old bounds, not weakly merge
them. Incomplete generic shapes retain whole-lineage uncertainty. Root/field raw-read coverage preserves real
loans without invented aggregate aliases. No new graph, source reconstruction or reference exemption.

RetainedSnapshots recognizes an unchanged borrowed field through its actual retained current owner. The
same contents rule is shared with ObservationSet. Ownership queries stay unchanged. Scalar copies cannot
retain a source loan, replacement cannot retain old contents, and a retained loan cannot grant ownership.
Existing temporary-loan coverage includes versioned unchanged/replaced fields and actual-owner retirement.

Yielded values remain in their authoritative pending transfer until normal ResultCompletion. Yield initialization
and completion have distinct liveness boundaries. Early publication makes cleanup-free descriptors eligible
for last-use removal before the join and breaks established projection composition.

Failure cleanup acquires the produced pending value at receiver preparation. The exact cleanup root must equal
storage.value_slot(producer), and the producer must be a result region. This excludes old local replacements,
With inputs, scoped capabilities and unproduced call outputs. Consume that transfer once before conditional
memory snapshots. Lifecycle effects, opaque cleanup and cancellation then read the same current contents.
Skipped parts leave pending state alone. Later parts cannot replay the consumed transfer. Ordinary publication,
liveness, joins and normal-ensures timing retain their prior behavior. No overlay or historical reconstruction.

Growth is bounded by one prepared allocation field, shared projection/contents rules, typed replacement and
cleanup admission through the existing transfer mechanism. Reconsider if another representation, reconstructed
history, authority widening or source-name exception appears. Test setup shares integer_expression_unit across
27 callers. Preserve the user's formatting edits and readable inline Bray fixtures.

## Design invariants

Producer owns native retention and exact inactive-exclusive authority. Root acquisition clears its mutable ABI
input. Restored address bits cannot reacquire authority. PreparedStart owns the exact Producer it inspected.
Pending Request owns Producer, continuation and disposition before current(). Composition carries the real
parent Cursor and origin. begin_child consumes/returns complete Composition with empty-producer and parent
selection postconditions. Do not reassert predicates after callbacks.

Trace actual producers and consumers through ownership, storage flow, liveness, lowering and cleanup.
Production precedes consumption. Moves retire old authority. Loops stabilize. Explicit empty ownership layouts
remain authoritative. Tags do not preserve payloads, and identity does not preserve overwritten contents.
Shared references in mutable holders preserve their slot across sibling writes, with no shared raw exemption.
Keep previous epochs and replacements excluded. RawBuffer contents retain actual payload dependencies, including
remaining contents after Pop. Preserve deferred admission d302124 and immutable proof graphs. Internal invariant
failures panic at detection. Source diagnostics retain the specific structured cause.

## Remaining gates

Verify root/main authority, child progress and direct await without another TCB. Root lexical cleanup must precede
terminal publication on normal/panic/cancellation, rollback must run exactly once, incidents and result disposal
must precede services, and services must survive selected static disposal. Preserve synchronous/no-optional-resource
controls. Use async-ownership-composition.bray and runtime-root-smoke.c. Native archive/link evidence must establish
that Rust root/shutdown orchestration is absent. BRA-507 owns the suspended host/static fallback using the same
continued-execution owner. BRA-559 owns test-host policy.

Complete affected interface/source/native gates, maintainability review and formal PR re-review. Only then mark
PR ready and BRA-567 In Review. Do not close before merge.

## Operations

Use optimized tooling. CARGO_TARGET_DIR and CARGO_BUILD_BUILD_DIR point to the main target/cargo. Tooling enables
incremental compilation. LLVM is target/toolchains/llvm/active, version 22.1.8. Native Rust uses installed
1.98.1-x86_64-pc-windows-msvc because default 1.99 emits LLVM 23 bitcode. Native toolchains run outside the sandbox.
Bound expensive work, observe progress and preserve leaf failures. Reuse evidence for unchanged behavior.
Replace stale task logs instead of accumulating history. Never clean shared storage or interrupt editor work.
Run style at batch finalization and format Bray. Inline fixture formatting needs manual inspection.
