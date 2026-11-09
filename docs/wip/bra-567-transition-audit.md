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

Audit baseline b90076c1f21f3853ebfd90954b3babe26f122eca.
Accepted comparison724 passed /55 failed /99 ignored after explicit field-copy repair, with unchanged55 failing
identities. Checker466 passed before the current call-projection extension. Earlier diagnostic batch: diagnostics44, messages87, JSON48 and readiness passed.
The expanded predicate group has25 passed /2 failed /2 ignored. Both failures reproduce in the retainedfc4 baseline:
checked_unsigned_division_guards_preserve_exact_products and joined_index_bounds_require_each_predecessors_actual_value.

Accepted native.log uses compiler Rustc91787784 and runtime Source2fc405bf7. It has183 diagnostics:
E7123=96, E7135=58, E7119=28, E7087=1. Latest native candidate167 has E7123=96, E7135=45, E7119=23,
E7113=2 and E7087=1. It removes18 old failures but adds two escapes in event/delivery92 and151, so remains unaccepted.
No native async composition has executed. Full transition inventory remains incomplete.

## Active investigation

WaitLink retains its actual DataPointer<Slot>, deleting one exposure and five numerical reconstructions plus five
unused intrinsic allowances. Lease/membership requirements remain. The typed-link law verifies execution_properties,
including an impure numeric-address negative. lowered_unit alone does not certify promises.

Borrow-free field reads use the existing storage-shape walker. Checked field selections are required: syntax members
also include methods. Classifying methods as fields regressed the Windows environment reborrow. The corrected domain
restores full standard-library planning and emission. Direct copied pointer/whole Link, stored-reference escape and
borrowed-field escape controls pass in Source/import. Matching-driver controls accept copying before moving the guard
and reject moving before the read. Those two controls are now in the permanent fixture too.

The remaining two escapes reduce to read_link(&mut State)->Link called with an opaque Guard.get view. The equivalent
raw-pointer helper passes. The permanent regression fails before the current fix, and its inferred contract wrongly
retains StorageAlive for the guard. Current candidate prepares copy-type facts from every bound call phase and reuses
contents-copy handling in field, call and pattern transfers. Genuine borrow projections retain their loans. Keep carried
value dependencies and explicit capability/provider/thread/lifecycle roots. Do not project fields onto a lifetime anchor.
The shared phase iterator also omitted deferred_result. The extended publication test accepted a foreign completion
subject before the fix. The iterator now includes it. All133 bound-tree tests and16 Source/import variants pass,
including explicit move ordering and absence of the reading lifetime in copied-result contracts. Query failures propagate
through the canonical dependency mapper. The frozen suite and matching driver are rebuilding. Recheck native regressions
before accepting this batch. No debug hooks or scratch probes remain. No style/format has run.

Next unresolved native transition: poll/read_state loses an invoked acquisition across retire_reference.
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

At semantic completion, reread AGENTS and mechanically audit changed files and behaviors, DRY, APIs, contracts and
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
