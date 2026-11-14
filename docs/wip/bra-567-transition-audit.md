# BRA-567 current work

Branch: feature/bra-567-async-root-orchestration. PR: https://github.com/lejmer/bray/pull/710.
Continue through final review. Commits/pushes and bounded full matrices are authorized.
Further performance optimization is a separate PR. Keep compiler-owned versions at 1 / 1.0.
Read this record and the work-on-bray-issues contract gate after compaction.

## Current evidence

The current batch preserves nullable payload owners, portable nullable contents, false ordinary guards,
private nested-frame custody and complete inexact private-region views. No probes are installed.
- custody-shape-validation.json: owned505/505, deferred298/298, frozen30 and focused19 pass.
  No missing identities or regressions in those groups. Both former sets of four matrix failures pass.
- Checker exposed one wrapped-absence regression. The existing known_presence query repairs it.
  custody-shape-checker-fixed.log: all559 pass. Matrices precede this final one-line correction and
  must be included in the next combined validation. Style/format pass.
- Last pushed checkpoint8dcca93cc fixed-current.json:907 pass,1 fail (loop cleanup),8 unverified.
  These broad fixed916 results precede the current batch. Source/interface/native gates remain.
- Full-provider report exceeded240s. body-trace.log locates serial checking through bridge::install.
  Previously verified: symbols221, interfaces217, bound-tree136, lowering38, tooling115,
  runtime interface29, runtime ABI35, test protocol23, backend identity1.

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

Ownership alternatives retain proven true and false ordinary payload guards. Unknown stays unknown.
A false guard qualifies the actual empty layout, never an invented payload. Nullable joins compare
stored payload snapshots and use known_presence for inactive wrappers. Equal owners retain identity,
not overwritten contents. Portable tag+inline preservation never freezes generic/protected data.
Private nested-frame custody uses existing recorded child admission and actual retained owners, never
outer admission or public availability. Actual writes still revoke captured evidence. Preserve d302124.
Inexact complete private-region views stay private. Unknown/displaced/escaped regions remain conservative,
and no view acquires exact identity or initialized-memory authority. Existing controls cover these rules.

Loop cleanup remains unresolved. nullable-loop.log shows retired escaped Returned origins overlap a
fresh local guard because returned_inputs is keyed only by place and accumulates both evaluations.
State::retire also omits returned_inputs when retiring referred storage, unlike Transfer::retire_dependencies.
A repair must distinguish current/previous bounds and update referenced epochs, while old returned
references can still alias an unchanged outer-loop object. Merely declaring old returned values disjoint
or dropping escaped history is unsound. Trace staging/commit, observe_reference, capture/relocation and
join before changing this representation. No epoch patch is installed.
Proposed owning change: key existing State/Transfer.returned_inputs by (previous, place), preserving
one finite historical summary. Stage bounds only in Transfer, then retire old origins and publish
current bounds in commit_value. observe_reference retires before publishing its own bounds. Retirement
updates referred epochs in State as Transfer already does. Relocation maps keys and referred targets.
Returned-to-owned-storage alias checks require the bound's actual storage epoch, even for unknown
external alternatives. Unchanged outer-loop storage must remain aliasable. Existing returned_borrow_regions
and recreated_local tests will cover same-site return, copied old reference, fresh local, and explicit
escape. Its current first-region accumulation expectation should become Disjoint only for the new
return, while an old return must keep its original bound. Generic RawBuffer and deferred matrices guard
staging/relocation. No second history map, reconstruction or new public API. Reconsider if a staged
consumer needs bounds before commit, and resolve it through the owning Transfer rather than publishing
unproduced output into State.

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
Latest counts-current.json on8dcca93cc: loop1,482,315 comparisons and38,038 nodes,
versus previous1,482,433 and38,060. All five controls pass. Timing and counts use separate runs.
Counter patch retains four measured hooks.
Preserve immutable conditions, scope-keyed ProofMemo, ordered BooleanGraph, role-sensitive snapshots,
actual predecessor evidence, absence/unknown and owner/contents/capture distinctions.
No new cache or evidence representation without removing superseded machinery.
