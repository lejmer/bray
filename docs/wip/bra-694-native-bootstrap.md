# Native bootstrap contracts

Issue: https://linear.app/bray-lang/issue/BRA-694.
Draft PR: https://github.com/lejmer/bray/pull/717.
Branch: `feature/bra-694-native-bootstrap-contracts`, based on develop c1d01eb34.
PR #710 and its seven prerequisites are merged. Execution and teardown remain BRA-695 and BRA-696.
Reviewer: `/root/bra694_review`, last published review at 870a00d03:
https://github.com/lejmer/bray/pull/717#pullrequestreview-5912523799.
Its lazy-result discovery and static effect-path findings are repaired, awaiting committed re-review.

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

The current production batch builds with the optimized tooling profile. Temporary tracing is removed.
All 136 bound-tree and 562 checker tests pass. The bounded 25-case integration selection passes 23.
The protected real program passes raw read, Uninit extraction and nested buffer disposal in source and imported modes.
It preserves an unrelated marker and rejects repeated raw reads, overwritten/fabricated initialization and saved retired
backing at the intended boundaries. Same-site deferred loops, independent-owner negatives, static-owner controls,
nested-union mutation, exclusive-entry tags and the earlier allocator/reference/retirement controls also pass.

Both native probes still fail at dispatch_ready's context/view_owned call and reserve_start's registration presence
postcondition. owner_entry_scope passes. These are failing tests, not a verified count of remaining compiler defects.
The actual Windows product's older baseline had nine leaf call failures, one registration postcondition and sixteen
propagated errors. That whole-product result is not current-tree evidence.

The inherited fixed comparison passed 1,718 identities. It has not been rerun on this batch. Broad matrices, actual
four-target product gates, Bray/native event layout, fixed-five/loop measurements, style and format remain open.
Use shared Cargo/LLVM storage. Native LLVM/archive/link/executable workflows run outside the sandbox.

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

## Next work

Commit/push this composition checkpoint and request incremental PR re-review. Inspect remaining native failures as a
class across actual products, including current summaries and every affected consumer. Preserve the fixed oracles.
Then run fixed comparison, owned/deferred and projection/cleanup gates, actual four-target product checks/lowering and
layout checks, plus fixed-five/loop controls. Finish maintainability/DRY audit and mutating style/format commands.
Update the PR description and resolve review findings. Mark PR ready and BRA-694 In Review only when its contract passes.
