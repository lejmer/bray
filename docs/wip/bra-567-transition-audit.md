# BRA-567 current work

Branch: feature/bra-567-async-root-orchestration. PR: https://github.com/lejmer/bray/pull/710.
Continue through implementation, verification and review. Checkpoint pushes and bounded matrices are authorized.
Performance optimization is deferred to a separate PR. Versions remain 1 / 1.0.
After restart, read this record and the active worktree's work-on-bray-issues contract gate.

## Current batch

Pushed head: 7c684322b. Retained reviewer: /root/pr_review. Formal re-review 5909639379 is verified.
Both prior findings are resolved. The current uncommitted batch has read-only reviewer scrutiny, but still
needs checkpoint publication and formal re-review on the PR.

Three related contracts now have concrete repairs:
- Owning extraction follows its prepared input reference path to CURRENT storage before staging descriptors.
  BoundReturnedValue carries that immutable path. Borrowed returns keep their descriptor route. Existing
  guaranteed-transfer and preservation filters remain. Every possible source must supply metadata before
  values merge. Wrapper adaptations remain explicit. Shared projection conversion replaces duplicated mapping.
- Whole owning return preserves exhaustive ordinary scalar/address aggregates, including nested aggregates.
  The existing Shape walk records complete returned-value paths. Borrowed, atomic, opaque and RawBuffer
  backing contents are excluded. Returned observations pass through borrowed-boundary filtering and portable
  partial-preservation masks. Physical invalidation classification is unchanged. This removes synthetic
  changed contents for unchanged ordinary owners without reconstructing their evidence.
- Cleanup expansion preserves the incoming ambient classification when its component route becomes unknown.
  RawBuffer disposal, unportable cleanup projections and lifecycle receiver remapping share unbounded_scope.
  Ambient disposal must not become caller-reaching retirement. Existing caller-reaching effects remain so.
  The diagnostic probe located the loss in scheduler initialize/destroy_reference and reserve_start. There
  were no deep-entry retirement publication events. No backing-route reconstruction was needed.

Reconsider on any conditional-to-unconditional transfer, retained overwritten/reached state, borrowed authority,
previous-epoch recovery or new invalid acceptance. Growth is limited to one existing record's prepared path,
shared conversion, exhaustive shape paths and one cleanup-scope helper. No new language rule, cache or public type.

## Verification

The fixed comparison completes all 1,717 identities with zero failures or omissions. comparison-current.json/log
contain exact test outcomes and binary/source identities. It includes the source/import matrices and the two
added shared-slot/native-acquisition controls. All 559 checker tests pass. Bound-tree's 136 tests passed the
prepared-path change and remain unchanged by the latest private checker changes.

The expanded raw-extraction fixture passes whole-pointer-owner forwarding, empty source preservation across a
callback, and rejection of callback-invalidated, fabricated and duplicate owners. The existing report test now
includes start_frame and passes its mandatory scoped cleanup after reserve_start. Both nested begin_child
postconditions pass. The four isolated production checks pass in 174-190 seconds within 360-second bounds.

Fixed five and loop pass. The loop was 228 ms during concurrent work. Current counts were not measured.
counts-current.json retains the last instrumented loop: 1,485,955 comparisons / 38,484 nodes. timing-current.json
retains control identities. The ignored counter patch is not applied. No memory leak has been established.

Style and Bray format pass. Final raw-extraction rejection-location check passes in 0.24 seconds. The fixed
comparison predates only expression layout, avoiding excluded-path clones and stricter test assertions.

Native checking fails after 308.92 seconds, before execution/archive/link evidence. native-current.json/log
identify the inputs. Nine E7123 occurrences group into borrowed owner markers (4), allocation authority (2),
observer preparation (1) and neighboring links (2). One independent E7135 loses registration presence in
reserve_start after demand_lane. Sixteen more E7135 occurrences depend on failing callees. The filtered
production report test does not certify these other bodies or all execution properties.

Next contract investigation: trace physical marker preservation and resolved raw-effect targets across
ensure_deadlines/ensure_workers/demand_lane/origin_driver_startup, plus the disjoint registration field in
reserve_start. If physical markers survive, inspect refreshed logical borrowed observations. Do not patch
until the first divergence and equivalent paths are established. Preserve both current marker positives and
reachable mutation negatives. Superseded extraction traces are removed. The retirement probe retains only
distinct relevant events.

## Design invariants

Producer owns native retention and exact inactive-exclusive authority. Root acquisition clears the mutable ABI
input slot. Restoring copied address bits cannot reacquire authority. PreparedStart owns the exact Producer
whose requirements it inspected. Pending Request owns Producer, continuation and disposition before current().
Composition carries the real parent Cursor and origin. begin_child consumes and returns complete Composition,
retaining empty producer and parent-selection postconditions. No predicate reassertion after callbacks.

Shared references inside mutable holders preserve their current slot across sibling writes. Address classification
has no shared-loan exemption. Replacement and previous epochs remain excluded. Both replacement negatives remain.

Trace actual producers and consumers through ownership, storage flow, liveness, lowering and cleanup.
Production precedes consumption. Moves retire old authority. Loop analysis stabilizes. Explicit empty ownership
layouts remain authoritative. Tag preservation alone is not payload preservation. Retaining identity does not
retain overwritten contents. Staged outputs use pending producers, other observations use committed state.
Generic RawBuffer contents retain their actual payload dependencies, including remaining contents after Pop.
Preserve deferred admission (d302124), exposure limits, role-sensitive snapshots and immutable proof graphs.
Compiler invariant defects panic at detection. Source diagnostics retain specific structured causes.

## Remaining completion gates

Refresh native failures, fix causes as coherent batches, and stop on unexplained comparison regressions.
Verify main-lane/root authority, compatible child progress and direct await without another TCB. Prove root
lexical cleanup precedes terminal publication for normal/panic/cancellation, rollback exactly once, incidents
and result disposal before services, and services retained through selected static disposal. Preserve sync-only
and no-optional-resource controls. Use async-ownership-composition.bray and runtime-root-smoke.c. Native archive
and link evidence must prove the migrated Rust root/shutdown orchestration is absent. Suspended host/static
fallback belongs to BRA-507, using the same continued-execution owner. Test-host policy belongs to BRA-559.

Complete affected interface/source/native gates, maintainability audit and formal PR re-review. Then mark the PR
ready and BRA-567 In Review. Do not close the issue before merge. No implementation completion is claimed yet.

## Operations

Use optimized tooling. Set CARGO_TARGET_DIR and CARGO_BUILD_BUILD_DIR to the main target/cargo.
Checker incremental is disabled. LLVM is target/toolchains/llvm/active (22.1.8). Native Rust uses installed
1.98.1-x86_64-pc-windows-msvc because default 1.99 emits LLVM 23 bitcode. LLVM/archive/link/execution runs outside
the sandbox. Bound expensive processes and preserve leaf failures. Reuse prior evidence for unchanged behavior.
Do not clean shared storage or interrupt unrelated editor work. Replace stale task logs instead of accumulating
history. Run style at batch finalization and format Bray. Inline fixtures require manual readable formatting.
