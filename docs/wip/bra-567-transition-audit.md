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

Audit baseline:b90076c1f21f3853ebfd90954b3babe26f122eca. Latest pushed:cab277c75.
Fixed comparison:756 passed /52 failed /99 ignored, exact failure identities unchanged. Checker475, bound-tree133,
messages91, JSON48 and failure diagnostics7 pass. Expanded predicate26/2/2, both failures reproduce in retainedfc4 baseline.
Accepted native.log:82 diagnostics(E7123=38,E7135=24,E7119=20), unchanged runtime Source64a4e5490.
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

deadline.delivery48/167/190 and registration87: domain_owned(domain(owner)) fails after lock(value), before state(value).
Exact trace shows available ledger/global domain_owned(Borrowed(Expression9)), but requirement includes the updated
owner's structural field overlay (state.prepared.locked at PostState13). Evidence exists, argument snapshots differ.
Global predicate normalization to owner identity was rejected:10 added comparison failures exposed unsound mutation
preservation. Fully reverted, including its speculative tests. Do not strip predicate snapshots globally.

Storage flow already defers retirement of admitted private InvocationInput slots until completion. Guarantee flow
retired them at admission, so lock's completion refreshed from a terminal stale borrowed snapshot while the caller's
binding held the current field overlay. Extracted StoragePlan.value_retirements as the shared existing policy and
adopted it in both flows. Admission still transfers/suspends witness authority. surviving_call_values already excludes
private inputs and failure cleanup retires them. The boundary regression distinguishes source slots from private inputs.
Checker475 and bound-tree134 pass. Fixed comparison756/52/99 has exactly the same failures. Storage integration33/4
matches all4 failure identities in retainedfc4 baseline. recursive_inactive_cleanup_report_dependencies_remain_finite
stalls in the current tests, pre-change5d driver and02:09 intermediate test binary, but passesfc4 in0.05s. Stalled
processes were stopped. This convergence regression predates the retirement edit and remains unresolved.
All82 native diagnostic blocks remain exactly identical, excluding build timings. The shared retirement rule removes a real consumer
inconsistency but does not resolve the Returned-origin reference refresh gap. Preserve the refactor. Next trace the
actual invalidation and refreshed arguments for deadline.commit, and the recursive component summary iteration.
Trace: lock invalidation.accesses names local value10, while referents explicitly names both value10 and admitted input11
at state.prepared.locked. Guarantee invalidated_places ignored referents, leaving input11 stale. Blindly treating every
referent as a value update introduced39 regressions by overwriting raw-address bits. Rejected without changing expectations.
An attempted category lookup through value_references was rejected: that map intentionally covers inline slots, not
every reached reference cell. Fully reverted. Do not add fallback classification or expand inline ownership through loans.
Current producer repair uses existing typed storage accesses. For a Borrow-valued direct Storage access,
target_observations follows its retained exact stored target and publishes reached contents as a logical observation.
Dead/inexact views cannot become unknown fresh aliases. Borrow-capability routes already reach their target and must
not be dereferenced again. Raw addresses keep their descriptor interpretation. Existing controls exposed and pin these
distinctions. Unclassified referent bounds remain for conservative revocation. No new representation.
Checker475 and comparison756/52/99 passed, but native88 added6 marker-owner failures and cleared0. Not accepted.
Additions:deadline/storage455, event/registration46, execution/worker_callbacks150/157, worker/callback311, worker/entry179.
Producer was treating possible alias overlap as a particular changed borrowed snapshot. Current restriction requires
exact target containment for the new borrowed-value observation. Existing possible-origin bounds still revoke evidence.
Restricted producer passes checker475 and all907 fixed identities/outcomes(756/52/99). All82 native diagnostic blocks
are exactly unchanged. Producer-only repair does not resolve alias snapshot agreement. Frozen expectations unchanged.
Next pin same-object/same-mutation snapshot agreement at ValuePreservation, including stale-data and distinct-slot controls.
Recursive trace: discard component exceeds400 iterations, adding UnboundedMutation(Cleanup(Wrapper<...<T>>)) each round.
dependency/cleanup/effects.rs expands Dispose types but preserves Cleanup types unchanged, admitting unbounded growth.
Fix must preserve region semantics without treating a captured referent as the disposed value's layout. Existing
full_cleanup_parts and effect normalization own this boundary. No fix yet. All hooks/traces removed. No style/format or agent active.

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
