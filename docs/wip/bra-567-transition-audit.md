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

Complete native gate:30 sites with no new identities. Against original native.log30, root_scope265, task_entry77 and initialization72
cleanup are repaired and3 added E7135 cascades name already-failing polling.stage. Current30 comprises14 E7123
requirements and16 E7135 reports. Eleven E7135 reports explicitly name a failing callable dependency.
Count reports separately from causes. Current E7123 inventory:5 cleanup,4 marker and5 relational requirements.

Established boundaries include completed argument snapshots, consumed reference captures, scoped completion/retirement,
owned versus borrowed contents, and projected-owner nullable joins. Fixed controls reject overwrite, wrong-owner,
fabricated-evidence and escaped-pointer variants. Keep these contracts while tracing later consumers.

## Current boundary

Continue Current acquisition -> borrowed operations -> cleanup and EntryScope's guarded optional Current.
The physical-slot and explicit escape-root repairs are pushed in b363f24d0. task_start cleanup is repaired.

The remaining startup trace reaches create_execution -> own_binding -> install_binding -> abnormal cleanup.
own_binding's producer retains empty reference slots in its inactive Cleanup variant. ReferenceExit publication drops
those slots. Callers then invent opaque pointers bounded by all inputs, including unrelated Current storage.
The source/import wrapper-publication control fails before the repair and passes afterward. Carry the producer's empty
reference source in the existing relation through substitution, package encoding, ordinary calls and lifecycle calls.
Do not reconstruct variant activity from ownership evidence. Empty alternatives cannot erase real pointer origins.
The memory property, symbols221/interface216, fixed912 and checker504/bound136 pass their expected comparisons.
Both ownership/deferred matrices and adjacent suites retain their fixed outcomes. Complete native30 has no new failure identities. Startup cleanup now passes.
Next trace the four scoped-entry cleanup failures together through the guarded optional Current transition.

Use the ABI2 standard-library bundle at target/runtime-bootstrap-standard-library/x86_64-pc-windows-msvc.
Direct233-source CLI checks add3 missing-native-link cascades and are investigation controls, not the native gate.
Remaining completion gates: native async success/panic/cancel/startup rollback/blocked shutdown, result/static disposal,
foreign restoration, architecture/test pruning, final style/format, independent review and PR/Linear review handoff.
