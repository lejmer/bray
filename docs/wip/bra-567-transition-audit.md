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
Four added controls give fixed911 totals782/30/99. Checker503, bound136 and focused18 pass. All248 owned-transfer and148
deferred cases pass each source/import surface. Cleanup57/4 and predicates26/2/2 retain their known outcomes.
Source/MIR async composition and3 invalid variants pass. Native composition has not run. No style pass yet.

Complete native gate:32 sites, unchanged by the current physical-slot batch, with no new identities. Against original
native.log30, root_scope265 is repaired and3 added E7135 cascades name already-failing polling.stage. Current32 comprises
16 E7123 requirements and16 E7135 reports. Eleven E7135 reports explicitly name a failing callable dependency.
Count reports separately from causes. Current E7123 inventory:7 cleanup,4 marker and5 relational requirements.

Established boundaries include completed argument snapshots, consumed reference captures, scoped completion/retirement,
owned versus borrowed contents, and projected-owner nullable joins. Fixed controls reject overwrite, wrong-owner,
fabricated-evidence and escaped-pointer variants. Keep these contracts while tracing later consumers.

## Current boundary

Follow Current acquisition -> borrowed operations -> cleanup, including EntryScope's guarded optional Current.
current(), create_execution() and start_frame() explicitly promise view_owned. Basic and composed small controls pass.
The actual native program remains the reproduction. A rejected small probe was a permission-order setup error and is
not regression evidence. The temporary source harness and all trace hooks are removed.

Native trace: task_start unit1116252 receives a precise Current escape from start_frame. PreparedStart.exit unit946710
(source75) produces it. At call10 a Replace effect removes an established Current.pointer entry slot. At call19 a
returned pointer alias at slot9 participates in transitive escape, which adds owner.selected (field2307). The resulting
Current contents epoch disagrees with its surviving view_owned guarantee in both composite and proof passes.
The nested physical slot cannot be represented by observation_path. Do not flatten its entry snapshot into a mutable
containing descriptor or patch cleanup to accept stale contents.

Current batch carries proven physical reference slots in the existing invalidation publication. Shared views select
prepared descriptor paths. Memory writes/effects consume exact physical identities. Merge intersects survival and
atomic restriction carries it. Properties cover nested slots, exact replacement, revoked access authority and differing
epochs. Fixed911 and native32 retain their expected outcomes. This has not repaired the native composition.
Continue at the producer: establish why the failing operation still supplies no usable physical slot protection, then
trace the same value through subsequent capture, mutation, aliasing, completion, failure and cleanup before rebuilding.
Bound growth to this metadata and its existing producers/consumers. No reverse alias reconstruction.

Use the ABI2 standard-library bundle at target/runtime-bootstrap-standard-library/x86_64-pc-windows-msvc.
Direct233-source CLI checks add3 missing-native-link cascades and are investigation controls, not the native gate.
Remaining completion gates: native async success/panic/cancel/startup rollback/blocked shutdown, result/static disposal,
foreign restoration, architecture/test pruning, final style/format, independent review and PR/Linear review handoff.
