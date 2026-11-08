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

The custom-index publication gate passes. The trace proved that direct field copying found and rejected the live
witness, while indexed copying resolved no physical source path and therefore checked Unknown. The temporary trace
has been removed. The repair registers the indexer's returned borrow separately from reached data in the existing
invocation-result binding map. `index-result-slot-repair.log` records 11 passes, including owned-witness rejection,
lowering, lifetime conflicts, imported effects, and the const-contract control. `index-result-trusted.log` records
138 passes, 94 failures, and 97 ignored. Its failed identities exactly match `trust-result-trusted.log` at 777ffb930.
The projected-loan repair passes all nine value-lowering tests. The fixed trusted population is now 139 passed,
93 failed, 97 ignored. Only `transparent_predicate_getters_preserve_the_contained_owner` changed from failure to pass.
The adjacent storage/index population completed 41 passes and three failures before its expensive
`operation_cleanup_native_fixture_lowers_its_checked_inputs` probe was stopped after about two minutes. That probe
is unverified. The three failures are callback report outputs, operator report borrowing, and conversion report
borrowing. A controlled build of exactly those three at production checkpoint 0e2e943d3 fails all three in
`projected-loan-controlled-baseline.log`. The baseline files and executable have been restored to the repair.
The preserved repaired executable is `target/audit/projected-loan-compilation.exe`, and the baseline executable is
`index-slot-baseline-compilation.exe`. Run preserved executables with the shared dependency directory on PATH.
The fixed comparison gate passes. Next rebuild xtask and retry native composition. Source acceptance still does not
certify runtime execution. The existing coarse
transition table is an entry-point inventory, not the complete executable transition ledger required above.

### Executable ledger for the current blocking boundaries

These rows trace code paths, not completed native coverage. The failure edges are part of each transition.

| Operation and producer | Before and production | Consumption and failure | Actual consumers and evidence |
| --- | --- | --- | --- |
| Invocation Input, `execution.rs::begin_invocation` | Actual receiver and argument values exist. Borrow inputs are prepared. No result exists. | A failed precondition bypasses Attempt and Completion. Existing input owners remain cleanup obligations. | `AnalysisOperationKind::storage_plans` excludes self-produced accesses. Liveness `collect_edge_lifetime_ends` includes failed-invocation subjects on the bypass edge. Existing admission and cleanup controls still require final native evidence. |
| Invocation Attempt, `execution.rs::push_invocation` | Admitted inputs exist. Input transfers use Admission consumption. | Invocation failure cannot publish a result. Completion is only on the successful continuation edge. | Storage and trusted transfer checks use the shared phase selector. OutputDefinition plans never act as source reads. |
| Invocation Completion | Returned references are staged, then `memory.complete_transition` publishes them. | Result-reaching operations run after publication. Failure does not traverse this operation. | `apply_storage_operation` publishes before applying produced accesses. `TrustedChecks::after_operation` checks those transfers after semantic completion. |
| Custom index returned borrow, `expression/access.rs::custom_index_access` | Receiver and selector borrows are inputs. The indexer produces a borrow descriptor with its own stored borrow type. The reached item is a different value. | An ordinary item copy does not consume the reached owner. A live witness forbids that copy. | The existing InvocationResult binding identifies the descriptor. `invocation_result` stages its reference contract. `reference_target` resolves reached storage for the later copy. All 11 custom-index controls pass at 0e2e943d3. |
| Built-in field or index projection, `project_access` | A produced loan descriptor and its reached storage already exist. Projection extends the same source path. It produces no independent owner. | Retiring the descriptor at an intermediate projection destroys the source of subsequent projections. The repair defers retirement to the actual consumer. | Frozen `returned_borrow_survives_field_and_index_projection` had two baseline moved-value errors. It and all eight adjacent value-lowering tests now pass. |
| Actual consumer of a projected loan | The source path still identifies the producer's materialized borrow slot. The consumer first captures reached data or forms a new borrow. | Completion consumption retires the temporary descriptor, independently of the original referent owner. User parameter and binding storage are excluded by the producer/value-slot identity check. | `consume_expression_value` records the same consumption metadata read by storage flow and liveness `OperationEffects::prepare`. Lowering must still pass the frozen regression. |

