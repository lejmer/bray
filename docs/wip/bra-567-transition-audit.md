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
Five added controls give fixed912 totals783/30/99. Checker504, bound136 and focused18 pass. All248 owned-transfer and148
deferred cases pass each source/import surface. Cleanup57/4 and predicates26/2/2 retain their known outcomes.
Source/MIR async composition and3 invalid variants pass. Native composition has not run. No style pass yet.

Complete native gate:31 sites with no new identities. Against original native.log30, root_scope265 and task_entry77
cleanup are repaired and3 added E7135 cascades name already-failing polling.stage. Current31 comprises15 E7123
requirements and16 E7135 reports. Eleven E7135 reports explicitly name a failing callable dependency.
Count reports separately from causes. Current E7123 inventory:6 cleanup,4 marker and5 relational requirements.

Established boundaries include completed argument snapshots, consumed reference captures, scoped completion/retirement,
owned versus borrowed contents, and projected-owner nullable joins. Fixed controls reject overwrite, wrong-owner,
fabricated-evidence and escaped-pointer variants. Keep these contracts while tracing later consumers.

## Current boundary

Follow Current acquisition -> borrowed operations -> cleanup, including EntryScope's guarded optional Current.
current(), create_execution() and start_frame() explicitly promise view_owned. Basic and composed small controls pass.
The actual native program remains the reproduction. A rejected small probe was a permission-order setup error and is
not regression evidence. The temporary source harness and all trace hooks are removed.

Native trace: task_start unit1116252 receives a precise Current escape from start_frame. PreparedStart.exit unit946710
(source75) forwards it. The physical-slot batch now prevents the Current.pointer descriptor from being erased.
A targeted producer/consumer trace confirms correct schema, referents and exact emitted slot; subsequent removals affect
address-pointee state only. The next traced edge is in reserve_start unit960156 (source75): an incoming frame pointer
may alias the live slot holding owner.selected. No historical slot participates. escape_effects publishes this possible
alias closure as unconditional input escapes, discarding the caller's more precise storage identities.
Do not flatten entry snapshots into mutable containing descriptors or let cleanup accept stale contents.

Current batch carries proven physical reference slots in the existing invalidation publication. Shared views select
prepared descriptor paths. Memory writes/effects consume exact physical identities. Merge intersects survival and
atomic restriction carries it. Properties cover nested slots, exact replacement, revoked access authority and differing
epochs. The physical-slot checkpoint017522 retained fixed911/native32. Native composition remains unverified.
Escape-summary batch is verified. The real-source test escape_summaries_preserve_input_roots_before_alias_expansion
fails at checkpoint017522: one actual pointer escape publishes3 roots instead of1. Its explicit-reference variant must
retain that reference and its stored pointer. Keep local possible-alias checks conservative, but publish explicit roots
and actual containment. A memory-boundary control also checks real stored-reference escapes through local field addresses.
Source/import4 controls and the memory property pass. Fixed912, both ownership/deferred matrices, checker504/bound136,
adjacent suites and complete native31 are verified. task_start's cleanup failure is repaired with no new native sites.
All temporary trace hooks are removed. Next compare passing task_start with failing initialization/create_execution and
EntryRestoration cleanup, tracing remaining losses beyond the two repaired boundaries. No reverse alias reconstruction.

Use the ABI2 standard-library bundle at target/runtime-bootstrap-standard-library/x86_64-pc-windows-msvc.
Direct233-source CLI checks add3 missing-native-link cascades and are investigation controls, not the native gate.
Remaining completion gates: native async success/panic/cancel/startup rollback/blocked shutdown, result/static disposal,
foreign restoration, architecture/test pruning, final style/format, independent review and PR/Linear review handoff.
