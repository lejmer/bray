# Native bootstrap contracts

Issue: https://linear.app/bray-lang/issue/BRA-694.
Draft PR: https://github.com/lejmer/bray/pull/717.
Branch: `feature/bra-694-native-bootstrap-contracts`, rebased onto develop 5de46243b.
PR #710 and its seven prerequisites are merged. Execution and teardown remain BRA-695 and BRA-696.
Reviewer: `/root/bra694_review`, clean incremental review of 1ceb765eb, replayed unchanged as 418fecf7f:
https://github.com/lejmer/bray/pull/717#pullrequestreview-5912959148.
Both earlier findings have resolved PR threads.

## Outcome and constraints

Make the actual supported Linux and Windows bootstrap products check and lower, for x86_64 and AArch64.
Preserve source/import composition, disjoint fields/owners, and rejection of fabricated owners, replacement, old epochs
and real raw mutation/retirement. Keep exact negative rejection locations. Internal invariant defects panic.
Registration must be stored before demand_lane so panic rollback can withdraw it exactly once.
Performance and runtime execution/teardown are separate work.

Keep one authority for each representation. No historical-value reconstruction, fabricated pointer identities, freshness
from owned_allocation, exact identity from an inexact region, layout projection onto mere alias bounds, callback predicate
reassertion or blanket shared-loan exemption. Retain whole branch masks and authoritative empty ownership records.
Style is deferred until errors are resolved. Commit/push checkpoints, then publish independent review on the PR.

## Current evidence

Head 1440b142a is pushed. All 563 checker tests pass. Protected address/raw-read/Uninit/nested-buffer composition passes
source/import, including exact retirement and fabrication rejection locations. Conditional-future active, inactive and
dropped controls pass both modes. The frozen opaque-execution negative is restored. No atomic-frame panic appeared
after completing the effect inventory, but whole-provider completion is unverified.

The frozen 1,718-identity comparison on 1ceb765eb had three regressions and seven panic timeouts. The three regressions
now pass focused checks. A subsequent 39-case run completed 20 passes and stopped at 300 seconds with 16 unfinished.
Both native probes still fail at dispatch_ready/context(view_owned) and reserve_start/registration presence.
The corrected source/import tag fixture passes with its API import and conditional memory import.

The uncommitted setup batch removes four duplicate provider tests and their separate cache and shares native dependency
preparation across readiness/composition/std tests/performance/smoke. Five artifact reuse/pinned-input controls pass.
All 13 remaining boundary/totality/cache controls pass in 19.1 seconds. No fixture-cache semantic change remains.
Do not bypass the existing identity-namespace invalidation contract.

Native-exit, retirement and shutdown now retain all 15/5/7 cases as named functions under native-execution/contracts.
One immutable Compilation discovers all three files and shares one CPU-sized worker pool across all groups, without
source revisions. Their helper matches the actual source-unit inventory bijectively to expected outcomes and reports
each case. All 27 pass in 16.8 seconds total under a 60-second bound. First probe checks cold dependencies in 10.26 seconds,
the other 26 total about two seconds, mostly 2–50 ms. The former 15-source-revision run exceeded 120 seconds. A single
cold probe demanded 479 body units, 501 preparations, 293 certificates and 192 components through attachment cleanup
and provider callbacks. Remaining cold compiler cost is separate performance work. Temporary query tracing and selection
are removed. No test is running. Actual native products replace the four removed positive gates and remain mandatory.
Do not count those removed identities as passes.

Actual four-target products, event layout, final matrices/comparison, fixed-five/loop, mutating style/format and
maintainability gates remain open. Use shared Cargo/LLVM storage. Native toolchain workflows run outside the sandbox.
Do not rerun full matrices or the redundant whole-provider group during this setup repair.

## Provider reuse contract

Canonical native builds already check whole packages, nested units and executable-template families, and publish
validated interfaces, implementations and archives. Remove the three filtered host/report/storage positive gates,
their independent provider cache and the full-provider timing test. Native Linux/Windows product gates replace that
coverage and remain mandatory, currently failing. Keep metadata-specific assertions, private-owner source probes,
negative outcomes and exact failure locations. Do not pretend removed test identities passed the frozen comparison.

Readiness, native std tests, composition, smoke and default performance preparation must share the existing release
runtime destination and runtime-bootstrap-standard-library per-target bundle. Prepare once per orchestration and use
assemble_from_bundles. Delete assemble's implicit second std build. Explicit outputs and pinned inputs remain exact.
Carry the builder's existing Package inventory through readiness to smoke, instead of reconstructing it from a path.
Keep executable/work directories isolated. Existing publication locks reject concurrent writers, but do not promise
single-flight waiting or pin readers across replacement. Do not start competing configurations while consuming a pair.

Private source-owner probes cannot all become imported consumers without changing their visibility/authority contract.
Share one immutable source program across independent probe bodies. Any source edit invalidates the compiler's local
identity namespace, including semantic results for unchanged sources, as existing snapshot tests require. Reusing a
certificate across such edits is not promised and would require a separate compiler-owned incremental design.
Keep existing identical-input/syntax/isolation controls. Use ordinary tiny fixtures for self-contained totality/payload
borrow cases. Do not add a fixture framework or locally bypass the dependency graph.