Audit baseline: `b90076c1f21f3853ebfd90954b3babe26f122eca`.
The behavioral contract remains [the implementation handoff](bra-567-handoff.md).
The fixed regressions below were established before semantic repairs. The user has now authorized repairing the
violations and continuing through the blocked composition. Do not change regression expectations. Completion
requires the whole composition, including lowering and native execution. This report does not yet certify it.

## Active repair checkpoint

Continue directly on PR #710's branch, `feature/bra-567-async-root-orchestration`. Checkpoint `235686a17` is committed
and pushed there. It was a fast-forward from the audit baseline. Preserve and push further coherent checkpoints.
The worktree remains isolated from develop. Style and formatting are deferred until the end of the audit.

The scalar-value repair passed five focused regressions. Both scoped-exit controls still fail at continuation
authority, but ordinary entry postconditions and imported provider export now pass. Native execution remains
unverified. The earlier 440-pass checker result predates the latest producer and scalar repairs.

The native fixture is now `async-ownership-composition.bray`. Its extension retains the previous root cleanup
assertions and adds witnessed ownership across direct suspension, loop back-edges, both return branches, future
capture, task-result transport, and disposal of an inactive future without running its body. It checks Main affinity,
exactly-once finalization/destruction, child cancellation, and lexical-before-static cleanup. These assertions are
not evidence of success until the fixture actually executes. Retained-error and worker-lane fixtures remain required
adjacent coverage. The synchronous ownership program is a focused regression, not the async completion criterion.

Reuse the shared Cargo target and LLVM installation. Logs are under the worktree's ignored `target/audit` directory.
The current native attempt rebuilds xtask in `async-composition-build.log` and runs in `async-composition-native.log`.
It uses the installed LLVM-compatible archive producer. Do not launch duplicate builds.

The first repair passed 436/439 checker tests. A controlled baseline comparison with identical tests and original
production code failed seven tests, including all three remaining failures. The comparison script backed up and
restored repair sources. The first repair composition no longer panicked: fabricated authority was rejected, while
the valid fixture reported four trusted-obligation failures at union cleanup/observation and scoped observation.
The fixed existing controls then passed movement, copy rejection, scoped capability, and failing-exit rejection.
The loop control still failed. These results are in `repair-*.log` and `comparative-baseline-checker.log`.

The next coherent correction preserves an explicit Reference role even when its target identity is unknown, and
normalizes predicate references to the actual reached owner rather than the access handle. Scalar reference
subjects remain tied to their own storage. Retirement membership is indexed by ownership role and identity,
independent of changing contents. The unchanged full checker suite now passes all 439 tests. Its composition
finished. The negative composition check passes. The positive check now reports three trusted-obligation failures,
all in conditional union transport and consuming-match cleanup. The scoped and loop composition has no reported
obligation failures. The fixed standalone loop control passes. No native acceptance has run. Do not weaken any
regression to obtain a pass.

A broader run of the existing trusted-contract compilation tests reports 122 passed, 104 failed and 97 ignored.
This is current-tree evidence, not a baseline comparison. Do not classify these failures as regressions or fixture
defects without comparison. Its log is `context-trusted.log`. The checker unit suite alone does not certify Source
composition. Further repair builds use the existing incremental `tooling` profile. Temporary trace instrumentation
is backed up in `trace-backup.json` and must be removed before final validation.

## Findings established by code inspection

### A call reads its own output before successful production

After the build-only reexport fix, both composition tests panic at the first `callback()` in `exercise`.
`apply_plan` reports Read, Uninitialized, Temporary, with the temporary defined by the same call expression.
`StoragePlan::access_plan_role` recognizes OutputDefinition only inside its Copy/Move/ValueTransfer branch.
A Read of that same output falls through to Observation. `AnalysisOperationKind::storage_plans` permits it at
Attempt, and `apply_operation` applies the plan before completing the result. The violated contract is publication
before observation of a self-produced output. This happens before the extended scoped composition is reached.
Neither test result can establish correctness of later transitions or the fabricated-owner rejection.

### Transfer checking bypasses the checked operation phase

`TrustedChecks::before_operation` in `analysis/guarantee/trusted/check.rs` invokes
`check_witness_transfers` for the current operation. In `trusted/transfers.rs`, that function enumerates
`storage.occurrence_plans(occurrence)`, selecting Copy, Move and ValueTransfer by purpose. It then independently
guesses whether the operation defines a value from expression equality and Bound/Attempt variants.

