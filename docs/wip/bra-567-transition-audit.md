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
Fixed comparison:762 passed /46 failed /99 ignored, all907 identities retained. Checker481, bound-tree135,
messages91, JSON48 and failure diagnostics8 pass. Expanded predicate26/2/2, both failures reproduce in retainedfc4 baseline.
Current native.log:44 diagnostics(E7123=26,E7124=1,E7135=17), unchanged runtime Source64a4e5490.
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

PostState names current contents by carried value identity, never alias slots or old contents. Preserve actual owner,
loan and scalar contexts. StoragePlan.value_retirements shares private-input retirement at completion, independently
of admission's witness transfer. Exact stored-borrow observations follow typed Storage routes once, never possible alias
bounds. Those bounds independently revoke evidence.

StoragePlan owns SourceMaterialization versus EvaluatedValueTransfer, including projected produced components. Witness
checking uses that classification, with no separate freshness list. Stored and explicit copies retain their restrictions.
Deferred contracts preserve their declared result-component paths. The pending-execution guard controls publication.
Loop displacement reads computed contents and only already-proven exact arithmetic. Every back edge must advance.
Mixed Boolean proof reuses implication in an acyclic direct mode. Trusted requirement OR checks the whole clause before
specialized branch proofs. Unrelated clauses cannot starve known operands or promote an individual trusted alternative.
Recursive Cleanup(type) expands under a formal Result, retaining region effects without inventing layout projections.

In-place contents framing retains an already explicit Owned context independently of descriptor survival. It creates
no owner context for other observation kinds. Contents must change and replay must stabilize at the owning boundary.
The composition fixture now checks a whole-owner invariant across awaited mutation, loop joins and returned owners.
This exposed stale async completion arguments. Deferred physical transfers now retain evaluated input reference
descriptors. Before completion publishes outputs, their typed routes read current pointees. Publish the same exact
observations in StorageCompletion for subsequent certificate consumers. No syntax or historical-value reconstruction.
The8-case sync/async matrix, Source/MIR fixture and3 invalid variants pass. Physical frame movement, input-slot reuse
and referent retirement pass. All907 identities retained. Normal deferred completion now passes its old positive while
the caught-panic negative still rejects. Expanded61/30 groups unchanged. Native44 unchanged, including every full block.
Receiver input completion now applies the existing coerce operation to the declared passed type. Capture retained the
correct produced value, but completion skipped the implicit borrow before publishing its receiver slot. Physical routes
were correct. The3 source controls pass. Source/import real spin-lock positives now pass while fabrication, reconstruction
and replacement negatives still reject. Native clears deadline/storage374 without other changes. The async fixture also
mutates the owner returned by await, then checks its whole-owner guarantee. All907 identities and14 progress tests pass.
Execution/binding168 and closing144 reduce to union-callback.bray. Both consuming and borrowed matches fail, including
direct observe(reference(...)). A single-path accessor passes. Spelling out the predicate alias does not repair it.
The new6-case guarded_reference_accessors regression is red at its first valid direct call. Proof tracing found a
specification-local storage identity wrapped around the const Call in known evidence, versus a fresh body Expression.
The producer exports two alternative reference origins, no guarded owning result. Do not invent source/variant recovery.
An ordinary Boolean accessor control also fails total proof without trusted predicates. Retaining const Call data inside
the produced reference repairs its6-case sync/async matrix, including another owner and direct mutable-input invalidation.
Call now carries its checked borrowed result type through substitution and portable Typed terms. Predicate comparison
reuses its existing referent normalization without assigning ownership to a computation. Ordinary owned-return calls and
scalar contexts remain distinct. The boundary and portable checks pass, checker482 and all907 fixed outcomes unchanged.
Explicit symbolic reborrows retain their checked value. Predicate substitution classifies reference values separately
from ownership authority, using one is_reference rule for substitution, reborrowing and returned borrows. Identity
substitution preserves a borrowed computation. Direct accessor use passes. The guarded-accessor test remains red at
borrowed branch extraction, so later negative/async arms remain unverified. Checker482, all907 fixed outcomes,15 progress
tests, JSON48, bound135 and expanded61/30 groups pass or retain their accepted outcomes. Hooks removed.
Native adds E7124 at closing142, with all43 prior full diagnostic blocks unchanged. This body already fails at callback144
and its postcondition120. The now-tracked contract is lost when extracting ownership before selecting its union case.
Next use the existing prepared dependency queries to relate a const reference computation to its selected branch. Flow
preparation forbids hidden queries. Do not promote alternative reference-origin bounds to an exact alias. Native rerun
next must repair this transfer too. E7124 currently loses its actual guarantee and conflates copy with carrier separation.
Retain only union-callback and accessor-boolean probes until consumed.
No review agent. Style/format remain deferred.

## Remaining

Repair native proof/execution failures by transition class, then execute the complete async fixture. Trace Bound and
PatternObservation, ResultBegin/Production/Completion, explicit/default/operator/construction inputs, attempt/completion,
iterate/next/scoped enter/exit, await/yield, cancellation, outcomes/recovery and cleanup through all consumers.
Verify Source/import certificates and native success/panic/cancel/startup rollback/blocked shutdown, result/static disposal
and nested foreign restoration. Audit from_core admission/refcount ownership. Do not weaken initialized-memory policy to
hide task/result318 copy<Uninit<u8>> failures.
E7124 still conflates unsupported copying and partial separation without naming the affected guarantee. Preserve the
specific transfer cause and condition at its proof boundary. The retained tuple-context-projection source is its control.

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
Always parse build.log for fresh executable identities. Cargo feature selection changes test hashes.
The compilation test binary accepts the7 comparison filters together. Assert all907 identities/outcomes match.

Retain baseline-failures.txt67, native-baseline-identities.txt, accepted comparison.log/native.log, native intermediates
and deferred-report-fc4b5160b-baseline.exe for the old controls. Keep only active build/probe logs. No temporary hooks remain.
Git trust: public lejmer/bray, git@github.com:lejmer/bray.git, authenticated lejmer. No goal or review agent is active.

