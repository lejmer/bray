# BRA-567 transition audit

## Completion contract and compaction guard

The objective is to implement BRA-567 and complete the agreed design in [the implementation handoff](bra-567-handoff.md).
The compiler repairs and regression reductions below are intermediate evidence. They do not replace that objective.
After compaction, reread this section before choosing more work.

1. Derive the transition inventory from executable CFG producers and the actual ownership, storage-flow, liveness,
   and lowering consumers. For every relevant operation and failure edge, record what exists before it, what it
   produces, what it consumes, and what survives failure. A category such as calls is not a completed inventory.
2. Prove one complete real async composition before expanding the migration. Trace the same owner across temporary
   production, calls, borrows or mutation, branches, loops, suspension, task outcomes, cancellation, and cleanup.
   Pair the valid program with invalid variants whose intended rejection is demonstrated. Native execution is required.
3. Test the architectural properties at their owning boundaries: no consumption before production, no authority left
   usable after owner movement, no overwritten contents preserved merely by retained identity, and stable repeated
   loop analysis. Passing incidental examples does not substitute for these properties.
4. Keep test identities, expected outcomes, and code versions fixed for comparison. An unexplained regression blocks
   migration expansion until explained and repaired. Investigate transition defects, not individual runtime failures.
   Correct a demonstrably invalid new fixture explicitly, with the specification reason and evidence retained.
5. Make every verification claim name its paths, evidence, and exclusions. Unit or Source passes cannot become an
   end-to-end claim. The agreed design remains unverified while required composition paths are blocked or unexecuted.

Continue on PR #710's branch and commit/push coherent checkpoints. Write inline Bray fixtures in Bray formatting.
Defer automated style and formatting passes until the audit and semantic repairs are complete.

The latest pushed source is 1ba7a3100. Fixed comparison gates pass with no unexplained
comparison regression. Native planning has no remaining moved-value diagnostic, but still fails on trusted obligations
and execution guarantees. No native composition has executed. The copied-pointer owning-boundary regression and
all 446 checker tests pass at that checkpoint. The fixed trusted population is 189 passed, 48 failed, 97 ignored with
no newly failed old identity. Native planning remains blocked. The active gate is carried loan requirements versus physical storage anchors
for returned references. The receiver repair passes the real std.format.bytes body and preserves
the local-owner escape rejection. Projected-input mapping now selects the original evaluated component's
portable dependencies. The generator still fails in liveness, which retains its container loan.
All temporary probes are removed. No native fixture has executed.
The ledger remains incomplete. Source passes do not certify runtime execution.

### Executable ledger for the current blocking boundaries

These rows trace code paths, not completed native coverage. The failure edges are part of each transition.

