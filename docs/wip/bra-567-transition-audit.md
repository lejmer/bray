# BRA-567 active audit

## Contract

Complete [BRA-567](https://linear.app/bray-lang/issue/BRA-567) on draft [PR710](https://github.com/lejmer/bray/pull/710).
Use feature/bra-567-async-root-orchestration in target/bra-567-audit. Original develop stays untouched.
Checkpoint commits/pushes are authorized. Independent review, PR ready and Linear In Review follow completion.
Never merge or mark Done.

1. Derive transitions from actual CFG producers and ownership, storage, liveness and lowering consumers.
2. Execute async-ownership-composition.bray natively, with invalid variants.
3. Check production before consumption, moved authority retirement, identity versus contents, and loop convergence.
4. Preserve comparison identities/expectations. Explain and repair regressions before expanding migration.
5. Name traced paths, evidence and exclusions. Source/MIR success does not certify native composition.

Retain producer-owned facts. Reconstruction is a last resort. Keep one current-value tree, alias model, witness ledger
and composite storage flow. Distinguish owners, loans, addresses, pointees and contents. Preserve specific diagnostic
causes. Remove duplicate mechanisms rather than adding parallel mappings or new dependency kinds without evidence.
Keep records short/current and delete stale task-owned logs/probes by exact path. Proper Bray formatting applies to fixtures.
No style/format until semantic completion. Then perform the architectural/test-pruning pass requested by the user,
mechanical AGENTS/DRY audit, mutating cargo xtask style and cargo xtask format, inspect changes and rerun affected checks.

Root cleanup precedes terminal publication. Main authority, execution, attachments and providers survive outcome,
incident and selected static disposal. Direct await creates no TCB. Closed admission permits cleanup. Blocked shutdown
returns unresolved ownership. Share teardown state. Sync products retain no async executor or migrated Rust.
Verify worker/deadline omission, native provenance and costs. PreparedStart retains admission/registration through
publication/rollback. EntryRestoration retains displaced context through abnormal exits. RetiredCell disposes control,
pin and arena before vacancy. WaitResources retains leased storage through membership changes.

## Accepted evidence

Audit baseline:b90076c1f21f3853ebfd90954b3babe26f122eca.
Fixed comparison:728 passed /55 failed /99 ignored, unchanged55 identities after pointer preservation. Checker468 and
failure-diagnostics6 pass. Earlier bound-tree133, diagnostics44, messages88, JSON48 and conformance7 pass.
Readiness has not been rerun after the latest cause additions.
Expanded predicate group:25 passed /2 failed /2 ignored. Both reproduce in the retainedfc4 baseline.

Accepted native.log uses compiler code equivalent to68f477051 and runtime Source64a4e5490:153 diagnostics
(E7123=85,E7135=44,E7119=23,E7087=1). The reduction164 to153 removed13 identities and exposed two E7135 dependency
failures: polling171 references observe131..164, worker/entry170 references worker/callback50..120. Both callees already
failed in164 (polling155, callback118). proof.rs61 skips caller certification on source errors, proof.rs557 then reports
DependencyErrors. These are explained exposed failures. All153 identities persist after the diagnostic correction.

Typed execution causes now distinguish loops, panic, cancellation, recovery, nonreturn and input mutation. CFG producers
retain loop/exit identities and share one failure record with certification. Native labels identify the failed loops
and specific panic paths. Only host/attachment/cleanup192 remains locally generic. No native async composition has executed.
The complete transition inventory remains unfinished. No agent is active. No style/format has run.

The partial production_host_request contains129 of233 runtime files. Source Linux/Windows and imported-library controls
passed poll there, while the complete233-file Windows request with canonical Bootstrap roles and the matching native
library reproduced polling184. Bare current inputs bypassed the canonical component mapper. That duplicate mapper is
removed. An owning-boundary regression fails before the correction, and the complete imported graph subsequently passes
poll. The temporary filesystem audit and trace hooks are removed. This partial fixture cannot certify the complete graph.

## Current investigation

Pointer postconditions now preserve evaluated address bits across mutation. Storage flow already distinguishes stable
address descriptors from reachable-state evidence. The guarantee mask had replaced each copied pointer with a different
slot-specific post-state. Completion then reconstructed the input from that slot. The mask now retains address values
while the existing witness ledger revokes reachable-state claims. The redundant completion reconstruction is removed.
Source/import regression:16 cases cover conditional/unconditional producers, parameters, a supported pure foreign
control, staged input before a later field overwrite, pointer replacement and subsequent pointee mutation. All pass.

Bodyless execution promises without a supported foreign assertion now report MissingEvidence before export. Bare extern
and native Bray-symbol cases previously passed source checking and panicked exporting. The C assertion control exports.
One frozen borrowed-result fixture had unnecessary pure/total promises on its unresolved extern. Those promises are
removed, its expected outcomes are unchanged, and it passes. No temporary trace hooks remain.

Next rebuild the matching native compiler/library product and compare the153 diagnostic identities before accepting
native improvement. Then continue the transition audit through remaining proof/execution failures.

## Remaining

Repair native proof/execution failures by transition class, then execute the complete async fixture. Trace Bound and
PatternObservation, ResultBegin/Production/Completion, explicit/default/operator/construction inputs, attempt/completion,
iterate/next/scoped enter/exit, await/yield, cancellation, outcomes/recovery and cleanup through all consumers.
Verify Source/import certificates and native success/panic/cancel/startup rollback/blocked shutdown, result/static disposal
and nested foreign restoration. Audit from_core admission/refcount ownership. Do not weaken initialized-memory policy to
hide task/result318 copy<Uninit<u8>> failures.

Two frozen negatives need genuine fabrication/mutation controls: trusted_implementation_does_not_export_undeclared_authority
and shared_marker_borrows_preserve_the_original_owner_without_mutable_authority. The terminating-branch positive passes,
but its negative opaque call has no owner input. Add a mutable-authority control before changing expectations. Do not
implement the disproven normal_effects hypothesis. Keep a fresh review agent for the committed PR only at semantic completion.

## Execution

One CPU-heavy operation at a time, default Cargo parallelism, no clean. Share original target and target/cargo build dir.
BRAY_LLVM_PREFIX and LLVM_SYS_221_PREFIX use target/toolchains/llvm/active. Tooling uses default stable. Compiler/linker and
produced-executable workflows run outside sandbox. A10s rustc sample averaged2.39cores with all24cores available.
Native uses Rust1.98.1, LLVM bin on PATH, TMP/TEMP worktree target/audit.
Native: target/tooling/xtask.exe runtime-artifact smoke-test --output target/audit/native --profile release.
Current test hashes: compilation2f208104c15db1e3, checker8ec8a76a178a79eb. Parse build.log for fresh executable identities.

Retain baseline-failures.txt67, native-baseline-identities.txt, accepted comparison.log/native.log, native intermediates
and deferred-report-fc4b5160b-baseline.exe for the old controls. Keep only active build/probe logs. No temporary hooks remain.
Git trust: public lejmer/bray, git@github.com:lejmer/bray.git, authenticated lejmer. No goal or review agent is active.
