# BRA-567 active audit

## Contract

Complete [BRA-567](https://linear.app/bray-lang/issue/BRA-567) on [PR710](https://github.com/lejmer/bray/pull/710).
Use feature/bra-567-async-root-orchestration in target/bra-567-audit. Original develop stays untouched.
Checkpoint commits/pushes are authorized. After completion obtain independent review, set PR ready and Linear In Review.
Never merge or mark Done.

1. Derive transitions from actual CFG producers and ownership, storage, liveness and lowering consumers.
2. Execute async-ownership-composition.bray natively with invalid variants.
3. Check production before consumption, authority retirement, identity versus contents and loop convergence.
4. Keep comparison identities/expectations fixed. Unexplained regressions block expansion.
5. Name paths, evidence and exclusions. Source/MIR success does not certify native composition.

Keep producer-owned facts, one current-value tree, alias model, witness ledger and composite storage flow.
Reconstruction is a last resort. Reads must not replace whole-owner contents snapshots. Preserve specific diagnostic
causes. Internal compiler defects panic where detected. User-source and external-input failures use structured diagnostics.
Batch independent fixes/checks. Use proper Bray formatting. Keep records short and delete consumed task-owned probes/logs.
No style/format until semantic completion. Then simplify production code/prune tests, perform AGENTS/DRY audit,
run mutating cargo xtask style and cargo xtask format, inspect changes and rerun affected checks.

Root cleanup precedes terminal publication. Main authority, execution, attachments and providers survive outcomes,
incidents and static disposal. Direct await creates no TCB. Closed admission permits cleanup. Blocked shutdown
returns unresolved ownership. Share teardown state. Sync products retain no async executor or migrated Rust.
Verify worker/deadline omission, native provenance/costs and from_core admission/refcounts. PreparedStart retains
admission/registration through publication/rollback. EntryRestoration retains displaced context through abnormal exits.
RetiredCell disposes control, pin and arena before vacancy. WaitResources retains leased storage through membership changes.

## Current evidence

Frozen907 identities:776 passed,32 failed,99 ignored, with14 explained repairs. comparison.txt is unchanged.
Three added controls pass, giving fixed910 totals779/32/99. Checker503/bound136 and focused18 pass.
All244 owned-transfer and148 deferred cases pass each in source/import. Cleanup57/4 and predicates26/2/2 retain known failures.
The20-case loan fixture covers borrowed/owned/fresh origins, entry/result-established evidence, repeated admission,
source/import and missing-renewal negatives. No accepted invalids remain in the fixed suite. No style pass yet.
Source/MIR async composition and3 invalid variants pass. Native composition has not run.

Current carrier/framing batch is source-validated. Retain actual loan and backing-context evidence in the existing ledger.
Borrowed contexts gain no owned-result transfer. Frame current value and existing formal-input layout atomically.
Actual ownership does not require a formal-input mapping. Normalized/explicit/fresh owners retain identity while overwritten
contents are replaced. Scalar copies read current data. Pointer projection reads current descriptor bits while keeping a
borrowed pointer-slot address distinct. Both newly strengthened boundary properties fail before and pass after their repairs.
An accepted-invalid escaped-pointer regression was found and cleared. The complete fixed comparison has no unexplained changes.
All trace hooks are removed. Current native verification is next.

Original native.log has30 sites. Last native atd6f30b594 also has30, with root_scope265 and resolution110 removed but
new E7123 atdriving201 and resolution124. Those additions remain unresolved until the current batch is checked.
Count sites and cascades separately. Earlier29-site inventory grouped7 cleanup requirements,4 marker requirements,
5 relational requirements and13 postcondition failures, at least8 directly naming already-failing required callables.
Do not interpret totals as bug counts or choose the next patch from a diagnostic alone.

## Remaining

Trace whole producer/consumer families before another implementation batch. Current audit follows physical slot disjointness,
published invalidation, value/layout framing, aliases, input/result/entry carriers, join remapping/predecessor proof,
captured call/Future entry, transfer, retirement and cleanup. Continue through every applicable operation path.

The7 nested-borrow cases were masked by an unused forward function whose own postcondition fails. With unused forward
omitted,2 non-forwarding positives pass and4 negatives reject. Forwarding cases remain blocked by its own postcondition.
Keep expectations fixed, make caller checks independent, and aggregate outcomes instead of stopping at the first failure.
complete_trusted_cleanup separately assumes guarantees without registering witnesses, and lifecycle.contract lacks
witness_subjects. This is an unverified publication gap, not the demonstrated cause of those7 failures. Inspect final
receiver snapshots and retirement before changing it. Share typed subject production/publication, never reconstruct it
from condition syntax. Implicit ScopedEnter/Exit also bypass direct prepare_call_observations. Trace their borrowed fields.

Then verify native async success/panic/cancel/startup rollback/blocked shutdown, result/static disposal and foreign restoration.
Finish production simplification/test pruning, AGENTS/DRY audit, mutating style/format, affected checks and independent
PR review before Ready/In Review. tuple-context-projection.bray remains unresolved and does not establish copy validity.
Two frozen negatives need genuine controls: trusted_implementation_does_not_export_undeclared_authority and
shared_marker_borrows_preserve_the_original_owner_without_mutable_authority. The terminating-branch negative has no owner input.

## Execution

One CPU-heavy operation at a time. No Rust edits during Rust builds. Default24 Cargo processors/16 tooling codegen units.
No clean. Share original target and target/cargo build dir. LLVM prefixes use target/toolchains/llvm/active.
Compiler/linker/produced-executable workflows run outside sandbox. Native uses Rust1.98.1, LLVM bin on PATH and worktree target/audit as TMP/TEMP.
Build compiler, driver and tests together. Capture executable paths/hashes from Cargo JSON in verification.json.
Native command: target/tooling/xtask.exe runtime-artifact smoke-test --output target/audit/native --profile release.
Compare native diagnostics only after the explicit completion marker. comparison-candidate.log holds current fixed-suite results.
Keep only original comparison.txt, current native.log, verification.json and unresolved probes. Consume other logs promptly.
Git trust: public lejmer/bray, git@github.com:lejmer/bray.git, authenticated lejmer. No goal or review agent is active.
