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

Audit baseline:b90076c1f21f3853ebfd90954b3babe26f122eca. Current accepted work is checkpointed on the PR branch.
Fixed comparison:756 passed /52 failed /99 ignored, exact failure identities unchanged. Checker476, bound-tree134,
messages91, JSON48 and failure diagnostics7 pass. Expanded predicate26/2/2, both failures reproduce in retainedfc4 baseline.
Accepted native.log:70 diagnostics(E7123=30,E7135=20,E7119=20), unchanged runtime Source64a4e5490.
Use the complete233-file Windows request. Partial129-file requests cannot certify it. Native async fixture has not run.

Yield/Break and callable Return now publish the actual planned result slot before cleanup. Completion reads current
contents. Removed duplicate ExecutionState.result. Liveness defines direct initialized storage, never reached borrows.
Producer tag facts preserve nested multiway joins and private catch wrappers. Scoped reference evidence expires without
retiring an independently live pointee. Sole expired views lose evidence, actual retirement/replacement revokes it.
Source/import/MIR matrices:40 produced-result cases,12 caught-nullable sync/async cases,12 scoped-view sync/async cases.
All pass. Accepted native result/tag/catch/view changes remove8 diagnostics with0 additions.

Diagnostic snippets use a shared80-character/first-line formatter. Conditions, messages and structured JSON stay complete.
Unsupported bodyless execution promises report MissingEvidence. Native C assertions remain supported.

## Current boundary

Mutation contents now use the carried value identity, independent of the alias slot. PostState retains that identity,
not old contents, and replay retains its original key. Logical production slots explicitly supply Input(place).
Framing uses the actual owner or scalar context. No new type or second value tree. Do not strip predicate snapshots
back to bare owner identity: mutation/retirement controls require distinct contents observations.

The new property was red before this repair and now passes with stale-content, distinct-owner, distinct-mutation and
replay controls. All476 checker tests, all907 fixed identities/outcomes(756/52/99), storage33/4, expanded predicate26/2/2,
failure diagnostics7 and JSON48 preserve their outcomes. Four storage failures and two predicate failures reproducefc4.
Native70 clears all4 deadline domain/state calls, both native_deadlines calls, two start calls and four deadline
postconditions. No added locations. Seven retained headers now show the produced origin instead of an alias slot.
start234 progresses from selected.status==0 to the next expanded prepared_start requirement, activation.producer(token).
Coverage is same carried owner through different aliases. Independent fresh-loan origins remain to trace.

StoragePlan.value_retirements shares private-input observation retirement at completion, independently of admission's
witness transfer. Exact stored-borrow observations use existing typed Storage routes, never a borrow capability twice,
dead/inexact handles or mere possible-alias overlap. Possible-origin bounds revoke evidence separately.

Next repair the proven recursive convergence regression. recursive_inactive_cleanup_report_dependencies_remain_finite
stalled before this repair, including pre-change5d/02:09 intermediate binaries, but passedfc4 in0.05s. Trace exceeded400 component
iterations: each adds UnboundedMutation(Cleanup(Wrapper<...<T>>)). dependency/cleanup/effects.rs expands Dispose types
but preserves Cleanup types unchanged. Normalize through the existing cleanup expansion while preserving region
semantics. A cleanup region is not the disposed value's layout. Confirmed stalled at f4f82a276 before editing.
expand_region now resolves Dispose(type) through the existing expansion under a formal Result value, then maps
receiver-relative mutation kinds onto the original region without layout projections or access-type claims. Generic
leaves remain conditional Cleanup(type), unrelated/global effects retain their identities. The formerly stalled test
passes in0.07s. Configured recursion-limit, retained-capture and recursive-default controls pass. Checker476 and all907
fixed outcomes remain unchanged. Full cleanup/storage57/4 includes the now-passing recursive test. All4 failures match
the retained baseline. The real source passes the rebuilt driver. All70 native diagnostic blocks are unchanged.
Consumed probes/traces removed. Next inspect E7119's operation-can-panic cause, starting with host/access77 authorize.
No trace hook or review agent. Style/format remain deferred.

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
Always parse build.log for fresh executable identities. Cargo feature selection changes test hashes.
The compilation test binary accepts the7 comparison filters together. Assert all907 identities/outcomes match.

Retain baseline-failures.txt67, native-baseline-identities.txt, accepted comparison.log/native.log, native intermediates
and deferred-report-fc4b5160b-baseline.exe for the old controls. Keep only active build/probe logs. No temporary hooks remain.
Git trust: public lejmer/bray, git@github.com:lejmer/bray.git, authenticated lejmer. No goal or review agent is active.