By contrast, `AnalysisOperationKind::storage_plans` in `analysis/model.rs` excludes OutputDefinition and selects
private-input initialization, admission and observation by operation phase. Physical storage checking uses that
selector in `analysis/storage_flow/check/analysis/core.rs`. These consumers therefore disagree on which transfers
exist at a given program point. A publication can be inspected as a source transfer before the result exists.
This violates the agreed requirement that semantic and physical consumers share the checked transport phases.
The disagreement is statically established. Which Source diagnostics it causes remains a reproduction question.

### Retirement uses a bounded positive proof as a revocation decision

`ExecutionState::retired_owners` in `analysis/guarantee/retirement.rs` computes a set of invalidated owners.
`retire_values` then defines `retired_owner` using `ObservationSet::retains_owner(&invalidated, value)` when
retained snapshots are present. That query searches snapshots with `ExecutionCondition::WORK_LIMIT` and returns
false on exhaustion in `execution_guarantees/condition/observations.rs::retained_snapshot`.

A false result therefore means either absent or not reached within the budget. Treating both as not retired leaves
a witness available. The new regression passes its one-owner control, then fails with 129 invalidated owners:
`retirement retained authority with 129 invalidated owners`. This directly demonstrates a violation of the
requirement that proof exhaustion must never preserve revoked authority. Do not increase the budget as a repair.

### Rejoining borrowed contents promotes a data summary into ownership

`repeated_loan_join_stabilizes_without_acquiring_ownership` joins two views of the same loan with different Boolean
field contents, then joins the summary with the same incoming view again. The first summary contains an Input
term at the field. The second contains an Owned observation around that field identity with Unknown contents.
The first equality assertion fails, before the reversed-order and ownership-count assertions execute.

In `execution_guarantees/snapshot.rs`, differing scalar values produce a result identity. On the repeated join,
the identity term and scalar have different ownership roles, selecting `mixed_role_components`. That function
unconditionally reanchors the common data with `owned_snapshot` and an Owned wrapper. This conflates a summary
of observed data with ownership of a selected object. The outer loan remains Reference, but its reached field's
representation changes ownership role. This violates the agreed separation of observed contents and authority.
The test demonstrates failure to stabilize after the first join. It does not prove unbounded growth, nor establish
the cause of the historical stack overflow or the separate Source loop failure.

### The saved role migration does not compile

`storage/plan/model.rs::access_plan_role` names `crate::StorageAccessPlanRole`, but `bray-bound-tree/src/lib.rs`
does not reexport the enum. The baseline focused build reports E0432 and E0425 at lines 383 and 382.
The audit adds the missing reexport. This changes API visibility plumbing, not ownership policy.

## Concrete composition and transition trace

The existing native fixture `xtask/fixtures/native-execution/ownership-transitions.bray` is extended rather than
duplicated. `exercise_scope` now carries the same owner through acquisition, construction, a consuming call with
two return branches, repeated sibling mutation and callback, scoped entry, and normal or early-return exit.
Its destructor requires the retained witness. `main` executes both branches. Other existing functions retain
conditional union transport and consuming-match coverage.

| Transition | Actual implementation path | Obligation and current audit status |
| --- | --- | --- |
| Acquire and produce result | guarantee `flow/operation.rs::complete_operation` and `complete_value` | Publish before consuming result inputs. Completion selector is present. |
| Materialize and retain operand | storage `expression/access.rs`, `expression/planning.rs`, `expression/consumption.rs` | Distinguish source access from independently retained evaluated slot. |
| Admit input | guarantee `flow/operation.rs`, `StorageValueConsumptionPhase::Admission` | Capture private input before vacating caller value. Witness checking bypasses phase selection, as above. |
| Complete call | guarantee `trusted/borrows.rs`, `flow/operation.rs` | Apply returned identity and contents contracts, then consume input slots. |
| Physically consume value | storage-flow `check/analysis/core.rs` | Uses `value_consumption_phase`, including completion of admitted call inputs. |
| Retain for last use | liveness `effects/core.rs` and `effects/retention.rs` | Consume the same planned value-consumption entries. Dynamic agreement remains unverified. |
| Join branch/loop state | guarantee `flow/joins.rs::merge_returned`, `flow/merge.rs`, `common_components` | Preserve role and stable identity independently of contents. Stabilization remains unverified. |
| Enter and exit scope | scoped invocation result and cleanup transition | Exit must receive the actual entered value on normal and early return. Fixture covers both. |
| Retire and destroy | guarantee `retirement.rs`, cleanup transitions | Revoke ended authority while retaining terminal data. Bounded revocation defect described above. |
| Lower selected input | lowering input storage and invocation consumers | Must initialize private storage before use. Dynamic agreement remains unverified. |

