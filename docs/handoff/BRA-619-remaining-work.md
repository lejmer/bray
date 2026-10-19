# BRA-619 completion acceptance

## Resume contract

Continue the authorized stack through BRA-619, verification, maintainability audit, independent PR review and pushed fixes.
Do not stop at status answers, restarts, checkpoints or PR boundaries. No merge or manual Done is authorized. Delete
this temporary record when its obligations are accepted. No handoff survives the stack. After compaction actually reread
AGENTS.md, coding-conventions.md, crates.md, BRA-505-retained-incidents.md and this file.

No language feature, syntax, weaker authority, Rust runtime frame, TLS report owner, deferred destruction queue,
generalized suspended synchronous body, post-failure allocation or downstream semantic validation pass. Runtime ownership
stays in Bray. Work top down, reuse established owning results and remove superseded mechanisms. Correctness, compile
time, generated execution speed, binary size and maintainability are independent acceptance criteria. Batch tests, then
format. Use only release xtask. Preserve the user's C formatting. BRA-619 has changed no C files.

## Workspace and publication

Only edit C:/Users/anton/.codex/worktrees/bra-553-host-bootstrap/bray.
Branch feature/bra-619-scoped-cleanup-acceptance, based on reviewed BRA-626 d69592ca0. Published checkpoint f12e6e62b, following c329313fd.
Draft PR #614: https://github.com/lejmer/bray/pull/614. BRA-619 remains In Progress. No independent review has started.
BRA-561 PR #611, BRA-505 PR #612 and BRA-626 PR #613 are ready. Preserve their branches and older BRA-619 recovery
6bbcb4a3. Do not replay them or restart completed reviews. Destination verified as git@github.com:lejmer/bray.git,
PUBLIC lejmer/bray, authenticated lejmer. Reuse this evidence unless destination or authentication changes.

The tracked Linear plan and coverage inventory retain the sequence. Semantic proof:
https://github.com/lejmer/bray/blob/d69592ca0/docs/handoff/runtime-stack-semantic-proof.md.
Auto-review rejected copying the full proof into a new Linear document. No document was created. Do not retry.

## Established fixes and evidence

- Scoped cleanup preserves selected invocation, pending transfer, execution requirements, admission and frame retention.
  Exit admission precedes entry, with separate admitted and entered guards. Failed entry discharges only admission.
- Repeated semantic occurrences retire a dependency only when every reachable occurrence retires it. Actual destructor
  and finalizer hooks alone require whole-value cleanup. Inactive scoped protocols can partially move represented fields.
- Source structural cleanup now uses Lowerer::push_operation before shared outcome.check. The deleted forwarding resolve
  bypassed initialization retirement and caused duplicate BodyLocal destruction. Synthetic cleanup uses its existing sink.
- Storage analysis publishes actual Result(Completion) joins. The removed Bound(ScopedUse) interpretation marked a
  reachable continuation Unreachable and caused normal execution to cancel.
- Native large aggregates use private addressable snapshots while preserving native ABI and physical layout. Typed
  transfers cover construction, mutation, projection, joins, patterns, arrays, nullable values, imports and ABI calls.
  Ordinary moved bytes are not zeroed. Native PanicReport empty-slot ownership remains explicit.
- Fixtures enable allocation denial inside the entered body after local construction. Admission may allocate before
  establishing cleanup. All six pending transfers retain their no-allocation checks. propagate_error returns its with
  result because try targets that nearest compatible result boundary. No assertion was weakened.

Published c329 passes 38 lowering, 27 scoped, 316 source-query and 171 product-codegen tests, plus earlier unchanged
93 LLVM tests. Rustfmt, mutating release style and Bray format passed. Logs use bra-619-retirement-* and completion-*.

## Current connected report repair

Checkpoint f12e6e62b extends the existing owned outcome, rather than adding a second owner or runtime protocol:

