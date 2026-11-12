# BRA-567 active audit

## Contract

Complete [BRA-567](https://linear.app/bray-lang/issue/BRA-567) on [PR710](https://github.com/lejmer/bray/pull/710).
Branch feature/bra-567-async-root-orchestration in target/bra-567-audit. Leave original develop untouched.
Checkpoint pushes authorized. After semantic completion obtain independent review, set PR ready and Linear In Review.
Never merge or mark Done.

Inventory actual CFG transitions and their ownership, storage, liveness and lowering consumers.
Prove native async-ownership-composition.bray and invalid variants. Check production before consumption,
authority retirement, identity versus contents and loop convergence. Preserve fixed identities/outcomes.
Unexplained regressions block expansion. Name traced paths, evidence and exclusions.

Keep one current-value tree, alias model, witness ledger and composite flow. Carry producer-owned identities.
Reconstruction is a last resort. Specific structured diagnostics. Internal defects panic at their owning boundary.
Batch independent checks. Proper Bray formatting. Delete consumed probes. Keep records short and current.
No style/format until semantic completion. Then simplify production code, prune redundant tests, audit AGENTS/DRY,
run mutating cargo xtask style and cargo xtask format, and rerun affected checks.

Root cleanup precedes terminal publication. Main authority, execution, attachments and providers survive outcomes,
incidents and static disposal. Direct await creates no TCB. Closed admission permits cleanup. Blocked shutdown returns
unresolved ownership. Share teardown state. Sync products retain no async executor or migrated Rust. Verify worker/deadline
omission, native provenance/costs and from_core admission/refcounts. PreparedStart retains admission/registration through
publication/rollback. EntryRestoration retains displaced context through abnormal exits. RetiredCell disposes control,
pin and arena before vacancy. WaitResources retains leased storage through membership changes.

## Fixed evidence

Pushed90ea88a85. Frozen907:778 pass/30 fail/99 ignored,16 explained repairs, original SHA unchanged.
Six added controls give913:784/30/99. These outcomes, checker507/bound136 and all matrices pass.
Owned248/deferred148 each source/import surface and witness matrices pass. Adjacent cleanup57/4, predicates26/2/2,
focused18 retain outcomes at checkpoint. Real async source/callable/lifecycle MIR and3 invalid variants pass.
Native composition has not run. No style pass yet.

Complete native gate27, with the explained cleanup repair and start_frame dependency cascade. Ten E7123 and17 E7135 reports comprise
12 primary failures and15 explicit dependency cascades. verification.json contains exact accepted identities.
Seven original sites repaired and4 cascades added against original native.log30. Reports are not independent bug counts.

## Current boundary

root_execution source84/unit1086282: root_scope E17, trusted output E18/storage15, EntryRestoration E24/storage22,
entry_current E35/reference30, admit_source_task E78, start_frame E105, dispose_reference E135, scoped E149.
Existing repaired boundaries: exact stored-pointer reads through complete equally typed bounded views, private
borrowed roots excluded from implicit external reachability, observation-aware component discovery, single completion,
and whole-root observed binding canonicalization. Fixed controls and native inventory retain accepted outcomes.

Two red-before properties now cover one joined contents identity for an aggregate's nested owner/shared alias
and owned_snapshot idempotence for an unchanged destination. Active/inactive nullable joining also preserves a
physically retained payload rather than moving it. The composed property checks independently framed views,
retained acquisition identity, conditional payload witnesses and a subsequent join.
Real join62 retained the guarantee but reanchored its acquisition marker in mixed_role_components. The central
owned_snapshot correction preserves nested producers when the destination already owns the snapshot.
root_execution now checks and produces MIR, including conditional EntryRestoration cleanup. All trace hooks removed.
The shortcut applies only to explicit owned observations, since structural data still needs anchoring.
Fixed912, checker507/bound136 and adjacent suites pass. Native27 replaces start cleanup E7123 with a cascade
from already-failing start_frame. No other native identity changes.

Next grouped frontier: task/ownership64, task/waiting27 and scheduler/parking123 lose applicable imported
guard.get pure evidence because select_imported_execution_obligation tests the caller's accumulated diagnostic
bag. Their entry proofs succeed and checked pure/total domains exist, but an earlier error suppresses selection.
Source selection already owns local diagnostics. Isolate imported decision diagnostics and append them afterward.
The real exported/imported regression is red before and green after isolating the decision's diagnostics.
Collected output diagnostics are preserved. Source/imported selectors and neighboring shared-bag checks were traced.
Fixed913, checker507/bound136 and all matrices retain accepted outcomes. No probe hooks remain.
Complete native27 retains all report identities. Three false missing-evidence causes now correctly name the
existing driving.bray checking failure. Only12 reports are primary and15 are explicit dependency cascades.
Adjacent cleanup57/4, predicates26/2/2 and focused18 retain their outcomes.
Panic360 is separate: slice_pointer's imported entry fails count <= slice.length with actual Unknown.
Consumed probe logs and passing exploratory guard programs have been removed.

Remaining E7123 groups: conditional cleanup start79..174, prepared cleanup/start234, driving197, entry_scope422,
root_scope268, attachment/exit464, observer/polling105 and withdrawal58/185/199.
Primary E7135: reserve_start175 registration postcondition and panic338. The three execution/root reports are cascades.
Follow reserve_start into start_frame too. UnboundedMutation Retire with no access type alone proves no defect.

## Execution and remaining gates

One CPU-heavy job at a time. Cargo jobs24, tooling profile, same combined target set. No Rust edits during Rust builds.
Native tools run escalated outside sandbox. Shared target/cargo build dirs, no clean. Selected lowering takes30s.
Use ABI2 bundle target/runtime-bootstrap-standard-library/x86_64-pc-windows-msvc.
Direct233-source CLI checks add3 missing-native-link cascades and are investigation controls, not the native gate.
Remove consumed probes immediately after extracting evidence.

Remaining gates: all native errors, native async success/panic/cancel/startup rollback/blocked shutdown,
result/static disposal, foreign restoration, architecture/test pruning, final style/format, independent review,
PR and Linear review handoff. Never claim complete composition before these gates.