These are traced entry points, not a claim that every path has been verified. Remaining audit work includes an
executable revocation-budget regression, a phase-specific transfer regression, loop-state stabilization, failure
edges, imported contracts and native disposal counts. A successful source check alone cannot prove those properties.

## Fixed regression selection

Run the following selection unchanged after build-only repairs and after any later semantic repair:

`cargo test --release -p bray-compilation --lib ownership_composition --no-fail-fast -- --test-threads=1`

- `ownership_composition_accepts_complete_transport` checks the real native fixture through the compiler.
- `ownership_composition_rejects_fabricated_authority` replaces acquisition with ordinary construction and requires
  CheckingTrustedObligationNotProven. Rejection for a syntax or binding error is insufficient.

Both tests use the existing compilation helper. The fixture remains registered in the existing runtime-artifact
smoke workflow. Native execution is a separate check and must not be reported as passing from Source acceptance.
Record each test result separately. Do not accept a reduction in rejected bodies as evidence of soundness.

The same built compilation test executable also ran these existing controls individually, without rebuilding:

| Test name | Baseline result after build repair |
| --- | --- |
| `moving_a_witness_preserves_the_new_owner` | Panic on Read of uninitialized self-produced call temporary |
| `implicit_copy_cannot_duplicate_a_live_witness` | Pass |
| `borrowed_invariants_survive_loops_only_when_every_back_edge_preserves_them` | Fail, CheckingTrustedObligationNotProven after the trusted advance case |
| `scoped_guarantees_and_requirements_use_the_actual_capability` | Panic on Read of uninitialized self-produced call temporary |
| `failing_scoped_exit_does_not_publish_guarantees_to_a_catch` | Panic on Read of uninitialized self-produced call temporary |

Use each exact name as a Cargo test filter. Confirm one test runs for each filter. Preserve these controls alongside
the new composition checks. The loop failure has a distinct observed symptom and has not been attributed to the
output-phase defect. The passing copy negative must remain passing during any preservation repair.

## Repair order constrained by evidence

1. Keep the build-only reexport repair. Do not hide the storage invariant panic.
2. Repair classification of self-produced output reads at the checked planner/phase boundary, with the composition
   and existing Source controls frozen. Verify no real source read is suppressed.
3. Finish witness-transfer consumers' migration to that shared phase contract, with complete/partial/copy cases.
4. Resolve retirement using exact computed membership and retain the budget-boundary regression.
5. Investigate loop evidence using the fixed loop control and join regression, then repeat the whole fixed selection.

Only after these checks reach their intended outcomes can the composition reach later lifecycle transitions.
Imported and native acceptance must then check the same contracts. A clean initial prefix does not certify later
phases, and the report must continue to state those limits.

## Current evidence

The first focused build stopped in the baseline missing reexport before running tests. After the reexport repair
and correction of two borrowed diagnostic lifetimes in the new tests, the composition selection compiled and ran:
zero passed, two panicked, 2,354 filtered out. Both failures expose the self-output Read described above.
The negative test must eventually reach CheckingTrustedObligationNotProven, not merely panic.
Both new checker regressions ran individually and failed as described above. Each selected exactly one test.
The initial Cargo invocation used an unqualified filter with `--exact` and selected zero tests. That invocation
is not behavioral evidence. The matching filters were then run directly against its freshly built test executable.
Required `cargo xtask style` and `cargo xtask format` completed. Style inserted blank lines in six additional
existing source files. Those changes do not alter behavior. `git diff --check` passes.
This section records the baseline before the semantic repairs listed in the active checkpoint.

The audit establishes three executable failure mechanisms and a separate statically confirmed phase-consumer
disagreement. It does not establish that these account for every Source failure. The early call-output panic
prevents the complete composition from reaching native lowering and disposal. Those later outcomes remain
explicitly unverified until the authorized semantic repairs pass the fixed regressions and native verification.


## Retired snapshot and joined payload mismatch

