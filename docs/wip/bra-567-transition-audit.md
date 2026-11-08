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

The latest pushed source is 7aa514618. The previous fc4b5160b checkpoint fixes proved-absent conditional cleanup and reference-free byte-copy
address exposure. Both actual rename paths pass Source and lowering. conditional-cleanup-checker.log has 447 passes.
conditional-cleanup-native.log still has 25 E7123, 27 E7119, 1 E7113, 8 E7036, 3 E7063 and 2 E7127.
No native fixture has executed. The compiler repairs below are intermediate steps toward that acceptance gate.

The pushed 7aa514618 batch preserves normal and abnormal report formation independently, including symbolic generic
requirements. Opaque frames retain their captured dependencies. A boundary law checks channel separation, predecessor
immutability and repeated-join stabilization. Await admission now consumes its Future operand at Suspension(Await),
matching storage planning. The earlier selector only consumed at CallAttempt, which await never emits. The stale Future
handle had produced phantom cleanup reports after completion. The actual awaited writer now passes its Source and
portable-report checks. Real local escapes and fabricated/moved-owner negatives retain their expected rejection.

Slice lifetime retention previously dropped an already-recorded local array SliceRange because it cannot name scalar
contents. The existing storage_owner helper now retains the projectable containing owner for lifetime bounds. Exact
value selection stays strict. The frozen expired-storage negative and its new active-storage positive both pass.
await-admission-checker.log has all 448 checker passes. await-admission-trusted.log has 198 passes, 47 failures and
97 ignored. It newly exposes the composition lowering panic, which blocks expansion until repaired.

await-ownership-trace.log proves the panic occurs after the child completes: lower_await materializes the returned
ChildCleanup for a new cancellation request, but the CFG only models invocation failure before production. The
speculative operand-temporary truncation was ineffective and is removed. The current coherent repair adds the
post-completion cancellation edge without InvocationFailure and publishes the completed value in its checked output
slot before that check. Existing guards distinguish cancellation before and after production. The CFG regression now
requires both the admission transfer and a cancellation edge from successful completion. Two manual lowering fixtures
also planned private accesses without their checked decisions. Their setup is corrected without weakening assertions.
await-completed-cancellation-checker.log has 448 passes and await-completed-cancellation-lowering.log has 38 passes.
await-completed-cancellation-build-fixed.log completes. The Source composition, both invalid variants, awaited writer
and slice lifetime controls pass. The trusted comparison has 198 passes, 46 failures and 97 ignored, with only the
composition failure removed from the preceding identity set. No temporary instrumentation remains.

The current broader comparison await-completed-cancellation-import-lowering.log has 380 passes, 60 failures and
two ignored in 82.54 seconds, with exactly six old failures removed and none added. These include the restored slice
negative, completed composite reports, independent pointer origins, protected-frame lowering, partial-array retention
and implicit panic-report cleanup. The existing await outcome test still fails its immediate-block Store assumption,
also present before the batch. Inspect the intervening report-retention blocks before correcting that assertion.
The broader fixed export/lowering run before these await/lifetime changes is deferred-report-import-lowering.log:
374 passed, 66 failed, two ignored in 81.44 seconds. It introduced one failure identity against the old comparison:
checked_slice_prefix_authority_expires_with_its_storage. The exact preceding fc4 baseline also fails it, while the
retained projected-loan executable passes. The current lifetime repair restores the narrow positive/negative check.
Retained report, origin and completed-await traces establish the actual boundary defects. Comparison identities and
executables remain in target/audit. Do not equate these Source test populations with native diagnostics or execution.

The current fixture adds ChildCleanup.advance as a mutable async receiver, updates visits across suspension and loop
iterations, and adds a pending-mutable-future conflict negative without changing frozen test identities. All three
intended negatives pass, but mutable-async-composition.log records the positive's marker-preservation failure.
The existing await outcome regression still fails its result-store assertion. Its expectations are restored unchanged.
Expansion is stopped until both transitions and the fixed comparison pass.