- MIR Return carries an optional owned normal report. Protected frames carry it in typed Completed terminal state.
  CheckCallOutcome has an optional completed-report destination. Walkers, remapping, validation, demand, inspection,
  executable serialization and LLVM consume all operands. Scalar inlining excludes report returns. Receiver-tail append
  forwards normal reports through its existing MirEdge. Shared report joins preserve panic over cancellation over incidents.
- The existing native RunOutcome and FrameProgress already carry reports. Every synchronous Bray invocation already has
  an incoming outcome context. No ABI field or version change is needed. Native direct-await delivery already retains
  normal progress reports. Source lowering still emits report None, so the native loss is not fixed yet.
- Callable dependency summaries distinguish no report from an independent report, separately for normal and abnormal
  outcomes. Substitution, contextual Self, witness resolution, default-input expansion, fixed-point merge, portable
  encoding/decoding, selective record inventory/remapping, validation and specialization hashing include those fields.
- Checked call contracts have immediate and deferred report outputs identified by CallReportOutput. The oversized
  dependency_table module is split into call contracts and the table, with a thin root. Scoped exit result contracts
  now retain their dependencies even without a source expression for the result. The old input mapping incorrectly
  treated the exit parameter as the original initializer and now uses the capability's actual access.
- Existing CFG blocks record their lexical catch collection. Catch entry has a separate block. Edge flow forwards
  collections across normal and nonlocal exits, or transfers caught reports into the ordinary catch result. Scoped call
  completion records its exact StorageExitPoint. The normal-with-result classification is shared in bray-bound-tree.
- Pending report dependencies use the existing storage fixed point, borrow retirement, escape diagnostics and suspension
  retention. Report values snapshot portable dependencies at call entry and commit on the actual outcome. Static value
  retention seeds ordinary with errors and caught reports through the existing ownership closure. StorageFlow publishes
  which source/catch collections can retain reports, for lowering to consume directly.
- CFG call phases also delimit compiler-owned evaluation and with results. Report lookup classifies actual selected
  source invocations and asserts their checked contracts, rather than requiring a callable contract for every phase.

Current batch compilation succeeds. Batched tests pass: 317 source-query, 331 checker, 45 MIR, 38 lowering,
192 package-interface and 205 symbols. The new retained_scoped_exit_errors_keep_borrowed_origins_alive regression rejects
an error whose destructor reads a dying owned resource and accepts the same error borrowing caller-owned storage.
Logs: bra-619-report-owner-check.log, report-owner-test-build.log, report-owner-query.log and per-test-binary logs.
The previous scoped batch passes all 27 tests. Rustfmt and mutating release style, including Bray formatting, pass. Three style limits were resolved by moving mutable authority preparation into StorageFlowInput, sharing inspection target rendering and isolating return validation. The resulting compiler check passes. No native build has tested this batch.

## Work still required

1. Finish the same ownership boundary for default-provider evaluation reports, opaque/trait/generic abstraction,
   recursive summaries, direct await and finalizer report/error capture. Do not infer absence from total, pure, ownership
   or an unresolved contract. Preserve entry snapshots and do not query only on failure edges with lost typed errors.
   The current symbolic report-instantiation fallback handles top-level terms only and needs recursive handling.
2. Complete targeted source/imported acceptance for ordinary with errors, retained incidents, nested catches, repeated
   scopes, moves, mutation, cancellation and destruction. Validate report-presence inference as well as lifetimes.
3. Implement the source accumulator using the published StorageFlow collection demand and existing CleanupOutcome.
   IMPORTANT: CleanupOutcome::new/begin ENTERS A CLEANUP SHIELD. A pending source collection must use allocate plus
   initialization of Normal and EmptyPanicReport without entering a shield. Keep ordinary source cancellation intact.
   Catch success forwards pending incidents, caught panic owns them in its Error value, nonlocal exits forward the
   collections they leave, and normal Return/Completed publishes the report. Checked calls and direct await capture
   completed reports before another native call resets the context. Local cleanup dispatch forwards Incidents instead
   of dropping them. Reuse PanicReportSuppression, with no allocation and no immediate incident disposal.