A traced positive composition requires the retained predicate on the joined union payload. Available evidence
still names the original acquisition result. Both ordinary control-flow joins and returned-result joins collected
stable owners from all current storage snapshots, including retired slots. Terminal snapshots consequently pinned
old identities and prevented legal witness rebasing into the selected payload.

The additional fixed regression `retired_source_snapshot_does_not_pin_a_moved_payload_owner` fails before this
repair with no proof of the required predicate. Its baseline log is `retired-snapshot-baseline.log`. The repair
shares stable-observation collection between both join paths and selects independently live entries. Terminal
storage remains readable as historical data. Its presence alone no longer prevents a live owner from moving.
Validation is running in `live-join-composition.log`. Temporary trace instrumentation has been removed.


The live-owner repair passes all 440 checker tests, both composition tests and all five fixed Source controls.
The broader trusted-contract suite now reports 123 passed, 103 failed and 97 ignored. Relative to the preceding
repair run, the positive composition is the sole changed outcome and there are no new failing test identities.
The combined Cargo filter selected no checker tests. The full 440-test result comes from a separate run of that
fresh checker executable, recorded in `live-join-checker.log`. Imported/lowering checks and native smoke validation
are running. Source acceptance alone still does not certify completion.


## Pending scoped retirement repair

The unchanged `scoped_exit_must_explicitly_transfer_trusted_authority` control panics at
`StorageFlowDomain::retire_consumed_value_slots`: the origin lookup uses `definition_node`, which names a pattern
for `ScopedCapability`. The actual producer is already represented by `definition_occurrence` as `ScopedEnter`.
The consumption itself can also occur at a pattern and has no expression. Use the checked producer occurrence
rather than guessing an expression from declaration syntax. No patch has been applied yet, while the native
baseline build remains active. Preserve this test and its imported counterpart without changing expectations.

Active processes: native smoke in `native-smoke.log`, and imported/lowering test selection in
`live-join-import-lowering.log`. The latter has 332 completed passes, 104 completed failures and two ignored tests,
with `generator_iteration_lowers_selected_cursor_operations` still running. These are partial counts, not a suite
result. Do not launch duplicate work or change compiler sources during the native baseline run.


## Native and lowering boundary evidence

The imported/lowering selection was stopped after its generator case consumed more than four CPU-minutes without
completing. Its 104 completed failing tests were rerun alone with capture disabled to recover diagnostics in
`import-lowering-failures.log`. None passed on that rerun. The generator result remains unverified.

The first native smoke attempt stopped at incompatible platform bitcode: Rust 1.99 emits LLVM 23 while the selected
Bray LLVM tools are 22.1.8. Installed Rust 1.98.1 reports LLVM 22.1.8. Reusing the exact current xtask executable with
that native archive producer got past platform archive inspection. `native-smoke-compatible.log` then records an
exit-101 lowering failure in standard-library preparation: an executed borrow has checked status Unreachable.
The native ownership fixture was not reached. Native execution is still unverified.

Phase inspection finds that a borrow of newly produced call storage is intentionally excluded from Input, but is
also excluded by `storage_plans` at Completion. Liveness already classifies that same producer borrow as a result
definition. The new real-source regression `returned_temporary_is_borrowable_after_call_completion` uses
`make().read()` and demands valid MIR. Its baseline build is running in `returned-borrow-baseline.log` before any
repair of this transition. Other observed self-output panics involve nonreturning calls with temporary storage
but no materialized value slot. Custom indexing has a separate reached-source access through a produced borrow.
Do not classify the reached access itself as an output definition merely to suppress its checks.


## Validation order correction

Defer all style and formatting passes until the audit and semantic repairs are complete, as explicitly requested.
The attempted `repair-style.log` command was stopped while building xtask, before the style executable ran. Its
Cargo process tree was stopped without touching other work. Do not restart style or format during the audit.

Completion-boundary validation in `completion-boundaries.log` passed the returned-temporary borrowing regression,
nonreturning-array regression and both frozen composition tests. Both Source and imported scoped-transfer tests
now reach diagnostics rather than panicking. They expose premature consumption of the original scoped capability
when its data is copied into the pattern. A repair now keeps that lifecycle-owned slot until ScopedExit admission.
That repair and the scalar-join helper extraction have compiled as xtask dependencies but are not yet tested.

