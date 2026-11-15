# BRA-567 current work

Branch: feature/bra-567-async-root-orchestration. PR: https://github.com/lejmer/bray/pull/710.
Continue through final review. Commits/pushes and bounded full matrices are authorized.
Further performance optimization is a separate PR. Keep compiler-owned versions at 1 / 1.0.
Read this record and the work-on-bray-issues contract gate after compaction.

## Current evidence

Comparison baseline: 854757009. validation-current.json/log retain fixed 916, owned 505, deferred 298,
frozen 30 and focused controls. comparison-current.json/log preserve current exact identities and hashes.
Compiler checkpoint 86ef6ef86 passes all 1,715 identities with none missing in 398.28 s. Focused checks pass 559 checker,
221 symbol and 17 carrier/nullable/probe tests. No expected outcome or failure site changed.
No checker instrumentation or temporary probe remains in source. The focused binary includes the removed
producer audit. Pin records identify an earlier binary. Four count hooks remain only in an ignored patch.

The current pin probe verifies clean bodies for ensure_deadlines, ensure_workers, close_owner,
close_owner_binding and native_owner_release. close_owner now publishes the complete execution Reference
and its acquisition child. pin-current.json/log retain exact identities and layouts.

The last native gate at 86ef6ef86 fails with 12 primary E7123 diagnostics and 15 dependent E7135 diagnostics.
All five E7124 separation failures are gone. native-current.json/log identify the release tool and pinned
toolchains. Native execution, root C smoke, real async composition and archive/link removal remain unrun.
Native Rust must use installed 1.98.1 with pinned LLVM 22.1.8. Default 1.99 emits LLVM 23 bitcode.
The provider tests filter selected functions, and the profiling test checks reuse rather than zero diagnostics.
Their passing results do not establish that every production runtime body checks.

Remaining groups: borrowed view/storage retention, raw allocation evidence, observer preparation,
and neighbor access carriers. The producer repair below now checks awaited_composition through current().
Do not recover opaque guarantees from unchanged pointer bits or weaken indirect-storage negatives.
LinkAccess still holds a lease borrow and raw pointer.
allocation-current.log and links-current.log retain the unresolved earlier traces, identified by proof-current.json.

Reviewer /root/pr_review reviewed c7c69aba7 and published review 5909136276, verified on PR 710.
The repeated-layout finding is fixed and thread PRRT_kwDOTFaIPc63mNSa is resolved, reply 4577051057.
Lazy per-invocation reuse passes all 559 checker and 820 owned/deferred/carrier/nullable/consume controls.
No new actionable findings. All prior review threads are resolved.
Current fixes use selected_lane for all callback readiness, share referent_owner_identity and move imports.
The existing root fixture checks blocking/compute rejection while the cooperative task is active and driver
acceptance after terminal return. Native execution of those assertions remains blocked by contracts.
The Windows worker counter now intercepts __imp_CreateThread, verified against the pinned Rust std imports.
An isolated native control counts a real created thread. Combined C smoke syntax passes with three existing
CRT deprecation warnings. Style and Bray formatting pass. No new test fixture families or representations.

Timing controls pass: four completion cases 141-174 ms, loop 162 ms, provider separate. Counts below are
from the previous checkpoint and have not been remeasured for the pin batch.
Counts: loop 1,485,955 comparisons / 38,484 nodes, up 0.23% / 1.09%. The four other cases decrease slightly.
Current counters do not measure destruction. The earlier checkpoint measured all condition nodes released.
No leak has been demonstrated. Reused provider compilation previously plateaued at 3.8 GB private memory.
Previously unaffected gates: interfaces 217, bound-tree 136, lowering 38, tooling 115,
runtime interface 29, runtime ABI 35, test protocol 23 and backend identity 1.
Preserve fixed identities and rejection sites. Host-oracle corrections follow contract-reasoning.md.

## Current contracts and unresolved causes

