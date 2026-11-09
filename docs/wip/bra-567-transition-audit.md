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

Audit baseline b90076c1f21f3853ebfd90954b3babe26f122eca. Pushed HEADc91787784.
Comparison722 passed /55 failed /99 ignored in98s, unchanged55 failing identities. Checker466 passed.
Repaired const_borrowed_getters_preserve_the_existing_carrier_identity and union owner forwarding.
Compiler/foreign42 passed /1 ignored, diagnostics44, messages86 and JSON47 passed after diagnostic changes.
Source/import laws cover copied addresses versus carried loans, acquisition versus pointee mutation/replacement,
and disjunctive variant reachability. Callable diagnostics retain actual signatures through rendering and JSON.

Current native run has183 diagnostics: E7123=96, E7135=58, E7119=28, E7087=1. The five wait-resource regressions
are repaired. publish_wait's local ownership requirement now proves. Two diagnostics differ from the old182 run:
observer/polling226 read_state is newly reached after entry repair in the already-invalid poll body, and publish_wait's
certificate now exposes an existing Pure failure at waiting/links39. Keep both active, with their root causes.
No native async composition has executed. Full transition inventory remains incomplete.

Structural registration ownership uses actual retained/acquisition component claims and guarded const getters.
release_wait_target consumes actual target fields and returns actual waiter, deleting raw-read reconstruction.
Keep this improvement while repairing its transitions. Opaque pointer-bearing ownership can depend on pointee contents.
The paired structural acquisition law verifies that distinction without weakening compiler checks.

## Active investigation

Entry observation seeding wrote union payload Inputs into the containing object without a variant, replacing the
whole input with Unknown. CurrentValues.observe_input now seeds one complete symbolic root without component writes.
entry_union_observations_preserve_sibling_owner_values failed before and passes after, including Source/import invalid
replacement controls. Fixed comparison improves57 to55. Native failures remain.

Wait ownership uses one exact getter per active variant. Returning any of several origin alternatives does not establish
exact per-branch identity. Do not promote possible aliases or reconstruct callee bodies. The contract-reasoning specification
permits bounded implication. The new three-variant composition uses actual variant fields, consumes the callback and target,
and returns the independent sibling. Source/import positive and replacement controls pass. All466 checker tests pass and
comparison722/55/99 has unchanged55 failures. A symbolic-borrow experiment broke the existing nested-join law and is fully
removed. No hooks or scratch probes remain.

Diagnostic batch in progress: E7139 CheckingPredicateBlockNotAllowed retains BlockExpression/MatchExpression, a primary
label and const-helper guidance. It validates source blocks independently of reachability once per public Pure candidate.
Producer/text/JSON focused checks pass. Full validation session91843 is running. No other process.

Next semantic class: waiting/links.wait_link promises Pure but calls core.target.expose_address at39. Numerical address
exposure is explicitly impure under execution-guarantees.md73-77. Audit related runtime Pure promises and preservation
consumers before changing anything. Do not weaken the checker to accept that promise. This existing root now explains
publish_wait's exposed E7135. Also trace poll/read_state's lost invoked acquisition across retire_reference.
Always set LLVM bin on PATH and capture exit status when using retained drivers.

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
Finish and verify the predicate-block diagnostic batch, including diagnostic readiness. Keep predicate restrictions unchanged.

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

Retain frozen baseline-failures.txt67 IDs and native-baseline-identities.txt, accepted comparison.log/native.log,
unaccepted native-current.log, native intermediates and deferred-report baseline executable for two old controls.
Keep only active build/probe logs. Delete stale task-owned logs/probes/traces by exact path after preserving evidence.
No debug hooks remain. Git trust: public lejmer/bray, git@github.com:lejmer/bray.git, authenticated lejmer.
