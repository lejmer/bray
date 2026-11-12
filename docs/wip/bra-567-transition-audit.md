# BRA-567 active audit

## Contract and method

Complete [BRA-567](https://linear.app/bray-lang/issue/BRA-567) on [PR710](https://github.com/lejmer/bray/pull/710).
Work on feature/bra-567-async-root-orchestration in target/bra-567-audit. Leave original develop untouched.
Checkpoint pushes are authorized. After semantic completion obtain independent review, set PR ready and Linear In Review.
Never merge or mark Done.

1. Inventory actual CFG producers and ownership, storage, liveness and lowering consumers.
2. Prove native async-ownership-composition.bray and its invalid variants.
3. Check production before consumption, authority retirement, identity versus contents and loop convergence.
4. Freeze comparison identities, expected outcomes and versions. Unexplained regressions block expansion.
5. State traced paths, evidence and exclusions. Source/MIR success does not certify native composition.

Keep one current-value tree, alias model, witness ledger and composite storage flow. Carry producer-owned identities.
Reconstruction is a last resort. Preserve specific diagnostic causes. Internal defects panic at their owning boundary.
Batch independent checks. Use Bray formatting. Keep only necessary current records and remove consumed task artifacts.
No style/format until semantic completion. Then simplify production code and prune redundant tests, audit AGENTS/DRY,
run mutating cargo xtask style and cargo xtask format, and rerun affected checks.

Root cleanup precedes terminal publication. Main authority, execution, attachments and providers survive outcomes,
incidents and static disposal. Direct await creates no TCB. Closed admission permits cleanup. Blocked shutdown returns
unresolved ownership. Share teardown state. Sync products retain no async executor or migrated Rust. Verify worker/deadline
omission, native provenance/costs and from_core admission/refcounts. PreparedStart retains admission/registration through
publication/rollback. EntryRestoration retains displaced context through abnormal exits. RetiredCell disposes control,
pin and arena before vacancy. WaitResources retains leased storage through membership changes.

## Fixed evidence

Frozen907 identities:778 pass/30 fail/99 ignored, with16 explained repairs. comparison.txt and its SHA stay fixed.
Five added controls give fixed912 totals783/30/99. Checker505, bound136 and focused18 pass. All248 owned-transfer and148
deferred cases pass each source/import surface. Cleanup57/4 and predicates26/2/2 retain their known outcomes.
Source/MIR async composition and3 invalid variants pass. Native composition has not run. No style pass yet.

Complete native gate:27 sites with no new identities. Against original native.log30, root_scope265, task_entry77, initialization72, resolution35 and root93/root206
cleanup are repaired and3 added E7135 cascades name already-failing polling.stage. Current27 comprises11 E7123
requirements and16 E7135 reports. Eleven E7135 reports explicitly name a failing callable dependency.
Count reports separately from causes. Current E7123 inventory:2 cleanup,4 marker and5 relational requirements.

Established boundaries include completed argument snapshots, consumed reference captures, scoped completion/retirement,
owned versus borrowed contents, and projected-owner nullable joins. Fixed controls reject overwrite, wrong-owner,
fabricated-evidence and escaped-pointer variants. Keep these contracts while tracing later consumers.

## Current boundary

Physical tags survive joins and owner reanchoring without retaining overwritten payloads. Known Present/Tagged joins
retain their existing handling. Fixed912, owned248/deferred148 each surface, witness matrices, checker505/bound136
and adjacent suites retain accepted outcomes. Native remains27 with no new identities. Checkpoint478e764f2 is pushed.
Single-completion dispatch and canonical whole-root observed aliases pass fixed912, checker506/bound136 and adjacent
suites. Complete native remains27 with no new identities. No trace hooks remain.
Retained-component discovery passes its red-before property, fixed912, checker505/bound136,
owned/deferred/witness matrices and adjacent suites. Complete native remains27 with no new identities.

Stored-pointer reads now use existing field values through a complete, current, equally typed returned view.
Incomplete, opaque, retyped and retired backing remains conservative. A red-before memory property and exact source/import
escape-field checks pass. Ordinary reads and call inputs share this boundary and the canonical projected-type query.

Root_execution's entry_current choice30 has complete current bounds22.scope.current and22.scope.operation.NullableValue.
The repaired pointer read forwards both existing pointer fields. admit_source_task E78 no longer exposes scoped result22,
and its tags/markers survive. Bound mapping confirms start_frame is E105 and dispose_reference is E135.
The complete tag trace proves joins331/62 initially preserve it. E135 disposal drops current/operation observations
although22 is unexposed, then joins62/57 propagate the loss. reachable_targets seeds reference slots with any symbolic
Returned origin as external storage, including a complete borrow of private inline storage. The only reached region
overlapping22 at E135 is choice30.pointer with those complete private bounds. The extended fixed property is red before
repair and checks private backing before/after nested-pointer publication, plus actual owner publication. The root repair
uses existing producer bounds and preserves explicit roots, escaped regions, unknown backing and heap projections.
The fixed property and exact source/import escapes pass. Native start cleanup now reports an unknown payload obligation.
Trace proves the tag survives but nullable fields disappear when an ownership observation joins an untouched value.
component_projections did not inspect observation contents, so joins saw no retained payload fields. The extended existing
preservation property reproduces the exact empty-payload state through repeated framing/joins and checks stabilization.
Canonical component discovery now unwraps actual observation contents and nullable snapshots. Joins and witness renaming
consume it, and redundant caller unwrapping is removed. Repeated-frame/join marker and stabilization assertions pass.
Native cleanup now has a represented tag and payload marker. The proof entry has only the unconditional current-view
guarantee, while its conditional operation-view guarantee is missing. Locate its first loss through the actual ledger,
operation and merge consumers. The ledger trace locates the first loss at join62: both original predecessors have the
same available conditional guarantee on the complete owner24 and operation payload, but renaming conflicts discard it.
Old slot15 remains a live Owned observation of scope17 in standalone guarantee analysis. Its effect on the published
cleanup failure is not established. It is trusted-expression18's output. The plan consumes it at pattern0 Completion, and composite
storage flow retires it. Standalone operation dispatch skipped ordinary completion without checked storage-flow data.
Composite call completion also ran during transfer and again with physical completion observations. The repair makes
standalone operation call transfer then completion once, and leaves composite completion to its physical-data adapter.
The existing red native path still fails after this repair. Follow only the published state's actual ledger next.
Passing synthetic witness variants were removed.
Published storage flow had separate current roots for entered and its produced scoped slot22. ExecutionPlace::storage_identity
ignored whole-root Access bindings. Its producer/observed-name current-tree regression is red before and green after
canonicalizing these roots from the existing storage plan. The cleanup still fails. Next check that the nested scope owner
and its shared alias choose one joined contents identity, rather than merely retaining the same immutable marker.
Do not infer complete composition from the local property.
Selected inspect-lowered reproduces the native path in about30s.
Follow reserve_start E31 as well: start_frame loses token and service-owner proof, and reserve_start fails its registration
postcondition. Both observed calls carry UnboundedMutation Retire with no access type, which alone establishes no defect.
Keep the fixed comparison and native27 identities unchanged unless explained.

Use the ABI2 standard-library bundle at target/runtime-bootstrap-standard-library/x86_64-pc-windows-msvc.
Direct233-source CLI checks add3 missing-native-link cascades and are investigation controls, not the native gate.
Remaining completion gates: native async success/panic/cancel/startup rollback/blocked shutdown, result/static disposal,
foreign restoration, architecture/test pruning, final style/format, independent review and PR/Linear review handoff.
