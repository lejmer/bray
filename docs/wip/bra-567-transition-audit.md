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

Audit baseline b90076c1f21f3853ebfd90954b3babe26f122eca. Pushed HEAD1ffaadf09.
Entry-state repair comparison721 passed /55 failed /99 ignored in89s, zero new failing identities. Checker466 passed.
Repaired const_borrowed_getters_preserve_the_existing_carrier_identity and union owner forwarding.
Compiler/foreign42 passed /1 ignored, diagnostics44, messages86 and JSON47 passed after diagnostic changes.
Source/import laws cover copied addresses versus carried loans, acquisition versus pointee mutation/replacement,
and disjunctive variant reachability. Callable diagnostics retain actual signatures through rendering and JSON.

Accepted native.log at8f68e51fa has182 errors: E7123=96, E7135=57, E7119=28, E7087=1.
Fresh entry-repair drivers built. Candidate native-current.log has188: E7123=102, E7135=57, E7119=28, E7087=1.
All old identities remain, with six added E7123:
delivery179 dispose_reference,183 release_callback,184/188 release_event, withdrawal64 optional registration,
observer/polling226 set_phase. The last appears after entry repair in an already-invalid poll body.
These six failures are unaccepted. Existing publish_wait requirement also remains unproven.
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

Closer three-variant/callback source reproduces native losses without pointer operations. Its normalized owned contract
becomes an opaque predicate instead of its defined structure. conditional_component_guarantees_survive_complete_union_consumption
is an active failing regression. Inspect the first loss, not downstream calls. Build session52726 has temporary gated
AUDIT normalization/portable hooks and temporary requirement prints in that test. Encoding/decoding and normalization
budget exhaustion are ruled out. Encoding receives Unknown. Capture the first Unknown expression before changing code.
Remove all hooks before commit. Active probe.bray/probe.log captures the three-variant case. No other process.
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
Forbidden match/block predicate syntax currently emits vague E7119. Add its specific owning diagnostic without
relaxing predicate restrictions, then run diagnostic readiness. No new diagnostic is implemented yet.

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
