# BRA-567 active audit

## Contract

Complete [BRA-567](https://linear.app/bray-lang/issue/BRA-567) on draft
[PR #710](https://github.com/lejmer/bray/pull/710), branch feature/bra-567-async-root-orchestration.
Use target/bra-567-audit. Original develop stays untouched. Checkpoint commits and pushes are authorized.

1. Derive transitions from executable CFG producers and ownership, storage-flow, liveness and lowering consumers.
2. Verify async-ownership-composition.bray natively, with invalid variants.
3. Check production before consumption, moved authority retirement, identity versus contents, and loop convergence.
4. Preserve comparison identities and expectations. Explain and repair regressions before migration expands.
5. Name traced paths and evidence. Source/MIR success does not establish native completion.

Retain actual producer-owned values. Reconstruction is a last resort. Keep one current-value tree, alias model,
witness ledger and composite storage flow. Distinguish owners, loans, addresses, pointees and contents.
Preserve specific failure causes and typed diagnostic context. Fix vague diagnostics encountered.
Keep verified design improvements even when they do not fix the current failure. Keep this record concise and current.

Root cleanup precedes terminal publication. Main authority, execution, attachments and providers survive outcome,
incident and selected static disposal. Direct await creates no TCB. Closed admission permits cleanup. Blocked shutdown
returns unresolved ownership. Share teardown state. Sync products retain no async executor or migrated Rust.
Verify worker/deadline omission, native provenance and costs. PreparedStart retains admission/registration through
publication or rollback. EntryRestoration retains displaced context through abnormal exits. RetiredCell disposes
control, pin and arena before vacancy. WaitResources retains leased storage through membership changes.

## Current evidence

Latest pushed checkpoint:1b1bcf8e1. The accepted comparison preserves the fixed comparison:725 passed /55 failed /99 ignored, unchanged55 identities.
All468 checker tests pass. Bound-tree133 passed before this candidate. Earlier diagnostics44, messages87, JSON48 and
readiness passed. Expanded predicates:25 passed /2 failed /2 ignored, both failures reproduce in the retainedfc4 baseline.
Accepted native.log uses compiler code equivalent to1b1bcf8e1 and runtime Source64a4e5490:153 diagnostics
(E7123=85,E7135=44,E7119=23,E7087=1). It removes13 old identities from164, including polling184, and exposes two
E7135 dependency failures: polling171 points to observe131..164, worker/entry170 points to worker/callback50..120.
Both callees already failed in the164 baseline (E7123 polling155 and callback118). proof.rs61 skips certification when
caller candidates have source errors, and proof.rs557 reports DependencyErrors for these callees. Repairing the callers
removed that earlier gate. These are explained exposed failures, not implementation regressions.
No native async composition has executed.
Full transition inventory remains incomplete. No style/format has run.

## Active investigation

WaitLink retains DataPointer<Slot>, removing numerical reconstruction while preserving lease/membership requirements.
The pointer transport regression checks execution_properties, including an impure numeric-address control.
Checked field selections, complete phase_contracts including deferred_result, and canonical query-error propagation
are implemented. Methods must not be classified as field reads. Source/import tests retain that boundary.
Borrow-free copied contents exclude reading-anchor loans while preserving explicit scope/provider/thread/lifecycle
subjects through a shared classification. The container-following retained_storage_borrows mode is removed.

The latest native trace identifies a missing handoff through an existing borrowed input: event_state returns
ValueDependencies(Param1 anchor), detach returns ValueDependencies(Param0.head), commit_locked returns
ValueDependencies(Param1 anchor), commit returns StorageAlive(EvaluationStorage).
Current candidate separates physical pointer data from BorrowCapabilityActive(owner) in raw-view production and
preserves that existing requirement kind through staged input mapping. Borrow-free copies discard the reading loan.
All32 Source/import variants pass, including owned and existing-borrow helper chains, overwrites, nullable/tuple
transport, move ordering and genuine borrowed-result/stored-reference escape controls. The raw-borrow unit fixture
now supplies its actual second pointer input, preserving all owner/capability assertions. Fixed suite has no regression.
The matching native build removes both delivery escapes without new diagnostics. No new dependency representation is added.
Earlier diagnostic hooks and qualifier prototypes are removed. No standalone probes or trace hooks remain. The current temporary import audit is described below.

Failure-only condition rendering resolves staged inputs by selected ordinal. All seven diagnostic tests pass.
Actual poll trace: both predecessors of the READY/FAILED protocol join hold the same invoked claim. Renaming the
unchanged predecessor drops it because an implicit nested field is exposed only in the joined snapshot. The owning
result_replacements regression fails first by renaming an unchanged child, then by reducing its implicit retained
field to Unknown. Candidate correction pins identical component observations and traverses actual retained projections
from both snapshots, replacing source-only field/nullable cases. It adds no reconstruction or dependency construct.
The overwritten-field control passes, all467 checker tests pass, and actual poll storage flow has no diagnostics.
A permanent production regression selects the exact observer module/function through symbol identity. Its final build
passes. The fixed comparison has726 passed /55 failed /99 ignored with unchanged55 identities. Matching native diagnostics
fall165 to164, removing only polling226. The partial production_host_request includes129 of233 runtime files. Linux/Windows and imported-library controls
pass there, while the complete233-file Windows request with canonical Bootstrap roles and the matching native library
reproduced polling184 in12s. Root cause: join7 begins with invoked on both paths, but current_replacements has a
separate bare-input path that bypasses nested component mapping. It drops the unchanged path's fact before loop entry.
A direct current_replacements regression fails before the candidate. The candidate removes that duplicate mapper and
routes all live values through result_replacements. All468 checker tests pass. The complete imported runtime request
now checks poll without diagnostics. The two-target permanent source regression passes. Temporary hooks, probes and
the ignored filesystem audit are removed. Fixed comparison remains726/55/99 with unchanged55 failure identities. Native verification is accepted as described above. The complete imported-graph audit is removed after preserving the boundary regression.
ExecutionCandidate now shares the typed verification failure record with certification. CFG construction retains
loop expression and exit-node identities. Text/JSON preserve termination, panic, cancellation, recovery, nonreturn and
input-mutation causes. Source while/continue/loop/panic regressions pass. A panic control used an invalid integer
message and now uses a string, including the existing conformance control. Checker468, bound-tree133, diagnostics44,
messages88, JSON48, conformance7 and failure-diagnostics6 pass. Fixed comparison remains726/55/99 with unchanged55
identities. Matching native run keeps exactly153 identities. Labels now identify failed loops or specific panic paths.
Only host/attachment/cleanup192 remains locally generic. No style/format has run. No temporary hooks or audits remain.

Current small pointer-contract.bray probe: requiring/forwarding ready(DataPointer) passes, but a conditional producer
followed by inspect(pointer) fails E7123 when pointer is address_of_mut of a local value. The producer itself is the
reported invalidation. Separate pointer copying, conditionality and pointee mutation before changing semantics. Add a
real negative that overwrites the pointer value. The complete native graph remains the required composition check.
Builds/tests involving native child processes run outside sandbox. A10s compiler sample averaged2.39cores, affinity
allows all24cores, and outside-sandbox builds no longer emit hard-link fallback warnings. Do not assume single-core pinning.
Other false Pure promises still use raw address observations. Do not weaken checks for false promises.

## Remaining work

Repair native regressions and proof failures by transition class, then execute full async composition.
Trace Bound/PatternObservation, ResultBegin/Production/Completion, explicit/default/operator/construction call inputs,
call attempt/completion, iterate/next/scoped enter/exit, await/yield, cancellation, outcomes/recovery and cleanup.
Map actual producers in analysis assembly/execution/result/control to ownership, storage, liveness and lowering.
Verify Source/import certificates, native success/panic/cancel/startup rollback/blocked shutdown, result/static disposal
and nested foreign restoration. Audit from_core admission/refcount failure ownership.
Do not change initialized-memory policy to hide result-copy proof failures.

Two frozen negatives need genuine fabrication/mutation controls:
trusted_implementation_does_not_export_undeclared_authority and
shared_marker_borrows_preserve_the_original_owner_without_mutable_authority.
The terminating-branch positive passes. Its negative opaque call has no owner input. Add a mutable-authority control
before changing expectations. Do not implement the disproven normal_effects hypothesis.

After all errors are resolved, perform the user-requested architectural pass: simplify production mechanisms and
remove or shrink redundant tests with retained behavioral evidence. Keep the comparison suite fixed until correctness
is established. Then, at semantic completion, reread AGENTS and mechanically audit changed files and behaviors, DRY, APIs, contracts and
structural cost. Only then run mutating cargo xtask style and cargo xtask format, inspect edits and affected checks.
No style/format has run. No goal or agent is active. Dispatch the authorized independent reviewer on committed PR710
only at semantic completion. Reviewer publishes and returns GitHub artifacts. Resolve and rereview with the same agent,
then mark PR ready and Linear In Review. Never merge or mark Done.

## Execution and artifacts

One CPU-heavy operation at a time, default Cargo parallelism, no clean. Shared original target and target/cargo build dir.
BRAY_LLVM_PREFIX and LLVM_SYS_221_PREFIX use target/toolchains/llvm/active. Tooling builds use default stable.
Native LLVM/linker/executables run outside sandbox with Rust1.98.1, LLVM bin on PATH and TMP/TEMP worktree target/audit.
Native command: target/tooling/xtask.exe runtime-artifact smoke-test --output target/audit/native --profile release.
Five-package tooling compilation test hash a64b618a384421e7, checker0bc5fe31253c00a4.

Retain frozen baseline-failures.txt67 IDs and native-baseline-identities.txt, accepted comparison.log/native.log, native intermediates and deferred-report baseline executable for two old controls.
Keep only active build/probe logs. Delete stale task-owned logs/probes/traces by exact path after preserving evidence.
Git trust: public lejmer/bray, git@github.com:lejmer/bray.git, authenticated lejmer.
