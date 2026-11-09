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

Latest semantic checkpoint07c39b6bb is pushed. Current diagnostics candidate preserves the fixed comparison:725 passed /55 failed /99 ignored, unchanged55 identities.
All466 checker tests pass. Bound-tree133 passed before this candidate. Earlier diagnostics44, messages87, JSON48 and
readiness passed. Expanded predicates:25 passed /2 failed /2 ignored, both failures reproduce in the retainedfc4 baseline.
Accepted native.log uses compiler07c39b6bb and runtime Source64a4e5490:165 diagnostics
(E7123=96,E7135=45,E7119=23,E7087=1). It removes the two event/delivery escapes from the167 candidate,
with no added identities, and18 earlier failures from the183 checkpoint. No native async composition has executed.
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
Temporary diagnostic hooks and qualifier prototypes are removed. Only the current loop-probe remains active.

Failure-only condition rendering now resolves InvocationInput through its actual selected input ordinal. The real
raw-pointer CLI reproducer changed from compatible(inspect(first, second), inspect(first, second)) to
compatible(first, second). Both Bray/C ABI variants and all seven diagnostic tests pass. No proof policy changes.
A minimal nested branch/loop with a raw-state projection and real mutable pointee write passes. Do not infer a
universal join bug. Next trace the actual poll body at184/226 through call input, completion and join states.
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
No debug hooks remain. Git trust: public lejmer/bray, git@github.com:lejmer/bray.git, authenticated lejmer.
