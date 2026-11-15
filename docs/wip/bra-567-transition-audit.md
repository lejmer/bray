# BRA-567 current work

Branch: feature/bra-567-async-root-orchestration. PR: https://github.com/lejmer/bray/pull/710.
Continue through final review. Commits/pushes and bounded full matrices are authorized.
Further performance optimization is a separate PR. Keep compiler-owned versions at 1 / 1.0.
Read this record and the work-on-bray-issues contract gate after compaction.

## Current evidence

Pushed checkpoint: aeb36316b. Current source is uncommitted.
Checker: 559 passed. Fixed/matrix comparison: 1,714 passed, proof-reuse check running.
validation-current.json preserves fixed 916 identities, owned 505, deferred 298, frozen 30,
focused controls, binary/source hashes and exact outcomes. Session 71535 owns the remaining check.
The three whole-provider gates pass, including report consumers. They share one CPU-parallel
Compilation. Small caller fixtures remain serial. Peak private memory previously plateaued at 3.8 GB.
No condition-node leak was demonstrated. Stale traces and superseded validation files were removed.

Observer stage now lends the actual pointer slots to its permit and take_transfer/finish_transfer.
transfer_entry borrows slots, so equal copied pointers cannot substitute for those slots.
Borrow normalization and invalidation share storage_borrow_identity and retain checked StoragePath.
Writes retire slot evidence, including equal-content writes, without retiring independent copied data.
Descriptor-only retention and real transfers keep their existing rules. Constants retain scalar context.
No new authority, cache or historical reconstruction. Production growth is 34 net lines.
Existing tests cover direct/copied aggregate borrowing, distinct copies, equal overwrites, nested
replacement and observer certification on Linux and Windows. slot-coupled.json: 32 passed.
Trusted-obligation diagnostics now preserve the specific Unproven cause through dependency reporting.

Style and Bray formatting passed. Temporary instrumentation is removed.
Native tool rebuilt from current source. Session 25347 runs the bounded native artifact smoke gates
outside the sandbox, with native-current.json/log. Native Rust uses installed 1.98.1, matching LLVM
22.1.8. Default Rust 1.99 produces incompatible LLVM 23 bitcode. No native completion claim yet.
Timing controls pass under concurrent provider/native work: four cases 220–315 ms, loop 424 ms.
Current counts remain the previous checkpoint's measurements, not new-source evidence.
Remaining: proof reuse, native gates, full PR maintainability audit, checkpoint/push and independent review.

Previously verified unaffected gates: symbols 221, interfaces 217, bound-tree 136, lowering 38,
tooling 115, runtime interface 29, runtime ABI 35, test protocol 23 and backend identity 1.
Preserve fixed identities and rejection sites. The deliberate oracle rename is
production_host_lease_cannot_claim_preservation_across_fallible_preparation to
production_host_lease_retains_authority_across_fallible_preparation. Other corrected host
oracles follow contract-reasoning.md. Replacement and fabrication must reject.

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
Latest counts-current.json on aeb36316b: loop1,482,598 comparisons and38,068 nodes,
versus previous1,482,315 and38,038. All five controls pass and release every measured condition node.
Timing and counts use separate runs.
Counter patch retains four measured hooks.
Preserve immutable conditions, scope-keyed ProofMemo, ordered BooleanGraph, role-sensitive snapshots,
actual predecessor evidence, absence/unknown and owner/contents/capture distinctions.
No new cache or evidence representation without removing superseded machinery.