Separation contract: a child transfer's known ordinary guards may qualify an enclosing transfer at the
same relative output path, only when it occurs in every possible alternative. Unknown guards, empty
alternatives, different owners and output paths remain conservative. The existing intersection owns this
rule. Its unconditional API supplies no guards. Fixed source/import and fabrication controls pass.

Projected-owner contract: observe_memory_read carries the exact current snapshot through observe_move
and pending_results. Owned(projected input, current contents) uses the same ancestry lookup as a bare
input. Immediate-parent projection respects empty intermediate records. Keep root_layout only and derive
preservation from current contents. Never import historical children or derive ownership through loans.
Existing projection and mutation tests cover nested owners, overwritten children and explicit empties.

Producer contract: Inactive remains the copied ABI token. Durable producer permission must
include BOTH retained provider and exact inactive-exclusive claims. Native fixtures drop Arc<Frame>
(including descriptor/backing) during destroy, before returning. Admission/destruction must consume the
permission, and no native-bound permit may escape its authorized entry lifetime. A generic shared-borrow
issuer can mint duplicates and is unsound. Mutable borrowing alone does not solve later reacquisition:
read<Inactive> is Copy with no target consumption, even through address_of_mut. Do not change that rule.

Inactive.into_producer consumes the ABI token and introduces the scoped noncopy owner from both native
claims. The receiver retires caller authority. Its raw read copies data only and cannot recreate the claim.

Current producer batch: all 8 admission controls and 3 async composition controls pass. The latter include
actual async source checking/lowering and fabricated/moved-owner rejection. producer-controls and
composition-current JSON/logs identify the binary and results. producer-bodies JSON/log/err record 65
callable bodies from the 14 changed sources, with the declaration-only request source having no callable
body. All three composition entry paths and root_frame now check/lower cleanly.

Two bodies still fail: commit_start has storage_retained failures at lock_cell and wake_reference,
and start_frame retains its cleanup failure. The lock_cell rejection is NEW after take_nullable clears
owner.token. The earlier wake failure follows clearing owner.registration. This points to the same
sibling-write/borrowed-reference retention boundary, but its compiler cause is not established yet.
Do not claim no regressions. This blocks further migration and final handoff. No full matrix or fresh
native execution gate has been run for this batch. Next trace invalidation through actual stored borrows.
Relevant code: guarantee/flow/invalidation.rs, flow/preservation.rs, retirement.rs and condition/retained.rs.
No compiler semantic edits have been made for this gap.

Composition now has two actual stages. Pending Request owns the exact Producer, continuation,
destination/disposition and typed Ready before current(). Bound Composition owns that same Request,
actual parent Cursor and origin. Native callback pointers convert once. Source stores composition_readiness.
The retained active invocation is paired through request_for_task(request marker, parent task marker, origin).
This relation grants no independent producer/destination/callback authority. Task binding validates the
actual dispatching/resumable parent under its lock, and source origin is checked against Current.
Admission takes/clears the nullable producer slot. Independent request and parent authority survive for
connection, which reads stored destination/disposition. Old expanded tuple predicates, raw forwarding and
post-callback producer reassertion are removed. The retained reviewer agreed on this design before editing.

root_frame consumes the mutable native address slot with replace_value and does not preserve frame_input.
Its two callers own mutable local slots. The regression also restores address bits before retrying.
Inactive acquire must publish independent output permission while consuming its exact entry claim.
Publication/provider initialization retain their weaker retention-only protocol and cannot mint Producer.
PreparedStart owns the exact Producer it inspected, preserves it through reservation, then takes/clears it
before begin. No refill against old requirements. Rollback keeps independent registration/service duties.
Retain the local invoke cleanup adapter: enter_cleanup is a separate state change after shared admission.
Existing native/source admission test identities and outcomes remain, with metadata rejection now at
into_producer's inactive_owner requirement, before metadata access. No permission weakening is allowed.