4. Preserve pending collection state/report storage through frames and cancellation cleanup. Allocate before relevant
   frame states. Include these physical initialized slots in existing MirFrameRetention. Inactive frame cancellation
   must not read source-body storage whose initialization never ran. Normal source pending state is not an active shield.
5. Finish native acceptance, cost checks, mechanical audit and formatting. Checkpoint/push, independently review PR #614,
   resolve findings, push fixes and mark ready/In Review only when complete. Remove stale handoffs before final completion.

## Native observation and cost

Fresh c329 product vL9bks reaches final assertions. Entries 23 and exits 17 pass. Errors and captures are 12 instead of20.
The eight missing errors match six return/break/continue cases, nullable propagation and result propagation.
CleanupOutcome::dispatch lets Incidents continue normally without transferring its report owner. The source accumulator
above must fix this ownership loss and its lifetimes together. No new native build has tested the current report batch.
All previous bootstrap, foreign-entry, async-values and ordinary-finalization smokes pass both profiles.
Required scoped totals: entries23, exits17, errors20, captures20, finalizers9, correct exit-before-local ordering,
no inactive body entry, no allocation after admission and exactly two final incident stderr lines. Do not weaken checks.

Retained product target/native/bra-505/bray-runtime-artifact-smoke-vL9bks/scoped-cleanup-development/scoped-cleanup.exe.
Logs: bra-619-pending-native-smoke.log, pending-native-panic.log, pending-native-counts.log. Counter trace labels used old
PDB offsets. Current .data RVA0x5e2000, entries+2712, exits+2720, errors+2704, captures+2672, finalizers+2688.
The earlier nL6Inc duplicate destruction, L5EJSv unreachable join, EcWNsS pre-admission denial and Xh3zAJ discarded try
result have established causes and fixes above. Do not re-debug them.

Old native object generation exceeded36GB. LLVM SROA expanded one resume from62,494 to1,151,009 instructions.
The aggregate storage repair emits LLVM plus objects in35.883s with four workers, sampled near1.1GB under8GB,
134.78MiB combined artifacts. No optimizer pass was disabled. Latest retained matching-package IR is
bra-619-retirement-ir-product,132.88MiB,9,943MIR blocks,7,099operations before the completion publication repair.
Reuse this cost evidence unless changed inputs justify a new measurement.

## Execution

Shared CARGO_TARGET_DIR=C:/Users/anton/source/repos/bray/target and CARGO_BUILD_BUILD_DIR=the same /target/cargo.
BRAY_LLVM_PREFIX and LLVM_SYS_221_PREFIX use /target/toolchains/llvm/active. Compiler tools use default Rust1.99.
Native packaging uses process-local Rust1.98.1-x86_64-pc-windows-msvc, TMP/TEMP=/target/native/bra-505.
Native LLVM, archiver, linker and executable workflows run outside the sandbox. Worktree writes and Git need escalation.
Never clean shared storage. Use rg --files before unknown paths. Never wildcard rg paths in PowerShell. Python always
uses explicit UTF-8 read_text/write_text. Read short relevant slices. Parse selected facts from large single-line profiles
and LLVM files. Task-owned debug helpers terminate their children and close handles. No debuggee remains.

## Active edits after f12e6e62b

All edits below are being saved as the next recovery checkpoint after f12e6e62b. No new review agent or native build has run.
Continue through BRA-619. The next action is to finish the current batch and resolve its actual findings, not restart the
published work or repeat unchanged broad suites. No C or Bray source has changed in this batch.

The source accumulator is now connected across normal Return/Completed, scoped cleanup, catch success, nonlocal exits,
panic and cancellation. It is allocated from StorageFlow demand and initialized without a shield. Body collections are
excluded from inactive/stored-result callbacks using the existing completion_cleanup fact. Physical state/report slots
are included in frame retention. Failure adoption keeps the direct old path when no pending collection exists.