| Operation and producer | Before and production | Consumption and failure | Actual consumers and evidence |
| --- | --- | --- | --- |
| Invocation Input, `execution.rs::begin_invocation` | Actual receiver and argument values exist. Borrow inputs are prepared. No result exists. | A failed precondition bypasses Attempt and Completion. Existing input owners remain cleanup obligations. | `AnalysisOperationKind::storage_plans` excludes self-produced accesses. Liveness `collect_edge_lifetime_ends` includes failed-invocation subjects on the bypass edge. Existing admission and cleanup controls still require final native evidence. |
| Invocation Attempt, `execution.rs::push_invocation` | Admitted inputs exist. Input transfers use Admission consumption. | Invocation failure cannot publish a result. Completion is only on the successful continuation edge. | Storage and trusted transfer checks use the shared phase selector. OutputDefinition plans never act as source reads. |
| Invocation Completion | Returned references are staged, then `memory.complete_transition` publishes them. | Result-reaching operations run after publication. Failure does not traverse this operation. | `apply_storage_operation` publishes before applying produced accesses. `TrustedChecks::after_operation` checks those transfers after semantic completion. |
| Custom index returned borrow, `expression/access.rs::custom_index_access` | Receiver and selector borrows are inputs. The indexer produces a borrow descriptor with its own stored borrow type. The reached item is a different value. | An ordinary item copy does not consume the reached owner. A live witness forbids that copy. | The existing InvocationResult binding identifies the descriptor. `invocation_result` stages its reference contract. `reference_target` resolves reached storage for the later copy. All 11 custom-index controls pass at 0e2e943d3. |
| Built-in field or index projection, `project_access` | A produced loan descriptor and its reached storage already exist. Projection extends the same source path. It produces no independent owner. | Retiring the descriptor at an intermediate projection destroys the source of subsequent projections. The repair defers retirement to the actual consumer. | Frozen `returned_borrow_survives_field_and_index_projection` had two baseline moved-value errors. It and all eight adjacent value-lowering tests now pass. |
| Actual consumer of a projected loan | The source path still identifies the producer's materialized borrow slot. The consumer first captures reached data or forms a new borrow. | Completion consumption retires the temporary descriptor, independently of the original referent owner. User parameter and binding storage are excluded by the producer/value-slot identity check. | `consume_expression_value` records the same consumption metadata read by storage flow and liveness `OperationEffects::from_storage_plan`. The frozen lowering regression passes. |
| Await suspension, `execution.rs::build_await` | The Future operand has been evaluated. No awaited result exists. The operation branches to a suspended block. | Resume uses InvocationCompletion and only that edge publishes Call Completion. Panic and cancellation use InvocationFailure and resolve scopes through `push_exit`. | `flow/deferred.rs::deferred_alternatives` selects the actual deferred value. Storage flow records suspension before effects and does not publish the awaited result at suspension. Liveness includes current uses and live-out subjects. Async analysis requires the correlated suspension state. Native paths remain unexecuted. |
| Suspension retention | Current live storage, initialized storage, moved paths, active loans and cleanup dependencies exist. | A merge unions possible live and moved obligations, but intersects definite initialization and active loans. It cannot promise one predecessor's initialized state on all paths. | `core.rs::record_suspension`, `liveness/solve.rs`, `asynchronous/check.rs` and lowering `cleanup/frame.rs::frame_retention` consume the same checked subjects. Frame retention also includes pending report owners and exact execution lane requirements. Source composition lowers, native retention remains unverified. |
| Opaque effects on a copied descriptor | The private input has Scalar context plus an immutable source value. Physical survival distinguishes complete value from descriptor-only survival. | Descriptor-only survival must replace reached contents without replacing the copied descriptor's origin or creating owning authority. | `ValuePreservation::apply_bounded` discarded that origin. The real dynamic wrappers and `preserving_a_copied_descriptor_keeps_its_value_origin_without_old_contents` now pass at f1b8bc032. |
| Nullable component publication at a loop join | The root snapshot already contains payload data. Reading its NullableValue produces an extraction view that retains its parent. | Republishing that view as payload data must not embed the same parent inside its own payload. Otherwise equivalent states alternate and the solver does not stabilize. | `CurrentValues::set_slot` and `update_parent_observations` use `with_storage_component`. The frozen law and both previously stalled generator/default lowering tests pass at 02d82b5bc. |
| Shared or mutable implicit receiver entry | `self` has reached Self type, while its checked StorageBinding is an Access through the entry loan. A consuming receiver instead has an Identity binding. | Reborrowing a field retains that entry loan. It must not require the private receiver argument slot to outlive the result. A real local owner still cannot escape. | `result/sources.rs::reborrowed_receiver` missed this distinction. The working repair consumes StoragePlan rather than inferring the receiver mode from expression type. Real Buffer and std.format.bytes pass, local escape rejects, and the consuming stored-reference control passes. |
| Selected input and declaration-owned default | `execution.rs::build_selected_inputs` evaluates each explicit argument, then emits Input, Attempt and Completion for its receiver-first ordinal. Defaults can fail at Attempt. | Only successful Completion initializes the private input. Earlier initialized inputs survive a later default failure for cleanup. Parent invocation admission follows all successful inputs. | Checked source/materialization roles feed the shared operation selector, liveness boundary uses and lowering's checked constructor input metadata. Source operation-cleanup/default lowering passes. Native rollback remains unexecuted. |
| Pattern observation and binding | `build_pattern_observations` walks child patterns before recording PatternObservation. A checked pattern reads its established subject. | Observation forms no owned value. Binding transport happens at the later Bound pattern operation, with actual subject dependencies. Failed match edges cannot publish successful bindings. | `ResultTransfer::operation` separates pattern observation from binding, and storage flow consumes checked pattern metadata. Generator local-storage escape controls pass. Full pattern alternatives remain to be mapped. |
| Scope cancellation broadcast | `assembly.rs::push_scope_exit` records TaskCancellationBroadcast before LifecycleResolution. Unresolved nested task owners are still live. | Broadcast requests cancellation without consuming cleanup authority or waiting. A scoped asynchronous exit is shielded and resumes through its own Completion before the remaining phases. | `guarantee/cleanup/transition.rs` and storage-flow cleanup switch on the exact phase. Native cancellation order remains unexecuted. |
| Scope lifecycle resolution and exit payload | `cleanup.rs::push_exit` preserves Return, ResultErrorPropagation, Yield and normal fallthrough values while resolving scopes. Panic and cancellation retain incidents instead. | Lifecycle cleanup resolves owners before terminal publication. A cleanup failure takes its dedicated failure edge, and catastrophic divergence bypasses cleanup by design. | Liveness `transfer_operation` retains exit dependencies only for the preserving lifecycle phase. `collect_liveness` records actual cleanup suspension subjects. Lowering and native terminal publication still need final correspondence checks. |
| Task outcome completion | Storage flow splits feasible task outcomes before applying completion effects. A failed outcome cannot publish successful payload guarantees. | `apply_task_outcome` applies failure effects when the selected task completion is false. Completed alternatives merge only after each has its own effects and result evidence. | `apply_operation_branch`, `split_task_completion` and `merge_completed` are the actual analysis path. Source composition checks the real outcome selection. Native payload and incident disposal remain unexecuted. |

