# BRA-567 current work

Branch: feature/bra-567-async-root-orchestration. PR: https://github.com/lejmer/bray/pull/710.
Continue through final review. Commits/pushes and bounded full matrices are authorized.
Further performance optimization is a separate PR. Keep compiler-owned versions at 1 / 1.0.
Read this record and the work-on-bray-issues contract gate after compaction.

## Current evidence

transport-batch-validation.json identifies the source/binary and fixed comparisons:
- Checker559, frozen30 and all19 focused ownership/exposure controls pass.
- Owned501/505 and deferred294/298 retain the same four failures each in source/import modes.
  No passing identity regressed and no baseline identity is missing.
- Nullable extraction and mutable-carrier forwarding now pass, including mutation inside the
  forwarder, mutation after forwarding, fabricated ownership and real async composition.
- fixed-current.json:907 pass,1 fail (loop cleanup),8 unverified. All previously passing identities
  remain passing. The eight previously unverified slow cases were excluded from this comparison run.
- Style and maintained-source formatting pass. There are no installed probes.

Preserve fixed916 identities and rejection locations. The explicit oracle rename is
production_host_lease_cannot_claim_preservation_across_fallible_preparation to
production_host_lease_retains_authority_across_fallible_preparation. Earlier corrected host
oracles follow contract-reasoning.md: immutable completion fields, unrelated ambient state and
an explicitly authorized host borrow do not imply mutation. Actual replacement and fabrication reject.
The full-provider report fixture exceeded240s. body-trace.log locates serial body checking through
entry/bridge::install, not its hot transitive query. Source/interface/native gates remain outstanding.
Previously verified: symbols221, interfaces217, bound-tree136, lowering38, tooling115,
runtime interface29, runtime ABI35, test protocol23 and backend identity1.

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
layouts remain conservative. The remaining nullable join matrix failures lose conditional result shape.
Publish ordinary active/empty shape without deriving presence from ownership or overriding empty layouts.

Loop cleanup loses guard authority after get<T>/opaque execution. Deferred task_frame_borrow_ends and
task_returns_observation_future lose held witnesses before the RunResult join. witness-trace.log shows
capture, not completed-body authority. Locate the removal before editing. Do not remove the admission
availability filter or grant authority while a future remains dormant. Batch independently understood
repairs before broad validation. Keep temporary traces bounded and remove superseded evidence.

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
Latest timing-current.json: all five pass at135-153ms, loop153ms, provider time separate.
Latest counted checkpoint transport-validation.json: loop1,482,433 comparisons and38,060 nodes,
versus prior1,481,729 and38,060. Baseline proof-count-current.json is1,471,543 and38,058. Timing and counts use separate runs. Counter patch retains four measured hooks.
Preserve immutable conditions, scope-keyed ProofMemo, ordered BooleanGraph, role-sensitive snapshots,
actual predecessor evidence, absence/unknown and owner/contents/capture distinctions.
No new cache or evidence representation without removing superseded machinery.