mutable-composition-trace.log proves three exact defects. Contract clauses stage &result with unknown contents despite
having a known successful-variant guard. The actual PostconditionResult formal was never initialized in current storage.
The current entry repair installs ExecutionCondition::Result only at clause entry, without creating an owner. Its
unchanged fallible-pair positive/fabrication-negative test passes in contract-entry-trace.log.
That trace also shows Await's cancellation plan classifies the newly produced Guard as Moved. The new cancellation edge
was emitted from resume, but build_await returned that same block, so later source Return consumed the result before
any outgoing edge. The current repair gives the successful continuation a separate block. The CFG law now requires
completion to be the last operation before cancellation. No result-store assertion was weakened.
Finally, the receiver's entry witness is keyed by an owning Input(self). On await, storage flow proves its Marker field
stable, but current_observation changes the formal to Reference, so same-role carrier retention drops the witness.
Both direct() and the controlled bare yield_now() variant show this. ADVANCE-EFFECTS records Ambient with the Marker
in stable_values. The first attempted correction changed borrowed-formal entry representation. The frozen comparison exposed six new
old-test failures, and the receiver still failed, so that migration is reverted. Entry representation stays fixed.

All temporary traces and variant hooks are restored. Backups mutable-core-before-trace.rs, mutable-proof-before-trace.rs,
mutable-host-before-trace.rs preserve the prior semantic sources. The MIR test is restored from 7aa514618, keeping its
original owner-store and cleanup assertions. formal-loan-completion-boundaries-build.log completes. Its checker and lowering runs have 448 and 38 passes. The
unchanged await result-store/cleanup regression passes. The trusted comparison has 198 passes, 47 failures and 97
ignored, with six old failures removed, six old failures introduced and the extended composition failing. No expansion.
The current repair instead removes current_observation's type-based descriptor reclassification. The existing actual
source identity and ownership role now determine contents replacement in ExecutionCondition::with_current_contents.
ValuePreservation uses that canonical operation directly. Its caller-specific callback and the old helper are removed.
A new boundary law covers owning values, reference views and scalar copies, requiring identical role/identity and new
contents. The postcondition Result entry and split post-await continuation repairs remain. Build session 28036 writes
descriptor-role-completion-build.log for checker, lowering and compilation. Current sources contain no diagnostic hooks.
descriptor-role-checker.log has all 449 passes and descriptor-role-lowering.log has all 38 passes. The fixed trusted
population has 204 passes, 41 failures and 97 ignored. Exactly six old failures disappear and none is added in unchanged
cases. The extended async receiver now passes, including its ensures. The real composition reaches produce and fails
its cleanup marker requirement after awaiting transport. Both invalid-owner cases and the new pending-borrow negative
still reject as intended. The original await result cleanup check passes unchanged. This is an intermediate checkpoint,
not completion. Trace produced owner evidence through the post-completion cancellation edge before further migration.
After completion rerun libraries, fixed trusted population, original await check, real async composition and negatives.
If no unexplained old regression remains, broaden export/lowering, checkpoint/push, then rebuild xtask for native.
await-completed-cancellation-native.log at 7aa514618 has 25 E7123, 27 E7119, three E7063 and two E7127. The previous
E7113 and eight E7036 are gone. No native fixture executes. The retained xtask matches 7aa514618 plus the fixture edits.
The latest compilation executable contains the diagnostic contract-entry build, not the current borrowed-formal/split
continuation repair until session 5858 completes. Rebuild xtask only after the coherent current comparison is valid.

The runtime protocol inspection confirms reserve_start stores the registration before demand_lane and graceful exit
withdraws it before close_owner/release_pin. commit_start calls activation.begin before locking and publishing the task
control, then clears registration and wakes. Still trace actual abnormal ownership between begin and publication through
the native frame owner and activation cleanup. Its shape alone is not proof of rollback. EntryRestoration consumes its
EntryScope and writes completion after finish_entry_scope. RetiredCell disposes control before the execution pin and arena.
These remain read-only correspondence observations awaiting Source and native acceptance.

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

- Finish the post-completion cancellation repair and rerun the fixed comparison before expanding migration. Keep the restored slice negative.
- Finish the report-formation repair and trace the writer's actual loan through await and report disposal. Preserve open
  generic report equations, distinct normal/abnormal outputs, and conservative opaque-frame dependencies.
- Keep the trusted comparison identities fixed. Its current 47 failures include the blocked composition. Recompare exact identities after repair. Preserve and explain the
  broader export/lowering comparison, including the conversion reference-path panic and callback/operator report gaps.
- Finish the executable ledger across constructors/defaults, branches/loops, suspension and task outcomes, scoped/root
  cleanup, retained incidents, shutdown and imported facts. Source successes do not establish native behavior.
- Extend the real async composition with mutable async receiver use after that boundary works. Its existing assertions
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