Audit baseline: `b90076c1f21f3853ebfd90954b3babe26f122eca`.
The behavioral contract remains [the implementation handoff](bra-567-handoff.md).
The fixed regressions below were established before semantic repairs. The user has now authorized repairing the
violations and continuing through the blocked composition. Do not change regression expectations. Completion
requires the whole composition, including lowering and native execution. This report does not yet certify it.

## Current implementation and evidence

Worktree: `target/bra-567-audit`, directly on PR #710 branch `feature/bra-567-async-root-orchestration`.
The original develop checkout is untouched. Latest pushed head is **1ba7a3100**.
The user explicitly authorized continued implementation, checkpoint pushes, and an independent review on the draft PR
once implementation and verification finish. Continue through resolved review findings until ready for human Review.
No review agent has been started. No style or formatting pass has run during this audit.

### Checkpoints

- `235686a17`: compile repair, shared access roles and phases, exact retirement membership, data/loan joins,
  scoped capability and produced-reference corrections, initial regression fixtures.
- `777ffb930`: ordinary scalar operator semantics, trust-boundary output transfer, real async composition extension.
- `0e2e943d3`: custom index returned-borrow publication before reached copies and witness checks.
- `4b39bfe38`: retain projected loan descriptors until their actual consumer captures data or a new loan.
- `c73547c02`: match copied pointer descriptors in closed spatial proofs, with async invalid variants and boundary test.
- `da3e3c779`: share scalar computation reads across arithmetic, const calls and spatial extents, plus real buffer regression.
- `1af46e380`: normalize all three initialization predicates while preserving contents invalidation, and collect all real buffer controls.
- `1fdbc16e7`: observe copied predicate values without rebinding borrowed storage subjects.
- `f1b8bc032`: preserve copied descriptor origins while replacing reached contents.
- `02d82b5bc`: stabilize nullable component publication and unblock generator/default composition checks.
- `1ba7a3100`: distinguish carried loan requirements from physical storage anchors and accept actual Buffer views.

### Fixed comparisons and their limits