Call completion receives normal reports, including a normal-only MIR TakeCallReport for a call proved unable to unwind.
The existing checked-call inventory, shared native report extraction and incoming outcome context are reused. Source
admission/memory outcomes do not accidentally receive a source invocation's reports. Scoped entry uses its exact
ScopedEnter occurrence. CleanupOutcome is split into a thin root plus model, state, reports, calls and finalization.

Catch/normal report merging must preserve MIR's block-local SSA contract. The current patch threads ordinary values
through the same report-state branches with MirEdge arguments and the shared continuation::forward_cleanup_edge.
It adds no runtime value storage. That patch is newer than the first failing test batch and is now being compiled.

Storage analysis now records reporting_completions alongside report_collections. It handles proved-normal Bound calls,
actual consumed Future values, selected lifecycle callbacks and default-provider evaluation reports. Deferred snapshots
come from the consumed Future, never a mutable factory-site snapshot. Liveness adds report ownership to its existing
value-retention closure. Cleanup reporting accesses are projected once from the shared CleanupPlan, indexed by scope,
and reused rather than recomputed at every exit. ValueDependencies retain the value's dependencies, not its former slot.

Generated lifecycle calls receive normal reports. Standalone StaticFinalize helpers initialize their own unshielded
pending collection and publish it on normal return. Source destructor body/tail handoff publishes pending reports too.
Selected callable report presence is read through SyntheticLoweringContext from existing checked summaries.

The first test batch exposed an overbroad symbolic fallback. It retained all inputs of unresolved generic report
producers and rejected existing report-free implementations. The patch removes that widening. DependencyRequirement's
existing ResultCall now selects DependencyCallOutput::Value, CompletionReport or AbnormalReport. Substitution,
contextual Self, interface encoding/decoding/interning, selective remapping, matching, structural identity and hashing
preserve the selector. Witness resolution uses the selected output in its existing fixed point and memoization key.
Generic parameter relevance includes all outputs. No new solver, syntax or language feature is added.

The native regression exposed a completed normal report being supplied to a helper that interpreted every completed
payload as an integer result handle. native_run_outcome_value now takes separate typed optional integer payload and
owned report fields for every state. All seven existing producers use the same ABI constructor. No ABI/version change.

## Current verification and unresolved boundaries

Production check passed after the output-selector change, before the latest MIR argument-threading patch.
The current no-run test build passes, log target/bra-619-report-equations-test-build.log, completed session29788.
All build sessions42306,73442,81408,80030,55132 and29788 finished. All test sessions39922,45111,37793 and67835 finished.
The emitted binaries include all current source. No formatter has run after this batch yet.

First connected test batch before the latest fixes:
- MIR45, interface192 and symbols205 passed.
- Checker330/331. The index-only test unnecessarily requested a fabricated callable signature after the earlier
  constructor refactor. It now calls its actual prepare_locations owner directly and removes unrelated setup.
- Lowering37/38. No-report abnormal cleanup had gained unnecessary report-merge branches. The direct initialization
  path is restored when no pending report collection exists.
- Query310/318. The new normal_reports_follow_calls_and_consumed_futures test passed for synchronous and asynchronous
  producers, publishing nonempty report dependencies and report operands in MIR. It now also asserts MIR validity.
  Eight failures covered the too-broad generic/opaque retention described below.
- Scoped24/28. Failures were one invalid MIR edge, the completed-report ABI field above, and two contract fixtures
  rejected by the too-broad fallback. The MIR value threading, output selector and ABI fixes await the current batch.

Logs use target/bra-619-source-reports-{query,scoped,checker,mir,lowering,interface,symbols}.log.
Do not treat those old results as validation of the newer patch.

Latest corrected batch, logs target/bra-619-report-equations-{query,scoped,checker,mir,lowering,interface,symbols}.log:
- Checker331, MIR45, lowering38, interface192 and symbols205 all pass.
- Query312/318. The source normal-report test passes both synchronous and asynchronous calls, including MIR validity.
  Remaining failures are the two raw_pointer_storage cases, reborrowing::implicit_call_reborrows_preserve_local_borrow_values,
  returned_values::returned_values_preserve_caller_storage_and_owned_results, and two timed recursive cases:
  recursive_dependencies::abstract_dependencies_do_not_expand_unneeded_recursive_arguments and
  trait_result_dependencies::abstract_trait_results_converge_through_recursive_generic_composition.
