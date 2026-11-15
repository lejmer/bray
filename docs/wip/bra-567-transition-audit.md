# BRA-567 current work

Branch: feature/bra-567-async-root-orchestration. PR: https://github.com/lejmer/bray/pull/710.
Continue through final review. Commits/pushes and bounded full matrices are authorized.
Further performance optimization is a separate PR. Keep compiler-owned versions at 1 / 1.0.
Read this record and the work-on-bray-issues contract gate after compaction.

## Current evidence

Checkpoint5f284f398 is pushed. The uncommitted lifetime batch now passes checker559 and
1709/1715 combined fixed/matrix cases, with no reported diagnostic failures. Six full-provider
checks exceeded420s and remain unverified. epoch-combined.json preserves every identity, outcome
and source/binary hash. Owned505, deferred298, frozen30 and focused19 all pass. All eleven earlier
panic regressions pass. Two previously passing slow checks are now unverified, not passes.

The user observed7.5GB in the concurrent provider run. An isolated120s profiled provider run reached
1.85GB peak RSS, then dropped to1.62GB. Still in checking at timeout. This does not establish a leak.
provider-memory.json/log and provider-profile contain the bounded measurement. The profiling fixture
used WorkerBudget::serial. Give that single full-provider probe available_parallelism, keep small
shared fixtures serial, then trace condition creation/release and body progress. No full matrix rerun.

Preserve fixed916 identities and rejection locations. The deliberate oracle rename is
production_host_lease_cannot_claim_preservation_across_fallible_preparation to
production_host_lease_retains_authority_across_fallible_preparation. Other corrected host
oracles follow contract-reasoning.md. Actual replacement and fabrication must reject.
Previously verified: symbols221, interfaces217, bound-tree136, lowering38, tooling115,
runtime interface29, runtime ABI35, test protocol23 and backend identity1. Native gates remain.

## Current contracts and unresolved causes

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

Checkpoint5f284f398 is pushed. Current uncommitted lifetime batch:
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

Pending and completed references must retain their own bounds without early State publication.
Result preparation merges independent and input alternatives before completion. Audit that merge
alongside deferred cleanup capture, output relocation and old-epoch retirement. Existing independent
pointer source/import, activation, storage and async controls expose the regression. Keep the loop
and matrices fixed. nullable-loop.log retains the loop cause, epoch-capture.log the repaired representatives.
All four representative panics identify expand_returned_regions. Its staged-output caller must read
the pending producer, while ordinary inputs use committed epochs. The existing rewritten-return
control now also completes bounded staged alternatives. Cleanup capture independently flattens pending
result effects to physical regions. No new history map or reconstruction is permitted.

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

Performance regression monitoring remains active. Fixed identities are in proof-indexed-current.json.
Latest timing-current.json: all five pass at151-155ms, loop166ms, provider time separate.
Latest counts-current.json on8dcca93cc: loop1,482,315 comparisons and38,038 nodes,
versus previous1,482,433 and38,060. All five controls pass. Timing and counts use separate runs.
Counter patch retains four measured hooks.
Preserve immutable conditions, scope-keyed ProofMemo, ordered BooleanGraph, role-sensitive snapshots,
actual predecessor evidence, absence/unknown and owner/contents/capture distinctions.
No new cache or evidence representation without removing superseded machinery.