| Evidence | Code state and result | Meaning and exclusions |
| --- | --- | --- |
| `comparative-baseline-checker.log` | Original production with the same audit tests had seven failures | Controlled comparison, not a historical count from a different population. |
| `trust-result-checker.log` | 777ffb930, 440 passed | Checker properties, including exact revocation beyond proof budget, loan join stabilization and retired source ownership. Needs affected final rerun after later repairs. |
| `trust-result-trusted.log` | 777ffb930, 136 passed, 94 failed, 97 ignored | Frozen trusted-contract test identities. |
| `index-result-slot-repair.log` | 0e2e943d3, all 11 custom-index tests passed | Copy lowering, owned-witness rejection, borrowing conflicts, imported effects and const-contract control. |
| `index-result-trusted.log` | 0e2e943d3, 138 passed, same 94 failures, 97 ignored | Two added index tests passed. No changed failed identities. |
| `projected-loan-repair.log` | 4b39bfe38, all nine value-lowering tests passed | Includes the frozen returned field/index borrow regression, returned temporary and trust-boundary result. |
| `projected-loan-trusted.log` | 4b39bfe38, 139 passed, 93 failed, 97 ignored | Only transparent_predicate_getters_preserve_the_contained_owner changed from failure to pass. |
| `projected-loan-storage.log` | 41 completed passes, three failures, one uncompleted probe | Stopped only the task-owned operation_cleanup_native_fixture_lowers_its_checked_inputs process after about two minutes. It remains unverified. |
| `projected-loan-controlled-baseline.log` | Exact preceding production 0e2e943d3 with the same three failing tests | All three also fail before the projection repair. Sources and repaired executable restored in finally. |
| `projected-loan-adjacent-failures.log` | Repaired executable, same three failures | native callback report assertion, operator report borrowing, conversion reference-path panic. Pre-existing transition gaps still require classification where needed by the issue. |
| `projected-loan-native.log` | 4b39bfe38, standard-library native planning failed in 22.9 seconds | No moved-value diagnostics remain. 233 E7123, 123 E7119, 48 E7113, 24 E7087, 9 E7036, 4 E7125, 3 E7063, 2 E7127. These diagnostics are not source-body test counts. No native fixture ran. |
| `pointer-copy-boundary-repair.log` | Pointer repair, one checker boundary and eight source controls passed | Both formerly failing address cases, async positive and negative variants, witness movement/copy rejection and scalar-copy authority. |
| `pointer-copy-checker.log` | Pointer repair, 441 passed | All current checker properties pass. |
| `pointer-copy-trusted.log` | Pointer repair, 159 passed, 74 failed, 97 ignored | Exactly 19 old failed identities removed, no added failures. Includes address expiry, range bounds and invalidation controls. |
| `pointer-copy-native.log` | c73547c02, native standard-library planning still fails | 197 E7123, down from 233. Other diagnostic category totals unchanged. No fixture executed. First leaf remains byte_buffer_fill in std.memory.fill_spare_bytes. |

Earlier `live-join-trusted.log` had 123 passes and 103 failures. The nine removed failure identities at 777ffb930
are recorded by the comparison logs. Do not equate these test populations with runtime Source-body populations.
The older `live-join-import-lowering.log` completed 332 passes, 104 failures and two ignored before a generator analysis
was stopped after over four CPU minutes. `import-lowering-failed.txt` freezes the 104 completed failed identities.
Later checks blocked by that probe were unverified. Avoid an unchanged broad rerun.

### Current gate and owned process

All temporary production traces are removed. The loan-anchor repair is pushed as 1ba7a3100. The current build adds
only temporary test tracing for checked dependency contracts, storage plans, exported result summaries and physical
completion facts. Originals are retained as view-contracts-host-tests-before-trace.rs and
view-contracts-control-flow-before-trace.rs. The host original already includes the new formatted-bytes regression,
which must remain when restoring. Active build: view-contracts-trace-build.log, session 41049.

`returned-view-boundary-baseline.log` freezes actual std.bytes.Buffer.as_slice/as_slice_mut rejection, shared generator
reference rejection and the borrowed-heap positive failure. Local generator references are correctly rejected.
The actual Buffer views are checked through production source bodies, with the selected owning type asserted.
The trace identifies an escaping InvocationInput slot, rather than the external owner reached by its stored loan.

`callee_result_template` retains ValueDependencies and BorrowCapabilityActive for trusted borrowed views. Instantiation
correctly names the staged loan value. Liveness expands the loans carried by that value. The storage-input collector
incorrectly also classified BorrowCapabilityActive on a staged slot as a physical storage anchor. The repair excludes
that value-level requirement from collect_retained_storage, just as ValueDependencies is excluded. StorageAlive,
StorageInitialized and exclusive-storage dependencies still retain physical storage.