## Current contracts and their owners

- The canonical platform aligned allocator publishes Allocation origin separation only. It grants no non-nullness,
  initialization, lifetime, ownership or access. Ordinary extern owned_allocation results remain nonfresh.
- Event Reference stores its actual DataPointer<Domain>. Copies and moves retain it, final release uses byte_address.
  The Rust ABI mirror remains one pointer-width field. Actual native layout validation is pending.
- BoundReturnedValue resolves full ownership masks at dependency preparation. Completion consumes that immutable mask.
  Storage flow still intersects alternatives. Common owner identities factor within complete Choice arms, preserving
  each arm's stronger mask and Case feasibility. Inactive and fabricated owners still require output shape evidence.
- Ordinary exclusive inline framing consults actual normal/failure effects and publication. Reference boundaries and
  protected/reached Mutable observations stay separate. Real raw effects remain authoritative.
- Shape carries physical reference paths separately from ordinary proof observations. UninitPayload is physically inline,
  without making its payload ordinary observable contents. Atomic and recursive/unknown shapes remain conservative.
- Initialization retention selects only existing true UninitInitialized clauses on the transported inline snapshot.
  It preserves no opaque predicate or source-slot permission. Non-intrinsic retention additionally requires synchronous
  normal storage preservation and a complete ownership mask. General deferred transport needs declared postconditions.

## Physical storage and publication

contains_storage keeps one origin/epoch and stops at OwnedTarget, while retaining UninitPayload and dynamic selector
inexactness. Ordered range termination stays logical. Capture, clear, staged lookup, absence joins and descriptor lifetime
retirement use physical containment. Existing backing records remain at their original physical identities after moves.
Actual backing retirement still invalidates its inline records and saved aliases.

Returned/Allocation root payload publication advances the prior producer namespace, including nested backing coordinates.
Copied protected payloads within existing referents remain physical writes. Ordinary moves create no producer epoch.
Deferred relocation preserves captured inputs and stable conservative bounds, and instantiates pending output coordinates.
A potential same-site produced/captured occurrence collision has no established source acceptance defect. Raw-pointer proof
identity is separate, and repeated semantic evaluations expire. Do not add speculative occurrence metadata.

Addressed storage outlives logical last use. has_addressed_storage queries existing same-epoch physical Contents and is
shared by pruning and mutation short-circuits. forget/retain_live retain current inline records while that storage exists.
They do not retain descriptors merely because backing exists. Clear/write, consumption and end_storage still remove
contents or authority. Previous coordinates come from Origin::Storage, not the current-only storage_path accessor.

The sole value_references producer keeps physical inline fields. Reached OwnedTarget routes remain entry demands instead
of becoming fabricated inline result fields. Generic result formation and inline reachability share that inventory.
Entry seeding follows prerequisite pointers first. Cleanup, calls and stored replay use the same segmented current-pointer
resolver, preserving AnyElement bounds. Nested buffer element disposal and backing retirement remain distinct effects.

The shared dependency projection converter retains the existing AnyElement selector. It does not make local dynamic or
exact-offset selectors portable. Effect publication, reference provenance and interface serialization preserve inexactness.
The protected fixture accepts bounded direct owning-region and indirect pointer retirement, and forbids unresolved effects.

## Cleanup closure and static effects

Guarded cleanup expansion supplies both effect paths and source callee discovery before summary components are solved.
The inventory includes lazy completion types. Recursive lazy-result cleanup now checks and lowers without a cycle fallback.
Static effect subjects prepare declared types through the existing declaration query and resolve from static roots through
current pointer records. Direct/hidden calls, lifecycle, nested-buffer and deferred cleanup and stored replay share this.
Unresolved paths remain conservative. Standalone static reference exits remain outside the verified composition, with no
native consumer identified. Do not invent reference anchors to expand that scope.

## Remaining source contract

Incoming mutable Entry storage is not proven private. reserve_start takes &mut PreparedStart, calls opaque demand_lane,
then promises its registration tag. Its sole caller already owns PreparedStart in a with body. Inline reservation there
and remove reserve_start, preserving the actual private local storage rather than adding an authority exception.
Keep select/register failures as function returns, store the successful reservation before demand, and borrow the scoped
owner for commit. Every return, panic and cancellation runs PreparedStart.exit. Failed admission withdraws the stored
registration once before releasing services. Commit transfers it under the task lock, clears the preparation field before
waking, and scope exit releases only the preparation's independent services. The intervening guard destructor is total.
Never move the scoped capability into a helper, duplicate withdrawal, postpone the store or reassert predicates.
Update the native bootstrap named-body control to start_frame. Expected production growth is negative. Reconsider on
any loss of cleanup authority, changed failure outcome or regression in frozen opaque/raw-mutation controls.
This source change remains unimplemented. dispatch_ready's first view-owned invalidation remains unresolved.

## Next work

Complete the source contract above, then inspect remaining native failures across
actual products, including current summaries and every affected consumer. Preserve the fixed oracles.
Run fixed comparison, owned/deferred and projection/cleanup gates, actual four-target product checks/lowering and
layout checks, plus fixed-five/loop controls. Finish maintainability/DRY audit and mutating style/format commands.
Update the PR description and resolve review findings. Mark PR ready and BRA-694 In Review only when its contract passes.
