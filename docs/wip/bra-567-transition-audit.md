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

## Current checkpoint and next decision

The latest pushed source is 3b9d8906d on PR #710's branch. The original audit baseline is
b90076c1f21f3853ebfd90954b3babe26f122eca. The agreed goal remains in bra-567-handoff.md.
No native composition has executed and the issue is not complete.

The pushed batch initializes the postcondition Result formal at clause entry, separates successful await completion
from later source continuation, and preserves each actual descriptor's role when its contents change. It removes the
type-based current_observation helper and ValuePreservation callback. Its fixed comparison has 449 checker passes,
38 lowering passes, 204 trusted passes with 41 failures and 97 ignored, and 383 broader export/lowering passes with
57 failures and two ignored. No unchanged failure identity was added. The original await result cleanup test passes
without changing its assertions. The extended composition has a mutable async receiver and three intended negatives.
advance and transport pass, but cancellation after the awaited transport result originally failed in produce.

completed-owner-trace.log proves storage flow initialized the destination local at initializer completion, before
ownership analysis or lowering published it at the pattern binding. Cancellation tried cleanup both on the actual
await result and the unknown local. The current uncommitted batch removes that early-local shortcut and its helper.
produce now passes. The fixed suite exposed three old regressions in borrowed locals, so expansion is stopped:
owned_postconditions_follow_a_scoped_mutable_alias, shared_component_borrow_preserves_only_its_marker_observations,
and nullable_input_adaptation_preserves_physical_owners_and_rejected_values. Do not weaken their expectations.

The repaired binding consumes a formed borrow descriptor instead of copying the reached object. binding-reference-trace.log
shows that the earlier transfer selected the referent while memory.values was empty and the actual descriptor remained
in memory.borrows. Ownership and physical transfer now share expression-relative projection selection. Nullable wrapping
and payload-absence observation occur at binding completion. The early-local helper and map are removed entirely.

binding-descriptor-boundaries.log passes the real async composition, all three intended negatives, the three borrowed
controls and the new publication law. The law requires possible result liveness before cancellation, no local before
binding, and an initialized local plus moved result at a subsequent suspension. Exit records join cancellation before
and after production, so definite result initialization is deliberately not asserted across that join.

The composition main now explicitly requires main_thread_execution. Its Source harness loads it as a library callable,
not an executable root. binding-source-boundaries.log identified RootCleanup's missing ordinary Main premise. Native
acceptance must still prove the actual root supplies that authority. No production authority rule was weakened.

binding-descriptor-build.log completes in 2m30s. The current source contains no temporary diagnostic hooks.
binding-descriptor-checker.log has 449 passes and binding-descriptor-lowering.log has 38 passes.
binding-descriptor-trusted.log has 207 passes, 40 failures and 97 ignored. It includes two extra passing adjacent
controls compared with descriptor-role-trusted.log. Across unchanged identities, only the composition failure disappears.
binding-descriptor-import-lowering.log has 383 passes, the same 57 failures and two ignored in 112.53 seconds while
xtask builds concurrently. No unexplained regression remains in either fixed comparison.

Push this coherent checkpoint, finish xtask build session 78595 and run native acceptance. The last native at 7aa514618 has 25 E7123,
27 E7119, three E7063 and two E7127. It contains none of 3b9d8906d's Result-entry and descriptor-role repairs.

Runtime protocol inspection confirms reserve_start stores registration before demand_lane and rollback withdraws it
before close_owner/release_pin. commit_start calls activation.begin before locking/publishing task control. Still prove
actual abnormal ownership and lock ordering through native frame ownership and activation cleanup before changing it.
EntryRestoration finishes its EntryScope before writing completion. RetiredCell disposes control before pin and arena.
These observations await Source and native acceptance.

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
| Conditional payload cleanup | `Plan::prepare` derives typed payload paths and presence guards from checked cleanup parts. The parent value exists before cleanup. | A proved-absent payload produces no lifecycle call, effect or retirement. An unknown payload retains its possible cleanup. Proved presence can make lifecycle completion definite, subject to initialization and moves. | `dispose` previously applied opaque cleanup without those guards. The current batch shares guard selection across both step kinds and removes the redundant conditional flag. Actual native-path and absent/possible-buffer probes are pending validation. |
| Reference-free memory range write | The checked Copy operation identifies source, destination and pointee type. The shared footprint proves whether values of that type contain references. | The operation mutates destination contents. Copying reference-free data or filling bytes does not publish either input pointer. Unknown or reference-containing copies retain their conservative exposure. | `address_escapes` previously treated all memory operations as stores of their input addresses. The current batch uses the existing checked type-shape result. Production byte-copy portable effects and actual two-path preparation protect the boundary. Native evidence remains outstanding. |
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
The original develop checkout is untouched. Latest pushed head is **cd9520c26**.
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

### Remaining completion work

- Finish local binding publication and restore the three borrowed-local controls before expanding migration. Keep the restored slice negative.
- Finish the report-formation repair and trace the writer's actual loan through await and report disposal. Preserve open
  generic report equations, distinct normal/abnormal outputs, and conservative opaque-frame dependencies.
- Keep the trusted comparison identities fixed. Its last pushed 41 failures include the blocked composition. Recompare exact identities after repair. Preserve and explain the
  broader export/lowering comparison, including the conversion reference-path panic and callback/operator report gaps.
- Finish the executable ledger across constructors/defaults, branches/loops, suspension and task outcomes, scoped/root
  cleanup, retained incidents, shutdown and imported facts. Source successes do not establish native behavior.
- Verify the extended real async composition with mutable async receiver use. Its existing assertions
  cover transport return branches, loop suspension, genuine Main authority, inactive Future disposal without executing
  its body, scoped child cancellation, exactly-once finalization/destruction and lexical cleanup before static cleanup.
  Preserve invalid fabricated/moved-owner variants and their exact intended diagnostics. Execute the program natively.
- Audit and complete the agreed runtime protocols from bra-567-handoff.md. PreparedStart owns admission and registration
  until publication. EntryRestoration owns displaced context through abnormal exits. RetiredCell physically owns control,
  pin and arena before vacancy. WaitResources stays stable while linked access is acquired before mutation. Trace actual
  producers and consumers through native acceptance rather than treating these structures as proof.
- Run all required runtime Source bodies/certificates and Source/imported/native acceptance, including worker continuation,
  retained errors, root panic/cancellation, partial startup rollback, blocked shutdown, result/static disposal and nested
  foreign entry. Verify zero Rust implementation for migrated consumers, synchronous no-async demand and comparative costs.
- Reread AGENTS and perform the mechanical maintainability/DRY audit over the full active diff. Remove superseded machinery.
  Only at finalization run mandatory cargo xtask style and cargo xtask format, inspect edits and run affected checks.
- Push checkpoints. After implementation and verification are complete, dispatch the independent draft-PR reviewer.
  The reviewer must publish its review and actionable findings directly on PR #710 and return artifact IDs. Verify them,
  resolve findings, reuse the reviewer for material fixes, then make the PR ready and move BRA-567 to In Review.

Two older negatives were identified as unsupported by the language contract: forwarding an unchanged whole witness does
not create undeclared authority, and a callback taking a mutable reference without changing or exposing its owner need
not revoke that owner. Correct those fixtures with explicit positive forwarding and real fabrication/opaque mutation
controls when reached. Do not change production semantics to satisfy them or treat the other failures as fixture errors.

No independent review agent has been started. The user authorizes unattended work and pushes to the PR branch.
Automated style and formatting have not run during this audit, as requested.

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