Returned ownership contract: prepare each returned snapshot together with its current owned layout, using the same ownership
recording as ValuePreservation.frame. Completion must consume that prepared mapping, not reconstruct input.root_layout.
A lifted nullable has payload ownership without a separate wrapper owner. Preserve that actual payload,
but never recover overwritten children, empty layouts or ownership through an ordinary loan. Producers:
prepare_returned_values and checked extraction. Consumers: guarded completion, retirement and joins.
Remove the duplicate completion reconstruction. Expected production reduction, no new representation. Data-only mask replay must remain distinct from
checked whole-owner framing: preserving a discriminant alone cannot grant ownership.
Extend nullable adaptation coverage with a fresh witnessed owner forwarded through a second callable.
Retain replacement/fabrication, nested overwrite, borrowed extraction and source/import controls.
Portable preservation describes checked inline transport, while the physical mask refreshes reached state.
Do not reconstruct portable preservation from that mask: a callable payload can retain its nullable tag
without retaining all reachable state. Record returned layouts using the callee preservation contract,
filtered by actual surviving owner identities. Frame invalidation still uses its physical survival mask.
Reconsider if reference refresh changes inline contents rather than only reached snapshots.

Checked extraction: ValuePreservation::frame captures extracted_result for the entry's existing
extracted_values before vacating borrowed formal storage. Ordinary borrowing cannot create ownership.
Explicit empty layouts remain authoritative. No reconstruction of historical entry contents.

Return completion prepares each transferred snapshot once before retirement. Guard qualification,
retirement and alternative construction share that snapshot. Exact borrowed-field mappings come from
footprint.references, with current committed targets. Pass the actual destination through direct,
implicit and deferred completion, including RunResult.Completed. Multiple targets, raw addresses and
absent payloads cannot establish borrowed equality. Preserve branch guards and exact ownership layouts.

A complete portable transfer plus checked normal-storage preservation retains the actual synchronous
entry snapshot. The portable mask excludes inline replacement and storage-flow record_effect tracks
reached writes. Neither guarantee alone suffices. Deferred entries can predate suspension and still
require current completion mappings. No historical fallback. Reconsider if local protected writes fail
to revoke the portable mask. with_referent preserves its snapshot when contents differ only by an
Owned/Scalar wrapper for the same owner, using retention's nonunknown contents comparison.
Changed contents or different owners still refresh. Existing aggregate/carrier tests cover both.

A guaranteed enclosing transfer to the same relative output path makes its descendant annotation part
of the complete owner, not a detached witness. Use guaranteed_transfers. Partial-only returns, a parent
returned elsewhere or only in another Choice cannot authorize separation. Conditional enclosing layouts require the same known guards and a transfer in every possible alternative. Publish ordinary active/empty shape without deriving presence from ownership or overriding empty layouts.

Ownership alternatives retain proven true and false ordinary payload guards. Unknown stays unknown.
A false guard qualifies the actual empty layout, never an invented payload. Nullable joins compare
stored payload snapshots and use known_presence for inactive wrappers. Equal owners retain identity,
not overwritten contents. Portable tag+inline preservation never freezes generic/protected data.
Private nested-frame custody uses existing recorded child admission and actual retained owners, never
outer admission or public availability. Actual writes still revoke captured evidence. Preserve d302124.
Inexact complete private-region views stay private. Unknown/displaced/escaped regions remain conservative,
and no view acquires exact identity or initialized-memory authority. Existing controls cover these rules.

