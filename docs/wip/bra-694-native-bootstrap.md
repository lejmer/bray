# Native bootstrap contracts

Issue: https://linear.app/bray-lang/issue/BRA-694.
Branch: `feature/bra-694-native-bootstrap-contracts`, based on develop c1d01eb34.
Draft PR: https://github.com/lejmer/bray/pull/717. Initial regression checkpoint: 818499c39.
Reviewer: `/root/bra694_review`, reviewed 8de7d45c6 at https://github.com/lejmer/bray/pull/717#pullrequestreview-5912211328.
PR #710 and its seven prerequisites are merged. Execution and teardown remain BRA-695 and BRA-696.

## Contract and verification

Make actual supported Linux and Windows bootstrap products check and lower without widening authority.
Preserve disjoint owners and fields. Reject fabricated owners, replaced contents, old epochs and real raw retirement.
Preserve source/import composition and negative rejection locations. Internal invariant defects panic.
Assignment of registration must precede demand_lane so panic rollback can withdraw it exactly once.

The inherited baseline passed 1,718 fixed identities, 561 checker tests and 24 controls. Base style and format passed.
The connected batch builds. Latest bounded twelve-test run: nine pass, including the nested-union source/import
composition, exclusive-entry tags and all six prior authority controls. The protected provider now checks through raw
read and assume_initialized. Its repeated-read negative rejects correctly but also reached a later observation, so the
fixture was corrected to isolate that boundary. That correction and remaining protected source/import cases await rerun.
Both native probes still reach context/view_owned and registration postcondition failures. The dependency cycle is gone.
Whole composition and broad suites remain unverified. Style is deferred.

Actual Windows product baseline has nine call failures, one registration postcondition and sixteen propagated errors.
Other leaf sites are create_execution, root_scope, create_worker_context, observer create and unlink_registration.
Both imported-product verification and the four Linux/Windows target gates remain outstanding.
Temporary tracing has been removed. Keep only current evidence and overwrite investigation outputs.

## Producer repairs

Recognize only the standard-library platform aligned allocator and publish the existing Allocation result relation.
This gives origin separation, never non-nullness, initialization, ownership or access authority. Keep foreign effects
and ABI lowering unchanged. Normal staging commits once, wrappers/imports retain alias groups, null grants no authority,
and replay leaves old epochs retired. Ordinary extern owned_allocation results remain nonfresh.

Event Reference stores its actual DataPointer<Domain>. Initialization publishes it, retain copies it after count commit,
directory/wait records move the complete owner, and final release uses byte_address under final_reference.
Remove its integer roundtrips. Callback context and directory identities retain their integer ABI. The C record remains
one pointer-width field, but actual Bray/native layout validation is still required.

Resolve complete ownership masks once in BoundReturnedValue preparation, alongside source paths and observations.
Direct, deferred, hidden, protocol and conversion calls share that producer. Completion consumes the immutable mask.
Lifecycle cleanup has no owned-return mask consumer. Storage flow still intersects masks across alternatives.
This removes late conversion through an incomplete union-selector inventory. No guards or authority rules change.

## Ordinary observations

Mutable-loan framing omitted ordinary values and tags before consulting effects. Consider ordinary inline observations
per exact physical target. Keep Reference boundaries and protected/reached Mutable observations on dedicated paths.
Nonraw Ambient work preserves exclusive inline state only when actual publication is disjoint. Normal and failure
publication precede their respective framing. Keep conservative Entry reachability and real raw effects unchanged.
Controls cover a nested shared argument, opaque mutable argument, raw tag/owner publication and actual replacement.

## Protected references and nested cleanup

Confirmed: take_retired_arena publishes allocation.pointer only, with no stored records. release_arena has no
UninitPayload reference paths. Shape mixed physical reference inventory with ordinary observations that Uninit hides.
Separate canonical physical paths and use them for entry demands, reference-free checks and assignment transport.
Compose all inline shapes and bounded arrays, including UninitPayload. Atomic and recursive/unknown shapes stay opaque.
Ordinary return snapshots still exclude protected payloads. Raw reads and extraction transport current stored records.

Nested buffer cleanup also loses the typed descriptor route. Dispose elements at owner.OwnedTarget.AnyElement and
retire backing separately through owner.pointer. Prepare finite cleanup subjects with the existing guarded expansion.
Reuse segmented stored_locations for call effects, lifecycle effects and entry seeding. Follow actual current pointers,
retain AnyElement inexactness, seed prerequisites first, and preserve existing pointer records. Do not add unrestricted
owning-element parent edges or project value layout onto inexact bounds. Cleanup regions stay conservative.

Equivalent paths include lock destructors, worker nested buffers, scheduler/deadline retirement and channel extraction.
Validate raw read, protected extraction, imported publication, nested disposal, overwrite and saved-alias retirement.
Reuse existing paths, demands and stored-value graph. Remove superseded observation-derived physical queries and
parallel backing lookup. Growth must buy those current contracts, without new authority or a new graph representation.

## Cleanup dependency closure

Finite nested cleanup expansion queries source destructors that the summary component did not inventory.
The producer must discover those edges before solving components. Reuse the same guarded cleanup expansion with
a callable visitor, never a second list or a cycle fallback. Actual summary evaluation uses the existing component's
immutable approximations. Preserve recursive/generic and imported cleanup effects and all failure effects.
The corrected native checks pass cycle discovery. Keep that as a regression control.

## Current normal-publication repair

Nested union mutation retains the same complete Owner but branch masks differ. The observed export guards its whole
mapping and rejects a child as separation. Factor verified common identities outside their Case guards inside each
complete Choice arm. Preserve each arm's mask and Case(empty) feasibility. Never merge a weaker common mask into
an arm's stronger mapping. Inactive/fabricated owners still require their ordinary output shape.

Exact moving raw read captures an initialized Uninit owner before consuming source storage, but ordinary snapshot
publication omits its existing initialization clause. Add an initialization observation, excluded from ordinary values,
Mutable preservation and ownership masks. Normal proof retention selects only already-established UninitInitialized
clauses on the exact transported inline owner. It must not preserve opaque wrapper predicates, source-slot access,
reached contents or new initialization. Non-intrinsic call transport additionally requires synchronous normal storage
preservation and a complete ownership mask. Controls include fabricated Uninit, repeated source read, nested overwrite,
real nested backing retirement and an unrelated retained owner, with source/import call composition.

Next: rerun the corrected protected fixture and trace the remaining native current summaries, then finish producer/consumer repairs. Final gates remain
fixed regression, owned/deferred, projection/cleanup, native product and fixed-five/loop, followed by PR re-review.
