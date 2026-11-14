# BRA-567 current work

Branch: feature/bra-567-async-root-orchestration. PR: https://github.com/lejmer/bray/pull/710.
Continue through final review. Commits/pushes and bounded full matrices are authorized.
Further performance optimization is a separate PR. Keep compiler-owned versions at 1 / 1.0.
Read this record and the work-on-bray-issues contract gate after compaction.

## Current evidence

All evidence below is in target/audit. batch-validation.json records source and binary hashes:
- Checker559, frozen30 and composition5 pass.
- Owned501/505 and deferred294/298 retain the same four failures each in source/import modes.
  No passing identity regressed and no baseline identity is missing.
- Extracted-owned-field composition passes. Nullable and loop cleanup still fail.
- The three corrected host oracles and the host fabrication/mutation controls pass.
- batch-focus.json: generic borrowed aggregate current-referent controls and nine exposure tests pass.
  Opaque mutable-carrier forwarding still fails. Do not describe it as repaired.
- Style and maintained-source formatting pass. There are no installed probes.

Last complete fixed916 run: fixed-move.json has897 pass,11 fail,8 unverified. Focused reruns have
repaired extraction, driver-view, closing-owner, registration, spin-lock diagnostic and three stale
host oracles. The remaining observed failures are nullable extraction, loop cleanup and mutable-carrier
forwarding. This is not a new full fixed916 result. Preserve its identities and failure locations.
Explicit oracle identity change: production_host_lease_cannot_claim_preservation_across_fallible_preparation
is now production_host_lease_retains_authority_across_fallible_preparation.

The eight unverified fixed cases still need bounded runs. The full-provider report fixture exceeded240s.
body-trace.log locates serial body checking through entry/bridge::install, not its hot transitive query.
Previously verified gates: symbols221, interfaces217, bound-tree136, lowering38, tooling115,
runtime interface29, runtime ABI35, test protocol23 and backend identity1.

## Current contracts and unresolved causes

Checked extraction: ValuePreservation::frame must capture extracted_result for the operation's existing
extracted_values before vacating borrowed formal storage. Ordinary borrow observation cannot create
ownership. Explicit empty layouts remain authoritative. The existing framing test covers both.
No new provenance map or reconstruction of historical entry contents.

Reference completion: prepare_returned_values stages exact reference fields before consumption.
Publish them from footprint.references. footprint.observations excludes reference entries.
Return construction can replace the initial reference refresh, so refresh the produced result before
instantiating postconditions. Pass its actual destination through direct, implicit and deferred calls,
including the Completed payload of task observations. Read current committed targets and preserve
reference descriptors and the exact ownership layout. Multiple targets, raw addresses and absent
payloads cannot establish exact borrowed equality. Conditional transfer staging remains guarded.
The mutable-carrier fixture still rejects forward(forward(make(&mut payload))) before mutation.
Next trace must distinguish lost witness authority from lost referent data across the whole call.

Nullable extraction now publishes a whole input-to-result transfer plus a conditional descendant
transfer into NullableValue.marker. batch-focused.log reports the exact layout. Witness separation
checking rejects that descendant despite the enclosing transfer. Trace simultaneous layout composition
and its consumers before changing either. Partial extraction without the complete carrier must still
fail, and a transfer in another alternative cannot authorize separation in this one.
The nullable join matrix failures still lose usable conditional result shape. Preserve authoritative
empty alternatives and never infer presence solely from a surviving child identity.

Loop cleanup still loses guard authority after get<T>/opaque execution. Deferred task_frame_borrow_ends
and task_returns_observation_future lose held witnesses before the RunResult join. witness-trace.log
shows capture, not completed-body authority. Locate the removal before editing. Do not remove the
admission availability filter or grant authority while a future remains dormant.

Oracle corrections follow contracts-and-trust/contract-reasoning.md. Opaque predicates cannot depend
on unrelated ambient state. publish_completion(&Completed) borrows immutable scalar fields.
initialize_host cannot access a private acquisition marker. Merely obtaining a host borrow under
explicit live requirements does not invalidate retirement. Actual host.storage replacement still
rejects, as do mutable completion updates and fabricated owners. These are oracle corrections,
not compiler fixes.

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
Latest timing-current.json: all five pass at143-160ms, loop160ms, provider time separate.
Latest counted checkpoint performance-current.json: loop1,481,729 comparisons and38,060 nodes,
versus baseline proof-count-current.json1,471,543 and38,058. Remeasure counts at the next instrumented
checkpoint. Timing and counts use separate runs. Counter patch retains four measured hooks.
Preserve immutable conditions, scope-keyed ProofMemo, ordered BooleanGraph, role-sensitive snapshots,
actual predecessor evidence, absence/unknown and owner/contents/capture distinctions.
No new cache or evidence representation without removing superseded machinery.