State.returned_inputs uses (place, previous), preserving one finite historical summary. Transfer owns
only unproduced current bounds. Staging no longer publishes bounds in State. Completion retires the
old origin before installing new bounds. Reads retire before publishing. Retirement updates referred
epochs even without tracked contents. Relocation moves bounds' referents alongside other references.
Private local alias checks require matching storage epochs. Old returns may still alias unchanged
outer-loop objects, external backing and unknown global alternatives. No extra history map or fallback.
Escape publication keeps explicit roots unrestricted. Unknown candidates expand to physical origins
and pass unknown_reference_may_reach, so a symbolic external alternative cannot publish a private local.
Existing tests cover pending production, old surviving aliases, fresh locals, explicit and raw exposure.
The two old unit fixtures now create retired epochs through actual completion instead of flipping a flag.

Staged output expansion reads its pending producer. Ordinary inputs and historical aliases use
committed epochs. Deferred result cleanup captures physical input regions before completion.
The rewritten-return, independent-pointer source/import, activation, storage, async and loop controls
pass. No early publication, new history map or reconstruction is permitted.

## Transition requirements

Trace executable operations and their consumers. Verify one real async composition and invalid variants.
Exercise production-before-consumption, exclusive authority transfer, overwritten-content invalidation
and loop stabilization. Preserve the fixed comparison suite. An unexplained regression blocks expansion.
Claims must name paths, evidence and exclusions. Compiler defects panic at the violated invariant.

Move replay and lowering share the exact checked StorageAccessPlan lookup. Preserve the source entry
before admission, vacate the actual moved storage, then complete its destination. Copies, rejected moves
and failure before admission cannot retire it. No reconstruction by expression/access identity.

RawBuffer payloads use the existing State.values/Transfer/ReferenceExit graph. Whole-source lineage is
possible data, never authority. Exact overwrites replace contents, conditional stores retain possible
survivors, and Pop retains remaining elements. Publish on normal completion with actual allocation
anchors. Scalar buffers stay independent. Result cannot be an entry source.
Nullable whole preservation requires both its discriminant and complete payload, never the tag alone.

## Remaining gates and operating constraints

Finish corrected fixed916 and owned/deferred matrices, source/interface gates and native artifact smoke
checks outside the sandbox. Verify root/main authority, cleanup before terminal publication,
normal/panic/cancellation, rollback once, services through static disposal, synchronous control and
archive/link removal of Rust orchestration. Use async-ownership-composition.bray and runtime-root-smoke.c.
Preserve deferred-admission d302124. Complete maintainability audit and independent draft-PR review.
Only then mark PR ready and BRA-567 In Review. Closing follows merge.

Use optimized tooling and shared target/cargo with checker incremental disabled and LLVM active.
Use namespace filters for full matrices. Windows cannot launch505 long identities as one command.
For bounded runs, write output directly to files and save each result even on timeout or launch failure.
Keep actual Bray formatting in inline fixtures. Run style at coherent batch finalization.
Delete stale traces/caches when replaced. Never clean the entire shared target.

Performance regression monitoring remains active. Fixed identities are in timing-current.json.
timing-current.json records the latest five controls, including loop, with provider time separate.
Latest counts-current.json records loop 1,485,955 comparisons and 38,484 nodes. All five controls pass.
The current four-counter probe does not measure destruction.
Timing and counts use separate runs.
Counter patch retains four measured hooks.
Preserve immutable conditions, scope-keyed ProofMemo, ordered BooleanGraph, role-sensitive snapshots,
actual predecessor evidence, absence/unknown and owner/contents/capture distinctions.
No new cache or evidence representation without removing superseded machinery.

Active batch: owned producer migration is ready for a WIP backup checkpoint, not final verification.
No build or test process is running. Temporary audit_changed_producer_bodies was removed from source.
Binary 4930D763 contains the removed audit. Its result metadata stores exact runtime-source hashes.
Style/format passed again after removing the temporary Rust audit.
Always set CARGO_TARGET_DIR and CARGO_BUILD_BUILD_DIR to the shared main target/cargo for cargo xtask.
The duplicate worktree release intermediates were reclaimed with bounded cargo clean, removing 1.3 GiB.
Shared main target/cargo is intact. Obsolete producer experiments were deleted.
