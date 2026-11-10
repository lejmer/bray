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
Fixed comparison:762 passed /46 failed /99 ignored, all907 identities retained. Checker484, bound-tree135,
messages92, JSON49 and failure diagnostics8 pass. Expanded predicate26/2/2, both failures reproduce in retainedfc4 baseline.
Current native.log:39 diagnostics(E7123=24,E7135=15), unchanged runtime Source64a4e5490. No added failure identities.
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

StoragePlan owns SourceMaterialization versus EvaluatedValueTransfer, including projected produced components.
Retirement occurs after destination publication. PostState names new contents by carried value identity. Framing
retains an existing Owned context without preserving overwritten contents or inventing ownership. Loop replay stabilizes.

The async fixture checks marker and whole-owner guarantees across moves, calls, mutation, awaited returns, loops,
scoped exits and cleanup. Source/MIR and its3 invalid variants pass. Full native execution remains blocked.
Const reference results retain computation data inside the produced loan. Call carries its checked borrowed result type
through substitution and portable Typed terms. Reference-value classification is independent of ownership authority.
Identity substitution preserves borrowed computations. Untyped owned-return computations and scalar contexts stay distinct.

Symbolic reference origins are prepared from the existing dependency query, keyed by callable, dispatch and witnesses.
Flow performs no hidden queries. Proof resolves forwarded alternatives only when branch facts leave one origin.
Independent and broad-region origins cannot establish equality. All16 sync/async accessor cases pass, including wrong
owners and an extra origin that the union case cannot exclude. Required witness subjects retain their actual expressions
until prepared origins are available. Computation-only Boolean selector inputs no longer become authority carriers.
Origin payload paths and marker containment survive normalization. Independent entry/completion clauses stay separate.
Branch refinement publishes established payload clauses on the same ledger before joins. The fixture passes sync/async
closing and release, including the returned postcondition. Missing-entry and fabricated-result variants reject in both modes.

CurrentValues shares PreparedBodyAnalysis.observed_types by Arc and tracks observed entry components as metadata.
It projects live subtrees from that inventory after partial moves. Inventory never enters the changed data slots used by joins.
Values stay absent before production, moved parts stay absent, and replacement initializes the new owner's components.
Parameter and produced owners pass sync/async source/MIR controls and missing-authority negatives. All18 focused checks,
all907 outcomes and expanded61/30 groups pass or retain their frozen outcomes. No preservation exception or runtime workaround.
The full native request clears closing142 and2 dependent execution-contract failures. Retirement141 and event/wait registration
are clear. The owning-boundary assertion that metadata stays out of data slots passes.

E7124 now retains the actual carrier, affected live guarantee and copy/separation cause through rendering and JSON.
Native driving197 loses the shared Current marker at event.wait_execution's raw retirement. The returned task pointer
remains bounded by the execution owner's stored pointer. No preservation exception has been added.
A smaller verified defect is allocation freshness lost through a bodyful return: direct allocate/free preserves an
unrelated owner, while the same allocation returned through a helper loses its guarantee at free. The new fixed fixture
fresh-allocation-returns.bray covers sync/async forwarding. Carry producer allocation identity and alias groups through
reference contracts, substitution and interfaces. Independent returns must remain conservative. Publication must wait
for completion, preserve aliases, grant no initialization authority and retire old incarnations on replay.
This batch passes sync/async source/MIR, source/import consumers, the independent-return negative and allocation alias/replay boundaries.
All907 frozen outcomes remain unchanged, with the new import test passing separately. Checker485, bound135, symbols221,
interfaces216 and the expanded61/30/18 groups pass or retain their frozen outcomes. The complete233-file native request remains39 with identical failure identities.
All temporary trace hooks are removed.
No review agent. Style/format remain deferred.
Consumed probes/traces are removed. The source fixtures live under execution_guarantees/fixtures.
Build brayc and xtask together to retain Cargo feature unification.
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