## Current checkpoint

The audit is incomplete. Checkpoints preserve the working implementation and frozen regressions, not a completion claim.
Style and formatting remain deferred until the end of the audit.

The latest scalar-value batch passed five focused tests: ordinary scalar copy, rejection of copied borrowed authority,
both synchronous ownership compositions, and returned-temporary borrowing. Source and imported scoped-exit tests still
fail to transfer explicitly republished authority to the continuation. Their ordinary entry postconditions now pass,
and the imported provider exports successfully. Temporary trace instrumentation has been removed.

The latest operation repair unwraps scalar contents for ordinary operators. Predicate arguments retain storage identity.
The trace established the prior mismatch as a scalar copy wrapper compared with its own unwrapped input data.
The broader checker result of 440 passes predates this batch and must not be attributed to the latest implementation.

Async verification is required. The synchronous composition covers ownership transport only. The existing native
`async-root-cleanup` fixture is renamed to `async-ownership-composition`, including its smoke registration. Its existing
Main-thread, direct-await, cancellation, lexical cleanup and static cleanup assertions are preserved. Extension with
witnessed ownership across suspension and task-result disposal is pending. No native composition has executed successfully.

Remaining work includes the scoped continuation, reached custom-index materialization through a newly produced borrow,
the previously nonterminating generator check, and native execution through standard-library preparation. Keep the
regression expectations fixed. Complete the maintainability audit and mandatory style/format commands only after the
semantic audit and repairs are finished.

## Produced trust-boundary result checkpoint

The native attempt no longer hits the returned-borrow lowering panic. It stops during standard-library native planning
with checking diagnostics, including 578 moved-storage reports. These are diagnostic counts across the standard library,
not the earlier runtime source-body population, and cannot be compared as a failure-rate regression.

A reduced real-source trust-boundary test reproduced a moved temporary. The planner consumed the operand output and
then exposed that retired slot as the boundary result. Materialized operands now use the existing transferred-operand
planner, publishing a distinct live output before retiring the input. Place-valued operands retain their source access.
No recovery, weakened assertion, or expanded proof budget was added.

The focused repair batch passed all eight selected tests: returned borrowing, scalar and borrowed trust-boundary results,
the selected async composition bodies, ordinary scalar copying, borrowed-authority copy rejection, and all three ownership
composition controls including witnessed results through trust boundaries. Broader checking and native execution remain
pending. Temporary tracing is absent. The expanded async fixture also requires cancellation through async scoped exit
and explicitly rejects a failed payload producer. Its lifecycle members still need direct source-level coverage in the
audit test in addition to the required native run. The previous native driver predates those last fixture additions.

## Index publication and projected-loan boundaries

At checkpoint `777ffb930`, all 440 checker tests pass. The trusted-contract population reports 136 passed, 94 failed,
97 ignored. Comparing failing identities with the prior 123/103/97 result removes nine failures and introduces none.
Four newly added tests account for the rest of the pass-count increase. Native standard-library planning still fails,
but moved-storage diagnostics fall from 578 to 11 on the same workload. Native composition remains unexecuted.

The async source regression now discovers every callable body in its fixture source, including lifecycle members,
rather than enumerating selected function names. Every such body passed checking and lowering before the index repair.

The index-copy baseline panics because Attempt reads through CustomIndexBorrow before that result exists. The pending
repair selects these accesses at Completion, publishes physical result references before checking reached accesses,
and performs their witness checks after completion. Ten focused controls pass, including ordinary indexed copying,
existing imported/index lowering controls, and async ownership composition. The initial negative used only a borrowed
predicate requirement and did not construct an owning witness. Its rejection expectation is retained, but the setup is
corrected to acquire a witness through trusted ensures, with direct field copying as a control for indexed copying.
The first setup also omitted @copy, which was corrected before the publication repair. Do not use either setup error
as evidence about compiler safety. The acquired-witness control is currently running.

A separate frozen source regression, returned_borrow_survives_field_and_index_projection, reports premature moves for
field_view(value).value and array_view(values)[0].value. Projection planning consumes the receiver loan at an intermediate
place-valued expression, before a later consumer captures the reached value. A repair is not yet applied. Preserve loan
descriptors through projection and retire them at the actual consumer, without retaining temporary owned resources or
weakening borrow checks. Do not replace runtime source validation with source-only composition evidence.