- Scoped26/28. MIR and native artifact emission now pass. Only imported_scoped_calls_enforce_requirements_on_the_entered_capability
  and scoped_guarantees_and_requirements_use_the_actual_capability fail from the opaque report lifetime fallback.
- No native runtime acceptance has run on the current source. No formal PR review has started.

The recursive failures indicate output-specific parameter relevance is needed. ResultResolver is keyed by output, but
ResultParameters and SelectionParameters were changed to traverse all outputs together. That erases the old bounded
normal-result relevance boundary and expands unused recursive arguments. Parameter relevance must follow the same selected
output in its existing graph/cache. Do not add a second solver or hide cancellation/timeouts. Preserve needed parameters
for report outputs while retaining the existing bounded recursion behavior for ordinary values.

Still required before acceptance:
1. Run the current focused batches and fix remaining actual findings. Then finish source/imported, cancellation,
   destruction, repeated scopes and native counts20. No formatter/style pass has run after f12 because tests are not yet
   passing. Use release xtask after tests, never debug xtask.
2. Close callable-value provenance and opaque-call report contracts. The old opaque fallback still assumes all inputs
   may be retained. It rejects returned_values::returned_values_preserve_caller_storage_and_owned_results for a function
   parameter called with &local, and reborrowing::implicit_call_reborrows_preserve_local_borrow_values for an annotated
   function value assigned increment and called twice. Two direct generic trait cases now pass with the output-specific witness
   relation. The recursive cases still need output-specific relevance as described above. Known anonymous bodies must use their actual checked summary too.
3. Read the actual dependency-contract specification before changing that boundary. It says callable values carry their
   producing declaration/lambda's contract, and generic bodies retain open subjects for type AND value parameters until
   actual value contracts are instantiated. Do not silently invent a no-report default for erased callables, ban borrowed
   errors, weaken authority, or rewrite positive tests merely to accept this fallback. Use existing checked inference.
4. Report presence is distinct from dependency emptiness. A selected provider with no report and one with an independent
   report both resolve to empty requirements in the current output equation projection. Do not drop independent reports
   by treating [] as absence. Report-free generic specialization must avoid unnecessary report buffers/joins and inlining
   barriers before LLVM. Derive the smallest representation in the owning summary, reuse its result downstream, and
   assess compile time, runtime speed and binary size independently. Do not rely on LLVM to remove this overhead.
5. Audit constructor/type-form/field defaults, selected operator calls, opaque/trait/generic calls, recursive summaries,
   returned callable/Future values, finalizer report/error capture and total-destructor capacity against the same output
   ownership contract. The source-call/default path alone does not establish those consumers.
6. Complete tests, native acceptance, cost checks, mechanical maintainability/DRY audit, release style/rustfmt/Bray format,
   checkpoint/push, full independent PR614 review and pushed fixes. Only then ready/In Review. No Done or merge. Remove
   stale handoffs before the stack is accepted.

## Efficient continuation

Save multi-file codemods to an ignored task-owned target path, stage edits in memory, and write only after all assertions
pass. After Python commands check LASTEXITCODE before launching Cargo. One failed staged patch previously launched a
check of unchanged files because PowerShell continued after Python's exit. The successful MIR edge patch is retained as
shared target/bra-619-thread-report-results.py. Do not rerun it after it has applied.

Use rg --files before unfamiliar paths. Do not pass wildcard paths to rg in PowerShell. This mistake wasted many calls.
Python read_text/write_text always uses explicit UTF-8. Read short slices and limit aggregate output so tool results do
not truncate necessary evidence. Shared drive C had about41.5GB free before the current build. Never clean shared storage.
