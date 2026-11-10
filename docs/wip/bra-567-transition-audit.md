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

## Current evidence and blockers

Pushed checkpoint4a0d59a90. Both source/import witness matrices pass all positives and negatives.
Canonical return publication removes the duplicate reconstruction map. Nullable wrapper/payload snapshots,
whole-carrier protocols, projected aliases and admitted-entry publication retain owning-boundary checks.

Frozen907 identities:769 passed,39 failed,99 ignored. Six production repairs and one corrected scalar-copy
control explain all differences. Preserve comparison.txt. Extra allocation import case passes. Checker492,
bound135, symbols221 and interfaces216 pass. Cleanup57/4, predicates26/2/2 and focused18 are unchanged.
Source/MIR composition plus3 invalid variants pass. Native composition has not run.
Accepted native baseline:37 diagnostics,22 E7123 and15 E7135, complete233-file request, Source64a4e5490.

Pending FutureStart repair uses complete result transfer instead of identity-only rebasing. The captured
input previously named the old Future after captures moved under Task. All16 direct/Task controls pass,
fabrication rejects, checker492 and fixed comparison are unchanged. Native verification retains exactly37 original identities.
The provider now passes. Newly reachable source consumers have8 positive failures and one invalid loop
accepted. These cases were previously blocked and unverified. Imported consumers panic on CallableSignature
interface0/owner4. Export does contain that signature for change_state. Trace selected graph diagnostics
and resolved owner before changing export or import. Temporary failure traces remain pending consumption.

Error projection now propagates TypeData::Error at its owning boundary. The permanent unresolved-async-input
regression passes with originating E6001 instead of the copying invariant panic. No downstream catch/default.

The grouped147-case deferred source is a cheap experiment. Identical source is stable, but grouped module
context differs from the original harness. Do not replace the fixed tests. Trace existing normal-completion
edges for only_diverging, captured entries/guards for repeated-site failures and Task/container publication.
Do not add preservation exceptions or weaken initialized-memory policy. Batch independent fixes/checks.

Other unresolved families: returned-pointer retirement at driving197, address expiry through mutation/input
postconditions, PreparedStart registration and SpinLock cleanup. The retained tuple-context-projection source
is an unresolved partial-carrier control. Bodyless execution promises do not explain the other failures.

## Remaining

Repair native proof/execution failures by transition class, then execute the complete async fixture. Trace Bound and
PatternObservation, ResultBegin/Production/Completion, explicit/default/operator/construction inputs, attempt/completion,
iterate/next/scoped enter/exit, await/yield, cancellation, outcomes/recovery and cleanup through all consumers.
Verify Source/import certificates and native success/panic/cancel/startup rollback/blocked shutdown, result/static disposal
and nested foreign restoration. Audit from_core admission/refcount ownership. Do not weaken initialized-memory policy to
hide task/result318 copy<Uninit<u8>> failures.
The retained tuple-context-projection source is the partial-carrier control.

Two frozen negatives need genuine fabrication/mutation controls: trusted_implementation_does_not_export_undeclared_authority
and shared_marker_borrows_preserve_the_original_owner_without_mutable_authority. The terminating-branch positive passes,
but its negative opaque call has no owner input. Add a mutable-authority control before changing expectations. Do not
implement the disproven normal_effects hypothesis. Keep a fresh review agent for the committed PR only at semantic completion.

## Execution

One CPU-heavy operation at a time, default Cargo parallelism, no clean. Share original target and target/cargo build dir.
BRAY_LLVM_PREFIX and LLVM_SYS_221_PREFIX use target/toolchains/llvm/active. Tooling uses default stable. Compiler/linker and
produced-executable workflows run outside sandbox. Cargo uses the default24 logical processors and16 tooling codegen units.
Native uses Rust1.98.1, LLVM bin on PATH, TMP/TEMP worktree target/audit.
Native: target/tooling/xtask.exe runtime-artifact smoke-test --output target/audit/native --profile release.
After a build, capture executable identities in verification.json and remove the consumed build log. Cargo feature selection changes test hashes.
The compilation test binary accepts the7 comparison filters together. Assert all907 identities/outcomes match.

Keep comparison.txt as the fixed907 identities/outcomes. verification.json holds current results and executable paths.
native.log holds the current unresolved diagnostics. tuple-context-projection.bray is the one
unresolved source probe. Consume successful logs and old baselines immediately. Build logs exist only while active.
Git trust: public lejmer/bray, git@github.com:lejmer/bray.git, authenticated lejmer. No goal or review agent is active.