`loan-anchor-checker.log` passes 446. `loan-anchor-boundary-repair.log` passes both actual Buffer methods, local generator
rejection, async composition and operation cleanup. The borrowed-heap positive now reaches a trusted-obligation failure
instead of the old escaping-slot error. Its later owned-Owner negative remains blocked by that positive failure.
The shared generator test still reports five escaping dependencies. These remain completion work.
`loan-anchor-trusted.log` has 189 passed, 48 failed, 97 ignored. nested_buffer_views_preserve_checked_storage_effects is
the only removed old failed identity, and none were added. The added production Buffer-view regression passes.


`loan-anchor-native.log` at 1ba7a3100 reports 29 E7113, 28 E7119, 27 E7123, 9 E7036, 3 E7063 and 2 E7127.
The preceding native run had 48 E7113 and 37 E7119. The first leaf remains std.format.bytes at SourceId(94),
range 2560..2593. The actual Buffer.as_slice body now passes, so the next gate traces its caller and exported
result contract. production_formatted_bytes_retain_the_buffer_owner freezes that exact Source caller before any
further semantic change. The existing first versus first_cursor generator bodies are paired trace controls.
No native fixture has executed.

The smaller frozen generator_iteration_lowers_selected_cursor_operations test repeated the same blocks beyond
14,000 updates. Its task-owned process was stopped after sufficient repeated-update evidence. The verbose flow trace
costs enough output time that its wall time is not a compiler performance measurement.

A bounded exact-state capture at block 281, after 1,000 updates, takes 0.18 seconds and terminates deliberately.
`generator-loop-state-snapshot.log` contains the before/after states. Every root field except execution.current agrees.
One nullable payload alternates between Joined(..., 281, payload place) and a Projection(NullableValue, Constructed(...))
view containing that same Joined payload. `with_storage_component` unwraps the old payload view but not the newly
published view, so reading and republishing a joined component adds another parent snapshot. The new owning-boundary
law freezes this asymmetry. Keep nullable extraction owner identity and witness transport checks intact.

`nullable-publication-baseline.log` fails only the new republishing assertion. Extraction keeps the payload owner and
rewrapping keeps the nullable snapshot. The one-line repair unwraps the incoming payload using the same helper
already applied to the previous payload. `nullable-publication-checker.log` passes all 446 checker tests.
`nullable-publication-composition.log` passes both the previously stalled generator lowering test and the full
operation-cleanup/default fixture in 3.83 seconds. No probe remains running, and no temporary trace remains in source.

`nullable-publication-trusted.log` retains exactly 187 passed, 49 failed and 97 ignored, with no changed failed identity.
`nullable-publication-adjacent.log` completes 51 passes and four failures across storage-flow and lowering/control-flow.
The three storage-flow failures match the earlier controlled baseline. The fourth,
shared_generator_values_keep_their_original_reference_dependency, is already in import-lowering-failed.txt and failed
in live-join-import-lowering.log. It reports an escaping dependency and belongs to the next returned-reference gate.
The one-line publication repair and its fixed regression are pushed as 02d82b5bc. Native planning is still unverified
after this repair and remains blocked by the returned-reference class in the preceding native run.

Earlier repairs and retained evidence:

- Scalar computation reads now share computed_value for arithmetic, const calls and extents. The actual three buffer
  bodies pass. `range-staging-boundary-baseline.log` records the original arithmetic/view mismatch.
- Nine closed pointer predicates share descriptor normalization. The three initialization predicates remain invalidated
  by writes. `initialization-predicates-trusted.log` has 171 passes, 63 failures, 97 ignored and no old regression.
- The scoped positive trace preserves the surviving resource fact at retirement. Its missing consumer was a by-value
  predicate observing private Scalar context. `copied-value-predicate-trusted.log` has 185 passes, 50 failures, 97 ignored.
  All 444 checker tests pass there. Direct copied-bool borrowed-authority rejection remains fixed.
- `copied-descriptor-trusted.log` has 187 passes, 49 failures, 97 ignored. Only the old failure
  opaque_effects_preserve_unexposed_pointer_identity_without_preserving_authority was removed. The new dynamic-wrapper
  regression passes. `copied-descriptor-final-checker.log` passes all 445 checker tests.
- `copied-descriptor-boundary-repair.log` passes seven Source tests, including all three real dynamic output wrappers,
  both async composition tests and copied-authority negatives. It does not prove native composition.
- The descriptor unit's initial final assertion mistakenly required an empty owned_values traversal. That API visits
  a scalar root without owning it. The corrected assertion requires every visited value to lack owning identity.
  Origin preservation and replacement of reached contents are unchanged requirements.
