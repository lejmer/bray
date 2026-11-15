# BRA-567 current work

Branch: feature/bra-567-async-root-orchestration. PR: https://github.com/lejmer/bray/pull/710.
Continue through final review. Commits/pushes and bounded full matrices are authorized.
Further performance optimization is a separate PR. Keep compiler-owned versions at 1 / 1.0.
Read this record and the work-on-bray-issues contract gate after compaction.

## Current evidence

Comparison baseline: 854757009. The current repair is the latest checkpoint on this branch.
All 1,715 fixed comparison tests pass, zero failed or missing, in 743.83 s.
comparison-current.json/log preserve exact identities and binary/source hashes. validation-current.json/log
retain the comparison baseline: fixed 916, owned 505, deferred 298, frozen 30 and focused controls.
All 559 checker tests and 11 nullable controls pass. repair-current.json/logs record final focused evidence.
The strengthened nullable test checks whole-input publication and preservation of the tag through await.
No temporary fixture probes or checker instrumentation remain. Four counter hooks are retained only as
an ignored patch for future measurements. The compilation test binary still contains disabled counters.

The repair prepares current returned ownership once, including owners inside anonymous nullable wrappers.
Completion consumes that mapping instead of reconstructing input.root_layout. Portable preservation comes
from the checked callee contract, not from the narrower physical mask that refreshes reached state.
Source body candidates for create_execution and commit_driver are clean. boundary-source.log retains the
six sampled body outcomes. Its query exit status is not a claim that all sampled bodies pass.

Fresh native smoke compilation fails with 17 primary diagnostics and 15 dependent E7135 diagnostics.
Counts: E7123=12, E7124=5. native-current.json/log contain the current release tool hash and pinned toolchains.
The nullable deadline/worker take_pin failures have disappeared. Their callers in execution/deadlines.bray
and execution/workers.bray now report separation failures. Trace these alongside the three existing
close_owner separation failures before classifying or repairing the group. Native execution, root C smoke,
real async composition and archive/link removal gates remain unrun.
Native Rust must use installed 1.98.1 with pinned LLVM 22.1.8. Default 1.99 emits LLVM 23 bitcode.
The provider tests filter selected functions, and the profiling test checks reuse rather than zero diagnostics.
Their passing results do not establish that every production runtime body checks.

Remaining groups: borrowed view/storage retention, raw allocation evidence, separation, observer preparation,
and inactive producer/neighbor access carriers. awaited_composition loses producer(token) at current().
Inactive has copied IDs and a raw descriptor pointer, but no represented retained permission. Do not recover
its opaque guarantee from unchanged pointer bits or weaken the indirect-storage negative. Establish the
actual native producer-retention contract. LinkAccess similarly holds a lease borrow and raw pointer.
allocation-current.log and links-current.log retain the unresolved earlier traces, identified by proof-current.json.

Reviewer /root/pr_review reviewed 854757009 and published review 5908506132 on PR 710.
Open comments 4576635425/5429/5433 concern callback lane widening, duplicated normalization and local imports.
Current fixes use selected_lane for all callback readiness, share referent_owner_identity and move imports.
The existing root fixture checks blocking/compute rejection while the cooperative task is active and driver
acceptance after terminal return. Native execution of those assertions remains blocked by contracts.
The Windows worker counter now intercepts __imp_CreateThread, verified against the pinned Rust std imports.
An isolated native control counts a real created thread. Combined C smoke syntax passes with three existing
CRT deprecation warnings. Style and Bray formatting pass. No new test fixture families or representations.

Timing controls pass under concurrent work: four completion cases 170-190 ms, loop 217 ms, provider separate.
Counts: loop 1,485,955 comparisons / 38,484 nodes, up 0.23% / 1.09%. The four other cases decrease slightly.
Current counters do not measure destruction. The earlier checkpoint measured all condition nodes released.
No leak has been demonstrated. Reused provider compilation previously plateaued at 3.8 GB private memory.
Previously unaffected gates: symbols 221, interfaces 217, bound-tree 136, lowering 38, tooling 115,
runtime interface 29, runtime ABI 35, test protocol 23 and backend identity 1.
Preserve fixed identities and rejection sites. Host-oracle corrections follow contract-reasoning.md.

## Current contracts and unresolved causes

Active repair contract: prepare each returned snapshot together with its current owned layout, using the same ownership
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
returned elsewhere or only in another Choice cannot authorize separation. Conditional-only enclosing
layouts remain conservative. Publish ordinary active/empty shape without deriving presence from ownership or overriding empty layouts.

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
