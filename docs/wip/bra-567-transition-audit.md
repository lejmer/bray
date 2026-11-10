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
Fixed comparison:759 passed /49 failed /99 ignored, all907 identities retained. Checker479, bound-tree135,
messages91, JSON48 and failure diagnostics8 pass. Expanded predicate26/2/2, both failures reproduce in retainedfc4 baseline.
Accepted native.log:46 diagnostics(E7123=28,E7135=18), unchanged runtime Source64a4e5490.
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

The property covers stale contents, distinct owners, distinct mutations and stable replay. Four storage failures and
two predicate failures reproducefc4. Coverage is the same carried owner through aliases. Independent fresh-loan origins
remain to trace. start234's remaining expanded prepared_start requirement is activation.producer(token).

StoragePlan.value_retirements shares private-input observation retirement at completion, independently of admission's
witness transfer. Exact stored-borrow observations use existing typed Storage routes, never a borrow capability twice,
dead/inexact handles or mere possible-alias overlap. Possible-origin bounds revoke evidence separately.

Recursive conditional cleanup converges. expand_region resolves Dispose(type) under a formal Result value, then maps
receiver-relative mutation kinds onto the original region without layout projections or access-type claims. Generic
leaves remain conditional Cleanup(type), unrelated/global effects retain their identities. The formerly stalled test
passes in0.07s. Configured recursion-limit, retained-capture and recursive-default controls pass. Full cleanup/storage57/4
includes the recursive test. All4 failures match the retained baseline. The real source passes the rebuilt driver.

Panic diagnostics retain failed call requirements and actual arguments through the existing proof model. Context lookup
reads the actual captured input state before the failing entry edge, independently of later cleanup or merged evidence.
The red regression now passes. Borrowed Boolean proof reads current contents consistently with reference observations.
The new proof property was red and now passes with false/missing facts, different owner and trusted-authority controls.
Six real sync/async source variants and the compiled inline fixture verify named/anonymous forwarding and reject
mutation or switching owners. The fixed anonymous_total_proof_uses_its_own_field_requirements positive/negative cases
now match their existing expectations. Native clears host/access56 and host/owner144,269,724.

Loop tracing found a header Observation(contents=ScalarValue(x)) and back-edge Add(ScalarValue(x),1). Displacement
now reads existing computed_value on both sides. It retains scalar identity and uses already-proven exact machine
arithmetic. Twelve sync/async cases cover increment, compound guard, decrement, wrap, stalled edge and reset. Four existing
termination controls preserve their pre-fix outcomes. The fixed suite repairs two more positive tests:
slice_bounds_follow_descriptor_identity_through_element_writes_and_replacement and
nested_slice_bounds_require_descriptor_storage_separation. Their replacement/aliasing negatives still reject. No expected
outcomes changed. Native clears all16 remaining E7119 diagnostics. All50 retained diagnostic blocks are identical.
Produced-component transfer repairs activation/provider158 constructing(value,context). @copy Inactive is decisive. The minimal
tuple-context source fails for a fresh copyable component, borrowing it in place passes, and stored-tuple destructuring
correctly reports E7124. Trace: fresh root equals the producer value slot, but projected ValueTransfer was classified
SourceMaterialization. Witness checking separately recognized it as fresh, hiding the lost guarantee until E7123.
Projected produced-slot transfers now share EvaluatedValueTransfer, and the witness check reuses SourceMaterialization
instead of rebuilding freshness from storage kinds. The async control exposed another loss: the contract adapter
replaced Result.0 with Result whenever execution was deferred. Subject inference now preserves the declaration path,
independently of the existing pending-execution guard. Bound-tree135, checker477 and all8 sync/async source cases pass.
All907 fixed outcomes, cleanup/storage61 and predicate30 identities/outcomes are unchanged. A direct member projection
is planned as copying a place and now reports E7124. Native clears provider158 with no added locations or changed retained
blocks. Consumed traces/probes removed.

Guarded requirement proof repairs invoke321. Minimal scalar and borrowed-field programs reproduced failure to derive
A||B||C from A||B after canonical regrouping. Existing implies already proves this, but mixed obligation proof never
consulted it. Mixed proof now uses that implication fallback. Its private direct mode keeps implication acyclic.
A budget control caught clause scanning starving a known operand. Direct truth now runs first and each implication
keeps its own bounded proof. The source stayed red because trusted prove_requirement split OR before calling the
general prover. Its owning-boundary test was red. OR now tries whole-clause proof before branch-specific range checks.
All479 checker tests pass, including256 unrelated clauses, no selected trusted alternative and no promotion from ordinary
occurrences. Six sync/async source controls and both real reductions pass. All907 fixed outcomes, cleanup/storage61 and
predicate30 identities/outcomes stay unchanged. Native clears invoke321, run/advance24 and inactive105. No added locations.
The step34 postcondition advances to its broadcast dependency. Next trace run/abandonment81's second read(cursor).
A minimal conditional mutation/early-return join passes. Add the actual snapshot/default-input path before changing joins.
No trace hook or review agent. Style deferred.

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
produced-executable workflows run outside sandbox. A10s rustc sample averaged2.39cores with all24cores available.
Native uses Rust1.98.1, LLVM bin on PATH, TMP/TEMP worktree target/audit.
Native: target/tooling/xtask.exe runtime-artifact smoke-test --output target/audit/native --profile release.
Always parse build.log for fresh executable identities. Cargo feature selection changes test hashes.
The compilation test binary accepts the7 comparison filters together. Assert all907 identities/outcomes match.

Retain baseline-failures.txt67, native-baseline-identities.txt, accepted comparison.log/native.log, native intermediates
and deferred-report-fc4b5160b-baseline.exe for the old controls. Keep only active build/probe logs. No temporary hooks remain.
Git trust: public lejmer/bray, git@github.com:lejmer/bray.git, authenticated lejmer. No goal or review agent is active.