- The pointer unit introduced at c73547c02 originally asserted inequality of raw by-value predicate representations.
  It now asserts distinct borrowed storage identities, while retaining spatial matching and no-copy-ownership checks.
  Source negatives were not changed.

### Remaining completion work

`receiver-entry-native.log` contains 27 E7123, 27 E7119, 2 E7113, 9 E7036, 3 E7063 and 2 E7127.
The previous native checkpoint had 29 E7113 and 28 E7119. The real std.format.bytes return is repaired.
The first remaining leaf is a trusted precondition on prepare_native_path at SourceId(102), bytes 11359..11369.
The product string-sorted source order maps that ID to std/fs/windows.bray. Remaining escaping references occur
at writer.bray's awaited flush_buffered_async and condition/variable.bray's returned mutex.lock.
The shared-generator copied element also retains local cursor loans in liveness despite its portable projection
now resolving to the original parameter. These are the next carried-value versus container-loan consumers to trace.
No native fixture executed. The native attempt failed in standard-library planning after 22.6 seconds.

The receiver/projection batch passes all 446 checker tests. The fixed trusted population is 191 passed,
48 failed and 97 ignored with no old failure added or removed. Both new Source tests pass. Nine of ten focused
boundary tests pass, including the local-owner and generator escape negatives, consuming stored-reference control,
both async composition checks and operation cleanup lowering. The existing shared-generator positive still fails.
Its newly added exact portable-contract assertion awaits the next test build. The diagnostic-only projection probe
resolved the copied element to the original parameter before that physical failure. No trait-view transparency change
was made. The new local-owner fixture originally used reserved `view` as a method name. Renaming it preserved its
escape-rejection expectation. Style and formatting remain deferred.

- Finish the executable ledger across remaining operation/failure transitions, including constructors/defaults,
  branches/loops, suspension and task outcomes, scoped and root cleanup, retained incidents, shutdown and imported facts.
- Preserve the fixed comparisons while resolving the next actual transition that blocks native planning.
- Execute async-ownership-composition.bray. Its assertions cover both transport return branches, loop suspension,
  genuine root Main authority, returned inactive Future disposal without executing its body, scoped child cancellation,
  exactly-once finalization/destruction, and lexical cleanup before static cleanup.
- Execute adjacent worker continuation, retained-error, panic/cancellation, partial startup, shutdown/disposal and
  imported/native acceptance paths. Synchronous ownership-transitions.bray is a focused control, not async completion.
- Complete the agreed runtime ownership protocols from bra-567-handoff.md, rather than assuming current runtime code
  satisfies them. Required Source-body/certificate runs must converge to the supported expected outcomes.
- Perform mechanical maintainability/DRY and AGENTS compliance review across the full active diff. Keep owned data,
  copied data, loan descriptors, physical references and lifetime dependencies distinct. Remove superseded mechanisms.
- Only at finalization run mandatory cargo xtask style and cargo xtask format, inspect their edits, and run required
  affected checks. Then push, dispatch the independent draft-PR review, resolve findings and move to human Review.

### Build and retained product rules

Use shared Cargo storage and LLVM. CARGO_TARGET_DIR points at the original repository target directory.
CARGO_BUILD_BUILD_DIR points at target/cargo. BRAY_LLVM_PREFIX and LLVM_SYS_221_PREFIX point at
`target/toolchains/llvm/active`. Profile tooling uses optimized incremental code generation without a Cargo job cap.
The machine has 24 logical processors. Compiler frontend phases can still serialize.

Default Rust 1.99 produces LLVM 23 archives, incompatible with the Bray LLVM 22 toolchain. Build xtask with default
stable, then run its native command with RUSTUP_TOOLCHAIN=1.98.1 and LLVM bin on PATH. This compatibility selection
is for the native archive producer. Native LLVM, archiver, linker and produced processes run outside the sandbox.
Use ignored task-owned target/audit for retained logs and temporary files. Do not clean shared storage.

The compilation test executable is under shared target/cargo/tooling/deps. Preserved repaired and index baseline
copies are in target/audit. They need the original dependency directory on PATH to load their neighboring DLL.
All task-owned test/build processes before the current pointer-boundary run have ended.
